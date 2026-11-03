include!("common/modern_ui_fixture.rs");

fn solid(background: u8, palette: bool) -> Vec<u8> {
    // Contêiner próprio: quadro sem movimentos; nenhuma imagem de jogo embutida.
    let mut header = vec![
        1,
        6,
        3,
        2,
        8,
        4,
        0,
        0,
        2,
        2,
        0,
        if palette { 0x20 } else { 0 },
    ];
    if palette {
        header.extend_from_slice(&768u16.to_be_bytes());
        let mut colors = vec![0; 768];
        colors[usize::from(background) * 3..usize::from(background) * 3 + 3]
            .copy_from_slice(&[17, 34, 51]);
        header.extend(colors);
    }
    header.extend_from_slice(&[3, 3, 1, 0, 7]);
    let mut bytes = b"SAF\0\x02\0\0\x07\0\x02".to_vec();
    bytes.extend_from_slice(&(header.len() as u16).to_be_bytes());
    bytes.extend(header);
    bytes.extend_from_slice(&[6, 0, 2, background, 0, 8, 0, 0]);
    let len = bytes.len() as u16;
    bytes[5..7].copy_from_slice(&len.to_be_bytes());
    bytes
}

fn load_image(f: &mut F, bytes: &[u8]) -> u32 {
    std::fs::write(f.root.join("solid.bmp"), bytes).unwrap();
    f.m.cpu_mut().write_mem(TEXT, b"solid.bmp\0").unwrap();
    let slot = (0..256)
        .find(|&s| Interface::Shell.method(s) == Some("LoadResObject"))
        .unwrap();
    f.run(
        aee::encode(Interface::Shell, slot),
        [loader::OBJECT_BASE, TEXT, 0, 0],
    )
}

fn frame(f: &mut F, image: u32, number: u32) {
    let slot = (0..256)
        .find(|&s| Interface::Image.method(s) == Some("DrawFrame"))
        .unwrap();
    f.run(aee::encode(Interface::Image, slot), [image, number, 5, 6]);
}

#[test]
fn saf_each_empty_animation_frame_uses_its_own_complete_background() {
    let mut bytes = solid(224, false);
    bytes[26] = 2;
    let end = bytes.len() - 3;
    bytes.splice(end..end, [6, 0, 2, 28, 0]);
    let len = bytes.len() as u16;
    bytes[5..7].copy_from_slice(&len.to_be_bytes());
    assert!(
        zeebx::video::icon::decode(&bytes).is_err(),
        "miniatura estática não anima"
    );
    let mut f = F::new();
    let image = load_image(&mut f, &bytes);
    assert_ne!(image, 0);
    for (number, color) in [(0, 0xf800), (1, 0x07e0)] {
        frame(&mut f, image, number);
        for y in 6..8 {
            for x in 5..8 {
                assert_eq!(f.m.screen().get_pixel(x, y), color);
            }
        }
    }
}

#[test]
fn saf_missing_animation_records_remain_rejected() {
    let mut bytes = solid(0, false);
    bytes[26] = 2;
    let mut f = F::new();
    assert_eq!(load_image(&mut f, &bytes), 0);
}

#[test]
fn saf_background_only_frame_decodes_and_draws_its_opaque_native_color() {
    let bytes = solid(224, false);
    let image = zeebx::video::icon::decode(&bytes).unwrap();
    assert_eq!((image.width, image.height), (3, 2));
    assert!(image.rgba.chunks_exact(4).all(|p| p == [255, 0, 0, 255]));
    let mut f = F::new();
    std::fs::write(f.root.join("solid.bmp"), bytes).unwrap();
    f.m.cpu_mut().write_mem(TEXT, b"solid.bmp\0").unwrap();
    let slot = (0..256)
        .find(|&s| Interface::Shell.method(s) == Some("LoadResObject"))
        .unwrap();
    let image = f.run(
        aee::encode(Interface::Shell, slot),
        [loader::OBJECT_BASE, TEXT, 0, 0],
    );
    assert_ne!(image, 0);
    let slot = (0..256)
        .find(|&s| Interface::Image.method(s) == Some("Draw"))
        .unwrap();
    f.run(aee::encode(Interface::Image, slot), [image, 5, 6, 0]);
    for y in 6..8 {
        for x in 5..8 {
            assert_eq!(f.m.screen().get_pixel(x, y), 0xf800);
        }
    }
    assert_eq!(f.m.screen().get_pixel(4, 6), 0);
}

