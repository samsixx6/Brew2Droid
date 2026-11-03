//! O pacote original contém MIF opaco e executável gzip de instalação.
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use zeebx::{
    handset::{input::Key, launch, types::DisplayGeometry},
    session::{Session, Step},
    storage::StoragePaths,
};
static NEXT: AtomicU64 = AtomicU64::new(0);
const CLASS: u32 = 0x0106cbd5;
struct Fixture {
    root: PathBuf,
    module: PathBuf,
    mif: PathBuf,
    storage: StoragePaths,
}
impl Fixture {
    fn new() -> Self {
        let source = PathBuf::from(
            std::env::var_os("BREW_MGS_MODULE")
                .expect("BREW_MGS_MODULE aponta para o pacote original"),
        );
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests/wave22-work-20261101/runtime-tests")
            .join(format!(
                "mgs-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&root).unwrap();
        let module = root.join("renamed.mod");
        let mif = root.join("renamed.mif");
        std::fs::copy(&source, &module).unwrap();
        std::fs::copy(
            source.parent().unwrap().parent().unwrap().join("mgs.mif"),
            &mif,
        )
        .unwrap();
        for name in ["mgs.s3e", "all.dz"] {
            std::fs::copy(source.parent().unwrap().join(name), root.join(name)).unwrap();
        }
        let storage = StoragePaths::from_root(root.join("runtime"));
        storage.create_dirs().unwrap();
        Self {
            root,
            module,
            mif,
            storage,
        }
    }
    fn overlay(&self) -> PathBuf {
        self.storage
            .save_for(&self.storage.content_id(&self.module).unwrap())
    }
    fn start(&self) -> Result<Session, zeebx::session::StartError> {
        Session::start_handset_with_storage(
            &self.module,
            DisplayGeometry {
                width: 240,
                height: 320,
                depth: 16,
            },
            &self.storage,
            None,
        )
    }
    fn raw(&self) -> Vec<u8> {
        std::fs::read(self.root.join("mgs.s3e")).unwrap()
    }
    fn expanded(&self) {
        let data =
            std::fs::read(self.overlay().join("mgs.s3e")).expect("instalacao precede o guest");
        assert_eq!(data.len(), 998472);
        assert_eq!(&data[..4], b"XE3U");
    }
}
#[test]
#[ignore = "requer pacote original em BREW_MGS_MODULE"]
fn exact_opaque_pair_keeps_original_metadata_and_catalog_class() {
    let f = Fixture::new();
    let pkg = launch::read(&f.module)
        .expect("par original auditado identifica sua classe sem fabricar MIF");
    assert_eq!(pkg.applet_class, CLASS);
    assert_eq!(pkg.manifest.unwrap().bytes, std::fs::read(&f.mif).unwrap());
    assert_eq!(zeebx::library::applet_clsid(&f.module), Some(CLASS));
    let game = zeebx::library::scan(&f.root)
        .into_iter()
        .find(|g| g.path == f.module)
        .unwrap();
    assert_eq!(game.clsid, Some(CLASS));
}
#[test]
#[ignore = "requer pacote original em BREW_MGS_MODULE"]
fn normal_session_installs_verified_s3e_before_guest_without_changing_source() {
    let f = Fixture::new();
    let before = f.raw();
    let mif = std::fs::read(&f.mif).unwrap();
    let _session = f.start().expect("Session normal abre o pacote original");
    f.expanded();
    assert_eq!(f.raw(), before);
    assert_eq!(std::fs::read(&f.mif).unwrap(), mif);
}
#[test]
#[ignore = "requer pacote original em BREW_MGS_MODULE"]
fn changed_module_or_opaque_metadata_never_selects_audited_class() {
    for module in [true, false] {
        let f = Fixture::new();
        let path = if module { &f.module } else { &f.mif };
        let mut data = std::fs::read(path).unwrap();
        let last = data.len() - 1;
        data[last] ^= 1;
        std::fs::write(path, data).unwrap();
        assert!(launch::read(&f.module).is_err());
        assert!(!f.overlay().join("mgs.s3e").exists());
    }
}
#[test]
#[ignore = "requer pacote original em BREW_MGS_MODULE"]
fn absent_manifest_does_not_borrow_the_opaque_pair_exception() {
    let f = Fixture::new();
    std::fs::remove_file(&f.mif).unwrap();
    assert!(launch::read(&f.module).is_err());
}
#[test]
#[ignore = "requer pacote original em BREW_MGS_MODULE"]
fn damaged_compressed_executable_is_rejected_before_installation() {
    let f = Fixture::new();
    let mut bytes = f.raw();
    bytes.truncate(bytes.len() - 5);
    std::fs::write(f.root.join("mgs.s3e"), bytes).unwrap();
    assert!(f.start().is_err());
    assert!(!f.overlay().join("mgs.s3e").exists());
}
#[test]
#[ignore = "requer pacote original em BREW_MGS_MODULE"]
fn old_byte_identical_compressed_overlay_is_upgraded_and_modified_overlay_is_preserved() {
    let f = Fixture::new();
    let overlay = f.overlay();
    std::fs::create_dir_all(&overlay).unwrap();
    std::fs::write(overlay.join("mgs.s3e"), f.raw()).unwrap();
    drop(f.start().expect("seed gzip antigo e reconhecido por hash"));
    f.expanded();
    let custom = b"user supplied executable data";
    std::fs::write(overlay.join("mgs.s3e"), custom).unwrap();
    drop(
        f.start()
            .expect("dados do usuario permanecem a cargo do loader"),
    );
    assert_eq!(std::fs::read(overlay.join("mgs.s3e")).unwrap(), custom);
}
#[test]
#[ignore = "requer pacote original em BREW_MGS_MODULE; replay de 20 segundos"]
fn original_package_executes_loaded_game_code_in_ordinary_session() {
    let f = Fixture::new();
    let mut session = f.start().expect("caminho publico de Session");
    session.set_tracing(true);
    let wall = Instant::now();
    let mut previous = wall;
    let mut next = 2000;
    let mut down = false;
    while wall.elapsed() < Duration::from_secs(20) {
        let now = Instant::now();
        if wall.elapsed().as_millis() >= next {
            down = !down;
            session.set_handset_key(Key::Select, down).unwrap();
            next += if down { 100 } else { 1900 };
        }
        assert_ne!(
            session.step(now.duration_since(previous), true),
            Step::Stopped
        );
        previous = now;
        while session.mostra_quadro_intermediario() {}
        assert!(!session.saiu_sozinho());
        std::thread::sleep(Duration::from_millis(2));
    }
    // Os imports do S3E passam pelo loader: o LR do trampolim continua no MOD.
    // O replay permanece vivo sob entrada e avança além do diálogo fatal.
    f.expanded();
    assert!(session.screen().escritas() > 500000);
    assert!(session.stopped_reason().is_none());
}
