use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend, Reg};
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

const OUT: u32 = loader::STACK_BASE + 0x1000;
const CX: u32 = OUT + 0x100;
const DATA: u32 = OUT + 0x200;
const CALLBACK: u32 = loader::MODULE_BASE + 0x40;
const ERROR: u32 = u32::MAX;
const ENETDOWN: u32 = 0x216;
const EOPNOTSUPP: u32 = 0x20a;

struct Fixture {
    machine: Machine<BackendPadrao>,
    root: PathBuf,
    net: u32,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "brew-offline-sockets-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let storage = StoragePaths::from_root(root.join("runtime"));
        let image = ModImage::parse(
            [0xe3a00000u32, 0xe12fff1e]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect(),
        )
        .unwrap();
        let mut machine = Machine::new_with_storage(
            BackendPadrao::new().unwrap(),
            loader::load_with(&image, &[]).unwrap(),
            &root,
            &storage,
            None,
        );
        assert_eq!(machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
        let mut f = Self {
            machine,
            root,
            net: 0,
        };
        assert_eq!(
            f.api(
                Interface::Shell,
                "CreateInstance",
                [loader::OBJECT_BASE, 0x0100102e, OUT, 0]
            ),
            0
        );
        f.net = f.read(OUT);
        // PFNCONNECTCB(context,error): conta entregas e registra o segundo argumento.
        f.machine
            .cpu_mut()
            .write_mem(
                CALLBACK,
                &[
                    0xe5902000u32,
                    0xe2822001,
                    0xe5802000,
                    0xe5801004,
                    0xe12fff1e,
                ]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
            )
            .unwrap();
        f.machine.cpu_mut().write_mem(CX, &[0; 48]).unwrap();
        f
    }

