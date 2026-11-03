use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend, Reg};
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

const OUT: u32 = loader::STACK_BASE + 0x1000;
const CX: u32 = OUT + 0x100;
const CALLBACK: u32 = loader::MODULE_BASE + 0x40;
const ENETDOWN: u32 = 0x216;

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
fn offline_socket_save_preserves_registration_order_across_socket_addresses() {
    let mut f = Fixture::new();
    let lower = f.open(0);
    let higher = f.open(0);
    let (function, first, second) = fifo_callbacks(&mut f);
    f.socket(higher, 7, [0x04030201, 0x5000, function], &[first]);
    f.socket(lower, 7, [0x04030201, 0x5000, function], &[second]);
    let saved = f.machine.grava_estado();
    f.deliver();
    assert_eq!([f.read(CX + 4), f.read(CX + 8)], [1, 2]);
    f.machine.restaura_estado(&saved).unwrap();
    f.deliver();
    assert_eq!(
        [f.read(CX + 4), f.read(CX + 8)],
        [1, 2],
        "save não pode promover endereço baixo à frente da inscrição antiga"
    );
}

#[test]
fn offline_socket_old_single_completion_save_without_queue_section_loads() {
    let mut f = Fixture::new();
    let tcp = f.open(0);
    f.socket(tcp, 7, [0x04030201, 0x5000, CALLBACK], &[CX]);
    let saved = f.machine.grava_estado();
    let reader = zeebx::save_state::Leitor::abre(&saved).unwrap();
    let old = zeebx::save_state::escreve(
        &reader
            .nomes()
            .into_iter()
            .filter(|name| {
                !matches!(
                    *name,
                    "tab.socket_legacy.connect_queue" | "tab.socket_legacy.callback_order"
                )
            })
            .map(|name| (name.to_owned(), reader.secao(name).unwrap().to_vec()))
            .collect::<Vec<_>>(),
    );
    f.machine.restaura_estado(&old).unwrap();
    f.deliver();
    assert_eq!(f.read(CX), 1);
    assert_eq!(f.read(CX + 4), ENETDOWN);
}

#[test]
fn offline_socket_restore_rejects_unowned_unbounded_or_successful_queue_entries() {
    let mut f = Fixture::new();
    let tcp = f.open(0);
    for cx in [CX, CX + 16] {
        f.socket(tcp, 7, [0x04030201, 0x5000, CALLBACK], &[cx]);
    }
    let saved = f.machine.grava_estado();
    let reader = zeebx::save_state::Leitor::abre(&saved).unwrap();
    let section = "tab.socket_legacy.connect_queue";
    let valid = vec![tcp, CALLBACK, CX + 16, ENETDOWN];
    let mut unowned = valid.clone();
    unowned[0] = f.net;
    let mut null_callback = valid.clone();
    null_callback[1] = 0;
    let mut fake_success = valid.clone();
    fake_success[3] = 0;
    for rows in [
        vec![unowned],
        vec![null_callback],
        vec![fake_success],
        vec![valid; 1024],
    ] {
        let mut sections = zeebx::save_state::Secoes::nova();
        for name in reader.nomes().into_iter().filter(|name| *name != section) {
            sections.poe(name, reader.secao(name).unwrap().to_vec());
        }
        sections.poe_registros(section, rows);
        assert!(f.machine.restaura_estado(&sections.fecha()).is_err());
    }
}

#[test]
fn offline_socket_restore_rejects_missing_duplicate_invalid_or_reversed_order() {
    let mut f = Fixture::new();
    let tcp = f.open(0);
    let udp = f.open(1);
    f.socket(tcp, 7, [0x04030201, 0x5000, CALLBACK], &[CX]);
    f.socket(udp, 14, [CALLBACK, CX + 16, 0], &[]);
    f.socket(tcp, 2, [CALLBACK, CX + 32, 0], &[]);
    f.socket(tcp, 7, [0x04030201, 0x5000, CALLBACK], &[CX + 48]);
    let saved = f.machine.grava_estado();
    let reader = zeebx::save_state::Leitor::abre(&saved).unwrap();
    let section = "tab.socket_legacy.callback_order";
    let valid = vec![
        vec![tcp, 0, 0],
        vec![udp, 2, 0],
        vec![tcp, 1, 0],
        vec![tcp, 0, 1],
    ];
    let mut missing = valid.clone();
    missing.pop();
    let mut duplicate = valid.clone();
    duplicate[2] = duplicate[0].clone();
    let mut unowned = valid.clone();
    unowned[0][0] = f.net;
    let mut unknown_channel = valid.clone();
    unknown_channel[0][1] = 3;
    let mut invalid_index = valid.clone();
    invalid_index[2][2] = 1;
    let mut reversed_fifo = valid.clone();
    reversed_fifo.swap(0, 3);
    for rows in [
        missing,
        duplicate,
        unowned,
        unknown_channel,
        invalid_index,
        reversed_fifo,
        vec![vec![tcp, 0, 0]; 8197],
    ] {
        let mut sections = zeebx::save_state::Secoes::nova();
        for name in reader.nomes().into_iter().filter(|name| *name != section) {
            sections.poe(name, reader.secao(name).unwrap().to_vec());
        }
        sections.poe_registros(section, rows);
        assert!(
            f.machine.restaura_estado(&sections.fecha()).is_err(),
            "ordem deve cobrir inscrições vivas uma vez e conservar a fila Connect"
        );
    }
}

#[test]
fn offline_socket_restore_rejects_order_without_any_socket_state() {
    let mut f = Fixture::new();
    let saved = f.machine.grava_estado();
    let reader = zeebx::save_state::Leitor::abre(&saved).unwrap();
    let mut sections = zeebx::save_state::Secoes::nova();
    for name in reader.nomes().into_iter().filter(|name| {
        !matches!(
            *name,
            "tab.socket_legacy.state" | "tab.socket_legacy.callback_order"
        )
    }) {
        sections.poe(name, reader.secao(name).unwrap().to_vec());
    }
    sections.poe_registros("tab.socket_legacy.callback_order", [vec![f.net, 1, 0]]);
    assert!(f.machine.restaura_estado(&sections.fecha()).is_err());
}

#[test]
fn offline_socket_saved_order_includes_read_write_and_queued_connect_notifications() {
    let mut f = Fixture::new();
    let tcp = f.open(0);
    let udp = f.open(1);
    let (function, first, second) = fifo_callbacks(&mut f);
    f.machine.cpu_mut().write_u32(CX + 80, 4).unwrap();
    f.machine.cpu_mut().write_u32(CX + 84, CX).unwrap();
    f.socket(tcp, 2, [function, first, 0], &[]);
    f.socket(tcp, 7, [0x04030201, 0x5000, function], &[second]);
    f.socket(udp, 14, [function, CX + 64, 0], &[]);
    f.socket(tcp, 7, [0x04030201, 0x5000, function], &[CX + 80]);
    let saved = f.machine.grava_estado();
    f.machine.restaura_estado(&saved).unwrap();
    f.deliver();
    assert_eq!(f.read(CX), 4);
    assert_eq!(
        [
            f.read(CX + 4),
            f.read(CX + 8),
            f.read(CX + 12),
            f.read(CX + 16)
        ],
        [1, 2, 3, 4]
    );
    assert_eq!(
        [
            f.read(first + 8),
            f.read(second + 8),
            f.read(CX + 72),
            f.read(CX + 88)
        ],
        [0, ENETDOWN, 0, ENETDOWN]
    );
}
