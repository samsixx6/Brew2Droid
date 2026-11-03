use std::path::PathBuf;
use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend, Reg};
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::save_state::{self, Leitor};
use zeebx::storage::StoragePaths;

const MODULE: &str = "2f89e3126a26eed73a2451621a07636b9cce1db005138c708131919d7024bb64";
const MIF: &str = "e11d8cf13406b75f309fd1345a5879c4ec3f5f077439a4e2d97c2479514e44ea";
const MASTER: &str = "7eab58a8392c02e8ffce76e1afdb0516f9f4586480dbeaeecc14c7c0c9837358";
const MASTER_MIF: &str = "1d9893e4c7d54ef7138706b1dc0dac962e6d6442a0aad30033198061b31c798e";
const TEXT: u32 = loader::STACK_BASE + 0x1800;
const OUTPUT: u32 = TEXT + 0x400;
const DISPLAY: u32 = loader::OBJECT_BASE + 0x10;

fn machine() -> (Machine<BackendPadrao>, PathBuf) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("local-tests/startup-repairs-wave21-20261031/graphics")
        .join(format!(
            "font-unit-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
    std::fs::create_dir_all(&root).unwrap();
    let image = ModImage::parse(
        [0xe3a0_0000u32, 0xe12f_ff1e]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect(),
    )
    .unwrap();
    let storage = StoragePaths::from_root(root.join("runtime"));
    storage.create_dirs().unwrap();
    let mut m = Machine::new_with_storage(
        BackendPadrao::new().unwrap(),
        loader::load_with(&image, &[]).unwrap(),
        &root,
        &storage,
        None,
    );
    m.enable_handset_clock();
    assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
    (m, root)
}

fn call(m: &mut Machine<BackendPadrao>, name: &str, args: [u32; 4], stack: &[u32]) -> u32 {
    let slot = (0..256)
        .find(|&slot| Interface::Display.method(slot) == Some(name))
        .unwrap();
    let old = m.cpu().read_reg(Reg::Sp);
    let scratch = loader::STACK_BASE + 0x400;
    for (i, &v) in stack.iter().enumerate() {
        m.cpu_mut().write_u32(scratch + i as u32 * 4, v).unwrap();
    }
    m.cpu_mut().write_reg(Reg::Sp, scratch);
    let result = m
        .call_guest(aee::encode(Interface::Display, slot), args, 10000)
        .unwrap();
    m.cpu_mut().write_reg(Reg::Sp, old);
    match result {
        Outcome::Returned { code } => code,
        other => panic!("{name}: {other:?}"),
    }
}

fn write_units(m: &mut Machine<BackendPadrao>, units: &[u16]) {
    m.cpu_mut()
        .write_mem(
            TEXT,
            &units
                .iter()
                .copied()
                .chain([0])
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
}
fn power(m: &mut Machine<BackendPadrao>) {
    // Bytes originais CP949: c6 c4 bf f6 3a 00 34 00 35 00.
    write_units(m, &[0xc4c6, 0xf6bf, 0x3a, 0x34, 0x35]);
}
fn metrics(m: &mut Machine<BackendPadrao>, font: u32) -> (u32, u32, u32) {
    let height = call(
        m,
        "GetFontMetrics",
        [DISPLAY, font, OUTPUT, OUTPUT + 4],
        &[],
    );
    (
        height,
        m.cpu().read_u32(OUTPUT).unwrap(),
        m.cpu().read_u32(OUTPUT + 4).unwrap(),
    )
}
fn width(m: &mut Machine<BackendPadrao>, font: u32, limit: u32) -> (u32, u32) {
    m.cpu_mut().write_u32(OUTPUT, 0xfefefefe).unwrap();
    let width = call(
        m,
        "MeasureTextEx",
        [DISPLAY, font, TEXT, u32::MAX],
        &[limit, OUTPUT],
    );
    (width, m.cpu().read_u32(OUTPUT).unwrap())
}
fn cleanup(m: Machine<BackendPadrao>, root: PathBuf) {
    drop(m);
    let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    assert!(root.canonicalize().unwrap().starts_with(parent));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn master_sword_dialogue_fits_its_original_thirteen_pixel_row_grid() {
    let (mut m, root) = machine();
    m.set_native_text_identity(MASTER, MASTER_MIF);
    // O chamador original desenha em y=195,208,221, começando em x=63
    // dentro do painel x=60..187. As linhas já vêm quebradas no executável.
    // A rotina nativa em0x454aa soma6 para ASCII e12 para Hangul.
    for (line, expected_width) in [
        ("너무나 강대한 힘때", 108),
        ("문에 우주에 버려진 ", 114),
        ("최강의 검. 얼티밋", 102),
    ] {
        let units: Vec<_> = line.encode_utf16().collect();
        write_units(&mut m, &units);
        let (pixels, fits) = width(&mut m, 0x8000, 125);
        assert_eq!(
            fits,
            units.len() as u32,
            "linha original deve caber inteira"
        );
        assert_eq!(pixels, expected_width);
        assert!(pixels <= 125);
    }
    let (height, ascent, descent) = metrics(&mut m, 0x8000);
    assert_eq!(height, ascent + descent);
    assert!(height <= 13, "altura {height} sobrepõe as linhas de 13px");
    cleanup(m, root);
}

#[test]
fn master_sword_dialogue_raster_keeps_the_last_glyph_inside_the_original_panel() {
    let (mut m, root) = machine();
    m.set_native_text_identity(MASTER, MASTER_MIF);
    write_units(
        &mut m,
        &"너무나 강대한 힘때".encode_utf16().collect::<Vec<_>>(),
    );
    call(&mut m, "SetColor", [DISPLAY, 1, 0xffff_ff00, 0], &[]);
    call(
        &mut m,
        "DrawText",
        [DISPLAY, 0x8000, TEXT, u32::MAX],
        &[0, 40, 0, 0],
    );
    let right = m.screen().width() - 125;
    call(
        &mut m,
        "DrawText",
        [DISPLAY, 0x8000, TEXT, u32::MAX],
        &[right, 0, 0, 0],
    );
    let mut ink = 0;
    for y in 0..24 {
        for x in 0..180 {
            let reference = m.screen().get_pixel(x, y + 40);
            if reference != 0 {
                ink += 1;
            }
            if y >= 13 {
                assert_eq!(reference, 0, "tinta invade a próxima linha");
            }
            if x >= 125 {
                assert_eq!(reference, 0, "glifo em x{x} seria cortado pelo painel");
            } else {
                assert_eq!(m.screen().get_pixel(x + right as i32, y), reference);
            }
        }
    }
    assert!(ink > 40, "o controle deve desenhar glifos reais");
    assert_eq!(m.drawn_text().last().unwrap().3, "너무나 강대한 힘때");
    cleanup(m, root);
}

#[test]
fn master_sword_font_profile_requires_both_hashes_and_preserves_other_font_ids() {
    let (mut m, root) = machine();
    let baseline = metrics(&mut m, 0x8000);
    let other = metrics(&mut m, 0x8001);
    let wrong = "0".repeat(64);
    for (module, mif) in [(MASTER, wrong.as_str()), (wrong.as_str(), MASTER_MIF)] {
        m.set_native_text_identity(module, mif);
        assert_eq!(metrics(&mut m, 0x8000), baseline);
    }
    m.set_native_text_identity(&MASTER.to_uppercase(), &MASTER_MIF.to_uppercase());
    assert!(metrics(&mut m, 0x8000).0 <= 13);
    assert_eq!(metrics(&mut m, 0x8001), other);
    m.set_native_text_identity("", "");
    assert_eq!(metrics(&mut m, 0x8000), baseline);
    cleanup(m, root);
}

#[test]
fn bowling_normal_fits_all_native_stat_words_in_the_guests_fixed_panel() {
    let (mut m, root) = machine();
    assert!(m.set_native_text_identity(MODULE, MIF));
    // 0x225ac..0x225c8 reserva 43+10, 0x2294c soma 5: restam 48px.
    for words in [
        &[0xc4c6, 0xf6bf, 0x3a, 0x34, 0x35][..],
        &[0xbabd, 0xc9c7, 0x3a, 0x34, 0x35][..],
    ] {
        write_units(&mut m, words);
        let (pixels, fits) = width(&mut m, 0x8000, 48);
        assert_eq!(
            fits, 5,
            "o ultimo digito precisa caber no painel nativo de 48px"
        );
        assert!(pixels <= 48);
    }
    let (height, ascent, descent) = metrics(&mut m, 0x8000);
    assert_eq!(height, ascent + descent);
    // O caller em 0x15b84..0x15b9c limita a base de layout a12, com passo13.
    assert!(
        height <= 13,
        "fonte com altura {height} sobrepoe as linhas originais de13px"
    );
    cleanup(m, root);
}

#[test]
fn bowling_font_height_agrees_with_the_original_arm_layout_clamp() {
    let (mut m, root) = machine();
    assert!(m.set_native_text_identity(MODULE, MIF));
    let height = metrics(&mut m, 0x8000).0;
    // Instruções originais 0x15b78..0x15b9c, seguidas por BX LR.
    // O teste executa a decisão do guest em vez de reproduzi-la em Rust.
    let code = loader::HEAP_BASE + 0x100;
    let app = loader::HEAP_BASE + 0x4000;
    let instructions = [
        0xe2845d40u32,
        0xe585026c,
        0xe58507dc,
        0xe350000e,
        0xc3a0000c,
        0xca000002,
        0xe350000c,
        0xa2400001,
        0xb3a0000a,
        0xe585026c,
        0xe12fff1e,
    ];
    m.cpu_mut()
        .write_mem(
            code,
            &instructions
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    m.cpu_mut().write_reg(Reg::R4, app);
    assert!(matches!(
        m.call_guest(code, [height, 0, 0, 0], 1000).unwrap(),
        Outcome::Returned { .. }
    ));
    let raw = m.cpu().read_u32(app + 0x17dc).unwrap();
    let layout = m.cpu().read_u32(app + 0x126c).unwrap();
    assert_eq!(raw, height);
    assert!(
        height <= layout + 1,
        "altura real {height} excede o passo {} produzido pelo guest",
        layout + 1
    );
    cleanup(m, root);
}

#[test]
fn bowling_panel_raster_keeps_the_same_last_digit_as_an_unclipped_draw() {
    let (mut m, root) = machine();
    assert!(m.set_native_text_identity(MODULE, MIF));
    power(&mut m);
    call(&mut m, "SetColor", [DISPLAY, 1, 0x00ff_ffff, 0], &[]);
    call(
        &mut m,
        "DrawText",
        [DISPLAY, 0x8000, TEXT, u32::MAX],
        &[0, 32, 0, 0],
    );
    call(
        &mut m,
        "DrawText",
        [DISPLAY, 0x8000, TEXT, u32::MAX],
        &[192, 0, 0, 0],
    );
    let mut ink = 0;
    for y in 0..24 {
        for x in 0..80 {
            let expected = m.screen().get_pixel(x, y + 32);
            if expected != 0 {
                ink += 1;
            }
            if x >= 48 {
                assert_eq!(
                    expected, 0,
                    "tinta do stat em x{x} seria cortada na borda do painel"
                );
            } else {
                assert_eq!(
                    m.screen().get_pixel(x + 192, y),
                    expected,
                    "raster cortado em ({x},{y})"
                );
            }
        }
    }
    assert!(ink > 40, "controle precisa desenhar glifos reais");
    let (measured, fits) = width(&mut m, 0x8000, u32::MAX);
    assert_eq!(fits, 5);
    assert!(
        (0..80)
            .filter(|&x| (0..24).any(|y| m.screen().get_pixel(x, y + 32) != 0))
            .all(|x| x < measured as i32)
    );
    cleanup(m, root);
}

#[test]
fn bowling_font_profile_requires_both_hashes_and_leaves_other_logical_fonts_unchanged() {
    let (mut m, root) = machine();
    // Unicode para o controle nao perfilado.
    write_units(&mut m, &[0xd30c, 0xc6cc, 0x3a, 0x34, 0x35]);
    let baseline = (metrics(&mut m, 0x8000), width(&mut m, 0x8000, u32::MAX));
    assert!(baseline.0.0 > 13);
    m.set_native_text_identity(MODULE, &"0".repeat(64));
    assert_eq!(
        (metrics(&mut m, 0x8000), width(&mut m, 0x8000, u32::MAX)),
        baseline
    );
    m.set_native_text_identity(&"0".repeat(64), MIF);
    assert_eq!(
        (metrics(&mut m, 0x8000), width(&mut m, 0x8000, u32::MAX)),
        baseline
    );
    assert!(m.set_native_text_identity(&MODULE.to_uppercase(), &MIF.to_uppercase()));
    power(&mut m);
    assert_eq!(
        (metrics(&mut m, 0x8001), width(&mut m, 0x8001, u32::MAX)),
        baseline
    );
    let profiled = (metrics(&mut m, 0x8000), width(&mut m, 0x8000, 48));
    let saved = m.grava_estado();
    m.set_native_text_identity(MODULE, &"0".repeat(64));
    assert!(m.restaura_estado(&saved).is_err());
    m.set_native_text_identity(MODULE, MIF);
    m.restaura_estado(&saved).unwrap();
    assert_eq!(
        (metrics(&mut m, 0x8000), width(&mut m, 0x8000, 48)),
        profiled
    );
    let reader = Leitor::abre(&saved).unwrap();
    let legacy = save_state::escreve(
        &reader
            .nomes()
            .into_iter()
            .filter(|name| *name != "text.native")
            .map(|name| (name.to_owned(), reader.secao(name).unwrap().to_vec()))
            .collect::<Vec<_>>(),
    );
    m.restaura_estado(&legacy).unwrap();
    assert_eq!(
        (metrics(&mut m, 0x8000), width(&mut m, 0x8000, 48)),
        profiled,
        "save antigo conserva a identidade verificada pelo launcher"
    );
    m.set_native_text_identity("", " ");
    write_units(&mut m, &[0xd30c, 0xc6cc, 0x3a, 0x34, 0x35]);
    assert_eq!(
        (metrics(&mut m, 0x8000), width(&mut m, 0x8000, u32::MAX)),
        baseline
    );
    cleanup(m, root);
}
