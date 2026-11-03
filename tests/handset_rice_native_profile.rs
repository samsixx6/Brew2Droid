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
    mif: PathBuf,
    profiles: PathBuf,
}
impl Fixture {
    fn copy_original() -> Self {
        let source = PathBuf::from(
            std::env::var_os("BREW_RICE_NATIVE_MOD")
                .expect("set BREW_RICE_NATIVE_MOD to the original yongman2.mod"),
        );
        let selected = zeebx::library::resolve_mif(&source).unwrap();
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests")
            .join(format!(
                "rice-native-profile-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&root).unwrap();
        let module = root.join("yongman2.mod");
        let mif = root.join("yongman2.mif");
        std::fs::copy(source, &module).unwrap();
        std::fs::copy(selected, &mif).unwrap();
        Self {
            profiles: root.join("preferences"),
            root,
            module,
            mif,
        }
    }
    fn native() -> DisplayGeometry {
        DisplayGeometry {
            width: 120,
            height: 146,
            depth: 16,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
#[test]
#[ignore = "requer o par original em BREW_RICE_NATIVE_MOD"]
fn exact_original_pair_selects_compatible_full_drawable_viewport() {
    let f = Fixture::copy_original();
    let selected = display::resolve(&f.module, &f.profiles).unwrap();
    assert_eq!(
        selected.module_sha256,
        "6c655333625da5b0f0ef78bff6ec23533572fb69f695afa3c756b885bef8577d"
    );
    assert_eq!(selected.geometry, Fixture::native());
    assert_eq!(selected.source, Source::Known);
}
#[test]
#[ignore = "requer o par original em BREW_RICE_NATIVE_MOD"]
fn wrong_manifest_or_module_cannot_select_the_known_viewport() {
    for change_mif in [true, false] {
        let f = Fixture::copy_original();
        let path = if change_mif { &f.mif } else { &f.module };
        let mut bytes = std::fs::read(path).unwrap();
        bytes[0] ^= 1;
        std::fs::write(path, bytes).unwrap();
        let selected = display::resolve(&f.module, &f.profiles).unwrap();
        assert_eq!(selected.source, Source::Fallback);
        assert_ne!(selected.geometry, Fixture::native());
    }
}
#[test]
#[ignore = "requer o par original em BREW_RICE_NATIVE_MOD"]
fn explicit_manual_geometry_wins_and_reset_returns_to_native_auto() {
    let f = Fixture::copy_original();
    let manual = DisplayGeometry {
        width: 320,
        height: 240,
        depth: 16,
    };
    let chosen = display::set_override(&f.module, &f.profiles, Some(manual)).unwrap();
    assert_eq!(chosen.geometry, manual);
    assert_eq!(chosen.source, Source::Manual);
    assert_eq!(chosen.automatic.geometry, Fixture::native());
    let restored = display::set_override(&f.module, &f.profiles, None).unwrap();
    assert_eq!(restored.geometry, Fixture::native());
    assert_eq!(restored.source, Source::Known);
}
