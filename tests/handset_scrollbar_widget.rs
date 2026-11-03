//! Contratos públicos de CScrollbarWidget; não dependem de nomes de interfaces novos.
include!("common/modern_ui_fixture.rs");
impl F {
    fn sb(&mut self) -> u32 {
        self.create(0x010551dc)
    }
    fn sb_prop(&mut self, w: u32, id: u32, value: u32) -> u32 {
        self.call(w, 3, [0x801, id, value])
    }
    fn sb_get(&mut self, w: u32, id: u32) -> u32 {
        self.put(OUT, 0xdeadbeef);
        assert_eq!(self.call(w, 3, [0x800, id, OUT]), 1);
        self.read(OUT)
    }
    fn sb_extent(&mut self, w: u32, x: u32, y: u32) {
        self.put(OUT, x);
        self.put(OUT + 4, y);
        assert_eq!(self.call(w, 7, [OUT, 0, 0]), 0);
    }
    fn sb_model(&mut self, w: u32) -> u32 {
        assert_eq!(self.call(w, 12, [0x0101593a, OUT, 0]), 0);
        self.read(OUT)
    }
    fn sb_event(&mut self, model: u32, vertical: bool, range: u16, visible: u16, position: u16) {
        let mut bytes = [0u8; 20];
        bytes[..4].copy_from_slice(&0x1064u32.to_le_bytes());
        bytes[12] = vertical as u8;
        for (i, v) in [range, visible, position].into_iter().enumerate() {
            bytes[14 + i * 2..16 + i * 2].copy_from_slice(&v.to_le_bytes());
        }
        self.m.cpu_mut().write_mem(OUT + 1024, &bytes).unwrap();
        self.call(model, 4, [OUT + 1024, 0, 0]);
    }
    fn sb_rect(&mut self, w: u32, id: u32) -> [i16; 4] {
        assert_eq!(self.call(w, 3, [0x800, id, OUT]), 1);
        let mut b = [0; 8];
        self.m.cpu().read_mem(OUT, &mut b).unwrap();
        std::array::from_fn(|i| i16::from_le_bytes([b[i * 2], b[i * 2 + 1]]))
    }
    fn sb_canvas(&mut self) -> u32 {
        let b = self.bitmap();
        assert_eq!(self.call(b, 2, [0x0101e443, OUT, 0]), 0);
        self.read(OUT)
    }
}
#[test]
fn scrollbar_factory_has_distinct_container_and_typed_independent_model() {
    let mut f = F::new();
    let w = f.sb();
    assert_eq!(f.call(w, 2, [0x01015932, OUT, 0]), 0);
    let c = f.read(OUT);
    assert_ne!(c, w);
    assert_eq!(f.call(c, 2, [0x01015934, OUT, 0]), 0);
    assert_eq!(f.read(OUT), w);
    f.call(w, 1, [0; 3]);
    let model = f.sb_model(w);
    assert_ne!(model, w);
    assert_eq!(f.call(model, 2, [0x0101593b, OUT, 0]), 3);
    assert_eq!(f.read(OUT), 0);
    assert_eq!(f.call(w, 12, [0x0101593b, OUT, 0]), 3);
    assert_eq!(f.read(OUT), 0);
    assert_eq!(f.sb_get(w, 0x182), 0);
    assert_eq!(f.sb_get(w, 0x180), 3);
    assert_eq!(f.sb_get(w, 0x181), 2);
    assert_eq!(f.call(w, 1, [0; 3]), 1);
    assert_eq!(f.call(c, 1, [0; 3]), 0);
    assert_eq!(f.call(model, 1, [0; 3]), 0);
    assert_eq!(f.call(w, 0, [0; 3]), 14);
}
#[test]
fn exact_scroll_event_uses_primary_proportional_thumb_formula_and_clamps_position() {
    let mut f = F::new();
    let w = f.sb();
    let model = f.sb_model(w);
    f.sb_extent(w, 40, 100);
    f.sb_prop(w, 0x153, 0);
    f.sb_event(model, true, 200, 50, 0);
    let first = f.sb_rect(w, 0x194);
    assert!(first[3] >= 4 && first[3] < 100);
    f.sb_event(model, true, 200, 50, 75);
    let middle = f.sb_rect(w, 0x194);
    f.sb_event(model, true, 200, 50, u16::MAX);
    let end = f.sb_rect(w, 0x194);
    assert_eq!(middle[1] - first[1], (end[1] - first[1]) / 2);
    assert_eq!(first[3], middle[3]);
    let fill = f.sb_rect(w, 0x197);
    assert_eq!(end[1] + end[3], fill[1] + fill[3]);
    assert_eq!(
        f.call(w, 3, [0x800, 0x196, OUT]),
        0,
        "horizontal query is invalid for vertical style"
    );
    f.call(model, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
}
#[test]
fn scrollbar_real_child_layout_returns_hidden_area_and_container_owns_one_child() {
    let mut f = F::new();
    let w = f.sb();
    let child = f.create(0x010551e0);
    let model = f.sb_model(w);
    f.sb_extent(w, 100, 80);
    f.call(w, 14, [child, 0, 0]);
    f.call(child, 6, [OUT, 0, 0]);
    assert_eq!(f.read(OUT), 100);
    f.sb_event(model, true, 200, 50, 0);
    f.call(child, 6, [OUT, 0, 0]);
    assert!(f.read(OUT) < 100);
    assert_eq!(f.read(OUT + 4), 80);
    assert_eq!(f.call(child, 8, [OUT, 0, 0]), 0);
    let parent = f.read(OUT);
    assert_ne!(parent, w);
    assert_eq!(f.call(parent, 7, [0, 1, 0]), child);
    f.call(child, 1, [0; 3]);
    f.call(parent, 1, [0; 3]);
    f.sb_event(model, true, 50, 50, 0);
    f.call(child, 6, [OUT, 0, 0]);
    assert_eq!(f.read(OUT), 100);
    f.call(w, 14, [0; 3]);
    f.call(child, 8, [OUT, 0, 0]);
    assert_eq!(f.read(OUT), 0);
    f.call(child, 1, [0; 3]);
    f.call(model, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
}
#[test]
fn scrollbar_native_thumb_and_child_draw_obey_extent_clip() {
    let mut f = F::new();
    let w = f.sb();
    let child = f.create(0x010551e0);
    let model = f.sb_model(w);
    f.sb_extent(w, 40, 60);
    f.call(w, 14, [child, 0, 0]);
    f.sb_prop(child, 0x130, 0x0000ffff);
    f.sb_prop(w, 0x190, 0xff0000ff);
    f.sb_prop(w, 0x189, 0x00ff00ff);
    f.sb_event(model, true, 200, 50, 0);
    let thumb = f.sb_rect(w, 0x194);
    let fill = f.sb_rect(w, 0x197);
    let canvas = f.sb_canvas();
    assert_eq!(f.call(w, 10, [canvas, 4, 5]), 0);
    assert_eq!(f.m.screen().get_pixel(4, 5), 0xf800);
    assert_eq!(
        f.m.screen()
            .get_pixel(4 + i32::from(thumb[0]), 5 + i32::from(thumb[1])),
        0x07e0
    );
    assert_eq!(
        f.m.screen()
            .get_pixel(4 + i32::from(fill[0]), 5 + i32::from(fill[1] + fill[3] - 1)),
        0x001f
    );
    assert_eq!(f.m.screen().get_pixel(44, 5), 0);
    assert_eq!(f.m.screen().get_pixel(4, 65), 0);
    f.call(child, 1, [0; 3]);
    f.call(model, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
}
#[test]
fn scrollbar_replaced_model_cancels_old_listener_and_snapshot_preserves_geometry() {
    let mut f = F::new();
    let w = f.sb();
    let old = f.sb_model(w);
    let new = f.create(0x010551e9);
    f.sb_extent(w, 40, 100);
    assert_eq!(f.call(w, 13, [new, 0, 0]), 0);
    f.sb_event(new, true, 200, 50, 75);
    let expected = f.sb_rect(w, 0x194);
    f.sb_event(old, true, 200, 50, 0);
    assert_eq!(f.sb_rect(w, 0x194), expected);
    let saved = f.m.grava_estado();
    f.sb_event(new, true, 200, 50, 150);
    assert_ne!(f.sb_rect(w, 0x194), expected);
    f.m.restaura_estado(&saved).unwrap();
    assert_eq!(f.sb_rect(w, 0x194), expected);
    f.call(old, 1, [0; 3]);
    f.call(new, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
}
#[test]
fn scrollbar_both_axes_and_explicit_errors_preserve_state() {
    let mut f = F::new();
    let w = f.sb();
    let model = f.sb_model(w);
    let child = f.create(0x010551e0);
    f.call(w, 14, [child, 0, 0]);
    f.sb_extent(w, 100, 80);
    assert_eq!(f.sb_prop(w, 0x182, 2), 1);
    f.sb_event(model, true, 200, 50, 0);
    f.sb_event(model, false, 300, 100, 100);
    let v = f.sb_rect(w, 0x195);
    let h = f.sb_rect(w, 0x196);
    assert!(v[3] > 0 && h[2] > 0);
    assert_eq!(f.sb_prop(w, 0x182, 3), 0);
    assert_eq!(f.sb_get(w, 0x182), 2);
    assert_eq!(f.sb_prop(w, 0x180, u32::MAX), 0);
    assert_eq!(f.sb_prop(w, 0x9999, 1), 0);
    assert_eq!(f.call(w, 12, [0x0101593a, 0, 0]), 14);
    assert_eq!(f.call(w, 13, [0xdead0000, 0, 0]), 14);
    assert_eq!(f.call(w, 14, [0xdead0000, 0, 0]), 14);
    f.call(w, 15, [OUT, 0, 0]);
    assert_eq!(f.read(OUT), child);
    f.call(child, 1, [0; 3]);

    assert_eq!(f.call(w, 10, [0xdead0000, 0, 0]), 20);
    assert_eq!(f.call(w, 3, [0x702, 0, OUT]), 1);
    assert_eq!(f.read(OUT) & 255, 0);
    f.call(child, 1, [0; 3]);
    f.call(model, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
}
#[test]
fn scrollbar_factory_oom_rolls_back_three_native_identities() {
    let mut f = F::new();
    let w = f.sb();
    f.call(w, 1, [0; 3]);
    let mut live = Vec::new();
    loop {
        let r = f.run(
            aee::encode(Interface::Shell, 2),
            [loader::OBJECT_BASE, 0x01001001, OUT, 0],
        );
        if r != 0 {
            break;
        }
        live.push(f.read(OUT));
        assert!(live.len() <= loader::OBJECT_SIZE / 4);
    }
    for _ in 0..2 {
        let p = live.pop().unwrap();
        f.call(p, 1, [0; 3]);
    }
    assert_eq!(
        f.run(
            aee::encode(Interface::Shell, 2),
            [loader::OBJECT_BASE, 0x010551dc, OUT, 0]
        ),
        2
    );
    assert_eq!(f.read(OUT), 0);
    for _ in 0..2 {
        live.push(f.create(0x01001001));
    }
    for p in live {
        f.call(p, 1, [0; 3]);
    }
    assert_eq!(
        f.run(
            aee::encode(Interface::Shell, 2),
            [loader::OBJECT_BASE, 0x010551dc, 0, 0]
        ),
        14
    );
}

#[test]
fn scrollbar_tracks_real_viewport_and_detaches_its_old_driving_listener() {
    let mut f = F::new();
    let w = f.sb();
    let vp = f.create(0x010551eb);
    let content = f.create(0x010551e0);
    f.sb_extent(content, 160, 150);
    f.call(vp, 14, [content, 0, 0]);
    f.sb_extent(w, 80, 60);
    f.call(w, 14, [vp, 0, 0]);
    assert_eq!(f.sb_prop(w, 0x182, 2), 1);
    let old_view = f.sb_get(vp, 0x161);
    let linked = f.sb_get(w, 0x161);
    assert_eq!(linked, old_view);
    f.call(linked, 1, [0; 3]);
    let first = f.sb_rect(w, 0x195);
    assert!(first[3] > 0);
    assert_eq!(f.call(w, 3, [0x900, 104, u16::MAX as u32]), 1);
    assert!(f.sb_rect(w, 0x195)[1] > first[1]);
    f.sb_prop(w, 0x182, 0);
    f.call(w, 14, [0; 3]);
    f.call(vp, 8, [OUT, 0, 0]);
    assert_eq!(f.read(OUT), 0);
    let detached = f.sb_rect(w, 0x194);
    f.sb_event(old_view, true, 200, 50, 0);
    assert_eq!(
        f.sb_rect(w, 0x194),
        detached,
        "old view model no longer drives this decorator"
    );
    let standalone = f.sb_model(w);
    f.sb_event(standalone, true, 200, 50, 0);
    assert_ne!(
        f.sb_rect(w, 0x194),
        detached,
        "default dummy model remains usable in standalone mode"
    );
    for object in [standalone, old_view, content, vp, w] {
        f.call(object, 1, [0; 3]);
    }
}

#[test]
fn scrollbar_common_background_uses_rgba_without_losing_scroll_rgbval_colors() {
    let mut f = F::new();
    let w = f.sb();
    f.sb_extent(w, 24, 24);
    let canvas = f.sb_canvas();
    f.sb_prop(w, 0x130, u32::MAX);
    assert_eq!(f.call(w, 10, [canvas, 2, 3]), 0);
    assert_eq!(
        f.m.screen().get_pixel(3, 4),
        0xffff,
        "RGBA_WHITE is an opaque common background"
    );
    f.sb_prop(w, 0x130, 0x0000ff80);
    f.call(w, 10, [canvas, 2, 3]);
    assert_eq!(
        f.m.screen().get_pixel(3, 4),
        0xfbef,
        "common background alpha blends into the current canvas"
    );
    f.sb_prop(w, 0x130, 0);
    f.call(w, 10, [canvas, 2, 3]);
    assert_eq!(
        f.m.screen().get_pixel(3, 4),
        0xfbef,
        "RGBA alpha zero preserves prior pixels"
    );
    f.sb_prop(w, 0x153, 1);
    f.sb_prop(w, 0x190, 0);
    let model = f.sb_model(w);
    f.sb_event(model, true, 200, 50, 150);
    let fill = f.sb_rect(w, 0x197);
    f.call(w, 10, [canvas, 2, 3]);
    assert_eq!(
        f.m.screen()
            .get_pixel(2 + i32::from(fill[0]), 3 + i32::from(fill[1])),
        0,
        "scroll-specific RGBVAL zero is opaque black"
    );
    f.call(model, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
}
