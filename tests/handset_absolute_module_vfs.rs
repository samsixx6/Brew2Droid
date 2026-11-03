use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use zeebx::brew::vfs::{OpenIntent, Vfs};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> (Self, Vfs) {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "brew-absolute-module-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("mod/gobaksodor")).unwrap();
        std::fs::create_dir(root.join("overlay")).unwrap();
        std::fs::write(root.join("mod/gobaksodor/level.dat"), b"original").unwrap();
        let mut vfs = Vfs::new(root.join("mod/gobaksodor"));
        vfs.set_save_root(root.join("overlay"));
        vfs.set_device_root(root.join("device"));
        (Self(root), vfs)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn absolute_own_module_readwrite_copies_package_seed_to_overlay() {
    let (fixture, vfs) = Fixture::new();
    let original = fixture.0.join("mod/gobaksodor/level.dat");
    for intent in [OpenIntent::ReadWrite, OpenIntent::Append] {
        let target = vfs
            .open_target("fs:/mod/gobaksodor/level.dat", intent)
            .unwrap();
        assert_eq!(target.path, fixture.0.join("overlay/level.dat"));
        assert_eq!(target.copy_from, Some(original.clone()));
    }
    assert_eq!(std::fs::read(original).unwrap(), b"original");
}

#[test]
fn absolute_own_module_created_save_is_visible_through_both_names() {
    let (fixture, vfs) = Fixture::new();
    let target = vfs
        .open_target("fs:/mod/GOBaksodor/new.dat", OpenIntent::Create)
        .unwrap();
    assert_eq!(target.path, fixture.0.join("overlay/new.dat"));
    assert_eq!(target.copy_from, None);
    std::fs::write(&target.path, b"save").unwrap();
    for name in ["new.dat", "fs:/~/new.dat", "fs:/mod/gobaksodor/new.dat"] {
        assert_eq!(
            std::fs::read(vfs.resolve(name).unwrap()).unwrap(),
            b"save",
            "{name}"
        );
    }
    assert!(!fixture.0.join("mod/gobaksodor/new.dat").exists());
}

#[test]
fn absolute_own_module_deletion_hides_seed_through_both_names() {
    let (fixture, vfs) = Fixture::new();
    vfs.remove_file("fs:/mod/gobaksodor/level.dat").unwrap();
    assert!(fixture.0.join("mod/gobaksodor/level.dat").is_file());
    assert_eq!(vfs.resolve("level.dat"), None);
    assert_eq!(vfs.resolve("fs:/mod/gobaksodor/level.dat"), None);
}

#[test]
fn absolute_own_module_overlay_paths_share_relative_directory_namespace() {
    let (fixture, vfs) = Fixture::new();
    std::fs::create_dir(fixture.0.join("overlay/newdir")).unwrap();
    assert_eq!(
        vfs.overlay_dir("fs:/mod/gobaksodor/"),
        Some(fixture.0.join("overlay"))
    );
    assert_eq!(
        vfs.overlay_dir("fs:/mod/gobaksodor/newdir"),
        vfs.overlay_dir("newdir")
    );
    assert_eq!(
        vfs.overlay_path("fs:/mod/gobaksodor/level.dat"),
        vfs.overlay_path("level.dat")
    );
}

#[test]
fn absolute_device_entries_and_sibling_modules_keep_their_ownership() {
    let (fixture, vfs) = Fixture::new();
    std::fs::create_dir_all(fixture.0.join("device/mod/gobaksodor")).unwrap();
    let persisted = fixture.0.join("device/mod/gobaksodor/level.dat");
    std::fs::write(&persisted, b"persisted").unwrap();
    assert_eq!(
        vfs.open_target("fs:/mod/gobaksodor/level.dat", OpenIntent::ReadWrite)
            .unwrap()
            .path,
        persisted
    );
    assert_eq!(
        vfs.open_target("fs:/mod/other/new.dat", OpenIntent::Create)
            .unwrap()
            .path,
        fixture.0.join("device/mod/other/new.dat")
    );
    assert_eq!(vfs.resolve("fs:/mod/gobaksodor/../../../outside"), None);
}

#[test]
fn normalized_absolute_own_module_parent_component_keeps_overlay_write_ownership() {
    let (fixture, vfs) = Fixture::new();
    let original = fixture.0.join("mod/gobaksodor/level.dat");
    let path = "fs:/mod/gobaksodor/sub/../level.dat";
    for intent in [OpenIntent::ReadWrite, OpenIntent::Append] {
        let target = vfs.open_target(path, intent).unwrap();
        assert_eq!(target.path, fixture.0.join("overlay/level.dat"));
        assert_eq!(target.copy_from, Some(original.clone()));
    }
    assert_eq!(std::fs::read(original).unwrap(), b"original");
}

#[test]
fn normalized_absolute_own_module_parent_component_never_deletes_original_seed() {
    let (fixture, vfs) = Fixture::new();
    vfs.remove_file("fs:/mod/gobaksodor/sub/../level.dat")
        .unwrap();
    assert_eq!(
        std::fs::read(fixture.0.join("mod/gobaksodor/level.dat")).unwrap(),
        b"original",
        "normalizar um caminho interno não transfere a propriedade para o pacote"
    );
    assert_eq!(vfs.resolve("level.dat"), None);
    assert_eq!(vfs.resolve("fs:/mod/gobaksodor/level.dat"), None);
}

#[test]
fn normalized_sibling_prefix_returning_to_own_module_keeps_overlay_ownership() {
    let (fixture, vfs) = Fixture::new();
    let guest = "fs:/mod/other/../gobaksodor/level.dat";
    let target = vfs.open_target(guest, OpenIntent::ReadWrite).unwrap();
    assert_eq!(target.path, fixture.0.join("overlay/level.dat"));
    assert_eq!(
        target.copy_from,
        Some(fixture.0.join("mod/gobaksodor/level.dat"))
    );
    vfs.remove_file(guest).unwrap();
    assert_eq!(
        std::fs::read(fixture.0.join("mod/gobaksodor/level.dat")).unwrap(),
        b"original"
    );
    assert_eq!(vfs.resolve(guest), None);
    assert_eq!(vfs.resolve("level.dat"), None);
}

#[test]
fn anchored_absolute_parent_path_never_returns_original_as_writable_target() {
    let (fixture, vfs) = Fixture::new();
    let original = fixture.0.join("mod/gobaksodor/level.dat");
    // O fallback histórico ancora a busca no módulo ao encontrar uma barra
    // dupla. Resolver esse caminho não pode transferir a escrita para o seed.
    for intent in [OpenIntent::ReadWrite, OpenIntent::Append] {
        assert!(
            vfs.open_target("fs:/mod/gobaksodor//../level.dat", intent)
                .is_none_or(|target| target.path != original),
            "uma alternativa de leitura não autoriza gravar no pacote"
        );
    }
}

#[test]
fn anchored_absolute_parent_path_never_deletes_original_seed() {
    let (fixture, vfs) = Fixture::new();
    let _ = vfs.remove_file("fs:/mod/gobaksodor//../level.dat");
    assert_eq!(
        std::fs::read(fixture.0.join("mod/gobaksodor/level.dat")).unwrap(),
        b"original",
        "o fallback de diretório não autoriza apagar o pacote"
    );
}

#[test]
fn numeric_resource_directories_keep_overlay_and_deletion_ownership() {
    for directory in ["123", "0x00123456"] {
        let (fixture, vfs) = Fixture::new();
        let seed = fixture.0.join("mod/gobaksodor").join(directory).join("level.dat");
        std::fs::create_dir(seed.parent().unwrap()).unwrap();
        std::fs::write(&seed, b"numbered directory").unwrap();
        let guest = format!("fs:/mod/gobaksodor/{directory}/level.dat");
        let target = vfs.open_target(&guest, OpenIntent::ReadWrite).unwrap();
        assert_eq!(target.path, fixture.0.join("overlay").join(directory).join("level.dat"));
        assert_eq!(target.copy_from, Some(seed.clone()));
        vfs.remove_file(&guest).unwrap();
        assert!(seed.is_file());
        assert_eq!(vfs.resolve(&guest), None);
        assert_eq!(vfs.resolve(&format!("{directory}/level.dat")), None);
        assert_eq!(std::fs::read(fixture.0.join("mod/gobaksodor/level.dat")).unwrap(), b"original");
    }
}
