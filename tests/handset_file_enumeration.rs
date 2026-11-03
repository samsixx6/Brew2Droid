use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend};
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

const NAME: u32 = loader::STACK_BASE + 0x1000;
const INFO: u32 = NAME + 0x400;
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let path = self.0.canonicalize().unwrap();
        assert!(path.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(path.file_name().unwrap().to_string_lossy().starts_with("brew-enumeration-"));
        let _ = std::fs::remove_dir_all(path);
    }
}
fn call(m: &mut Machine<BackendPadrao>, name: &str, args: [u32; 4]) -> u32 {
    let iface = if name == "CreateInstance" { Interface::Shell } else { Interface::FileMgr };
    let slot = (0..256).find(|&slot| iface.method(slot) == Some(name)).unwrap();
    match m.call_guest(aee::encode(iface, slot), args, 10_000).unwrap() {
        Outcome::Returned { code } => code,
        other => panic!("{name}: {other:?}"),
    }
}
fn setup() -> (Machine<BackendPadrao>, u32, Fixture) {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let fixture = Fixture(std::env::temp_dir().join(format!(
        "brew-enumeration-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    let module = fixture.0.join("content/demo");
    let overlay = fixture.0.join("overlay");
    std::fs::create_dir_all(&module).unwrap();
    std::fs::create_dir_all(&overlay).unwrap();
    std::fs::write(module.join("seed.dat"), b"seed").unwrap();
    std::fs::create_dir(overlay.join("saves")).unwrap();
    std::fs::write(overlay.join("saves/slot.dat"), b"saved").unwrap();
    let storage = StoragePaths::from_root(fixture.0.join("runtime"));
    storage.create_dirs().unwrap();
    let image = ModImage::parse([0xe3a0_0000u32, 0xe12f_ff1e].into_iter().flat_map(u32::to_le_bytes).collect()).unwrap();
    let mut m = Machine::new_with_storage(
        BackendPadrao::new().unwrap(), loader::load_with(&image, &[]).unwrap(),
        &module, &storage, Some(overlay),
    );
    assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
    assert_eq!(call(&mut m, "CreateInstance", [loader::OBJECT_BASE, 0x0100_1003, INFO, 0]), 0);
    let manager = m.cpu().read_u32(INFO).unwrap();
    (m, manager, fixture)
}
fn init(m: &mut Machine<BackendPadrao>, manager: u32, path: &str, dirs: bool) -> u32 {
    m.cpu_mut().write_mem(NAME, &[path.as_bytes(), &[0]].concat()).unwrap();
    call(m, "EnumInit", [manager, NAME, u32::from(dirs), 0])
}
fn next(m: &mut Machine<BackendPadrao>, manager: u32) -> Option<String> {
    if call(m, "EnumNext", [manager, INFO, 0, 0]) == 0 { return None; }
    let mut name = [0; 64];
    m.cpu().read_mem(INFO + 12, &mut name).unwrap();
    Some(String::from_utf8(name.into_iter().take_while(|&byte| byte != 0).collect()).unwrap())
}
#[test]
fn enumeration_reads_application_directory_through_absolute_module_alias() {
    let (mut m, manager, _fixture) = setup();
    assert_eq!(init(&mut m, manager, "fs:/mod/demo/", false), 0);
    assert_eq!(next(&mut m, manager).as_deref(), Some("fs:/mod/demo/seed.dat"));
    assert_eq!(next(&mut m, manager), None);
}
#[test]
fn enumeration_accepts_directory_created_only_in_save_overlay() {
    let (mut m, manager, _fixture) = setup();
    assert_eq!(init(&mut m, manager, "saves", false), 0);
    assert_eq!(next(&mut m, manager).as_deref(), Some("saves/slot.dat"));
    assert_eq!(next(&mut m, manager), None);
    assert_eq!(call(&mut m, "GetLastError", [manager, 0, 0, 0]), 1);
}
#[test]
fn enumeration_never_falls_back_to_files_after_failed_reinitialization() {
    let (mut m, manager, _fixture) = setup();
    assert_eq!(init(&mut m, manager, "", false), 0);
    assert_eq!(init(&mut m, manager, "missing", false), 1);
    assert_eq!(next(&mut m, manager), None);
}

#[test]
fn enumeration_skips_package_seed_removed_after_initializing_its_queue() {
    let (mut m, manager, fixture) = setup();
    assert_eq!(init(&mut m, manager, "fs:/mod/demo/", false), 0);
    let path = "fs:/mod/demo/seed.dat";
    m.cpu_mut().write_mem(NAME, &[path.as_bytes(), &[0]].concat()).unwrap();
    assert_eq!(call(&mut m, "Remove", [manager, NAME, 0, 0]), 0);
    assert_eq!(
        std::fs::read(fixture.0.join("content/demo/seed.dat")).unwrap(),
        b"seed",
        "Remove oculta o seed sem alterar o pacote"
    );
    assert_eq!(
        next(&mut m, manager),
        None,
        "uma entrada já enfileirada não pode ressuscitar o seed apagado"
    );
}

#[test]
fn enumeration_filters_qx_asset_suffix_in_the_final_path_component() {
    let (mut m, manager, fixture) = setup();
    let data = fixture.0.join("content/demo/data");
    std::fs::create_dir(&data).unwrap();
    for name in ["ruins.qxm", "coin.QXM", "ruins.qxa", "ruins.qxm.bak"] {
        std::fs::write(data.join(name), name).unwrap();
    }
    assert_eq!(init(&mut m, manager, "data\\*.qxm", false), 0);
    let mut names = Vec::new();
    while let Some(name) = next(&mut m, manager) { names.push(name); }
    assert_eq!(names, ["data/coin.QXM", "data/ruins.qxm"]);
    assert_eq!(init(&mut m, manager, "data/*.qxa", false), 0);
    assert_eq!(next(&mut m, manager).as_deref(), Some("data/ruins.qxa"));
    assert_eq!(next(&mut m, manager), None);
}

#[test]
fn filtered_enumeration_preserves_overlay_metadata_and_deleted_resources() {
    let (mut m, manager, fixture) = setup();
    std::fs::write(fixture.0.join("content/demo/removed.qxm"), b"seed").unwrap();
    std::fs::write(fixture.0.join("content/demo/live.qxm"), b"seed").unwrap();
    std::fs::write(fixture.0.join("overlay/live.qxm"), b"modified").unwrap();
    std::fs::write(fixture.0.join("overlay/new.qxm"), b"new").unwrap();
    m.cpu_mut().write_mem(NAME, b"removed.qxm\0").unwrap();
    assert_eq!(call(&mut m, "Remove", [manager, NAME, 0, 0]), 0);
    assert_eq!(init(&mut m, manager, "fs:/mod/demo/*.qxm", false), 0);
    assert_eq!(next(&mut m, manager).as_deref(), Some("fs:/mod/demo/live.qxm"));
    assert_eq!(m.cpu().read_u32(INFO + 8).unwrap(), 8);
    assert_eq!(next(&mut m, manager).as_deref(), Some("fs:/mod/demo/new.qxm"));
    assert_eq!(next(&mut m, manager), None);
}

#[test]
fn asset_filter_does_not_expand_directory_components_or_escape_vfs() {
    let (mut m, manager, fixture) = setup();
    std::fs::write(fixture.0.join("content/demo/seed.qxm"), b"seed").unwrap();
    assert_eq!(init(&mut m, manager, "*.qxm", false), 0);
    assert_eq!(init(&mut m, manager, "*/seed.qxm", false), 1);
    assert_eq!(next(&mut m, manager), None);
    assert_eq!(init(&mut m, manager, "../../../../*.qxm", false), 1);
    assert_eq!(next(&mut m, manager), None);
}

#[test]
fn asset_filter_preserves_directory_only_and_empty_result_semantics() {
    let (mut m, manager, fixture) = setup();
    std::fs::create_dir(fixture.0.join("content/demo/model.qxm")).unwrap();
    std::fs::write(fixture.0.join("content/demo/seed.qxm"), b"seed").unwrap();
    assert_eq!(init(&mut m, manager, "*.qxm", true), 0);
    assert_eq!(next(&mut m, manager).as_deref(), Some("model.qxm"));
    assert_eq!(next(&mut m, manager), None);
    assert_eq!(init(&mut m, manager, "*.qxa", false), 0);
    assert_eq!(next(&mut m, manager), None);
    assert_eq!(call(&mut m, "GetLastError", [manager, 0, 0, 0]), 1);
}

#[test]
fn asset_filter_preserves_the_slash_that_identifies_module_root_aliases() {
    let (mut m, manager, _fixture) = setup();
    for alias in ["~/*.dat", "fs:/~/*.dat", "fs:/~0x01010101/*.dat"] {
        assert_eq!(init(&mut m, manager, alias, false), 0, "{alias}");
        let expected = alias.replace("*.dat", "seed.dat");
        assert_eq!(next(&mut m, manager).as_deref(), Some(expected.as_str()));
        assert_eq!(next(&mut m, manager), None);
    }
}

#[test]
fn asset_filter_preserves_the_device_root_alias() {
    let (mut m, manager, fixture) = setup();
    std::fs::write(fixture.0.join("runtime/aparelho/shared.dat"), b"device").unwrap();
    assert_eq!(init(&mut m, manager, "fs:/*.dat", false), 0);
    assert_eq!(next(&mut m, manager).as_deref(), Some("fs:/shared.dat"));
    assert_eq!(next(&mut m, manager), None);
}
