//! CStaticWidget usa IValueModel e IHFont reais, sem alias de widgets.
include!("common/modern_ui_fixture.rs");
impl F {
    fn value_model(&mut self, w: u32) -> u32 {
        assert_eq!(self.call(w, 12, [0x0101593b, OUT, 0]), 0);
        let p = self.read(OUT);
        assert_ne!(p, w);
        p
    }
    fn widget_text(&mut self, model: u32, text: &str) {
        let units: Vec<_> = text.encode_utf16().collect();
        self.m
            .cpu_mut()
            .write_mem(
                OUT + 4096,
                &units
                    .iter()
                    .copied()
                    .chain([0])
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        self.call(model, 5, [OUT + 4096, units.len() as u32, 0]);
    }
    fn canvas(&mut self) -> u32 {
        let b = self.bitmap();
        assert_eq!(self.call(b, 2, [0x0101e443, OUT, 0]), 0);
        self.read(OUT)
    }
    fn extent(&mut self, w: u32, width: u32, height: u32) {
        self.put(OUT, width);
        self.put(OUT + 4, height);
        self.call(w, 7, [OUT, 0, 0]);
    }
}
#[test]
fn static_factory_default_model_is_typed_and_independent_of_widget_lifetime() {
    let mut f = F::new();
    let w = f.create(0x010551e0);
    assert_eq!(f.call(w, 6, [OUT, 0, 0]), 0);
    assert_eq!((f.read(OUT), f.read(OUT + 4)), (30, 12));
    let model = f.value_model(w);
    assert_eq!(f.call(w, 12, [0x01015946, OUT, 0]), 3);
    assert_eq!(f.read(OUT), 0);
    assert_eq!(f.call(model, 2, [0x0101593b, OUT, 0]), 0);
    assert_eq!(f.read(OUT), model);
    f.call(model, 1, [0; 3]);
    assert_eq!(f.call(w, 2, [0x01015952, OUT, 0]), 0);
    assert_eq!(f.read(OUT), w);
    f.call(w, 1, [0; 3]);
    assert_eq!(f.call(w, 1, [0; 3]), 0);
    f.widget_text(model, "alive");
    assert_eq!(f.call(model, 6, [OUT, 0, 0]), OUT + 4096);
    assert_eq!(f.read(OUT), 5);
    assert_eq!(f.call(model, 1, [0; 3]), 0);
}
#[test]
fn static_draws_model_text_through_retained_font_with_real_clip_and_background() {
    let mut f = F::new();
    let w = f.create(0x010551e0);
    let model = f.value_model(w);
    let font = f.font();
    assert_eq!(f.call(w, 3, [0x801, 0x152, font]), 1);
    f.call(font, 1, [0; 3]);
    f.widget_text(model, "AB");
    f.extent(w, 24, 20);
    assert_eq!(f.call(w, 3, [0x801, 0x153, 0x200000 | 0x110]), 1);
    assert_eq!(f.call(w, 3, [0x801, 0x130, 0xff0000ff]), 1);
    assert_eq!(f.call(w, 3, [0x801, 0x140, 0x0000ff00]), 1);
    let canvas = f.canvas();
    f.call(w, 10, [canvas, 4, 5]);
    let mut red = 0;
    for y in 0..30 {
        for x in 0..34 {
            let p = f.m.screen().get_pixel(x, y);
            if (4..28).contains(&x) && (5..25).contains(&y) {
                if p == 0xf800 {
                    red += 1;
                }
            } else {
                assert_eq!(p, 0);
            }
        }
    }
    assert!(
        red > 0,
        "Draw consulta IValueModel e chama IHFont no bitmap do canvas"
    );
    assert_eq!(f.m.screen().get_pixel(27, 24), 0x001f);
    assert_eq!(f.call(w, 3, [0x800, 0x152, OUT]), 1);
    assert_eq!(f.read(OUT), font);
    f.call(font, 1, [0; 3]);
    f.call(model, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
    assert_eq!(f.call(font, 0, [0; 3]), 14);
}
#[test]
fn static_preferred_extent_wrap_alignment_and_snapshot_use_actual_metrics() {
    let mut f = F::new();
    let w = f.create(0x010551e0);
    let model = f.value_model(w);
    let font = f.font();
    f.call(w, 3, [0x801, 0x152, font]);
    f.widget_text(model, "AB AB AB");
    assert_eq!(f.call(w, 5, [OUT, 0, 0]), 0);
    assert_eq!(f.read(OUT), F::reference().width("AB AB AB", 16.0));
    assert_eq!(f.call(w, 3, [0x801, 0x153, 0x100000 | 0x110]), 1);
    assert_eq!(f.call(w, 3, [0x801, 0x173, 20]), 1);
    assert_eq!(f.call(w, 5, [OUT, 0, 0]), 0);
    assert_eq!(f.read(OUT), 20);
    assert!(f.read(OUT + 4) >= 3 * (F::reference().ascent(16.0) + F::reference().descent(16.0)));
    f.extent(w, 20, 60);
    let state = f.m.grava_estado();
    f.widget_text(model, "changed");
    f.m.restaura_estado(&state).unwrap();
    let canvas = f.canvas();
    f.call(w, 3, [0x801, 0x140, 0x00ff0000]);
    f.call(w, 10, [canvas, 2, 3]);
    assert!((3..63).any(|y| (2..22).any(|x| f.m.screen().get_pixel(x, y) == 0x07e0)));
    f.call(model, 1, [0; 3]);
    f.call(font, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
}
#[test]
fn value_model_replacement_cleanup_and_listeners_survive_save_restore() {
    let mut f = F::new();
    let model = f.create(0x010551e9);
    let old = OUT + 1024;
    let new = OUT + 2048;
    let free = loader::MODULE_BASE + 0x100;
    let code = [0xe5901040u32, 0xe2811001, 0xe5801040, 0xe12fff1e];
    f.m.cpu_mut()
        .write_mem(
            free,
            &code
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    assert_eq!(f.call(model, 5, [old, 3, free]), 0);
    assert_eq!(f.call(model, 6, [OUT, 0, 0]), old);
    assert_eq!(f.read(OUT), 3);
    assert_eq!(f.call(model, 5, [new, 4, free]), 0);
    assert_eq!(f.read(old + 64), 1);
    assert_eq!(f.read(new + 64), 0);
    let state = f.m.grava_estado();
    f.call(model, 1, [0; 3]);
    assert_eq!(f.read(new + 64), 1);
    f.m.restaura_estado(&state).unwrap();
    assert_eq!(f.read(new + 64), 0);
    assert_eq!(f.call(model, 6, [0, 0, 0]), new);
    assert_eq!(f.call(model, 1, [0; 3]), 0);
    assert_eq!(f.read(new + 64), 1);
}
#[test]
fn value_model_adapters_use_the_documented_five_and_six_argument_callbacks() {
    let mut f = F::new();
    let model = f.create(0x010551e9);
    let get = loader::MODULE_BASE + 0x100;
    let set = loader::MODULE_BASE + 0x180;
    let getcode = [
        0xe5830000u32,
        0xe59d3000,
        0xe3a02003,
        0xe5832000,
        0xe12fff1e,
    ];
    f.m.cpu_mut()
        .write_mem(
            get,
            &getcode
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    let setcode = [
        0xe5830000u32,
        0xe59d3000,
        0xe3a02004,
        0xe5832000,
        0xe59d3004,
        0xe3a02000,
        0xe5832000,
        0xe12fff1e,
    ];
    f.m.cpu_mut()
        .write_mem(
            set,
            &setcode
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    f.call(model, 5, [42, 0, 0]);
    assert_eq!(f.call(model, 6, [0, 0, 0]), 42);
    f.call(model, 7, [get, OUT + 1024, 0]);
    assert_eq!(f.call(model, 6, [OUT, 0, 0]), OUT + 1024);
    assert_eq!(f.read(OUT), 3);
    f.call(model, 7, [0, 0, 0]);
    f.call(model, 8, [set, OUT + 2048, 0]);
    f.call(model, 5, [77, 0, 0]);
    assert_eq!(f.call(model, 6, [OUT, 0, 0]), OUT + 2048);
    assert_eq!(f.read(OUT), 4);
    f.call(model, 1, [0; 3]);
}
#[test]
fn static_models_fontmaps_parent_and_handler_lifetimes_are_real() {
    let mut f = F::new();
    let w = f.create(0x010551e0);
    let parent = f.create(0x010551ed);
    let model = f.create(0x010551e9);
    let map = f.create(0x010551cb);
    assert_eq!(f.call(w, 13, [model, 0, 0]), 0);
    assert_eq!(f.call(model, 1, [0; 3]), 1);
    assert_eq!(f.call(w, 3, [0x801, 0x418, map]), 1);
    assert_eq!(f.call(map, 1, [0; 3]), 1);
    f.call(w, 9, [parent, 0, 0]);
    assert_eq!(f.call(w, 8, [OUT, 0, 0]), 0);
    assert_eq!(f.read(OUT), parent);
    assert_eq!(f.call(parent, 1, [0; 3]), 1);
    f.call(w, 9, [0, 0, 0]);
    assert_eq!(f.call(w, 12, [0x0101593b, OUT, 0]), 0);
    assert_eq!(f.read(OUT), model);
    f.call(model, 1, [0; 3]);
    assert_eq!(f.call(w, 3, [0x801, 0x9999, 1]), 0);
    assert_eq!(f.call(w, 12, [0x0101593b, 0, 0]), 14);
    assert_eq!(f.call(w, 10, [0xdead0000, 0, 0]), 20);
    f.put(OUT, 0xaabbccdd);
    assert_eq!(f.call(w, 3, [0x702, 0, OUT]), 1);
    assert_eq!(f.read(OUT), 0xaabbcc00);
    f.call(w, 1, [0; 3]);
    assert_eq!(f.call(model, 0, [0; 3]), 14);
    assert_eq!(f.call(map, 0, [0; 3]), 14);
    f.call(parent, 1, [0; 3]);
}
#[test]
fn static_factory_native_oom_rolls_back_widget_model_and_font() {
    let mut f = F::new();
    let w = f.create(0x010551e0);
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
    assert_eq!(
        f.run(
            aee::encode(Interface::Shell, 2),
            [loader::OBJECT_BASE, 0x010551e0, OUT, 0]
        ),
        2
    );
    assert_eq!(f.read(OUT), 0);
    for _ in 0..2 {
        let p = live.pop().unwrap();
        f.call(p, 1, [0; 3]);
    }
    assert_eq!(
        f.run(
            aee::encode(Interface::Shell, 2),
            [loader::OBJECT_BASE, 0x010551e0, OUT, 0]
        ),
        2,
        "faltou a terceira identidade; as duas primeiras devem ser devolvidas"
    );
    for _ in 0..2 {
        let p = f.create(0x01001001);
        live.push(p);
    }
    for p in live {
        f.call(p, 1, [0; 3]);
    }
    assert_eq!(
        f.run(
            aee::encode(Interface::Shell, 2),
            [loader::OBJECT_BASE, 0x010551e0, 0, 0]
        ),
        14
    );
}

#[test]
fn static_rgba_none_black_and_state_backgrounds_follow_primary_helpers() {
    let mut f = F::new();
    let w = f.create(0x010551e0);
    let fill = f.font();
    let bitmap = f.bitmap();
    f.call(bitmap, 2, [0x0101e443, OUT, 0]);
    let canvas = f.read(OUT);
    f.extent(w, 8, 8);
    f.draw(fill, bitmap, 0, 0, "    ", 0, 0x07e0, [0, 0, 40, 24], 0);
    assert_eq!(f.call(w, 3, [0x801, 0x130, 0xff]), 1);
    f.call(w, 10, [canvas, 4, 4]);
    assert_eq!(f.m.screen().get_pixel(5, 5), 0, "RGBA_BLACK=ff é opaco");
    f.draw(fill, bitmap, 0, 0, "    ", 0, 0x07e0, [0, 0, 40, 24], 0);
    assert_eq!(f.call(w, 3, [0x801, 0x130, 0]), 1);
    f.call(w, 10, [canvas, 4, 4]);
    assert_eq!(
        f.m.screen().get_pixel(5, 5),
        0x07e0,
        "RGBA_NONE=0 preserva o canvas"
    );
    f.call(w, 3, [0x801, 0x130, 0xff0000ff]);
    f.call(w, 3, [0x801, 0x131, 0]);
    f.call(w, 10, [canvas, 4, 4]);
    assert_eq!(
        f.m.screen().get_pixel(5, 5),
        0x001f,
        "mudar fundo ativo não desliga fundo inativo"
    );
    f.draw(fill, bitmap, 0, 0, "    ", 0, 0x07e0, [0, 0, 40, 24], 0);
    f.call(w, 3, [0x700, 1, 0]);
    f.call(w, 10, [canvas, 4, 4]);
    assert_eq!(f.m.screen().get_pixel(5, 5), 0x07e0);
    f.call(w, 1, [0; 3]);
    f.call(fill, 1, [0; 3]);
}

#[test]
fn static_redraw_text_diagnostics_remain_bounded() {
    let mut f = F::new();
    let w = f.create(0x010551e0);
    let model = f.value_model(w);
    f.widget_text(model, "AB");
    f.extent(w, 30, 20);
    let canvas = f.canvas();
    for _ in 0..80 {
        f.call(w, 10, [canvas, 0, 0]);
    }
    assert_eq!(
        f.m.drawn_text().count(),
        64,
        "redraws não podem acumular strings sem limite"
    );
    f.call(model, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
}

#[test]
fn review_adapt_set_owner_release_cannot_overwrite_a_new_model_reusing_identity() {
    let mut f = F::new();
    let model = f.create(0x010551e9);
    let context = OUT + 1024;
    let callback = loader::MODULE_BASE + 0x500;
    f.put(context, model);
    f.put(context + 4, loader::OBJECT_BASE);
    f.put(context + 8, 0x010551e9);
    f.put(context + 12, 0);
    f.put(context + 16, aee::encode(Interface::Shell, 2));
    // Documented AdaptSet outputs are preserved, while application cleanup drops
    // the old owner and creates an independent empty model before returning.
    let code = [
        0xe92d4070u32,
        0xe1a04000,
        0xe5831000,
        0xe59d5010,
        0xe5852000,
        0xe59d5014,
        0xe3a06000,
        0xe5856000,
        0xe5940000,
        0xe590c000,
        0xe59cc004,
        0xe12fff3c,
        0xe5940004,
        0xe5941008,
        0xe284200c,
        0xe594c010,
        0xe12fff3c,
        0xe3a00000,
        0xe8bd8070,
    ];
    f.m.cpu_mut()
        .write_mem(
            callback,
            &code
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    f.call(model, 8, [callback, context, 0]);
    f.call(model, 5, [77, 0, 0]);
    let fresh = f.read(context + 12);
    assert_ne!(fresh, 0);
    assert_eq!(
        f.call(fresh, 6, [0; 3]),
        0,
        "new empty model {fresh:#x} must not receive suspended writes for destroyed model {model:#x}"
    );
    f.call(fresh, 1, [0; 3]);
}

#[test]
fn review_value_change_notifies_registered_listener_when_guest_heap_is_exhausted() {
    let mut f = F::new();
    let model = f.create(0x010551e9);
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
    assert_eq!(f.call(model, 5, [1, 0, 0]), 0);
    let notifications = f.read(context);
    for allocation in allocations {
        f.run(
            aee::encode(Interface::Helpers, free_slot),
            [allocation, 0, 0, 0],
        );
    }
    f.call(model, 1, [0; 3]);
    assert_eq!(
        notifications, 1,
        "void SetValue must notify existing listeners even under OOM"
    );
}

#[test]
fn review_static_draw_pins_widget_state_during_get_adapter_owner_release() {
    let mut f = F::new();
    let w = f.create(0x010551e0);
    let model = f.value_model(w);
    f.widget_text(model, "alive");
    let context = OUT + 1024;
    let get = loader::MODULE_BASE + 0x500;
    let state = f.read(w + 4);
    let malloc_slot = (0..256)
        .find(|&s| Interface::Helpers.method(s) == Some("malloc"))
        .unwrap();
    f.put(context, w);
    f.put(context + 4, aee::encode(Interface::Helpers, malloc_slot));
    f.put(context + 8, 0);
    f.put(context + 12, 0xdead0000);
    // Preserve AdaptGet outputs, drop the parent's last widget reference,
    // allocate unrelated live storage and initialize a word in that storage.
    let code = [
        0xe92d4070u32,
        0xe1a04000,
        0xe5831000,
        0xe59d5010,
        0xe5852000,
        0xe5940000,
        0xe590c000,
        0xe59cc004,
        0xe12fff3c,
        0xe3a000a0,
        0xe594c004,
        0xe12fff3c,
        0xe5840008,
        0xe594100c,
        0xe5801004,
        0xe3a00000,
        0xe8bd8070,
    ];
    f.m.cpu_mut()
        .write_mem(
            get,
            &code
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    f.call(model, 7, [get, context, 0]);
    let canvas = f.canvas();
    let code = f.call(w, 10, [canvas, 0, 0]);
    eprintln!(
        "Draw code={code}, old state={state:#x}, allocated={:#x}, saved font={:#x}, saved model={:#x}",
        f.read(context + 8),
        f.read(state + 4),
        f.read(state)
    );
    assert_eq!(
        code, 0,
        "Draw must keep its state alive across model callbacks"
    );
    assert_ne!(
        f.read(context + 8),
        state,
        "adapter allocation must not reclaim the widget's active state"
    );
    f.call(model, 7, [0; 3]);
    f.call(model, 1, [0; 3]);
}

#[test]
fn static_draw_handler_is_a_distinct_shared_facet_with_chaining_and_cleanup() {
    let mut f = F::new();
    let w = f.create(0x010551e0);
    let model = f.value_model(w);
    f.widget_text(model, "AB");
    f.extent(w, 30, 24);
    f.call(w, 3, [0x801, 0x140, 0xffffff00]);
    assert_eq!(
        f.call(w, 2, [0x0103465d, OUT, 0]),
        0,
        "StaticWidget expõe IDrawHandler real usado em0x34cae"
    );
    let draw = f.read(OUT);
    assert_ne!(draw, w);
    assert_ne!(f.read(draw), f.read(w));
    let context = OUT + 1024;
    let handler = loader::MODULE_BASE + 0x100;
    let free = loader::MODULE_BASE + 0x180;
    let code = [
        0xe92d4070u32,
        0xe1a04000,
        0xe5841000,
        0xe5842004,
        0xe5843008,
        0xe5940014,
        0xe594c010,
        0xe12fff3c,
        0xe3a00000,
        0xe8bd8070,
    ];
    f.m.cpu_mut()
        .write_mem(
            handler,
            &code
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    let code = [0xe590100cu32, 0xe2811001, 0xe580100c, 0xe12fff1e];
    f.m.cpu_mut()
        .write_mem(
            free,
            &code
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    f.put(OUT, handler);
    f.put(OUT + 4, context);
    f.put(OUT + 8, free);
    assert_eq!(f.call(draw, 3, [OUT, 0, 0]), 0);
    let old = f.read(OUT);
    let old_context = f.read(OUT + 4);
    assert_ne!(old, 0);
    f.put(context + 16, old);
    f.put(context + 20, old_context);
    let state = f.m.grava_estado();
    f.m.restaura_estado(&state).unwrap();
    let canvas = f.canvas();
    assert_eq!(f.call(w, 10, [canvas, 3, 4]), 0);
    assert_eq!(
        (f.read(context), f.read(context + 4), f.read(context + 8)),
        (canvas, 3, 4)
    );
    assert!((4..28).any(|y| (3..33).any(|x| f.m.screen().get_pixel(x, y) == 0xffff)));
    assert_eq!(f.call(w, 1, [0; 3]), 1);
    assert_eq!(f.call(draw, 2, [0x01015952, OUT, 0]), 0);
    assert_eq!(f.read(OUT), w);
    assert_eq!(f.call(w, 1, [0; 3]), 1);
    assert_eq!(f.call(draw, 1, [0; 3]), 0);
    assert_eq!(f.read(context + 12), 1);
    assert_eq!(f.call(draw, 0, [0; 3]), 14);
    f.call(model, 1, [0; 3]);
}

#[test]
fn value_model_adapters_use_independent_stack_outputs_when_heap_is_exhausted() {
    let mut f = F::new();
    let model = f.create(0x010551e9);
    let get = loader::MODULE_BASE + 0x100;
    let set = loader::MODULE_BASE + 0x180;
    let getcode = [
        0xe5830000u32,
        0xe59d3000,
        0xe3a02003,
        0xe5832000,
        0xe12fff1e,
    ];
    f.m.cpu_mut()
        .write_mem(
            get,
            &getcode
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    let setcode = [
        0xe5830000u32,
        0xe59d3000,
        0xe3a02004,
        0xe5832000,
        0xe59d3004,
        0xe3a02000,
        0xe5832000,
        0xe12fff1e,
    ];
    f.m.cpu_mut()
        .write_mem(
            set,
            &setcode
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    f.call(model, 8, [set, OUT + 1024, 0]);
    let malloc = (0..256)
        .find(|&s| Interface::Helpers.method(s) == Some("malloc"))
        .unwrap();
    let free = (0..256)
        .find(|&s| Interface::Helpers.method(s) == Some("free"))
        .unwrap();
    let mut size = loader::HEAP_SIZE as u32;
    let mut blocks = Vec::new();
    while size > 0 {
        let p = f.run(aee::encode(Interface::Helpers, malloc), [size, 0, 0, 0]);
        if p == 0 {
            size /= 2;
        } else {
            blocks.push(p);
        }
        assert!(blocks.len() < 256);
    }
    let set_status = f.call(model, 5, [77, 0, 0]);
    let raw = f.call(model, 6, [OUT, 0, 0]);
    let raw_len = f.read(OUT);
    f.call(model, 7, [get, OUT + 2048, 0]);
    f.put(OUT, 0xdeadbeef);
    let adapted = f.call(model, 6, [OUT, 0, 0]);
    let adapted_len = f.read(OUT);
    for p in blocks {
        f.run(aee::encode(Interface::Helpers, free), [p, 0, 0, 0]);
    }
    f.call(model, 1, [0; 3]);
    assert_eq!(
        (set_status, raw, raw_len),
        (0, OUT + 1024, 4),
        "void AdaptSet não depende de malloc para três saídas escalares"
    );
    assert_eq!(
        (adapted, adapted_len),
        (OUT + 2048, 3),
        "AdaptGet precisa entregar valor e comprimento mesmo com heap cheio"
    );
}
#[test]
fn static_background_public_rgba_white_zero_alpha_and_partial_alpha_follow_helpers() {
    let mut f = F::new();
    let w = f.create(0x010551e0);
    f.extent(w, 8, 8);
    let c = f.canvas();
    f.call(w, 3, [0x801, 0x130, 0xffffffff]);
    f.call(w, 10, [c, 2, 2]);
    assert_eq!(
        f.m.screen().get_pixel(3, 3),
        0xffff,
        "IWidget_SetBGColor recebe RGBA_WHITE, não RGB_NONE"
    );
    f.call(w, 3, [0x801, 0x130, 0]);
    f.call(w, 10, [c, 2, 2]);
    assert_eq!(
        f.m.screen().get_pixel(3, 3),
        0xffff,
        "alpha zero preserva branco"
    );
    f.call(w, 3, [0x801, 0x130, 0xff]);
    f.call(w, 10, [c, 2, 2]);
    assert_eq!(f.m.screen().get_pixel(3, 3), 0, "RGBA_BLACK é opaco");
    f.call(w, 3, [0x801, 0x130, 0x0000ff80]);
    f.call(w, 10, [c, 2, 2]);
    assert_eq!(
        f.m.screen().get_pixel(3, 3),
        0x8000,
        "alpha 128 mistura vermelho com preto"
    );
    f.call(w, 1, [0; 3]);
}
#[test]
fn static_font_class_system_large_selects_real_virtual_font_metrics_and_preserves_old_on_failure() {
    let mut f = F::new();
    let w = f.create(0x010551e0);
    let model = f.value_model(w);
    f.widget_text(model, "A");
    f.call(w, 5, [OUT, 0, 0]);
    let small = f.read(OUT + 4);
    assert_eq!(
        f.call(w, 3, [0x801, 0x315, 0x01012787]),
        1,
        "AEEFont.h FONTSYSLARGE usa a fonte real do dispositivo virtual"
    );
    assert_eq!(f.call(w, 3, [0x800, 0x152, OUT]), 1);
    let font = f.read(OUT);
    assert_eq!(f.call(font, 2, [0x01001022, OUT, 0]), 0);
    f.call(font, 1, [0; 3]);
    f.call(w, 5, [OUT, 0, 0]);
    assert!(f.read(OUT + 4) > small);
    let (fits, width) = f.measure(font, "A", 1000);
    assert_eq!(fits, 1);
    assert_eq!(width, F::reference().width("A", 28.0));
    assert_eq!(f.call(w, 3, [0x801, 0x315, 0xdeadbeef]), 0);
    f.call(w, 3, [0x800, 0x152, OUT]);
    assert_eq!(f.read(OUT), font);
    f.call(font, 1, [0; 3]);
    let mut objs = Vec::new();
    loop {
        let status = f.run(
            aee::encode(Interface::Shell, 2),
            [loader::OBJECT_BASE, 0x01001001, OUT, 0],
        );
        if status != 0 {
            break;
        }
        objs.push(f.read(OUT));
    }
    assert_eq!(f.call(w, 3, [0x801, 0x315, 0x01012787]), 0);
    f.call(w, 3, [0x800, 0x152, OUT]);
    assert_eq!(f.read(OUT), font);
    f.call(font, 1, [0; 3]);
    for o in objs {
        f.call(o, 1, [0; 3]);
    }
    f.call(font, 1, [0; 3]);
    f.call(model, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
}

#[test]
fn static_system_normal_bold_large_are_declared_virtual_font_profiles_with_real_pixels() {
    let mut f = F::new();
    let w = f.create(0x010551e0);
    let model = f.value_model(w);
    f.widget_text(model, "AB");
    f.extent(w, 100, 40);
    let canvas = f.canvas();
    for (class, size, bold) in [
        (0x01012786, 18, 0),
        (0x01012788, 18, 1),
        (0x01012787, 28, 0),
    ] {
        assert_eq!(f.call(w, 3, [0x801, 0x315, class]), 1);
        f.call(w, 3, [0x800, 0x152, OUT]);
        let font = f.read(OUT);
        assert_eq!(f.call(font, 6, [0x800, 0x1065, OUT]), 1);
        assert_eq!(f.read(OUT), size);
        assert_eq!(f.call(font, 6, [0x800, 0x1001, OUT]), 1);
        assert_eq!(f.read(OUT), bold);
        let (_fits, width) = f.measure(font, "AB", 1000);
        assert_eq!(
            width,
            F::reference().width("AB", size as f32) + if bold != 0 { 2 } else { 0 }
        );
        f.call(w, 3, [0x801, 0x140, 0x0000ff00]);
        f.call(w, 10, [canvas, 0, 0]);
        assert!((0..40).any(|y| (0..100).any(|x| f.m.screen().get_pixel(x, y) == 0xf800)));
        f.call(font, 1, [0; 3]);
    }
    f.call(model, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
}
