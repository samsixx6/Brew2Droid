use std::path::PathBuf;
use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend, Reg},
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    storage::StoragePaths,
};

const CLASS: u32 = 0x0106_e415;
const IID: u32 = 0x0106_e2e0;
const OUT: u32 = loader::HEAP_BASE + 0x2000;

struct Fixture {
    m: Machine<BackendPadrao>,
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests")
            .join(format!(
                "mem-cache-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&root).unwrap();
        let image = ModImage::parse(
            [0xe3a0_0000u32, 0xe12f_ff1e]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect(),
        )
        .unwrap();
        let storage = StoragePaths::from_root(root.join("runtime"));
        storage.create_dirs().unwrap();
        let mut m = Machine::new_with_storage(
            BackendPadrao::new().unwrap(),
            loader::load_with(&image, &[]).unwrap(),
            &root,
            &storage,
            None,
        );
        assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
        Self { m, root }
    }
    fn create(&mut self) -> u32 {
        self.m.cpu_mut().write_u32(OUT, 0xfeedface).unwrap();
        assert_eq!(
            run(
                &mut self.m,
                aee::encode(Interface::Shell, 2),
                [loader::OBJECT_BASE, CLASS, OUT, 0]
            ),
            0,
            "a classe pública CMemCache1 deve expor a vtable IMemCache1 real"
        );
        let o = self.m.cpu().read_u32(OUT).unwrap();
        assert_ne!(o, 0);
        o
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let path = self.root.canonicalize().unwrap();
        let allowed = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests")
            .canonicalize()
            .unwrap();
        assert!(path.starts_with(allowed));
        std::fs::remove_dir_all(path).unwrap();
    }
}
fn run(m: &mut Machine<BackendPadrao>, target: u32, args: [u32; 4]) -> u32 {
    match m.call_guest(target, args, 10000).unwrap() {
        Outcome::Returned { code } => code,
        outcome => panic!("{outcome:?}"),
    }
}
fn method(m: &Machine<BackendPadrao>, o: u32, slot: u32) -> u32 {
    let table = m.cpu().read_u32(o).unwrap();
    m.cpu().read_u32(table + 4 * slot).unwrap()
}
fn call(m: &mut Machine<BackendPadrao>, o: u32, slot: u32, args: [u32; 3]) -> u32 {
    let target = method(m, o, slot);
    run(m, target, [o, args[0], args[1], args[2]])
}
fn clear(
    m: &mut Machine<BackendPadrao>,
    o: u32,
    address: u32,
    size: u32,
    operation: u32,
    flags: u32,
) -> u32 {
    let target = method(m, o, 3);
    let sp = m.cpu().read_reg(Reg::Sp);
    let args = (sp - 8) & !7;
    m.cpu_mut().write_u32(args, flags).unwrap();
    m.cpu_mut().write_reg(Reg::Sp, args);
    let result = run(m, target, [o, address, size, operation]);
    m.cpu_mut().write_reg(Reg::Sp, sp);
    result
}

#[test]
fn mem_cache_public_factory_and_iqi_keep_references_and_clear_rejected_output() {
    let mut f = Fixture::new();
    let o = f.create();
    assert_eq!(
        run(
            &mut f.m,
            aee::encode(Interface::Shell, 3),
            [loader::OBJECT_BASE, CLASS, 0, 0]
        ),
        1
    );
    for iid in [IID, 0x0100_0001] {
        assert_eq!(call(&mut f.m, o, 2, [iid, OUT, 0]), 0);
        assert_eq!(f.m.cpu().read_u32(OUT).unwrap(), o);
    }
    assert_eq!(call(&mut f.m, o, 1, [0; 3]), 2);
    assert_eq!(call(&mut f.m, o, 1, [0; 3]), 1);
    f.m.cpu_mut().write_u32(OUT, 0xfeedface).unwrap();
    assert_eq!(call(&mut f.m, o, 2, [0xdeadbeef, OUT, 0]), 3);
    assert_eq!(f.m.cpu().read_u32(OUT).unwrap(), 0);
    assert_eq!(call(&mut f.m, o, 2, [IID, 0, 0]), 14);
    assert_eq!(call(&mut f.m, o, 2, [IID, 0xdead0000, 0]), 14);
    assert_eq!(call(&mut f.m, o, 1, [0; 3]), 0);
}

#[test]
fn cache_clear_accepts_public_data_and_instruction_operations_without_changing_bytes() {
    let mut f = Fixture::new();
    let o = f.create();
    let data = OUT + 64;
    f.m.cpu_mut().write_mem(data, b"preservar-cache!").unwrap();
    for (operation, flags) in [(1, 1), (2, 1), (3, 1), (1, 2), (2, 2), (3, 2), (3, 3)] {
        assert_eq!(
            clear(&mut f.m, o, data, 16, operation, flags),
            0,
            "op={operation} flags={flags}"
        );
        let mut bytes = [0; 16];
        f.m.cpu().read_mem(data, &mut bytes).unwrap();
        assert_eq!(&bytes, b"preservar-cache!");
    }
}

#[test]
fn cache_clear_reads_fifth_argument_from_the_arm_stack_and_rejects_unknown_bits() {
    let mut f = Fixture::new();
    let o = f.create();
    assert_eq!(
        clear(&mut f.m, o, OUT, 4, 1, 4),
        1,
        "flags fora do SDK vêm de sp, não de r3"
    );
    assert_eq!(clear(&mut f.m, o, OUT, 4, 4, 1), 1);
    assert_eq!(clear(&mut f.m, o, 0xfffffff0, 32, 1, 1), 1);
    assert_eq!(
        clear(&mut f.m, o, OUT, 4, 1, 1),
        0,
        "a recusa não deixa estado pendente"
    );
}

#[test]
fn coherent_cache_does_not_read_or_allocate_unmapped_and_zero_sized_ranges() {
    let mut f = Fixture::new();
    let o = f.create();
    assert_eq!(clear(&mut f.m, o, 0, 0, 3, 3), 0);
    assert_eq!(clear(&mut f.m, o, 0, 0xfffffff0, 3, 3), 0);
    assert_eq!(clear(&mut f.m, o, 0xfffffff0, 16, 3, 3), 0);
    assert!(f.m.bad_pointers().is_empty());
}

#[test]
fn cache_information_fails_without_inventing_hardware_geometry_or_touching_output() {
    let mut f = Fixture::new();
    let o = f.create();
    f.m.cpu_mut().write_mem(OUT, &[0xa5; 20]).unwrap();
    for kind in [1, 2, 3, 0xffff] {
        assert_eq!(call(&mut f.m, o, 4, [kind, OUT + 4, 0]), 1);
        let mut bytes = [0; 20];
        f.m.cpu().read_mem(OUT, &mut bytes).unwrap();
        assert_eq!(bytes, [0xa5; 20]);
    }
}

#[test]
fn instruction_cache_observes_code_stored_by_arm_without_corrupting_it() {
    let mut f = Fixture::new();
    let o = f.create();
    let code = loader::HEAP_BASE + 0x4000;
    let writer = code + 0x100;
    f.m.cpu_mut()
        .write_mem(
            code,
            &[0xe3a0_0001u32, 0xe12f_ff1e]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    f.m.cpu_mut()
        .write_mem(
            writer,
            &[0xe580_1000u32, 0xe12f_ff1e]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    assert_eq!(run(&mut f.m, code, [0; 4]), 1);
    run(&mut f.m, writer, [code, 0xe3a0_0002, 0, 0]);
    assert_eq!(clear(&mut f.m, o, code, 8, 3, 3), 0);
    assert_eq!(run(&mut f.m, code, [0; 4]), 2);
    assert_eq!(f.m.cpu().read_u32(code).unwrap(), 0xe3a0_0002);
    let thumb = code + 0x200;
    f.m.cpu_mut().write_u32(thumb, 0x4770_2001).unwrap();
    assert_eq!(run(&mut f.m, thumb | 1, [0; 4]), 1);
    run(&mut f.m, writer, [thumb, 0x4770_2002, 0, 0]);
    assert_eq!(clear(&mut f.m, o, thumb, 4, 3, 2), 0);
    assert_eq!(run(&mut f.m, thumb | 1, [0; 4]), 2);
    assert_eq!(f.m.cpu().read_u32(thumb).unwrap(), 0x4770_2002);
}

#[test]
fn stateless_cache_interface_and_reference_count_restore_with_save_state() {
    let mut f = Fixture::new();
    let o = f.create();
    assert_eq!(call(&mut f.m, o, 0, [0; 3]), 2);
    let state = f.m.grava_estado();
    assert_eq!(call(&mut f.m, o, 1, [0; 3]), 1);
    assert_eq!(call(&mut f.m, o, 1, [0; 3]), 0);
    f.m.restaura_estado(&state).unwrap();
    assert_eq!(clear(&mut f.m, o, OUT, 4, 1, 1), 0);
    assert_eq!(call(&mut f.m, o, 1, [0; 3]), 1);
    assert_eq!(call(&mut f.m, o, 1, [0; 3]), 0);
}