#[test]
fn saf_background_only_frame_uses_explicit_palette() {
    let image = zeebx::video::icon::decode(&solid(28, true)).unwrap();
    assert!(image.rgba.chunks_exact(4).all(|p| p == [17, 34, 51, 255]));
}

#[test]
fn saf_empty_frame_does_not_hide_missing_objects_or_trailing_commands() {
    let mut missing = solid(0, false);
    missing[22] = 1;
    assert!(zeebx::video::icon::decode(&missing).is_err());
    let mut commands = solid(0, false);
    let count = commands.len() - 4;
    commands[count] = 1;
    assert!(zeebx::video::icon::decode(&commands).is_err());
    let mut trailing = solid(0, false);
    let at = trailing.len() - 3;
    trailing.insert(at, 0);
    let len = trailing.len() as u16;
    trailing[5..7].copy_from_slice(&len.to_be_bytes());
    assert!(zeebx::video::icon::decode(&trailing).is_err());
}

#[test]
fn saf_zero_object_header_is_required_and_cannot_be_duplicated() {
    let mut duplicate = solid(0, false);
    duplicate.splice(24..24, [2, 2, 0, 0]);
    duplicate[10..12].copy_from_slice(&21u16.to_be_bytes());
    let len = duplicate.len() as u16;
    duplicate[5..7].copy_from_slice(&len.to_be_bytes());
    assert!(zeebx::video::icon::decode(&duplicate).is_err());
    let mut missing = solid(0, false);
    missing.drain(20..24);
    missing[10..12].copy_from_slice(&13u16.to_be_bytes());
    let len = missing.len() as u16;
    missing[5..7].copy_from_slice(&len.to_be_bytes());
    assert!(zeebx::video::icon::decode(&missing).is_err());
}

#[test]
#[ignore = "requer pacote SSA original em BREW_SSA_MODULE"]
fn original_ssa_background_resource_140_is_complete_and_white() {
    let path = std::path::PathBuf::from(std::env::var_os("BREW_SSA_MODULE").unwrap())
        .with_file_name("SSA.BAR");
    let resources = zeebx::loader::resfile::ResFile::parse(std::fs::read(path).unwrap()).unwrap();
    let bytes = zeebx::loader::resfile::blob_data(resources.get(6, 140).unwrap()).unwrap();
    let image = zeebx::video::icon::decode(bytes).unwrap();
    assert_eq!((image.width, image.height), (3, 1));
    assert_eq!(image.rgba, vec![255; 12]);
}

#[test]
#[ignore = "requer pacote SSA original em BREW_SSA_MODULE"]
fn original_ssa_156_contains_one_sprite_frame_and_nine_complete_blank_frames() {
    let path = std::path::PathBuf::from(std::env::var_os("BREW_SSA_MODULE").unwrap())
        .with_file_name("SSA.BAR");
    let resources = zeebx::loader::resfile::ResFile::parse(std::fs::read(path).unwrap()).unwrap();
    let bytes = zeebx::loader::resfile::blob_data(resources.get(6, 156).unwrap()).unwrap();
    let mut f = F::new();
    let image = load_image(&mut f, bytes);
    assert_ne!(image, 0);
    frame(&mut f, image, 0);
    let colors = (6..15)
        .flat_map(|y| (5..55).map(move |x| (x, y)))
        .map(|(x, y)| f.m.screen().get_pixel(x, y))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(colors, std::collections::BTreeSet::from([0, 0xf81f]));
    for n in 1..10 {
        frame(&mut f, image, n);
        for y in 6..15 {
            for x in 5..55 {
                assert_eq!(f.m.screen().get_pixel(x, y), 0xffff);
            }
        }
    }
}
