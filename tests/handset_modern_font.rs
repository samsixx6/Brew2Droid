//! Tipografia Brew MP: ABI numérico, glifos verdadeiros e modelo de nomes.
include!("common/modern_ui_fixture.rs");

#[test]
fn modern_hfont_has_typed_iqi_and_four_byte_font_info() {
    let mut f = F::new();
    let font = f.font();
    for iid in [0x01000001, 0x01001022, 0x0102ef8e] {
        assert_eq!(f.call(font, 2, [iid, OUT, 0]), 0);
        assert_eq!(f.read(OUT), font);
        assert_eq!(f.call(font, 1, [0; 3]), 1);
    }
    assert_eq!(f.call(font, 2, [0x01015952, OUT, 0]), 3);
    assert_eq!(f.read(OUT), 0);
    assert_eq!(f.call(font, 2, [0x01001022, 0, 0]), 14);
    f.m.cpu_mut().write_mem(OUT, &[0xa5; 12]).unwrap();
    assert_eq!(f.call(font, 5, [OUT + 4, 4, 0]), 0);
    let mut bytes = [0; 12];
    f.m.cpu().read_mem(OUT, &mut bytes).unwrap();
    let reference = F::reference();
    assert_eq!(
        &bytes[4..8],
        &[
            reference.ascent(16.0) as i16,
            (reference.descent(16.0) + 2) as i16
        ]
        .into_iter()
        .flat_map(i16::to_le_bytes)
        .collect::<Vec<_>>()
    );
    assert_eq!(&bytes[..4], &[0xa5; 4]);
    assert_eq!(&bytes[8..], &[0xa5; 4]);
    assert_eq!(f.call(font, 5, [OUT + 4, 12, 0]), 20);
    assert_eq!(f.call(font, 5, [0, 4, 0]), 14);
    assert_eq!(f.call(font, 1, [0; 3]), 0);
}
#[test]
fn modern_measure_uses_real_advances_exact_utf16_count_and_fit_width() {
    let mut f = F::new();
    let font = f.font();
    let reference = F::reference();
    let text = "ABC가나다";
    let width = reference.width(text, 16.0);
    assert_eq!(
        f.measure(font, text, i32::MAX),
        (text.encode_utf16().count() as u32, width)
    );
    let first = reference.width("AB", 16.0);
    assert_eq!(f.measure(font, text, first as i32), (2, first));
    assert_eq!(f.measure(font, text, 0), (0, 0));
    let n = f.text("AB");
    assert_eq!(
        f.extra(font, 4, [TEXT, n, u32::MAX], &[OUT + 32, OUT + 36]),
        14
    );
    assert_eq!(
        f.extra(font, 4, [TEXT, u32::MAX, 100], &[OUT + 32, OUT + 36]),
        14
    );
    assert_eq!(f.extra(font, 4, [TEXT, n, 100], &[0, OUT + 36]), 14);
    f.put(OUT + 32, 0xdeadbeef);
    assert_eq!(
        f.extra(font, 4, [TEXT, n, 100], &[OUT + 32, 0xdead0000]),
        14
    );
    assert_eq!(f.read(OUT + 32), 0xdeadbeef);
    assert_eq!(
        f.extra(font, 4, [0xdead0000, n, 100], &[OUT + 32, OUT + 36]),
        14
    );
    f.call(font, 1, [0; 3]);
}
#[test]
fn modern_draw_rasterizes_actual_glyphs_with_native_colors_and_clip() {
    let mut f = F::new();
    let font = f.font();
    let bitmap = f.bitmap();
    assert_eq!(
        f.draw(font, bitmap, 4, 5, "AB", 0xf800, 0x001f, [5, 6, 8, 10], 0),
        0
    );
    let reference = F::reference();
    let glyphs = reference.layout("AB", 16.0);
    let mut red = 0;
    for y in 0..24 {
        for x in 0..28 {
            let actual = f.m.screen().get_pixel(x, y);
            if (5..13).contains(&x) && (6..16).contains(&y) {
                let coverage = glyphs
                    .iter()
                    .filter_map(|g| {
                        let gx = x - 4 - g.x;
                        let gy = y - 5 - g.y;
                        if gx >= 0 && gy >= 0 && gx < g.width as i32 && gy < g.height as i32 {
                            Some(g.coverage[(gy as u32 * g.width + gx as u32) as usize])
                        } else {
                            None
                        }
                    })
                    .max()
                    .unwrap_or(0);
                if coverage >= 240 {
                    assert_eq!(actual, 0xf800);
                    red += 1;
                } else if coverage == 0 {
                    assert_eq!(actual, 0x001f);
                }
            } else {
                assert_eq!(actual, 0, "Draw não escapa do clip nem troca o destino");
            }
        }
    }
    assert!(red > 0);
    f.call(font, 1, [0; 3]);
}
#[test]
fn modern_draw_transparency_negative_coordinates_and_unsupported_bitmap() {
    let mut f = F::new();
    let font = f.font();
    let bitmap = f.bitmap();
    assert_eq!(
        f.draw(
            font,
            bitmap,
            -3,
            -2,
            "AB",
            0x07e0,
            0xf800,
            [0, 0, 12, 10],
            0x8000 | 0x40000000
        ),
        0
    );
    let mut green = 0;
    for y in 0..10 {
        for x in 0..12 {
            let p = f.m.screen().get_pixel(x, y);
            assert_ne!(p, 0xf800);
            if p == 0x07e0 {
                green += 1;
            }
        }
    }
    assert!(green > 0);
    assert_eq!(
        f.draw(font, 0xdead0000, 0, 0, "A", 0xffff, 0, [0, 0, 12, 10], 0),
        20
    );
    let n = f.text("A");
    assert_eq!(
        f.extra(font, 3, [bitmap, 0, 0], &[TEXT, n, 0xffff, 0, 0, 0]),
        14
    );
    assert_eq!(
        f.draw(font, bitmap, 0, 0, "A", 0xffff, 0, [0, 0, -1, 10], 0),
        0
    );
    f.call(font, 1, [0; 3]);
}
#[test]
fn modern_typeface_missing_file_fails_before_real_virtual_class_fallback() {
    let mut f = F::new();
    let face = f.create(0x01035156);
    assert_eq!(f.call(face, 2, [0x010340cb, OUT, 0]), 0);
    f.call(face, 1, [0; 3]);
    f.m.cpu_mut()
        .write_mem(TEXT, b"fs:/shared/ttf/helvetica neue ots.ttf\0")
        .unwrap();
    f.put(OUT, 0xdeadbeef);
    assert_ne!(f.extra(face, 3, [TEXT, 0, 0], &[15, OUT]), 0);
    assert_eq!(
        f.read(OUT),
        0,
        "arquivo ausente não pode devolver fonte substituta silenciosa"
    );
    assert_eq!(f.call(face, 5, [0x0103fb4b, 15, OUT]), 0);
    let font = f.read(OUT);
    assert_eq!(f.call(font, 6, [0x800, 0x1065, OUT + 8]), 1);
    assert_eq!(f.read(OUT + 8), 15);
    assert_eq!(f.call(face, 5, [0xdeadbeef, 15, OUT]), 20);
    assert_eq!(f.read(OUT), 0);
    assert_eq!(f.call(face, 1, [0; 3]), 1, "a fonte mantém o foundry vivo");
    assert_eq!(f.call(font, 1, [0; 3]), 0);
    assert_eq!(f.call(face, 0, [0; 3]), 14);
}
#[test]
fn modern_typeface_loads_real_ttf_file_and_memory_and_restores_references() {
    let mut f = F::new();
    let face = f.create(0x01035156);
    let bytes = raw_font();
    std::fs::write(f.root.join("licensed.ttf"), &bytes).unwrap();
    f.m.cpu_mut().write_mem(TEXT, b"licensed.ttf\0").unwrap();
    assert_eq!(f.extra(face, 3, [TEXT, 0, 0], &[19, OUT]), 0);
    let filefont = f.read(OUT);
    assert_eq!(
        f.measure(filefont, "ABC", i32::MAX).1,
        F::reference().width("ABC", 19.0)
    );
    let data = f.alloc(bytes.len() as u32);
    f.m.cpu_mut().write_mem(data, &bytes).unwrap();
    assert_eq!(f.extra(face, 4, [data, bytes.len() as u32, 13], &[OUT]), 0);
    let memoryfont = f.read(OUT);
    assert_eq!(
        f.measure(memoryfont, "가A", i32::MAX).1,
        F::reference().width("가A", 13.0)
    );
    let state = f.m.grava_estado();
    f.call(filefont, 1, [0; 3]);
    f.call(memoryfont, 1, [0; 3]);
    f.m.restaura_estado(&state).unwrap();
    assert_eq!(
        f.measure(memoryfont, "가A", i32::MAX).1,
        F::reference().width("가A", 13.0)
    );
    assert_eq!(f.call(face, 1, [0; 3]), 2);
    assert_eq!(f.call(filefont, 1, [0; 3]), 0);
    assert_eq!(f.call(memoryfont, 1, [0; 3]), 0);
}
#[test]
fn modern_hfont_properties_apply_real_bold_color_and_reject_unknown_effects() {
    let mut f = F::new();
    let font = f.font();
    let base = f.measure(font, "AB", i32::MAX).1;
    assert_eq!(f.call(font, 6, [0x801, 0x1001, 1]), 1);
    assert!(f.measure(font, "AB", i32::MAX).1 > base);
    assert_eq!(f.call(font, 6, [0x801, 0x1004, 0x0000ff00]), 1);
    assert_eq!(f.call(font, 6, [0x800, 0x1004, OUT]), 1);
    assert_eq!(f.read(OUT), 0x0000ff00);
    assert_eq!(f.call(font, 6, [0x801, 0x1002, 7]), 0);
    assert_eq!(f.call(font, 6, [0x801, 0x9999, 1]), 0);
    let bitmap = f.bitmap();
    f.draw(font, bitmap, 1, 1, "AB", 0xffff, 0, [0, 0, 24, 24], 0x8000);
    assert!((0..24).any(|y| (0..24).any(|x| f.m.screen().get_pixel(x, y) == 0xf800)));
    let state = f.m.grava_estado();
    f.m.restaura_estado(&state).unwrap();
    assert_eq!(f.call(font, 6, [0x800, 0x1001, OUT]), 1);
    assert_eq!(f.read(OUT), 1);
    f.call(font, 1, [0; 3]);
}
#[test]
fn font_map_default_list_and_typed_lookup_copy_names_and_retain_fonts() {
    let mut f = F::new();
    let map = f.create(0x010551cb);
    for iid in [0x01015936, 0x0103a3e6, 0x0101593a] {
        assert_eq!(f.call(map, 2, [iid, OUT, 0]), 0);
        assert_eq!(f.read(OUT), map);
        f.call(map, 1, [0; 3]);
    }
    let font = f.font();
    f.text("normal");
    assert_eq!(f.call(map, 8, [TEXT, font, 0]), 0);
    f.text("changed");
    assert_eq!(f.call(map, 5, [0; 3]), 1);
    f.text("normal");
    assert_eq!(f.call(map, 7, [TEXT, OUT, 0]), 0);
    assert_eq!(f.read(OUT), font);
    assert_eq!(f.call(font, 1, [0; 3]), 2);
    assert_eq!(f.call(map, 6, [0, OUT, 0]), 0);
    let style = f.read(OUT);
    assert_eq!(f.read(style), font);
    let mut name = [0; 14];
    f.m.cpu().read_mem(style + 4, &mut name).unwrap();
    assert_eq!(
        name,
        "normal\0"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()[..]
    );
    let state = f.m.grava_estado();
    f.call(map, 10, [0; 3]);
    f.m.restaura_estado(&state).unwrap();
    assert_eq!(f.call(map, 9, [TEXT, 0, 0]), 0);
    assert_eq!(f.call(map, 5, [0; 3]), 0);
    assert_eq!(f.call(map, 7, [TEXT, OUT, 0]), 1);
    assert_eq!(f.read(OUT), 0);
    f.call(map, 1, [0; 3]);
    assert_eq!(f.call(font, 1, [0; 3]), 0);
}
#[test]
fn font_map_null_and_empty_default_tags_are_distinct_and_invalid_outputs_fail() {
    let mut f = F::new();
    let map = f.create(0x010551cb);
    let font = f.font();
    assert_eq!(f.call(map, 8, [0, font, 0]), 0);
    f.text("");
    assert_eq!(f.call(map, 8, [TEXT, font, 0]), 0);
    assert_eq!(f.call(map, 5, [0; 3]), 2);
    assert_eq!(f.call(map, 7, [0, OUT, 0]), 0);
    assert_eq!(f.read(OUT), font);
    f.call(font, 1, [0; 3]);
    assert_eq!(f.call(map, 7, [TEXT, OUT, 0]), 0);
    f.call(font, 1, [0; 3]);
    assert_eq!(f.call(map, 7, [TEXT, 0, 0]), 14);
    assert_eq!(f.call(map, 6, [99, OUT, 0]), 14);
    assert_eq!(f.call(map, 8, [TEXT, 0, 0]), 14);
    assert_eq!(f.call(map, 8, [0xdead0000, font, 0]), 14);
    assert_eq!(f.call(map, 9, [0, 0, 0]), 0);
    assert_eq!(f.call(map, 5, [0; 3]), 1);
    assert_eq!(f.call(map, 10, [0; 3]), 0);
    assert_eq!(f.call(map, 1, [0; 3]), 0);
    assert_eq!(f.call(font, 1, [0; 3]), 0);
}

