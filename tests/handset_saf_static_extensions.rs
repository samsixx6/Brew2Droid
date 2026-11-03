fn saf_rgb332() -> Vec<u8> {
    // Fixture própria: tabela DEFLATE canônica externa, quatro literais e fim.
    // Os literais são vermelho, verde, azul e magenta; não há arte de um jogo.
    vec![
        0x53,0x41,0x46,0x00,0x02,0x00,0x6f,0x07,0x00,0x02,0x00,0x15,
        0x01,0x06,0x02,0x02,0x08,0x04,0x00,0x00,0x02,0x02,0x01,0x00,
        0x03,0x03,0x01,0x00,0x07,0x0a,0x02,0x01,0x90,0x0b,0x00,0x2a,
        0x00,0x38,0x80,0x00,0x00,0x00,0x40,0x00,0x84,0x01,0x00,0x00,
        0x06,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,
        0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,
        0xc0,0x0c,0x00,0x00,0x00,0x05,0x04,0x00,0x05,0x00,
        0x02,0x02,0x01,0xe3,0x00,0x08,0x78,0x9c,0x5d,0x73,0x04,0xc1,
        0x01,0xe3,0x06,0x00,0x09,0xe3,0x01,0x00,0x05,0x00,0x00,0x00,
        0x00,0x00,0x08,0x00,0x00,
    ]
}

#[test]
fn static_mlz_accepts_reference_metadata_without_changing_colors_or_transparency() {
    for scheme in [2u8,4] {
        for palette_flags in [0u8,0x80] {
            let mut bytes = extended();
            bytes[17] = scheme;
            bytes[23] = palette_flags;
            let image = zeebx::video::icon::decode(&bytes).unwrap();
            assert_eq!((image.width,image.height),(2,2));
            assert_eq!(image.rgba,[255,0,0,255,0,255,0,255,0,0,255,255,255,0,255,255]);
            bytes[85] = 224;
            bytes[99] = 3;
            bytes[110] = 2;
            let image = zeebx::video::icon::decode(&bytes).unwrap();
            assert_eq!(image.rgba,[0,0,255,255,0,255,0,255,0,0,255,255,255,0,255,255]);
        }
    }
}

fn extended() -> Vec<u8> {
    let original = saf_rgb332();
    let mut bytes = original[..96].to_vec();
    bytes.extend_from_slice(&[6,0,14,0,2,0x21,3,1,0,0,0,5,0,0,0,0,0,8,0,0]);
    let size = bytes.len() as u16;
    bytes[5..7].copy_from_slice(&size.to_be_bytes());
    bytes[17] = 2;
    bytes[23] = 0x80;
    bytes
}

#[test]
fn static_metadata_does_not_enable_other_effects_or_bypass_integrity_checks() {
    let original = extended();
    for (at,value) in [(101,0x20),(102,2),(103,2),(104,1),(105,1),(100,1),(100,3),(23,0x20),(17,1)] {
        let mut bytes=original.clone();
        bytes[at]=value;
        assert!(zeebx::video::icon::decode(&bytes).is_err(),"campo {at}");
    }
    for end in 0..original.len() {
        let mut bytes=original[..end].to_vec();
        if end>=7 { bytes[5..7].copy_from_slice(&(end as u16).to_be_bytes()); }
        assert!(zeebx::video::icon::decode(&bytes).is_err(),"truncado {end}");
    }
    let mut bytes=original;
    bytes[95]^=1;
    assert!(zeebx::video::icon::decode(&bytes).is_err());
}