use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend};
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

const PATH: u32 = loader::STACK_BASE + 0x1000;
const OUTPUT: u32 = PATH + 0x400;
const DATA: u32 = PATH + 0x800;

struct Fixture(PathBuf);
impl Fixture {
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn call(
    machine: &mut Machine<BackendPadrao>,
    interface: Interface,
    method: &str,
    args: [u32; 4],
) -> u32 {
    let slot = (0..256)
        .find(|&slot| interface.method(slot) == Some(method))
        .unwrap();
    match machine
        .call_guest(aee::encode(interface, slot), args, 10_000)
        .unwrap()
    {
        Outcome::Returned { code } => code,
        other => panic!("{method}: {other:?}"),
    }
}

fn setup() -> (Machine<BackendPadrao>, u32, Fixture) {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = Fixture(std::env::temp_dir().join(format!(
        "brew-file-usage-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::create_dir(root.path()).unwrap();
    let module = root.path().join("module");
    let overlay = root.path().join("overlay");
    std::fs::create_dir_all(module.join("resources")).unwrap();
    std::fs::create_dir(&overlay).unwrap();
    std::fs::write(module.join("app.mod"), b"1234").unwrap();
    std::fs::write(module.join("app.sig"), b"12").unwrap();
    std::fs::write(module.join("resources/data.bin"), b"123").unwrap();
    std::fs::write(
        module.join(".zeebx-pacote"),
        b"host manifest is not a guest resource",
    )
    .unwrap();
    std::fs::create_dir(overlay.join(".brew-vfs-deleted")).unwrap();
    std::fs::write(
        overlay.join(".brew-vfs-deleted/host-marker"),
        b"host metadata",
    )
    .unwrap();
    std::fs::write(root.path().join("outside.bin"), b"outside the module").unwrap();
    let image = ModImage::parse(
        [0xe3a0_0000u32, 0xe12f_ff1e]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect(),
    )
    .unwrap();
    let storage = StoragePaths::from_root(root.path().join("runtime"));
    storage.create_dirs().unwrap();
    let mut machine = Machine::new_with_storage(
        BackendPadrao::new().unwrap(),
        loader::load_with(&image, &[]).unwrap(),
        &module,
        &storage,
        Some(overlay),
    );
    assert_eq!(machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
    assert_eq!(
        call(
            &mut machine,
            Interface::Shell,
            "CreateInstance",
            [loader::OBJECT_BASE, 0x0100_1003, OUTPUT, 0]
        ),
        0
    );
    let manager = machine.cpu().read_u32(OUTPUT).unwrap();
    (machine, manager, root)
}

fn path(machine: &mut Machine<BackendPadrao>, value: &str) {
    machine
        .cpu_mut()
        .write_mem(PATH, &[value.as_bytes(), &[0]].concat())
        .unwrap();
}

fn usage(machine: &mut Machine<BackendPadrao>, manager: u32) -> (u16, u16, u32, u32) {
    assert_eq!(
        call(
            machine,
            Interface::FileMgr,
            "GetFileUseInfo",
            [manager, OUTPUT, 0, 0]
        ),
        0
    );
    let mut bytes = [0; 12];
    machine.cpu().read_mem(OUTPUT, &mut bytes).unwrap();
    (
        u16::from_le_bytes(bytes[..2].try_into().unwrap()),
        u16::from_le_bytes(bytes[2..4].try_into().unwrap()),
        u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        u32::from_le_bytes(bytes[8..].try_into().unwrap()),
    )
}

fn write_file(
    machine: &mut Machine<BackendPadrao>,
    manager: u32,
    name: &str,
    mode: u32,
    data: &[u8],
) {
    path(machine, name);
    let file = call(
        machine,
        Interface::FileMgr,
        "OpenFile",
        [manager, PATH, mode, 0],
    );
    assert_ne!(file, 0);
    machine.cpu_mut().write_mem(DATA, data).unwrap();
    assert_eq!(
        call(
            machine,
            Interface::File,
            "Write",
            [file, DATA, data.len() as u32, 0]
        ),
        data.len() as u32
    );
    assert_eq!(
        call(machine, Interface::File, "Release", [file, 0, 0, 0]),
        0
    );
}

#[test]
fn file_usage_reports_sdk_layout_and_real_module_files_with_bounded_output() {
    let (mut machine, manager, _root) = setup();
    machine
        .cpu_mut()
        .write_mem(OUTPUT - 4, &[0xa5; 20])
        .unwrap();
    let (max_files, files, max_space, used) = usage(&mut machine, manager);
    assert!(max_files >= 7);
    assert_eq!(files, 3, "mod, sig e recurso recursivo pertencem ao módulo");
    assert_eq!(
        used, 9,
        "metadados do host e arquivos fora do módulo não pertencem ao aplicativo"
    );
    let mut guards = [0; 4];
    machine.cpu().read_mem(OUTPUT - 4, &mut guards).unwrap();
    assert_eq!(guards, [0xa5; 4]);
    machine.cpu().read_mem(OUTPUT + 12, &mut guards).unwrap();
    assert_eq!(guards, [0xa5; 4], "AEEFileUseInfo ocupa exatamente12bytes");
    let free = call(
        &mut machine,
        Interface::FileMgr,
        "GetFreeSpace",
        [manager, OUTPUT + 32, 0, 0],
    );
    let filesystem_total = machine.cpu().read_u32(OUTPUT + 32).unwrap();
    assert_eq!(
        max_space, filesystem_total,
        "o limite do módulo respeita a capacidade virtual informada"
    );
    assert!(free <= max_space && used < max_space);
}

#[test]
fn file_usage_tracks_overlay_writes_creates_and_deletions_without_double_counting() {
    let (mut machine, manager, root) = setup();
    write_file(&mut machine, manager, "APP.MOD", 2, b"1234567");
    assert_eq!(usage(&mut machine, manager).1, 3);
    assert_eq!(usage(&mut machine, manager).3, 12);
    assert_eq!(
        std::fs::read(root.path().join("module/app.mod")).unwrap(),
        b"1234"
    );
    write_file(&mut machine, manager, "new.dat", 4, b"12");
    let (_, files, _, used) = usage(&mut machine, manager);
    assert_eq!((files, used), (4, 14));
    path(&mut machine, "app.mod");
    assert_eq!(
        call(
            &mut machine,
            Interface::FileMgr,
            "Remove",
            [manager, PATH, 0, 0]
        ),
        0
    );
    let (_, files, _, used) = usage(&mut machine, manager);
    assert_eq!(
        (files, used),
        (3, 7),
        "o seed ocultado por exclusão não reaparece na contagem"
    );
}

#[test]
fn file_usage_sees_overlay_only_nested_files_and_shadowed_package_files() {
    let (mut machine, manager, root) = setup();
    std::fs::create_dir_all(root.path().join("overlay/resources")).unwrap();
    std::fs::create_dir(root.path().join("overlay/newdir")).unwrap();
    std::fs::write(root.path().join("overlay/resources/DATA.BIN"), b"123456789").unwrap();
    std::fs::write(root.path().join("overlay/newdir/only.dat"), b"12345").unwrap();
    let (_, files, _, used) = usage(&mut machine, manager);
    assert_eq!((files, used), (4, 20));
}

#[test]
fn file_usage_rejects_null_overflowed_and_unmapped_output() {
    let (mut machine, manager, _root) = setup();
    for output in [0, u32::MAX - 5, 1] {
        assert_ne!(
            call(
                &mut machine,
                Interface::FileMgr,
                "GetFileUseInfo",
                [manager, output, 0, 0]
            ),
            0,
            "{output:#x}"
        );
    }
}

#[cfg(any(unix, windows))]
#[test]
fn file_usage_does_not_walk_outside_directory_aliases_or_cycles() {
    let (mut machine, manager, root) = setup();
    let outside = root.path().join("outside-directory");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(
        outside.join("foreign.dat"),
        b"outside storage must not be counted",
    )
    .unwrap();
    let external_link = root.path().join("module").join("external-link");
    let cycle_link = root
        .path()
        .join("module")
        .join("resources")
        .join("back-to-root");

    #[cfg(unix)]
    let alias = |link: &Path, target: &Path| std::os::unix::fs::symlink(target, link).unwrap();
    #[cfg(windows)]
    let alias = |link: &Path, target: &Path| {
        use std::os::windows::process::CommandExt;
        if std::os::windows::fs::symlink_dir(target, link).is_ok() {
            return;
        }
        let result = std::process::Command::new("cmd.exe")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .creation_flags(0x0800_0000)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    };
    alias(&external_link, &outside);
    alias(&cycle_link, &root.path().join("module"));
    assert!(external_link.join("foreign.dat").is_file());
    let (_, files, _, used) = usage(&mut machine, manager);
    assert_eq!((files, used), (3, 9));
    assert!(outside.join("foreign.dat").is_file());
    #[cfg(windows)]
    for link in [&external_link, &cycle_link] {
        std::fs::remove_dir(link).unwrap();
    }
    #[cfg(unix)]
    for link in [&external_link, &cycle_link] {
        std::fs::remove_file(link).unwrap();
    }
}
