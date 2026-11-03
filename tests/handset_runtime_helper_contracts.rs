//! Contratos de memória observados nos chamadores ARM e no AEEShell.h do SDK.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend};
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

const BUFFER: u32 = loader::STACK_BASE + 0x100;

struct Fixture<C: CpuBackend = BackendPadrao> {
    machine: Machine<C>,
    work: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        Self::with_cpu(BackendPadrao::new().unwrap())
    }
}

impl<C: CpuBackend> Fixture<C> {
    fn with_cpu(cpu: C) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let work = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests/startup-repairs-20261029/faults")
            .join(format!(
                "contract-{}-{}",
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
            cpu,
            loader::load_with(&image, &[]).unwrap(),
            &work,
            &storage,
            None,
        );
        assert_eq!(machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
        Self { machine, work }
    }

    fn device_info(&mut self) {
        let slot = (0..256)
            .find(|slot| Interface::Shell.method(*slot) == Some("GetDeviceInfo"))
            .unwrap();
        assert_eq!(
            self.machine
                .call_guest(
                    aee::encode(Interface::Shell, slot),
                    [loader::OBJECT_BASE, BUFFER, 0, 0],
                    1000,
                )
                .unwrap(),
            Outcome::Returned { code: 0 }
        );
    }

    fn bytes(&self, address: u32, size: usize) -> Vec<u8> {
        let mut output = vec![0; size];
        self.machine.cpu().read_mem(address, &mut output).unwrap();
        output
    }
}

impl<C: CpuBackend> Drop for Fixture<C> {
    fn drop(&mut self) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests/startup-repairs-20261029/faults")
            .canonicalize()
            .unwrap();
        let work = self.work.canonicalize().unwrap();
        assert!(work.starts_with(root));
        std::fs::remove_dir_all(work).unwrap();
    }
}

#[test]
fn device_info_accepts_the_sdk_structure_size_without_writing_past_it() {
    let mut f = Fixture::new();
    f.machine.cpu_mut().write_mem(BUFFER, &[0xa5; 80]).unwrap();
    f.machine
        .cpu_mut()
        .write_mem(BUFFER + 44, &64u16.to_le_bytes())
        .unwrap();
    f.device_info();
    assert_eq!(f.bytes(BUFFER + 44, 2), 64u16.to_le_bytes());
    assert_ne!(
        f.machine.cpu().read_u32(BUFFER + 56).unwrap() & 0xffff,
        0,
        "wMaxPath é parte da estrutura de 64 bytes solicitada"
    );
    assert_eq!(f.bytes(BUFFER + 64, 16), [0xa5; 16]);
}

#[test]
fn device_info_does_not_treat_an_unsupported_structure_size_as_an_extension_request() {
    let mut f = Fixture::new();
    // AEEShell.h, ISHELL_GetDeviceInfo(): os campos estendidos exigem wStructSize
    // igual a sizeof(AEEDeviceInfo). Chamadores antigos deixam o campo indefinido. Aceitar todo
    // valor >=64 como pedido estendido apaga o r4 salvo logo após o buffer de 60 bytes.
    f.machine.cpu_mut().write_mem(BUFFER, &[0xa5; 80]).unwrap();
    f.machine
        .cpu_mut()
        .write_mem(BUFFER + 44, &u16::MAX.to_le_bytes())
        .unwrap();
    let extension_before = f.bytes(BUFFER + 44, 36);
    f.device_info();
    assert_ne!(
        f.machine.cpu().read_u32(BUFFER).unwrap(),
        0xa5a5_a5a5,
        "a parte legada ainda recebe a geometria"
    );
    assert_eq!(
        f.bytes(BUFFER + 44, 36),
        extension_before,
        "um tamanho desconhecido não autoriza escrever sobre a pilha do chamador"
    );
}

