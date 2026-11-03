use zeebx::{
    cpu::{BackendPadrao, CpuBackend, Reg},
    handset::types::DisplayGeometry,
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    storage::StoragePaths,
};
pub const ORIGINAL: [u8; 84] = [
    0x1d, 0x98, 0x00, 0x25, 0xc0, 0x07, 0xc0, 0x0f, 0x43, 0x01, 0xc0, 0x1a, 0x1f, 0x90, 0x18, 0x21,
    0x04, 0x91, 0x34, 0x21, 0x34, 0x20, 0x03, 0x90, 0x00, 0x22, 0x05, 0x92, 0x1f, 0x98, 0x1e, 0x9a,
    0x69, 0x43, 0x41, 0x18, 0x02, 0x92, 0x00, 0x22, 0x01, 0x91, 0x00, 0x97, 0x00, 0x23, 0x20, 0x1c,
    0x20, 0x99, 0xf2, 0xf7, 0xc0, 0xfd, 0x01, 0x35, 0x04, 0x2d, 0xe8, 0xdb, 0x1d, 0x98, 0x01, 0x30,
    0x1d, 0x90, 0x1d, 0x98, 0x18, 0x22, 0x42, 0x43, 0x1e, 0x92, 0x00, 0x23, 0xf0, 0x5e, 0x18, 0x30,
    0x82, 0x42, 0xd5, 0xdb,
];

