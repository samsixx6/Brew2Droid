//! Port Swerve guardado, contratos reais e restauração atômica dos originais.
//! Usa somente originais locais em BREW_KH_MODULE, BREW_KH_MIF, BREW_SWERVE_MODULE.
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use zeebx::{
    cpu::{BackendPadrao, CpuBackend},
    handset::{launch, types::DisplayGeometry},
    loader::{self, ExtensionImage, modfile::ModImage},
    machine::{AppletResult, Machine, Outcome},
    save_state::{self, Leitor},
    storage::StoragePaths,
};
const MOD: &str = "10f18a0c8e77715459c7eefbb276b8054fe7d8afdb5f69a105aa370c1193b804";
const MIF: &str = "624b4d689c21b25ed9aba934a05ba0a2c907c34d171cf734188f95b7dbbfa4db";
const WORDS: [(u32, u32, u32); 3] = [
    (0x2a028, 0x010164ea, 0x0102bbfc),
    (0x29eb4, 0xe59cc024, 0xe59cc020),
    (0x34c9c, 0xe5922048, 0xe5922044),
];
static NEXT: AtomicU64 = AtomicU64::new(0);
fn source(name: &str) -> PathBuf {
    std::env::var_os(name)
        .expect("informe os originais BREW_KH_MODULE, BREW_KH_MIF e BREW_SWERVE_MODULE")
        .into()
}
fn geometry() -> DisplayGeometry {
    DisplayGeometry {
        width: 240,
        height: 320,
        depth: 16,
    }
}
fn work() -> PathBuf {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("local-tests/wave25-work-20261101/kingdom/proposed-regression-runtime")
        .join(format!(
            "{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    std::fs::create_dir_all(&p).unwrap();
    p
}
fn unstarted(altered_provider: bool) -> Machine<BackendPadrao> {
    let game = source("BREW_KH_MODULE");
    let mut provider = std::fs::read(source("BREW_SWERVE_MODULE")).unwrap();
    if altered_provider {
        provider[0x2000] ^= 1;
    }
    let image = ModImage::parse(std::fs::read(&game).unwrap()).unwrap();
    let ext = ExtensionImage {
        image: ModImage::parse(provider).unwrap(),
        classes: vec![0x0102bbfc],
    };
    let p = work();
    let storage = StoragePaths::from_root(p.join("runtime"));
    storage.create_dirs().unwrap();
    std::fs::create_dir_all(p.join("overlay")).unwrap();
    let mut m = Machine::new_with_storage_geometry(
        BackendPadrao::new().unwrap(),
        loader::load_with(&image, &[ext]).unwrap(),
        game.parent().unwrap(),
        &storage,
        Some(p.join("overlay")),
        geometry(),
    )
    .unwrap();
    m.enable_handset_clock();
    // As comparações estruturais de snapshots exigem tempo real congelado.
    m.pause_handset_clock(true);
    m
}
fn machine() -> Machine<BackendPadrao> {
    let mut m = unstarted(false);
    assert_eq!(m.run(2_000_000).unwrap(), Outcome::Returned { code: 0 });
    m
}
fn words(m: &Machine<BackendPadrao>) -> Vec<u32> {
    WORDS
        .iter()
        .map(|(a, _, _)| m.cpu().read_u32(*a).unwrap())
        .collect()
}
fn expected(fixed: bool) -> Vec<u32> {
    WORDS
        .iter()
        .map(|(_, a, b)| if fixed { *b } else { *a })
        .collect()
}
fn selected(m: &mut Machine<BackendPadrao>) {
    assert!(
        m.set_handset_runtime_identity(MOD, MIF),
        "o par e o provedor auditados precisam selecionar o port"
    );
}
fn rewrite(
    m: &mut Machine<BackendPadrao>,
    mask: u8,
    change: Option<u32>,
    regs: Option<(u32, u32)>,
) -> Vec<u8> {
    let data = m.grava_estado();
    let reader = Leitor::abre(&data).unwrap();
    let base = m
        .module()
        .mem
        .regions()
        .iter()
        .find(|r| r.name == "module")
        .unwrap()
        .base;
    let parts = reader
        .nomes()
        .into_iter()
        .map(|name| {
            let mut b = reader.secao(name).unwrap().to_vec();
            if name == "mem.module" {
                for (i, (a, old, new)) in WORDS.iter().enumerate() {
                    let at = (a - base) as usize;
                    b[at..at + 4].copy_from_slice(
                        &(if mask & (1 << i) != 0 { *new } else { *old }).to_le_bytes(),
                    );
                }
                if let Some(at) = change {
                    b[(at - base) as usize] ^= 1;
                }
            }
            if name == "cpu.regs" {
                if let Some((pc, lr)) = regs {
                    b[4 + 14 * 4..4 + 15 * 4].copy_from_slice(&lr.to_le_bytes());
                    b[4 + 15 * 4..4 + 16 * 4].copy_from_slice(&pc.to_le_bytes());
                }
            }
            (name.to_owned(), b)
        })
        .collect::<Vec<_>>();
    save_state::escreve(&parts)
}
#[test]
#[ignore = "requer os originais locais"]
fn exact_pair_applies_all_three_words_before_reset() {
    let mut m = unstarted(false);
    selected(&mut m);
    assert_eq!(m.run(2_000_000).unwrap(), Outcome::Returned { code: 0 });
    assert_eq!(words(&m), expected(true));
}
#[test]
#[ignore = "requer os originais locais"]
fn live_patch_is_idempotent_and_keeps_real_factory_and_arm_renderer() {
    let mut m = machine();
    selected(&mut m);
    assert_eq!(words(&m), expected(true));
    selected(&mut m);
    assert_eq!(words(&m), expected(true));
    // O desenho real depende dos timers; só os snapshots estruturais ficam pausados.
    m.pause_handset_clock(false);
    let AppletResult::Called { code: 0, applet } = m.create_applet(0x01026191, 5_000_000).unwrap()
    else {
        panic!("factory original recusou")
    };
    assert!(matches!(
        m.start_applet(applet, 0x01026191, 20_000_000).unwrap(),
        Outcome::Returned { .. }
    ));
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline && m.screen().escritas() == 0 {
        m.comeca_volta();
        let result = m.advance(1_000_000).unwrap();
        m.fecha_volta();
        assert!(
            result
                .iter()
                .all(|o| matches!(o, Outcome::Returned { .. } | Outcome::Budget)),
            "{result:?}"
        );
    }
    assert!(m.screen().escritas() > 0);
    assert!(m.swallowed_faults().is_empty());
    assert!(m.missing_apis().is_empty());
}
#[test]
#[ignore = "requer os originais locais"]
fn wrong_module_or_manifest_hash_never_changes_any_word() {
    for (a, b) in [("wrong", MIF), (MOD, "wrong")] {
        let mut m = machine();
        assert!(!m.set_handset_runtime_identity(a, b));
        assert_eq!(words(&m), expected(false));
    }
}
#[test]
#[ignore = "requer os originais locais"]
fn altered_provider_never_activates_the_port() {
    let mut m = unstarted(true);
    assert!(!m.set_handset_runtime_identity(MOD, MIF));
    assert_eq!(m.run(2_000_000).unwrap(), Outcome::Returned { code: 0 });
    assert_eq!(words(&m), expected(false));
}
#[test]
#[ignore = "requer os originais locais"]
fn absent_provider_never_activates_the_port() {
    let game = source("BREW_KH_MODULE");
    let image = ModImage::parse(std::fs::read(&game).unwrap()).unwrap();
    let p = work();
    let storage = StoragePaths::from_root(p.join("runtime"));
    storage.create_dirs().unwrap();
    let mut m = Machine::new_with_storage_geometry(
        BackendPadrao::new().unwrap(),
        loader::load_with(&image, &[]).unwrap(),
        game.parent().unwrap(),
        &storage,
        None,
        geometry(),
    )
    .unwrap();
    m.enable_handset_clock();
    // As comparações estruturais de snapshots exigem tempo real congelado.
    m.pause_handset_clock(true);
    assert!(!m.set_handset_runtime_identity(MOD, MIF));
}
#[test]
#[ignore = "requer os originais locais"]
fn changed_context_rejects_before_touching_live_or_template_words() {
    for at in [
        0x29d58, 0x29d78, 0x2a020, 0x2a034, 0x29e80, 0x29ec4, 0x34c90, 0x34ca0,
    ] {
        let mut m = machine();
        let old = m.cpu().read_u32(at).unwrap();
        m.cpu_mut().write_u32(at, old ^ 1).unwrap();
        assert!(!m.set_handset_runtime_identity(MOD, MIF), "{at:x}");
        assert_eq!(words(&m), expected(false));
    }
}
#[test]
#[ignore = "requer os originais locais"]
fn every_mixed_patch_state_is_rejected_without_partial_changes() {
    for mask in 1u8..7 {
        let mut m = machine();
        for (i, (a, _, new)) in WORDS.iter().enumerate() {
            if mask & (1 << i) != 0 {
                m.cpu_mut().write_u32(*a, *new).unwrap();
            }
        }
        let before = words(&m);
        assert!(!m.set_handset_runtime_identity(MOD, MIF), "mask {mask}");
        assert_eq!(words(&m), before);
    }
}
#[test]
#[ignore = "requer os originais locais"]
fn original_safe_save_is_upgraded_and_adapted_save_roundtrips() {
    let mut m = machine();
    selected(&mut m);
    let old = rewrite(&mut m, 0, None, None);
    m.restaura_estado(&old).unwrap();
    assert_eq!(words(&m), expected(true));
    let fixed = m.grava_estado();
    m.restaura_estado(&fixed).unwrap();
    assert_eq!(words(&m), expected(true));
}
#[test]
#[ignore = "requer os originais locais"]
fn changed_or_mixed_save_is_refused_atomically() {
    let mut m = machine();
    selected(&mut m);
    for mask in 1u8..7 {
        let bad = rewrite(&mut m, mask, None, None);
        let before = m.grava_estado();
        assert!(m.restaura_estado(&bad).is_err(), "mask {mask}");
        assert_eq!(m.grava_estado(), before);
    }
    let bad = rewrite(&mut m, 7, Some(0x34c90), None);
    let before = m.grava_estado();
    assert!(m.restaura_estado(&bad).is_err());
    assert_eq!(m.grava_estado(), before);
}
#[test]
#[ignore = "requer os originais locais"]
fn original_inflight_pc_or_return_into_old_contract_is_refused_atomically() {
    let mut m = machine();
    selected(&mut m);
    for addr in [0x29d6c, 0x29eb4, 0x29ebc, 0x34c9c, 0x34ca4, 0x08000194] {
        for pc in [false, true] {
            let bad = rewrite(
                &mut m,
                0,
                None,
                Some(if pc { (addr, 0) } else { (0, addr) }),
            );
            let before = m.grava_estado();
            assert!(m.restaura_estado(&bad).is_err(), "{addr:x} pc={pc}");
            assert_eq!(m.grava_estado(), before);
        }
    }
}
#[test]
#[ignore = "requer os originais locais"]
fn adapted_save_is_refused_when_current_provider_is_foreign() {
    let mut m = machine();
    selected(&mut m);
    let state = m.grava_estado();
    let mut other = unstarted(true);
    assert!(!other.set_handset_runtime_identity(MOD, MIF));
    assert_eq!(other.run(2_000_000).unwrap(), Outcome::Returned { code: 0 });
    let before = other.grava_estado();
    assert!(other.restaura_estado(&state).is_err());
    assert_eq!(other.grava_estado(), before);
}
#[test]
#[ignore = "requer os originais locais"]
fn exact_opaque_manifest_pair_exposes_factory_class_without_parsing_or_replacement() {
    let p = launch::read(&source("BREW_KH_MODULE")).expect("par opaco auditado");
    assert_eq!(p.applet_class, 0x01026191);
    let f = p.manifest.unwrap();
    assert!(!f.parsed);
    assert_eq!(f.bytes, std::fs::read(source("BREW_KH_MIF")).unwrap());
}
#[test]
#[ignore = "requer os originais locais"]
fn ordinary_dependency_resolution_finds_only_the_actual_provider() {
    let ext = zeebx::session::extensoes_de(&source("BREW_KH_MODULE"));
    assert!(
        ext.iter().any(|e| e.classes.contains(&0x0102bbfc)),
        "dependencia do port precisa usar o resolvedor normal"
    );
    assert!(
        ext.iter().all(|e| !e.classes.contains(&0x010164ea)),
        "o CID antigo nao e uma exportacao real do provedor"
    );
}

fn rewrite_provider_code(m: &mut Machine<BackendPadrao>, offset: u32) -> Vec<u8> {
    let entry = m.module().extensions[0].entry;
    let region = m
        .module()
        .mem
        .regions()
        .iter()
        .find(|r| r.base <= entry && u64::from(r.base) + r.bytes.len() as u64 > u64::from(entry))
        .unwrap();
    let section = format!("mem.{}", region.name);
    let at = (entry + offset - region.base) as usize;
    let saved = m.grava_estado();
    let reader = Leitor::abre(&saved).unwrap();
    let parts = reader
        .nomes()
        .into_iter()
        .map(|name| {
            let mut bytes = reader.secao(name).unwrap().to_vec();
            if name == section {
                bytes[at] ^= 1;
            }
            (name.to_owned(), bytes)
        })
        .collect::<Vec<_>>();
    save_state::escreve(&parts)
}
#[test]
#[ignore = "requer os originais locais"]
fn changed_saved_provider_factory_or_renderer_code_is_refused_atomically() {
    let mut m = machine();
    selected(&mut m);
    for offset in [0x2894, 0x2479c, 0x2ff38, 0x2fcf8, 0x2feb8] {
        let bad = rewrite_provider_code(&mut m, offset);
        let before = m.grava_estado();
        assert!(m.restaura_estado(&bad).is_err(), "provider +{offset:x}");
        assert_eq!(m.grava_estado(), before);
    }
}
#[test]
#[ignore = "requer os originais locais"]
fn changed_live_provider_code_rejects_before_any_consumer_word_changes() {
    for offset in [0x2894, 0x2479c, 0x2ff38, 0x2fcf8, 0x2feb8] {
        let mut m = machine();
        let at = m.module().extensions[0].entry + offset;
        let prior = m.cpu().read_u32(at).unwrap();
        m.cpu_mut().write_u32(at, prior ^ 1).unwrap();
        assert!(!m.set_handset_runtime_identity(MOD, MIF));
        assert_eq!(words(&m), expected(false));
        assert_eq!(m.cpu().read_u32(at).unwrap(), prior ^ 1);
    }
}
#[test]
#[ignore = "requer os originais locais"]
fn genuine_provider_initialization_and_mutable_bss_roundtrip_without_full_live_hash() {
    use zeebx::brew::aee::{self, Interface};
    let mut m = machine();
    selected(&mut m);
    assert!(matches!(
        m.create_applet(0x01026191, 5_000_000).unwrap(),
        AppletResult::Called { code: 0, .. }
    ));
    let out = loader::STACK_BASE + 0x1000;
    let shell = m.module().shell;
    assert_eq!(
        m.call_guest(
            aee::encode(Interface::Shell, 2),
            [shell, 0x0102bbfc, out, 0],
            20_000_000
        )
        .unwrap(),
        Outcome::Returned { code: 0 }
    );
    assert_ne!(m.cpu().read_u32(out).unwrap(), 0);
    // A reserva BSS é gravável por contrato do loader e fica depois da imagem original.
    // Nenhum fingerprint de código inclui este word; a imagem imutável do loader continua zero.
    let data = m.module().extensions[0].entry + 273188 + 16;
    assert_eq!(m.module().mem.read_u32(data).unwrap(), 0);
    m.cpu_mut().write_u32(data, 0x12345678).unwrap();
    selected(&mut m); // Reaplicação após inicialização/data alterada continua válida.
    let saved = m.grava_estado();
    m.cpu_mut().write_u32(data, 0x87654321).unwrap();
    m.restaura_estado(&saved).unwrap();
    assert_eq!(m.cpu().read_u32(data).unwrap(), 0x12345678);
    assert_eq!(words(&m), expected(true));
    assert_eq!(m.module().mem.read_u32(data).unwrap(), 0);
}
#[test]
#[ignore = "requer os originais locais"]
fn changed_or_absent_matching_opaque_metadata_never_borrows_the_pair_exception() {
    let root = work();
    let module = root.join("renamed.mod");
    let mif = root.join("renamed.mif");
    let raw_mod = std::fs::read(source("BREW_KH_MODULE")).unwrap();
    let raw_mif = std::fs::read(source("BREW_KH_MIF")).unwrap();
    std::fs::write(&module, &raw_mod).unwrap();
    assert!(launch::read(&module).is_err(), "KH não é exceção sem MIF");
    std::fs::write(&mif, &raw_mif).unwrap();
    assert_eq!(launch::read(&module).unwrap().applet_class, 0x01026191);
    let mut changed = raw_mif.clone();
    changed[100] ^= 1;
    std::fs::write(&mif, &changed).unwrap();
    assert!(launch::read(&module).is_err());
    std::fs::write(&mif, &raw_mif).unwrap();
    let mut changed = raw_mod.clone();
    changed[100] ^= 1;
    std::fs::write(&module, &changed).unwrap();
    assert!(launch::read(&module).is_err());
    // Somente cópias privadas foram escritas.
    assert_eq!(std::fs::read(source("BREW_KH_MODULE")).unwrap(), raw_mod);
    assert_eq!(std::fs::read(source("BREW_KH_MIF")).unwrap(), raw_mif);
}

#[test]
#[ignore = "requer os originais locais"]
fn later_audited_provider_cannot_override_foreign_first_class_owner() {
    let game = source("BREW_KH_MODULE");
    let raw = std::fs::read(source("BREW_SWERVE_MODULE")).unwrap();
    let mut changed = raw.clone();
    changed[0x2000] ^= 1;
    let image = ModImage::parse(std::fs::read(&game).unwrap()).unwrap();
    let extensions = [
        ExtensionImage {
            image: ModImage::parse(changed).unwrap(),
            classes: vec![0x0102bbfc],
        },
        ExtensionImage {
            image: ModImage::parse(raw).unwrap(),
            classes: vec![0x0102bbfc],
        },
    ];
    let p = work();
    let storage = StoragePaths::from_root(p.join("runtime"));
    storage.create_dirs().unwrap();
    let mut m = Machine::new_with_storage_geometry(
        BackendPadrao::new().unwrap(),
        loader::load_with(&image, &extensions).unwrap(),
        game.parent().unwrap(),
        &storage,
        Some(p.join("overlay")),
        geometry(),
    )
    .unwrap();
    assert!(!m.set_handset_runtime_identity(MOD, MIF));
    assert_eq!(m.run(2_000_000).unwrap(), Outcome::Returned { code: 0 });
    assert_eq!(words(&m), expected(false));
}