#[test]
fn shell_class_factory_returns_a_functional_shell_interface() {
    let mut f = Fixture::new();
    let create = (0..256)
        .find(|slot| Interface::Shell.method(*slot) == Some("CreateInstance"))
        .unwrap();
    assert_eq!(
        f.machine
            .call_guest(
                aee::encode(Interface::Shell, create),
                [loader::OBJECT_BASE, 0x0100_1000, BUFFER + 96, 0],
                1000,
            )
            .unwrap(),
        Outcome::Returned { code: 0 },
        "AEEClassIDs.h: AEECLSID_SHELL é uma classe nativa, não uma extensão ausente"
    );
    let shell = f.machine.cpu().read_u32(BUFFER + 96).unwrap();
    assert_ne!(shell, 0);
    let vtable = f.machine.cpu().read_u32(shell).unwrap();
    let get_info = f.machine.cpu().read_u32(vtable + 4 * 4).unwrap();
    assert_eq!(
        f.machine
            .call_guest(get_info, [shell, BUFFER, 0, 0], 1000)
            .unwrap(),
        Outcome::Returned { code: 0 }
    );
    assert_ne!(
        f.machine.cpu().read_u32(BUFFER).unwrap(),
        0,
        "a interface criada precisa executar GetDeviceInfo pela própria vtable"
    );
}

fn wide_copy(f: &mut Fixture, destination: u32, capacity: u32, source: u32, limit: u32) -> u32 {
    let slot = (0..256)
        .find(|&slot| Interface::Helpers.method(slot) == Some("wstrncopyn"))
        .unwrap();
    match f
        .machine
        .call_guest(
            aee::encode(Interface::Helpers, slot),
            [destination, capacity, source, limit],
            10000,
        )
        .unwrap()
    {
        Outcome::Returned { code } => code,
        other => panic!("WSTRNCOPYN: {other:?}"),
    }
}

#[test]
fn wide_copy_preserves_raw_aechar_words_in_unicode_and_native_contexts() {
    for native in [false, true] {
        let mut f = Fixture::new();
        if native {
            assert!(f.machine.set_handset_runtime_identity(
                "8622d31bcb9b5649a14b7dd5894e6e1902c1a9f2f3281984a8dc138c9f814b9e",
                "afce4811ad7c04b7487bc9657b81d8f2c81285114f6413cbbc1b21d2cf995cde"
            ));
        }
        // AECHAR é uma unidade de 16 bits. Uma cópia não valida Unicode nem
        // substitui D9B4 (bytes B4 D9, '다' no contexto nativo) por FFFD.
        let words = [
            0xd9b4u16, 0xeebf, 0xceb7, 0xe5b5, 0x20, 0xd800, 0xdc00, 0xffff, 0x41, 0,
        ];
        let source = BUFFER + 0x100;
        let destination = BUFFER + 0x200;
        let bytes = words
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        f.machine.cpu_mut().write_mem(source, &bytes).unwrap();
        f.machine
            .cpu_mut()
            .write_mem(destination - 2, &[0xa5; 30])
            .unwrap();
        assert_eq!(wide_copy(&mut f, destination, 14, source, u32::MAX), 9);
        assert_eq!(f.bytes(destination, bytes.len()), bytes);
        assert_eq!(f.bytes(destination - 2, 2), [0xa5; 2]);
        assert_eq!(f.bytes(destination + 20, 8), [0xa5; 8]);
        assert_eq!(f.bytes(source, 20), bytes);
    }
}

