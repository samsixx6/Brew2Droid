use zeebx::handset::package::{ParseError, inspect_package, parse_mif, parse_module};
use zeebx::handset::profile::{resolve_profile, validate_mif_identity, validate_release_profile};
use zeebx::handset::types::GameId;

fn raw_arm() -> Vec<u8> {
    [0xe3a0_0000u32.to_le_bytes(), 0xe12f_ff1eu32.to_le_bytes()].concat()
}

fn empty_mif() -> Vec<u8> {
    let mut bytes = vec![0u8; 36];
    bytes[..4].copy_from_slice(&[0x11, 0, 1, 0]);
    bytes[16..20].copy_from_slice(&32u32.to_le_bytes());
    bytes[32..36].copy_from_slice(&36u32.to_le_bytes());
    bytes
}

#[test]
fn raw_arm_entry_is_preserved_without_mapping_guest_memory() {
    let module = parse_module(raw_arm()).unwrap();
    assert_eq!(module.entry(), 0);
    assert_eq!(module.image().len(), 8);
}

#[test]
fn truncated_and_unverified_entry_formats_are_rejected() {
    assert!(matches!(
        parse_module(vec![0; 7]),
        Err(ParseError::Module(_))
    ));
    assert!(matches!(
        parse_module(vec![0; 8]),
        Err(ParseError::UnsupportedEntry)
    ));
    // Thumb direto ainda não tem contrato de entrada confirmado para esses pacotes.
    assert!(matches!(
        parse_module(
            [
                0x2000u16.to_le_bytes(),
                0x4770u16.to_le_bytes(),
                [0; 2],
                [0; 2]
            ]
            .concat()
        ),
        Err(ParseError::UnsupportedEntry)
    ));
}

#[test]
fn brew_header_cannot_redirect_execution_outside_the_image() {
    let mut bytes = vec![0u8; 64];
    bytes[..4].copy_from_slice(&0xea00_000eu32.to_le_bytes());
    bytes[8..12].copy_from_slice(b"BREW");
    bytes[12..16].copy_from_slice(&1u32.to_le_bytes());
    bytes[16..20].copy_from_slice(&64u32.to_le_bytes());
    assert!(matches!(
        parse_module(bytes.clone()),
        Err(ParseError::InvalidEntry)
    ));
    bytes[..4].copy_from_slice(&0xeaff_fffcu32.to_le_bytes());
    assert!(matches!(parse_module(bytes), Err(ParseError::InvalidEntry)));
}

#[test]
fn mif_counts_and_bounds_are_checked_before_parser_allocation() {
    let valid = empty_mif();
    assert!(parse_mif(&valid).is_ok());
    for truncated in 0..valid.len() {
        assert!(parse_mif(&valid[..truncated]).is_err());
    }
    let mut excessive = valid.clone();
    excessive[20..24].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        parse_mif(&excessive),
        Err(ParseError::InvalidMif(_))
    ));
    let mut overlap = valid;
    overlap[32..36].copy_from_slice(&32u32.to_le_bytes());
    assert!(matches!(
        parse_mif(&overlap),
        Err(ParseError::InvalidMif(_))
    ));
}

#[test]
fn exact_mobile_game_identity_never_implies_a_verified_release_profile() {
    let hashes = [
        (
            GameId::Bowling3d,
            "1f5174a83a784d7ce67145b5d182d09ae734f78a434f2eac4bc80416503d8f60",
        ),
        (
            GameId::Asphalt2,
            "c4a114ae6391fec4b04080e2971465197b1867751b0436cb7e8891eabb1b1819",
        ),
        (
            GameId::Bia240,
            "e2cc4d4c05bb927569a26cdc87bf7f66978ddd7dce04583fe8f169089b979d6d",
        ),
    ];
    for (id, hash) in hashes {
        let profile = resolve_profile(id, hash).unwrap();
        assert_eq!(profile.game_id, id);
        assert!(profile.geometry.is_none());
        assert!(profile.minimum_isa.is_none());
        assert!(profile.bss_bytes.is_none());
        let error = validate_release_profile(&profile).unwrap_err().to_string();
        assert!(error.contains("geometry") && error.contains("ISA") && error.contains("BSS"));
        assert!(resolve_profile(id, &"0".repeat(64)).is_err());
        assert!(validate_mif_identity(&profile, &profile.mif_sha256).is_ok());
        assert!(validate_mif_identity(&profile, &"0".repeat(64)).is_err());
    }
}

#[test]
fn package_inspection_hashes_resources_without_changing_the_package() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    let work = workspace.join("local-tests").join(format!(
        "inspect-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let content = work.join("source");
    std::fs::create_dir_all(&content).unwrap();
    assert!(work.canonicalize().unwrap().starts_with(workspace));
    let module = content.join("game.mod");
    let resource = content.join("main.bar");
    let mif = work.join("game.mif");
    std::fs::write(&module, raw_arm()).unwrap();
    std::fs::write(&resource, b"abc").unwrap();
    std::fs::write(&mif, empty_mif()).unwrap();
    let report = inspect_package(&module, &mif).unwrap();
    assert_eq!(report.schema_version, 1);
    assert!(report.game_id.is_none());
    assert!(report.profile.is_none());
    assert_eq!(report.resources.len(), 1);
    assert_eq!(report.resources[0].path, "main.bar");
    assert_eq!(report.resources[0].size_bytes, 3);
    assert_eq!(
        report.resources[0].sha256,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(std::fs::read(&module).unwrap(), raw_arm());
    assert_eq!(std::fs::read(&resource).unwrap(), b"abc");
    assert_eq!(std::fs::read(&mif).unwrap(), empty_mif());
    assert_eq!(std::fs::read_dir(&content).unwrap().count(), 2);
    std::fs::remove_dir_all(work).unwrap();
}
