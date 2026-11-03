//! AEEITextLayout.h: ELLIPSIFY40000000 e AECHAR de elipse2026.
//! A chamada real de Vlingo usa essa flag no botão de aceite; ela não pode
//! transformar GetPreferredExtent em EUNSUPPORTED nem apenas fingir sucesso.
include!("common/modern_ui_fixture.rs");
const ELLIPSIFY: u32 = 0x40000000;
const MULTILINE: u32 = 0x10000000;

impl F {
    fn ellip_model(&mut self, text: &str) -> u32 {
        let model = self.create(0x010551e9);
        let units: Vec<_> = text.encode_utf16().collect();
        let pointer = self.alloc((units.len() as u32 + 1) * 2);
        self.m
            .cpu_mut()
            .write_mem(
                pointer,
                &units
                    .iter()
                    .copied()
                    .chain([0])
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        assert_eq!(self.call(model, 5, [pointer, units.len() as u32, 0]), 0);
        model
    }
    fn ellip_setup(&mut self) -> (u32, u32, u32) {
        let layout = self.create(0x0103bda6);
        let font = self.font();
        assert_eq!(self.call(layout, 3, [0x801, 0x152, font]), 1);
        assert_eq!(self.call(layout, 3, [0x801, 0x140, 0xffffffff]), 1);
        assert_eq!(self.call(layout, 3, [0x801, 0x130, 0xff0000ff]), 1);
        assert_eq!(self.call(font, 5, [OUT, 4, 0]), 0);
        let info = self.read(OUT);
        (layout, font, (info & 65535) + (info >> 16))
    }
    fn ellip_layout(&mut self, l: u32, model: u32, w: u32, h: u32, flags: u32) -> u32 {
        self.put(OUT, w);
        self.put(OUT + 4, h);
        self.call(l, 5, [model, OUT, flags])
    }
    fn ellip_size(&mut self, l: u32) -> (u32, u32, u32) {
        assert_eq!(self.call(l, 7, [OUT, OUT + 8, 0]), 0);
        (self.read(OUT), self.read(OUT + 4), self.read(OUT + 8))
    }
    fn ellip_draw(&mut self, l: u32) -> Vec<u8> {
        let bitmap = self.bitmap();
        assert_eq!(self.call(bitmap, 2, [0x0101e443, OUT, 0]), 0);
        let canvas = self.read(OUT);
        assert_eq!(self.call(l, 6, [canvas, 0, 0]), 0);
        self.call(canvas, 1, [0; 3]);
        self.m.screen().to_rgb565_bytes()
    }
    fn ellip_expect_text(
        &mut self,
        source: &str,
        expected: &str,
        ellipsis: u32,
        flags: u32,
        rows: u32,
    ) {
        let (layout, font, line_height) = self.ellip_setup();
        assert_eq!(self.call(layout, 3, [0x801, 0x349, ellipsis]), 1);
        let width = self
            .measure(font, expected.lines().last().unwrap(), i32::MAX)
            .1;
        let input = self.ellip_model(source);
        assert_eq!(
            self.ellip_layout(layout, input, width, line_height * rows, flags | ELLIPSIFY),
            0
        );
        let size = self.ellip_size(layout);
        assert!(size.0 <= width && size.1 <= line_height * rows);
        assert_eq!(size.2, rows);
        let actual = self.ellip_draw(layout);
        let literal = self.ellip_model(expected);
        assert_eq!(
            self.ellip_layout(layout, literal, width, line_height * rows, flags),
            0
        );
        let reference = self.ellip_draw(layout);
        assert!(
            actual == reference,
            "ellipsis glyphs must equal the literal visible text"
        );
        self.call(input, 1, [0; 3]);
        self.call(literal, 1, [0; 3]);
        self.call(font, 1, [0; 3]);
        self.call(layout, 1, [0; 3]);
    }
}

#[test]
fn ellipsify_unbounded_preferred_extent_keeps_the_normal_text_extent() {
    let mut f = F::new();
    let (l, font, _) = f.ellip_setup();
    let model = f.ellip_model("Accept");
    assert_eq!(f.call(l, 4, [model, 0, OUT]), 0);
    let ordinary = (f.read(OUT), f.read(OUT + 4));
    assert_eq!(
        f.call(l, 4, [model, ELLIPSIFY, OUT]),
        0,
        "documented flag used by the actual consumer"
    );
    assert_eq!((f.read(OUT), f.read(OUT + 4)), ordinary);
    assert!(ordinary.0 > 0 && ordinary.1 > 0);
    f.call(model, 1, [0; 3]);
    f.call(font, 1, [0; 3]);
    f.call(l, 1, [0; 3]);
}

#[test]
fn ellipsify_single_line_draws_the_default_ellipsis_inside_the_requested_width() {
    F::new().ellip_expect_text("ABCDEFGH", "AB…", 0x2026, 0, 1);
}

#[test]
fn ellipsify_uses_the_configured_aechar_instead_of_a_fixed_three_dot_suffix() {
    F::new().ellip_expect_text("ABCDEFGH", "AB!", '!' as u32, 0, 1);
}

#[test]
fn ellipsify_multiline_shortens_only_the_last_visible_line_when_text_overflows_height() {
    F::new().ellip_expect_text(
        "AB…\nCDEFGHIJKLMNOPQRSTUVWXYZ",
        "AB…\nCD…",
        0x2026,
        MULTILINE,
        2,
    );
}

#[test]
fn ellipsify_preserves_utf16_boundaries_and_the_original_model_text() {
    let mut f = F::new();
    let (l, font, height) = f.ellip_setup();
    let width = f.measure(font, "A😀…", i32::MAX).1;
    let model = f.ellip_model("A😀BCDEFGHIJK");
    assert_eq!(f.ellip_layout(l, model, width, height, ELLIPSIFY), 0);
    assert_eq!(
        f.call(l, 10, [2, OUT, 0]),
        14,
        "index2 divides the original surrogate pair"
    );
    assert_eq!(f.call(l, 10, [3, OUT, 0]), 0);
    let actual = f.ellip_draw(l);
    let literal = f.ellip_model("A😀…");
    assert_eq!(f.ellip_layout(l, literal, width, height, 0), 0);
    assert!(
        actual == f.ellip_draw(l),
        "UTF16-safe shortened glyphs must match the literal text"
    );
    let pointer = f.call(model, 6, [OUT, 0, 0]);
    assert_ne!(pointer, 0);
    let count = f.read(OUT);
    assert_eq!(count, "A😀BCDEFGHIJK".encode_utf16().count() as u32);
    let mut bytes = vec![0; count as usize * 2];
    f.m.cpu().read_mem(pointer, &mut bytes).unwrap();
    assert_eq!(
        String::from_utf16(
            &bytes
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect::<Vec<_>>()
        )
        .unwrap(),
        "A😀BCDEFGHIJK"
    );
    f.call(model, 1, [0; 3]);
    f.call(literal, 1, [0; 3]);
    f.call(font, 1, [0; 3]);
    f.call(l, 1, [0; 3]);
}

#[test]
fn ellipsify_does_not_add_a_suffix_when_all_text_fits() {
    let mut f = F::new();
    let (l, font, height) = f.ellip_setup();
    let model = f.ellip_model("AB");
    let width = f.measure(font, "ABCDEFGH", i32::MAX).1;
    assert_eq!(f.ellip_layout(l, model, width, height, ELLIPSIFY), 0);
    let actual = f.ellip_draw(l);
    assert_eq!(f.ellip_layout(l, model, width, height, 0), 0);
    assert!(
        actual == f.ellip_draw(l),
        "fitting text must remain unchanged"
    );
    f.call(model, 1, [0; 3]);
    f.call(font, 1, [0; 3]);
    f.call(l, 1, [0; 3]);
}

#[test]
fn ellipsify_too_narrow_for_the_marker_draws_no_overflowing_glyph() {
    let mut f = F::new();
    let (l, font, height) = f.ellip_setup();
    let marker_width = f.measure(font, "…", i32::MAX).1;
    assert!(marker_width > 0);
    let model = f.ellip_model("ABCDEFGH");
    assert_eq!(
        f.ellip_layout(l, model, marker_width - 1, height, ELLIPSIFY),
        0
    );
    assert!(f.ellip_size(l).0 <= marker_width - 1);
    f.ellip_draw(l);
    assert!(!(0..height).any(|y| {
        (0..marker_width - 1).any(|x| f.m.screen().get_pixel(x as i32, y as i32) == 0xffff)
    }));
    f.call(model, 1, [0; 3]);
    f.call(font, 1, [0; 3]);
    f.call(l, 1, [0; 3]);
}

#[test]
fn unknown_layout_flag_rejection_preserves_the_existing_cache_and_pixels() {
    let mut f = F::new();
    let (l, font, height) = f.ellip_setup();
    let model = f.ellip_model("AB");
    assert_eq!(f.ellip_layout(l, model, 60, height, 0), 0);
    let before = f.ellip_size(l);
    let pixels = f.ellip_draw(l);
    let other = f.ellip_model("ABCDEFGH");
    assert_eq!(f.ellip_layout(l, other, 20, height * 2, ELLIPSIFY | 1), 20);
    assert_eq!(f.ellip_size(l), before);
    assert!(
        f.ellip_draw(l) == pixels,
        "failed relayout must preserve the existing pixels"
    );
    f.call(model, 1, [0; 3]);
    f.call(other, 1, [0; 3]);
    f.call(font, 1, [0; 3]);
    f.call(l, 1, [0; 3]);
}

#[test]
fn ellipsis_property_rejects_a_non_aechar_scalar_without_truncating_it() {
    let mut f = F::new();
    let (l, font, _) = f.ellip_setup();
    assert_eq!(
        f.call(l, 3, [0x801, 0x349, 0x1f600]),
        0,
        "AECHAR is16bits, not a silently truncated scalar"
    );
    assert_eq!(f.call(l, 3, [0x801, 0x349, 0xd800]), 0);
    f.put(OUT, 0xaabbccdd);
    assert_eq!(f.call(l, 3, [0x800, 0x349, OUT]), 1);
    assert_eq!(f.read(OUT), 0xaabb2026);
    f.call(font, 1, [0; 3]);
    f.call(l, 1, [0; 3]);
}

#[test]
fn ellipsify_rich_empty_line_cannot_grow_beyond_the_captured_height_bound() {
    let mut f = F::new();
    let (layout, normal, height) = f.ellip_setup();
    let big = f.font();
    assert_eq!(f.call(big, 6, [0x801, 0x1011, 40]), 1);
    assert_eq!(f.call(big, 5, [OUT, 4, 0]), 0);
    let info = f.read(OUT);
    let big_height = (info & 65535) + (info >> 16);
    assert!(big_height > height);
    let map = f.create(0x010551cb);
    assert_eq!(f.call(map, 8, [0, normal, 0]), 0);
    f.text("big");
    assert_eq!(f.call(map, 8, [TEXT, big, 0]), 0);
    assert_eq!(f.call(layout, 3, [0x801, 0x418, map]), 1);
    let model = f.create(0x010551e5);
    assert_eq!(f.call(model, 2, [0x0103969b, OUT, 0]), 0);
    let rich = f.read(OUT);
    let count = f.text("[big]\nX[]");
    assert_eq!(f.call(rich, 6, [TEXT, count, 0]), 0);
    assert_eq!(f.ellip_layout(layout, model, 100, height, MULTILINE), 0);
    let ordinary = f.ellip_size(layout);
    assert_eq!(ordinary.2, 2);
    assert_eq!(f.call(layout, 10, [0, OUT, 0]), 0);
    let first_height = f.read(OUT + 4) >> 16;
    assert_eq!(first_height, height);
    eprintln!("base_height={height}, named_height={big_height}, ordinary={ordinary:?}, first_line_height={first_height}");
    assert_eq!(f.ellip_layout(layout, model, 100, height, MULTILINE | ELLIPSIFY), 0);
    let shortened = f.ellip_size(layout);
    eprintln!("requested=(100,{height}), ellipsified={shortened:?}");
    assert!(shortened.0 <= 100);
    assert!(
        shortened.1 <= height,
        "adding an ellipsis to a visible empty rich line must not grow the layout beyond its height bound: requested={height}, actual={shortened:?}"
    );
    for object in [model, rich, map, normal, big, layout] {
        f.call(object, 1, [0; 3]);
    }
}
