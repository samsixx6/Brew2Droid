use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use zeebx::{
    handset::types::DisplayGeometry,
    session::{Session, Step},
    storage::StoragePaths,
};
static NEXT: AtomicU64 = AtomicU64::new(0);
const SBS: u32 = 0x0102cc31;
struct Fixture {
    root: PathBuf,
    module: PathBuf,
}
impl Fixture {
    fn ssa() -> Self {
        let source = PathBuf::from(
            std::env::var_os("BREW_SSA_MODULE")
                .expect("BREW_SSA_MODULE aponta para o MOD original"),
        );
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests")
            .join(format!(
                "audited-ssa-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&root).unwrap();
        let module = root.join("renamed.mod");
        std::fs::copy(&source, &module).unwrap();
        for name in ["SSA.BAR", "SSA.SIG", "SAVE.TXT"] {
            std::fs::copy(source.parent().unwrap().join(name), root.join(name)).unwrap();
        }
        assert_eq!(std::fs::metadata(&module).unwrap().len(), 127660);
        Self { root, module }
    }
    fn original() -> Self {
        let source = PathBuf::from(
            std::env::var_os("BREW_SBS_MODULE")
                .expect("BREW_SBS_MODULE aponta para o MOD original"),
        );
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests")
            .join(format!(
                "audited-manifestless-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&root).unwrap();
        let module = root.join("renamed.mod");
        std::fs::copy(&source, &module).unwrap();
        for name in ["portriss.bar", "portriss.cfg", "portriss.dat"] {
            std::fs::copy(source.parent().unwrap().join(name), root.join(name)).unwrap();
        }
        assert_eq!(std::fs::metadata(&module).unwrap().len(), 137956);
        Self { root, module }
    }
    fn session(&self) -> Result<Session, zeebx::session::StartError> {
        let storage = StoragePaths::from_root(self.root.join("runtime"));
        storage.create_dirs().unwrap();
        Session::start_handset_with_storage(
            &self.module,
            DisplayGeometry {
                width: 240,
                height: 320,
                depth: 16,
            },
            &storage,
            None,
        )
    }
    fn mif(&self, clsid: u32) -> Vec<u8> {
        let mut bytes = vec![0u8; 60];
        bytes[..4].copy_from_slice(&[0x11, 0, 1, 0]);
        for (at, value) in [
            (16, 32u32),
            (20, 1),
            (32, 40),
            (36, 60),
            (40, clsid),
            (48, 1),
        ] {
            bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }
}

#[test]
#[ignore = "requer pacote SSA original em BREW_SSA_MODULE"]
fn audited_ssa_executable_opens_its_original_image_resources_without_a_manifest() {
    let f = Fixture::ssa();
    let before = std::fs::read(&f.module).unwrap();
    assert_eq!(zeebx::library::applet_clsid(&f.module), Some(0x01013be4));
    let mut session = f
        .session()
        .expect("a classe SSA comprovada no executavel abre sem MIF");
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut visible = false;
    while Instant::now() < deadline {
        assert_ne!(session.step(Duration::from_millis(16), true), Step::Stopped);
        while session.mostra_quadro_intermediario() {}
        let pixels = session.screen().pixels();
        if pixels.iter().any(|p| *p != pixels[0]) {
            visible = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(visible);
    drop(session);
    assert_eq!(std::fs::read(&f.module).unwrap(), before);
    assert!(!f.module.with_extension("mif").exists());
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
#[ignore = "requer pacote SBS original em BREW_SBS_MODULE"]
fn audited_executable_class_starts_without_creating_a_manifest() {
    let f = Fixture::original();
    let before = std::fs::read(&f.module).unwrap();
    assert_eq!(zeebx::library::applet_clsid(&f.module), Some(SBS));
    let mut session = f
        .session()
        .expect("o applet cujo ClassID foi comprovado no executavel abre sem MIF");
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut visible = false;
    while Instant::now() < deadline {
        assert_ne!(session.step(Duration::from_millis(16), true), Step::Stopped);
        while session.mostra_quadro_intermediario() {}
        let pixels = session.screen().pixels();
        if pixels.iter().any(|p| *p != pixels[0]) {
            visible = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        visible,
        "os recursos originais devem produzir imagem, nao apenas uma fabrica vazia"
    );
    drop(session);
    assert_eq!(std::fs::read(&f.module).unwrap(), before);
    assert!(!f.module.with_extension("mif").exists());
}

#[test]
#[ignore = "requer pacote SBS original em BREW_SBS_MODULE"]
fn unrelated_manifest_cannot_replace_the_audited_executable_class() {
    let f = Fixture::original();
    std::fs::write(f.root.join("unrelated.mif"), f.mif(0x12345678)).unwrap();
    assert_eq!(zeebx::library::applet_clsid(&f.module), Some(SBS));
    let games = zeebx::library::scan(&f.root);
    assert_eq!(
        games.iter().find(|g| g.path == f.module).unwrap().clsid,
        Some(SBS)
    );
    assert!(f.session().is_ok());
}

#[test]
#[ignore = "requer pacote SBS original em BREW_SBS_MODULE"]
fn edited_executable_cannot_reuse_the_missing_manifest_exception() {
    let f = Fixture::original();
    let mut bytes = std::fs::read(&f.module).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    std::fs::write(&f.module, bytes).unwrap();
    assert_eq!(zeebx::library::applet_clsid(&f.module), None);
    assert!(f.session().is_err());
}

#[test]
#[ignore = "requer pacote SBS original em BREW_SBS_MODULE"]
fn matching_malformed_manifest_remains_an_error_instead_of_being_bypassed() {
    let f = Fixture::original();
    std::fs::write(f.module.with_extension("mif"), b"not a MIF").unwrap();
    assert!(f.session().is_err());
}

#[test]
#[ignore = "requer pacote SBS original em BREW_SBS_MODULE"]
fn matching_real_manifest_keeps_its_declared_class_and_factory_rejection() {
    let f = Fixture::original();
    std::fs::write(f.module.with_extension("mif"), f.mif(0x12345678)).unwrap();
    assert_eq!(zeebx::library::applet_clsid(&f.module), Some(0x12345678));
    assert!(f.session().is_err());
}

#[test]
#[ignore = "requer pacote SBS original em BREW_SBS_MODULE"]
fn ambiguous_shared_manifests_cannot_inherit_the_executable_exception() {
    let f = Fixture::original();
    for (directory, clsid) in [("a", 0x12345678), ("b", 0x23456789)] {
        let parent = f.root.join("mif").join(directory);
        std::fs::create_dir_all(&parent).unwrap();
        std::fs::write(parent.join("renamed.mif"), f.mif(clsid)).unwrap();
    }
    assert_eq!(zeebx::library::applet_clsid(&f.module), None);
    let games = zeebx::library::scan(&f.root);
    assert_eq!(
        games.iter().find(|g| g.path == f.module).unwrap().clsid,
        None
    );
    assert!(f.session().is_err());
}

#[test]
#[ignore = "requer pacote SBS original em BREW_SBS_MODULE"]
fn nested_matching_manifest_wins_when_scan_root_has_no_shared_index() {
    let mut f = Fixture::original();
    let nested = f.root.join("Game");
    std::fs::create_dir_all(nested.join("Mif")).unwrap();
    let module = nested.join("renamed.mod");
    std::fs::rename(&f.module, &module).unwrap();
    f.module = module;
    std::fs::write(nested.join("Mif/renamed.mif"), f.mif(0x12345678)).unwrap();
    let index = zeebx::handset::mif_lookup::Index::for_root(&f.root);
    assert!(
        index.find(&f.module).unwrap().is_none(),
        "scan root index misses nested Mif"
    );
    assert_eq!(zeebx::library::applet_clsid(&f.module), Some(0x12345678));
    let games = zeebx::library::scan(&f.root);
    assert_eq!(
        games.iter().find(|g| g.path == f.module).unwrap().clsid,
        Some(0x12345678)
    );
}
#[test]
#[ignore = "requer pacote SBS original em BREW_SBS_MODULE"]
fn nested_ambiguous_manifest_cannot_inherit_executable_class() {
    let mut f = Fixture::original();
    let nested = f.root.join("Game");
    std::fs::create_dir_all(&nested).unwrap();
    let module = nested.join("renamed.mod");
    std::fs::rename(&f.module, &module).unwrap();
    f.module = module;
    for (dir, class) in [("a", 0x12345678), ("b", 0x23456789)] {
        let folder = nested.join("Mif").join(dir);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("renamed.mif"), f.mif(class)).unwrap();
    }
    let index = zeebx::handset::mif_lookup::Index::for_root(&f.root);
    assert!(
        index.find(&f.module).unwrap().is_none(),
        "scan root index misses nested Mif"
    );
    assert_eq!(zeebx::library::applet_clsid(&f.module), None);
    let games = zeebx::library::scan(&f.root);
    assert_eq!(
        games.iter().find(|g| g.path == f.module).unwrap().clsid,
        None
    );
}
