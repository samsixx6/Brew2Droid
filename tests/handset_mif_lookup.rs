use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: PathBuf,
    module: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests")
            .join(format!(
                "mif-lookup-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        let module = root.join("games/Mod/more games/39367/game.mod");
        std::fs::create_dir_all(module.parent().unwrap()).unwrap();
        std::fs::write(&module, [0; 8]).unwrap();
        Self { root, module }
    }
    fn mif(&self, path: &str, id: u32) -> PathBuf {
        let path = self.root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut bytes = vec![0u8; 60];
        bytes[..4].copy_from_slice(&[0x11, 0, 1, 0]);
        for (at, value) in [(16, 32u32), (20, 1), (32, 40), (36, 60), (40, id), (48, 1)] {
            bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        std::fs::write(&path, bytes).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
#[test]
fn nested_shared_mif_matches_module_filename() {
    let f = Fixture::new();
    let mif = f.mif("games/Mif/more mifs/game.mif", 0x1234);
    let before = std::fs::read(&mif).unwrap();
    assert_eq!(zeebx::library::applet_clsid(&f.module), Some(0x1234));
    assert_eq!(std::fs::read(mif).unwrap(), before);
    assert!(!f.module.with_extension("mif").exists());
}
#[test]
fn nested_shared_mif_matches_numeric_game_folder() {
    let f = Fixture::new();
    f.mif("games/Mif/more mifs/39367.mif", 0x4567);
    assert_eq!(zeebx::library::applet_clsid(&f.module), Some(0x4567));
}
#[test]
fn matching_names_and_extensions_ignore_case() {
    let f = Fixture::new();
    f.mif("games/Mif/deeper/GAME.MIF", 0x1234);
    assert_eq!(zeebx::library::applet_clsid(&f.module), Some(0x1234));
}
#[test]
fn local_match_takes_precedence_over_shared_match() {
    let f = Fixture::new();
    f.mif("games/Mod/more games/39367/game.mif", 0x1234);
    f.mif("games/Mif/game.mif", 0x5678);
    assert_eq!(zeebx::library::applet_clsid(&f.module), Some(0x1234));
}
#[test]
fn unrelated_shared_manifest_is_not_selected() {
    let f = Fixture::new();
    f.mif("games/Mif/other.mif", 0x5678);
    assert_eq!(zeebx::library::applet_clsid(&f.module), None);
}
#[test]
fn identical_shared_copies_are_one_match() {
    let f = Fixture::new();
    f.mif("games/Mif/one/game.mif", 0x1234);
    f.mif("games/Mif/two/game.mif", 0x1234);
    assert_eq!(zeebx::library::applet_clsid(&f.module), Some(0x1234));
}
#[test]
fn different_shared_matches_are_ambiguous() {
    let f = Fixture::new();
    f.mif("games/Mif/one/game.mif", 0x1234);
    f.mif("games/Mif/two/game.mif", 0x5678);
    assert_eq!(zeebx::library::applet_clsid(&f.module), None);
}
#[test]
fn library_scan_uses_same_nested_match() {
    let f = Fixture::new();
    f.mif("games/Mif/more mifs/39367.mif", 0x4567);
    let games = zeebx::library::scan(&f.root.join("games"));
    assert_eq!(games.len(), 1);
    assert_eq!(games[0].clsid, Some(0x4567));
}
#[test]
fn launch_inspection_reads_the_resolved_shared_mif() {
    let f = Fixture::new();
    let expected = f.mif("games/Mif/more mifs/39367.mif", 0x4567);
    std::fs::write(&f.module, [0x1e, 0xff, 0x2f, 0xe1, 0, 0, 0, 0]).unwrap();
    let path = zeebx::library::resolve_mif(&f.module).unwrap();
    assert_eq!(path, expected);
    let package = zeebx::handset::package::read_package(&f.module, &path).unwrap();
    assert_eq!(package.report.applet_clsid, Some(0x4567));
    assert_eq!(package.report.source_mif, path.canonicalize().unwrap());
}
#[test]
fn local_only_mif_matches_uppercase_extension() {
    let f = Fixture::new();
    f.mif("games/Mod/more games/39367/GAME.MIF", 0x1234);
    assert_eq!(zeebx::library::applet_clsid(&f.module), Some(0x1234));
    assert_eq!(
        zeebx::library::scan(&f.root.join("games"))[0].clsid,
        Some(0x1234)
    );
}
#[test]
fn local_only_mif_prefers_matching_folder_over_unrelated_manifest() {
    let f = Fixture::new();
    f.mif("games/Mod/more games/39367/000-unrelated.mif", 0x5555);
    f.mif("games/Mod/more games/39367/39367.mif", 0x1234);
    assert_eq!(zeebx::library::applet_clsid(&f.module), Some(0x1234));
}
#[test]
fn nested_mod_folder_keeps_outer_shared_mifs() {
    let f = Fixture::new();
    let module = f.module.parent().unwrap().join("Mod/game.mod");
    std::fs::create_dir_all(module.parent().unwrap()).unwrap();
    std::fs::write(&module, [0u8; 8]).unwrap();
    let expected = f.mif("games/Mif/more mifs/game.mif", 0x1234);
    assert_eq!(zeebx::library::resolve_mif(&module).unwrap(), expected);
}
#[test]
fn legacy_metadata_skips_an_extension_manifest_to_find_the_applet() {
    let f = Fixture::new();
    let ext = f.mif("games/Mod/more games/39367/game.mif", 0x1111);
    let mut bytes = std::fs::read(&ext).unwrap();
    bytes[36..40].copy_from_slice(&48u32.to_le_bytes());
    bytes.truncate(48);
    std::fs::write(ext, bytes).unwrap();
    f.mif("games/Mod/more games/39367/39367.mif", 0x1234);
    assert_eq!(
        zeebx::library::scan(&f.root.join("games"))[0].clsid,
        Some(0x1234)
    );
}
#[test]
fn a_truncated_shared_scan_cannot_choose_an_unchecked_duplicate() {
    let f = Fixture::new();
    f.mif("games/Mif/000/game.mif", 0x1234);
    for n in 0..4000 {
        f.mif(&format!("games/Mif/other/{n:04}.mif"), 0x3333);
    }
    f.mif("games/Mif/zzz/game.mif", 0x5555);
    assert!(
        zeebx::library::resolve_mif(&f.module)
            .unwrap_err()
            .contains("limit")
    );
    let local = f.mif("games/Mod/more games/39367/game.mif", 0x1234);
    assert_eq!(zeebx::library::resolve_mif(&f.module).unwrap(), local);
}
#[test]
fn launch_pair_inspection_does_not_hash_unrelated_collection_data() {
    let f = Fixture::new();
    let mif = f.mif("games/Mif/39367.mif", 0x1234);
    std::fs::write(&f.module, [0x1e, 0xff, 0x2f, 0xe1, 0, 0, 0, 0]).unwrap();
    std::fs::File::create(f.module.with_extension("unrelated-data"))
        .unwrap()
        .set_len(33 * 1024 * 1024)
        .unwrap();
    assert!(zeebx::handset::package::read_package(&f.module, &mif).is_err());
    let pair = zeebx::handset::package::read_module_pair(&f.module, &mif).unwrap();
    assert_eq!(pair.report.applet_clsid, Some(0x1234));
    assert!(pair.report.resources.is_empty());
}

#[test]
fn organized_parent_manifest_matches_inner_folder_when_module_name_differs() {
    let f = Fixture::new();
    let module = f.root.join("Games/Asphalt Urban GT 2 (240x320) 1.0.5/12345/cryptic.mod");
    std::fs::create_dir_all(module.parent().unwrap()).unwrap();
    std::fs::write(&module, [0u8; 8]).unwrap();
    let expected = f.mif("Games/Asphalt Urban GT 2 (240x320) 1.0.5/12345.MIF", 0x1234);
    assert_eq!(zeebx::library::resolve_mif(&module).unwrap(), expected);
    assert_eq!(zeebx::library::scan(&f.root.join("Games"))[0].clsid, Some(0x1234));
}

#[test]
fn local_manifest_wins_over_a_matching_parent_manifest() {
    let f = Fixture::new();
    let expected = f.mif("games/Mod/more games/39367/game.mif", 0x1234);
    f.mif("games/Mod/more games/39367.mif", 0x5678);
    assert_eq!(zeebx::library::resolve_mif(&f.module).unwrap(), expected);
}

#[test]
fn parent_manifest_must_match_inner_folder_not_a_generic_module_name() {
    let f = Fixture::new();
    f.mif("games/Mod/more games/game.mif", 0x5678);
    assert!(zeebx::library::resolve_mif(&f.module).is_err());
}

#[test]
fn collection_scan_keeps_per_package_mod_mif_layouts_without_a_collection_mif_folder() {
    let f = Fixture::new();
    let module = f.root.join("Games/Proper Name/Mod/12345/game.mod");
    std::fs::create_dir_all(module.parent().unwrap()).unwrap();
    std::fs::write(&module,[0u8;8]).unwrap();
    f.mif("Games/Proper Name/Mif/12345.mif",0x4567);
    let games = zeebx::library::scan(&f.root.join("Games"));
    let game = games.iter().find(|game| game.path == module).unwrap();
    assert_eq!(game.clsid,Some(0x4567));
}
