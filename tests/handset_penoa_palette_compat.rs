use zeebx::{
    cpu::{BackendPadrao, CpuBackend, Reg},
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    storage::StoragePaths,
};
const MODULE: &str = "1d6e4265fb2f4093a986b2694812cbf2dd14b8ede7e53220d1e51dfee0572550";
const MIF: &str = "2f2bbec33c41fa5df1ad705f47795281eb5eed6f0c12c9f6046e2b1aeae386df";
const AT: u32 = 0x162f8;
const ORIGINAL: [u8; 8] = [1, 0x22, 3, 0x68, 0x20, 0x1c, 0x92, 2];
const FIXED: [u8; 8] = [1, 0x22, 3, 0x68, 0x20, 0x1c, 0x52, 1];
fn unstarted(context: [u8; 8]) -> Machine<BackendPadrao> {
    let mut data = vec![0; 0x7000];
    data[..8].copy_from_slice(&[0, 0, 0xa0, 0xe3, 0x1e, 0xff, 0x2f, 0xe1]);
    data[(AT - 0x10000) as usize..(AT - 0x10000 + 8) as usize].copy_from_slice(&context);
    data[(AT - 0x10000 + 8) as usize..(AT - 0x10000 + 10) as usize].copy_from_slice(&[0x70, 0x47]);
    let image = ModImage::parse(data).unwrap();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("local-tests/penoa-palette-fixture");
    let storage = StoragePaths::from_root(root.join("runtime"));
    Machine::new_with_storage(
        BackendPadrao::new().unwrap(),
        loader::load_with(&image, &[]).unwrap(),
        &root,
        &storage,
        None,
    )
}
fn machine() -> Machine<BackendPadrao> {
    let mut m = unstarted(ORIGINAL);
    assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
    m
}
fn count(m: &mut Machine<BackendPadrao>) -> u32 {
    assert_eq!(
        m.call_guest(AT + 7, [0, 0, 1, 0], 1000).unwrap(),
        Outcome::Returned { code: 0 }
    );
    m.cpu().read_reg(Reg::R2)
}
#[test]
fn exact_pair_reduces_guest_copy_to_initialized_palette_and_invalidates_jit() {
    let mut m = machine();
    assert_eq!(count(&mut m), 1024);
    assert!(m.set_handset_runtime_identity(MODULE, MIF));
    assert_eq!(count(&mut m), 32);
}
#[test]
fn only_the_guest_shift_changes_and_reapplication_is_idempotent() {
    let mut m = machine();
    let before = m.dump(AT - 8, 32).unwrap();
    assert!(m.set_handset_runtime_identity(&MODULE.to_uppercase(), &MIF.to_uppercase()));
    let mut expected = before;
    expected[8..16].copy_from_slice(&FIXED);
    assert_eq!(m.dump(AT - 8, 32).unwrap(), expected);
    assert!(m.set_handset_runtime_identity(MODULE, MIF));
    assert_eq!(m.dump(AT - 8, 32).unwrap(), expected);
}
#[test]
fn both_fingerprints_are_required() {
    for (module, mif) in [
        (MODULE, "unknown"),
        ("unknown", MIF),
        ("unknown", "unknown"),
    ] {
        let mut m = machine();
        assert!(!m.set_handset_runtime_identity(module, mif));
        assert_eq!(count(&mut m), 1024);
    }
}
#[test]
fn changed_template_or_live_context_is_rejected_atomically() {
    for position in 0..8 {
        let mut altered = ORIGINAL;
        altered[position] ^= 0x10;
        let mut m = unstarted(altered);
        assert!(!m.set_handset_runtime_identity(MODULE, MIF));
        assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
        assert_eq!(m.dump(AT, 8).unwrap(), altered);
        let mut m = machine();
        m.cpu_mut().write_mem(AT, &altered).unwrap();
        assert!(!m.set_handset_runtime_identity(MODULE, MIF));
        assert_eq!(m.dump(AT, 8).unwrap(), altered);
    }
}
#[test]
fn launcher_identity_applies_before_cpu_initialization() {
    let mut m = unstarted(ORIGINAL);
    assert!(m.set_handset_runtime_identity(MODULE, MIF));
    assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
    assert_eq!(count(&mut m), 32);
}
#[test]
fn restoring_original_saved_code_preserves_the_verified_correction() {
    let mut m = machine();
    assert!(m.set_handset_runtime_identity(MODULE, MIF));
    m.cpu_mut().write_mem(AT, &ORIGINAL).unwrap();
    let old = m.grava_estado();
    assert!(m.set_handset_runtime_identity(MODULE, MIF));
    m.restaura_estado(&old).unwrap();
    assert_eq!(count(&mut m), 32);
}
#[test]
fn changed_saved_context_is_rejected_before_any_live_mutation() {
    use zeebx::save_state::{self, Leitor};
    let mut m = machine();
    assert!(m.set_handset_runtime_identity(MODULE, MIF));
    let before = m.grava_estado();
    let reader = Leitor::abre(&before).unwrap();
    let base = m
        .module()
        .mem
        .regions()
        .iter()
        .find(|r| r.name == "module")
        .unwrap()
        .base;
    let sections = reader
        .nomes()
        .into_iter()
        .map(|name| {
            let mut data = reader.secao(name).unwrap().to_vec();
            if name == "mem.module" {
                data[(AT - base) as usize] ^= 0x10;
            }
            (name.to_string(), data)
        })
        .collect::<Vec<_>>();
    m.cpu_mut().write_reg(Reg::R4, 0xdecafbad);
    let live = m.grava_estado();
    assert!(m.restaura_estado(&save_state::escreve(&sections)).is_err());
    assert_eq!(m.grava_estado(), live);
    assert_eq!(count(&mut m), 32);
}
