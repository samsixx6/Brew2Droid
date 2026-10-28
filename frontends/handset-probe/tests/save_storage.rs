#[path = "../../android/src/save_storage.rs"]
mod save_storage;

use std::path::PathBuf;
use zeebx::storage::{ContentId, StoragePaths};

struct Fixture {
    root: PathBuf,
    legacy: StoragePaths,
    public: PathBuf,
    id: ContentId,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "brew-public-saves-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let legacy = StoragePaths::from_root(root.join("private/.config/zeebx"));
        legacy.create_dirs().unwrap();
        let public = root.join("Internal storage/brew2droid/data");
        let id = ContentId::from_reader(b"same game module".as_slice()).unwrap();
        Self {
            root,
            legacy,
            public,
            id,
        }
    }
    fn old_save(&self) -> PathBuf {
        self.legacy.save_for(&self.id)
    }
    fn old_progress(&self) {
        let save = self.old_save().join("nested");
        std::fs::create_dir_all(&save).unwrap();
        std::fs::write(save.join("progress.dat"), b"level 7").unwrap();
    }
    fn archive_with_cached_progress(&self) -> (PathBuf, ContentId, PathBuf) {
        let game = self.root.join("game.zip");
        std::fs::write(&game, include_bytes!("fixtures/save-migration.zip")).unwrap();
        let id = self.legacy.content_id(&game).unwrap();
        let module = zeebx::loader::archive::extract_in(&game, &self.legacy.cache).unwrap();
        let progress = module.parent().unwrap().join("cached-progress.sav");
        std::fs::write(&progress, b"old cached progress").unwrap();
        (game, id, progress)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn migration_preserves_existing_private_progress_and_uses_public_data() {
    let f = Fixture::new();
    f.old_progress();
    std::fs::write(f.legacy.device.join("score.db"), b"score 123").unwrap();
    let storage = save_storage::prepare(&f.legacy, &f.public).unwrap();
    assert_eq!(storage.saves, f.public.join("saves"));
    assert_eq!(storage.device, f.public.join("shared"));
    assert_eq!(storage.cache, f.legacy.cache, "cache permanece privado");
    assert_eq!(
        std::fs::read(storage.save_for(&f.id).join("nested/progress.dat")).unwrap(),
        b"level 7"
    );
    assert_eq!(
        std::fs::read(storage.device.join("score.db")).unwrap(),
        b"score 123"
    );
    assert_eq!(
        std::fs::read(f.old_save().join("nested/progress.dat")).unwrap(),
        b"level 7"
    );
    assert_eq!(
        std::fs::read(f.legacy.device.join("score.db")).unwrap(),
        b"score 123"
    );
}

#[test]
fn an_existing_public_save_is_not_mixed_with_older_private_files() {
    let f = Fixture::new();
    f.old_progress();
    let public_save = f.public.join("saves").join(f.id.as_str());
    std::fs::create_dir_all(&public_save).unwrap();
    std::fs::write(public_save.join("new-progress.dat"), b"level 12").unwrap();
    let storage = save_storage::prepare(&f.legacy, &f.public).unwrap();
    assert_eq!(
        std::fs::read(storage.save_for(&f.id).join("new-progress.dat")).unwrap(),
        b"level 12"
    );
    assert!(!storage.save_for(&f.id).join("nested/progress.dat").exists());
}

#[test]
fn a_file_at_the_selected_public_save_path_blocks_start_after_reinstall() {
    let f = Fixture::new();
    let game = f.root.join("main.mod");
    std::fs::write(&game, b"same game module").unwrap();
    let public_save = f.public.join("saves").join(f.id.as_str());
    std::fs::create_dir_all(public_save.parent().unwrap()).unwrap();
    std::fs::write(&public_save, b"blocked").unwrap();
    assert!(save_storage::prepare_game(&f.legacy, &f.public, &game, &f.id).is_err());
    assert_eq!(std::fs::read(public_save).unwrap(), b"blocked");
}

#[test]
fn reopening_after_a_reinstall_reads_the_public_progress_through_the_real_vfs() {
    use zeebx::brew::vfs::{OpenIntent, Vfs};
    let f = Fixture::new();
    let first = save_storage::prepare(&f.legacy, &f.public).unwrap();
    let roms = f.root.join("roms");
    std::fs::create_dir_all(&roms).unwrap();
    std::fs::write(roms.join("main.mod"), b"module").unwrap();
    let mut vfs = Vfs::new(&roms);
    vfs.set_save_root(first.save_for(&f.id));
    vfs.set_device_root(&first.device);
    let target = vfs.open_target("progress.dat", OpenIntent::Create).unwrap();
    std::fs::create_dir_all(target.path.parent().unwrap()).unwrap();
    std::fs::write(&target.path, b"race won").unwrap();
    let fresh = StoragePaths::from_root(f.root.join("reinstalled/private"));
    fresh.create_dirs().unwrap();
    let second = save_storage::prepare(&fresh, &f.public).unwrap();
    assert!(target.path.starts_with(&f.public));
    let mut reopened = Vfs::new(&roms);
    reopened.set_save_root(second.save_for(&f.id));
    reopened.set_device_root(&second.device);
    assert_eq!(
        std::fs::read(reopened.resolve("progress.dat").unwrap()).unwrap(),
        b"race won"
    );
    assert_eq!(std::fs::read(roms.join("main.mod")).unwrap(), b"module");
    assert!(!roms.join("progress.dat").exists());
}

#[test]
fn unavailable_public_data_is_an_error_and_does_not_consume_private_saves() {
    let f = Fixture::new();
    f.old_progress();
    std::fs::create_dir_all(f.public.parent().unwrap()).unwrap();
    std::fs::write(&f.public, b"blocked").unwrap();
    assert!(save_storage::prepare(&f.legacy, &f.public).is_err());
    assert_eq!(
        std::fs::read(f.old_save().join("nested/progress.dat")).unwrap(),
        b"level 7"
    );
    assert_eq!(std::fs::read(&f.public).unwrap(), b"blocked");
}

#[test]
fn a_blocked_shared_tree_prevents_start_without_consuming_saves() {
    let f = Fixture::new();
    std::fs::create_dir_all(f.old_save()).unwrap();
    std::fs::write(f.old_save().join("good.dat"), b"progress").unwrap();
    let shared = f.public.join("shared");
    std::fs::create_dir_all(&f.public).unwrap();
    std::fs::write(&shared, b"blocked").unwrap();
    assert!(save_storage::prepare(&f.legacy, &f.public).is_err());
    assert!(!f.public.join("saves").join(f.id.as_str()).exists());
    assert_eq!(
        std::fs::read(f.old_save().join("good.dat")).unwrap(),
        b"progress"
    );
}

#[test]
fn migration_includes_games_that_have_not_been_opened_since_the_update() {
    let f = Fixture::new();
    f.old_progress();
    let other = ContentId::from_reader(b"another game".as_slice()).unwrap();
    let old = f.legacy.save_for(&other);
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(old.join("career.dat"), b"championship").unwrap();
    let storage = save_storage::prepare(&f.legacy, &f.public).unwrap();
    assert_eq!(
        std::fs::read(storage.save_for(&other).join("career.dat")).unwrap(),
        b"championship"
    );
    assert_eq!(
        std::fs::read(old.join("career.dat")).unwrap(),
        b"championship"
    );
}

#[test]
fn a_copy_failure_never_publishes_the_partial_game_directory() {
    let f = Fixture::new();
    f.old_progress();
    let mut deep = f.old_save();
    for _ in 0..66 {
        deep = deep.join("d");
    }
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::write(deep.join("deep.dat"), b"original").unwrap();
    assert!(save_storage::prepare(&f.legacy, &f.public).is_err());
    assert!(!f.public.join("saves").join(f.id.as_str()).exists());
    assert!(
        std::fs::read_dir(f.public.join("saves"))
            .unwrap()
            .next()
            .is_none(),
        "a árvore temporária foi limpa"
    );
    assert_eq!(std::fs::read(deep.join("deep.dat")).unwrap(), b"original");
}

#[test]
fn established_public_progress_never_resurrects_old_archive_cache_files() {
    let f = Fixture::new();
    let (game, id, cached) = f.archive_with_cached_progress();
    let public_save = f.public.join("saves").join(id.as_str());
    std::fs::create_dir_all(&public_save).unwrap();
    std::fs::write(public_save.join("new.sav"), b"new public progress").unwrap();
    for _ in 0..2 {
        let storage = save_storage::prepare_game(&f.legacy, &f.public, &game, &id).unwrap();
        assert!(!storage.save_for(&id).join("cached-progress.sav").exists());
        assert_eq!(
            std::fs::read(public_save.join("new.sav")).unwrap(),
            b"new public progress"
        );
    }
    assert_eq!(std::fs::read(cached).unwrap(), b"old cached progress");
}

#[test]
fn a_new_public_archive_save_includes_both_private_overlay_and_cached_progress() {
    let f = Fixture::new();
    let (game, id, cached) = f.archive_with_cached_progress();
    let overlay = f.legacy.save_for(&id);
    std::fs::create_dir_all(&overlay).unwrap();
    std::fs::write(overlay.join("profile.sav"), b"existing private profile").unwrap();
    let storage = save_storage::prepare_game(&f.legacy, &f.public, &game, &id).unwrap();
    assert_eq!(
        std::fs::read(storage.save_for(&id).join("profile.sav")).unwrap(),
        b"existing private profile"
    );
    assert_eq!(
        std::fs::read(storage.save_for(&id).join("cached-progress.sav")).unwrap(),
        b"old cached progress"
    );
    assert!(!storage.save_for(&id).join("main.mod").exists());
    assert_eq!(
        std::fs::read(overlay.join("profile.sav")).unwrap(),
        b"existing private profile"
    );
    assert_eq!(std::fs::read(cached).unwrap(), b"old cached progress");
}