const MODULE: &str = "ec94b2c155bdece2f8106e391061bce55273af0a1a4cdd70244718409b6a8541";
const MIF: &str = "da50aa48f15a60150b1b94c051d6e83d173e905bc8086e2c6ded6188f1a263e4";
const START: u32 = 0x2051e;
const COUNT: usize = 56;
fn unstarted(width: u16, context: [u8; 84]) -> Machine<BackendPadrao> {
    let mut data = vec![0; 0x11000];
    data[..8].copy_from_slice(&[0, 0, 0xa0, 0xe3, 0x1e, 0xff, 0x2f, 0xe1]);
    data[(START - 0x10000) as usize..(START - 0x10000 + 84) as usize].copy_from_slice(&context);
    // O tile loop original roda no ARM; o destino mínimo conta quantas cópias ele pede.
    // A prova dos pixels usa separadamente o pacote real, não este destino de teste.
    data[0x30d4..0x30dc].copy_from_slice(&[0x40, 0x46, 1, 0x30, 0x80, 0x46, 0x70, 0x47]);
    data[0x10680..0x10688].copy_from_slice(&[0xf0, 0xb5, 0xa4, 0xb0, 0, 0x20, 0x1d, 0x90]);
    let delta = (START as i32) - (0x20688 + 4);
    let first = 0xf000 | ((delta >> 12) & 0x7ff) as u16;
    let second = 0xf800 | ((delta >> 1) & 0x7ff) as u16;
    data[0x10688..0x1068a].copy_from_slice(&first.to_le_bytes());
    data[0x1068a..0x1068c].copy_from_slice(&second.to_le_bytes());
    let exit = 0xe000u16 | (((0x206a0i32 - (0x20572 + 4)) >> 1) & 0x7ff) as u16;
    data[0x10572..0x10574].copy_from_slice(&exit.to_le_bytes());
    data[0x106a0..0x106a4].copy_from_slice(&[0x24, 0xb0, 0xf0, 0xbd]);
    let image = ModImage::parse(data).unwrap();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("local-tests/startup-repairs-wave19-20261031/graphics/fixture");
    let storage = StoragePaths::from_root(root.join("runtime"));
    Machine::new_with_storage_geometry(
        BackendPadrao::new().unwrap(),
        loader::load_with(&image, &[]).unwrap(),
        &root,
        &storage,
        None,
        DisplayGeometry {
            width,
            height: 320,
            depth: 16,
        },
    )
    .unwrap()
}
fn machine(width: u16) -> Machine<BackendPadrao> {
    let mut m = unstarted(width, ORIGINAL);
    assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
    m
}
fn count(m: &mut Machine<BackendPadrao>) -> u32 {
    m.cpu_mut().write_reg(Reg::R6, loader::HEAP_BASE);
    m.cpu_mut().write_reg(Reg::R8, 0);
    m.cpu_mut().write_mem(loader::HEAP_BASE, &[0; 4]).unwrap();
    assert_eq!(
        m.call_guest(0x20681, [0, 0, 0, 0], 10000).unwrap(),
        Outcome::Returned { code: 24 }
    );
    m.cpu().read_reg(Reg::R8)
}
fn rewrite_saved(m: &mut Machine<BackendPadrao>, change: impl FnOnce(&mut [u8])) -> Vec<u8> {
    use zeebx::save_state::{self, Leitor};
    let state = m.grava_estado();
    let reader = Leitor::abre(&state).unwrap();
    let base = m
        .module()
        .mem
        .regions()
        .iter()
        .find(|r| r.name == "module")
        .unwrap()
        .base;
    let mut change = Some(change);
    let sections = reader
        .nomes()
        .into_iter()
        .map(|name| {
            let mut data = reader.secao(name).unwrap().to_vec();
            if name == "mem.module" {
                change.take().unwrap()(
                    &mut data[(START - base) as usize..(START - base + 84) as usize],
                );
            }
            (name.to_string(), data)
        })
        .collect::<Vec<_>>();
    save_state::escreve(&sections)
}
#[test]
fn unpatched_guest_loop_requests_four_tiles() {
    let mut m = machine(240);
    assert_eq!(count(&mut m), 4);
}
#[test]
fn exact_pair_uses_current_display_width_and_invalidates_existing_jit() {
    for (width, expected) in [(176, 4), (240, 6), (480, 10), (1024, 21)] {
        let mut m = machine(width);
        assert_eq!(count(&mut m), 4);
        assert!(m.set_handset_runtime_identity(MODULE, MIF));
        assert_eq!(count(&mut m), expected, "width {width}");
    }
}
#[test]
fn both_fingerprints_are_required() {
    for (module, mif) in [
        (MODULE, "unknown"),
        ("unknown", MIF),
        ("unknown", "unknown"),
    ] {
        let mut m = machine(240);
        assert!(!m.set_handset_runtime_identity(module, mif));
        assert_eq!(count(&mut m), 4);
    }
}
#[test]
fn initialization_before_cpu_reset_and_reapplication_are_idempotent() {
    let mut m = unstarted(240, ORIGINAL);
    assert!(m.set_handset_runtime_identity(&MODULE.to_uppercase(), &MIF.to_uppercase()));
    assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
    let mut expected = ORIGINAL;
    expected[COUNT] = 6;
    assert_eq!(m.dump(START, 84).unwrap(), expected);
    assert!(m.set_handset_runtime_identity(MODULE, MIF));
    assert_eq!(m.dump(START, 84).unwrap(), expected);
    assert_eq!(count(&mut m), 6);
}
#[test]
fn every_changed_template_context_byte_is_rejected_before_cpu_reset() {
    for position in 0..84 {
        let mut altered = ORIGINAL;
        altered[position] ^= 0x10;
        let mut m = unstarted(240, altered);
        assert!(
            !m.set_handset_runtime_identity(MODULE, MIF),
            "byte {position}"
        );
        assert_eq!(m.module().mem.read(START, 84).unwrap(), altered);
    }
}
#[test]
fn every_changed_live_context_byte_rejects_without_changing_template_or_cpu() {
    for position in 0..84 {
        let mut m = machine(240);
        let mut altered = ORIGINAL;
        altered[position] ^= 0x10;
        m.cpu_mut().write_mem(START, &altered).unwrap();
        assert!(
            !m.set_handset_runtime_identity(MODULE, MIF),
            "byte {position}"
        );
        assert_eq!(m.dump(START, 84).unwrap(), altered);
        assert_eq!(m.module().mem.read(START, 84).unwrap(), ORIGINAL);
    }
}
#[test]
fn old_original_saved_module_is_upgraded_to_current_geometry() {
    let mut m = machine(240);
    assert!(m.set_handset_runtime_identity(MODULE, MIF));
    let original = rewrite_saved(&mut m, |bytes| bytes.copy_from_slice(&ORIGINAL));
    m.restaura_estado(&original).unwrap();
    assert_eq!(count(&mut m), 6);
    assert_eq!(m.screen().width(), 240);
}
#[test]
fn current_geometry_saved_patch_round_trips() {
    let mut m = machine(480);
    assert!(m.set_handset_runtime_identity(MODULE, MIF));
    let state = m.grava_estado();
    m.restaura_estado(&state).unwrap();
    assert_eq!(count(&mut m), 10);
}
#[test]
fn saved_patch_from_another_geometry_or_arbitrary_count_is_rejected_atomically() {
    for unexpected in [0, 3, 5, 7, 10, 21, 255] {
        let mut m = machine(240);
        assert!(m.set_handset_runtime_identity(MODULE, MIF));
        let corrupt = rewrite_saved(&mut m, |bytes| bytes[COUNT] = unexpected);
        m.cpu_mut().write_reg(Reg::R4, 0xdecafbad);
        let before = m.grava_estado();
        assert!(m.restaura_estado(&corrupt).is_err(), "count {unexpected}");
        assert_eq!(m.grava_estado(), before);
        assert_eq!(count(&mut m), 6);
    }
}
#[test]
fn changed_saved_loop_context_is_rejected_atomically() {
    for position in [0, 14, 18, 50, 57, 58, 66, 82] {
        let mut m = machine(240);
        assert!(m.set_handset_runtime_identity(MODULE, MIF));
        let corrupt = rewrite_saved(&mut m, |bytes| bytes[position] ^= 0x10);
        m.cpu_mut().write_reg(Reg::R4, 0xdecafbad);
        let before = m.grava_estado();
        assert!(m.restaura_estado(&corrupt).is_err(), "byte {position}");
        assert_eq!(m.grava_estado(), before);
        assert_eq!(count(&mut m), 6);
    }
}
