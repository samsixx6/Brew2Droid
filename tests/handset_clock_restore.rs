//! O relógio handset pertence à linha do tempo do guest, não à sessão recém-aberta.
//! APIs públicas, snapshots seccionados e callbacks ARM reais.
use std::{path::PathBuf, sync::atomic::{AtomicU64, Ordering}, time::Duration};
use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend, Reg},
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    save_state::{self, Erro, Leitor},
    storage::StoragePaths,
};

const HANDSET: &str = "relogio.handset_us";
const MARK: u32 = loader::STACK_BASE + 0x2000;
const CALLBACK: u32 = loader::MODULE_BASE + 0x40;
const SAVED_US: u64 = 63_000_123;

struct Fixture {
    m: Machine<BackendPadrao>,
    root: PathBuf,
}

fn api(iface: Interface, name: &str) -> u32 {
    let slot = (0..256).find(|slot| iface.method(*slot) == Some(name)).unwrap();
    aee::encode(iface, slot)
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests/wave25-work-20261101/clock/runtime-tests")
            .join(format!("clock-restore-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&root).unwrap();
        let mut bytes = vec![0u8; 0x80];
        for (offset, words) in [
            (0, &[0xe3a00000u32, 0xe12fff1e][..]),
            // PFNNOTIFY(pUser): marca a entrega real do timer no endereço recebido.
            (0x40, &[0xe3a01001, 0xe5801000, 0xe3a00000, 0xe12fff1e][..]),
        ] {
            for (i, word) in words.iter().enumerate() {
                bytes[offset + i * 4..offset + i * 4 + 4].copy_from_slice(&word.to_le_bytes());
            }
        }
        let image = ModImage::parse(bytes).unwrap();
        let storage = StoragePaths::from_root(root.join("runtime"));
        storage.create_dirs().unwrap();
        let mut m = Machine::new_with_storage(BackendPadrao::new().unwrap(), loader::load_with(&image, &[]).unwrap(), &root, &storage, None);
        assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
        m.cpu_mut().write_u32(MARK, 0).unwrap();
        Self { m, root }
    }

    fn pause_handset(&mut self) {
        self.m.enable_handset_clock();
        self.m.pause_handset_clock(true);
    }

    fn shell(&mut self, name: &str, args: [u32; 3]) -> u32 {
        match self.m.call_guest(api(Interface::Shell, name), [loader::OBJECT_BASE, args[0], args[1], args[2]], 10000).unwrap() {
            Outcome::Returned { code } => code,
            other => panic!("{name}: {other:?}"),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let path = self.root.canonicalize().unwrap();
        let allowed = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests/wave25-work-20261101/clock/runtime-tests")
            .canonicalize().unwrap();
        assert!(path.starts_with(allowed));
        assert!(path.file_name().unwrap().to_string_lossy().starts_with("clock-restore-"));
        std::fs::remove_dir_all(path).unwrap();
    }
}

fn replace(archive: &[u8], name: &str, bytes: Option<&[u8]>) -> Vec<u8> {
    let reader = Leitor::abre(archive).unwrap();
    let mut sections = reader.nomes().into_iter().filter(|n| *n != name)
        .map(|n| (n.to_owned(), reader.secao(n).unwrap().to_vec())).collect::<Vec<_>>();
    if let Some(bytes) = bytes {
        sections.push((name.into(), bytes.to_vec()));
    }
    save_state::escreve(&sections)
}

#[test]
fn handset_writer_records_the_exact_guest_clock_in_microseconds() {
    let mut f = Fixture::new();
    f.m.cpu_mut().set_instructions(528 * 12345);
    let state = replace(&f.m.grava_estado(), "relogio.us", Some(&71_000_123u64.to_le_bytes()));
    f.m.restaura_estado(&state).unwrap();
    f.pause_handset();
    let expected = f.m.clock_us_for_input();
    assert!(expected >= 71_012_468);
    let saved = f.m.grava_estado();
    let reader = Leitor::abre(&saved).unwrap();
    assert_eq!(reader.secao(HANDSET), Some(&expected.to_le_bytes()[..]), "o estado deve capturar o relógio realmente consultado pelo handset");
    assert_eq!(reader.secao("relogio.us"), Some(&71_000_123u64.to_le_bytes()[..]), "a parcela determinística continua separada");
}

#[test]
fn fresh_paused_handset_restore_recovers_the_exact_saved_clock_and_stays_paused() {
    let source = Fixture::new();
    let state = replace(&source.m.grava_estado(), HANDSET, Some(&SAVED_US.to_le_bytes()));
    let mut reopened = Fixture::new();
    reopened.pause_handset();
    reopened.m.restaura_estado(&state).unwrap();
    assert_eq!(reopened.m.clock_us_for_input(), SAVED_US);
    // Execução da CPU e pausa repetida não consomem tempo real enquanto pausado.
    reopened.m.cpu_mut().set_instructions(528 * 99_000_000);
    reopened.m.pause_handset_clock(true);
    for _ in 0..4 { reopened.m.advance(1000).unwrap(); }
    assert_eq!(reopened.m.clock_us_for_input(), SAVED_US);
}

#[test]
fn reopened_handset_keeps_saved_absolute_timer_deadline_and_delivers_when_due() {
    let mut source = Fixture::new();
    assert_eq!(source.m.clock_ms(), 0);
    assert_eq!(source.shell("SetTimer", [65000, CALLBACK, MARK]), 0);
    let state = source.m.grava_estado();
    assert_eq!(Leitor::abre(&state).unwrap().trios("agenda.timers").unwrap(), vec![(65000, CALLBACK, MARK)]);
    let state = replace(&state, HANDSET, Some(&SAVED_US.to_le_bytes()));
    let mut reopened = Fixture::new();
    reopened.pause_handset();
    reopened.m.restaura_estado(&state).unwrap();
    assert_eq!(reopened.shell("GetTimerExpiration", [CALLBACK, MARK, 0]), 2000, "um relógio novo não pode transformar dois segundos restantes em sessenta e cinco");
    assert_eq!(reopened.m.next_guest_work_delay(), Some(Duration::from_secs(2)));
    let due = replace(&state, HANDSET, Some(&65_000_123u64.to_le_bytes()));
    reopened.m.restaura_estado(&due).unwrap();
    assert_eq!(reopened.m.next_guest_work_delay(), Some(Duration::ZERO));
    assert_eq!(reopened.m.advance(1000).unwrap(), vec![Outcome::Returned { code: 0 }]);
    assert_eq!(reopened.m.cpu().read_u32(MARK).unwrap(), 1);
    assert_eq!(reopened.m.armed_timers(), 0);
    assert_eq!(reopened.m.clock_us_for_input(), 65_000_123);
}

#[test]
fn malformed_optional_handset_clock_is_rejected_before_any_live_state_changes() {
    let older = Fixture::new().m.grava_estado();
    for handset in [false, true] {
        for len in [0, 1, 4, 7, 9, 16] {
            let mut live = Fixture::new();
            if handset { live.pause_handset(); }
            assert_eq!(live.shell("SetTimer", [60000, CALLBACK, MARK]), 0);
            live.m.cpu_mut().write_reg(Reg::R7, 0x12345678);
            live.m.cpu_mut().write_u32(MARK, 0x12345678).unwrap();
            let pixels = 0xf81fu16.to_le_bytes().repeat(live.m.screen().width() as usize * live.m.screen().height() as usize);
            assert!(live.m.pinta_tela_rgb565(&pixels));
            let before = live.m.grava_estado();
            let invalid = replace(&older, HANDSET, Some(&vec![0; len]));
            let result = live.m.restaura_estado(&invalid);
            assert!(matches!(result, Err(Erro::Secao { ref nome, .. }) if nome == HANDSET), "tamanho{len}, handset={handset}: {result:?}");
            assert!(live.m.grava_estado() == before, "recusa precisa preservar CPU, heap, agenda, imagem e relógio: tamanho{len}, handset={handset}");
        }
    }
}

#[test]
fn legacy_state_without_handset_clock_is_accepted_and_keeps_the_current_host_clock() {
    let source = Fixture::new();
    let state = replace(&source.m.grava_estado(), "relogio.us", Some(&SAVED_US.to_le_bytes()));
    let legacy = replace(&state, HANDSET, None);
    let mut reopened = Fixture::new();
    reopened.pause_handset();
    let previous = reopened.m.clock_us_for_input();
    reopened.m.restaura_estado(&legacy).unwrap();
    assert_eq!(reopened.m.clock_us_for_input(), previous, "o formato legado não permite reconstruir o tempo de parede ausente");
}

#[test]
fn deterministic_machine_ignores_a_valid_handset_clock_without_changing_clock_mode() {
    let mut source = Fixture::new();
    let state = replace(&source.m.grava_estado(), "relogio.us", Some(&42_000_123u64.to_le_bytes()));
    source.m.restaura_estado(&state).unwrap();
    source.m.cpu_mut().set_instructions(528 * 777);
    let expected = source.m.clock_us_for_input();
    let state = replace(&source.m.grava_estado(), HANDSET, Some(&SAVED_US.to_le_bytes()));
    let mut deterministic = Fixture::new();
    deterministic.m.restaura_estado(&state).unwrap();
    assert_eq!(deterministic.m.clock_us_for_input(), expected);
    let instructions = deterministic.m.cpu().instructions();
    deterministic.m.cpu_mut().set_instructions(instructions + 528_000);
    assert_eq!(deterministic.m.clock_us_for_input(), expected + 1000);
    let state = deterministic.m.grava_estado();
    assert!(Leitor::abre(&state).unwrap().secao(HANDSET).is_none(), "restauração não liga o relógio handset para consumidores determinísticos");
}

