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
fn accepted_start_request_is_not_overwritten_by_another_start() {
    let mut f = Fixture::new();
    assert_eq!(f.start(0x01001234), 0);
    assert_eq!(
        f.start(0x01005678),
        26,
        "EALREADY while first request awaits host"
    );
    assert_eq!(f.m.take_launch_request(), Some(0x01001234));
}
#[test]
fn start_request_stays_private_until_guest_releases_control() {
    let mut f = Fixture::new();
    let slot = (0..256)
        .find(|s| Interface::Shell.method(*s) == Some("StartApplet"))
        .unwrap();
    let at = loader::STACK_BASE + 0x100;
    let code = [
        0xe92d4010u32,
        0xe59f0020,
        0xe59f1020,
        0xe59fc020,
        0xe12fff3c,
        0xe3a02801,
        0xe2522001,
        0x1afffffd,
        0xe8bd8010,
        0xe1a00000,
        0xe1a00000,
        loader::OBJECT_BASE,
        0x01001234,
        aee::encode(Interface::Shell, slot),
    ];
    f.m.cpu_mut()
        .write_mem(
            at,
            &code
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    assert_eq!(f.m.call_guest(at, [0; 4], 40).unwrap(), Outcome::Budget);
    assert!(!f.m.handset_event_ready());
    assert_eq!(
        f.m.take_launch_request(),
        None,
        "StartApplet is asynchronous until yield"
    );
    for _ in 0..10 {
        f.m.comeca_volta();
        let _ = f.m.advance(500000).unwrap();
        f.m.fecha_volta();
        if f.m.handset_event_ready() {
            break;
        }
    }
    assert!(f.m.handset_event_ready());
    assert_eq!(f.m.take_launch_request(), Some(0x01001234));
}
