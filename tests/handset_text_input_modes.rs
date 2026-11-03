use std::path::PathBuf;

use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend, Reg};
use zeebx::handset::types::DisplayGeometry;
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::save_state::{self, Leitor};
use zeebx::storage::StoragePaths;

const TEXT: u32 = 0x0100_3009;
const OUT: u32 = loader::STACK_BASE + 0x800;
const WIDE: u32 = OUT + 0x100;
const OTHER: u32 = OUT + 0x200;

fn machine() -> (Machine<BackendPadrao>, PathBuf) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("local-tests")
        .join(format!(
            "text-input-modes-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
    std::fs::create_dir(&root).unwrap();
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
fn current_and_unsupported_modes_report_the_actual_mode_without_changing_text() {
    let (mut machine, root) = machine();
    let text = create(&mut machine, TEXT);
    write_wide(&mut machine, WIDE, "Player");
    assert_eq!(call(&mut machine, text, 12, &[WIDE, u32::MAX]), 1);
    // AEEText.h: CURRENT conserva o modo; KANJI/RAPID não existem neste aparelho.
    for requested in [1, 6, 4, 0xffff_ffff] {
        assert_eq!(call(&mut machine, text, 18, &[requested]), 3);
        assert_eq!(call(&mut machine, text, 21, &[0]), 3);
    }
    assert_eq!(call(&mut machine, text, 18, &[5]), 5);
    for requested in [1, 6, 4] {
        assert_eq!(call(&mut machine, text, 18, &[requested]), 5);
    }
    assert_eq!(
        call(&mut machine, text, 18, &[0]),
        3,
        "NONE restaura multitap padrão"
    );
    assert_eq!(call(&mut machine, text, 13, &[OTHER, 20]), 1);
    assert_eq!(read_wide(&machine, OTHER), "Player");
    let second = create(&mut machine, TEXT);
    assert_eq!(call(&mut machine, second, 21, &[0]), 3);
    cleanup(machine, root);
}

#[test]
fn mode_enumeration_is_per_control_and_writes_a_bounded_sdk_mode_info() {
    let (mut machine, root) = machine();
    let text = create(&mut machine, TEXT);
    let second = create(&mut machine, TEXT);
    call(&mut machine, text, 22, &[]);
    for expected in [3, 5, 0, 0] {
        machine.cpu_mut().write_mem(OTHER, &[0xa5; 40]).unwrap();
        assert_eq!(call(&mut machine, text, 23, &[OTHER]), expected);
        assert_eq!(machine.cpu().read_u32(OTHER).unwrap(), expected);
        let label = read_wide(&machine, OTHER + 4);
        assert!(label.encode_utf16().count() < 15);
        assert_eq!(label.is_empty(), expected == 0);
        let mut guard = [0; 6];
        machine.cpu().read_mem(OTHER + 34, &mut guard).unwrap();
        assert_eq!(
            guard, [0xa5; 6],
            "AEETextInputModeInfo contém 4 + 30 bytes de campos"
        );
    }
    assert_eq!(call(&mut machine, second, 23, &[0]), 3);
    call(&mut machine, text, 22, &[]);
    assert_eq!(call(&mut machine, text, 23, &[0]), 3);
    assert_eq!(call(&mut machine, text, 18, &[5]), 5);
    assert_eq!(call(&mut machine, text, 21, &[OTHER]), 5);
    assert_eq!(machine.cpu().read_u32(OTHER).unwrap(), 5);
    cleanup(machine, root);
}

#[test]
fn numbers_mode_edits_at_cursor_respects_max_size_and_ignores_non_key_events() {
    let (mut machine, root) = machine();
    let text = create(&mut machine, TEXT);
    assert_eq!(call(&mut machine, text, 18, &[5]), 5);
    call(&mut machine, text, 16, &[3]);
    assert_eq!(call(&mut machine, text, 2, &[0x100, 0xe023, 0]), 0);
    call(&mut machine, text, 4, &[1]);
    for key in [0xe023, 0xe024] {
        assert_eq!(call(&mut machine, text, 2, &[0x100, key, 0]), 1);
    }
    assert_eq!(call(&mut machine, text, 2, &[0x101, 0xe024, 0]), 0);
    assert_eq!(call(&mut machine, text, 2, &[0x102, 0xe024, 0]), 0);
    call(&mut machine, text, 20, &[1]);
    assert_eq!(call(&mut machine, text, 2, &[0x100, 0xe025, 0]), 1);
    let pointer = call(&mut machine, text, 14, &[]);
    assert_eq!(read_wide(&machine, pointer), "243");
    assert_eq!(call(&mut machine, text, 19, &[]), 2);
    call(&mut machine, text, 2, &[0x100, 0xe026, 0]);
    assert_eq!(read_wide(&machine, pointer), "243");
    assert_eq!(call(&mut machine, text, 2, &[0x100, 0xe030, 0]), 1);
    assert_eq!(read_wide(&machine, pointer), "23");
    call(&mut machine, text, 2, &[0x100, 0xe021, 0]);
    assert_eq!(read_wide(&machine, pointer), "203");
    cleanup(machine, root);
}

#[test]
fn letters_multitap_cycles_one_character_and_cursor_movement_commits_it() {
    let (mut machine, root) = machine();
    // Tempo pausado torna a janela multitap determinística sem dormir no teste.
    machine.pause_handset_clock(true);
    let text = create(&mut machine, TEXT);
    assert_eq!(call(&mut machine, text, 18, &[3]), 3);
    call(&mut machine, text, 4, &[1]);
    call(&mut machine, text, 16, &[2]);
    let pointer = call(&mut machine, text, 14, &[]);
    for expected in ["a", "b", "c"] {
        assert_eq!(call(&mut machine, text, 2, &[0x100, 0xe023, 0]), 1);
        assert_eq!(read_wide(&machine, pointer), expected);
        assert_eq!(call(&mut machine, text, 19, &[]), 1);
    }
    call(&mut machine, text, 2, &[0x100, 0xe024, 0]);
    assert_eq!(read_wide(&machine, pointer), "cd");
    call(&mut machine, text, 2, &[0x100, 0xe024, 0]);
    assert_eq!(
        read_wide(&machine, pointer),
        "ce",
        "multitap funciona no limite do buffer"
    );
    call(&mut machine, text, 2, &[0x100, 0xe030, 0]);
    call(&mut machine, text, 20, &[1]);
    call(&mut machine, text, 2, &[0x100, 0xe023, 0]);
    assert_eq!(read_wide(&machine, pointer), "ca");
    cleanup(machine, root);
}

#[test]
fn text_mode_survives_save_and_older_control_sections_default_to_letters() {
    let (mut machine, root) = machine();
    let text = create(&mut machine, TEXT);
    call(&mut machine, text, 18, &[5]);
    call(&mut machine, text, 22, &[]);
    assert_eq!(call(&mut machine, text, 23, &[0]), 3);
    let saved = machine.grava_estado();
    call(&mut machine, text, 18, &[3]);
    machine.restaura_estado(&saved).unwrap();
    assert_eq!(call(&mut machine, text, 21, &[0]), 5);
    assert_eq!(call(&mut machine, text, 23, &[0]), 5);

    let reader = Leitor::abre(&saved).unwrap();
    let mut controls: serde_json::Value =
        serde_json::from_slice(reader.secao("tab.legacy_controls").unwrap()).unwrap();
    for control in controls["controls"].as_array_mut().unwrap() {
        let metadata = control[1].as_object_mut().unwrap();
        metadata.remove("input_mode");
        metadata.remove("mode_enum_index");
        metadata.remove("multitap");
    }
    let old = save_state::escreve(
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
    machine.restaura_estado(&old).unwrap();
    assert_eq!(call(&mut machine, text, 21, &[0]), 3);
    cleanup(machine, root);
}

#[test]
fn restored_control_rejects_an_unimplemented_input_mode_or_invalid_enumerator() {
    let (mut machine, root) = machine();
    let _text = create(&mut machine, TEXT);
    let saved = machine.grava_estado();
    let reader = Leitor::abre(&saved).unwrap();
    let controls: serde_json::Value =
        serde_json::from_slice(reader.secao("tab.legacy_controls").unwrap()).unwrap();
    for (field, value) in [("input_mode", 6), ("mode_enum_index", 100)] {
        let mut malformed = controls.clone();
        malformed["controls"][0][1][field] = serde_json::json!(value);
        let bytes = save_state::escreve(
            &reader
                .nomes()
                .into_iter()
                .map(|name| {
                    (
                        name.to_owned(),
                        if name == "tab.legacy_controls" {
                            serde_json::to_vec(&malformed).unwrap()
                        } else {
                            reader.secao(name).unwrap().to_vec()
                        },
                    )
                })
                .collect::<Vec<_>>(),
        );
        assert!(
            machine.restaura_estado(&bytes).is_err(),
            "{field} inválido não pode declarar suporte inexistente"
        );
    }
    cleanup(machine, root);
}

#[test]
fn multitap_roundtrips_but_never_overwrites_a_guest_edit_to_the_public_buffer() {
    let (mut machine, root) = machine();
    machine.pause_handset_clock(true);
    let text = create(&mut machine, TEXT);
    call(&mut machine, text, 4, &[1]);
    call(&mut machine, text, 16, &[8]);
    assert_eq!(call(&mut machine, text, 2, &[0x100, 0xe023, 0]), 1);
    let pointer = call(&mut machine, text, 14, &[]);
    let saved = machine.grava_estado();
    call(&mut machine, text, 2, &[0x100, 0xe024, 0]);
    machine.restaura_estado(&saved).unwrap();
    assert_eq!(read_wide(&machine, pointer), "a");
    call(&mut machine, text, 2, &[0x100, 0xe023, 0]);
    assert_eq!(read_wide(&machine, pointer), "b");
    write_wide(&mut machine, pointer, "Z");
    call(&mut machine, text, 2, &[0x100, 0xe023, 0]);
    assert_eq!(read_wide(&machine, pointer), "Za");
    cleanup(machine, root);
}

#[test]
fn menu_raised_style_renders_two_pixel_bevel_and_bitwise_or_image_after_restore() {
    menu_bevel_style(3);
}

#[test]
fn menu_sunken_style_renders_the_inverse_bevel_and_survives_save_restore() {
    menu_bevel_style(4);
}

fn menu_bevel_style(frame: u8) {
    let (mut machine, root) = machine();
    // Ícone vermelho sobre azul: OR produz magenta, COPY produziria vermelho.
    let mut bmp = vec![0; 58];
    bmp[0..2].copy_from_slice(b"BM");
    for (at, value) in [(2, 58u32), (10, 54), (14, 40), (18, 1), (22, 1)] {
        bmp[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
    bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
    bmp[54..57].copy_from_slice(&[0, 0, 255]);
    std::fs::write(root.join("or-icon.bmp"), bmp).unwrap();
    machine
        .cpu_mut()
        .write_mem(OTHER, b"or-icon.bmp\0")
        .unwrap();
    let load = (0..256)
        .find(|slot| Interface::Shell.method(*slot) == Some("LoadResObject"))
        .unwrap();
    let image = call(&mut machine, loader::OBJECT_BASE, load, &[OTHER, 0, 0]);
    assert_ne!(image, 0);
    let menu = create(&mut machine, 0x0100_3100);
    let normal = [1, 0xa5, 0, 0, 0, 0, 0, 0xa5];
    let selected = [frame, 0xa5, 0, 0, 0, 0, 0, 0xa5];
    machine.cpu_mut().write_mem(OTHER, &normal).unwrap();
    machine.cpu_mut().write_mem(OTHER + 8, &selected).unwrap();
    assert_eq!(call(&mut machine, menu, 23, &[OTHER, OTHER + 8]), 0);
    let rect = [10i16, 20, 80, 40]
        .into_iter()
        .flat_map(i16::to_le_bytes)
        .collect::<Vec<_>>();
    machine.cpu_mut().write_mem(OTHER, &rect).unwrap();
    call(&mut machine, menu, 6, &[OTHER]);
    write_wide(&mut machine, WIDE, "");
    let item = [WIDE, image, 0, 0, 0x8000_0000, 41u32 << 16, 0];
    machine
        .cpu_mut()
        .write_mem(
            OTHER,
            &item
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    assert_eq!(call(&mut machine, menu, 13, &[OTHER]), 1);
    // MC_BACK e MC_SEL_BACK, ambos azul RGBVAL; demais cores ficam intactas.
    machine.cpu_mut().write_u32(OTHER, 5).unwrap();
    machine.cpu_mut().write_u32(OTHER + 4, 0xff00_0000).unwrap();
    machine
        .cpu_mut()
        .write_u32(OTHER + 12, 0xff00_0000)
        .unwrap();
    call(&mut machine, menu, 24, &[OTHER]);
    call(&mut machine, menu, 4, &[1]);
    let saved = machine.grava_estado();
    machine.restaura_estado(&saved).unwrap();
    call(&mut machine, menu, 3, &[]);
    assert_eq!(
        machine.screen().get_pixel(12, 22),
        0xf81f,
        "ROP_OR precisa dos dois operandos"
    );
    let outer_light = machine.screen().get_pixel(10, 25);
    let inner_light = machine.screen().get_pixel(11, 25);
    let inner_dark = machine.screen().get_pixel(88, 25);
    let outer_dark = machine.screen().get_pixel(89, 25);
    assert!(
        if frame == 3 {
            outer_light > inner_light && inner_light > inner_dark && inner_dark > outer_dark
        } else {
            outer_light < inner_light && inner_light < inner_dark && inner_dark < outer_dark
        },
        "o relevo de dois pixels deve ser invertido para SUNKEN"
    );
    machine.cpu_mut().write_mem(OTHER, &[0xa5; 20]).unwrap();
    call(&mut machine, menu, 34, &[OTHER, OTHER + 8]);
    let mut styles = [0; 20];
    machine.cpu().read_mem(OTHER, &mut styles).unwrap();
    assert_eq!(
        [styles[0], styles[6], styles[8], styles[14]],
        [1, 0, frame, 0]
    );
    assert_eq!(&styles[16..], &[0xa5; 4]);
    cleanup(machine, root);
}

fn icon_grid_fixture() -> (Machine<BackendPadrao>, PathBuf, u32) {
    let (mut machine, root) = machine();
    let mut bmp = vec![0; 246];
    bmp[0..2].copy_from_slice(b"BM");
    for (at, value) in [(2, 246u32), (10, 54), (14, 40), (18, 8), (22, 8)] {
        bmp[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
    bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
    for pixel in bmp[54..].chunks_exact_mut(3) {
        pixel.copy_from_slice(&[0, 0, 255]);
    }
    std::fs::write(root.join("grid-icon.bmp"), bmp).unwrap();
    machine
        .cpu_mut()
        .write_mem(OTHER, b"grid-icon.bmp\0")
        .unwrap();
    let load = (0..256)
        .find(|slot| Interface::Shell.method(*slot) == Some("LoadResObject"))
        .unwrap();
    let image = call(&mut machine, loader::OBJECT_BASE, load, &[OTHER, 0, 0]);
    assert_ne!(image, 0);
    // AEEClassIDs.h: ICONVIEWCTL_10; AEEMenu.h exige o mesmo ABI IMenuCtl.
    let icon = create(&mut machine, 0x0100_3003);
    let rect = [0i16, 0, 28, 64]
        .into_iter()
        .flat_map(i16::to_le_bytes)
        .collect::<Vec<_>>();
    machine.cpu_mut().write_mem(OTHER, &rect).unwrap();
    call(&mut machine, icon, 6, &[OTHER]);
    for (index, id) in [10u32, 20, 30, 40, 50].into_iter().enumerate() {
        write_wide(&mut machine, WIDE, &format!("Icon{}", index + 1));
        let item = [WIDE, image, 0, 0, 0x8000_0000, id << 16, id + 100];
        machine
            .cpu_mut()
            .write_mem(
                OTHER,
                &item
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        assert_eq!(call(&mut machine, icon, 13, &[OTHER]), 1);
    }
    (machine, root, icon)
}

#[test]
fn icon_view_uses_menu_abi_but_draws_a_grid_with_one_selected_label_and_wraps_by_column() {
    let (mut machine, root, icon) = icon_grid_fixture();
    call(&mut machine, icon, 4, &[1]);
    let before = machine.drawn_text().count();
    call(&mut machine, icon, 3, &[]);
    let labels = machine
        .drawn_text()
        .skip(before)
        .filter(|row| row.3.starts_with("Icon"))
        .collect::<Vec<_>>();
    assert_eq!(
        labels.len(),
        1,
        "IconView mostra apenas o texto do item selecionado"
    );
    assert_eq!(labels[0].3, "Icon1");
    assert!(labels[0].2 >= 40, "a legenda padrão fica abaixo da grade");
    let columns = (0..28)
        .filter(|x| (0..40).any(|y| machine.screen().get_pixel(*x, y) == 0xf800))
        .collect::<Vec<_>>();
    let rows = (0..40)
        .filter(|y| (0..28).any(|x| machine.screen().get_pixel(x, *y) == 0xf800))
        .collect::<Vec<_>>();
    let column_runs = columns
        .windows(2)
        .filter(|pair| pair[1] != pair[0] + 1)
        .count()
        + 1;
    let row_runs = rows
        .windows(2)
        .filter(|pair| pair[1] != pair[0] + 1)
        .count()
        + 1;
    assert_eq!(
        (column_runs, row_runs, columns.len(), rows.len()),
        (2, 3, 16, 24),
        "ícones 8x8 precisam ocupar duas colunas e três linhas, sem texto repetido por célula"
    );
    // A propriedade WRAP pode parecer desligada, mas IconView sempre dá a volta.
    call(&mut machine, icon, 8, &[0x0008_0000]);
    assert_eq!(call(&mut machine, icon, 9, &[]) & 0x10000, 0);
    for (key, expected) in [
        (0xe034, 20),
        (0xe032, 40),
        (0xe032, 20),
        (0xe033, 10),
        (0xe031, 50),
        (0xe032, 10),
    ] {
        assert_eq!(call(&mut machine, icon, 2, &[0x100, key, 0]), 1);
        assert_eq!(call(&mut machine, icon, 18, &[]), expected);
    }
    let saved = machine.grava_estado();
    call(&mut machine, icon, 17, &[50]);
    machine.restaura_estado(&saved).unwrap();
    assert_eq!(call(&mut machine, icon, 2, &[0x100, 0xe034, 0]), 1);
    assert_eq!(
        call(&mut machine, icon, 18, &[]),
        20,
        "save precisa reter o tipo de grade"
    );
    // A classe atual usa o mesmo contrato, não a tabela de controles do console.
    let current = create(&mut machine, 0x0100_3103);
    assert_eq!(call(&mut machine, current, 26, &[]), 0);
    assert!(
        matches!(
            raw_call(&mut machine, current, 8, &[0x0200_0000]),
            Outcome::Unimplemented { .. }
        ),
        "SINGLE_FRAME não pode anunciar uma vista que ainda não desenhamos"
    );
    cleanup(machine, root);
}

#[test]
fn legacy_menu_scoped_softkey_flag_preserves_icon_grid_label_navigation_and_save() {
    let (mut machine, root, icon) = icon_grid_fixture();
    // AEEMenu.h: NO_ARROWS + ICON_TEXT_TOP. Sem SINGLE_FRAME, continuam várias células.
    call(&mut machine, icon, 8, &[0x0082_0000]);
    call(&mut machine, icon, 4, &[1]);
    call(&mut machine, icon, 3, &[]);
    let before = machine.screen().to_rgb565_bytes();
    // Máscara observada em Raiden: MAXSOFTKEYITEMS não altera a vista IconView.
    call(&mut machine, icon, 8, &[0x0092_0000]);
    assert_eq!(call(&mut machine, icon, 9, &[]), 0x0092_0000);
    let labels_before = machine.drawn_text().count();
    call(&mut machine, icon, 3, &[]);
    assert_eq!(machine.screen().to_rgb565_bytes(), before);
    let labels = machine
        .drawn_text()
        .skip(labels_before)
        .filter(|row| row.3.starts_with("Icon"))
        .collect::<Vec<_>>();
    assert_eq!(labels.len(), 1);
    assert_eq!((labels[0].3.as_str(), labels[0].2), ("Icon1", 2));
    let red_rows = (20..64)
        .filter(|y| (0..28).any(|x| machine.screen().get_pixel(x, *y) == 0xf800))
        .collect::<Vec<_>>();
    assert_eq!(
        red_rows.len(),
        24,
        "três linhas de ícones 8x8, não SingleFrame"
    );
    assert_eq!(call(&mut machine, icon, 2, &[0x100, 0xe034, 0]), 1);
    assert_eq!(call(&mut machine, icon, 18, &[]), 20);
    let saved = machine.grava_estado();
    call(&mut machine, icon, 8, &[0]);
    call(&mut machine, icon, 17, &[50]);
    machine.restaura_estado(&saved).unwrap();
    assert_eq!(call(&mut machine, icon, 9, &[]), 0x0092_0000);
    assert_eq!(call(&mut machine, icon, 18, &[]), 20);
    assert_eq!(call(&mut machine, icon, 2, &[0x100, 0xe032, 0]), 1);
    assert_eq!(call(&mut machine, icon, 18, &[]), 40);
    cleanup(machine, root);
}

#[test]
fn legacy_menu_scoped_softkey_flag_keeps_unsupported_icon_properties_diagnostic() {
    let (mut machine, root) = machine();
    let icon = create(&mut machine, 0x0100_3003);
    for unsupported in [0x0200_0000, 0x0800_0000, 0x1000_0000, 0x0000_8000] {
        assert!(matches!(
            raw_call(&mut machine, icon, 8, &[0x0010_0000 | unsupported]),
            Outcome::Unimplemented { .. }
        ));
        assert_eq!(
            call(&mut machine, icon, 9, &[]),
            0,
            "não armazenar propriedades recusadas"
        );
    }
    cleanup(machine, root);
}

#[test]
fn list_control_shows_only_the_selected_text_and_always_wraps_up_and_down() {
    let (mut machine, root) = machine();
    let list = create(&mut machine, 0x01003002);
    let rect = [0i16, 0, 120, 160]
        .into_iter()
        .flat_map(i16::to_le_bytes)
        .collect::<Vec<_>>();
    machine.cpu_mut().write_mem(OTHER, &rect).unwrap();
    call(&mut machine, list, 6, &[OTHER]);
    for (i, label) in ["Choice A", "Choice B", "Choice C"].into_iter().enumerate() {
        write_wide(&mut machine, WIDE, label);
        let item = [WIDE, 0, 0, 0, 0x80000000, ((i + 1) as u32) << 16, 0];
        machine
            .cpu_mut()
            .write_mem(
                OTHER,
                &item
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        assert_eq!(call(&mut machine, list, 13, &[OTHER]), 1);
    }
    call(&mut machine, list, 8, &[0]);
    assert_ne!(call(&mut machine, list, 9, &[]) & 0x10000, 0);
    call(&mut machine, list, 4, &[1]);
    let before = machine.drawn_text().count();
    call(&mut machine, list, 3, &[]);
    let labels = machine
        .drawn_text()
        .skip(before)
        .filter(|row| row.3.starts_with("Choice"))
        .map(|row| row.3.as_str())
        .collect::<Vec<_>>();
    assert_eq!(labels, ["Choice A"]);
    assert_eq!(call(&mut machine, list, 2, &[0x100, 0xe031, 0]), 1);
    assert_eq!(call(&mut machine, list, 18, &[]), 3);
    let saved = machine.grava_estado();
    call(&mut machine, list, 17, &[2]);
    machine.restaura_estado(&saved).unwrap();
    assert_eq!(call(&mut machine, list, 2, &[0x100, 0xe032, 0]), 1);
    assert_eq!(call(&mut machine, list, 18, &[]), 1);
    let current = create(&mut machine, 0x01003102);
    assert_eq!(call(&mut machine, current, 26, &[]), 0);
    cleanup(machine, root);
}
