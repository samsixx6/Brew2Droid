use zeebx::{
    cpu::{BackendPadrao, CpuBackend, Reg},
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    storage::StoragePaths,
};
const MODULE: &str = "605fbf1d43f9edb1a4c3e21c6336a5f145457d6d8285c817f8b67e0815b03a60";
const MIF: &str = "12ded40869036ceec929b4e0fa4086b8ce6517d99252f68cd9c5529fcdeea89f";
const FIRST: u32 = 0x4db80;
const SECOND: u32 = 0x4dba0;
const OLD: u32 = 0xe0020191;
const FIXED: u32 = 0xe0020791;
fn unstarted(second: u32) -> Machine<BackendPadrao> {
    let mut data = vec![0u8; 0x40000];
    for (at, word) in [
        (0, 0xe3a00000u32),
        (4, 0xe12fff1e),
        (FIRST - 0x10000, OLD),
        (FIRST + 4 - 0x10000, 0xe12fff1e),
        (SECOND - 0x10000, second),
        (SECOND + 4 - 0x10000, 0xe12fff1e),
    ] {
        data[at as usize..at as usize + 4].copy_from_slice(&word.to_le_bytes());
    }
    let image = ModImage::parse(data).unwrap();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("local-tests/pocketgod-font-fixture");
    let storage = StoragePaths::from_root(root.join("runtime"));
    Machine::new_with_storage(
        BackendPadrao::new().unwrap(),
        loader::load_with(&image, &[]).unwrap(),
        &root,
        &storage,
        None,
    )
}
fn machine(second: u32) -> Machine<BackendPadrao> {
    let mut m = unstarted(second);
    assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
    m
}
fn multiply(m: &mut Machine<BackendPadrao>, at: u32) -> u32 {
    m.cpu_mut().write_reg(Reg::R7, 5);
    assert_eq!(
        m.call_guest(at, [0, 19, 0, 0], 1000).unwrap(),
        Outcome::Returned { code: 0 }
    );
    m.cpu().read_reg(Reg::R2)
}
#[test]
fn exact_pair_uses_font_rows_and_invalidates_previously_executed_code() {
    let mut m = machine(OLD);
    assert_eq!(multiply(&mut m, FIRST), 361);
    assert!(m.set_handset_runtime_identity(MODULE, MIF));
    assert_eq!(multiply(&mut m, FIRST), 95);
    assert_eq!(multiply(&mut m, SECOND), 95);
}
#[test]
fn patch_preserves_every_other_byte_and_is_idempotent() {
    let mut m = machine(OLD);
    let before = m.dump(FIRST, 40).unwrap();
    assert!(m.set_handset_runtime_identity(&MODULE.to_uppercase(), &MIF.to_uppercase()));
    let mut expected = before;
    for at in [0, 32] {
        expected[at..at + 4].copy_from_slice(&FIXED.to_le_bytes());
    }
    assert_eq!(m.dump(FIRST, 40).unwrap(), expected);
    assert!(m.set_handset_runtime_identity(MODULE, MIF));
    assert_eq!(m.dump(FIRST, 40).unwrap(), expected);
}
#[test]
fn both_fingerprints_are_required() {
    for (module, mif) in [
        (MODULE, "unknown"),
        ("unknown", MIF),
        ("unknown", "unknown"),
    ] {
        let mut m = machine(OLD);
        let before = m.dump(FIRST, 40).unwrap();
        assert!(!m.set_handset_runtime_identity(module, mif));
        assert_eq!(m.dump(FIRST, 40).unwrap(), before);
    }
}
#[test]
fn changed_or_partly_patched_code_is_rejected_atomically() {
    for second in [0xe3a02000, FIXED] {
        let mut m = machine(second);
        let before = m.dump(FIRST, 40).unwrap();
        assert!(!m.set_handset_runtime_identity(MODULE, MIF));
        assert_eq!(m.dump(FIRST, 40).unwrap(), before);
    }
}
#[test]
fn launcher_identity_applies_before_the_cpu_is_initialized() {
    let mut m = unstarted(OLD);
    assert!(m.set_handset_runtime_identity(MODULE, MIF));
    assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
    assert_eq!(multiply(&mut m, FIRST), 95);
    assert_eq!(multiply(&mut m, SECOND), 95);
}
#[test]
fn restoring_a_pre_fix_state_keeps_the_verified_font_correction() {
    let mut m = machine(OLD);
    assert!(m.set_handset_runtime_identity(MODULE, MIF));
    for at in [FIRST, SECOND] {
        m.cpu_mut().write_u32(at, OLD).unwrap();
    }
    let old = m.grava_estado();
    assert!(m.set_handset_runtime_identity(MODULE, MIF));
    assert_eq!(multiply(&mut m, FIRST), 95);
    m.restaura_estado(&old).unwrap();
    assert_eq!(multiply(&mut m, FIRST), 95);
    assert_eq!(multiply(&mut m, SECOND), 95);
}
#[test]
fn unknown_saved_font_opcode_is_rejected_before_any_live_mutation() {
    use zeebx::save_state::{self, Leitor};
    let mut m = machine(OLD);
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
                let at = (SECOND - base) as usize;
                data[at..at + 4].copy_from_slice(&0xe3a02000u32.to_le_bytes());
            }
            (name.to_string(), data)
        })
        .collect::<Vec<_>>();
    m.cpu_mut().write_reg(Reg::R4, 0xdecafbad);
    let live = m.grava_estado();
    assert!(m.restaura_estado(&save_state::escreve(&sections)).is_err());
    assert_eq!(m.grava_estado(), live);
    assert_eq!(multiply(&mut m, FIRST), 95);
}
