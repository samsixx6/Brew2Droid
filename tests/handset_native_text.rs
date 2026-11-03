use std::path::PathBuf;
use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend, Reg};
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, NativeTextEncoding, Outcome};
use zeebx::save_state::{self, Leitor};
use zeebx::storage::StoragePaths;

const MODULE: &str = "8622d31bcb9b5649a14b7dd5894e6e1902c1a9f2f3281984a8dc138c9f814b9e";
const MIF: &str = "afce4811ad7c04b7487bc9657b81d8f2c81285114f6413cbbc1b21d2cf995cde";
const TEXT: u32 = loader::STACK_BASE + 0x1800;
const BUFFER: u32 = TEXT + 0x200;
const FILE: u32 = TEXT + 0x400;
const INFO: u32 = TEXT + 0x600;
const NEW_MODULE: &str = "e14a20c0029c38253b9c8e8ac4e5efbd15bd80e60eed5338064c3f1a35c1e76d";
const NEW_MIF: &str = "314e5ebd26a22623854d9562c3aeac30481a3def118f5b460fd8168d535dd3fc";

fn machine() -> (Machine<BackendPadrao>, PathBuf) {
    machine_with_font(false)
}

fn machine_with_font(font: bool) -> (Machine<BackendPadrao>, PathBuf) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("local-tests")
        .join(format!(
            "native-text-contract-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
    std::fs::create_dir(&root).unwrap();
    let storage = StoragePaths::from_root(root.join("runtime"));
    storage.create_dirs().unwrap();
    if font {
        let fonts = egui::FontDefinitions::default();
        std::fs::write(
            root.join("fixture.ttf"),
            &fonts.font_data["Ubuntu-Light"].font,
        )
        .unwrap();
    }
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
        &root,
        &storage,
        None,
    );
    machine.enable_handset_clock();
    assert_eq!(machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
    (machine, root)
}

fn call(
    machine: &mut Machine<BackendPadrao>,
    interface: Interface,
    name: &str,
    args: [u32; 4],
    stack: &[u32],
) -> u32 {
    let slot = (0..256)
        .find(|slot| interface.method(*slot) == Some(name))
        .unwrap();
    let old_sp = machine.cpu().read_reg(Reg::Sp);
    let scratch = loader::STACK_BASE + 0x400;
    for (index, value) in stack.iter().enumerate() {
        machine
            .cpu_mut()
            .write_u32(scratch + index as u32 * 4, *value)
            .unwrap();
    }
    machine.cpu_mut().write_reg(Reg::Sp, scratch);
    let outcome = machine
        .call_guest(aee::encode(interface, slot), args, 10_000)
        .unwrap();
    machine.cpu_mut().write_reg(Reg::Sp, old_sp);
    match outcome {
        Outcome::Returned { code } => code,
        other => panic!("{name}: {other:?}"),
    }
}

fn write_units(machine: &mut Machine<BackendPadrao>, units: &[u16]) {
    machine
        .cpu_mut()
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

fn draw(machine: &mut Machine<BackendPadrao>, pointer: u32) -> u32 {
    call(
        machine,
        Interface::Display,
        "DrawText",
        [loader::OBJECT_BASE + 0x10, 0x8000, pointer, u32::MAX],
        &[0, 0, 0, 0],
    )
}

fn bar(raw: &[u8]) -> Vec<u8> {
    let mut bar = vec![0; 48];
    bar[..2].copy_from_slice(&0x11u16.to_le_bytes());
    bar[6..8].copy_from_slice(&1u16.to_le_bytes());
    for (at, value) in [
        (8, 32u32),
        (12, 8),
        (16, 40),
        (20, 1),
        (32, 0x0007_0001),
        (36, 0),
        (40, 48),
        (44, 48 + raw.len() as u32),
    ] {
        bar[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    bar.extend_from_slice(raw);
    bar
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
fn native_encoding_requires_both_audited_identities_and_reports_the_sdk_encoding() {
    let (mut machine, root) = machine();
    assert!(machine.set_native_text_identity(MODULE, MIF));
    assert_eq!(machine.native_text_encoding(), NativeTextEncoding::EucKr);
    machine.cpu_mut().write_mem(INFO, &[0; 64]).unwrap();
    call(
        &mut machine,
        Interface::Shell,
        "GetDeviceInfo",
        [loader::OBJECT_BASE, INFO, 0, 0],
        &[],
    );
    let mut encoding = [0; 2];
    machine.cpu().read_mem(INFO + 10, &mut encoding).unwrap();
    assert_eq!(u16::from_le_bytes(encoding), 0xfe);
    assert!(!machine.set_native_text_identity(MODULE, &"0".repeat(64)));
    assert_eq!(machine.native_text_encoding(), NativeTextEncoding::Unicode);
    assert!(!machine.set_native_text_identity(&"0".repeat(64), MIF));
    assert_eq!(machine.native_text_encoding(), NativeTextEncoding::Unicode);
    cleanup(machine, root);
}

#[test]
fn packed_native_aechar_is_decoded_before_rendering_without_modifying_guest_words() {
    let (mut machine, root) = machine();
    machine.set_native_text_identity(MODULE, MIF);
    // KS X 1001 C7 D1/B1 DB ficam nos AECHARs ARM D1C7/DBB1, não em UTF-16.
    write_units(&mut machine, &[0xd1c7, 0x20, 0xdbb1]);
    assert_eq!(draw(&mut machine, TEXT), 0);
    assert_eq!(machine.drawn_text().last().unwrap().3, "한 글");
    let mut actual = [0; 8];
    machine.cpu().read_mem(TEXT, &mut actual).unwrap();
    assert_eq!(actual, [0xc7, 0xd1, 0x20, 0, 0xb1, 0xdb, 0, 0]);
    cleanup(machine, root);
}

#[test]
fn unprofiled_utf16_is_not_reinterpreted_as_a_native_codepage() {
    let (mut machine, root) = machine();
    assert_eq!(machine.native_text_encoding(), NativeTextEncoding::Unicode);
    write_units(&mut machine, &[0xd55c, 0x20, 0xae00]);
    assert_eq!(draw(&mut machine, TEXT), 0);
    assert_eq!(machine.drawn_text().last().unwrap().3, "한 글");
    cleanup(machine, root);
}

#[test]
fn normalized_resources_return_native_guest_words_and_the_correct_character_count() {
    let (mut machine, root) = machine();
    machine.set_native_text_identity(MODULE, MIF);
    std::fs::write(
        root.join("native.bar"),
        bar(&[0xfe, 0xfe, 0xc7, 0xd1, 0xb1, 0xdb]),
    )
    .unwrap();
    machine.cpu_mut().write_mem(FILE, b"native.bar\0").unwrap();
    machine.cpu_mut().write_mem(BUFFER, &[0xa5; 12]).unwrap();
    assert_eq!(
        call(
            &mut machine,
            Interface::Shell,
            "LoadResString",
            [loader::OBJECT_BASE, FILE, 7, BUFFER],
            &[8]
        ),
        2
    );
    let mut bytes = [0; 12];
    machine.cpu().read_mem(BUFFER, &mut bytes).unwrap();
    assert_eq!(
        bytes,
        [
            0xc7, 0xd1, 0xb1, 0xdb, 0, 0, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5
        ]
    );
    assert_eq!(
        call(
            &mut machine,
            Interface::Helpers,
            "wstrlen",
            [BUFFER, 0, 0, 0],
            &[]
        ),
        2
    );
    assert_eq!(draw(&mut machine, BUFFER), 0);
    assert_eq!(machine.drawn_text().last().unwrap().3, "한글");
    cleanup(machine, root);
}

#[test]
fn a_native_resource_cannot_replace_unrepresentable_characters_with_question_marks() {
    let (mut machine, root) = machine();
    machine.set_native_text_identity(MODULE, MIF);
    std::fs::write(
        root.join("emoji.bar"),
        bar(&[0xff, 0xfe, 0x3d, 0xd8, 0, 0xde]),
    )
    .unwrap();
    machine.cpu_mut().write_mem(FILE, b"emoji.bar\0").unwrap();
    machine.cpu_mut().write_mem(BUFFER, &[0xa5; 12]).unwrap();
    assert_eq!(
        call(
            &mut machine,
            Interface::Shell,
            "LoadResString",
            [loader::OBJECT_BASE, FILE, 7, BUFFER],
            &[12]
        ),
        0
    );
    let mut actual = [0; 12];
    machine.cpu().read_mem(BUFFER, &mut actual).unwrap();
    assert_eq!(actual, [0xa5; 12]);
    cleanup(machine, root);
}

#[test]
fn saved_native_context_cannot_be_applied_to_another_selected_identity() {
    let (mut machine, root) = machine();
    machine.set_native_text_identity(MODULE, MIF);
    write_units(&mut machine, &[0xd1c7, 0xdbb1]);
    let saved = machine.grava_estado();
    machine.set_native_text_identity(&"0".repeat(64), &"0".repeat(64));
    assert!(
        machine.restaura_estado(&saved).is_err(),
        "o perfil nativo acompanha o par selecionado"
    );
    machine.set_native_text_identity(MODULE, MIF);
    machine.restaura_estado(&saved).unwrap();
    assert_eq!(draw(&mut machine, TEXT), 0);
    assert_eq!(machine.drawn_text().last().unwrap().3, "한글");
    let reader = Leitor::abre(&saved).unwrap();
    let old = save_state::escreve(
        &reader
            .nomes()
            .into_iter()
            .filter(|name| *name != "text.native")
            .map(|name| (name.to_owned(), reader.secao(name).unwrap().to_vec()))
            .collect::<Vec<_>>(),
    );
    machine.restaura_estado(&old).unwrap();
    assert_eq!(
        machine.native_text_encoding(),
        NativeTextEncoding::EucKr,
        "saves antigos retêm o contexto verificado pelo launcher"
    );
    cleanup(machine, root);
}

#[test]
fn new_native_profiles_require_the_selected_mif_including_verified_companion_versions() {
    let (mut machine, root) = machine();
    let module = "67a61b9d238d10e978dfccc388f98019f6ae6ee73e349932709fb74b4b89754e";
    for mif in [
        "5f3da78ed87d5418972e8c5b4a8fb571d60c9275c41a7c78c6109863bddc5e9e",
        "6e965ef12ca68601861ef17f243464324b5a79802d57e9a94866be94372f4ebe",
    ] {
        assert!(machine.set_handset_runtime_identity(module, mif));
        assert_eq!(machine.native_text_encoding(), NativeTextEncoding::EucKr);
    }
    assert!(!machine.set_handset_runtime_identity(module, NEW_MIF));
    assert_eq!(machine.native_text_encoding(), NativeTextEncoding::Unicode);
    write_units(&mut machine, &[0xd55c, 0xae00]);
    assert_eq!(draw(&mut machine, TEXT), 0);
    assert_eq!(machine.drawn_text().last().unwrap().3, "한글");
    cleanup(machine, root);
}

#[test]
fn new_native_resource_copy_draw_and_measure_keep_the_observed_packed_word_contract() {
    let (mut machine, root) = machine_with_font(true);
    assert!(machine.set_handset_runtime_identity(NEW_MODULE, NEW_MIF));
    // Palavra comum usada como vetor de codec; nenhuma imagem ou recurso do jogo.
    // O primeiro AECHAR D9B4 também ocupa a faixa de surrogates em UTF-16.
    let raw = [0xb4, 0xd9, 0xbf, 0xee, 0xb7, 0xce, 0xb5, 0xe5];
    let mut resource = vec![0xfe, 0xfe];
    resource.extend_from_slice(&raw);
    std::fs::write(root.join("observed-codec.bar"), bar(&resource)).unwrap();
    machine
        .cpu_mut()
        .write_mem(FILE, b"observed-codec.bar\0")
        .unwrap();
    machine.cpu_mut().write_mem(BUFFER, &[0xa5; 16]).unwrap();
    assert_eq!(
        call(
            &mut machine,
            Interface::Shell,
            "LoadResString",
            [loader::OBJECT_BASE, FILE, 7, BUFFER],
            &[16]
        ),
        4
    );
    assert_eq!(
        call(
            &mut machine,
            Interface::Helpers,
            "wstrncopyn",
            [TEXT, 5, BUFFER, u32::MAX],
            &[]
        ),
        4
    );
    let mut actual = [0; 10];
    machine.cpu().read_mem(TEXT, &mut actual).unwrap();
    assert_eq!(&actual[..8], &raw);
    assert_eq!(&actual[8..], &[0, 0]);
    call(
        &mut machine,
        Interface::Display,
        "SetColor",
        [loader::OBJECT_BASE + 0x10, 1, 0xffff_ff00, 0],
        &[],
    );
    let before = machine.screen().to_rgb565_bytes();
    assert_eq!(draw(&mut machine, TEXT), 0);
    assert_eq!(machine.drawn_text().last().unwrap().3, "다운로드");
    assert_ne!(machine.screen().to_rgb565_bytes(), before);
    let fonts = egui::FontDefinitions::default();
    let font = zeebx::video::font::Font::load(
        fonts.font_data["Ubuntu-Light"].font.to_vec(),
        "fixture".into(),
    )
    .unwrap();
    let width = call(
        &mut machine,
        Interface::Display,
        "MeasureTextEx",
        [loader::OBJECT_BASE + 0x10, 0x8000, TEXT, 4],
        &[u32::MAX, INFO],
    );
    assert_eq!(width, font.width("다운로드", 16.0));
    assert_eq!(machine.cpu().read_u32(INFO).unwrap(), 4);
    let width = call(
        &mut machine,
        Interface::Display,
        "MeasureTextEx",
        [loader::OBJECT_BASE + 0x10, 0x8000, TEXT, 4],
        &[font.width("다운", 16.0), INFO],
    );
    assert_eq!(width, font.width("다운", 16.0));
    assert_eq!(machine.cpu().read_u32(INFO).unwrap(), 2);
    cleanup(machine, root);
}

const BOWLING_MODULE: &str = "2f89e3126a26eed73a2451621a07636b9cce1db005138c708131919d7024bb64";
const BOWLING_MIF: &str = "e11d8cf13406b75f309fd1345a5879c4ec3f5f077439a4e2d97c2479514e44ea";
#[test]
fn bowling_native_player_and_stats_decode_the_observed_guest_words() {
    let (mut machine, root) = machine();
    machine.set_handset_runtime_identity(BOWLING_MODULE, BOWLING_MIF);
    write_units(
        &mut machine,
        &[0xc3c7, 0xb9b7, 0xccc0, 0xeebe, 0x20, 0xb1bc, 0xc3c5],
    );
    assert_eq!(draw(&mut machine, TEXT), 0);
    assert_eq!(machine.drawn_text().last().unwrap().3, "플레이어 선택");
    assert_eq!(
        machine.dump(TEXT, 16).unwrap(),
        [
            199, 195, 183, 185, 192, 204, 190, 238, 32, 0, 188, 177, 197, 195, 0, 0
        ]
    );
    write_units(&mut machine, &[0xc4c6, 0xf6bf, 0x3a, 0x34, 0x35]);
    assert_eq!(draw(&mut machine, TEXT), 0);
    assert_eq!(machine.drawn_text().last().unwrap().3, "파워:45");
    assert_eq!(
        machine.dump(TEXT, 12).unwrap(),
        [198, 196, 191, 246, 58, 0, 52, 0, 53, 0, 0, 0]
    );
    write_units(&mut machine, &[0xbabd, 0xc9c7, 0x3a, 0x34, 0x35]);
    assert_eq!(draw(&mut machine, TEXT), 0);
    assert_eq!(machine.drawn_text().last().unwrap().3, "스핀:45");
    assert_eq!(
        machine.dump(TEXT, 12).unwrap(),
        [189, 186, 199, 201, 58, 0, 52, 0, 53, 0, 0, 0]
    );
    write_units(&mut machine, &[0xeeb5, 0xdeb1, 0x3a]);
    assert_eq!(draw(&mut machine, TEXT), 0);
    assert_eq!(machine.drawn_text().last().unwrap().3, "등급:");
    assert_eq!(
        machine.dump(TEXT, 8).unwrap(),
        [181, 238, 177, 222, 58, 0, 0, 0]
    );
    cleanup(machine, root);
}
#[test]
fn bowling_resource_labels_preserve_host_text_and_native_guest_encoding() {
    let (mut machine, root) = machine();
    machine.set_handset_runtime_identity(BOWLING_MODULE, BOWLING_MIF);
    std::fs::write(
        root.join("bowling-label.bar"),
        bar(&[0xfe, 0xfe, 0xbf, 0xc0, 0xb8, 0xa5, 0xbc, 0xd5]),
    )
    .unwrap();
    machine
        .cpu_mut()
        .write_mem(FILE, b"bowling-label.bar\0")
        .unwrap();
    assert_eq!(
        call(
            &mut machine,
            Interface::Shell,
            "LoadResString",
            [loader::OBJECT_BASE, FILE, 7, BUFFER],
            &[32]
        ),
        3
    );
    assert_eq!(
        machine.dump(BUFFER, 8).unwrap(),
        [0xbf, 0xc0, 0xb8, 0xa5, 0xbc, 0xd5, 0, 0]
    );
    assert_eq!(draw(&mut machine, BUFFER), 0);
    assert_eq!(machine.drawn_text().last().unwrap().3, "오른손");
    cleanup(machine, root);
}
#[test]
fn bowling_native_text_requires_both_exact_identity_hashes() {
    let (mut machine, root) = machine();
    for (module, mif) in [
        (BOWLING_MODULE, MIF),
        (MODULE, BOWLING_MIF),
        (BOWLING_MODULE, "unknown"),
        ("unknown", BOWLING_MIF),
    ] {
        assert!(!machine.set_handset_runtime_identity(module, mif));
        assert_eq!(machine.native_text_encoding(), NativeTextEncoding::Unicode);
        write_units(&mut machine, &[0xc624, 0xb978, 0xc190]);
        assert_eq!(draw(&mut machine, TEXT), 0);
        assert_eq!(machine.drawn_text().last().unwrap().3, "오른손");
    }
    cleanup(machine, root);
}
