use std::path::PathBuf;

use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend, Reg};
use zeebx::handset::types::DisplayGeometry;
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::save_state::{self, Leitor};
use zeebx::storage::StoragePaths;

const MENU: u32 = 0x0100_3100;
const SOFTKEY: u32 = 0x0100_3101;
const TEXT: u32 = 0x0100_3009;
const STATIC: u32 = 0x0100_110a;
const OUT: u32 = loader::STACK_BASE + 0x800;
const RECT: u32 = OUT + 0x40;
const WIDE: u32 = OUT + 0x100;
const OTHER: u32 = OUT + 0x200;
const APPLET: u32 = loader::HEAP_BASE + 0x20000;

fn machine() -> (Machine<BackendPadrao>, PathBuf) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("local-tests")
        .join(format!(
            "legacy-controls-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
    std::fs::create_dir_all(&root).unwrap();
    let storage = StoragePaths::from_root(root.join("runtime"));
    storage.create_dirs().unwrap();
    let image = ModImage::parse(
        [0xe3a0_0000u32, 0xe12f_ff1e]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect(),
    )
    .unwrap();
    let mut machine = Machine::new_with_storage_geometry(
        BackendPadrao::new().unwrap(),
        loader::load_with(&image, &[]).unwrap(),
        &root,
        &storage,
        None,
        DisplayGeometry {
            width: 240,
            height: 320,
            depth: 16,
        },
    )
    .unwrap();
    machine.enable_handset_clock();
    assert_eq!(machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
    (machine, root)
}

fn create(machine: &mut Machine<BackendPadrao>, class: u32) -> u32 {
    let slot = (0..256)
        .find(|slot| Interface::Shell.method(*slot) == Some("CreateInstance"))
        .unwrap();
    assert_eq!(
        machine
            .call_guest(
                aee::encode(Interface::Shell, slot),
                [loader::OBJECT_BASE, class, OUT, 0],
                1000
            )
            .unwrap(),
        Outcome::Returned { code: 0 },
        "classe BREW legada {class:#x}"
    );
    let object = machine.cpu().read_u32(OUT).unwrap();
    assert_ne!(object, 0);
    object
}

fn raw_call(machine: &mut Machine<BackendPadrao>, object: u32, slot: u32, args: &[u32]) -> Outcome {
    let vtable = machine.cpu().read_u32(object).unwrap();
    let function = machine.cpu().read_u32(vtable + slot * 4).unwrap();
    let sp = machine.cpu().read_reg(Reg::Sp);
    let scratch = loader::STACK_BASE + 0x400;
    for (index, value) in args.iter().skip(3).enumerate() {
        machine
            .cpu_mut()
            .write_u32(scratch + index as u32 * 4, *value)
            .unwrap();
    }
    machine.cpu_mut().write_reg(Reg::Sp, scratch);
    let arguments = [
        object,
        args.first().copied().unwrap_or(0),
        args.get(1).copied().unwrap_or(0),
        args.get(2).copied().unwrap_or(0),
    ];
    let result = machine.call_guest(function, arguments, 10000).unwrap();
    machine.cpu_mut().write_reg(Reg::Sp, sp);
    result
}

fn call(machine: &mut Machine<BackendPadrao>, object: u32, slot: u32, args: &[u32]) -> u32 {
    match raw_call(machine, object, slot, args) {
        Outcome::Returned { code } => code,
        other => panic!("slot {slot}: {other:?}"),
    }
}

fn write_wide(machine: &mut Machine<BackendPadrao>, pointer: u32, text: &str) {
    let bytes = text
        .encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    machine.cpu_mut().write_mem(pointer, &bytes).unwrap();
}

fn read_wide(machine: &Machine<BackendPadrao>, pointer: u32) -> String {
    let mut units = Vec::new();
    for index in 0..100 {
        let mut bytes = [0; 2];
        machine
            .cpu()
            .read_mem(pointer + index * 2, &mut bytes)
            .unwrap();
        let unit = u16::from_le_bytes(bytes);
        if unit == 0 {
            return String::from_utf16_lossy(&units);
        }
        units.push(unit);
    }
    panic!("texto sem terminador");
}

fn cleanup(machine: Machine<BackendPadrao>, root: PathBuf) {
    drop(machine);
    assert!(
        root.canonicalize().unwrap().starts_with(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .canonicalize()
                .unwrap()
        )
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_controls_keep_separate_state_and_write_exactly_one_aeerect() {
    let (mut machine, root) = machine();
    let rect = [3i16, 4, 120, 90]
        .into_iter()
        .flat_map(i16::to_le_bytes)
        .collect::<Vec<_>>();
    for class in [MENU, SOFTKEY, TEXT, STATIC] {
        let object = create(&mut machine, class);
        assert_eq!(call(&mut machine, object, 5, &[]), 0);
        call(&mut machine, object, 8, &[0x11234]);
        assert_eq!(call(&mut machine, object, 9, &[]), 0x11234);
        machine.cpu_mut().write_mem(RECT, &rect).unwrap();
        call(&mut machine, object, 6, &[RECT]);
        machine.cpu_mut().write_mem(OTHER, &[0xa5; 16]).unwrap();
        call(&mut machine, object, 7, &[OTHER]);
        let mut actual = [0; 16];
        machine.cpu().read_mem(OTHER, &mut actual).unwrap();
        assert_eq!(&actual[..8], rect);
        assert_eq!(&actual[8..], &[0xa5; 8], "GetRect não pode gravar 16 bytes");
        let second = create(&mut machine, class);
        assert_eq!(call(&mut machine, second, 5, &[]), 0);
        assert_ne!(call(&mut machine, second, 9, &[]), 0x11234);
    }
    cleanup(machine, root);
}

#[test]
fn menu_stores_items_navigates_once_and_dispatches_exact_command_payload() {
    let (mut machine, root) = machine();
    let menu = create(&mut machine, MENU);
    write_wide(&mut machine, WIDE, "Play");
    assert_eq!(
        call(&mut machine, menu, 12, &[0, 0, 41, WIDE, 0x12345678]),
        1
    );
    write_wide(&mut machine, WIDE, "Options");
    assert_eq!(
        call(&mut machine, menu, 12, &[0, 0, 73, WIDE, 0xabcdef01]),
        1
    );
    assert_eq!(call(&mut machine, menu, 26, &[]), 2);
    assert_eq!(call(&mut machine, menu, 27, &[1]), 73);
    assert_eq!(call(&mut machine, menu, 14, &[73, OUT]), 1);
    assert_eq!(machine.cpu().read_u32(OUT).unwrap(), 0xabcdef01);
    assert_eq!(call(&mut machine, menu, 18, &[]), 41);
    assert_eq!(
        call(&mut machine, menu, 2, &[0x100, 0xe032, 0]),
        0,
        "menu inativo não navega"
    );
    call(&mut machine, menu, 4, &[1]);
    assert_eq!(call(&mut machine, menu, 2, &[0x101, 0xe032, 0]), 0);
    assert_eq!(call(&mut machine, menu, 18, &[]), 41);
    assert_eq!(call(&mut machine, menu, 2, &[0x100, 0xe032, 0]), 1);
    assert_eq!(call(&mut machine, menu, 18, &[]), 73);
    assert_eq!(call(&mut machine, menu, 2, &[0x102, 0xe032, 0]), 0);
    assert_eq!(call(&mut machine, menu, 18, &[]), 73);
    // Tratador ARM registra evento, wParam e dwParam sem nenhum mock.
    let handler = loader::MODULE_BASE + 0x20;
    let code = [
        0xe5801004u32,
        0xe5802008,
        0xe580300c,
        0xe5901010,
        0xe2811001,
        0xe5801010,
        0xe3a00001,
        0xe12fff1e,
    ];
    machine
        .cpu_mut()
        .write_mem(
            handler,
            &code
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    machine.cpu_mut().write_u32(APPLET, APPLET + 0x100).unwrap();
    machine
        .cpu_mut()
        .write_u32(APPLET + 0x108, handler)
        .unwrap();
    assert_eq!(
        machine.start_applet(APPLET, 0x01017777, 1000).unwrap(),
        Outcome::Returned { code: 1 }
    );
    machine.cpu_mut().write_mem(APPLET + 4, &[0; 16]).unwrap();
    assert_eq!(call(&mut machine, menu, 2, &[0x101, 0xe035, 0]), 0);
    assert_eq!(call(&mut machine, menu, 2, &[0x100, 0xe035, 0]), 1);
    assert_eq!(machine.cpu().read_u32(APPLET + 4).unwrap(), 0x200);
    assert_eq!(machine.cpu().read_u32(APPLET + 8).unwrap(), 73);
    assert_eq!(machine.cpu().read_u32(APPLET + 12).unwrap(), 0xabcdef01);
    assert_eq!(call(&mut machine, menu, 2, &[0x102, 0xe035, 0]), 0);
    assert_eq!(machine.cpu().read_u32(APPLET + 16).unwrap(), 1);
    call(&mut machine, menu, 10, &[]);
    assert_eq!(call(&mut machine, menu, 26, &[]), 0);
    assert_eq!(call(&mut machine, menu, 5, &[]), 0);
    cleanup(machine, root);
}

#[test]
fn softkey_loads_resource_text_and_selects_items_by_physical_softkey() {
    let (mut machine, root) = machine();
    // Um BAR de uma string: cabeçalho32, índice8, tabela8, seção com marcador Latin-1.
    let mut bar = vec![0u8; 48];
    bar[..2].copy_from_slice(&0x11u16.to_le_bytes());
    bar[6..8].copy_from_slice(&1u16.to_le_bytes());
    for (at, value) in [
        (8, 32u32),
        (12, 8),
        (16, 40),
        (20, 1),
        (24, 48),
        (40, 48),
        (44, 56),
    ] {
        bar[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    bar[32..40].copy_from_slice(&[1, 0, 7, 0, 0, 0, 0, 0]);
    bar.extend_from_slice(b"\x03Options");
    std::fs::write(root.join("menu.bar"), bar).unwrap();
    machine.cpu_mut().write_mem(OTHER, b"menu.bar\0").unwrap();
    let softkey = create(&mut machine, SOFTKEY);
    assert_eq!(call(&mut machine, softkey, 12, &[OTHER, 7, 3, 0, 7]), 1);
    machine
        .cpu_mut()
        .write_mem(OTHER, b"reused-source\0")
        .unwrap();
    assert_eq!(call(&mut machine, softkey, 28, &[3, RECT]), 1);
    let resource_file = machine.cpu().read_u32(RECT + 12).unwrap();
    assert_eq!(machine.cpu().read_cstring(resource_file, 64), "menu.bar");
    let mut text_id = [0; 2];
    machine.cpu().read_mem(RECT + 16, &mut text_id).unwrap();
    assert_eq!(u16::from_le_bytes(text_id), 7);
    let owned_label = machine.cpu().read_u32(RECT).unwrap();
    assert_eq!(read_wide(&machine, owned_label), "Options");
    write_wide(&mut machine, WIDE, "Back");
    assert_eq!(call(&mut machine, softkey, 12, &[0, 0, 9, WIDE, 123]), 1);
    call(&mut machine, softkey, 4, &[1]);
    assert!(machine.drawn_text().any(|row| row.3 == "Options"));
    assert_eq!(call(&mut machine, softkey, 2, &[0x100, 0xe037, 0]), 1);
    assert_eq!(call(&mut machine, softkey, 18, &[]), 9);
    assert_eq!(call(&mut machine, softkey, 2, &[0x100, 0xe036, 0]), 1);
    assert_eq!(call(&mut machine, softkey, 18, &[]), 3);
    assert_eq!(call(&mut machine, softkey, 2, &[0x100, 0xe033, 0]), 1);
    assert_eq!(
        call(&mut machine, softkey, 18, &[]),
        9,
        "softkey faz wrap por padrão"
    );
    cleanup(machine, root);
}

#[test]
fn text_and_static_keep_utf16_and_redraw_visible_text() {
    let (mut machine, root) = machine();
    let text = create(&mut machine, TEXT);
    call(&mut machine, text, 16, &[20]);
    write_wide(&mut machine, WIDE, "Ação ♜");
    assert_eq!(call(&mut machine, text, 12, &[WIDE, u32::MAX]), 1);
    let pointer = call(&mut machine, text, 14, &[]);
    assert_ne!(pointer, 0);
    assert_eq!(read_wide(&machine, pointer), "Ação ♜");
    machine.cpu_mut().write_mem(OTHER, &[0xa5; 32]).unwrap();
    assert_eq!(call(&mut machine, text, 13, &[OTHER, 4]), 1);
    assert_eq!(read_wide(&machine, OTHER), "Açã");
    let static_control = create(&mut machine, STATIC);
    write_wide(&mut machine, WIDE, "Chess");
    write_wide(&mut machine, OTHER, "Choose color");
    assert_eq!(
        call(
            &mut machine,
            static_control,
            11,
            &[WIDE, OTHER, 0x8001, 0x8000]
        ),
        1
    );
    assert_eq!(call(&mut machine, static_control, 3, &[]), 1);
    assert!(machine.screen().escritas() > 0);
    assert!(
        machine
            .drawn_text()
            .any(|row| row.3.contains("Choose color"))
    );
    assert!(
        matches!(
            raw_call(&mut machine, static_control, 13, &[0, 0, 0]),
            Outcome::Unimplemented { .. }
        ),
        "texto de stream ainda não implementado deve ser observável"
    );
    cleanup(machine, root);
}

#[test]
fn control_state_roundtrips_and_old_states_without_the_section_clear_controls() {
    let (mut machine, root) = machine();
    let menu = create(&mut machine, MENU);
    write_wide(&mut machine, WIDE, "Saved menu");
    call(&mut machine, menu, 12, &[0, 0, 37, WIDE, 123]);
    call(&mut machine, menu, 8, &[0x10000]);
    let text = create(&mut machine, TEXT);
    call(&mut machine, text, 16, &[20]);
    write_wide(&mut machine, WIDE, "Saved text");
    call(&mut machine, text, 12, &[WIDE, u32::MAX]);
    let pointer = call(&mut machine, text, 14, &[]);
    let state = machine.grava_estado();
    call(&mut machine, menu, 10, &[]);
    write_wide(&mut machine, WIDE, "Changed");
    call(&mut machine, text, 12, &[WIDE, u32::MAX]);
    machine.restaura_estado(&state).unwrap();
    assert_eq!(call(&mut machine, menu, 26, &[]), 1);
    assert_eq!(call(&mut machine, menu, 18, &[]), 37);
    assert_eq!(call(&mut machine, menu, 9, &[]), 0x10000);
    assert_eq!(call(&mut machine, text, 14, &[]), pointer);
    assert_eq!(read_wide(&machine, pointer), "Saved text");
    let reader = Leitor::abre(&state).unwrap();
    assert!(reader.secao("tab.legacy_controls").is_some());
    let old = save_state::escreve(
        &reader
            .nomes()
            .into_iter()
            .filter(|name| *name != "tab.legacy_controls")
            .map(|name| (name.to_owned(), reader.secao(name).unwrap().to_vec()))
            .collect::<Vec<_>>(),
    );
    machine.restaura_estado(&old).unwrap();
    // Um objeto antigo sem livro de controle não ganha itens inventados ao restaurar.
    assert_eq!(
        raw_call(&mut machine, menu, 26, &[]),
        Outcome::Returned { code: 14 }
    );
    cleanup(machine, root);
}

#[test]
fn malformed_state_cannot_give_two_text_controls_the_same_owned_buffer() {
    let (mut machine, root) = machine();
    let first = create(&mut machine, TEXT);
    let second = create(&mut machine, TEXT);
    for (object, text) in [(first, "First"), (second, "Other")] {
        call(&mut machine, object, 16, &[20]);
        write_wide(&mut machine, WIDE, text);
        call(&mut machine, object, 12, &[WIDE, u32::MAX]);
    }
    let pointer = call(&mut machine, first, 14, &[]);
    let state = machine.grava_estado();
    let reader = Leitor::abre(&state).unwrap();
    let mut controls: serde_json::Value =
        serde_json::from_slice(reader.secao("tab.legacy_controls").unwrap()).unwrap();
    let second_state = controls["controls"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|row| row[0].as_u64() == Some(u64::from(second)))
        .unwrap();
    second_state[1]["text_pointer"] = serde_json::json!(pointer);
    let malformed = save_state::escreve(
        &reader
            .nomes()
            .into_iter()
            .map(|name| {
                (
                    name.to_owned(),
                    if name == "tab.legacy_controls" {
                        serde_json::to_vec(&controls).unwrap()
                    } else {
                        reader.secao(name).unwrap().to_vec()
                    },
                )
            })
            .collect::<Vec<_>>(),
    );
    assert!(
        machine.restaura_estado(&malformed).is_err(),
        "Release de um controle não pode liberar o texto ainda pertencente ao outro"
    );
    cleanup(machine, root);
}

#[test]
fn selecting_a_menu_item_redraws_unless_mp_no_redraw_is_set() {
    let (mut machine, root) = machine();
    let menu = create(&mut machine, MENU);
    for (id, label) in [(41, "Play"), (73, "Options")] {
        write_wide(&mut machine, WIDE, label);
        call(&mut machine, menu, 12, &[0, 0, id, WIDE, 0]);
    }
    call(&mut machine, menu, 4, &[1]);
    let before = machine.screen().escritas();
    call(&mut machine, menu, 17, &[73]);
    assert_eq!(call(&mut machine, menu, 18, &[]), 73);
    assert!(
        machine.screen().escritas() > before,
        "IMENUCTL_SetSel solicita redraw segundo AEEMenu.h"
    );
    call(&mut machine, menu, 8, &[0x0008_0000]);
    let before = machine.screen().escritas();
    call(&mut machine, menu, 17, &[41]);
    assert_eq!(call(&mut machine, menu, 18, &[]), 41);
    assert_eq!(
        machine.screen().escritas(),
        before,
        "MP_NO_REDRAW permite alterar seleção sem apagar a tela"
    );
    cleanup(machine, root);
}

#[test]
fn text_set_text_reads_only_the_requested_positive_character_prefix() {
    let (mut machine, root) = machine();
    let text = create(&mut machine, TEXT);
    call(&mut machine, text, 16, &[20]);
    write_wide(&mut machine, WIDE, &format!("ABC{}", "x".repeat(5000)));
    assert_eq!(
        call(&mut machine, text, 12, &[WIDE, 3]),
        1,
        "um prefixo de três caracteres cabe sem percorrer o restante da string"
    );
    let pointer = call(&mut machine, text, 14, &[]);
    assert_eq!(read_wide(&machine, pointer), "ABC");
    write_wide(&mut machine, WIDE, "Q");
    assert_eq!(call(&mut machine, text, 12, &[WIDE, 3]), 1);
    assert_eq!(
        read_wide(&machine, pointer),
        "Q",
        "NUL anterior encerra a cópia"
    );
    cleanup(machine, root);
}

#[test]
fn softkey_set_item_font_updates_only_the_masked_field_without_redrawing() {
    let (mut machine, root) = machine();
    let softkey = create(&mut machine, SOFTKEY);
    write_wide(&mut machine, WIDE, "Move");
    call(&mut machine, softkey, 12, &[0, 0, 6, WIDE, 123]);
    machine.cpu_mut().write_mem(OTHER, &[0xa5; 28]).unwrap();
    machine
        .cpu_mut()
        .write_mem(OTHER + 18, &0x8001u16.to_le_bytes())
        .unwrap();
    let before = machine.screen().escritas();
    assert_eq!(
        call(&mut machine, softkey, 29, &[6, 8, OTHER]),
        1,
        "MSIF_FONT seleciona apenas wFont no CtlAddItem"
    );
    assert_eq!(
        machine.screen().escritas(),
        before,
        "SetItem exige Redraw explícito no SDK"
    );
    assert_eq!(call(&mut machine, softkey, 14, &[6, OUT]), 1);
    assert_eq!(machine.cpu().read_u32(OUT).unwrap(), 123);
    let state = machine.grava_estado();
    let reader = Leitor::abre(&state).unwrap();
    let saved: serde_json::Value =
        serde_json::from_slice(reader.secao("tab.legacy_controls").unwrap()).unwrap();
    let control = saved["controls"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row[0].as_u64() == Some(u64::from(softkey)))
        .unwrap();
    assert_eq!(control[1]["items"][0]["font"].as_u64(), Some(0x8001));
    assert_eq!(control[1]["items"][0]["text"].as_array().unwrap().len(), 4);
    assert!(
        matches!(
            raw_call(&mut machine, softkey, 29, &[6, 2, OTHER]),
            Outcome::Unimplemented { .. }
        ),
        "substituição de imagem continua explicitamente não suportada"
    );
    cleanup(machine, root);
}

#[test]
fn control_redraw_publishes_device_frames_but_keeps_offscreen_drawing_private() {
    let (mut machine, root) = machine();
    let display = create(&mut machine, 0x01001001);
    let display_slot = |name| {
        (0..256)
            .find(|slot| Interface::Display.method(*slot) == Some(name))
            .unwrap()
    };
    call(&mut machine, display, display_slot("Update"), &[]);
    machine.toma_quadro_do_update().unwrap();
    let menu = create(&mut machine, MENU);
    write_wide(&mut machine, WIDE, "Play");
    call(&mut machine, menu, 12, &[0, 0, 41, WIDE, 0]);
    call(&mut machine, menu, 4, &[1]);
    assert!(
        machine.toma_quadro_do_update().is_some(),
        "SetActive redesenha e deve substituir o quadro em cache da Session"
    );
    call(&mut machine, menu, 3, &[]);
    assert!(machine.toma_quadro_do_update().is_some());
    assert!(machine.toma_quadro_do_update().is_none());
    assert_eq!(
        call(
            &mut machine,
            display,
            display_slot("CreateDIBitmap"),
            &[OUT, 16, 40, 40]
        ),
        0
    );
    let offscreen = machine.cpu().read_u32(OUT).unwrap();
    assert_ne!(offscreen, 0);
    call(
        &mut machine,
        display,
        display_slot("SetDestination"),
        &[offscreen],
    );
    call(&mut machine, menu, 3, &[]);
    assert!(
        machine.toma_quadro_do_update().is_none(),
        "Redraw para bitmap fora da tela não é apresentação do aparelho"
    );
    cleanup(machine, root);
}

#[test]
fn menu_set_style_copies_arm_layout_and_renders_frame_and_padding_after_restore() {
    let (mut machine, root) = machine();
    for class in [MENU, SOFTKEY] {
        let menu = create(&mut machine, class);
        let normal = [5, 0xa5, 7, 0, 3, 0, 2, 0xa5];
        let selected = [0, 0xa5, 0, 0, 0, 0, 2, 0xa5];
        machine.cpu_mut().write_mem(OTHER, &normal).unwrap();
        machine.cpu_mut().write_mem(OTHER + 8, &selected).unwrap();
        assert_eq!(call(&mut machine, menu, 23, &[OTHER, OTHER + 8]), 0);
        // AEEItemStyle no ARM tem enums de um byte e padding nos bytes 1 e 7.
        machine.cpu_mut().write_mem(OTHER, &[0xcc; 32]).unwrap();
        let saved = machine.grava_estado();
        call(&mut machine, menu, 23, &[0, 0]);
        machine.restaura_estado(&saved).unwrap();
        call(&mut machine, menu, 34, &[OTHER, OTHER + 16]);
        let mut actual = [0; 32];
        machine.cpu().read_mem(OTHER, &mut actual).unwrap();
        assert_eq!(
            [
                actual[0], actual[2], actual[3], actual[4], actual[5], actual[6]
            ],
            [5, 7, 0, 3, 0, 2]
        );
        assert_eq!(&actual[8..16], &[0xcc; 8], "GetStyle grava só oito bytes");
        assert_eq!(
            [actual[16], actual[18], actual[20], actual[22]],
            [0, 0, 0, 2]
        );
        assert_eq!(&actual[24..], &[0xcc; 8]);

        let rect = [10i16, 20, 100, 80]
            .into_iter()
            .flat_map(i16::to_le_bytes)
            .collect::<Vec<_>>();
        machine.cpu_mut().write_mem(RECT, &rect).unwrap();
        call(&mut machine, menu, 6, &[RECT]);
        write_wide(&mut machine, WIDE, "M");
        call(&mut machine, menu, 12, &[0, 0, 41, WIDE, 0]);
        // Branco no fundo, texto preto e moldura vermelha.
        machine.cpu_mut().write_u32(OTHER, 0x13).unwrap();
        for (index, color) in [0xffff_ff00, 0, 0, 0, 0x0000_ff00].into_iter().enumerate() {
            machine
                .cpu_mut()
                .write_u32(OTHER + 4 + index as u32 * 4, color)
                .unwrap();
        }
        call(&mut machine, menu, 24, &[OTHER]);
        call(&mut machine, menu, 3, &[]);
        assert_eq!(
            machine.screen().get_pixel(10, 20),
            0xf800,
            "AEE_FT_BOX desenha a moldura"
        );
        assert_eq!(machine.screen().get_pixel(11, 21), 0xffff);
        let black_pixels = machine
            .screen()
            .pixels()
            .iter()
            .enumerate()
            .filter(|(_, pixel)| **pixel == 0)
            .map(|(index, _)| ((index % 240) as i32, (index / 240) as i32))
            .filter(|(x, y)| *x > 10 && *x < 110 && *y > 20 && *y < 100)
            .collect::<Vec<_>>();
        assert!(!black_pixels.is_empty());
        assert!(
            black_pixels.iter().all(|(x, y)| *x >= 18 && *y >= 24),
            "padding exclui a moldura"
        );
        call(&mut machine, menu, 1, &[]);
    }
    cleanup(machine, root);
}

#[test]
fn menu_add_item_ex_retains_supplied_image_draws_it_and_releases_on_removal() {
    let (mut machine, root) = machine();
    let mut bmp = vec![0; 58];
    bmp[0..2].copy_from_slice(b"BM");
    bmp[2..6].copy_from_slice(&58u32.to_le_bytes());
    bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
    bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
    bmp[18..22].copy_from_slice(&1u32.to_le_bytes());
    bmp[22..26].copy_from_slice(&1u32.to_le_bytes());
    bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
    bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
    bmp[54..57].copy_from_slice(&[0, 0, 255]);
    std::fs::write(root.join("menu.bmp"), bmp).unwrap();
    machine.cpu_mut().write_mem(OTHER, b"menu.bmp\0").unwrap();
    let load = (0..256)
        .find(|slot| Interface::Shell.method(*slot) == Some("LoadResObject"))
        .unwrap();
    let image = call(&mut machine, loader::OBJECT_BASE, load, &[OTHER, 0, 0]);
    assert_ne!(image, 0);
    let menu = create(&mut machine, MENU);
    write_wide(&mut machine, WIDE, "Image");
    let fields = [WIDE, image, 0, 0, 0x8000_0000, 41u32 << 16, 0x1234_5678];
    machine
        .cpu_mut()
        .write_mem(
            OTHER,
            &fields
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    let writes = machine.screen().escritas();
    assert_eq!(call(&mut machine, menu, 13, &[OTHER]), 1);
    assert_eq!(
        machine.screen().escritas(),
        writes,
        "AddItemEx não redesenha"
    );
    assert_eq!(
        call(&mut machine, image, 1, &[]),
        1,
        "o controle retém sua referência ao IImage"
    );
    assert_eq!(call(&mut machine, menu, 14, &[41, OUT]), 1);
    assert_eq!(machine.cpu().read_u32(OUT).unwrap(), 0x1234_5678);
    call(&mut machine, menu, 3, &[]);
    assert_eq!(
        machine.screen().get_pixel(2, 2),
        0xf800,
        "o ícone deve continuar visível após Release do chamador"
    );
    let state = machine.grava_estado();
    call(&mut machine, menu, 15, &[41]);
    assert_eq!(
        call(&mut machine, image, 0, &[]),
        0,
        "DeleteItem libera a referência retida"
    );
    machine.restaura_estado(&state).unwrap();
    call(&mut machine, menu, 3, &[]);
    assert_eq!(machine.screen().get_pixel(2, 2), 0xf800);
    call(&mut machine, menu, 16, &[]);
    assert_eq!(
        call(&mut machine, image, 0, &[]),
        0,
        "DeleteAll também libera imagens"
    );
    cleanup(machine, root);
}

#[test]
fn menu_add_item_ex_loads_resource_icons_and_reset_releases_them() {
    let (mut machine, root) = machine();
    let mut bmp = vec![0; 58];
    bmp[0..2].copy_from_slice(b"BM");
    bmp[2..6].copy_from_slice(&58u32.to_le_bytes());
    bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
    bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
    bmp[18..22].copy_from_slice(&1u32.to_le_bytes());
    bmp[22..26].copy_from_slice(&1u32.to_le_bytes());
    bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
    bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
    bmp[54..57].copy_from_slice(&[0, 0, 255]);
    let mut bar = vec![0u8; 48];
    bar[..2].copy_from_slice(&0x11u16.to_le_bytes());
    bar[6..8].copy_from_slice(&1u16.to_le_bytes());
    for (at, value) in [
        (8, 32u32),
        (12, 8),
        (16, 40),
        (20, 1),
        (24, 48),
        (40, 48),
        (44, 106),
    ] {
        bar[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    bar[32..40].copy_from_slice(&[6, 0, 7, 0, 0, 0, 0, 0]);
    bar.extend_from_slice(&bmp);
    std::fs::write(root.join("icons.bar"), bar).unwrap();
    machine
        .cpu_mut()
        .write_mem(OUT + 0x80, b"icons.bar\0")
        .unwrap();
    write_wide(&mut machine, WIDE, "Resource");
    let menu = create(&mut machine, MENU);
    let mut fields = [WIDE, 0, OUT + 0x80, 0, 0x8000_0000, (41u32 << 16) | 99, 0];
    machine
        .cpu_mut()
        .write_mem(
            OTHER,
            &fields
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    assert_eq!(
        call(&mut machine, menu, 13, &[OTHER]),
        0,
        "recurso ausente não cria item invisível"
    );
    assert_eq!(call(&mut machine, menu, 26, &[]), 0);
    fields[5] = (41u32 << 16) | 7;
    machine
        .cpu_mut()
        .write_mem(
            OTHER,
            &fields
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    assert_eq!(call(&mut machine, menu, 13, &[OTHER]), 1);
    machine
        .cpu_mut()
        .write_mem(OUT + 0x80, b"reused-source\0")
        .unwrap();
    assert_eq!(call(&mut machine, menu, 28, &[41, RECT]), 1);
    assert_ne!(machine.cpu().read_u32(RECT + 4).unwrap(), 0);
    let resource_file = machine.cpu().read_u32(RECT + 8).unwrap();
    assert_eq!(machine.cpu().read_cstring(resource_file, 64), "icons.bar");
    let mut image_id = [0; 2];
    machine.cpu().read_mem(RECT + 20, &mut image_id).unwrap();
    assert_eq!(u16::from_le_bytes(image_id), 7);
    call(&mut machine, menu, 3, &[]);
    assert_eq!(machine.screen().get_pixel(2, 2), 0xf800);
    let saved = machine.grava_estado();
    assert_eq!(
        Leitor::abre(&saved).unwrap().u32s("img.ids").unwrap().len(),
        1
    );
    call(&mut machine, menu, 10, &[]);
    let cleared = machine.grava_estado();
    assert_eq!(
        Leitor::abre(&cleared)
            .unwrap()
            .u32s("img.ids")
            .unwrap()
            .len(),
        0,
        "Reset libera a imagem do recurso"
    );
    cleanup(machine, root);
}

#[test]
fn menu_get_item_returns_control_owned_text_and_exact_ctladditem_fields() {
    let (mut machine, root) = machine();
    let heap = create(&mut machine, 0x0100_1002);
    let menu = create(&mut machine, MENU);
    write_wide(&mut machine, WIDE, "Business");
    call(&mut machine, menu, 12, &[0, 0, 41, WIDE, 0x1234_5678]);
    write_wide(&mut machine, WIDE, "Reused source");
    let before = call(&mut machine, heap, 7, &[]);
    machine.cpu_mut().write_mem(OTHER, &[0xa5; 36]).unwrap();
    assert_eq!(call(&mut machine, menu, 28, &[41, OTHER + 4]), 1);
    let mut fields = [0; 36];
    machine.cpu().read_mem(OTHER, &mut fields).unwrap();
    assert_eq!(&fields[..4], &[0xa5; 4]);
    assert_eq!(&fields[32..], &[0xa5; 4], "CtlAddItem tem 28 bytes");
    let text = u32::from_le_bytes(fields[4..8].try_into().unwrap());
    assert_ne!(text, WIDE, "a origem não pertence ao controle");
    assert_eq!(read_wide(&machine, text), "Business");
    assert_eq!(
        &fields[8..20],
        &[0; 12],
        "sem imagem ou referências a recursos após resolver o texto"
    );
    assert_eq!(&fields[20..28], &[0, 0, 0, 0x80, 0, 0, 41, 0]);
    assert_eq!(&fields[28..32], &0x1234_5678u32.to_le_bytes());
    assert!(call(&mut machine, heap, 7, &[]) > before);
    call(&mut machine, menu, 28, &[41, OTHER + 4]);
    assert_eq!(machine.cpu().read_u32(OTHER + 4).unwrap(), text);
    let saved = machine.grava_estado();
    call(&mut machine, menu, 16, &[]);
    assert_eq!(
        call(&mut machine, heap, 7, &[]),
        before,
        "DeleteAll libera o texto exportado"
    );
    machine.restaura_estado(&saved).unwrap();
    call(&mut machine, menu, 28, &[41, OTHER + 4]);
    assert_eq!(machine.cpu().read_u32(OTHER + 4).unwrap(), text);
    assert_eq!(read_wide(&machine, text), "Business");
    write_wide(&mut machine, WIDE, "Changed");
    call(&mut machine, menu, 20, &[41, 0, 0, WIDE]);
    call(&mut machine, menu, 28, &[41, OTHER + 4]);
    let changed = machine.cpu().read_u32(OTHER + 4).unwrap();
    assert_eq!(read_wide(&machine, changed), "Changed");
    assert_eq!(call(&mut machine, menu, 28, &[99, OTHER + 4]), 0);
    assert_eq!(call(&mut machine, menu, 28, &[41, 0]), 0);
    cleanup(machine, root);
}

#[test]
fn static_centers_title_and_body_independently_with_sdk_property_flags() {
    let (mut machine, root) = machine();
    let control = create(&mut machine, STATIC);
    let rect = [0i16, 0, 120, 28]
        .into_iter()
        .flat_map(i16::to_le_bytes)
        .collect::<Vec<_>>();
    machine.cpu_mut().write_mem(RECT, &rect).unwrap();
    call(&mut machine, control, 6, &[RECT]);
    write_wide(&mut machine, WIDE, "Legolas");
    write_wide(&mut machine, OTHER, "TURN START");
    call(&mut machine, control, 11, &[WIDE, OTHER, 0x8001, 0x8000]);
    let definitions = egui::FontDefinitions::default();
    let font = zeebx::video::font::Font::load(
        definitions.font_data["Ubuntu-Light"].font.to_vec(),
        "fixture".into(),
    )
    .unwrap();
    let title_x = (120 - font.width("Legolas", 16.0) as i32) / 2;
    let body_x = (120 - font.width("TURN START", 16.0) as i32) / 2;
    for (flags, want_title_x, want_body_x) in [
        (0x70000, title_x, body_x),
        (0x10000, 2, body_x),
        (0x20000, title_x, 2),
    ] {
        call(&mut machine, control, 8, &[flags]);
        call(&mut machine, control, 3, &[]);
        let rows = machine.drawn_text().cloned().collect::<Vec<_>>();
        let title = &rows[rows.len() - 2];
        let body = &rows[rows.len() - 1];
        assert_eq!((&title.3[..], title.1), ("Legolas", want_title_x));
        assert_eq!((&body.3[..], body.1), ("TURN START", want_body_x));
    }
    cleanup(machine, root);
}

#[test]
fn menu_restore_requires_a_retained_reference_for_each_shared_image_owner() {
    let (mut machine, root) = machine();
    let mut bmp = vec![0; 58];
    bmp[..2].copy_from_slice(b"BM");
    for (offset, value) in [(2, 58u32), (10, 54), (14, 40), (18, 1), (22, 1)] {
        bmp[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
    bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
    bmp[54..57].copy_from_slice(&[0, 0, 255]);
    std::fs::write(root.join("shared.bmp"), bmp).unwrap();
    machine.cpu_mut().write_mem(OTHER, b"shared.bmp\0").unwrap();
    let load = (0..256)
        .find(|slot| Interface::Shell.method(*slot) == Some("LoadResObject"))
        .unwrap();
    let image = call(&mut machine, loader::OBJECT_BASE, load, &[OTHER, 0, 0]);
    assert_ne!(image, 0);
    let menus = [create(&mut machine, MENU), create(&mut machine, SOFTKEY)];
    write_wide(&mut machine, WIDE, "Shared");
    let fields = [WIDE, image, 0, 0, 0, 41u32 << 16, 0];
    machine
        .cpu_mut()
        .write_mem(
            OTHER,
            &fields
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    for menu in menus {
        assert_eq!(call(&mut machine, menu, 13, &[OTHER]), 1);
    }
    assert_eq!(call(&mut machine, image, 1, &[]), 2);
    let saved = machine.grava_estado();
    machine.restaura_estado(&saved).unwrap();
    call(&mut machine, menus[0], 15, &[41]);
    assert_eq!(
        call(&mut machine, image, 0, &[]),
        2,
        "second item must retain its image"
    );
    call(&mut machine, image, 1, &[]);
    let reader = Leitor::abre(&saved).unwrap();
    for insufficient in [1, 0] {
        let refs = reader.pares("objects.refs").unwrap();
        let replacement = std::iter::once(refs.len() as u32 * 2)
            .chain(refs.into_iter().flat_map(|(object, count)| {
                [object, if object == image { insufficient } else { count }]
            }))
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        let malformed = save_state::escreve(
            &reader
                .nomes()
                .into_iter()
                .map(|name| {
                    (
                        name.to_owned(),
                        if name == "objects.refs" {
                            replacement.clone()
                        } else {
                            reader.secao(name).unwrap().to_vec()
                        },
                    )
                })
                .collect::<Vec<_>>(),
        );
        assert!(
            machine.restaura_estado(&malformed).is_err(),
            "two image owners cannot share {insufficient} retained refs"
        );
    }
    cleanup(machine, root);
}
