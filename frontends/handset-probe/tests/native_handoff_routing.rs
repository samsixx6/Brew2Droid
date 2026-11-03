use std::{path::PathBuf, time::Duration};
use zeebx::{
    brew::aee::{self, Interface},
    handset::{
        installed::{InstalledApplet, InstalledCatalog},
        types::DisplayGeometry,
    },
    loader,
    session::{Session as Core, handset_runtime::HandsetRuntime},
    storage::StoragePaths,
    velocidade::Ritmo,
};
#[path = "../../android/src/native_session.rs"]
mod native_session;
#[test]
fn android_same_top_route_notifies_existing_owner_without_package_or_restart() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("local-tests")
        .join(format!("native-handoff-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let module = dir.join("self.mod");
    let mif = dir.join("self.mif");
    let class = 0x01001234;
    let mut bytes = vec![0u8; 512];
    let startslot = (0..256)
        .find(|s| Interface::Shell.method(*s) == Some("StartApplet"))
        .unwrap();
    let words = [
        (
            0,
            vec![
                0xe59f0008,
                0xe5820000,
                0xe3a00000,
                0xe12fff1e,
                loader::MODULE_BASE + 256,
            ],
        ),
        (
            32,
            vec![
                0xe59f0008,
                0xe5830000,
                0xe3a00000,
                0xe12fff1e,
                loader::MODULE_BASE + 288,
            ],
        ),
        (64, vec![0xe3a00001, 0xe12fff1e]),
        (
            80,
            vec![
                0xe92d4010,
                0xe280c010,
                0xe79c2101,
                0xe2822001,
                0xe78c2101,
                0xe3510000,
                0x1a000003,
                0xe59f0010,
                0xe59f1010,
                0xe59fc010,
                0xe12fff3c,
                0xe3a00001,
                0xe8bd8010,
                loader::OBJECT_BASE,
                class,
                aee::encode(Interface::Shell, startslot),
            ],
        ),
        (
            256,
            vec![
                loader::MODULE_BASE + 260,
                loader::MODULE_BASE + 64,
                loader::MODULE_BASE + 64,
                loader::MODULE_BASE + 32,
            ],
        ),
        (
            288,
            vec![
                loader::MODULE_BASE + 292,
                loader::MODULE_BASE + 64,
                loader::MODULE_BASE + 64,
                loader::MODULE_BASE + 80,
            ],
        ),
    ];
    for (at, words) in words {
        for (i, word) in words.into_iter().enumerate() {
            bytes[at + i * 4..at + i * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
    }
    std::fs::write(&module, bytes).unwrap();
    let mut manifest = vec![0u8; 60];
    manifest[..4].copy_from_slice(&[17, 0, 1, 0]);
    for (at, value) in [
        (16, 32u32),
        (20, 1),
        (32, 40),
        (36, 60),
        (40, class),
        (48, 1),
    ] {
        manifest[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    std::fs::write(&mif, manifest).unwrap();
    let app = InstalledApplet::inspect(&module, &mif).unwrap();
    let catalog = InstalledCatalog::new([app.clone()]);
    let storage = StoragePaths::from_root(dir.join("storage"));
    let source = Core::start_handset_with_storage_installed(
        &module,
        DisplayGeometry {
            width: 240,
            height: 320,
            depth: 16,
        },
        &storage,
        None,
        &catalog.shell_entries(),
    )
    .unwrap();
    let mut host =
        native_session::Session::Installed(HandsetRuntime::new(source, app, catalog).unwrap());
    host.anda(Duration::ZERO, &Ritmo::CONSOLE);
    assert!(host.installed().unwrap().pending_handoff().is_some());
    let before = host.memory();
    std::fs::remove_file(&module).unwrap();
    std::fs::remove_file(&mif).unwrap();
    assert_eq!(
        host.notify_top_launch(),
        Ok(true),
        "Android already-top route must not prepare/reload or read removed package files"
    );
    assert_eq!(host.memory(), before);
    assert_eq!(host.classe(), class);
    assert_eq!(host.installed().unwrap().retained_count(), 0);
    assert!(host.installed().unwrap().pending_handoff().is_none());
    let dump = host.grava_estado();
    let reader = zeebx::save_state::Leitor::abre(&dump).unwrap();
    let memory = reader.secao("mem.module").unwrap();
    let read = |offset| u32::from_le_bytes(memory[offset..offset + 4].try_into().unwrap());
    assert_eq!(
        read(zeebx::loader::MODULE_PREFIX as usize + 288 + 16),
        1,
        "START delivered once"
    );
    assert_eq!(
        read(zeebx::loader::MODULE_PREFIX as usize + 288 + 16 + 0x12 * 4),
        1,
        "real WOULD_START delivered to same guest object"
    );
    drop(host);
    let checked = dir.canonicalize().unwrap();
    assert!(
        checked.starts_with(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("local-tests")
                .canonicalize()
                .unwrap()
        )
    );
    std::fs::remove_dir_all(checked).unwrap();
}

mod closed_relaunch {
    use super::native_session;
    use std::{
        path::PathBuf,
        time::{Duration, Instant},
    };
    use zeebx::{
        brew::aee::{self, Interface},
        handset::{
            installed::{InstalledApplet, InstalledCatalog},
            types::DisplayGeometry,
        },
        loader,
        session::{Session as Core, handset_runtime::HandsetRuntime},
        storage::StoragePaths,
        velocidade::Ritmo,
    };
    const A: u32 = 0x01001234;
    const B: u32 = 0x01005678;
    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let checked = self.0.canonicalize().unwrap();
            assert!(
                checked.starts_with(
                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("local-tests")
                        .canonicalize()
                        .unwrap()
                )
            );
            std::fs::remove_dir_all(checked).unwrap();
        }
    }
    struct Arm {
        code: Vec<u32>,
        pool: Vec<(usize, u32)>,
    }
    impl Arm {
        fn new() -> Self {
            Self {
                code: vec![0xe92d4010],
                pool: Vec::new(),
            }
        }
        fn ldr(&mut self, reg: u32, value: u32) {
            let at = self.code.len();
            self.code.push(0xe59f0000 | (reg << 12));
            self.pool.push((at, value));
        }
        fn shell(&mut self, name: &str, args: [u32; 3]) {
            self.ldr(0, loader::OBJECT_BASE);
            for (i, v) in args.into_iter().enumerate() {
                self.ldr(i as u32 + 1, v);
            }
            let slot = (0..256)
                .find(|s| Interface::Shell.method(*s) == Some(name))
                .unwrap();
            self.ldr(12, aee::encode(Interface::Shell, slot));
            self.code.push(0xe12fff3c);
        }
        fn finish(mut self) -> Vec<u32> {
            self.code.extend([0xe3a00001, 0xe8bd8010]);
            for (at, value) in self.pool {
                let literal = self.code.len();
                assert!(literal >= at + 2);
                self.code[at] |= ((literal - at - 2) * 4) as u32;
                self.code.push(value);
            }
            self.code
        }
    }
    fn package(
        root: &std::path::Path,
        name: &str,
        class: u32,
        background: bool,
    ) -> InstalledApplet {
        package_target(root, name, class, background, B)
    }
    fn package_target(
        root: &std::path::Path,
        name: &str,
        class: u32,
        background: bool,
        requested_class: u32,
    ) -> InstalledApplet {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let module = dir.join("game.mod");
        let mif = dir.join("game.mif");
        let mut bytes = vec![0u8; 1024];
        let base = loader::MODULE_BASE;
        let mut handler = Arm::new();
        handler
            .code
            .extend([0xe280c010, 0xe79c2101, 0xe2822001, 0xe78c2101]);
        handler.code.push(0xe3510000);
        let branch = handler.code.len();
        handler.code.push(0x1a000000);
        handler.shell("CloseApplet", [0, 0, 0]);
        if background {
            handler.shell("StartApplet", [requested_class, 0, 0]);
            handler.shell("SetTimer", [30, base + 512, base + 800]);
        }
        let stop = handler.code.len();
        handler.code[branch] |= (stop - branch - 2) as u32;
        if background {
            handler.code.extend([0xe3510001, 0x03a02000, 0x05c32000]);
        }
        let handler = handler.finish();
        assert!(handler.len() * 4 < 384);
        let mut callback = Arm::new();
        callback.code.extend([0xe5901060, 0xe2811001, 0xe5801060]);
        callback.shell("StartApplet", [requested_class, 0, 0]);
        let callback = callback.finish();
        let sections = [
            (
                0,
                vec![0xe59f0008, 0xe5820000, 0xe3a00000, 0xe12fff1e, base + 768],
            ),
            (
                32,
                vec![0xe59f0008, 0xe5830000, 0xe3a00000, 0xe12fff1e, base + 800],
            ),
            (64, vec![0xe3a00001, 0xe12fff1e]),
            (128, handler),
            (512, callback),
            (768, vec![base + 772, base + 64, base + 64, base + 32]),
            (800, vec![base + 804, base + 64, base + 64, base + 128]),
        ];
        for (at, words) in sections {
            for (i, word) in words.into_iter().enumerate() {
                bytes[at + i * 4..at + i * 4 + 4].copy_from_slice(&word.to_le_bytes());
            }
        }
        std::fs::write(&module, bytes).unwrap();
        let mut manifest = vec![0u8; 60];
        manifest[..4].copy_from_slice(&[17, 0, 1, 0]);
        for (at, value) in [
            (16, 32u32),
            (20, 1),
            (32, 40),
            (36, 60),
            (40, class),
            (48, 1),
        ] {
            manifest[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        std::fs::write(&mif, manifest).unwrap();
        InstalledApplet::inspect(&module, &mif).unwrap()
    }
    fn session(app: &InstalledApplet, storage: &StoragePaths, catalog: &InstalledCatalog) -> Core {
        Core::start_handset_with_storage_installed(
            &app.module,
            DisplayGeometry {
                width: 240,
                height: 320,
                depth: 16,
            },
            storage,
            None,
            &catalog.shell_entries(),
        )
        .unwrap()
    }
    #[test]
    fn android_same_class_closed_owner_requires_fresh_preparation_instead_of_top_notification() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests")
            .join(format!("native-closed-relaunch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fixture = Fixture(dir);
        let source = package(&fixture.0, "source", A, true);
        let target = package(&fixture.0, "target", B, false);
        let catalog = InstalledCatalog::new([source.clone(), target.clone()]);
        let storage = StoragePaths::from_root(fixture.0.join("storage"));
        let mut prepared = Some(session(&target, &storage, &catalog));
        let core = session(&source, &storage, &catalog);
        let mut host = native_session::Session::Installed(
            HandsetRuntime::new(core, source, catalog.clone()).unwrap(),
        );
        let until = Instant::now() + Duration::from_secs(2);
        let mut initial = false;
        let mut observed = false;
        while Instant::now() < until {
            host.anda(Duration::ZERO, &Ritmo::CONSOLE);
            let pending = host
                .installed()
                .unwrap()
                .pending_handoff()
                .map(|r| (r.target.class, r.resume));
            if let Some((class, resume)) = pending {
                assert_eq!(class, B);
                if !initial {
                    host.installed_mut()
                        .unwrap()
                        .begin_handoff(prepared.take())
                        .unwrap();
                    initial = true;
                } else {
                    assert!(!resume, "the closed target is no longer a retained owner");
                    assert_eq!(host.classe(), B);
                    assert!(
                        host.stopped_reason().is_some(),
                        "ClassID equality does not imply a live top owner"
                    );
                    assert_eq!(host.installed().unwrap().retained_count(), 1);
                    assert_eq!(
                        host.notify_top_launch(),
                        Ok(false),
                        "actual Android wrapper must leave this accepted request for fresh target preparation"
                    );
                    assert!(
                        host.installed().unwrap().pending_handoff().is_some(),
                        "notification classification must preserve the accepted relaunch"
                    );
                    let fresh = session(&target, &storage, &catalog);
                    host.installed_mut()
                        .unwrap()
                        .begin_handoff(Some(fresh))
                        .unwrap();
                    assert!(
                        host.stopped_reason().is_none(),
                        "fresh prepared owner replaces the normally closed target"
                    );
                    assert_eq!(
                        host.installed().unwrap().retained_count(),
                        1,
                        "background source remains alive"
                    );
                    assert!(host.installed().unwrap().is_live_foreground(&target));
                    assert!(host.installed().unwrap().pending_handoff().is_none());
                    observed = true;
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            observed,
            "background source's genuine timer must request the target again after its normal close"
        );
    }
    #[test]
    fn android_background_self_activation_resumes_one_owner_without_fresh_session() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests")
            .join(format!("native-background-self-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fixture = Fixture(dir);
        let app = package_target(&fixture.0, "source", A, true, A);
        let catalog = InstalledCatalog::new([app.clone()]);
        let storage = StoragePaths::from_root(fixture.0.join("storage"));
        let core = session(&app, &storage, &catalog);
        let mut host = native_session::Session::Installed(
            HandsetRuntime::new(core, app.clone(), catalog.clone()).unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        while host.installed().unwrap().pending_handoff().is_none() && Instant::now() < deadline {
            host.anda(Duration::ZERO, &Ritmo::CONSOLE);
        }
        let request = host.installed().unwrap().pending_handoff().unwrap();
        assert!(request.resume);
        assert!(!host.installed().unwrap().is_live_foreground(&app));
        assert_eq!(host.notify_top_launch(), Ok(false));
        host.installed_mut().unwrap().begin_handoff(None).unwrap();
        assert_eq!(host.installed().unwrap().retained_count(), 0);
        assert!(host.installed().unwrap().is_live_foreground(&app));
        while Instant::now() < deadline {
            host.anda(Duration::ZERO, &Ritmo::CONSOLE);
            host.notify_top_launch().unwrap();
            let dump = host.grava_estado();
            let reader = zeebx::save_state::Leitor::abre(&dump).unwrap();
            let memory = reader.secao("mem.module").unwrap();
            let read = |offset| u32::from_le_bytes(memory[offset..offset + 4].try_into().unwrap());
            let data = loader::MODULE_PREFIX as usize + 800;
            assert_eq!(read(data + 16), 1);
            assert_eq!(read(data + 28), 1);
            if read(data + 96) == 1 {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("the original background timer must survive same-owner RESUME");
    }
}
