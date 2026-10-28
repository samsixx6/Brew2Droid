use std::path::PathBuf;

use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend};
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

const OUT: u32 = loader::STACK_BASE + 0x100;
const INPUT: u32 = loader::STACK_BASE + 0x1000;
const OUTPUT: u32 = loader::STACK_BASE + 0x40000;

struct Harness {
    machine: Machine<BackendPadrao>,
    work: PathBuf,
}

impl Harness {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let work = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local-tests").join(format!(
            "sha1-hash-test-{}-{}", std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        std::fs::create_dir_all(&work).unwrap();
        let storage = StoragePaths::from_root(work.join("runtime"));
        storage.create_dirs().unwrap();
        let image = ModImage::parse([0xe3a0_0000u32, 0xe12f_ff1e].into_iter()
            .flat_map(u32::to_le_bytes).collect()).unwrap();
        let module = loader::load_with(&image, &[]).unwrap();
        let mut result = Self {
            machine: Machine::new_with_storage(BackendPadrao::new().unwrap(), module, &work, &storage, None),
            work,
        };
        assert_eq!(result.machine.run(1000).unwrap(), Outcome::Returned { code: 0 });

        result
    }

    fn call(&mut self, interface: Interface, name: &str, args: [u32; 4]) -> u32 {
        let slot = (0..256).find(|slot| interface.method(*slot) == Some(name)).unwrap();
        match self.machine.call_guest(aee::encode(interface, slot), args, 1000).unwrap() {
            Outcome::Returned { code } => code,
            outcome => panic!("{name}: {outcome:?}"),
        }
    }

    fn create(&mut self, class: u32) -> u32 {
        assert_eq!(self.call(Interface::Shell, "CreateInstance", [loader::OBJECT_BASE, class, OUT, 0]), 0);
        let object = self.machine.cpu().read_u32(OUT).unwrap();
        assert_ne!(object, 0);
        object
    }


}

impl Drop for Harness {
    fn drop(&mut self) {
        assert!(self.work.canonicalize().unwrap().starts_with(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).canonicalize().unwrap().join("local-tests"),
        ));
        std::fs::remove_dir_all(&self.work).unwrap();
    }
}


const SIZE: u32 = OUT + 4;
fn digest(h: &mut Harness, hash: u32, capacity: u32) -> Vec<u8> {
    h.machine.cpu_mut().write_u32(SIZE, capacity).unwrap();
    h.machine.cpu_mut().write_mem(OUTPUT, &[0xa5; 24]).unwrap();
    h.call(Interface::Hash, "GetDigest", [hash, OUTPUT, SIZE, 0]);
    assert_eq!(h.machine.cpu().read_u32(SIZE).unwrap(), capacity.min(20));
    let mut bytes = vec![0; capacity.min(20) as usize];
    h.machine.cpu().read_mem(OUTPUT, &mut bytes).unwrap();
    let mut guard=[0;4];h.machine.cpu().read_mem(OUTPUT + capacity.min(20), &mut guard).unwrap();
    assert_eq!(guard,[0xa5;4]);
    bytes
}
#[test]
fn sha1_class_uses_standard_digest_and_keeps_algorithm_after_reset_save_and_reuse() {
    let mut h = Harness::new();
    let hash = h.create(0x0100_1034);
    let empty = [0xda,0x39,0xa3,0xee,0x5e,0x6b,0x4b,0x0d,0x32,0x55,0xbf,0xef,0x95,0x60,0x18,0x90,0xaf,0xd8,0x07,0x09];
    let abc = [0xa9,0x99,0x3e,0x36,0x47,0x06,0x81,0x6a,0xba,0x3e,0x25,0x71,0x78,0x50,0xc2,0x6c,0x9c,0xd0,0xd8,0x9d];
    assert_eq!(digest(&mut h,hash,20),empty);
    h.machine.cpu_mut().write_mem(INPUT,b"a").unwrap();
    h.call(Interface::Hash,"Update",[hash,INPUT,1,0]);
    let saved=h.machine.grava_estado();h.machine.restaura_estado(&saved).unwrap();
    h.machine.cpu_mut().write_mem(INPUT,b"bc").unwrap();
    h.call(Interface::Hash,"Update",[hash,INPUT,2,0]);
    assert_eq!(digest(&mut h,hash,20),abc);
    assert_eq!(digest(&mut h,hash,8),abc[..8]);
    h.call(Interface::Hash,"Reset",[hash,0,0,0]);
    assert_eq!(digest(&mut h,hash,20),empty);
    h.call(Interface::Hash,"Release",[hash,0,0,0]);
    let fresh=h.create(0x0100_1034);
    assert_eq!(digest(&mut h,fresh,20),empty);
}
