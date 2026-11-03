use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend};
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

const BUFFER: u32 = loader::STACK_BASE + 0x1000;

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(name);
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn call(
    machine: &mut Machine<BackendPadrao>,
    interface: Interface,
    name: &str,
    args: [u32; 4],
) -> u32 {
    let slot = (0..256)
        .find(|&slot| interface.method(slot) == Some(name))
        .unwrap();
    match machine
        .call_guest(aee::encode(interface, slot), args, 10_000)
        .unwrap()
    {
        Outcome::Returned { code } => code,
        outcome => panic!("{name}: {outcome:?}"),
    }
}

fn setup() -> (Machine<BackendPadrao>, u32, TempDir) {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = TempDir::new(&format!(
        "brew-file-root-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(root.join("existing-directory")).unwrap();
    std::fs::write(root.join("existing-file.dat"), b"file contents").unwrap();
    let image = ModImage::parse(
        [0xe3a0_0000u32, 0xe12f_ff1e]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect(),
    )
    .unwrap();
    let storage = StoragePaths::from_root(root.join("runtime"));
    storage.create_dirs().unwrap();
    let overlay = root.join("overlay");
    std::fs::create_dir(&overlay).unwrap();
    let mut machine = Machine::new_with_storage(
        BackendPadrao::new().unwrap(),
        loader::load_with(&image, &[]).unwrap(),
        root.path(),
        &storage,
        Some(overlay),
    );
    assert_eq!(machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
    assert_eq!(
        call(
            &mut machine,
            Interface::Shell,
            "CreateInstance",
            [loader::OBJECT_BASE, 0x0100_1003, BUFFER, 0]
        ),
        0
    );
    let manager = machine.cpu().read_u32(BUFFER).unwrap();
    (machine, manager, root)
}

fn test_path(machine: &mut Machine<BackendPadrao>, manager: u32, path: &str) -> u32 {
    let mut bytes = path.as_bytes().to_vec();
    bytes.push(0);
    machine.cpu_mut().write_mem(BUFFER, &bytes).unwrap();
    call(machine, Interface::FileMgr, "Test", [manager, BUFFER, 0, 0])
}

#[test]
fn filemgr_test_accepts_empty_application_root() {
    let (mut machine, manager, _root) = setup();
    // AEEFile.h especifica expressamente que "" nomeia a raiz do aplicativo.
    assert_eq!(test_path(&mut machine, manager, ""), 0);
}

#[test]
fn filemgr_test_accepts_current_and_anchored_application_roots() {
    let (mut machine, manager, _root) = setup();
    for path in [".", "\\.", "./", "fs:/~/"] {
        assert_eq!(test_path(&mut machine, manager, path), 0, "{path:?}");
    }
}

#[test]
fn filemgr_test_accepts_existing_files_and_directories() {
    let (mut machine, manager, _root) = setup();
    for path in [
        "existing-file.dat",
        "existing-directory",
        "EXISTING-DIRECTORY",
    ] {
        assert_eq!(test_path(&mut machine, manager, path), 0, "{path}");
    }
}

#[test]
fn filemgr_test_rejects_missing_entries_and_paths_outside_the_guest_namespace() {
    let (mut machine, manager, _root) = setup();
    for path in ["missing.dat", "missing-directory", "../../", "C:\\Windows"] {
        assert_eq!(test_path(&mut machine, manager, path), 1, "{path}");
    }
}

#[test]
fn filemgr_test_keeps_overlay_deleted_package_files_absent() {
    let (mut machine, manager, root) = setup();
    assert_eq!(test_path(&mut machine, manager, "existing-file.dat"), 0);
    assert_eq!(
        call(
            &mut machine,
            Interface::FileMgr,
            "Remove",
            [manager, BUFFER, 0, 0]
        ),
        0
    );
    assert!(
        root.join("existing-file.dat").is_file(),
        "o pacote original permanece intacto"
    );
    assert_eq!(test_path(&mut machine, manager, "existing-file.dat"), 1);
}
