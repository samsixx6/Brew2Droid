//! Vetores primários e limites de segurança do STREXPAND nativo.
//! Os casos de truncamento/overlap documentam política conservadora da implementação;
//! o SDK não especifica esses detalhes. Nenhum helper muda de contrato globalmente.
use std::path::PathBuf;
use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend, Reg};
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, NativeTextEncoding, Outcome};
use zeebx::storage::StoragePaths;

const EXISTING_MOD: &str = "8622d31bcb9b5649a14b7dd5894e6e1902c1a9f2f3281984a8dc138c9f814b9e";
const EXISTING_MIF: &str = "afce4811ad7c04b7487bc9657b81d8f2c81285114f6413cbbc1b21d2cf995cde";
const RAINBOW_MOD: &str = "e0192d2358038eb19b61d6d9409341f66aa03f8a65692482578584e259b81c44";
const RAINBOW_MIF: &str = "988ff5106202457cbe605115a3a2684872a7f022975496d2b86718a9123a17dc";
const SOURCE: u32 = loader::STACK_BASE + 0x1800;
const DEST: u32 = loader::STACK_BASE + 0x1a00;
const INFO: u32 = loader::STACK_BASE + 0x1c00;
const RAW: &[u8] = b"\xbc\xad\xb1\xe2 2005\xb3\xe2.";
const NATIVE: &[u8] = b"\xbc\xad\xb1\xe2 \0\x32\0\x30\0\x30\0\x35\0\xb3\xe2.\0\0\0";

