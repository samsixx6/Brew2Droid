use std::path::PathBuf;
use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend},
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    storage::StoragePaths,
};
const CLASS: u32 = 0x0107_1647;
const IID: u32 = 0x0107_163b;
const RESULTS_IID: u32 = 0x0107_000a;
const OUT: u32 = loader::HEAP_BASE + 0x4000;
const FILTER: u32 = OUT + 0x100;
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
                "pim-contacts-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&root).unwrap();
        let storage = StoragePaths::from_root(root.join("runtime"));
        storage.create_dirs().unwrap();
        let image = ModImage::parse(
            [0xe3a00000u32, 0xe12fff1e]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect(),
        )
        .unwrap();
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
        self.m.cpu_mut().write_u32(OUT, 0xdeadbeef).unwrap();
        assert_eq!(
            run(
                &mut self.m,
                aee::encode(Interface::Shell, 2),
                [loader::OBJECT_BASE, CLASS, OUT, 0]
            ),
            0,
            "a classe pública deve expor pim_IContacts, sem agenda fictícia"
        );
        let o = self.m.cpu().read_u32(OUT).unwrap();
        assert_ne!(o, 0);
        o
    }
    fn filter(&mut self, value: &str) {
        self.m
            .cpu_mut()
            .write_mem(
                FILTER,
                &value
                    .encode_utf16()
                    .chain([0])
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
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
fn run(m: &mut Machine<BackendPadrao>, f: u32, args: [u32; 4]) -> u32 {
    match m.call_guest(f, args, 20000).unwrap() {
        Outcome::Returned { code } => code,
        o => panic!("{o:?}"),
    }
}
fn call(m: &mut Machine<BackendPadrao>, o: u32, slot: u32, args: [u32; 3]) -> u32 {
    let table = m.cpu().read_u32(o).unwrap();
    let f = m.cpu().read_u32(table + slot * 4).unwrap();
    run(m, f, [o, args[0], args[1], args[2]])
}
fn count(f: &mut Fixture, o: u32, query: &str) -> u32 {
    f.filter(query);
    call(&mut f.m, o, 21, [FILTER, OUT + 4, 0])
}

#[test]
fn pim_contacts_factory_and_iqi_use_the_public_interface_and_native_errors() {
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
    assert_eq!(call(&mut f.m, o, 2, [IID, OUT, 0]), 0);
    assert_eq!(f.m.cpu().read_u32(OUT).unwrap(), o);
    assert_eq!(call(&mut f.m, o, 1, [0; 3]), 1);
    assert_eq!(call(&mut f.m, o, 2, [0x01000001, OUT, 0]), 0);
    assert_eq!(call(&mut f.m, o, 1, [0; 3]), 1);
    f.m.cpu_mut().write_u32(OUT, 0xdeadbeef).unwrap();
    assert_eq!(call(&mut f.m, o, 2, [0xdeadbeef, OUT, 0]), 3);
    assert_eq!(f.m.cpu().read_u32(OUT).unwrap(), 0);
    assert_eq!(call(&mut f.m, o, 2, [IID, 0, 0]), 14);
    assert_eq!(call(&mut f.m, o, 2, [IID, 0xdead0000, 0]), 14);
    assert_eq!(call(&mut f.m, o, 1, [0; 3]), 0);
}
#[test]
fn empty_contacts_count_matches_valid_public_filters_and_preserves_guards() {
    let mut f = Fixture::new();
    let o = f.create();
    for query in [
        "",
        "ORDERBY LASTNAME",
        "FIRSTNAME = 'Carl' AND NUMBER LIKE '555%'",
        "LASTNAME LIKE 'O''Neil%' ORDERBY NUMBER DESC",
        "CHANGECOUNTER > 0",
        "LASTNAME ISNOTNULL",
        "NUMBER LIKE '55!_%' ESCAPE '!'",
    ] {
        f.m.cpu_mut().write_mem(OUT, &[0xa5; 12]).unwrap();
        assert_eq!(count(&mut f, o, query), 0, "{query}");
        let mut bytes = [0; 12];
        f.m.cpu().read_mem(OUT, &mut bytes).unwrap();
        assert_eq!(
            bytes,
            [0xa5, 0xa5, 0xa5, 0xa5, 0, 0, 0, 0, 0xa5, 0xa5, 0xa5, 0xa5]
        );
    }
}
#[test]
fn malformed_unterminated_or_invalid_filters_and_outputs_fail_without_false_counts() {
    let mut f = Fixture::new();
    let o = f.create();
    for query in [
        "FIRSTNAME = 'unterminated",
        "LASTNAME =",
        "FIRSTNAME = 'A' OR NUMBER = '5'",
        "ORDERBY",
        "NAME LIKE 'a' ESCAPE 'ab'",
    ] {
        f.m.cpu_mut().write_u32(OUT + 4, 0xa5a5a5a5).unwrap();
        assert_eq!(count(&mut f, o, query), 14, "{query}");
        assert_eq!(f.m.cpu().read_u32(OUT + 4).unwrap(), 0xa5a5a5a5);
    }
    assert_eq!(count(&mut f, o, "CUSTOMFIELD = 'x'"), 20);
    f.filter("FIRSTNAME =");
    f.m.cpu_mut().write_u32(OUT, 0xdeadbeef).unwrap();
    assert_eq!(call(&mut f.m, o, 20, [FILTER, OUT, 0]), 14);
    assert_eq!(f.m.cpu().read_u32(OUT).unwrap(), 0);
    f.filter("");
    assert_eq!(call(&mut f.m, o, 21, [FILTER, 0, 0]), 14);
    assert_eq!(call(&mut f.m, o, 21, [FILTER, 0xdead0000, 0]), 14);
    assert_eq!(call(&mut f.m, o, 21, [0, OUT, 0]), 14);
    assert_eq!(call(&mut f.m, o, 21, [0xdead0000, OUT, 0]), 14);
    let edge = loader::STACK_BASE + loader::STACK_SIZE as u32 - 2;
    f.m.cpu_mut().write_mem(edge, &[65, 0]).unwrap();
    assert_eq!(call(&mut f.m, o, 21, [edge, OUT, 0]), 14);
    f.m.cpu_mut()
        .write_mem(FILTER, &[65, 0].repeat(4096))
        .unwrap();
    assert_eq!(call(&mut f.m, o, 21, [FILTER, OUT, 0]), 14);
}
#[test]
fn empty_results_are_real_independent_objects_and_end_with_sdk_enomore() {
    let mut f = Fixture::new();
    let o = f.create();
    f.filter("");
    let mut results = Vec::new();
    for _ in 0..2 {
        assert_eq!(call(&mut f.m, o, 20, [FILTER, OUT, 0]), 0);
        results.push(f.m.cpu().read_u32(OUT).unwrap());
    }
    assert_ne!(results[0], results[1]);
    assert_eq!(call(&mut f.m, o, 1, [0; 3]), 0);
    for r in results {
        assert_eq!(call(&mut f.m, r, 2, [RESULTS_IID, OUT, 0]), 0);
        assert_eq!(f.m.cpu().read_u32(OUT).unwrap(), r);
        assert_eq!(call(&mut f.m, r, 1, [0; 3]), 1);
        for _ in 0..2 {
            f.m.cpu_mut().write_u32(OUT, 0xdeadbeef).unwrap();
            assert_eq!(call(&mut f.m, r, 3, [OUT, 0, 0]), 47);
            assert_eq!(f.m.cpu().read_u32(OUT).unwrap(), 0);
            assert_eq!(call(&mut f.m, r, 4, [0; 3]), 0);
        }
        assert_eq!(call(&mut f.m, r, 3, [0, 0, 0]), 14);
        assert_eq!(call(&mut f.m, r, 3, [0xdead0000, 0, 0]), 14);
        assert_eq!(call(&mut f.m, r, 1, [0; 3]), 0);
    }
}
#[test]
fn empty_record_reads_and_unsupported_writes_never_fabricate_data_or_success() {
    let mut f = Fixture::new();
    let o = f.create();
    f.m.cpu_mut().write_u32(OUT, 0xdeadbeef).unwrap();
    assert_eq!(call(&mut f.m, o, 6, [1, OUT, 0]), 16);
    assert_eq!(f.m.cpu().read_u32(OUT).unwrap(), 0);
    f.m.cpu_mut().write_u32(OUT, 0xdeadbeef).unwrap();
    assert_eq!(call(&mut f.m, o, 3, [OUT, 0, 0]), 20);
    assert_eq!(f.m.cpu().read_u32(OUT).unwrap(), 0);
    for slot in [4, 5, 8, 11, 12, 13, 18, 19, 22, 23, 24] {
        assert_eq!(call(&mut f.m, o, slot, [0; 3]), 20, "slot{slot}");
    }
    assert_eq!(call(&mut f.m, o, 7, [1, 0, 0]), 16);
    for slot in [14, 27, 28] {
        f.m.cpu_mut().write_u32(OUT, 0xa5a5a5a5).unwrap();
        assert_eq!(call(&mut f.m, o, slot, [OUT, 0, 0]), 20);
        assert_eq!(f.m.cpu().read_u32(OUT).unwrap(), 0xa5a5a5a5);
    }
    f.m.cpu_mut().write_u32(OUT, 0xdeadbeef).unwrap();
    assert_eq!(call(&mut f.m, o, 25, [0, 1, OUT]), 20);
    assert_eq!(f.m.cpu().read_u32(OUT).unwrap(), 0);
    assert_eq!(call(&mut f.m, o, 25, [0, 1, 0]), 14);
    assert_eq!(count(&mut f, o, ""), 0);
    assert_eq!(f.m.cpu().read_u32(OUT + 4).unwrap(), 0);
}
#[test]
fn change_sequence_is_exactly_eight_zero_bytes_and_invalid_outputs_are_atomic() {
    let mut f = Fixture::new();
    let o = f.create();
    f.m.cpu_mut().write_mem(OUT, &[0xa5; 16]).unwrap();
    assert_eq!(call(&mut f.m, o, 26, [OUT + 4, 0, 0]), 0);
    let mut bytes = [0; 16];
    f.m.cpu().read_mem(OUT, &mut bytes).unwrap();
    assert_eq!(
        bytes,
        [
            0xa5, 0xa5, 0xa5, 0xa5, 0, 0, 0, 0, 0, 0, 0, 0, 0xa5, 0xa5, 0xa5, 0xa5
        ]
    );
    assert_eq!(call(&mut f.m, o, 26, [0, 0, 0]), 14);
    let edge = loader::STACK_BASE + loader::STACK_SIZE as u32 - 4;
    f.m.cpu_mut().write_u32(edge, 0xa5a5a5a5).unwrap();
    assert_eq!(call(&mut f.m, o, 26, [edge, 0, 0]), 14);
    assert_eq!(f.m.cpu().read_u32(edge).unwrap(), 0xa5a5a5a5);
}
#[test]
fn contacts_and_results_reference_counts_restore_without_hidden_contact_state() {
    let mut f = Fixture::new();
    let o = f.create();
    f.filter("");
    assert_eq!(call(&mut f.m, o, 20, [FILTER, OUT, 0]), 0);
    let r = f.m.cpu().read_u32(OUT).unwrap();
    assert_eq!(call(&mut f.m, r, 0, [0; 3]), 2);
    let state = f.m.grava_estado();
    assert_eq!(call(&mut f.m, o, 1, [0; 3]), 0);
    assert_eq!(call(&mut f.m, r, 1, [0; 3]), 1);
    assert_eq!(call(&mut f.m, r, 1, [0; 3]), 0);
    f.m.restaura_estado(&state).unwrap();
    assert_eq!(count(&mut f, o, ""), 0);
    assert_eq!(call(&mut f.m, r, 3, [OUT, 0, 0]), 47);
    assert_eq!(call(&mut f.m, r, 1, [0; 3]), 1);
    assert_eq!(call(&mut f.m, r, 1, [0; 3]), 0);
    assert_eq!(call(&mut f.m, o, 1, [0; 3]), 0);
}

#[test]
fn contacts_exhaustion_uses_native_oom_and_preserves_live_empty_database() {
    let mut f = Fixture::new();
    let o = f.create();
    let mut live = Vec::new();
    loop {
        let status = run(
            &mut f.m,
            aee::encode(Interface::Shell, 2),
            [loader::OBJECT_BASE, 0x01001001, OUT, 0],
        );
        if status != 0 {
            break;
        }
        live.push(f.m.cpu().read_u32(OUT).unwrap());
        assert!(live.len() <= loader::OBJECT_SIZE / 4);
    }
    f.filter("");
    f.m.cpu_mut().write_u32(OUT, 0xdeadbeef).unwrap();
    assert_eq!(
        call(&mut f.m, o, 20, [FILTER, OUT, 0]),
        2,
        "Results indisponível deve devolver AEE_ENOMEMORY"
    );
    assert_eq!(f.m.cpu().read_u32(OUT).unwrap(), 0);
    assert_eq!(count(&mut f, o, ""), 0);
    assert_eq!(f.m.cpu().read_u32(OUT + 4).unwrap(), 0);
    f.m.cpu_mut().write_u32(OUT, 0xdeadbeef).unwrap();
    assert_eq!(
        run(
            &mut f.m,
            aee::encode(Interface::Shell, 2),
            [loader::OBJECT_BASE, CLASS, OUT, 0]
        ),
        2,
        "a fábrica moderna também usa AEE_ENOMEMORY2"
    );
    assert_eq!(f.m.cpu().read_u32(OUT).unwrap(), 0);
    let free = live.pop().unwrap();
    call(&mut f.m, free, 1, [0; 3]);
    assert_eq!(call(&mut f.m, o, 20, [FILTER, OUT, 0]), 0);
    let results = f.m.cpu().read_u32(OUT).unwrap();
    assert_eq!(call(&mut f.m, results, 3, [OUT, 0, 0]), 47);
    assert_eq!(call(&mut f.m, results, 1, [0; 3]), 0);
    for object in live {
        call(&mut f.m, object, 1, [0; 3]);
    }
    assert_eq!(call(&mut f.m, o, 1, [0; 3]), 0);
}
