use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend};
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

const SHELL: u32 = loader::OBJECT_BASE;
const BUFFER: u32 = loader::STACK_BASE + 0x100;
const SIZE: u32 = BUFFER + 32;
const TOTAL: u32 = SIZE + 4;
const FSSPACE_ROOT: u32 = 65;

struct Fixture {
    machine: Machine<BackendPadrao>,
    work: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let work = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests")
            .join(format!(
                "device-storage-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&work).unwrap();
        let storage = StoragePaths::from_root(work.join("runtime"));
        storage.create_dirs().unwrap();
        let image = ModImage::parse(
            [0xe3a0_0000u32, 0xe12f_ff1e]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect(),
        )
        .unwrap();
        let mut machine = Machine::new_with_storage(
            BackendPadrao::new().unwrap(),
            loader::load_with(&image, &[]).unwrap(),
            &work,
            &storage,
            None,
        );
        assert_eq!(machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
        Self { machine, work }
    }

    fn call(&mut self, interface: Interface, name: &str, arguments: [u32; 4]) -> u32 {
        let slot = (0..256)
            .find(|slot| interface.method(*slot) == Some(name))
            .unwrap();
        match self
            .machine
            .call_guest(aee::encode(interface, slot), arguments, 1000)
            .unwrap()
        {
            Outcome::Returned { code } => code,
            outcome => panic!("{name}: {outcome:?}"),
        }
    }

    fn query(&mut self, buffer: u32, capacity: u32) -> u32 {
        self.machine.cpu_mut().write_u32(SIZE, capacity).unwrap();
        self.call(
            Interface::Shell,
            "GetDeviceInfoEx",
            [SHELL, FSSPACE_ROOT, buffer, SIZE],
        )
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let private_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests")
            .canonicalize()
            .unwrap();
        let actual = self.work.canonicalize().unwrap();
        assert!(actual.starts_with(private_root));
        std::fs::remove_dir_all(actual).unwrap();
    }
}

#[test]
fn root_flash_query_supplies_64_bit_total_and_free_space_consistent_with_filemgr() {
    let mut fixture = Fixture::new();
    fixture
        .machine
        .cpu_mut()
        .write_mem(BUFFER, &[0xa5; 24])
        .unwrap();
    assert_eq!(fixture.query(BUFFER, 24), 0);
    let mut bytes = [0; 24];
    fixture.machine.cpu().read_mem(BUFFER, &mut bytes).unwrap();
    let total = u64::from_le_bytes(bytes[..8].try_into().unwrap());
    let free = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    let file_free = fixture.call(Interface::FileMgr, "GetFreeSpace", [0, TOTAL, 0, 0]);
    assert_eq!(
        total,
        u64::from(fixture.machine.cpu().read_u32(TOTAL).unwrap())
    );
    assert_eq!(free, u64::from(file_free));
    // O arranque do Shift compara qwFreeSpace com 20.000 antes de abrir qualquer arquivo.
    assert!(free >= 20_000 && free <= total);
    assert_eq!(
        &bytes[16..],
        &[0xa5; 8],
        "a struct termina após os dois uint64"
    );
}

#[test]
fn root_flash_query_reports_required_size_without_touching_short_buffers() {
    let mut fixture = Fixture::new();
    for capacity in [0, 1, 8, 15] {
        fixture
            .machine
            .cpu_mut()
            .write_mem(BUFFER, &[0xa5; 24])
            .unwrap();
        assert_eq!(fixture.query(BUFFER, capacity), 0);
        assert_eq!(fixture.machine.cpu().read_u32(SIZE).unwrap(), 16);
        let mut bytes = [0; 24];
        fixture.machine.cpu().read_mem(BUFFER, &mut bytes).unwrap();
        assert_eq!(bytes, [0xa5; 24]);
    }
    assert_eq!(fixture.query(0, 0), 0);
    assert_eq!(fixture.machine.cpu().read_u32(SIZE).unwrap(), 16);
}

#[test]
fn root_flash_query_requires_size_and_keeps_unattached_storage_unsupported() {
    let mut fixture = Fixture::new();
    assert_eq!(
        fixture.call(
            Interface::Shell,
            "GetDeviceInfoEx",
            [SHELL, FSSPACE_ROOT, BUFFER, 0]
        ),
        2
    );
    for item in [64, 66, 67] {
        fixture.machine.cpu_mut().write_u32(SIZE, 16).unwrap();
        fixture
            .machine
            .cpu_mut()
            .write_mem(BUFFER, &[0xa5; 16])
            .unwrap();
        assert_eq!(
            fixture.call(
                Interface::Shell,
                "GetDeviceInfoEx",
                [SHELL, item, BUFFER, SIZE]
            ),
            20
        );
        let mut bytes = [0; 16];
        fixture.machine.cpu().read_mem(BUFFER, &mut bytes).unwrap();
        assert_eq!(bytes, [0xa5; 16]);
    }
}
