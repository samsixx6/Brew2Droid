use zeebx::{
    cpu::{BackendPadrao, CpuBackend, Reg},
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    storage::StoragePaths,
};
const MODULE: &str = "5fbb0a3d36da30c592fc01cbe4e4cd5ece4cf3e41421d965cd5a67cd70badd7a";
const MIF: &str = "872bac57d05185a938e37809254ff3cf9528f85bf367c482f9044ece2bc5a118";
const AT: u32 = 0x1e984;
const ORIGINAL: [u8; 12] = [
    0x68, 0x68, 0, 0x28, 0x2c, 0xd0, 0xe9, 0x68, 0x89, 7, 0x23, 0xd5,
];
const FIXED: [u8; 12] = [
    0x68, 0x68, 0, 0x28, 0x2c, 0xd0, 0xe9, 0x68, 0x89, 7, 0xc0, 0x46,
];
fn unstarted(context: [u8; 12]) -> Machine<BackendPadrao> {
    let mut data = vec![0; 0xf000];
    data[..8].copy_from_slice(&[0, 0, 0xa0, 0xe3, 0x1e, 0xff, 0x2f, 0xe1]);
    data[(AT - 0x10000) as usize..(AT - 0x10000 + 12) as usize].copy_from_slice(&context);
    data[0xe990..0xe994].copy_from_slice(&[1, 0x20, 0x70, 0x47]);
    data[0xe9d8..0xe9dc].copy_from_slice(&[0, 0x20, 0x70, 0x47]);
    let image = ModImage::parse(data).unwrap();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("local-tests/jumong-public_path-fixture");
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
    let flags = m.cpu().cpsr() & !0x80000000;
    m.cpu_mut().set_cpsr(flags);
    match m.call_guest(AT + 11, [0; 4], 1000).unwrap() {
        Outcome::Returned { code } => code,
        other => panic!("{other:?}"),
    }
}
#[test]
fn exact_pair_selects_existing_public_framebuffer_and_invalidates_jit() {
    let mut m = machine();
    assert_eq!(count(&mut m), 0);
    assert!(m.set_handset_runtime_identity(MODULE, MIF));
    assert_eq!(count(&mut m), 1);
}
#[test]
fn only_the_guest_branch_changes_and_reapplication_is_idempotent() {
    let mut m = machine();
    let before = m.dump(AT - 8, 32).unwrap();
    assert!(m.set_handset_runtime_identity(&MODULE.to_uppercase(), &MIF.to_uppercase()));
    let mut expected = before;
    expected[8..20].copy_from_slice(&FIXED);
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
        assert_eq!(count(&mut m), 0);
    }
}
#[test]
fn changed_template_or_live_context_is_rejected_atomically() {
    for position in 0..12 {
        let mut altered = ORIGINAL;
        altered[position] ^= 0x10;
        let mut m = unstarted(altered);
        assert!(!m.set_handset_runtime_identity(MODULE, MIF));
        assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
        assert_eq!(m.dump(AT, 12).unwrap(), altered);
        let mut m = machine();
        m.cpu_mut().write_mem(AT, &altered).unwrap();
        assert!(!m.set_handset_runtime_identity(MODULE, MIF));
        assert_eq!(m.dump(AT, 12).unwrap(), altered);
    }
}
#[test]
fn launcher_identity_applies_before_cpu_initialization() {
    let mut m = unstarted(ORIGINAL);
    assert!(m.set_handset_runtime_identity(MODULE, MIF));
    assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
    assert_eq!(count(&mut m), 1);
}
#[test]
fn restoring_original_saved_code_preserves_the_verified_correction() {
    let mut m = machine();
    assert!(m.set_handset_runtime_identity(MODULE, MIF));
    m.cpu_mut().write_mem(AT, &ORIGINAL).unwrap();
    let old = m.grava_estado();
    assert!(m.set_handset_runtime_identity(MODULE, MIF));
    m.restaura_estado(&old).unwrap();
    assert_eq!(count(&mut m), 1);
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
    assert_eq!(count(&mut m), 1);
}
