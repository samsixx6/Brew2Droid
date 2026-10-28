use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use zeebx::handset::display::{self, Source};
use zeebx::handset::types::DisplayGeometry;

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: PathBuf,
    module: PathBuf,
    profiles: PathBuf,
}
impl Fixture {
    fn new(folder: &str) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests")
            .join(format!(
                "display-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        let dir = root.join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        let module = dir.join("game.mod");
        std::fs::write(&module, b"synthetic game version one").unwrap();
        Self {
            profiles: root.join("preferences"),
            root,
            module,
        }
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
fn packed_asphalt3_versions_have_verified_distinct_screens() {
    for (hash,expected) in [
        ("c0acf91ae952f8be6d938ec1204a4ca50a430966d3f273a01e13c21b783bb0db",geometry(240,320)),
        ("685a2b45fc709eec565350926e95677bd8a3bfb9404e37d03eb2d89a8c2b41bc",geometry(176,220)),
        ("6a17ee50bbc11d0edcbf794a0d926b51140f152e8c613df81feef493f2c24f7f",geometry(176,220)),
    ] {
        let selected=display::automatic(hash,Path::new("Games/wrong-800x400/asphalt3.mod"));
        assert_eq!(selected.geometry,expected);
        assert_eq!(selected.source,Source::Known);
    }
}

#[test]
fn shared_loader_variants_use_payload_identity_for_manual_screens() {
    use sha2::{Digest,Sha256};
    let f=Fixture::new("Games/EditionA");
    let second=f.root.join("Games/EditionB");
    std::fs::create_dir_all(&second).unwrap();
    std::fs::write(second.join("game.mod"),std::fs::read(&f.module).unwrap()).unwrap();
    std::fs::write(f.module.with_file_name("z.bar"),b"payload one").unwrap();
    std::fs::write(second.join("z.bar"),b"payload two").unwrap();
    let key:String=Sha256::digest(b"payload one").iter().map(|b|format!("{b:02x}")).collect();
    let overrides=f.profiles.join("overrides");
    std::fs::create_dir_all(&overrides).unwrap();
    std::fs::write(overrides.join(format!("{key}.json")),br#"{"width":320,"height":240,"depth":16}"#).unwrap();
    let loader="94e7329d73313e7af041dd302b7b7edfdd6d5ea9b54c6881f6bbbeddbb1456e1";
    let first=display::resolve_hash(&f.module,&f.profiles,loader).unwrap();
    assert_eq!(first.geometry,geometry(320,240));
    assert_eq!(first.source,Source::Manual);
    let other=display::resolve_hash(&second.join("game.mod"),&f.profiles,loader).unwrap();
    assert_eq!(other.geometry,geometry(240,320));
    assert_eq!(other.source,Source::Fallback);
    assert_eq!(other.module_sha256,loader,"o SHA do módulo continua sendo o do arquivo real");
}

#[test]
fn known_versions_keep_their_existing_resolution_despite_name_hints() {
    for (id, hash, _) in zeebx::handset::profile::IDENTITIES {
        let choice = display::automatic(hash, Path::new("wrong-800x400/game.mod"));
        let expected = match id {
            zeebx::handset::types::GameId::Bowling3d => geometry(176, 220),
            _ => geometry(240, 320),
        };
        assert_eq!(choice.geometry, expected);
        assert_eq!(choice.source, Source::Known);
    }
}
#[test]
fn new_game_uses_a_clear_module_or_outer_folder_size_label() {
    for path in [
        "Games/Edition-176x220/game.mod",
        "Games/Edition-176x220/Mod/game.mod",
        "Games/39367/game-176X220.mod",
    ] {
        let choice = display::automatic(&"0".repeat(64), Path::new(path));
        assert_eq!(choice.geometry, geometry(176, 220), "{path}");
        assert_eq!(choice.source, Source::Name);
    }
}

#[test]
fn nested_mod_layout_uses_the_package_title_directly_above_mod() {
    for path in [
        "Games/Proper Name (176x220) 1.0/Mod/12345/game.mod",
        "800x480/Games/Proper Name (176x220) 1.0/mOd/12345/game.mod",
    ] {
        let choice = display::automatic(&"0".repeat(64),Path::new(path));
        assert_eq!(choice.geometry,geometry(176,220),"{path}");
        assert_eq!(choice.source,Source::Name,"{path}");
        for (_,hash,_) in zeebx::handset::profile::IDENTITIES {
            assert_eq!(display::automatic(hash,Path::new(path)).source,Source::Known);
        }
    }
}

#[test]
fn nested_mod_layout_stops_before_the_packages_collection_ancestors() {
    for path in [
        "Games/Collection (800x480)/Unknown Package/Mod/12345/game.mod",
        "176x220/Games/Unknown Package/Mod/12345/game.mod",
        "800x480/games/Mod/12345/game.mod",
    ] {
        let choice = display::automatic(&"0".repeat(64),Path::new(path));
        assert_eq!(choice.geometry,geometry(240,320),"{path}");
        assert_eq!(choice.source,Source::Fallback,"{path}");
    }
}

#[test]
fn manual_display_keeps_precedence_for_nested_mod_packages() {
    let f = Fixture::new("Games/Proper Name (176x220) 1.0/Mod/12345");
    let selected = display::set_override(&f.module,&f.profiles,Some(geometry(320,240))).unwrap();
    assert_eq!(selected.source,Source::Manual);
    assert_eq!(selected.geometry,geometry(320,240));
    assert_eq!(selected.automatic.geometry,geometry(176,220));
    assert_eq!(display::resolve(&f.module,&f.profiles).unwrap(),selected);
}
#[test]
fn collection_variant_labels_above_numeric_folders_are_not_lost() {
    for (path, expected) in [
        (
            "brew2droid/games/Mod/hu_m328_128x128/272834/dirt.mod",
            geometry(128, 128),
        ),
        (
            "brew2droid/games/Mod/mot_w755-176x204/271799/predator3d.mod",
            geometry(176, 204),
        ),
        (
            "brew2droid/games/Mod/more games/NASCAR09 - Lg un610 - 320x240/272762/nascar09.mod",
            geometry(320, 240),
        ),
        (
            "brew2droid/games/Mod/ver 7.5.45 en.sau960pre-sam_sch_u960-800x480touch/275777/nfs09.mod",
            geometry(800, 480),
        ),
        (
            "brew2droid/games/mOd/more games/ação 176x220/modules/39367/game.mod",
            geometry(176, 220),
        ),
    ] {
        let choice = display::automatic(&"0".repeat(64), Path::new(path));
        assert_eq!(choice.geometry, expected, "{path}");
        assert_eq!(choice.source, Source::Name, "{path}");
    }
}
#[test]
fn rediscovery_updates_automatic_hints_without_discarding_manual_preferences() {
    let f = Fixture::new("games/Mod/mot_v860_176x220/272834");
    let cached = display::resolve(&f.module, &f.profiles).unwrap();
    let from_hash = display::resolve_hash(&f.module, &f.profiles, &cached.module_sha256).unwrap();
    assert_eq!(from_hash.geometry, geometry(176, 220));
    assert_eq!(
        display::resolve(&f.module, &f.profiles).unwrap().source,
        Source::Name
    );
    let selected = display::set_override(&f.module, &f.profiles, Some(geometry(320, 240))).unwrap();
    assert_eq!(display::resolve(&f.module, &f.profiles).unwrap(), selected);
    assert_eq!(selected.geometry, geometry(320, 240));
    assert_eq!(selected.automatic.geometry, geometry(176, 220));
    assert_eq!(
        display::set_override(&f.module, &f.profiles, None).unwrap(),
        from_hash
    );
}
#[test]
fn ambiguous_invalid_and_unrelated_labels_use_the_chosen_default() {
    for path in [
        "39367/game.mod",
        "4x4 Championship/game.mod",
        "Game-176x220-240x320/game.mod",
        "Game-1920x1080/game.mod",
        "640x480/Games/Unknown/Mod/game.mod",
        "800x480/games/Mod/39367/game.mod",
        "800x480/games/Unknown/39367/game.mod",
        "games/Mod/240x320/edition176x220/39367/game.mod",
        "storage/emulated/0/800x480/MyLibrary/39367/game.mod",
        "176x220/Unknown/39367/game.mod",
    ] {
        let choice = display::automatic(&"0".repeat(64), Path::new(path));
        assert_eq!(choice.geometry, geometry(240, 320), "{path}");
        assert_eq!(choice.source, Source::Fallback);
    }
}
#[test]
fn discovery_saves_identity_outside_the_untouched_game_folder() {
    let f = Fixture::new("ação 176x220/Mod");
    let before = std::fs::read(&f.module).unwrap();
    let selected = display::resolve(&f.module, &f.profiles).unwrap();
    assert_eq!(selected.geometry, geometry(176, 220));
    assert_eq!(selected.source, Source::Name);
    assert_eq!(std::fs::read(&f.module).unwrap(), before);
    assert_eq!(
        std::fs::read_dir(f.module.parent().unwrap())
            .unwrap()
            .count(),
        1
    );
    assert!(f.profiles.join("index").is_dir());
    assert_eq!(display::resolve(&f.module, &f.profiles).unwrap(), selected);
}
#[test]
fn manual_choice_survives_reopen_and_rename_and_can_return_to_auto() {
    let f = Fixture::new("Edition 176x220");
    let saved = display::set_override(&f.module, &f.profiles, Some(geometry(320, 240))).unwrap();
    assert_eq!(saved.source, Source::Manual);
    assert_eq!(saved.geometry, geometry(320, 240));
    assert_eq!(saved.automatic.geometry, geometry(176, 220));
    assert_eq!(
        display::resolve(&f.module, &f.profiles).unwrap().geometry,
        geometry(320, 240)
    );
    let renamed = f.module.with_file_name("renamed.mod");
    std::fs::rename(&f.module, &renamed).unwrap();
    assert_eq!(
        display::resolve(&renamed, &f.profiles).unwrap().geometry,
        geometry(320, 240)
    );
    let reset = display::set_override(&renamed, &f.profiles, None).unwrap();
    assert_eq!(reset.source, Source::Name);
    assert_eq!(reset.geometry, geometry(176, 220));
}
#[test]
fn invalid_manual_dimensions_do_not_replace_the_saved_choice() {
    let f = Fixture::new("Unknown");
    display::set_override(&f.module, &f.profiles, Some(geometry(176, 220))).unwrap();
    for bad in [
        geometry(0, 320),
        geometry(240, 1025),
        DisplayGeometry {
            width: 240,
            height: 320,
            depth: 32,
        },
    ] {
        assert!(display::set_override(&f.module, &f.profiles, Some(bad)).is_err());
        assert_eq!(
            display::resolve(&f.module, &f.profiles).unwrap().geometry,
            geometry(176, 220)
        );
    }
}
#[test]
fn a_different_module_version_does_not_inherit_the_old_override() {
    let f = Fixture::new("Unknown");
    let old = display::set_override(&f.module, &f.profiles, Some(geometry(176, 220))).unwrap();
    std::fs::write(&f.module, b"different version with a different length").unwrap();
    let new = display::resolve(&f.module, &f.profiles).unwrap();
    assert_ne!(new.module_sha256, old.module_sha256);
    assert_eq!(new.source, Source::Fallback);
    assert_eq!(new.geometry, geometry(240, 320));
}
#[test]
fn corrupt_manual_preferences_are_reported_and_preserved_until_explicit_reset() {
    let f = Fixture::new("Unknown");
    let selected = display::set_override(&f.module, &f.profiles, Some(geometry(176, 220))).unwrap();
    let path = f
        .profiles
        .join("overrides")
        .join(format!("{}.json", selected.module_sha256));
    std::fs::write(&path, b"broken preference").unwrap();
    assert!(display::resolve(&f.module, &f.profiles).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"broken preference");
    assert_eq!(
        display::set_override(&f.module, &f.profiles, None)
            .unwrap()
            .source,
        Source::Fallback
    );
}
#[test]
fn missing_and_oversized_modules_are_rejected_without_scanning_resources() {
    let f = Fixture::new("Unknown");
    assert!(display::resolve(&f.root.join("missing.mod"), &f.profiles).is_err());
    std::fs::OpenOptions::new()
        .write(true)
        .open(&f.module)
        .unwrap()
        .set_len(16 * 1024 * 1024 + 1)
        .unwrap();
    assert!(display::resolve(&f.module, &f.profiles).is_err());
}
