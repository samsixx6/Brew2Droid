use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend};
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

const NAME: u32 = loader::STACK_BASE + 0x1000;
const OUT: u32 = NAME + 0x400;
const DATA: u32 = NAME + 0x800;

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn call(m: &mut Machine<BackendPadrao>, interface: Interface, name: &str, args: [u32; 4]) -> u32 {
    let slot = (0..256)
        .find(|&slot| interface.method(slot) == Some(name))
        .unwrap();
    match m
        .call_guest(aee::encode(interface, slot), args, 10_000)
        .unwrap()
    {
        Outcome::Returned { code } => code,
        outcome => panic!("{name}: {outcome:?}"),
    }
}

fn setup() -> (Machine<BackendPadrao>, u32, Fixture) {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let fixture = Fixture(std::env::temp_dir().join(format!(
        "brew-file-errors-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::create_dir(&fixture.0).unwrap();
    let module = fixture.0.join("module");
    let overlay = fixture.0.join("overlay");
    std::fs::create_dir(&module).unwrap();
    std::fs::create_dir(&overlay).unwrap();
    std::fs::write(module.join("existing.dat"), b"original").unwrap();
    let storage = StoragePaths::from_root(fixture.0.join("runtime"));
    storage.create_dirs().unwrap();
    let image = ModImage::parse(
        [0xe3a0_0000u32, 0xe12f_ff1e]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect(),
    )
    .unwrap();
    let mut m = Machine::new_with_storage(
        BackendPadrao::new().unwrap(),
        loader::load_with(&image, &[]).unwrap(),
        &module,
        &storage,
        Some(overlay),
    );
    assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
    assert_eq!(
        call(
            &mut m,
            Interface::Shell,
            "CreateInstance",
            [loader::OBJECT_BASE, 0x0100_1003, OUT, 0]
        ),
        0
    );
    let manager = m.cpu().read_u32(OUT).unwrap();
    (m, manager, fixture)
}

fn named(m: &mut Machine<BackendPadrao>, manager: u32, method: &str, path: &str, mode: u32) -> u32 {
    m.cpu_mut()
        .write_mem(NAME, &[path.as_bytes(), &[0]].concat())
        .unwrap();
    call(m, Interface::FileMgr, method, [manager, NAME, mode, 0])
}

fn last(m: &mut Machine<BackendPadrao>, manager: u32) -> u32 {
    call(m, Interface::FileMgr, "GetLastError", [manager, 0, 0, 0])
}

#[test]
fn file_error_missing_default_save_is_enoexists_until_success() {
    let (mut m, manager, _fixture) = setup();
    assert_eq!(named(&mut m, manager, "OpenFile", "scores.dat", 1), 0);
    assert_eq!(last(&mut m, manager), 0x101);
    assert_eq!(
        last(&mut m, manager),
        0x101,
        "a consulta não consome o erro"
    );
    assert_eq!(
        call(&mut m, Interface::FileMgr, "AddRef", [manager, 0, 0, 0]),
        2
    );
    assert_eq!(
        call(&mut m, Interface::FileMgr, "Release", [manager, 0, 0, 0]),
        1
    );
    assert_eq!(
        last(&mut m, manager),
        0x101,
        "referências não são operações de arquivo"
    );
    let file = named(&mut m, manager, "OpenFile", "scores.dat", 4);
    assert_ne!(file, 0, "o caminho de primeiro arranque pode criar o save");
    assert_eq!(last(&mut m, manager), 0);
    assert_eq!(call(&mut m, Interface::File, "Release", [file, 0, 0, 0]), 0);
}

#[test]
fn file_error_distinguishes_bad_namespace_missing_parent_and_missing_file() {
    let (mut m, manager, _fixture) = setup();
    for (path, error) in [
        ("../../outside.dat", 0x103),
        ("", 0x103),
        ("absent-parent/save.dat", 0x109),
        ("absent.dat", 0x101),
    ] {
        assert_eq!(named(&mut m, manager, "OpenFile", path, 1), 0);
        assert_eq!(last(&mut m, manager), error, "{path:?}");
    }
    let file = named(&mut m, manager, "OpenFile", "existing.dat", 1);
    assert_ne!(file, 0);
    assert_eq!(last(&mut m, manager), 0);
}

#[test]
fn file_error_successful_metadata_test_and_directory_operations_clear_failure() {
    let (mut m, manager, _fixture) = setup();
    assert_eq!(named(&mut m, manager, "Test", "absent.dat", 0), 1);
    assert_eq!(last(&mut m, manager), 0x101);
    assert_eq!(named(&mut m, manager, "Test", ".", 0), 0);
    assert_eq!(last(&mut m, manager), 0);
    assert_eq!(named(&mut m, manager, "GetInfo", "absent.dat", OUT), 1);
    assert_eq!(last(&mut m, manager), 0x101);
    assert_eq!(named(&mut m, manager, "GetInfo", "existing.dat", OUT), 0);
    assert_eq!(last(&mut m, manager), 0);
    assert_eq!(named(&mut m, manager, "Remove", "absent.dat", 0), 1);
    assert_eq!(last(&mut m, manager), 0x101);
    assert_eq!(named(&mut m, manager, "MkDir", "new-directory", 0), 0);
    assert_eq!(last(&mut m, manager), 0);
}

#[test]
fn file_error_seek_and_read_write_failures_keep_their_real_causes() {
    let (mut m, manager, _fixture) = setup();
    let file = named(&mut m, manager, "OpenFile", "existing.dat", 1);
    assert_ne!(file, 0);
    assert_ne!(
        call(&mut m, Interface::File, "Seek", [file, 0, u32::MAX, 0]),
        0
    );
    assert_eq!(last(&mut m, manager), 0x104);
    assert_eq!(call(&mut m, Interface::File, "Seek", [file, 0, 0, 0]), 0);
    assert_eq!(last(&mut m, manager), 0);
    m.cpu_mut().write_mem(DATA, b"x").unwrap();
    assert_eq!(
        call(&mut m, Interface::File, "Write", [file, DATA, 1, 0]),
        0
    );
    assert_eq!(
        last(&mut m, manager),
        0x10a,
        "um arquivo somente leitura não é um arquivo ausente"
    );
    assert_eq!(call(&mut m, Interface::File, "Read", [file, DATA, 8, 0]), 8);
    assert_eq!(last(&mut m, manager), 0);
}

#[test]
fn file_error_create_existing_does_not_hide_or_overwrite_the_package_file() {
    let (mut m, manager, fixture) = setup();
    assert_eq!(named(&mut m, manager, "OpenFile", "existing.dat", 4), 0);
    assert_eq!(last(&mut m, manager), 0x100);
    assert_eq!(
        std::fs::read(fixture.0.join("module/existing.dat")).unwrap(),
        b"original"
    );
    assert!(!fixture.0.join("overlay/existing.dat").exists());
}

#[test]
fn file_error_roundtrip_and_helper_preserve_the_applet_scoped_latch() {
    let (mut m, manager, _fixture) = setup();
    assert_eq!(named(&mut m, manager, "OpenFile", "absent.dat", 1), 0);
    assert_eq!(last(&mut m, manager), 0x101);
    let saved = m.grava_estado();
    let file = named(&mut m, manager, "OpenFile", "existing.dat", 1);
    assert_ne!(file, 0);
    assert_eq!(last(&mut m, manager), 0);
    m.restaura_estado(&saved).unwrap();
    assert_eq!(last(&mut m, manager), 0x101);
    assert_eq!(
        call(&mut m, Interface::Helpers, "getlasterror", [0; 4]),
        0x101
    );
}

#[test]
fn file_error_missing_entry_in_an_overlay_only_parent_is_not_a_missing_directory() {
    let (mut m, manager, fixture) = setup();
    let file = named(&mut m, manager, "OpenFile", "udata/first.dat", 4);
    assert_ne!(file, 0);
    assert_eq!(call(&mut m, Interface::File, "Release", [file, 0, 0, 0]), 0);
    assert!(!fixture.0.join("module/udata").exists());
    assert!(fixture.0.join("overlay/udata").is_dir());
    assert_eq!(named(&mut m, manager, "Test", "UDATA", 0), 0);
    for path in [
        "udata/second.dat",
        "UDATA\\second.dat",
        "fs:/~/UDATA/second.dat",
        "\\.\\udata\\second.dat",
    ] {
        assert_eq!(named(&mut m, manager, "OpenFile", path, 1), 0);
        assert_eq!(last(&mut m, manager), 0x101, "{path}");
        for (method, output) in [("Test", 0), ("GetInfo", OUT), ("Remove", 0)] {
            assert_eq!(named(&mut m, manager, method, path, output), 1);
            assert_eq!(last(&mut m, manager), 0x101, "{method} {path}");
        }
    }
    assert_eq!(
        named(
            &mut m,
            manager,
            "OpenFile",
            "udata/absent-parent/third.dat",
            1
        ),
        0
    );
    assert_eq!(last(&mut m, manager), 0x109);
}

#[test]
fn read_only_package_seeds_become_writable_saves_without_changing_the_seed() {
    for mode in [2, 8] {
        for absolute in [false, true] {
            let (mut m, manager, fixture) = setup();
            let seed = fixture.0.join("module/existing.dat");
            let copy = fixture.0.join("overlay/existing.dat");
            let original_permissions = std::fs::metadata(&seed).unwrap().permissions();
            let mut readonly = original_permissions.clone();
            readonly.set_readonly(true);
            std::fs::set_permissions(&seed, readonly).unwrap();
            let path = if absolute { "fs:/mod/module/existing.dat" } else { "existing.dat" };
            let file = named(&mut m, manager, "OpenFile", path, mode);
            let seed_still_readonly = std::fs::metadata(&seed).unwrap().permissions().readonly();
            let copy_is_readonly = std::fs::metadata(&copy).unwrap().permissions().readonly();
            // Restaurar apenas os arquivos da fixture antes de uma asserção que
            // pode falhar: o Windows também recusa apagar arquivos read-only.
            std::fs::set_permissions(&seed, original_permissions.clone()).unwrap();
            std::fs::set_permissions(&copy, original_permissions).unwrap();
            assert!(seed_still_readonly, "a abertura nunca altera as permissões do pacote");
            assert_ne!(file, 0, "{path}, modo {mode}");
            assert!(!copy_is_readonly, "o atributo do pacote não torna o save somente leitura");
            m.cpu_mut().write_mem(DATA, b"X").unwrap();
            assert_eq!(call(&mut m, Interface::File, "Write", [file, DATA, 1, 0]), 1);
            assert_eq!(call(&mut m, Interface::File, "Release", [file, 0, 0, 0]), 0);
            assert_eq!(std::fs::read(&seed).unwrap(), b"original");
            assert_eq!(std::fs::read(&copy).unwrap(), if mode == 8 { b"originalX".as_slice() } else { b"Xriginal".as_slice() });
        }
    }
}