#[test]
fn modern_ttf_segments_and_invalid_data_fail_without_fabricating_fonts() {
    let mut f = F::new();
    let face = f.create(0x01035156);
    let bytes = raw_font();
    let mut file = vec![0xa5; 16];
    file.extend_from_slice(&bytes);
    file.extend_from_slice(&[0x5a; 8]);
    std::fs::write(f.root.join("segment.bin"), file).unwrap();
    f.m.cpu_mut().write_mem(TEXT, b"segment.bin\0").unwrap();
    assert_eq!(
        f.extra(face, 3, [TEXT, 16, bytes.len() as u32], &[17, OUT]),
        0
    );
    let font = f.read(OUT);
    assert_eq!(
        f.measure(font, "ABC", i32::MAX).1,
        F::reference().width("ABC", 17.0)
    );
    f.call(font, 1, [0; 3]);
    f.m.cpu_mut().write_mem(TEXT, b"segment.bin\0").unwrap();
    for (offset, len, size) in [
        (u32::MAX, 20, 16),
        (0, u32::MAX, 16),
        (0, 0, 0),
        (0, 0, u32::MAX),
    ] {
        f.put(OUT, 0xdeadbeef);
        assert_eq!(f.extra(face, 3, [TEXT, offset, len], &[size, OUT]), 14);
        assert_eq!(f.read(OUT), 0);
    }
    assert_eq!(f.extra(face, 3, [TEXT, 0, 0], &[16, 0]), 14);
    assert_eq!(f.extra(face, 4, [0xdead0000, 100, 16], &[OUT]), 14);
    assert_eq!(f.read(OUT), 0);
    f.m.cpu_mut().write_mem(TEXT, b"not a real font\0").unwrap();
    assert_eq!(f.extra(face, 4, [TEXT, 16, 16], &[OUT]), 20);
    assert_eq!(f.read(OUT), 0);
    assert_eq!(f.call(face, 5, [0x0103fb4b, 0, OUT]), 14);
    assert_eq!(f.read(OUT), 0);
    assert_eq!(f.call(face, 5, [0x0103fb4b, 257, OUT]), 20);
    assert_eq!(f.read(OUT), 0);
    f.call(face, 1, [0; 3]);
}

