use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use zeebx::handset::{
    display::{self, Source},
    types::DisplayGeometry,
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: PathBuf,
    module: PathBuf,
}
impl Fixture {
    fn original() -> Self {
        let source = PathBuf::from(
            std::env::var_os("BREW_BEJEWELED_MODULE")
                .expect("BREW_BEJEWELED_MODULE aponta para o pacote original"),
        );
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests")
            .join(format!(
                "bejeweled-geometry-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&root).unwrap();
        let module = root.join("renamed.mod");
        std::fs::copy(&source, &module).unwrap();
        let mif = source
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("bejeweled.mif");
        std::fs::copy(mif, module.with_extension("mif")).unwrap();
        std::fs::copy(
            source.with_file_name("bejeweled.rlb"),
            root.join("bejeweled.rlb"),
        )
        .unwrap();
        Self { root, module }
    }
    fn resolve(&self) -> display::Selection {
        display::resolve(&self.module, &self.root.join("profiles")).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn geometry(width: u16, height: u16) -> DisplayGeometry {
    DisplayGeometry {
        width,
        height,
        depth: 16,
    }
}

#[test]
#[ignore = "requer pacote Bejeweled original em BREW_BEJEWELED_MODULE"]
fn audited_bejeweled_auto_matches_the_compiled_screen_and_full_board() {
    let f = Fixture::original();
    let result = f.resolve();
    assert_eq!(result.geometry, geometry(320, 480));
    assert_eq!(result.source, Source::Known);
}

#[test]
#[ignore = "requer pacote Bejeweled original em BREW_BEJEWELED_MODULE"]
fn audited_bejeweled_requires_the_selected_manifest_and_original_resource() {
    let f = Fixture::original();
    let resource = f.root.join("bejeweled.rlb");
    let original = std::fs::read(&resource).unwrap();
    std::fs::remove_file(&resource).unwrap();
    assert_eq!(f.resolve().source, Source::Fallback);
    let mut changed = original.clone();
    changed[100] ^= 1;
    std::fs::write(&resource, changed).unwrap();
    assert_eq!(f.resolve().source, Source::Fallback);
    std::fs::write(resource, original).unwrap();
    let mif = f.module.with_extension("mif");
    let mut bytes = std::fs::read(&mif).unwrap();
    bytes.push(0);
    std::fs::write(mif, bytes).unwrap();
    assert_eq!(f.resolve().source, Source::Fallback);
}

#[test]
#[ignore = "requer pacote Bejeweled original em BREW_BEJEWELED_MODULE"]
fn audited_bejeweled_never_matches_modified_executable_bytes() {
    let f = Fixture::original();
    let mut bytes = std::fs::read(&f.module).unwrap();
    bytes.push(0);
    std::fs::write(&f.module, bytes).unwrap();
    assert_eq!(f.resolve().source, Source::Fallback);
}

#[test]
#[ignore = "requer pacote Bejeweled original em BREW_BEJEWELED_MODULE"]
fn audited_bejeweled_keeps_manual_selection_until_reset() {
    let f = Fixture::original();
    let profiles = f.root.join("profiles");
    display::set_override(&f.module, &profiles, Some(geometry(176, 220))).unwrap();
    assert_eq!(f.resolve().geometry, geometry(176, 220));
    assert_eq!(f.resolve().source, Source::Manual);
    display::set_override(&f.module, &profiles, None).unwrap();
    assert_eq!(f.resolve().geometry, geometry(320, 480));
    assert_eq!(f.resolve().source, Source::Known);
}
