use std::path::PathBuf;
use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend, Reg},
    handset::types::DisplayGeometry,
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    storage::StoragePaths,
};

const OUT: u32 = loader::STACK_BASE + 0x800;
const TEXT: u32 = OUT + 0x100;
const RECT: u32 = OUT + 0x200;

fn machine() -> (Machine<BackendPadrao>, PathBuf) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local-tests").join(format!(
        "legacy-applet-startup-test-{}-{}", std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
    ));
    std::fs::create_dir_all(&root).unwrap();
    let storage = StoragePaths::from_root(root.join("runtime"));
    storage.create_dirs().unwrap();
    let image = ModImage::parse([0xe3a0_0000u32, 0xe12f_ff1e].into_iter().flat_map(u32::to_le_bytes).collect()).unwrap();
    let mut machine = Machine::new_with_storage_geometry(
        BackendPadrao::new().unwrap(), loader::load_with(&image, &[]).unwrap(), &root, &storage, None,
        DisplayGeometry { width: 120, height: 130, depth: 16 },
    ).unwrap();
    assert_eq!(machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
    (machine, root)
}

fn create(machine: &mut Machine<BackendPadrao>, class: u32) -> u32 {
    let shell = loader::OBJECT_BASE;
    assert_eq!(machine.call_guest(aee::encode(Interface::Shell, 2),
        [shell, class, OUT, 0], 1000).unwrap(), Outcome::Returned { code: 0 },
        "a classe BREW 1.0 {class:#x} deve construir o controle");
    let control = machine.cpu().read_u32(OUT).unwrap();
    assert_ne!(control, 0);
    control
}

fn call(machine: &mut Machine<BackendPadrao>, control: u32, slot: u32, args: &[u32]) -> u32 {
    let table = machine.cpu().read_u32(control).unwrap();
    let function = machine.cpu().read_u32(table + slot * 4).unwrap();
    let saved_sp = machine.cpu().read_reg(Reg::Sp);
    let scratch = loader::STACK_BASE + 0x400;
    for (index, argument) in args.iter().skip(3).enumerate() {
        machine.cpu_mut().write_u32(scratch + index as u32 * 4, *argument).unwrap();
    }
    machine.cpu_mut().write_reg(Reg::Sp, scratch);
    let result = machine.call_guest(function, [control, args.first().copied().unwrap_or(0),
        args.get(1).copied().unwrap_or(0), args.get(2).copied().unwrap_or(0)], 10000).unwrap();
    machine.cpu_mut().write_reg(Reg::Sp, saved_sp);
    let Outcome::Returned { code } = result else { panic!("slot {slot}: {result:?}") };
    code
}

fn check_menu(class: u32, next_key: u32) {
    let (mut machine, root) = machine();
    let menu = create(&mut machine, class);
    // AEEMenu.h preserva os primeiros 26 slots antes dos métodos v1.NNN. Este é o
    // caminho real de LOTR: SetRect(6), AddItem(12), SetSel(17) e HandleEvent(2).
    let rect: Vec<_> = [2i16, 3, 116, 100].into_iter().flat_map(i16::to_le_bytes).collect();
    machine.cpu_mut().write_mem(RECT, &rect).unwrap();
    call(&mut machine, menu, 6, &[RECT]);
    for (id, text, data) in [(11, "Play", 0x11223344), (12, "Options", 0x55667788)] {
        let wide: Vec<_> = text.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect();
        machine.cpu_mut().write_mem(TEXT, &wide).unwrap();
        assert_eq!(call(&mut machine, menu, 12, &[0, 0, id, TEXT, data]), 1);
    }
    call(&mut machine, menu, 17, &[11]);
    assert_eq!(call(&mut machine, menu, 18, &[]), 11);
    call(&mut machine, menu, 19, &[0]);
    call(&mut machine, menu, 4, &[1]);
    assert_eq!(call(&mut machine, menu, 2, &[0x100, next_key, 0]), 1);
    assert_eq!(call(&mut machine, menu, 18, &[]), 12);
    assert_eq!(call(&mut machine, menu, 14, &[12, OUT]), 1);
    assert_eq!(machine.cpu().read_u32(OUT).unwrap(), 0x55667788);
    machine.cpu_mut().write_mem(OUT, &[0xa5; 16]).unwrap();
    call(&mut machine, menu, 7, &[OUT]);
    let mut actual = [0; 16];
    machine.cpu().read_mem(OUT, &mut actual).unwrap();
    assert_eq!(&actual[..8], rect);
    assert_eq!(&actual[8..], &[0xa5; 8]);
    let second = create(&mut machine, class);
    assert_eq!(call(&mut machine, second, 18, &[]), 0, "cada controle guarda seus itens");
    assert!(machine.unknown_classes().is_empty());
    drop(machine);
    assert!(root.canonicalize().unwrap().starts_with(PathBuf::from(env!("CARGO_MANIFEST_DIR")).canonicalize().unwrap()));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn brew_10_menu_constructor_preserves_the_original_vtable_prefix() { check_menu(0x0100_3000, 0xe032); }

#[test]
fn brew_10_softkey_constructor_preserves_the_original_vtable_prefix() { check_menu(0x0100_3001, 0xe034); }



#[test]
fn brew_10_static_constructor_keeps_set_text_in_the_original_slot() {
    let (mut machine, root) = machine();
    let control = create(&mut machine, 0x0100_100a);
    let rect: Vec<_> = [0i16, 0, 120, 100].into_iter().flat_map(i16::to_le_bytes).collect();
    machine.cpu_mut().write_mem(RECT, &rect).unwrap();
    call(&mut machine, control, 6, &[RECT]);
    for (pointer, text) in [(TEXT, "Help"), (TEXT + 0x40, "Choose a character")] {
        let wide: Vec<_> = text.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect();
        machine.cpu_mut().write_mem(pointer, &wide).unwrap();
    }
    // AEEShell.h: o prefixo 1.0 termina em SetText(11); SetFont(12) é extensão 1.10.
    assert_eq!(call(&mut machine, control, 11, &[TEXT, TEXT + 0x40, 0x8001, 0x8000]), 1);
    assert_eq!(call(&mut machine, control, 3, &[]), 1);
    assert!(machine.screen().escritas() > 0);
    assert!(machine.drawn_text().any(|row| row.3 == "Choose a character"));
    assert!(machine.unknown_classes().is_empty());
    drop(machine);
    assert!(root.canonicalize().unwrap().starts_with(PathBuf::from(env!("CARGO_MANIFEST_DIR")).canonicalize().unwrap()));
    std::fs::remove_dir_all(root).unwrap();
}