fn fixture(native: bool) -> (Machine<BackendPadrao>, PathBuf) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("local-tests/startup-repairs-wave16-20261030")
        .join(format!(
            "fixture-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
    std::fs::create_dir_all(root.parent().unwrap()).unwrap();
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
    let mut m = Machine::new_with_storage(
        BackendPadrao::new().unwrap(),
        loader::load_with(&image, &[]).unwrap(),
        &root,
        &storage,
        None,
    );
    m.enable_handset_clock();
    assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
    if native {
        assert!(m.set_native_text_identity(EXISTING_MOD, EXISTING_MIF));
    }
    (m, root)
}

fn cleanup(m: Machine<BackendPadrao>, root: PathBuf) {
    drop(m);
    let allowed = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("local-tests/startup-repairs-wave16-20261030")
        .canonicalize()
        .unwrap();
    assert!(root.canonicalize().unwrap().starts_with(allowed));
    std::fs::remove_dir_all(root).unwrap();
}

fn call(
    m: &mut Machine<BackendPadrao>,
    interface: Interface,
    name: &str,
    args: [u32; 4],
    stack: &[u32],
) -> u32 {
    let slot = (0..256)
        .find(|s| interface.method(*s) == Some(name))
        .unwrap();
    let old = m.cpu().read_reg(Reg::Sp);
    let scratch = loader::STACK_BASE + 0x400;
    for (i, v) in stack.iter().enumerate() {
        m.cpu_mut().write_u32(scratch + i as u32 * 4, *v).unwrap();
    }
    m.cpu_mut().write_reg(Reg::Sp, scratch);
    let outcome = m
        .call_guest(aee::encode(interface, slot), args, 10_000)
        .unwrap();
    m.cpu_mut().write_reg(Reg::Sp, old);
    match outcome {
        Outcome::Returned { code } => code,
        other => panic!("{name}: {other:?}"),
    }
}

fn read(m: &Machine<BackendPadrao>, at: u32, length: usize) -> Vec<u8> {
    let mut bytes = vec![0; length];
    m.cpu().read_mem(at, &mut bytes).unwrap();
    bytes
}

fn source(m: &mut Machine<BackendPadrao>, raw: &[u8]) {
    m.cpu_mut()
        .write_mem(SOURCE, &raw.iter().copied().chain([0]).collect::<Vec<_>>())
        .unwrap();
}

fn guard(m: &mut Machine<BackendPadrao>, capacity: usize) -> Vec<u8> {
    let before = vec![0xa5; 16 + capacity + 16];
    m.cpu_mut().write_mem(DEST - 16, &before).unwrap();
    before
}

fn assert_exact_guard(m: &Machine<BackendPadrao>, before: &[u8], output: &[u8]) {
    let after = read(m, DEST - 16, before.len());
    assert_eq!(&after[..16], &before[..16]);
    assert_eq!(&after[16..16 + output.len()], output);
    assert_eq!(
        &after[16 + output.len()..],
        &before[16 + output.len()..],
        "somente saída exata, não a capacidade toda"
    );
}

fn expand(m: &mut Machine<BackendPadrao>, count: u32, capacity: u32) {
    // A assinatura SDK é void: nenhum código de retorno é afirmado aqui.
    call(
        m,
        Interface::Helpers,
        "strexpand",
        [SOURCE, count, DEST, capacity],
        &[],
    );
}

#[test]
fn rainbow_profile_requires_the_exact_observed_pair_and_reports_native_encoding() {
    let (mut m, root) = fixture(false);
    assert!(m.set_native_text_identity(RAINBOW_MOD, RAINBOW_MIF));
    assert_eq!(m.native_text_encoding(), NativeTextEncoding::EucKr);
    m.cpu_mut().write_mem(INFO, &[0; 64]).unwrap();
    call(
        &mut m,
        Interface::Shell,
        "GetDeviceInfo",
        [loader::OBJECT_BASE, INFO, 0, 0],
        &[],
    );
    assert_eq!(read(&m, INFO + 10, 2), [0xfe, 0]);
    cleanup(m, root);
}

#[test]
fn mismatched_rainbow_hashes_keep_unicode() {
    let (mut m, root) = fixture(false);
    for (module, mif) in [(RAINBOW_MOD, EXISTING_MIF), (EXISTING_MOD, RAINBOW_MIF)] {
        assert!(!m.set_native_text_identity(module, mif));
        assert_eq!(m.native_text_encoding(), NativeTextEncoding::Unicode);
    }
    cleanup(m, root);
}

#[test]
fn native_expansion_keeps_primary_ksc_words_and_draws_without_changing_them() {
    let (mut m, root) = fixture(true);
    source(&mut m, RAW);
    let before = guard(&mut m, 100);
    assert_eq!(RAW.len(), 12);
    assert_eq!(NATIVE.len(), 20);
    expand(&mut m, 12, 100);
    assert_exact_guard(&m, &before, NATIVE);
    let words = read(&m, DEST, 100);
    assert_eq!(
        call(
            &mut m,
            Interface::Display,
            "DrawText",
            [loader::OBJECT_BASE + 0x10, 0x8001, DEST, u32::MAX],
            &[0, 0, 0, 0]
        ),
        0
    );
    assert_eq!(m.drawn_text().last().unwrap().3, "서기 2005년.");
    assert_eq!(read(&m, DEST, 100), words);
    cleanup(m, root);
}

#[test]
fn ascii_expansion_in_native_context_remains_wide_ascii() {
    let (mut m, root) = fixture(true);
    source(&mut m, b"Ver 1.0.4");
    let before = guard(&mut m, 100);
    expand(&mut m, 9, 100);
    let expected: Vec<u8> = "Ver 1.0.4"
        .encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect();
    assert_exact_guard(&m, &before, &expected);
    cleanup(m, root);
}

#[test]
fn observed_ncount_limits_source_bytes_not_decoded_character_count() {
    let (mut m, root) = fixture(true);
    source(&mut m, b"A\xc7\xd1Z");
    let before = guard(&mut m, 20);
    // O chamador real passa strlen=12 para nove caracteres. Aqui3bytes são A+한.
    expand(&mut m, 3, 20);
    assert_exact_guard(&m, &before, b"A\0\xc7\xd1\0\0");
    cleanup(m, root);
}

#[test]
fn ncount_does_not_consume_following_ascii_bytes() {
    let (mut m, root) = fixture(true);
    source(&mut m, b"ABC");
    let before = guard(&mut m, 12);
    expand(&mut m, 1, 12);
    assert_exact_guard(&m, &before, b"A\0\0\0");
    cleanup(m, root);
}

#[test]
fn safety_count_bound_does_not_read_a_missing_trailing_byte() {
    let (mut m, root) = fixture(true);
    let edge = loader::STACK_BASE + loader::STACK_SIZE as u32 - 1;
    m.cpu_mut().write_mem(edge, &[0xc7]).unwrap();
    let before = guard(&mut m, 8);
    // SDK não detalha entrada truncada: a política segura para este caso descarta
    // o par incompleto e nunca lê fora de nCount ou grava metade de AECHAR.
    call(
        &mut m,
        Interface::Helpers,
        "strexpand",
        [edge, 1, DEST, 8],
        &[],
    );
    assert_exact_guard(&m, &before, &[0, 0]);
    cleanup(m, root);
}

#[test]
fn complete_native_pair_can_end_exactly_at_the_count_and_mapping_boundaries() {
    let (mut m, root) = fixture(true);
    let edge = loader::STACK_BASE + loader::STACK_SIZE as u32 - 2;
    m.cpu_mut().write_mem(edge, &[0xc7, 0xd1]).unwrap();
    let before = guard(&mut m, 8);
    call(
        &mut m,
        Interface::Helpers,
        "strexpand",
        [edge, 2, DEST, 8],
        &[],
    );
    assert_exact_guard(&m, &before, &[0xc7, 0xd1, 0, 0]);
    cleanup(m, root);
}

#[test]
fn zero_source_count_writes_only_a_terminator_in_a_nonempty_destination() {
    let (mut m, root) = fixture(true);
    source(&mut m, b"ABC");
    let before = guard(&mut m, 10);
    expand(&mut m, 0, 10);
    assert_exact_guard(&m, &before, &[0, 0]);
    cleanup(m, root);
}

#[test]
fn zero_or_single_byte_capacity_does_not_write_partial_aechar() {
    let (mut m, root) = fixture(true);
    source(&mut m, RAW);
    for capacity in [0, 1] {
        let before = guard(&mut m, 100);
        expand(&mut m, 12, capacity);
        assert_eq!(read(&m, DEST - 16, before.len()), before);
    }
    cleanup(m, root);
}

#[test]
fn odd_capacity_keeps_guards_and_only_complete_native_prefixes() {
    let (mut m, root) = fixture(true);
    source(&mut m, b"A\xc7\xd1B");
    let valid = [0x0041u16, 0xd1c7, 0x0042];
    for capacity in [3usize, 5, 7, 9] {
        let before = guard(&mut m, capacity);
        expand(&mut m, 4, capacity as u32);
        let after = read(&m, DEST - 16, before.len());
        assert_eq!(&after[..16], &before[..16]);
        assert_eq!(&after[16 + capacity..], &before[16 + capacity..]);
        assert_eq!(
            after[16 + capacity - 1],
            0xa5,
            "último byte ímpar não é um AECHAR completo"
        );
        for (index, word) in after[16..16 + capacity - 1].chunks_exact(2).enumerate() {
            let word = u16::from_le_bytes([word[0], word[1]]);
            if word == 0 || word == 0xa5a5 {
                break;
            }
            assert_eq!(
                Some(&word),
                valid.get(index),
                "saída truncada deve conservar prefixos de unidades nativas completas"
            );
        }
    }
    cleanup(m, root);
}

#[test]
fn safely_snapshotted_overlapping_buffers_preserve_source_units() {
    let (mut m, root) = fixture(true);
    // O SDK não promete overlap. A implementação candidata o suporta lendo a origem
    // inteira primeiro; este teste verifica essa capacidade explícita, não um contrato OEM.
    for delta in [-2i32, 0, 2] {
        let destination = (SOURCE as i64 + delta as i64) as u32;
        m.cpu_mut().write_mem(SOURCE - 32, &[0xa5; 160]).unwrap();
        source(&mut m, RAW);
        let before = read(&m, destination - 16, 132);
        call(
            &mut m,
            Interface::Helpers,
            "strexpand",
            [SOURCE, 12, destination, 100],
            &[],
        );
        let after = read(&m, destination - 16, 132);
        assert_eq!(&after[..16], &before[..16]);
        assert_eq!(&after[16..36], NATIVE);
        assert_eq!(&after[36..], &before[36..]);
    }
    cleanup(m, root);
}

#[test]
fn unprofiled_latin1_expansion_retains_existing_behavior() {
    let (mut m, root) = fixture(false);
    source(&mut m, b"Caf\xe9");
    let before = guard(&mut m, 20);
    expand(&mut m, 4, 20);
    assert_exact_guard(&m, &before, b"C\0a\0f\0\xe9\0\0\0");
    cleanup(m, root);
}

#[test]
fn unicode_draw_and_utf8_conversion_remain_separate_from_native_expansion() {
    let (mut m, root) = fixture(false);
    let words: Vec<u8> = "서기"
        .encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect();
    m.cpu_mut().write_mem(DEST, &words).unwrap();
    assert_eq!(
        call(
            &mut m,
            Interface::Display,
            "DrawText",
            [loader::OBJECT_BASE + 0x10, 0x8000, DEST, u32::MAX],
            &[0, 0, 0, 0]
        ),
        0
    );
    assert_eq!(m.drawn_text().last().unwrap().3, "서기");
    source(&mut m, "서기".as_bytes());
    for native in [false, true] {
        if native {
            m.set_native_text_identity(EXISTING_MOD, EXISTING_MIF);
        }
        let before = guard(&mut m, 20);
        call(
            &mut m,
            Interface::Helpers,
            "utf8towstr",
            [SOURCE, 6, DEST, 20],
            &[],
        );
        assert_exact_guard(&m, &before, &words);
    }
    cleanup(m, root);
}
