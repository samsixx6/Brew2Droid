// Padding do CStaticWidget herdado do border: ABI pública e conteúdo recortado.
include!("common/modern_ui_fixture.rs");
impl F {
    fn padding_set(&mut self, widget: u32, prop: u32, value: u32) {
        assert_eq!(
            self.call(widget, 3, [0x801, prop, value]),
            1,
            "propriedade {prop:#x}"
        );
    }
    fn padding_ex(&mut self, widget: u32, set: bool, prop: u32, size: u32, value: u32) -> u32 {
        self.put(OUT, prop);
        self.put(OUT + 4, size);
        self.put(OUT + 8, OUT + 16);
        self.put(OUT + 16, value);
        self.call(widget, 3, [if set { 0x801 } else { 0x800 }, 0x600, OUT])
    }
    fn padding_text(&mut self, widget: u32) -> u32 {
        assert_eq!(self.call(widget, 12, [0x0101593b, OUT, 0]), 0);
        let model = self.read(OUT);
        let length = self.text("AA");
        self.call(model, 5, [TEXT, length, 0]);
        model
    }
    fn padding_canvas(&mut self) -> u32 {
        let bitmap = self.bitmap();
        assert_eq!(self.call(bitmap, 2, [0x0101e443, OUT, 0]), 0);
        self.read(OUT)
    }
    fn padding_red(&self, x0: i32, x1: i32, y0: i32, y1: i32) -> Vec<(i32, i32)> {
        let mut pixels = Vec::new();
        for y in y0..y1 {
            for x in x0..x1 {
                if self.m.screen().get_pixel(x, y) == 0xf800 {
                    pixels.push((x, y));
                }
            }
        }
        pixels
    }
}
#[test]
fn static_padding_extended_and_scalar_properties_share_state_and_validate_before_mutation() {
    let mut f = F::new();
    let w = f.create(0x010551e0);
    for (ex, scalar, value) in [
        (0x01077adb, 0x121, 4),
        (0x01077adc, 0x122, 3),
        (0x01077add, 0x123, 2),
        (0x01077ade, 0x124, 5),
    ] {
        assert_eq!(f.padding_ex(w, true, ex, 4, value), 1);
        assert_eq!(f.call(w, 3, [0x800, scalar, OUT]), 1);
        assert_eq!(f.read(OUT), value);
        assert_eq!(f.padding_ex(w, false, ex, 4, 0xdeadbeef), 1);
        assert_eq!(f.read(OUT + 16), value);
        assert_eq!(f.padding_ex(w, true, ex, 3, 9), 0);
        assert_eq!(f.call(w, 3, [0x801, scalar, u32::MAX]), 0);
        assert_eq!(f.call(w, 3, [0x800, scalar, OUT]), 1);
        assert_eq!(f.read(OUT), value);
    }
    assert_eq!(f.padding_ex(w, true, 0x01077ada, 4, 7), 1);
    for scalar in 0x121..=0x124 {
        assert_eq!(f.call(w, 3, [0x800, scalar, OUT]), 1);
        assert_eq!(f.read(OUT), 7);
    }
    assert_eq!(f.call(w, 3, [0x800, 0x120, OUT]), 0);
    f.call(w, 1, [0; 3]);
}
#[test]
fn static_padding_preferred_size_includes_both_edges_with_either_text_engine() {
    for layout in [false, true] {
        let mut f = F::new();
        let w = f.create(0x010551e0);
        let model = f.padding_text(w);
        if layout {
            f.padding_set(w, 0x417, 0x0103bda6);
        }
        f.call(w, 5, [OUT, 0, 0]);
        let before = (f.read(OUT), f.read(OUT + 4));
        f.padding_set(w, 0x120, 3);
        f.call(w, 5, [OUT, 0, 0]);
        assert_eq!((f.read(OUT), f.read(OUT + 4)), (before.0 + 6, before.1 + 6));
        f.call(w, 6, [OUT, 0, 0]);
        assert_eq!(
            (f.read(OUT), f.read(OUT + 4)),
            (30, 12),
            "padding não altera o extent solicitado"
        );
        f.call(model, 1, [0; 3]);
        f.call(w, 1, [0; 3]);
    }
}
#[test]
fn static_padding_moves_only_content_and_preserves_the_outer_background_with_either_text_engine() {
    for layout in [false, true] {
        let mut f = F::new();
        let w = f.create(0x010551e0);
        let model = f.padding_text(w);
        let c = f.padding_canvas();
        f.padding_set(w, 0x153, 0x200110);
        f.padding_set(w, 0x130, 0xff0000ff);
        f.padding_set(w, 0x140, 0x0000ff00);
        if layout {
            f.padding_set(w, 0x417, 0x0103bda6);
        }
        f.put(OUT, 40);
        f.put(OUT + 4, 30);
        f.call(w, 7, [OUT, 0, 0]);
        f.call(w, 10, [c, 0, 0]);
        let baseline = f.padding_red(0, 40, 0, 30);
        assert!(!baseline.is_empty());
        for (property, value) in [(0x121, 4), (0x122, 3), (0x123, 2), (0x124, 5)] {
            f.padding_set(w, property, value);
        }
        f.call(w, 10, [c, 50, 0]);
        let shifted = f.padding_red(50, 90, 0, 30);
        assert_eq!(
            shifted,
            baseline
                .iter()
                .map(|(x, y)| (x + 54, y + 2))
                .collect::<Vec<_>>()
        );
        for (x, y) in [(50, 0), (53, 8), (60, 1), (87, 10), (60, 29)] {
            assert_eq!(f.m.screen().get_pixel(x, y), 0x001f);
        }
        f.call(model, 1, [0; 3]);
        f.call(w, 1, [0; 3]);
    }
}
#[test]
fn static_padding_roundtrip_and_interface_aliases_survive_save_restore() {
    let mut f = F::new();
    let w = f.create(0x010551e0);
    let model = f.padding_text(w);
    assert_eq!(f.call(w, 2, [0x0103465d, OUT, 0]), 0);
    let draw = f.read(OUT);
    f.padding_set(w, 0x120, 3);
    f.padding_set(w, 0x121, 8);
    let save = f.m.grava_estado();
    f.padding_set(w, 0x120, 19);
    f.m.restaura_estado(&save).unwrap();
    for (property, value) in [(0x121, 8), (0x122, 3), (0x123, 3), (0x124, 3)] {
        assert_eq!(f.call(w, 3, [0x800, property, OUT]), 1);
        assert_eq!(f.read(OUT), value);
    }
    assert_eq!(f.call(draw, 2, [0x01015952, OUT, 0]), 0);
    assert_eq!(f.read(OUT), w);
    f.call(w, 1, [0; 3]);
    assert_eq!(f.call(w, 12, [0x0101593b, OUT, 0]), 0);
    assert_eq!(f.read(OUT), model);
    f.call(model, 1, [0; 3]);
    f.call(model, 1, [0; 3]);
    f.call(draw, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
}
