//! CSoftkeyWidget compõe rótulos IWidget sem inventar comandos ou modelos.
include!("common/modern_ui_fixture.rs");
impl F {
    fn softkey(&mut self, w: u32, prop: u32) -> u32 {
        assert_eq!(self.call(w, 3, [0x800, prop, OUT]), 1);
        self.read(OUT)
    }
    fn extent(&mut self, w: u32, x: u32, y: u32) {
        self.put(OUT, x);
        self.put(OUT + 4, y);
        assert_eq!(self.call(w, 7, [OUT, 0, 0]), 0);
    }
    fn label(&mut self, w: u32, text: &str) {
        assert_eq!(self.call(w, 12, [0x0101593b, OUT, 0]), 0);
        let m = self.read(OUT);
        let n = self.text(text);
        self.call(m, 5, [TEXT, n, 0]);
        self.call(m, 1, [0; 3]);
    }
    fn canvas(&mut self) -> u32 {
        let b = self.bitmap();
        assert_eq!(self.call(b, 2, [0x0101e443, OUT, 0]), 0);
        self.read(OUT)
    }
}
#[test]
fn softkey_factory_has_two_independent_owned_static_labels_and_no_public_model_or_container() {
    let mut f = F::new();
    let w = f.create(0x010551df);
    f.call(w, 6, [OUT, 0, 0]);
    assert_eq!((f.read(OUT), f.read(OUT + 4)), (50, 10));
    let a = f.softkey(w, 0x200);
    let b = f.softkey(w, 0x201);
    assert_ne!(a, b);
    for (p, align) in [(a, 0x210), (b, 0x240)] {
        f.call(p, 3, [0x800, 0x153, OUT]);
        assert_eq!(f.read(OUT) & 0x770, align);
        assert_eq!(f.call(p, 12, [0x0101593b, OUT, 0]), 0);
        f.call(f.read(OUT), 1, [0; 3]);
    }
    assert_eq!(f.call(w, 2, [0x01015932, OUT, 0]), 3);
    assert_eq!(f.read(OUT), 0);
    assert_eq!(f.call(w, 12, [0x0101593a, OUT, 0]), 3);
    assert_eq!(f.read(OUT), 0);
    assert_eq!(
        f.call(w, 3, [0x100, 0, 0]),
        0,
        "não converte uma tecla em comando fictício"
    );
    f.call(w, 1, [0; 3]);
    assert_eq!(f.call(a, 8, [OUT, 0, 0]), 0);
    assert_eq!(f.read(OUT), 0);
    assert_eq!(f.call(a, 1, [0; 3]), 0);
    assert_eq!(f.call(b, 1, [0; 3]), 0);
}
#[test]
fn softkey_replace_uses_direct_pointer_typed_qi_and_retains_independent_child() {
    let mut f = F::new();
    let w = f.create(0x010551df);
    let old = f.softkey(w, 0x200);
    let child = f.create(0x010551e0);
    assert_eq!(f.call(w, 3, [0x801, 0x200, child]), 1);
    assert_eq!(f.call(old, 8, [OUT, 0, 0]), 0);
    assert_eq!(f.read(OUT), 0);
    assert_eq!(f.call(old, 1, [0; 3]), 0);
    let held = f.softkey(w, 0x200);
    assert_eq!(held, child);
    f.call(held, 1, [0; 3]);
    f.call(child, 1, [0; 3]);
    let model = f.create(0x010551e9);
    assert_eq!(f.call(w, 3, [0x801, 0x201, model]), 0);
    f.call(model, 1, [0; 3]);
    assert_eq!(
        f.call(w, 3, [0x801, 0x200, w]),
        0,
        "não cria ciclo de propriedade"
    );
    let held = f.softkey(w, 0x200);
    assert_eq!(held, child);
    f.call(w, 1, [0; 3]);
    assert_eq!(f.call(held, 1, [0; 3]), 0);
}
#[test]
fn softkey_actual_glyphs_have_independent_colors_alignment_padding_and_clipping() {
    let mut f = F::new();
    let w = f.create(0x010551df);
    let a = f.softkey(w, 0x200);
    let b = f.softkey(w, 0x201);
    let font = f.font();
    f.call(w, 3, [0x801, 0x152, font]);
    f.call(font, 1, [0; 3]);
    f.label(a, "A");
    f.label(b, "B");
    f.call(a, 3, [0x801, 0x140, 0x0000ff00]);
    f.call(b, 3, [0x801, 0x140, 0x00ff0000]);
    f.extent(w, 80, 24);
    let c = f.canvas();
    f.call(w, 10, [c, 3, 4]);
    let mut red = 0;
    let mut green = 0;
    for y in 0..32 {
        for x in 0..88 {
            let p = f.m.screen().get_pixel(x, y);
            if p == 0xf800 {
                red += 1;
                assert!((7..43).contains(&x));
            }
            if p == 0x07e0 {
                green += 1;
                assert!((43..79).contains(&x));
            }
            if !(3..83).contains(&x) || !(4..28).contains(&y) {
                assert_eq!(p, 0);
            }
        }
    }
    assert!(red > 0 && green > 0);
    f.call(a, 1, [0; 3]);
    f.call(b, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
}
#[test]
fn softkey_center_flag_creates_real_label_and_save_preserves_membership() {
    let mut f = F::new();
    let w = f.create(0x010551df);
    assert_eq!(f.call(w, 3, [0x801, 0x153, 1]), 1);
    let c = f.softkey(w, 0x254);
    assert_ne!(c, 0);
    f.call(c, 3, [0x800, 0x153, OUT]);
    assert_eq!(f.read(OUT) & 0x770, 0x220);
    let state = f.m.grava_estado();
    assert_eq!(f.call(w, 3, [0x801, 0x153, 0]), 1);
    f.m.restaura_estado(&state).unwrap();
    f.call(w, 3, [0x800, 0x153, OUT]);
    assert_eq!(f.read(OUT), 1);
    assert_eq!(f.softkey(w, 0x254), c);
    f.call(c, 1, [0; 3]);
    f.call(c, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
}
#[test]
fn softkey_embedded_parent_is_distinct_container_and_invalidation_names_outer_widget() {
    let mut f = F::new();
    let w = f.create(0x010551df);
    let a = f.softkey(w, 0x200);
    f.call(a, 8, [OUT, 0, 0]);
    let parent = f.read(OUT);
    assert_ne!(parent, 0);
    assert_ne!(parent, w);
    assert_eq!(f.call(parent, 2, [0x01015932, OUT, 0]), 0);
    assert_eq!(f.read(OUT), parent);
    f.call(parent, 1, [0; 3]);
    let p = f.alloc(96);
    let code = p + 48;
    f.put(p, p + 8);
    f.put(p + 8 + 12, code);
    for (i, op) in [0xe5801004u32, 0xe3a00000, 0xe12fff1e]
        .into_iter()
        .enumerate()
    {
        f.put(code + i as u32 * 4, op);
    }
    f.call(w, 9, [p, 0, 0]);
    f.label(a, "x");
    assert_eq!(f.read(p + 4), w);
    f.call(w, 9, [0; 3]);
    f.call(parent, 1, [0; 3]);
    f.call(a, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
}
#[test]
fn softkey_bad_outputs_and_native_oom_leave_no_partial_instance() {
    let mut f = F::new();
    assert_eq!(
        f.run(
            aee::encode(Interface::Shell, 2),
            [loader::OBJECT_BASE, 0x010551df, 0, 0]
        ),
        14
    );
    let slot = (0..256)
        .find(|&s| Interface::Helpers.method(s) == Some("malloc"))
        .unwrap();
    for n in [1024 * 1024, 65536, 4096, 256, 32, 1] {
        while f.run(aee::encode(Interface::Helpers, slot), [n, 0, 0, 0]) != 0 {}
    }
    f.put(OUT, 0xdeadbeef);
    assert_eq!(
        f.run(
            aee::encode(Interface::Shell, 2),
            [loader::OBJECT_BASE, 0x010551df, OUT, 0]
        ),
        2
    );
    assert_eq!(f.read(OUT), 0);
}

#[test]
fn review_void_child_draw_residual_does_not_skip_following_label() {
    fn rendered_right_pixel(residual: u32) -> u16 {
        let mut f = F::new();
        let w = f.create(0x010551df);
        let left = f.softkey(w, 0x200);
        let right = f.softkey(w, 0x201);
        f.extent(w, 80, 24);
        assert_eq!(f.call(right, 3, [0x801, 0x130, 0x00ff00ff]), 1);
        let old = f.read(left);
        let replacement = f.alloc(80);
        for slot in 0..15 {
            let entry = f.read(old + 4 * slot);
            f.put(replacement + 4 * slot, entry);
        }
        let draw = replacement + 64;
        f.put(draw, 0xe3a00000 | residual); // mov r0, #residual
        f.put(draw + 4, 0xe12fff1e); // bx lr
        f.put(replacement + 4 * 10, draw);
        f.put(left, replacement);
        let canvas = f.canvas();
        f.call(w, 10, [canvas, 0, 0]); // IWidget::Draw returns void.
        let pixel = f.m.screen().get_pixel(41, 1);
        f.call(left, 1, [0; 3]);
        f.call(right, 1, [0; 3]);
        f.call(w, 1, [0; 3]);
        pixel
    }
    assert_eq!(
        rendered_right_pixel(0),
        0x07e0,
        "right sibling control renders"
    );
    assert_eq!(
        rendered_right_pixel(77),
        0x07e0,
        "void child Draw may leave R0=77; right sibling still must render"
    );
}

fn review_replace_slot(f: &mut F, widget: u32, slot: u32, ops: &[u32]) {
    let old = f.read(widget);
    let p = f.alloc(64 + 4 * ops.len() as u32);
    for i in 0..15 {
        let v = f.read(old + 4 * i);
        f.put(p + 4 * i, v);
    }
    let code = p + 64;
    for (i, op) in ops.iter().copied().enumerate() {
        f.put(code + 4 * i as u32, op);
    }
    f.put(p + 4 * slot, code);
    f.put(widget, p);
}
#[test]
fn review_void_child_getparent_residual_does_not_reject_unparented_widget() {
    fn accepts(residual: u32) -> u32 {
        let mut f = F::new();
        let w = f.create(0x010551df);
        let child = f.create(0x010551e0);
        review_replace_slot(
            &mut f,
            child,
            8,
            &[0xe3a02000, 0xe5812000, 0xe3a00000 | residual, 0xe12fff1e],
        );
        let accepted = f.call(w, 3, [0x801, 0x200, child]);
        f.call(child, 1, [0; 3]);
        f.call(w, 1, [0; 3]);
        accepted
    }
    assert_eq!(accepts(0), 1, "null parent control accepts child");
    assert_eq!(
        accepts(77),
        1,
        "void GetParent output is NULL; R0 is incidental"
    );
}
#[test]
fn review_void_child_preferred_extent_residual_preserves_written_dimensions() {
    fn preferred(residual: u32) -> (u32, u32) {
        let mut f = F::new();
        let w = f.create(0x010551df);
        for prop in [0x200, 0x201] {
            let child = f.softkey(w, prop);
            review_replace_slot(
                &mut f,
                child,
                5,
                &[
                    0xe3a0201e,
                    0xe5812000,
                    0xe3a0201d,
                    0xe5812004,
                    0xe3a00000 | residual,
                    0xe12fff1e,
                ],
            );
            f.call(child, 1, [0; 3]);
        }
        f.call(w, 5, [OUT, 0, 0]);
        let result = (f.read(OUT), f.read(OUT + 4));
        f.call(w, 1, [0; 3]);
        result
    }
    assert_eq!(
        preferred(0),
        (68, 29),
        "two 30px labels plus horizontal padding"
    );
    assert_eq!(
        preferred(77),
        (68, 29),
        "void GetPreferredExtent still wrote real dimensions"
    );
}