#[test]
fn wide_copy_counts_aechar_units_and_bounds_the_terminator_without_padding() {
    let mut f = Fixture::new();
    let source = BUFFER + 0x100;
    let destination = BUFFER + 0x200;
    let units = [0xd9b4u16, 0xeebf, 0x20, 0x41, 0x42, 0];
    f.machine
        .cpu_mut()
        .write_mem(
            source,
            &units
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    for (capacity, limit, count) in [
        (0u32, u32::MAX, 0),
        (1, u32::MAX, 0),
        (3, u32::MAX, 2),
        (5, 1, 1),
        (5, 0, 0),
        (5, 4, 4),
        (8, 99, 5),
    ] {
        f.machine
            .cpu_mut()
            .write_mem(destination - 2, &[0xa5; 24])
            .unwrap();
        assert_eq!(
            wide_copy(&mut f, destination, capacity, source, limit),
            count as u32
        );
        let mut expected = vec![0xa5; 24];
        if capacity != 0 {
            let copied = units[..count]
                .iter()
                .copied()
                .chain([0])
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>();
            expected[2..2 + copied.len()].copy_from_slice(&copied);
        }
        assert_eq!(
            f.bytes(destination - 2, 24),
            expected,
            "cbDest={capacity}, lenSource={limit}: terminador dentro da capacidade, sem zerar a cauda"
        );
    }
}

fn wide_family_api<C: CpuBackend>(f: &mut Fixture<C>, name: &str, args: [u32; 4]) -> u32 {
    let slot = (0..256)
        .find(|&slot| Interface::Helpers.method(slot) == Some(name))
        .unwrap();
    match f
        .machine
        .call_guest(aee::encode(Interface::Helpers, slot), args, 10000)
        .unwrap()
    {
        Outcome::Returned { code } => code,
        other => panic!("{name}: {other:?}"),
    }
}

#[test]
fn wide_family_standard_copy_and_append_preserve_units_and_return_destination() {
    for native in [false, true] {
        let mut f = Fixture::new();
        if native {
            assert!(f.machine.set_handset_runtime_identity(
                "8622d31bcb9b5649a14b7dd5894e6e1902c1a9f2f3281984a8dc138c9f814b9e",
                "afce4811ad7c04b7487bc9657b81d8f2c81285114f6413cbbc1b21d2cf995cde"
            ));
        }
        let source = BUFFER + 0x100;
        let destination = BUFFER + 0x200;
        let source_units = [0xd9b4u16, 0xeebf, 0xd800, 0x41, 0];
        let source_bytes = source_units
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        f.machine
            .cpu_mut()
            .write_mem(source, &source_bytes)
            .unwrap();
        for name in ["wstrcpy", "wstrcat"] {
            f.machine
                .cpu_mut()
                .write_mem(destination - 2, &[0xa5; 32])
                .unwrap();
            let prefix = [0x42u16, 0xdc00];
            f.machine
                .cpu_mut()
                .write_mem(
                    destination,
                    &prefix
                        .into_iter()
                        .chain([0])
                        .flat_map(u16::to_le_bytes)
                        .collect::<Vec<_>>(),
                )
                .unwrap();
            assert_eq!(
                wide_family_api(&mut f, name, [destination, source, 0, 0]),
                destination
            );
            let expected_units = if name == "wstrcat" {
                prefix.into_iter().chain(source_units).collect::<Vec<_>>()
            } else {
                source_units.to_vec()
            };
            let expected = expected_units
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>();
            assert_eq!(
                f.bytes(destination, expected.len()),
                expected,
                "{name}: cópia de AECHAR não normaliza Unicode"
            );
            assert_eq!(f.bytes(destination - 2, 2), [0xa5; 2]);
            assert_eq!(
                f.bytes(destination + expected.len() as u32, 32 - 2 - expected.len()),
                vec![0xa5; 32 - 2 - expected.len()]
            );
            assert_eq!(f.bytes(source, source_bytes.len()), source_bytes);
        }
    }
}

#[test]
fn wide_family_bsd_copy_returns_full_source_length_and_preserves_truncated_units() {
    let mut f = Fixture::new();
    let source = BUFFER + 0x100;
    let destination = BUFFER + 0x200;
    let units = [0xd9b4u16, 0xeebf, 0xdc00, 0x41, 0];
    f.machine
        .cpu_mut()
        .write_mem(
            source,
            &units
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    for capacity in [0u32, 1, 3, 5] {
        f.machine
            .cpu_mut()
            .write_mem(destination - 2, &[0xa5; 24])
            .unwrap();
        assert_eq!(
            wide_family_api(&mut f, "wstrlcpy", [destination, source, capacity, 0]),
            4
        );
        let mut expected = vec![0xa5; 24];
        if capacity != 0 {
            let count = ((capacity - 1) as usize).min(4);
            let copied = units[..count]
                .iter()
                .copied()
                .chain([0])
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>();
            expected[2..2 + copied.len()].copy_from_slice(&copied);
        }
        assert_eq!(f.bytes(destination - 2, 24), expected);
    }
    assert_eq!(
        wide_family_api(&mut f, "wstrlcpy", [0x6000_0000, source, 0, 0]),
        4,
        "tamanho zero não acessa o destino, mas conta a origem inteira"
    );
}

#[test]
fn wide_family_bsd_append_bounds_destination_scan_and_reports_attempted_length() {
    let mut f = Fixture::new();
    let source = BUFFER + 0x100;
    let destination = BUFFER + 0x200;
    let units = [0xd9b4u16, 0xeebf, 0xdc00, 0x41, 0];
    f.machine
        .cpu_mut()
        .write_mem(
            source,
            &units
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    assert_eq!(
        wide_family_api(&mut f, "wstrlcat", [0x6000_0000, source, 0, 0]),
        4
    );
    for capacity in [1u32, 2, 4, 6] {
        f.machine
            .cpu_mut()
            .write_mem(destination - 2, &[0xa5; 24])
            .unwrap();
        f.machine
            .cpu_mut()
            .write_mem(destination, &[0x42, 0, 0, 0])
            .unwrap();
        assert_eq!(
            wide_family_api(&mut f, "wstrlcat", [destination, source, capacity, 0]),
            5
        );
        let mut expected = vec![0xa5; 24];
        expected[2..6].copy_from_slice(&[0x42, 0, 0, 0]);
        if capacity > 1 {
            let count = (capacity as usize - 2).min(4);
            let copied = units[..count]
                .iter()
                .copied()
                .chain([0])
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>();
            expected[4..4 + copied.len()].copy_from_slice(&copied);
        }
        assert_eq!(f.bytes(destination - 2, 24), expected, "nSize={capacity}");
    }
    // Sem NUL dentro de nSize: strlcat devolve nSize + strlen(src), sem tocar dst.
    f.machine
        .cpu_mut()
        .write_mem(destination - 2, &[0xa5; 24])
        .unwrap();
    let before = f.bytes(destination - 2, 24);
    assert_eq!(
        wide_family_api(&mut f, "wstrlcat", [destination, source, 3, 0]),
        7
    );
    assert_eq!(f.bytes(destination - 2, 24), before);
}

fn wide_compare_units(f: &mut Fixture, name: &str, left: &[u16], right: &[u16], n: u32) -> i32 {
    for (address, units) in [(BUFFER + 0x100, left), (BUFFER + 0x200, right)] {
        f.machine
            .cpu_mut()
            .write_mem(
                address,
                &units
                    .iter()
                    .copied()
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
    }
    wide_family_api(f, name, [BUFFER + 0x100, BUFFER + 0x200, n, 0]) as i32
}

#[test]
fn wide_compare_lexicographic_order_uses_aechar_units_without_unicode_replacement() {
    let mut f = Fixture::new();
    // AEEStdLib.h WSTRCMP/WSTRNCMP: ordem lexicográfica de AECHAR, não de
    // escalares Unicode. D9B4 e D9B5 são distintos mesmo sendo surrogates isolados.
    for name in ["wstrcmp", "wstrncmp"] {
        for (left, right) in [
            (vec![0xd9b4, 0], vec![0xd9b5, 0]),
            (vec![0xd800, 0xdc00, 0], vec![0xe000, 0]),
            (vec![0x41, 0], vec![0x41, 0xd9b4, 0]),
        ] {
            assert!(
                wide_compare_units(&mut f, name, &left, &right, 8) < 0,
                "{name}: {left:x?} < {right:x?}"
            );
            assert!(
                wide_compare_units(&mut f, name, &right, &left, 8) > 0,
                "{name}: {right:x?} > {left:x?}"
            );
            assert_eq!(wide_compare_units(&mut f, name, &left, &left, 8), 0);
        }
    }
}

#[test]
fn wide_compare_prefix_length_counts_aechar_and_stops_before_the_next_unit() {
    let mut f = Fixture::new();
    assert!(wide_compare_units(&mut f, "wstrncmp", &[0x4e00, 0], &[0x4e01, 0], 1) < 0);
    assert_eq!(
        wide_compare_units(
            &mut f,
            "wstrncmp",
            &[0x4e00, 0x42, 0],
            &[0x4e00, 0x43, 0],
            1
        ),
        0
    );
    assert!(
        wide_compare_units(
            &mut f,
            "wstrncmp",
            &[0x4e00, 0x42, 0],
            &[0x4e00, 0x43, 0],
            2
        ) < 0
    );
    // O prefixo pedido não precisa conter NUL; diferenças posteriores são irrelevantes.
    assert_eq!(
        wide_compare_units(
            &mut f,
            "wstrncmp",
            &[0xd800, 0xdc00, 0x41],
            &[0xd800, 0xdc00, 0x42],
            2
        ),
        0
    );
    assert!(
        wide_compare_units(
            &mut f,
            "wstrncmp",
            &[0xd800, 0xdc00, 0x41],
            &[0xd800, 0xdc00, 0x42],
            3
        ) < 0
    );
}

#[test]
fn wide_compare_null_equals_empty_and_zero_length_accepts_unused_pointers() {
    let mut f = Fixture::new();
    f.machine.cpu_mut().write_mem(BUFFER, &[0, 0]).unwrap();
    for name in ["wstrcmp", "wstrncmp"] {
        assert_eq!(wide_family_api(&mut f, name, [0, BUFFER, 8, 0]), 0);
        assert_eq!(wide_family_api(&mut f, name, [BUFFER, 0, 8, 0]), 0);
    }
    assert_eq!(
        wide_family_api(&mut f, "wstrncmp", [0x6000_0000, 0x7000_0000, 0, 0]),
        0
    );
}

struct WideReadCpu {
    inner: BackendPadrao,
    reads: std::cell::RefCell<Vec<(u32, usize)>>,
}

impl CpuBackend for WideReadCpu {
    fn reset(&mut self, memory: &zeebx::cpu::mem::GuestMemory) -> Result<(), zeebx::cpu::CpuError> {
        self.inner.reset(memory)
    }
    fn read_reg(&self, reg: zeebx::cpu::Reg) -> u32 {
        self.inner.read_reg(reg)
    }
    fn write_reg(&mut self, reg: zeebx::cpu::Reg, value: u32) {
        self.inner.write_reg(reg, value)
    }
    fn run(
        &mut self,
        pc: u32,
        budget: u64,
    ) -> Result<zeebx::cpu::StopReason, zeebx::cpu::CpuError> {
        self.inner.run(pc, budget)
    }
    fn instructions(&self) -> u64 {
        self.inner.instructions()
    }
    fn cpsr(&self) -> u32 {
        self.inner.cpsr()
    }
    fn set_cpsr(&mut self, value: u32) {
        self.inner.set_cpsr(value)
    }
    fn set_instructions(&mut self, value: u64) {
        self.inner.set_instructions(value)
    }
    fn em_thumb(&self) -> bool {
        self.inner.em_thumb()
    }
    fn read_mem(&self, address: u32, output: &mut [u8]) -> Result<(), zeebx::cpu::CpuError> {
        self.reads.borrow_mut().push((address, output.len()));
        self.inner.read_mem(address, output)
    }
    fn write_mem(&mut self, address: u32, input: &[u8]) -> Result<(), zeebx::cpu::CpuError> {
        self.inner.write_mem(address, input)
    }
}

#[test]
fn wide_compare_bounded_and_zero_length_do_not_read_unused_aechar_memory() {
    let mut f = Fixture::with_cpu(WideReadCpu {
        inner: BackendPadrao::new().unwrap(),
        reads: Default::default(),
    });
    let left = BUFFER + 0x100;
    let right = BUFFER + 0x200;
    for address in [left, right] {
        // Sem terminador dentro do prefixo solicitado.
        f.machine
            .cpu_mut()
            .write_mem(address, &[0x00, 0x4e, 0x00, 0x4e, 0xb4, 0xd9])
            .unwrap();
    }
    f.machine.cpu().reads.borrow_mut().clear();
    assert_eq!(wide_family_api(&mut f, "wstrncmp", [left, right, 2, 0]), 0);
    for &(address, size) in f.machine.cpu().reads.borrow().iter() {
        for base in [left, right] {
            if address >= base && address < base + 0x80 {
                assert!(
                    address as u64 + size as u64 <= base as u64 + 4,
                    "leitura além de dois AECHAR: {address:x}+{size}"
                );
            }
        }
    }
    f.machine.cpu().reads.borrow_mut().clear();
    assert_eq!(
        wide_family_api(&mut f, "wstrncmp", [0x6000_0000, 0x7000_0000, 0, 0]),
        0
    );
    assert!(
        !f.machine
            .cpu()
            .reads
            .borrow()
            .iter()
            .any(|&(address, _)| address == 0x6000_0000 || address == 0x7000_0000),
        "n=0 não consulta nenhuma origem"
    );
}