    fn invoke(&mut self, function: u32, args: [u32; 4], extra: &[u32]) -> u32 {
        let sp = self.machine.cpu().read_reg(Reg::Sp);
        self.machine.cpu_mut().write_reg(Reg::Sp, sp - 32);
        self.machine
            .cpu_mut()
            .write_mem(
                sp - 32,
                &extra
                    .iter()
                    .copied()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        let outcome = self.machine.call_guest(function, args, 10000).unwrap();
        self.machine.cpu_mut().write_reg(Reg::Sp, sp);
        match outcome {
            Outcome::Returned { code } => code,
            other => panic!("{other:?}"),
        }
    }

    fn api(&mut self, iface: Interface, method: &str, args: [u32; 4]) -> u32 {
        let slot = (0..256).find(|&s| iface.method(s) == Some(method)).unwrap();
        self.invoke(aee::encode(iface, slot), args, &[])
    }

    fn open(&mut self, kind: u32) -> u32 {
        let socket = self.api(Interface::NetLegacy, "OpenSocket", [self.net, kind, 0, 0]);
        assert_ne!(
            socket, 0,
            "OpenSocket constrói ISocket antes de estabelecer a rede"
        );
        socket
    }

    // Slots literais da ABI de AEENet.h; não dependem de uma enum nova no emulador.
    fn socket(&mut self, object: u32, slot: u32, args: [u32; 3], extra: &[u32]) -> u32 {
        let table = self.read(object);
        let function = self.read(table + slot * 4);
        assert_ne!(function, 0, "slot {slot} deve existir na vtable ISocket");
        self.invoke(function, [object, args[0], args[1], args[2]], extra)
    }

    fn read(&self, ptr: u32) -> u32 {
        self.machine.cpu().read_u32(ptr).unwrap()
    }
    fn deliver(&mut self) {
        for _ in 0..8 {
            let outcomes = self.machine.deliver_signals(10000).unwrap();
            assert!(
                outcomes
                    .iter()
                    .all(|o| matches!(o, Outcome::Returned { .. }))
            );
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn fifo_callbacks(f: &mut Fixture) -> (u32, u32, u32) {
    let ordered = CALLBACK + 0x100;
    f.machine.cpu_mut().write_mem(CX, &[0; 128]).unwrap();
    // Salva erro, incrementa contador compartilhado, e grava a identidade em ordem.
    let code = [
        0xe5801008u32,
        0xe5901000,
        0xe5902004,
        0xe5923000,
        0xe2833001,
        0xe5823000,
        0xe0822103,
        0xe5821000,
        0xe12fff1e,
    ];
    f.machine
        .cpu_mut()
        .write_mem(
            ordered,
            &code
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    for (context, id) in [(CX + 32, 1), (CX + 48, 2), (CX + 64, 3)] {
        f.machine.cpu_mut().write_u32(context, id).unwrap();
        f.machine.cpu_mut().write_u32(context + 4, CX).unwrap();
    }
    (ordered, CX + 32, CX + 48)
}

#[test]
fn offline_socket_every_accepted_connect_completes_fifo_including_after_save() {
    let mut f = Fixture::new();
    let tcp = f.open(0);
    let (function, first, second) = fifo_callbacks(&mut f);
    for context in [first, second, CX + 64] {
        assert_eq!(
            f.socket(tcp, 7, [0x04030201, 0x5000, function], &[context]),
            0
        );
    }
    let saved = f.machine.grava_estado();
    f.machine.restaura_estado(&saved).unwrap();
    assert_eq!(f.read(CX), 0);
    f.deliver();
    assert_eq!(
        f.read(CX),
        3,
        "toda solicitação aceita exige sua própria conclusão"
    );
    assert_eq!([f.read(CX + 4), f.read(CX + 8), f.read(CX + 12)], [1, 2, 3]);
    for context in [first, second, CX + 64] {
        assert_eq!(f.read(context + 8), ENETDOWN);
    }
    f.deliver();
    assert_eq!(f.read(CX), 3);
}

#[test]
fn offline_socket_cancel_removes_all_matching_connects_but_preserves_others() {
    let mut f = Fixture::new();
    let tcp = f.open(0);
    let (function, first, second) = fifo_callbacks(&mut f);
    for context in [first, second, first] {
        assert_eq!(
            f.socket(tcp, 7, [0x04030201, 0x5000, function], &[context]),
            0
        );
    }
    f.socket(tcp, 4, [function, first, 0], &[]);
    f.deliver();
    assert_eq!(f.read(CX), 1);
    assert_eq!(f.read(CX + 4), 2);
    assert_eq!(f.read(first + 8), 0);
    assert_eq!(f.read(second + 8), ENETDOWN);
}

#[test]
fn offline_socket_pending_work_is_not_idle() {
    for slot in [7, 2] {
        let mut f = Fixture::new();
        let tcp = f.open(0);
        if slot == 7 {
            f.socket(tcp, 7, [0x04030201, 0x5000, CALLBACK], &[CX]);
        } else {
            f.socket(tcp, 2, [CALLBACK, CX, 0], &[]);
        }
        assert!(
            !f.machine.is_idle(),
            "callback de socket é trabalho do guest"
        );
    }
}

#[test]
fn offline_socket_pending_work_has_zero_scheduler_delay() {
    for slot in [7, 2] {
        let mut f = Fixture::new();
        let tcp = f.open(0);
        if slot == 7 {
            f.socket(tcp, 7, [0x04030201, 0x5000, CALLBACK], &[CX]);
        } else {
            f.socket(tcp, 2, [CALLBACK, CX, 0], &[]);
        }
        assert_eq!(
            f.machine.next_guest_work_delay(),
            Some(std::time::Duration::ZERO)
        );
    }
}

#[test]
fn offline_socket_pending_work_prevents_idle_clock_jump() {
    for slot in [7, 2] {
        let mut f = Fixture::new();
        let tcp = f.open(0);
        if slot == 7 {
            f.socket(tcp, 7, [0x04030201, 0x5000, CALLBACK], &[CX]);
        } else {
            f.socket(tcp, 2, [CALLBACK, CX, 0], &[]);
        }
        assert_eq!(
            f.api(
                Interface::Shell,
                "SetTimer",
                [loader::OBJECT_BASE, 1000, CALLBACK, CX + 16]
            ),
            0
        );
        let before = f.machine.clock_ms();
        f.machine.advance(1000).unwrap();
        assert_eq!(
            f.machine.clock_ms(),
            before,
            "não saltar sobre conclusão pronta"
        );
        assert_eq!(
            f.read(CX),
            0,
            "advance não entrega a conclusão pelo caminho errado"
        );
    }
}

#[test]
fn offline_socket_released_pointer_never_reports_positive_transfer_counts() {
    let mut f = Fixture::new();
    let tcp = f.open(0);
    assert_eq!(f.socket(tcp, 1, [0; 3], &[]), 0);
    for slot in [3, 9, 10, 11, 12, 13] {
        assert_eq!(
            f.socket(tcp, slot, [DATA, 16, 0], &[0x04030201, 0x5000]),
            ERROR,
            "slot {slot}: EBADF positivo não é contagem de bytes transferidos"
        );
    }
    assert_eq!(
        f.socket(tcp, 7, [0x04030201, 0x5000, CALLBACK], &[CX]),
        ERROR
    );
    assert_eq!(f.socket(tcp, 8, [0, 0x5000, 0], &[]), ERROR);
    assert_eq!(f.socket(tcp, 0, [0; 3], &[]), 0);
    assert_eq!(f.socket(tcp, 1, [0; 3], &[]), 0);
    assert_eq!(f.socket(tcp, 6, [0; 3], &[]), 0x202);
    assert_eq!(f.socket(tcp, 5, [DATA, DATA + 4, 0], &[]), 0x202);
    f.deliver();
    assert_eq!(f.read(CX), 0);
}

#[test]
fn offline_socket_older_pending_callback_precedes_rearmed_budgeted_socket() {
    let mut f = Fixture::new();
    let first = f.open(1);
    let second = f.open(1);
    let slow_rearm = CALLBACK + 0x40;
    f.machine.cpu_mut().write_u32(CX + 8, first).unwrap();
    f.machine.cpu_mut().write_u32(CX + 12, slow_rearm).unwrap();
    let code = [
        0xe92d4010u32,
        0xe1a04000,
        0xe5941000,
        0xe2811001,
        0xe5841000,
        0xe5940008,
        0xe594100c,
        0xe1a02004,
        0xe5903000,
        0xe5933008,
        0xe12fff33,
        0xe3a010c8,
        0xe2511001,
        0x1afffffd,
        0xe8bd8010,
    ];
    f.machine
        .cpu_mut()
        .write_mem(
            slow_rearm,
            &code
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    f.socket(first, 2, [slow_rearm, CX, 0], &[]);
    f.socket(second, 2, [CALLBACK, CX + 16, 0], &[]);
    assert!(
        f.machine
            .deliver_signals(16)
            .unwrap()
            .contains(&Outcome::Budget)
    );
    assert_eq!(f.read(CX + 16), 0);
    for _ in 0..10 {
        f.machine.advance(1000).unwrap();
        if f.machine.handset_event_ready() {
            break;
        }
    }
    assert!(f.machine.handset_event_ready());
    f.machine.deliver_signals(16).unwrap();
    assert_eq!(
        f.read(CX + 16),
        1,
        "callback antigo não pode ficar atrás de trabalho rearmado"
    );
    for _ in 0..10 {
        f.machine.advance(1000).unwrap();
        if f.machine.handset_event_ready() {
            break;
        }
    }
    f.socket(first, 4, [0; 3], &[]);
}

#[test]
fn offline_socket_stream_and_datagram_have_real_independent_lifetimes() {
    let mut f = Fixture::new();
    let tcp = f.open(0);
    let udp = f.open(1);
    assert_ne!(tcp, udp);
    assert_eq!(f.socket(tcp, 6, [0; 3], &[]), 0, "sem erro inicial");
    assert_eq!(f.socket(tcp, 0, [0; 3], &[]), 2);
    assert_eq!(f.socket(tcp, 1, [0; 3], &[]), 1);
    assert_eq!(f.api(Interface::NetLegacy, "Release", [f.net, 0, 0, 0]), 0);
    assert_eq!(f.socket(tcp, 6, [0; 3], &[]), 0);
    assert_eq!(f.socket(tcp, 1, [0; 3], &[]), 0);
    assert_eq!(f.socket(udp, 1, [0; 3], &[]), 0);
}

#[test]
fn offline_socket_invalid_type_has_specific_manager_error() {
    let mut f = Fixture::new();
    for kind in [2, u32::MAX] {
        assert_eq!(
            f.api(Interface::NetLegacy, "OpenSocket", [f.net, kind, 0, 0]),
            0
        );
        assert_eq!(
            f.api(Interface::NetLegacy, "GetLastError", [f.net, 0, 0, 0]),
            0x207
        );
    }
}

#[test]
fn offline_socket_connect_reports_error_after_return_with_correct_abi() {
    let mut f = Fixture::new();
    let tcp = f.open(0);
    assert_eq!(f.socket(tcp, 7, [0x04030201, 0x5000, CALLBACK], &[CX]), 0);
    assert_eq!(
        f.read(CX),
        0,
        "construtores não recebem callbacks reentrantes"
    );
    f.deliver();
    assert_eq!(f.read(CX), 1);
    assert_eq!(f.read(CX + 4), ENETDOWN);
    assert_eq!(f.socket(tcp, 6, [0; 3], &[]), ENETDOWN);
    f.deliver();
    assert_eq!(f.read(CX), 1, "conclusão de disparo único");
    assert_eq!(f.socket(tcp, 7, [0x04030201, 0x5000, 0], &[CX]), ERROR);
    f.deliver();
    assert_eq!(f.read(CX), 1, "callback NULL não registra conclusão");
}

#[test]
fn offline_socket_datagram_connect_uses_deferred_type_error() {
    let mut f = Fixture::new();
    let udp = f.open(1);
    assert_eq!(f.socket(udp, 7, [0x04030201, 0x5000, CALLBACK], &[CX]), 0);
    assert_eq!(f.read(CX), 0);
    f.deliver();
    assert_eq!(f.read(CX), 1);
    assert_eq!(f.read(CX + 4), EOPNOTSUPP);
}

#[test]
fn offline_socket_bind_send_and_realize_fail_without_writes_or_success_bytes() {
    let mut f = Fixture::new();
    let tcp = f.open(0);
    let udp = f.open(1);
    f.machine.cpu_mut().write_mem(DATA, &[0xa5; 32]).unwrap();
    assert_eq!(f.socket(tcp, 8, [0, 0x5000, 0], &[]), ERROR);
    assert_eq!(f.socket(tcp, 6, [0; 3], &[]), EOPNOTSUPP);
    assert_eq!(f.socket(udp, 8, [0, 0x5000, 0], &[]), ERROR);
    assert_eq!(f.socket(udp, 6, [0; 3], &[]), ENETDOWN);
    assert_eq!(
        f.socket(udp, 12, [DATA, 32, 0], &[0x04030201, 0x5000]),
        ERROR
    );
    assert_eq!(f.socket(udp, 6, [0; 3], &[]), ENETDOWN);
    assert_eq!(f.socket(udp, 15, [0x1003, 0, 0], &[]), ERROR);
    assert_eq!(f.socket(udp, 6, [0; 3], &[]), ENETDOWN);
    assert_eq!(f.socket(udp, 15, [0xdead, DATA, 0], &[]), 20);
    assert_eq!(f.machine.dump(DATA, 32).unwrap(), vec![0xa5; 32]);
    assert_eq!(
        f.api(Interface::NetLegacy, "NetStatus", [f.net, 0, 0, 0]),
        4
    );
}

#[test]
fn offline_socket_callbacks_are_one_shot_pair_cancelled_and_release_owned() {
    let mut f = Fixture::new();
    let udp = f.open(1);
    f.socket(udp, 2, [CALLBACK, CX, 0], &[]);
    f.socket(udp, 14, [CALLBACK, CX + 16, 0], &[]);
    assert_eq!(f.read(CX), 0);
    f.socket(udp, 4, [CALLBACK, CX, 0], &[]);
    f.deliver();
    assert_eq!(f.read(CX), 0, "Cancel remove só o par solicitado");
    assert_eq!(f.read(CX + 16), 1, "erro terminal também é progresso");
    f.deliver();
    assert_eq!(f.read(CX + 16), 1);
    f.socket(udp, 2, [CALLBACK, CX, 0], &[]);
    f.socket(udp, 14, [CALLBACK, CX + 16, 0], &[]);
    f.socket(udp, 4, [0; 3], &[]);
    f.deliver();
    assert_eq!(f.read(CX), 0);
    assert_eq!(f.read(CX + 16), 1);
    f.socket(udp, 7, [0x04030201, 0x5000, CALLBACK], &[CX + 32]);
    f.socket(udp, 1, [0; 3], &[]);
    f.deliver();
    assert_eq!(
        f.read(CX + 32),
        0,
        "Release final cancela o callback do socket"
    );
}

#[test]
fn offline_socket_cancel_connect_and_null_registration_cancel_their_channels() {
    let mut f = Fixture::new();
    let tcp = f.open(0);
    f.socket(tcp, 7, [0x04030201, 0x5000, CALLBACK], &[CX]);
    f.socket(tcp, 2, [CALLBACK, CX + 16, 0], &[]);
    f.socket(tcp, 14, [CALLBACK, CX + 32, 0], &[]);
    f.socket(tcp, 4, [CALLBACK, CX, 0], &[]);
    f.socket(tcp, 2, [0; 3], &[]);
    f.socket(tcp, 14, [0; 3], &[]);
    f.deliver();
    assert_eq!(f.machine.dump(CX, 48).unwrap(), vec![0; 48]);
}

#[test]
fn offline_socket_callback_rearming_waits_a_round_without_starving_another_socket() {
    let mut f = Fixture::new();
    let first = f.open(1);
    let second = f.open(1);
    let rearm = CALLBACK + 0x40;
    f.machine.cpu_mut().write_u32(CX + 8, first).unwrap();
    f.machine.cpu_mut().write_u32(CX + 12, rearm).unwrap();
    // Incrementa contador e rearma Readable(this,rearm,context).
    let code = [
        0xe92d4010u32,
        0xe1a04000,
        0xe5941000,
        0xe2811001,
        0xe5841000,
        0xe5940008,
        0xe594100c,
        0xe1a02004,
        0xe5903000,
        0xe5933008,
        0xe12fff33,
        0xe8bd8010,
    ];
    f.machine
        .cpu_mut()
        .write_mem(
            rearm,
            &code
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    f.socket(first, 2, [rearm, CX, 0], &[]);
    f.socket(second, 2, [CALLBACK, CX + 16, 0], &[]);
    let outcomes = f.machine.deliver_signals(10000).unwrap();
    assert!(
        outcomes
            .iter()
            .all(|o| matches!(o, Outcome::Returned { .. }))
    );
    assert_eq!(
        f.read(CX),
        1,
        "rearmar aguarda outra volta, sem laço infinito"
    );
    assert_eq!(f.read(CX + 16), 1, "o segundo socket não fica faminto");
    let outcomes = f.machine.deliver_signals(10000).unwrap();
    assert!(
        outcomes
            .iter()
            .all(|o| matches!(o, Outcome::Returned { .. }))
    );
    assert_eq!(f.read(CX), 2);
    assert_eq!(f.read(CX + 16), 1);
    f.socket(first, 4, [0; 3], &[]);
}

#[test]
fn offline_socket_state_and_pending_connect_survive_save_restore() {
    let mut f = Fixture::new();
    let tcp = f.open(0);
    f.socket(tcp, 7, [0x04030201, 0x5000, CALLBACK], &[CX]);
    let saved = f.machine.grava_estado();
    f.deliver();
    assert_eq!(f.read(CX), 1);
    f.machine.restaura_estado(&saved).unwrap();
    assert_eq!(f.read(CX), 0);
    f.deliver();
    assert_eq!(f.read(CX), 1);
    assert_eq!(f.read(CX + 4), ENETDOWN);
    assert_eq!(f.socket(tcp, 6, [0; 3], &[]), ENETDOWN);
    f.deliver();
    assert_eq!(f.read(CX), 1);
}

#[test]
fn offline_socket_restore_requires_matching_unique_live_host_state() {
    let mut f = Fixture::new();
    let tcp = f.open(0);
    let saved = f.machine.grava_estado();
    let reader = zeebx::save_state::Leitor::abre(&saved).unwrap();
    let section = "tab.socket_legacy.state";
    // Registros: objeto,tipo,erro, três callbacks (presença,função,contexto),erroConnect.
    let valid = vec![tcp, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let mut duplicate = valid.clone();
    duplicate[1] = 1;
    let mut wrong_kind = valid.clone();
    wrong_kind[0] = f.net;
    let mut bad_presence = valid.clone();
    bad_presence[3] = 2;
    for rows in [
        None,
        Some(vec![valid.clone(), duplicate]),
        Some(vec![wrong_kind]),
        Some(vec![bad_presence]),
    ] {
        let mut sections = zeebx::save_state::Secoes::nova();
        for name in reader.nomes().into_iter().filter(|name| *name != section) {
            sections.poe(name, reader.secao(name).unwrap().to_vec());
        }
        if let Some(rows) = rows {
            sections.poe_registros(section, rows);
        }
        assert!(
            f.machine.restaura_estado(&sections.fecha()).is_err(),
            "objeto vivo exige estado único, tipo correto e callbacks válidos"
        );
    }
}

#[test]
fn offline_socket_old_save_without_socket_section_remains_compatible() {
    let mut f = Fixture::new();
    let saved = f.machine.grava_estado();
    let reader = zeebx::save_state::Leitor::abre(&saved).unwrap();
    let old = zeebx::save_state::escreve(
        &reader
            .nomes()
            .into_iter()
            .filter(|name| *name != "tab.socket_legacy.state")
            .map(|name| (name.to_owned(), reader.secao(name).unwrap().to_vec()))
            .collect::<Vec<_>>(),
    );
    f.machine.restaura_estado(&old).unwrap();
    assert_eq!(
        f.api(Interface::NetLegacy, "NetStatus", [f.net, 0, 0, 0]),
        4
    );
}

#[test]
fn offline_socket_reused_object_does_not_inherit_old_snapshot_delivery() {
    let mut f = Fixture::new();
    let first = f.open(1);
    let second = f.open(1);
    let replace = CALLBACK + 0x40;
    let old_context = CX + 64;
    let new_context = CX + 80;
    f.machine
        .cpu_mut()
        .write_mem(old_context, &[0; 32])
        .unwrap();
    for (offset, value) in [(8, second), (12, f.net), (16, CALLBACK), (20, new_context)] {
        f.machine.cpu_mut().write_u32(CX + offset, value).unwrap();
    }
    // Release(segundo), OpenSocket(net,UDP), Readable(novo,CALLBACK,new_context).
    let code = [
        0xe92d4010u32,
        0xe1a04000,
        0xe5940008,
        0xe5903000,
        0xe5933004,
        0xe12fff33,
        0xe594000c,
        0xe3a01001,
        0xe5903000,
        0xe5933014,
        0xe12fff33,
        0xe5840018,
        0xe5941010,
        0xe5942014,
        0xe5903000,
        0xe5933008,
        0xe12fff33,
        0xe8bd8010,
    ];
    f.machine
        .cpu_mut()
        .write_mem(
            replace,
            &code
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    f.socket(first, 2, [replace, CX, 0], &[]);
    f.socket(second, 2, [CALLBACK, old_context, 0], &[]);
    let outcomes = f.machine.deliver_signals(10000).unwrap();
    assert!(
        outcomes
            .iter()
            .all(|o| matches!(o, Outcome::Returned { .. }))
    );
    assert_eq!(
        f.read(CX + 24),
        second,
        "o alocador reutiliza o endereço liberado"
    );
    assert_eq!(
        f.read(old_context),
        0,
        "o objeto antigo morreu antes da entrega"
    );
    assert_eq!(
        f.read(new_context),
        0,
        "registro novo aguarda a próxima volta"
    );
    f.deliver();
    assert_eq!(f.read(old_context), 0);
    assert_eq!(f.read(new_context), 1);
}

#[test]
fn offline_socket_budgeted_callback_save_preserves_pending_tail() {
    let mut f = Fixture::new();
    let first = f.open(1);
    let second = f.open(1);
    let slow = CALLBACK + 0x40;
    let code = [
        0xe3a010c8u32,
        0xe2511001,
        0x1afffffd,
        0xe3a01001,
        0xe5801000,
        0xe12fff1e,
    ];
    f.machine
        .cpu_mut()
        .write_mem(
            slow,
            &code
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    f.socket(first, 2, [slow, CX, 0], &[]);
    f.socket(second, 2, [CALLBACK, CX + 16, 0], &[]);
    assert_eq!(
        f.machine.deliver_signals(16).unwrap(),
        vec![Outcome::Budget]
    );
    let saved = f.machine.grava_estado();
    f.machine.restaura_estado(&saved).unwrap();
    for _ in 0..100 {
        f.machine.advance(16).unwrap();
        if f.machine.handset_event_ready() {
            break;
        }
    }
    assert_eq!(f.read(CX), 1);
    assert_eq!(
        f.read(CX + 16),
        0,
        "a cauda aguarda o callback interrompido retornar"
    );
    f.deliver();
    assert_eq!(f.read(CX), 1);
    assert_eq!(f.read(CX + 16), 1);
}
