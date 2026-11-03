use zeebx::loader::resfile::ResFile;

fn resource_string(raw: &[u8]) -> Option<Vec<u16>> {
    let mut bar = vec![0; 48];
    bar[..2].copy_from_slice(&0x11u16.to_le_bytes());
    bar[6..8].copy_from_slice(&1u16.to_le_bytes());
    for (offset, value) in [
        (8, 32u32),
        (12, 8),
        (16, 40),
        (20, 1),
        (32, 0x0007_0001),
        (36, 0),
        (40, 48),
        (44, 48 + raw.len() as u32),
    ] {
        bar[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    bar.extend_from_slice(raw);
    ResFile::parse(bar).unwrap().string(7)
}

#[test]
fn korean_resource_selector_decodes_characters_without_leaking_the_encoding_header() {
    // AEE_ENC_KSC5601/EUC_KR = FE; o recurso codificado tem a marca FE FE.
    // KS X 1001: C7 D1 = U+D55C, B1 DB = U+AE00. Sem dependência do codec no esperado.
    assert_eq!(
        resource_string(&[0xfe, 0xfe, b'A', 0xc7, 0xd1, 0xb1, 0xdb, b'Z']),
        Some(vec![0x41, 0xd55c, 0xae00, 0x5a])
    );
    assert_eq!(
        resource_string(b"\xfe\xfePress any key\0ignored"),
        Some("Press any key".encode_utf16().collect())
    );
}

#[test]
fn japanese_resource_selector_decodes_shift_jis_instead_of_latin1() {
    // AEE_ENC_S_JIS = FD; FD FE é a marca observada nos recursos KDDI.
    assert_eq!(
        resource_string(&[0xfd, 0xfe, b'A', 0x82, 0xa0, 0x82, 0xa2, 0x82, 0xa4, b'Z']),
        Some(vec![0x41, 0x3042, 0x3044, 0x3046, 0x5a])
    );
}

#[test]
fn explicit_utf8_resource_selector_uses_unicode_not_individual_bytes() {
    assert_eq!(
        resource_string(b"\x02caf\xc3\xa9"),
        Some(vec![0x63, 0x61, 0x66, 0xe9])
    );
}

#[test]
fn unsupported_or_truncated_encoded_strings_do_not_invent_replacement_characters() {
    for raw in [
        &[0xfe, 0xfe, 0xc7][..],
        &[0xfd, 0xfe, 0x82][..],
        &[2, 0xc3][..],
    ] {
        assert_eq!(
            resource_string(raw),
            None,
            "sequência incompleta {raw:02x?}"
        );
    }
}

#[test]
fn legacy_unmarked_latin1_and_both_utf16_byte_orders_keep_their_existing_units() {
    assert_eq!(
        resource_string(b"Ic\xf4ne"),
        Some(vec![0x49, 0x63, 0xf4, 0x6e, 0x65])
    );
    assert_eq!(
        resource_string(b"\x03Ic\xf4ne"),
        Some(vec![0x49, 0x63, 0xf4, 0x6e, 0x65])
    );
    assert_eq!(resource_string(&[0xfe, b'X']), Some(vec![0xfe, 0x58]));
    assert_eq!(resource_string(&[0xfd, b'Y']), Some(vec![0xfd, 0x59]));
    assert_eq!(
        resource_string(&[0xff, 0xfe, 0x5c, 0xd5, 0, 0xae]),
        Some(vec![0xd55c, 0xae00])
    );
    assert_eq!(
        resource_string(&[0xfe, 0xff, 0xd5, 0x5c, 0xae, 0]),
        Some(vec![0xd55c, 0xae00])
    );
}
