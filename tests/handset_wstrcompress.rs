use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend};
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};

fn machine() -> Machine<BackendPadrao> {
    let image = ModImage::parse([0xe3a00000u32.to_le_bytes(), 0xe12fff1eu32.to_le_bytes()].concat()).unwrap();
    let module = loader::load_with(&image, &[]).unwrap();
    let mut machine = Machine::new(BackendPadrao::new().unwrap(), module, ".");
    assert_eq!(machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
    machine
}

fn compress(machine: &mut Machine<BackendPadrao>, source: u32, count: u32, dest: u32, size: u32) {
    let result = machine.call_guest(aee::encode(Interface::Helpers, 40), [source, count, dest, size], 1000).unwrap();
    assert!(matches!(result, Outcome::Returned { .. }), "{result:?}");
}

#[test]
fn legacy_wstrcompress_keeps_native_double_byte_units_and_honors_count_and_nul() {
    let mut machine = machine();
    let source = loader::HEAP_BASE + 0x100;
    let dest = source + 0x100;
    // O SDK comprime somente ASCII; as outras unidades conservam seus dois bytes.
    let units = [0x41u16, 0x82a0, 0x42, 0, 0x43];
    machine.cpu_mut().write_mem(source, &units.into_iter().flat_map(u16::to_le_bytes).collect::<Vec<_>>()).unwrap();
    machine.cpu_mut().write_mem(dest, &[0xcc; 12]).unwrap();
    compress(&mut machine, source, 5, dest, 12);
    let mut actual = [0; 12];
    machine.cpu().read_mem(dest, &mut actual).unwrap();
    assert_eq!(actual, [b'A', 0xa0, 0x82, b'B', 0, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc]);
    machine.cpu_mut().write_mem(dest, &[0xcc; 12]).unwrap();
    compress(&mut machine, source, 1, dest, 12);
    machine.cpu().read_mem(dest, &mut actual).unwrap();
    assert_eq!(actual[..4], [b'A', 0, 0xcc, 0xcc]);
}

#[test]
fn legacy_wstrcompress_never_splits_a_unit_or_overwrites_destination_guards() {
    let mut machine = machine();
    let source = loader::HEAP_BASE + 0x100;
    let dest = source + 0x100;
    machine.cpu_mut().write_mem(source, &[0xa0, 0x82, 0, 0]).unwrap();
    for (size, expected) in [(0, [0xcc, 0xcc, 0xcc, 0xcc]), (1, [0, 0xcc, 0xcc, 0xcc]),
        (2, [0, 0xcc, 0xcc, 0xcc]), (3, [0xa0, 0x82, 0, 0xcc])] {
        machine.cpu_mut().write_mem(dest, &[0xcc; 4]).unwrap();
        compress(&mut machine, source, 1, dest, size);
        let mut actual = [0; 4]; machine.cpu().read_mem(dest, &mut actual).unwrap();
        assert_eq!(actual, expected, "size={size}");
    }
    // Destino vazio não exige ler uma origem inválida.
    compress(&mut machine, 0xfffffff0, 10, 0xfffffff0, 0);
}
