//! RootContainer tem canvas adquirido, filhos posicionados e invalidacao real.
include!("common/modern_ui_fixture.rs");
impl F {
    fn root_container(&mut self) -> (u32, u32) {
        let r = self.create(0x01055241);
        assert_eq!(self.call(r, 2, [0x01015952, OUT, 0]), 0);
        (r, self.read(OUT))
    }
    fn display_canvas(&mut self) -> u32 {
        let c = self.create(0x010551c8);
        let d = self.create(0x01001001);
        assert_eq!(self.call(c, 8, [d, 0, 0]), 0);
        self.call(d, 1, [0; 3]);
        c
    }
    fn bind_root(&mut self, r: u32, c: u32, rect: [i16; 4]) {
        self.m
            .cpu_mut()
            .write_mem(
                OUT + 32,
                &rect
                    .into_iter()
                    .flat_map(i16::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        self.call(r, 10, [c, OUT + 32, 0]);
    }
    fn child_extent(&mut self, w: u32, x: u32, y: u32) {
        self.put(OUT, x);
        self.put(OUT + 4, y);
        self.call(w, 7, [OUT, 0, 0]);
    }
    fn root_insert(&mut self, r: u32, w: u32, x: i32, y: i32) -> u32 {
        self.put(OUT, x as u32);
        self.put(OUT + 4, y as u32);
        self.put(OUT + 8, 1);
        self.call(r, 5, [w, 0, OUT])
    }
    fn flush_root(&mut self) {
        for _ in 0..2 {
            for o in self.m.deliver_signals(500000).unwrap() {
                assert!(matches!(o, Outcome::Returned { .. }));
            }
        }
    }
}
#[test]
fn root_default_container_and_widget_have_distinct_vtables_shared_lifetime() {
    let mut f = F::new();
    let (r, w) = f.root_container();
    assert_ne!(r, w);
    for id in [0x0102ec16, 0x01015954, 0x01015932] {
        assert_eq!(f.call(w, 2, [id, OUT, 0]), 0);
        assert_eq!(f.read(OUT), r);
        f.call(r, 1, [0; 3]);
    }
    assert_eq!(f.call(r, 2, [0x01015946, OUT, 0]), 3);
    assert_eq!(f.read(OUT), 0);
    assert_eq!(f.call(r, 11, [OUT, OUT + 16, 0]), 1);
    assert_eq!(f.read(OUT), 0);
    assert_eq!(f.call(r, 1, [0; 3]), 1);
    assert_eq!(f.call(w, 1, [0; 3]), 0);
}
#[test]
fn root_paints_children_after_scheduled_invalidation_on_actual_canvas_region() {
    let mut f = F::new();
    let (r, w) = f.root_container();
    let c = f.display_canvas();
    f.bind_root(r, c, [3, 4, 20, 20]);
    let a = f.create(0x010551e0);
    f.child_extent(a, 5, 6);
    f.call(a, 3, [0x801, 0x130, 0x00ff00ff]);
    assert_eq!(f.root_insert(r, a, 2, 3), 0);
    f.flush_root();
    assert_eq!(f.m.screen().get_pixel(3, 4), 0xffff);
    assert_eq!(f.m.screen().get_pixel(5, 7), 0x07e0);
    assert_eq!(f.m.screen().get_pixel(10, 7), 0xffff);
    assert_eq!(f.m.screen().get_pixel(23, 4), 0);
    f.call(a, 8, [OUT, 0, 0]);
    assert_eq!(f.read(OUT), r);
    f.call(r, 1, [0; 3]);
    f.call(c, 1, [0; 3]);
    f.call(a, 1, [0; 3]);
    f.call(r, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
}
#[test]
fn root_locate_canvas_refs_and_saved_deferred_draw_state_are_real() {
    let mut f = F::new();
    let (r, w) = f.root_container();
    let c = f.display_canvas();
    f.bind_root(r, c, [4, 5, 30, 20]);
    let a = f.create(0x010551e0);
    f.child_extent(a, 4, 4);
    f.call(a, 3, [0x801, 0x130, 0x0000ffff]);
    assert_eq!(f.root_insert(r, a, 7, 8), 0);
    f.call(r, 11, [OUT, OUT + 16, 0]);
    assert_eq!(f.read(OUT), c);
    f.call(c, 1, [0; 3]);
    f.m.cpu_mut()
        .write_mem(
            OUT + 16,
            &[1i16, 2, 3, 4]
                .into_iter()
                .flat_map(i16::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    assert_eq!(f.call(r, 4, [a, OUT, OUT + 16]), 0);
    assert_eq!(f.read(OUT), r);
    assert_eq!(f.read(OUT + 16), 8 | (10 << 16));
    f.call(r, 1, [0; 3]);
    let state = f.m.grava_estado();
    f.call(r, 6, [a, 0, 0]);
    f.m.restaura_estado(&state).unwrap();
    f.flush_root();
    assert_eq!(f.m.screen().get_pixel(11, 13), 0xf800);
    assert_eq!(f.call(r, 6, [a, 0, 0]), 0);
    f.call(a, 8, [OUT, 0, 0]);
    assert_eq!(f.read(OUT), 0);
    f.call(a, 1, [0; 3]);
    f.call(c, 1, [0; 3]);
    f.call(r, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
}
#[test]
fn root_defer_blocks_schedule_and_forcedraw_is_only_synchronous_when_requested() {
    let mut f = F::new();
    let (r, w) = f.root_container();
    let c = f.display_canvas();
    f.bind_root(r, c, [0, 0, 20, 20]);
    let a = f.create(0x010551e0);
    f.child_extent(a, 4, 4);
    f.root_insert(r, a, 0, 0);
    let counter = f.alloc(16);
    let code = f.alloc(32);
    for (i, op) in [
        0xe5901000u32,
        0xe2811001,
        0xe5801000,
        0xe3a0004d,
        0xe12fff1e,
    ]
    .into_iter()
    .enumerate()
    {
        f.put(code + 4 * i as u32, op);
    }
    f.call(a, 2, [0x0103465d, OUT, 0]);
    let draw = f.read(OUT);
    f.put(OUT, code);
    f.put(OUT + 4, counter);
    f.put(OUT + 8, 0);
    f.call(draw, 3, [OUT, 0, 0]);
    f.call(draw, 1, [0; 3]);
    f.flush_root();
    let old = f.read(counter);
    f.call(r, 3, [a, 0, 4]);
    f.flush_root();
    assert_eq!(f.read(counter), old);
    f.call(r, 3, [a, 0, 2]);
    assert_eq!(f.read(counter), old);
    f.flush_root();
    assert_eq!(f.read(counter), old + 1);
    f.call(r, 3, [a, 0, 8]);
    assert_eq!(f.read(counter), old + 2);
    f.call(a, 1, [0; 3]);
    f.call(c, 1, [0; 3]);
    f.call(r, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
}
#[test]
fn root_view_model_notifies_extent_and_draw_complete_with_exact_payload() {
    let mut f = F::new();
    let (r, w) = f.root_container();
    assert_eq!(f.call(w, 3, [0x800, 0x161, OUT]), 1);
    let m = f.read(OUT);
    assert_eq!(f.call(m, 2, [0x0101593b, OUT, 0]), 3);
    let listener = f.alloc(24);
    let counter = f.alloc(16);
    let code = f.alloc(40);
    for (i, op) in [
        0xe5912000u32,
        0xe3520a01,
        0xe5802000,
        0xe5902004,
        0xe2822001,
        0xe5802004,
        0xe12fff1e,
    ]
    .into_iter()
    .enumerate()
    {
        f.put(code + 4 * i as u32, op);
    }
    f.put(listener + 8, code);
    f.put(listener + 12, counter);
    f.call(m, 3, [listener, 0, 0]);
    let c = f.display_canvas();
    f.bind_root(r, c, [0, 0, 20, 21]);
    f.flush_root();
    assert!(f.read(counter + 4) >= 2);
    assert_eq!(f.read(counter), 0x1015);
    f.call(m, 1, [0; 3]);
    f.call(c, 1, [0; 3]);
    f.call(r, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
    assert_eq!(f.read(listener + 16), 0);
}
#[test]
fn root_refuses_self_cycles_bad_canvas_and_cleans_pending_callbacks_on_release() {
    let mut f = F::new();
    let (r, w) = f.root_container();
    assert_eq!(f.root_insert(r, w, 0, 0), 14);
    assert_eq!(f.call(r, 11, [0, OUT, 0]), 14);
    let font = f.font();
    f.m.cpu_mut()
        .write_mem(
            OUT + 16,
            &[0i16, 0, 10, 10]
                .into_iter()
                .flat_map(i16::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    assert_eq!(f.call(r, 10, [font, OUT + 16, 0]), 3);
    assert_eq!(f.call(r, 11, [OUT, OUT + 16, 0]), 1);
    let c = f.display_canvas();
    f.bind_root(r, c, [0, 0, 20, 20]);
    f.call(r, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
    f.flush_root();
    assert_eq!(f.m.screen().get_pixel(0, 0), 0);
    f.call(c, 1, [0; 3]);
    f.call(font, 1, [0; 3]);
}

#[test]
fn root_scrollbar_viewport_draw_settles_when_geometry_and_model_are_unchanged() {
    let mut f = F::new();
    let (r, w) = f.root_container();
    let c = f.display_canvas();
    f.bind_root(r, c, [0, 0, 100, 100]);
    let text = f.create(0x010551e0);
    f.child_extent(text, 80, 200);
    let vp = f.create(0x010551eb);
    f.child_extent(vp, 90, 90);
    assert_eq!(f.call(vp, 14, [text, 0, 0]), 0);
    let bar = f.create(0x010551dc);
    f.child_extent(bar, 100, 100);
    assert_eq!(f.call(bar, 14, [vp, 0, 0]), 0);
    assert_eq!(f.root_insert(r, bar, 0, 0), 0);
    for _ in 0..6 {
        f.m.deliver_signals(500000).unwrap();
    }
    assert!(
        f.m.deliver_signals(500000).unwrap().is_empty(),
        "layout identico duranteDraw nao pode agendar pintura perpetua"
    );
    f.call(text, 1, [0; 3]);
    f.call(vp, 1, [0; 3]);
    f.call(bar, 1, [0; 3]);
    f.call(c, 1, [0; 3]);
    f.call(r, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
}

#[test]
fn root_paint_does_not_accumulate_bitmap_references() {
    let mut f = F::new();
    let (r, w) = f.root_container();
    let c = f.display_canvas();
    f.bind_root(r, c, [0, 0, 20, 20]);
    f.flush_root();
    f.call(c, 3, [OUT, 0, 0]);
    let bitmap = f.read(OUT);
    f.call(bitmap, 0, [0; 3]);
    let before = f.call(bitmap, 1, [0; 3]);
    for _ in 0..10 {
        f.call(r, 3, [0, 0, 8]);
    }
    f.call(bitmap, 0, [0; 3]);
    assert_eq!(
        f.call(bitmap, 1, [0; 3]),
        before,
        "cada pintura devolve leases do bitmap e do canvas"
    );
    f.call(bitmap, 1, [0; 3]);
    f.call(c, 1, [0; 3]);
    f.call(r, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
}

#[test]
fn nested_root_screen_property_includes_parent_canvas_and_child_position() {
    let mut f = F::new();
    let (parent, pw) = f.root_container();
    let (child, cw) = f.root_container();
    let canvas = f.display_canvas();
    f.bind_root(parent, canvas, [10, 10, 100, 100]);
    f.child_extent(cw, 20, 20);
    assert_eq!(f.root_insert(parent, cw, 15, 20), 0);
    let rect = OUT + 16;
    f.m.cpu_mut()
        .write_mem(
            rect,
            &[1i16, 2, 73, 61]
                .into_iter()
                .flat_map(i16::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    for (at, value) in [(0, 0x0103f55c), (4, 8), (8, rect)] {
        f.put(OUT + at, value);
    }
    assert_eq!(f.call(cw, 3, [0x800, 0x600, OUT]), 1);
    let mut bytes = [0u8; 8];
    f.m.cpu().read_mem(rect, &mut bytes).unwrap();
    let values: Vec<i16> = bytes
        .chunks_exact(2)
        .map(|x| i16::from_le_bytes([x[0], x[1]]))
        .collect();
    assert_eq!(
        values,
        [25, 30, 73, 61],
        "primary UTRootContainer_TestPropScreenAsChild: parentCanvas+childpos; width/height untouched"
    );
    f.call(child, 1, [0; 3]);
    f.call(cw, 1, [0; 3]);
    f.call(parent, 1, [0; 3]);
    f.call(pw, 1, [0; 3]);
    f.call(canvas, 1, [0; 3]);
}
