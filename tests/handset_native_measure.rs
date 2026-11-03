use std::path::PathBuf;
use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend, Reg};
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

const MODULE: &str = "8622d31bcb9b5649a14b7dd5894e6e1902c1a9f2f3281984a8dc138c9f814b9e";
const MIF: &str = "afce4811ad7c04b7487bc9657b81d8f2c81285114f6413cbbc1b21d2cf995cde";
const TEXT: u32 = loader::STACK_BASE + 0x1800;
const INFO: u32 = TEXT + 0x600;

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

fn metric_font() -> zeebx::video::font::Font {
    let f = egui::FontDefinitions::default();
    zeebx::video::font::Font::load(f.font_data["Ubuntu-Light"].font.to_vec(), "fixture".into())
        .unwrap()
}
fn measured(native: bool) -> (Machine<BackendPadrao>, PathBuf) {
    let (mut m, r) = machine_with_font(true);
    if native {
        assert!(m.set_native_text_identity(MODULE, MIF));
    }
    (m, r)
}
fn packed(bytes: &[u8]) -> Vec<u16> {
    let mut b = bytes.to_vec();
    if b.len() % 2 != 0 {
        b.push(0);
    }
    b.chunks_exact(2)
        .map(|p| u16::from_le_bytes([p[0], p[1]]))
        .collect()
}
fn install(m: &mut Machine<BackendPadrao>, units: &[u16]) -> Vec<u8> {
    m.cpu_mut().write_mem(TEXT - 16, &[0xa5; 256]).unwrap();
    write_units(m, units);
    let mut b = vec![0; 256];
    m.cpu().read_mem(TEXT - 16, &mut b).unwrap();
    b
}
fn unchanged(m: &Machine<BackendPadrao>, before: &[u8]) {
    let mut a = vec![0; before.len()];
    m.cpu().read_mem(TEXT - 16, &mut a).unwrap();
    assert_eq!(a, before);
}
fn measure(m: &mut Machine<BackendPadrao>, chars: i32, width: i32) -> (u32, u32) {
    m.cpu_mut().write_u32(INFO, 0xa5a5a5a5).unwrap();
    let w = call(
        m,
        Interface::Display,
        "MeasureTextEx",
        [loader::OBJECT_BASE + 0x10, 0x8000, TEXT, chars as u32],
        &[width as u32, INFO],
    );
    (w, m.cpu().read_u32(INFO).unwrap())
}
#[test]
fn primary_unlimited_mixed_ksc_returns_full_raster_width_and_word_count() {
    let (mut m, r) = measured(true);
    let raw = b"\xb7\xce\xbc\xad \xc1\xa4\xba\xb8\xc5\xeb\xbd\xc5\xb8\xc1\xc0\xcc";
    assert_eq!(raw.len(), 17);
    let u = packed(raw);
    assert_eq!(u.len(), 9);
    let b = install(&mut m, &u);
    // O ultimo NUL pertence ao buffer estreito empacotado. Mantemos exatamente a
    // decodificacao existente do DrawText; nao se muda codec, fonte ou texto.
    let expected = metric_font().width("로서 정보통신망이\0", 16.0);
    assert_eq!(measure(&mut m, -1, -1), (expected, 9));
    unchanged(&m, &b);
    cleanup(m, r);
}
#[test]
fn valid_native_full_text_does_not_fail_on_an_incomplete_internal_word_prefix() {
    let (mut m, r) = measured(true);
    let b = install(&mut m, &packed(b"A\xc7\xd1!"));
    assert_eq!(
        measure(&mut m, -1, -1),
        (metric_font().width("A한!", 16.0), 2)
    );
    unchanged(&m, &b);
    cleanup(m, r);
}
#[test]
fn bounded_native_measure_reports_only_complete_word_boundaries() {
    let (mut m, r) = measured(true);
    let b = install(&mut m, &packed(b"A\xc7\xd1B\xc7\xd1"));
    let width = metric_font().width("A한B", 16.0);
    assert_eq!(measure(&mut m, -1, width as i32), (width, 2));
    assert_eq!(measure(&mut m, -1, 0), (0, 0));
    unchanged(&m, &b);
    cleanup(m, r);
}
#[test]
fn explicit_native_nchars_preserves_odd_even_original_word_counts() {
    let (mut m, r) = measured(true);
    let b = install(&mut m, &packed(b"AB\xc7\xd1CD\xc7\xd1"));
    for (n, text) in [
        (0, ""),
        (1, "AB"),
        (2, "AB한"),
        (3, "AB한CD"),
        (4, "AB한CD한"),
    ] {
        assert_eq!(
            measure(&mut m, n, -1),
            (metric_font().width(text, 16.0), n as u32)
        );
    }
    unchanged(&m, &b);
    cleanup(m, r);
}
#[test]
fn explicit_native_count_ending_inside_a_dbcs_pair_fails_as_zero_width() {
    let (mut m, r) = measured(true);
    let b = install(&mut m, &packed(b"A\xc7\xd1!"));
    // AEEIDisplay.h: falha retorna largura0; pnFits0 e politica conservadora.
    assert_eq!(measure(&mut m, 1, -1), (0, 0));
    unchanged(&m, &b);
    cleanup(m, r);
}
#[test]
fn malformed_selected_native_text_fails_with_zero_width_and_zero_fits() {
    let (mut m, r) = measured(true);
    let b = install(&mut m, &[0xffff]);
    assert_eq!(measure(&mut m, -1, -1), (0, 0));
    unchanged(&m, &b);
    cleanup(m, r);
}
#[test]
fn native_odd_ascii_tail_and_null_fits_pointer_keep_existing_units() {
    let (mut m, r) = measured(true);
    let u = packed(b"\xc7\xd1A");
    let b = install(&mut m, &u);
    let w = call(
        &mut m,
        Interface::Display,
        "MeasureTextEx",
        [loader::OBJECT_BASE + 0x10, 0x8000, TEXT, u32::MAX],
        &[u32::MAX, 0],
    );
    assert_eq!(w, metric_font().width("한A", 16.0));
    unchanged(&m, &b);
    cleanup(m, r);
}
#[test]
fn unicode_latin_and_cjk_widths_remain_unchanged() {
    let (mut m, r) = measured(false);
    for s in ["ABC", "Café", "한글", "A한!"] {
        let u: Vec<u16> = s.encode_utf16().collect();
        let b = install(&mut m, &u);
        assert_eq!(
            measure(&mut m, -1, -1),
            (metric_font().width(s, 16.0), u.len() as u32)
        );
        unchanged(&m, &b);
    }
    let b = install(&mut m, &"ABC".encode_utf16().collect::<Vec<_>>());
    let w = metric_font().width("AB", 16.0);
    assert_eq!(measure(&mut m, -1, w as i32), (w, 2));
    unchanged(&m, &b);
    cleanup(m, r);
}
#[test]
fn unicode_lossy_surrogate_boundary_remains_unchanged() {
    let (mut m, r) = measured(false);
    let b = install(&mut m, &[0x0041, 0xd83d, 0xde00, 0x0042]);
    assert_eq!(measure(&mut m, 2, -1), (metric_font().width("A�", 16.0), 2));
    unchanged(&m, &b);
    cleanup(m, r);
}
