use zeebx::{handset::package::parse_module, loader::{self, modfile::Variant},
    cpu::{BackendPadrao, CpuBackend}, machine::{Machine, Outcome}};

fn header(version: u32, offset: u32) -> Vec<u8> {
    let mut data = vec![0u8; 0x60];
    for (at, word) in [(0, 0xea000010u32), (4, 0xea000011), (12, version),
        (16, offset), (20, 0x101), (24, 0x200), (28, 0x60), (52, 0x60),
        (0x48, 0xe3a0002a), (0x4c, 0xe12fff1e)] {
        data[at..at+4].copy_from_slice(&word.to_le_bytes());
    }
    data[8..12].copy_from_slice(b"BREW");data
}

#[test]
fn handset_accepts_observed_v2_header_and_executes_original_branch_target() {
    let bytes = header(2, 0x48);
    let image = parse_module(bytes.clone()).expect("cabeçalho BREW v2 observado não deve ser recusado como v1 inválido");
    assert_eq!(image.variant(), Variant::BrewHeader);
    assert_eq!(image.entry(), 0x48);
    assert_eq!(image.image(), bytes);
    let mut m = Machine::new(BackendPadrao::new().unwrap(), loader::load_with(&image, &[]).unwrap(), ".");
    assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 42 });
}

#[test]
fn handset_rejects_unknown_header_versions_mismatches_and_truncation() {
    for bytes in [header(2, 0x40), header(1, 0x48), header(3, 0x48), header(2, 0x50), header(2, 0x48)[..0x48].to_vec()] {
        assert!(parse_module(bytes).is_err());
    }
    let mut inside = header(2, 0x48);inside[..4].copy_from_slice(&0xea00000eu32.to_le_bytes());
    assert!(parse_module(inside).is_err(), "entrada dentro do cabeçalho deve continuar inválida");
}
