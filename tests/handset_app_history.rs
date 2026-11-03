use std::path::PathBuf;
use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend},
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    storage::StoragePaths,
};
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
                "handoff-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
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
        let mut m = Machine::new_with_storage(
            BackendPadrao::new().unwrap(),
            loader::load_with(&image, &[]).unwrap(),
            &root,
            &storage,
            None,
        );
        m.enable_handset_clock();
        assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
        m.set_installed_applets([(0x01001234, "first".into()), (0x01005678, "second".into())]);
        Self { m, root }
    }
    fn start(&mut self, cls: u32) -> u32 {
        let slot = (0..256)
            .find(|s| Interface::Shell.method(*s) == Some("StartApplet"))
            .unwrap();
        match self
            .m
            .call_guest(
                aee::encode(Interface::Shell, slot),
                [loader::OBJECT_BASE, cls, 0, 0],
                1000,
            )
            .unwrap()
        {
            Outcome::Returned { code } => code,
            o => panic!("{o:?}"),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let p = self.root.canonicalize().unwrap();
        assert!(
            p.starts_with(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("local-tests")
                    .canonicalize()
                    .unwrap()
            )
        );
        std::fs::remove_dir_all(p).unwrap();
    }
}

#[test]
fn app_history_uses_primary_slots_and_starts_at_the_real_current_class() {
    let mut f = Fixture::new();
    let app = loader::HEAP_BASE + 0x10000;
    let table = app + 0x100;
    let handler = loader::MODULE_BASE + 0x100;
    f.m.cpu_mut().write_u32(app, table).unwrap();
    f.m.cpu_mut().write_u32(table + 8, handler).unwrap();
    f.m.cpu_mut()
        .write_mem(
            handler,
            &[0xe3a00001u32, 0xe12fff1e]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    assert_eq!(
        f.m.start_applet(app, 0x01001234, 1000).unwrap(),
        Outcome::Returned { code: 1 }
    );
    let out = app + 0x200;
    let call =
        |m: &mut Machine<BackendPadrao>, at, args| match m.call_guest(at, args, 1000).unwrap() {
            Outcome::Returned { code } => code,
            o => panic!("{o:?}"),
        };
    assert_eq!(
        call(
            &mut f.m,
            aee::encode(Interface::Shell, 2),
            [loader::OBJECT_BASE, 0x0100104f, out, 0]
        ),
        0
    );
    let obj = f.m.cpu().read_u32(out).unwrap();
    let vt = f.m.cpu().read_u32(obj).unwrap();
    let get = f.m.cpu().read_u32(vt + 7 * 4).unwrap();
    assert_eq!(
        call(&mut f.m, get, [obj, out + 4, 0, 0]),
        0,
        "GetClass is read-only slot7, not empty generic Collection"
    );
    assert_eq!(f.m.cpu().read_u32(out + 4).unwrap(), 0x01001234);
    assert_eq!(
        call(&mut f.m, get, [obj, 0xdead0000, 0, 0]),
        14,
        "unmapped output follows local SDK EBADPARM"
    );
    let query = f.m.cpu().read_u32(vt + 2 * 4).unwrap();
    assert_eq!(call(&mut f.m, query, [obj, 0x010396bf, 0xdead0000, 0]), 14);
    let back = f.m.cpu().read_u32(vt + 4 * 4).unwrap();
    assert_eq!(
        call(&mut f.m, back, [obj, 0, 0, 0]),
        39,
        "Back reports ENOSUCH at bottom"
    );
    let current = f.m.cpu().read_u32(vt + 11 * 4).unwrap();
    assert_eq!(call(&mut f.m, current, [obj, out + 8, 0, 0]), 0);
    assert_eq!(f.m.cpu().read_u32(out + 8).unwrap(), 0);
    assert_eq!(
        call(&mut f.m, get, [obj, 0, 0, 0]),
        14,
        "null output follows SDK EBADPARM"
    );
    let insert = f.m.cpu().read_u32(vt + 6 * 4).unwrap();
    assert_eq!(
        call(&mut f.m, insert, [obj, 0x01005678, 0, 0]),
        20,
        "unimplemented mutation must not claim success"
    );
    assert_eq!(call(&mut f.m, get, [obj, out + 4, 0, 0]), 0);
    assert_eq!(f.m.cpu().read_u32(out + 4).unwrap(), 0x01001234);
}

#[test]
fn native_app_history_blocks_partial_machine_save_state() {
    let mut f = Fixture::new();
    let out = loader::HEAP_BASE + 0x10000;
    assert_eq!(
        f.m.call_guest(
            aee::encode(Interface::Shell, 2),
            [loader::OBJECT_BASE, 0x0100104f, out, 0],
            1000
        )
        .unwrap(),
        Outcome::Returned { code: 0 }
    );
    assert!(
        f.m.pode_salvar().is_err(),
        "AppHistory cursor/shared owner state must not be omitted from a successful snapshot"
    );
}
