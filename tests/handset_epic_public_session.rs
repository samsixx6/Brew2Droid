use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use zeebx::{
    handset::{display, package::read_module_pair},
    session::{Session, Step},
    storage::StoragePaths,
};
struct Runtime(PathBuf);
impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn replay(var: &str) {
    static N: AtomicU64 = AtomicU64::new(0);
    let module = PathBuf::from(
        std::env::var_os(var).unwrap_or_else(|| panic!("set {var} to the original Epic module")),
    );
    let mif = zeebx::library::resolve_mif(&module).unwrap();
    let before = read_module_pair(&module, &mif).unwrap().report;
    let geometry = display::automatic(&before.module_sha256, &module).geometry;
    let runtime = Runtime(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests")
            .join(format!(
                "epic-normal-session-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            )),
    );
    let storage = StoragePaths::from_root(&runtime.0);
    storage.create_dirs().unwrap();
    let mut session =
        Session::start_handset_with_storage(&module, geometry, &storage, None).unwrap();
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(2) {
        let step = session.step(Duration::from_millis(12), true);
        assert!(
            !matches!(step, Step::Stopped),
            "normal Session stopped: {:?}",
            session.stopped_reason()
        );
        while session.mostra_quadro_intermediario() {}
        if matches!(step, Step::Ahead) {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    assert!(session.stopped_reason().is_none());
    let pixels = session.screen().to_rgb565_bytes();
    assert!(pixels.chunks_exact(2).any(|p| p != &pixels[..2]));
    let after = read_module_pair(&module, &mif).unwrap().report;
    assert_eq!(before.module_sha256, after.module_sha256);
    assert_eq!(before.mif_sha256, after.mif_sha256);
}
#[test]
#[ignore = "requer par original em BREW_EPIC_ONE_MOD"]
fn epic_one_original_normal_session_reaches_real_frame_without_null_framebuffer() {
    replay("BREW_EPIC_ONE_MOD");
}
#[test]
#[ignore = "requer par original em BREW_EPIC_KOREAN_MOD"]
fn epic_korean_original_normal_session_reaches_real_frame_without_null_framebuffer() {
    replay("BREW_EPIC_KOREAN_MOD");
}
