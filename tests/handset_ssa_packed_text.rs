use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend, CpuError, Reg, StopReason, mem::GuestMemory},
    loader::{self, modfile::ModImage},
    machine::{Machine, NativeTextEncoding, Outcome},
    save_state::{self, Leitor},
    storage::StoragePaths,
};
const SSA: &str = "74968a7d74d9a5ac19b51e00d157abb5d63fd166961b277943bc21c187d15fcf";
const ROOK: &str = "698c839f56b3b7beb096dc1b58a2818afd2253ae0c7d55a6af35b2d8e8ad43af";
const ROOK_MIF: &str = "9bb8fab4fc4fe54b939c1cc654b7591e69b417a3a432f14f10a95a0780aafbfe";
const TEXT: u32 = loader::STACK_BASE + 0x1800;
const OUTPUT: u32 = TEXT + 0x400;
const DISPLAY: u32 = loader::OBJECT_BASE + 0x10;
#[derive(Default)]
struct Reads {
    guard: Option<(u32, u32)>,
    outside: Vec<(u32, usize)>,
}
struct CheckedCpu {
    inner: BackendPadrao,
    reads: Arc<Mutex<Reads>>,
}
impl CpuBackend for CheckedCpu {
    fn reset(&mut self, m: &GuestMemory) -> Result<(), CpuError> {
        self.inner.reset(m)
    }
    fn read_reg(&self, r: Reg) -> u32 {
        self.inner.read_reg(r)
    }
    fn write_reg(&mut self, r: Reg, v: u32) {
        self.inner.write_reg(r, v)
    }
    fn run(&mut self, p: u32, n: u64) -> Result<StopReason, CpuError> {
        self.inner.run(p, n)
    }
    fn instructions(&self) -> u64 {
        self.inner.instructions()
    }
    fn cpsr(&self) -> u32 {
        self.inner.cpsr()
    }
    fn set_cpsr(&mut self, v: u32) {
        self.inner.set_cpsr(v)
    }
    fn set_instructions(&mut self, v: u64) {
        self.inner.set_instructions(v)
    }
    fn em_thumb(&self) -> bool {
        self.inner.em_thumb()
    }
    fn read_mem(&self, a: u32, b: &mut [u8]) -> Result<(), CpuError> {
        let mut r = self.reads.lock().unwrap();
        if let Some((start, end)) = r.guard {
            if a >= start && a < start + 256 && u64::from(a) + b.len() as u64 > u64::from(end) {
                r.outside.push((a, b.len()));
            }
        }
        drop(r);
        self.inner.read_mem(a, b)
    }
    fn write_mem(&mut self, a: u32, b: &[u8]) -> Result<(), CpuError> {
        self.inner.write_mem(a, b)
    }
    fn watch_dirty(&mut self, i: u32, a: u32, n: u32) -> Result<(), CpuError> {
        self.inner.watch_dirty(i, a, n)
    }
    fn unwatch_dirty(&mut self, i: u32) {
        self.inner.unwatch_dirty(i)
    }
    fn take_dirty(&mut self, i: u32) -> bool {
        self.inner.take_dirty(i)
    }
    fn marca_sujo(&mut self, a: u32, n: u32) {
        self.inner.marca_sujo(a, n)
    }
}
type M = Machine<CheckedCpu>;
fn machine() -> (M, PathBuf, Arc<Mutex<Reads>>) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("local-tests/startup-repairs-wave21-20261031/graphics/ssa")
        .join(format!(
            "unit-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
    std::fs::create_dir_all(&root).unwrap();
    let reads = Arc::new(Mutex::new(Reads::default()));
    let cpu = CheckedCpu {
        inner: BackendPadrao::new().unwrap(),
        reads: reads.clone(),
    };
    let image = ModImage::parse(
        [0xe3a00000u32, 0xe12fff1e]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect(),
    )
    .unwrap();
    let storage = StoragePaths::from_root(root.join("runtime"));
    storage.create_dirs().unwrap();
    let mut m = Machine::new_with_storage(
        cpu,
        loader::load_with(&image, &[]).unwrap(),
        &root,
        &storage,
        None,
    );
    m.enable_handset_clock();
    assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
    (m, root, reads)
}
fn cleanup(m: M, root: PathBuf) {
    drop(m);
    assert!(
        root.canonicalize().unwrap().starts_with(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .canonicalize()
                .unwrap()
        )
    );
    std::fs::remove_dir_all(root).unwrap();
}
fn call(m: &mut M, iface: Interface, name: &str, args: [u32; 4], stack: &[u32]) -> u32 {
    let slot = (0..256).find(|&s| iface.method(s) == Some(name)).unwrap();
    let old = m.cpu().read_reg(Reg::Sp);
    let sp = loader::STACK_BASE + 0x400;
    for (i, v) in stack.iter().enumerate() {
        m.cpu_mut().write_u32(sp + i as u32 * 4, *v).unwrap();
    }
    m.cpu_mut().write_reg(Reg::Sp, sp);
    let r = m.call_guest(aee::encode(iface, slot), args, 10000).unwrap();
    m.cpu_mut().write_reg(Reg::Sp, old);
    match r {
        Outcome::Returned { code } => code,
        x => panic!("{name}: {x:?}"),
    }
}
fn write(m: &mut M, bytes: &[u8]) {
    m.cpu_mut().write_mem(TEXT, bytes).unwrap();
}
fn draw(m: &mut M, n: u32) -> u32 {
    call(
        m,
        Interface::Display,
        "DrawText",
        [DISPLAY, 0x8000, TEXT, n],
        &[0, 0, 0, 0x8000],
    )
}
fn measure(m: &mut M, n: u32, max: u32) -> (u32, u32) {
    measure_font(m, n, max, 0x8000)
}
fn measure_font(m: &mut M, n: u32, max: u32, font: u32) -> (u32, u32) {
    m.cpu_mut().write_u32(OUTPUT, 0xfeedface).unwrap();
    let width = call(
        m,
        Interface::Display,
        "MeasureTextEx",
        [DISPLAY, font, TEXT, n],
        &[max, OUTPUT],
    );
    (width, m.cpu().read_u32(OUTPUT).unwrap())
}
const VECTORS: &[(&[u8], &str)] = &[
    (
        &[0x2d, 0xb7, 0xa9, 0xc5, 0xb7, 0xc1, 0xa4, 0xba, 0xb8, 0x2d],
        "-랭킹정보-",
    ),
    (
        &[
            0xb7, 0xa9, 0xc5, 0xb7, 0xbc, 0xad, 0xb9, 0xf6, 0xbf, 0xa1, 0x20, 0xc1, 0xa2, 0xbc,
            0xd3,
        ],
        "랭킹서버에 접속",
    ),
    (
        &[
            0xc7, 0xcf, 0xbd, 0xc3, 0xb0, 0xda, 0xbd, 0xc0, 0xb4, 0xcf, 0xb1, 0xee, 0x3f,
        ],
        "하시겠습니까?",
    ),
    (
        &[
            0x28, 0xc6, 0xd0, 0xc5, 0xb6, 0xbf, 0xe4, 0xb1, 0xdd, 0x20, 0x30, 0xbf, 0xf8, 0x29,
        ],
        "(패킷요금 0원)",
    ),
    (b"1.YES / 2.NO", "1.YES / 2.NO"),
];
#[test]
fn ssa_packed_prompt_decodes_original_bytes_and_preserves_raw_wide_copies() {
    let (mut m, root, _) = machine();
    m.set_native_text_identity(SSA, "");
    for &(bytes, expected) in VECTORS {
        let mut raw = bytes.to_vec();
        raw.extend([0, 0, 0, 0]);
        write(&mut m, &raw);
        assert_eq!(
            call(
                &mut m,
                Interface::Helpers,
                "wstrcpy",
                [TEXT + 128, TEXT, 0, 0],
                &[]
            ),
            TEXT + 128
        );
        assert_eq!(
            m.dump(TEXT + 128, bytes.len()).unwrap(),
            bytes,
            "WSTRCPY deve preservar bytes originais"
        );
        assert_eq!(draw(&mut m, u32::MAX), 0);
        assert_eq!(m.drawn_text().last().unwrap().3, expected);
    }
    cleanup(m, root);
}

#[test]
fn rook_original_inline_text_uses_word_counts_over_packed_bytes() {
    let (mut m, root, _) = machine();
    assert!(m.set_native_text_identity(ROOK, ROOK_MIF));
    // No consumidor original, Exit Game! tem dez bytes e DrawText recebe cinco AECHAR.
    write(&mut m, b"Exit Game!\0\0");
    assert_eq!(draw(&mut m, 5), 0);
    assert_eq!(m.drawn_text().last().unwrap().3, "Exit Game!");
    assert_eq!(draw(&mut m, 3), 0);
    assert_eq!(m.drawn_text().last().unwrap().3, "Exit G");
    for (raw, text) in [
        (b"This is a \0\0".as_slice(), "This is a "),
        (b"Copy Game!\0\0".as_slice(), "Copy Game!"),
    ] {
        write(&mut m, raw);
        assert_eq!(draw(&mut m, u32::MAX), 0);
        assert_eq!(m.drawn_text().last().unwrap().3, text);
    }
    cleanup(m, root);
}

#[test]
fn rook_byte_nul_does_not_read_the_unused_half_word_or_following_memory() {
    let (mut m, root, reads) = machine();
    assert!(m.set_native_text_identity(ROOK, ROOK_MIF));
    write(&mut m, b"This is a\0");
    reads.lock().unwrap().guard = Some((TEXT, TEXT + 10));
    assert_eq!(draw(&mut m, u32::MAX), 0);
    assert_eq!(m.drawn_text().last().unwrap().3, "This is a");
    assert!(reads.lock().unwrap().outside.is_empty());
    cleanup(m, root);
}

#[test]
fn rook_packed_profile_requires_both_original_hashes() {
    let (mut m, root, _) = machine();
    assert!(m.set_native_text_identity(&ROOK.to_uppercase(), &ROOK_MIF.to_uppercase()));
    assert_eq!(m.native_text_encoding(), NativeTextEncoding::PackedEucKr);
    for (module, mif) in [(ROOK, ""), (ROOK, SSA), (SSA, ROOK_MIF), ("", ROOK_MIF)] {
        assert!(!m.set_native_text_identity(module, mif));
        assert_eq!(m.native_text_encoding(), NativeTextEncoding::Unicode);
    }
    cleanup(m, root);
}
#[test]
fn packed_display_stops_at_byte_nul_and_honors_selected_guest_word_bounds() {
    let (mut m, root, reads) = machine();
    m.set_native_text_identity(SSA, "");
    let bytes = VECTORS[2].0;
    let mut raw = bytes.to_vec();
    raw.push(0);
    raw.extend([0xa5; 64]);
    write(&mut m, &raw);
    reads.lock().unwrap().guard = Some((TEXT, TEXT + bytes.len() as u32 + 1));
    assert_eq!(draw(&mut m, u32::MAX), 0);
    let outside = reads.lock().unwrap().outside.clone();
    assert!(
        outside.is_empty(),
        "DrawText leu alem do NUL de byte: {outside:?}"
    );
    assert_eq!(m.drawn_text().last().unwrap().3, VECTORS[2].1);
    reads.lock().unwrap().guard = None;
    write(&mut m, b"1.YES / 2.NO");
    {
        let mut r = reads.lock().unwrap();
        r.guard = Some((TEXT, TEXT + 4));
        r.outside.clear();
    }
    assert_eq!(draw(&mut m, 2), 0);
    assert!(reads.lock().unwrap().outside.is_empty());
    assert_eq!(m.drawn_text().last().unwrap().3, "1.YE");
    {
        let mut r = reads.lock().unwrap();
        r.guard = Some((TEXT, TEXT));
        r.outside.clear();
    }
    assert_eq!(draw(&mut m, 0), 0);
    assert_eq!(measure(&mut m, 0, u32::MAX), (0, 0));
    assert!(reads.lock().unwrap().outside.is_empty());
    cleanup(m, root);
}
#[test]
fn packed_measure_matches_unicode_text_but_fits_counts_guest_words() {
    let (mut m, root, _) = machine();
    for &(bytes, expected) in VECTORS {
        m.set_native_text_identity("", "");
        let wide = expected
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        write(&mut m, &wide);
        let expected_width = measure_font(&mut m, u32::MAX, u32::MAX, 0x8001).0;
        m.set_native_text_identity(SSA, "");
        let mut raw = bytes.to_vec();
        raw.extend([0, 0]);
        write(&mut m, &raw);
        assert_eq!(
            measure_font(&mut m, u32::MAX, u32::MAX, 0x8001),
            (expected_width, bytes.len().div_ceil(2) as u32)
        );
    }
    m.set_native_text_identity(SSA, "");
    write(&mut m, &[b'-', 0xb7, 0x00, 0]);
    assert_eq!(
        draw(&mut m, 1),
        14,
        "um limite de palavra nao pode renderizar metade de um par CP949"
    );
    assert_eq!(measure(&mut m, 1, u32::MAX), (0, 0));
    cleanup(m, root);
}
#[test]
fn packed_profile_requires_the_audited_module_and_actual_manifest_absence() {
    let (mut m, root, _) = machine();
    let other = "0".repeat(64);
    for (module, mif) in [(other.as_str(), ""), (SSA, " "), (SSA, other.as_str())] {
        assert!(!m.set_native_text_identity(module, mif));
        assert_eq!(m.native_text_encoding(), NativeTextEncoding::Unicode);
    }
    assert!(m.set_native_text_identity(&SSA.to_uppercase(), ""));
    assert_ne!(m.native_text_encoding(), NativeTextEncoding::Unicode);
    m.set_native_text_identity("", "");
    write(&mut m, &[b'A', 0, b'B', 0, 0, 0]);
    assert_eq!(draw(&mut m, u32::MAX), 0);
    assert_eq!(m.drawn_text().last().unwrap().3, "AB");
    cleanup(m, root);
}
#[test]
fn ssa_normal_font_fits_the_dialogs_compiled_thirteen_pixel_grid() {
    let (mut m, root, _) = machine();
    m.set_native_text_identity(SSA, "");
    let height = call(
        &mut m,
        Interface::Display,
        "GetFontMetrics",
        [DISPLAY, 0x8000, OUTPUT, OUTPUT + 4],
        &[],
    );
    // O caller original posiciona as linhas em y+28,+41,+54: passo13.
    assert!(
        height > 0 && height <= 13,
        "altura {height} invade a proxima linha do dialogo"
    );
    assert_eq!(
        height,
        m.cpu().read_u32(OUTPUT).unwrap() + m.cpu().read_u32(OUTPUT + 4).unwrap()
    );
    for &(bytes, _) in VECTORS {
        let mut raw = bytes.to_vec();
        raw.extend([0, 0]);
        write(&mut m, &raw);
        // RSB #120 em 0x112bc centraliza as linhas nesta janela logica.
        assert!(measure(&mut m, u32::MAX, u32::MAX).0 <= 120);
    }
    cleanup(m, root);
}
#[test]
fn packed_missing_manifest_identity_survives_restore_and_rejects_a_real_mif() {
    let (mut m, root, _) = machine();
    m.set_native_text_identity(SSA, "");
    write(&mut m, b"1.YES / 2.NO\0\0");
    let saved = m.grava_estado();
    m.set_native_text_identity(SSA, &"0".repeat(64));
    assert!(m.restaura_estado(&saved).is_err());
    m.set_native_text_identity(SSA, "");
    m.restaura_estado(&saved).unwrap();
    assert_eq!(draw(&mut m, u32::MAX), 0);
    assert_eq!(m.drawn_text().last().unwrap().3, "1.YES / 2.NO");
    let r = Leitor::abre(&saved).unwrap();
    let legacy = save_state::escreve(
        &r.nomes()
            .into_iter()
            .filter(|name| *name != "text.native")
            .map(|name| (name.to_owned(), r.secao(name).unwrap().to_vec()))
            .collect::<Vec<_>>(),
    );
    m.restaura_estado(&legacy).unwrap();
    assert_eq!(draw(&mut m, u32::MAX), 0);
    assert_eq!(m.drawn_text().last().unwrap().3, "1.YES / 2.NO");
    cleanup(m, root);
}
