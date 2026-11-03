use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend};
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

const OUT: u32 = loader::STACK_BASE + 0x100;
const PCB: u32 = OUT + 0x100;
const CONTEXT: u32 = OUT + 0x200;
const RESULT: u32 = OUT + 0x300;
const HOST: u32 = OUT + 0x400;
const CALLBACK: u32 = loader::MODULE_BASE + 0x40;

struct Fixture {
    machine: Machine<BackendPadrao>,
    root: PathBuf,
    net: u32,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests")
            .join(format!(
                "offline-dns-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&root).unwrap();
        let image = ModImage::parse(
            [0xe3a00000u32, 0xe12fff1e]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect(),
        )
        .unwrap();
        let storage = StoragePaths::from_root(root.join("runtime"));
        let mut machine = Machine::new_with_storage(
            BackendPadrao::new().unwrap(),
            loader::load_with(&image, &[]).unwrap(),
            &root,
            &storage,
            None,
        );
        assert_eq!(machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
        let mut fixture = Self {
            machine,
            root,
            net: 0,
        };
        assert_eq!(
            fixture.api(
                Interface::Shell,
                "CreateInstance",
                [loader::OBJECT_BASE, 0x01001105, OUT, 0]
            ),
            0
        );
        fixture.net = fixture.machine.cpu().read_u32(OUT).unwrap();
        assert_ne!(fixture.net, 0);
        fixture
            .machine
            .cpu_mut()
            .write_mem(HOST, b"offline.example.invalid\0")
            .unwrap();
        fixture.machine.cpu_mut().write_mem(PCB, &[0; 28]).unwrap();
        fixture
            .machine
            .cpu_mut()
            .write_u32(PCB + 16, CALLBACK)
            .unwrap();
        fixture
            .machine
            .cpu_mut()
            .write_u32(PCB + 20, CONTEXT)
            .unwrap();
        fixture
            .machine
            .cpu_mut()
            .write_mem(CONTEXT, &[0; 16])
            .unwrap();
        fixture
            .machine
            .cpu_mut()
            .write_u32(CONTEXT + 4, RESULT)
            .unwrap();
        // O callback do guest conta entregas e observa o resultado já publicado.
        let code = [
            0xe5901000u32,
            0xe2811001,
            0xe5801000,
            0xe5902004,
            0xe5923000,
            0xe5803008,
            0xe5923004,
            0xe580300c,
            0xe12fff1e,
        ];
        fixture
            .machine
            .cpu_mut()
            .write_mem(
                CALLBACK,
                &code
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        fixture
    }

    fn api(&mut self, interface: Interface, method: &str, args: [u32; 4]) -> u32 {
        let slot = (0..256)
            .find(|&slot| interface.method(slot) == Some(method))
            .unwrap();
        match self
            .machine
            .call_guest(aee::encode(interface, slot), args, 10000)
            .unwrap()
        {
            Outcome::Returned { code } => code,
            outcome => panic!("{method}: {outcome:?}"),
        }
    }

    fn request(&mut self) {
        self.machine
            .cpu_mut()
            .write_mem(RESULT - 4, &[0xa5; 28])
            .unwrap();
        // GetHostByName é void: o contrato é pres e o AEECallback, não r0.
        self.api(
            Interface::NetLegacy,
            "GetHostByName",
            [self.net, RESULT, HOST, PCB],
        );
    }

    fn read(&self, address: u32) -> u32 {
        self.machine.cpu().read_u32(address).unwrap()
    }

    fn deliver(&mut self) {
        let outcomes = self.machine.deliver_signals(10000).unwrap();
        assert!(
            outcomes
                .iter()
                .all(|outcome| matches!(outcome, Outcome::Returned { .. }))
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let allowed = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests")
            .canonicalize()
            .unwrap();
        let root = self.root.canonicalize().unwrap();
        assert!(root.starts_with(allowed));
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn offline_dns_fills_exact_result_and_completes_asynchronously_once() {
    let mut fixture = Fixture::new();
    fixture.request();
    // AEENetworkTypes.h: int nResult e quatro endereços IPv4, total 20 bytes.
    assert_eq!(fixture.read(RESULT), 0x216);
    for offset in [4, 8, 12, 16] {
        assert_eq!(fixture.read(RESULT + offset), 0);
    }
    assert_eq!(fixture.read(RESULT - 4), 0xa5a5a5a5);
    assert_eq!(fixture.read(RESULT + 20), 0xa5a5a5a5);
    assert_eq!(
        fixture.read(CONTEXT),
        0,
        "não reentrar pelo despacho da API"
    );
    assert!(
        !fixture
            .machine
            .missing_apis()
            .iter()
            .any(|api| api == "INetMgr::GetHostByName")
    );
    fixture.deliver();
    assert_eq!(fixture.read(CONTEXT), 1);
    assert_eq!(fixture.read(CONTEXT + 8), 0x216);
    assert_eq!(fixture.read(CONTEXT + 12), 0);
    assert_eq!(fixture.read(PCB + 8), 0);
    fixture.deliver();
    assert_eq!(fixture.read(CONTEXT), 1);
}

#[test]
fn offline_dns_callback_can_be_cancelled_and_reused() {
    let mut fixture = Fixture::new();
    fixture.request();
    let cancel = fixture.read(PCB + 8);
    assert_ne!(cancel, 0);
    assert!(matches!(
        fixture
            .machine
            .call_guest(cancel, [PCB, 0, 0, 0], 10000)
            .unwrap(),
        Outcome::Returned { .. }
    ));
    fixture.deliver();
    assert_eq!(fixture.read(CONTEXT), 0);
    fixture.request();
    fixture.deliver();
    assert_eq!(fixture.read(CONTEXT), 1);
    fixture.deliver();
    assert_eq!(fixture.read(CONTEXT), 1);
}

#[test]
fn offline_dns_release_does_not_cancel_registered_completion() {
    let mut fixture = Fixture::new();
    fixture.request();
    assert_eq!(
        fixture.api(Interface::NetLegacy, "Release", [fixture.net, 0, 0, 0]),
        0
    );
    fixture.deliver();
    assert_eq!(fixture.read(CONTEXT), 1);
    assert_eq!(fixture.read(CONTEXT + 8), 0x216);
}

#[test]
fn offline_dns_queued_completion_survives_save_restore() {
    let mut fixture = Fixture::new();
    fixture.request();
    let saved = fixture.machine.grava_estado();
    fixture.deliver();
    assert_eq!(fixture.read(CONTEXT), 1);
    fixture.machine.restaura_estado(&saved).unwrap();
    assert_eq!(fixture.read(CONTEXT), 0);
    fixture.deliver();
    assert_eq!(fixture.read(CONTEXT), 1);
    assert_eq!(fixture.read(CONTEXT + 8), 0x216);
    fixture.deliver();
    assert_eq!(fixture.read(CONTEXT), 1);
}