#[test]
fn font_map_listener_receives_actual_sizes_and_can_cancel_after_restore() {
    let mut f = F::new();
    let map = f.create(0x010551cb);
    let font = f.font();
    let listener = OUT + 192;
    let context = OUT + 256;
    let cb = loader::MODULE_BASE + 0x100;
    let code = [0xe92d40f0u32, 0xe8b100fc, 0xe8a000fc, 0xe8bd80f0];
    f.m.cpu_mut()
        .write_mem(
            cb,
            &code
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    f.m.cpu_mut().write_mem(listener, &[0; 24]).unwrap();
    f.put(listener + 8, cb);
    f.put(listener + 12, context);
    assert_eq!(f.call(map, 3, [listener, 0, 0]), 0);
    assert_eq!(f.call(map, 8, [0, font, 0]), 0);
    assert_eq!(
        (
            f.read(context),
            f.read(context + 4),
            f.read(context + 16),
            f.read(context + 20)
        ),
        (0x1001, map, 0, 1)
    );
    let state = f.m.grava_estado();
    f.call(map, 10, [0; 3]);
    f.m.restaura_estado(&state).unwrap();
    let cancel = f.read(listener + 16);
    assert_ne!(cancel, 0);
    f.run(cancel, [listener, 0, 0, 0]);
    assert_eq!(f.read(listener + 16), 0);
    f.put(context, 0xdeadbeef);
    assert_eq!(f.call(map, 9, [0, 0, 0]), 0);
    assert_eq!(f.read(context), 0xdeadbeef);
    assert_eq!(f.call(map, 3, [0, 0, 0]), 14);
    assert_eq!(f.call(map, 3, [0xdead0000, 0, 0]), 14);
    assert_eq!(f.call(map, 1, [0; 3]), 0);
    assert_eq!(f.call(font, 1, [0; 3]), 0);
}

#[test]
fn modern_font_factories_and_font_map_report_native_oom_and_recover() {
    let mut f = F::new();
    let face = f.create(0x01035156);
    let font = f.font();
    let map = f.create(0x010551cb);
    let mut live = Vec::new();
    loop {
        let status = f.run(
            aee::encode(Interface::Shell, 2),
            [loader::OBJECT_BASE, 0x01001001, OUT, 0],
        );
        if status != 0 {
            break;
        }
        live.push(f.read(OUT));
        assert!(live.len() <= loader::OBJECT_SIZE / 4);
    }
    for class in [0x01035156, 0x0103fb4b, 0x010551cb] {
        f.put(OUT, 0xdeadbeef);
        assert_eq!(
            f.run(
                aee::encode(Interface::Shell, 2),
                [loader::OBJECT_BASE, class, OUT, 0]
            ),
            2
        );
        assert_eq!(f.read(OUT), 0);
    }
    assert_eq!(f.call(face, 5, [0x0103fb4b, 16, OUT]), 2);
    assert_eq!(f.read(OUT), 0);
    assert_eq!(
        f.call(map, 8, [0, font, 0]),
        0,
        "a lista usa heap e a referência da fonte existente, sem nova identidade"
    );
    assert_eq!(f.call(font, 0, [0; 3]), 3);
    f.call(font, 1, [0; 3]);
    let freed = live.pop().unwrap();
    f.call(freed, 1, [0; 3]);
    assert_eq!(f.call(face, 5, [0x0103fb4b, 16, OUT]), 0);
    let next = f.read(OUT);
    assert_eq!(f.call(next, 1, [0; 3]), 0);
    for object in live {
        f.call(object, 1, [0; 3]);
    }
    f.call(map, 1, [0; 3]);
    f.call(font, 1, [0; 3]);
    f.call(face, 1, [0; 3]);
}

#[test]
fn review_character_spacing_is_signed_thousandths_of_pixels() {
    let mut f = F::new();
    let font = f.font();
    let base = f.measure(font, "AB", i32::MAX).1;
    assert_eq!(f.call(font, 6, [0x801, 0x1010, 250]), 1);
    assert_eq!(
        f.measure(font, "AB", i32::MAX).1,
        base + 1,
        "250 means0.25pixels, rounded once; not250pixels"
    );
    assert_eq!(
        f.call(font, 6, [0x801, 0x1010, 1000]),
        1,
        "primarySDK1000=1pixel is valid"
    );
    assert_eq!(f.measure(font, "AB", i32::MAX).1, base + 1);
    assert_eq!(
        f.call(font, 6, [0x801, 0x1010, (-1000i32) as u32]),
        1,
        "negative spacing is supported by primary contract"
    );
    assert_eq!(f.measure(font, "AB", i32::MAX).1, base - 1);
}
#[test]
fn review_rgba_none_restores_drawtext_foreground_instead_of_painting_black() {
    let mut f = F::new();
    let font = f.font();
    let bitmap = f.bitmap();
    assert_eq!(f.call(font, 6, [0x801, 0x1004, 0xff0000ff]), 1);
    assert_eq!(f.call(font, 6, [0x801, 0x1004, 0]), 1, "RGBA_NONE=0");
    assert_eq!(f.call(font, 6, [0x801, 0x1003, 1]), 1);
    assert_eq!(
        f.draw(font, bitmap, 1, 1, "AB", 0x07e0, 0, [0, 0, 40, 24], 0x8000),
        0
    );
    assert!(
        (0..24).any(|y| (0..40).any(|x| f.m.screen().get_pixel(x, y) == 0x07e0)),
        "IHFont says RGBA_NONE uses supplied NativeColor foreground"
    );
}

#[test]
fn review_descent_padding_changes_font_info_but_never_rendered_background_pixels() {
    let mut f = F::new();
    let base = f.font();
    let padded = f.font();
    let bitmap = f.bitmap();
    assert_eq!(f.call(padded, 6, [0x801, 0x1011, 20]), 1);
    f.draw(base, bitmap, 0, 0, "AB", 0xf800, 0x001f, [0, 0, 32, 80], 0);
    f.draw(
        padded,
        bitmap,
        40,
        0,
        "AB",
        0xf800,
        0x001f,
        [40, 0, 32, 80],
        0,
    );
    for y in 0..80 {
        for x in 0..32 {
            assert_eq!(
                f.m.screen().get_pixel(x, y),
                f.m.screen().get_pixel(x + 40, y),
                "PROP_DESCENTPADDING affects only GetInfo; pixel({x},{y}) changed"
            );
        }
    }
}

#[test]
fn review_fontmap_delete_notifies_registered_listener_when_guest_heap_is_exhausted() {
    let mut f = F::new();
    let model = f.create(0x010551cb);
    let font = f.font();
    assert_eq!(f.call(model, 8, [0, font, 0]), 0);
    let listener = OUT + 1024;
    let context = OUT + 2048;
    let callback = loader::MODULE_BASE + 0x500;
    let code = [0xe5901000u32, 0xe2811001, 0xe5801000, 0xe12fff1e];
    f.m.cpu_mut()
        .write_mem(
            callback,
            &code
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    for at in (0..24).step_by(4) {
        f.put(listener + at, 0);
    }
    f.put(listener + 8, callback);
    f.put(listener + 12, context);
    f.put(context, 0);
    assert_eq!(f.call(model, 3, [listener, 0, 0]), 0);
    let malloc_slot = (0..256)
        .find(|&s| Interface::Helpers.method(s) == Some("malloc"))
        .unwrap();
    let free_slot = (0..256)
        .find(|&s| Interface::Helpers.method(s) == Some("free"))
        .unwrap();
    let mut request = loader::HEAP_SIZE as u32;
    let mut allocations = Vec::new();
    while request > 0 {
        let allocation = f.run(
            aee::encode(Interface::Helpers, malloc_slot),
            [request, 0, 0, 0],
        );
        if allocation == 0 {
            request /= 2;
        } else {
            allocations.push(allocation);
        }
        assert!(allocations.len() < 256);
    }
    assert_eq!(f.call(model, 9, [0, 0, 0]), 0);
    let notifications = f.read(context);
    for allocation in allocations {
        f.run(
            aee::encode(Interface::Helpers, free_slot),
            [allocation, 0, 0, 0],
        );
    }
    f.call(model, 1, [0; 3]);
    f.call(font, 1, [0; 3]);
    assert_eq!(
        notifications, 1,
        "FontMap Delete must notify existing listeners even under OOM"
    );
}
