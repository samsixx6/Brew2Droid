//! Contrato público: AEECButtonWidget.h, AEECBorderWidget.h, AEEModelHelpers.h
//! e UTButtonWidget.c do SDK 1.0.4.601. O consumidor Vlingo usa CID01053c8b
//! em30710 e configura os atributos abaixo antes de inserir o botão.
include!("common/modern_ui_fixture.rs");

const BUTTON: u32 = 0x01053c8b;
const SELECT: u32 = 0xe035;

impl F {
    fn button_get(&mut self, w: u32, prop: u32) -> u32 {
        assert_eq!(self.call(w, 3, [0x800, prop, OUT]), 1);
        self.read(OUT)
    }
    fn button_set(&mut self, w: u32, prop: u32, value: u32) {
        assert_eq!(
            self.call(w, 3, [0x801, prop, value]),
            1,
            "property {prop:#x}"
        );
    }
    fn button_ex(&mut self, w: u32, set: bool, prop: u32, size: u32, value: u32) -> u32 {
        self.put(OUT, prop);
        self.put(OUT + 4, size);
        self.put(OUT + 8, OUT + 16);
        self.put(OUT + 16, value);
        self.call(w, 3, [if set { 0x801 } else { 0x800 }, 0x600, OUT])
    }
    fn button_extent(&mut self, w: u32, width: u32, height: u32) {
        self.put(OUT, width);
        self.put(OUT + 4, height);
        self.call(w, 7, [OUT, 0, 0]); // void IWidget::SetExtent
    }
    fn button_size(&mut self, w: u32) -> (u32, u32) {
        self.call(w, 6, [OUT, 0, 0]);
        (self.read(OUT), self.read(OUT + 4))
    }
    fn button_child(&mut self, w: u32) -> u32 {
        self.call(w, 15, [OUT, 0, 0]); // void IDecorator::GetWidget, acquired
        self.read(OUT)
    }
    fn button_with_child(&mut self) -> (u32, u32) {
        let w = self.create(BUTTON);
        let child = self.create(0x010551e0);
        self.call(w, 14, [child, 0, 0]); // void IDecorator::SetWidget
        let held = self.button_child(w);
        assert_eq!(held, child);
        self.call(held, 1, [0; 3]);
        (w, child)
    }
    fn button_listener(&mut self, model: u32) -> (u32, u32) {
        let listener = self.alloc(24);
        let log = self.alloc(2048);
        self.m.cpu_mut().write_mem(listener, &[0; 24]).unwrap();
        self.m.cpu_mut().write_mem(log, &[0; 2048]).unwrap();
        let code = self.alloc(104);
        // ARM callback: count and copy ModelEvent into 32-byte records. Only
        // STATE1012 / CLICK1013 carry the three extra WidgetElemEvent fields.
        let ops = [
            0xe92d4010u32,
            0xe5902000,
            0xe2803010,
            0xe0833282,
            0xe2822001,
            0xe5802000,
            0xe5914000,
            0xe5834000,
            0xe591c004,
            0xe583c004,
            0xe591c008,
            0xe583c008,
            0xe59f202c,
            0xe1540002,
            0x3a000008,
            0xe2822001,
            0xe1540002,
            0x8a000005,
            0xe591c00c,
            0xe583c00c,
            0xe591c010,
            0xe583c010,
            0xe591c014,
            0xe583c014,
            0xe8bd8010,
            0x00001012,
        ];
        for (i, word) in ops.into_iter().enumerate() {
            self.put(code + 4 * i as u32, word);
        }
        self.put(listener + 8, code);
        self.put(listener + 12, log);
        assert_eq!(self.call(model, 3, [listener, 0, 0]), 0);
        (listener, log)
    }
    fn button_events(&self, log: u32) -> Vec<[u32; 6]> {
        let count = self.read(log);
        assert!(count < 60, "listener log overflow");
        (0..count)
            .map(|i| std::array::from_fn(|j| self.read(log + 16 + i * 32 + j as u32 * 4)))
            .collect()
    }
    fn button_cancel_listener(&mut self, listener: u32) {
        let cancel = self.read(listener + 16);
        assert_ne!(cancel, 0);
        self.run(cancel, [listener, 0, 0, 0]);
        assert_eq!(self.read(listener + 16), 0);
    }
}

#[test]
fn button_factory_exposes_real_decorator_widget_container_and_owns_its_child() {
    let mut f = F::new();
    let (w, child) = f.button_with_child();
    for iid in [0x01000001, 0x01015934, 0x01015952] {
        assert_eq!(f.call(w, 2, [iid, OUT, 0]), 0);
        assert_eq!(f.read(OUT), w);
        f.call(w, 1, [0; 3]);
    }
    assert_eq!(f.call(w, 2, [0x01015932, OUT, 0]), 0);
    let container = f.read(OUT);
    assert_ne!(container, w, "IContainer has a different public ABI");
    f.call(child, 8, [OUT, 0, 0]);
    assert_eq!(f.read(OUT), container);
    f.call(container, 1, [0; 3]); // GetParent acquired it
    assert_eq!(f.call(w, 2, [0x01015946, OUT, 0]), 3);
    assert_eq!(f.read(OUT), 0);
    f.call(w, 14, [w, 0, 0]); // reject a decorator cycle
    let held = f.button_child(w);
    assert_eq!(held, child);
    f.call(held, 1, [0; 3]);
    let non_widget = f.create(0x010551e9);
    f.call(w, 14, [non_widget, 0, 0]);
    let held = f.button_child(w);
    assert_eq!(
        held, child,
        "typed child rejection keeps the previous widget"
    );
    f.call(held, 1, [0; 3]);
    f.call(non_widget, 1, [0; 3]);
    f.call(container, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
    f.call(child, 8, [OUT, 0, 0]);
    assert_eq!(
        f.read(OUT),
        0,
        "destroying the decorator detaches its live child"
    );
    assert_eq!(f.call(child, 1, [0; 3]), 0);
}

#[test]
fn button_accepts_original_constructor_properties_and_delegates_the_real_text_model() {
    let mut f = F::new();
    let (w, child) = f.button_with_child();
    // Literal arguments from original 30730..30824, not an OEM style preset.
    for (prop, value) in [
        (0x153, 0x220),
        (0x130, 0x454545ff),
        (0x140, 0xffffffff),
        (0x145, 0xffffffff),
        (0x135, 0x0000ffff),
        (0x132, 0x454545ff),
        (0x315, 0x01012787),
        (0x110, 0x7f7f7fff),
        (0x115, 0xff),
        (0x112, 0x7f7f7fff),
        (0x111, 0xff),
        (0x119, 1),
        (0x103, 12),
        (0x204, 3),
        (0x205, 3),
        (0x252, (-3i32) as u32),
        (0x253, (-3i32) as u32),
    ] {
        f.button_set(w, prop, value);
    }
    assert_eq!(f.button_ex(w, true, 0x01077adf, 4, 1), 1);
    for key in [0x01077adb, 0x01077adc, 0x01077add, 0x01077ade] {
        assert_eq!(f.button_ex(w, true, key, 4, 2), 1);
    }
    for (prop, value) in [
        (0x119, 1),
        (0x103, 12),
        (0x204, 3),
        (0x205, 3),
        (0x252, (-3i32) as u32),
        (0x253, (-3i32) as u32),
        (0x131, 0x454545ff),
        (0x132, 0x454545ff),
        (0x133, 0x0000ffff),
        (0x111, 0xff),
        (0x112, 0x7f7f7fff),
        (0x113, 0xff),
        (0x114, 0xff),
    ] {
        assert_eq!(f.button_get(w, prop), value, "property {prop:#x}");
    }
    assert_eq!(f.button_get(child, 0x153) & 0x770, 0x220);
    let model = f.create(0x010551e5);
    assert_eq!(f.call(model, 2, [0x0103969b, OUT, 0]), 0);
    let rich = f.read(OUT);
    assert_eq!(f.call(w, 13, [rich, 0, 0]), 0);
    assert_eq!(f.call(w, 12, [0x01015946, OUT, 0]), 0);
    assert_eq!(
        f.read(OUT),
        model,
        "GetModel follows the decorated child's typed model"
    );
    f.call(model, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
    f.call(child, 1, [0; 3]);
    f.call(rich, 1, [0; 3]);
    f.call(model, 1, [0; 3]);
}

#[test]
fn button_border_padding_layout_and_selected_colors_produce_real_child_pixels() {
    let mut f = F::new();
    let (w, child) = f.button_with_child();
    f.button_set(w, 0x119, 0); // rectangle, no assumed OEM bevel geometry
    f.button_set(w, 0x203, 0);
    f.button_set(w, 0x100, 2);
    f.button_set(w, 0x120, 3);
    f.button_set(w, 0x110, 0x0000ffff); // MAKE_RGBA(255,0,0,255)
    f.button_set(w, 0x130, 0x00ff00ff);
    f.button_set(w, 0x135, 0x00ffffff); // MAKE_RGBA(255,255,0,255)
    f.button_set(child, 0x130, 0xff0000ff); // MAKE_RGBA(0,0,255,255)
    f.call(child, 5, [OUT, 0, 0]);
    let preferred = (f.read(OUT), f.read(OUT + 4));
    f.call(w, 5, [OUT, 0, 0]);
    assert_eq!(
        (f.read(OUT), f.read(OUT + 4)),
        (preferred.0 + 10, preferred.1 + 10),
        "preferred size includes the real child, two borders, and two paddings"
    );
    f.button_extent(w, 40, 24);
    assert_eq!(f.button_size(child), (30, 14));
    assert_eq!(f.call(w, 3, [0x800, 0x320, OUT]), 1);
    let mut rect = [0; 8];
    f.m.cpu().read_mem(OUT, &mut rect).unwrap();
    assert_eq!(rect, [5, 0, 5, 0, 30, 0, 14, 0]);
    let bitmap = f.bitmap();
    assert_eq!(f.call(bitmap, 2, [0x0101e443, OUT, 0]), 0);
    let canvas = f.read(OUT);
    f.call(w, 10, [canvas, 10, 12]);
    assert_eq!(f.m.screen().get_pixel(10, 12), 0xf800);
    assert_eq!(f.m.screen().get_pixel(12, 14), 0x07e0);
    assert_eq!(f.m.screen().get_pixel(15, 17), 0x001f);
    assert_eq!(f.m.screen().get_pixel(9, 12), 0);
    assert_eq!(f.m.screen().get_pixel(50, 12), 0);
    f.button_set(w, 0x151, 1);
    f.call(w, 10, [canvas, 10, 12]);
    assert_eq!(f.m.screen().get_pixel(12, 14), 0xffe0);
    assert_eq!(f.m.screen().get_pixel(15, 17), 0x001f);
}

#[test]
fn button_focus_and_corresponding_select_pair_emit_one_real_click_notification() {
    let mut f = F::new();
    let w = f.create(BUTTON);
    f.put(OUT, 0xdeadbeef);
    assert_eq!(f.call(w, 3, [0x702, 0, OUT]), 1);
    assert_eq!(f.read(OUT) & 255, 1);
    let model = f.button_get(w, 0x161);
    assert_ne!(model, 0);
    let (listener, log) = f.button_listener(model);
    assert_eq!(f.call(w, 3, [0x700, 1, 0]), 1);
    assert_eq!(f.call(w, 3, [0x701, 0, OUT]), 1);
    assert_eq!(f.read(OUT) & 255, 1);
    let before = f.button_events(log);
    assert!(
        before
            .iter()
            .any(|e| e[0] == 0x1012 && e[1] == model && e[5] == 1)
    );
    f.call(w, 3, [0x102, SELECT, 0]);
    assert!(!f.button_events(log).iter().any(|e| e[0] == 0x1013));
    assert_eq!(f.call(w, 3, [0x100, SELECT, 0]), 1);
    assert_eq!(f.button_get(w, 0x151), 1);
    assert!(!f.button_events(log).iter().any(|e| e[0] == 0x1013));
    assert_eq!(f.call(w, 3, [0x102, SELECT, 0]), 1);
    assert_eq!(f.button_get(w, 0x151), 0);
    f.call(w, 3, [0x102, SELECT, 0]);
    let events = f.button_events(log);
    let click: Vec<_> = events.iter().filter(|e| e[0] == 0x1013).collect();
    assert_eq!(click.len(), 1);
    assert_eq!((click[0][1], click[0][5]), (model, 1));
    let select: Vec<_> = events.iter().filter(|e| e[0] == 0x1008).collect();
    assert_eq!(select.len(), 1);
    assert_eq!((select[0][1], select[0][2]), (model, 0));
    f.button_cancel_listener(listener);
    f.call(model, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
}

#[test]
fn disabled_button_suppresses_key_clicks_but_honors_explicit_selection_and_focus() {
    let mut f = F::new();
    let w = f.create(BUTTON);
    let model = f.button_get(w, 0x161);
    let (listener, log) = f.button_listener(model);
    assert_eq!(f.button_ex(w, true, 0x01070bd3, 1, 1), 1);
    assert_eq!(f.button_ex(w, false, 0x01070bd3, 1, 0), 1);
    assert_eq!(f.read(OUT + 16) & 255, 1);
    assert_eq!(f.call(w, 3, [0x100, SELECT, 0]), 0);
    f.call(w, 3, [0x102, SELECT, 0]);
    assert_eq!(f.button_get(w, 0x151), 0);
    assert!(
        !f.button_events(log)
            .iter()
            .any(|e| matches!(e[0], 0x1008 | 0x1013))
    );
    f.button_set(w, 0x151, 1);
    assert_eq!(f.button_get(w, 0x151), 1);
    assert_eq!(f.call(w, 3, [0x700, 1, 0]), 1);
    assert!(
        f.button_events(log)
            .iter()
            .any(|e| e[0] == 0x1012 && e[5] == 7)
    );
    assert_eq!(f.call(w, 3, [0x801, 0xffff, 0]), 0);
    assert_eq!(f.button_ex(w, true, 0x7fffffff, 4, 0), 0);
    f.button_cancel_listener(listener);
    f.call(model, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
}

#[test]
fn button_save_round_trip_preserves_pressed_pair_child_and_view_model_listener() {
    let mut f = F::new();
    let (w, child) = f.button_with_child();
    let model = f.button_get(w, 0x161);
    let (listener, log) = f.button_listener(model);
    f.call(w, 3, [0x700, 1, 0]);
    assert_eq!(f.call(w, 3, [0x100, SELECT, 0]), 1);
    let save = f.m.grava_estado();
    f.call(w, 3, [0x102, SELECT, 0]);
    f.m.restaura_estado(&save).unwrap();
    assert_eq!(f.button_get(w, 0x151), 1);
    let held = f.button_child(w);
    assert_eq!(held, child);
    f.call(held, 1, [0; 3]);
    assert_eq!(f.call(w, 3, [0x102, SELECT, 0]), 1);
    assert_eq!(
        f.button_events(log)
            .iter()
            .filter(|e| e[0] == 0x1013)
            .count(),
        1
    );
    f.button_cancel_listener(listener);
    f.call(model, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
    f.call(child, 1, [0; 3]);
}

#[test]
fn button_factory_rejects_bad_outputs_and_truthfully_reports_native_oom() {
    let mut f = F::new();
    assert_eq!(
        f.run(
            aee::encode(Interface::Shell, 2),
            [loader::OBJECT_BASE, BUTTON, 0, 0]
        ),
        14
    );
    let malloc = (0..256)
        .find(|&slot| Interface::Helpers.method(slot) == Some("malloc"))
        .unwrap();
    for n in [1024 * 1024, 65536, 4096, 256, 32, 1] {
        while f.run(aee::encode(Interface::Helpers, malloc), [n, 0, 0, 0]) != 0 {}
    }
    f.put(OUT, 0xdeadbeef);
    assert_eq!(
        f.run(
            aee::encode(Interface::Shell, 2),
            [loader::OBJECT_BASE, BUTTON, OUT, 0]
        ),
        2
    );
    assert_eq!(f.read(OUT), 0);
}

#[test]
fn button_listener_fixture_copies_real_public_model_notifications() {
    // Control for the ARM listener used above: real IValueModel::Notify supplies
    // the documented event ABI, without a guessed Button implementation.
    let mut f = F::new();
    let model = f.create(0x010551e9);
    let (listener, log) = f.button_listener(model);
    for (i, value) in [0x1013, model, 0, 0, 0, 1].into_iter().enumerate() {
        f.put(OUT + i as u32 * 4, value);
    }
    assert_eq!(f.call(model, 4, [OUT, 0, 0]), 0);
    assert_eq!(f.button_events(log), vec![[0x1013, model, 0, 0, 0, 1]]);
    f.button_cancel_listener(listener);
    f.call(model, 1, [0; 3]);
}

#[test]
fn button_keeps_child_alive_when_setparent_callback_removes_its_membership() {
    let mut f = F::new();
    let w = f.create(BUTTON);
    let child = f.create(0x010551e0);
    f.button_extent(child, 38, 18);
    let original = f.read(child);
    let replacement = f.alloc(192);
    for slot in 0..15 {
        let entry = f.read(original + 4 * slot);
        f.put(replacement + 4 * slot, entry);
    }
    let output = f.alloc(8);
    f.put(output, 0xdeadbeef);
    f.put(output + 4, 0xdeadbeef);
    let parent_set = f.read(f.read(w) + 14 * 4);
    let original_setparent = f.read(original + 9 * 4);
    let original_release = f.read(original + 4);
    let original_extent = f.read(original + 6 * 4);
    let code = replacement + 64;
    // SetParent first delegates the real parent write. For a non-null parent,
    // consume this test's child reference, remove membership through the real
    // decorator ABI, then query the child while its SetParent is still active.
    let ops = [
        0xe92d4070u32,
        0xe1a04000,
        0xe1a05001,
        0xe59f3038,
        0xe12fff33,
        0xe3550000,
        0x0a00000a,
        0xe1a00004,
        0xe59f3028,
        0xe12fff33,
        0xe59f0024,
        0xe3a01000,
        0xe59f3020,
        0xe12fff33,
        0xe1a00004,
        0xe59f1018,
        0xe59f3018,
        0xe12fff33,
        0xe8bd8070,
        original_setparent,
        original_release,
        w,
        parent_set,
        output,
        original_extent,
    ];
    for (i, word) in ops.into_iter().enumerate() {
        f.put(code + i as u32 * 4, word);
    }
    f.put(replacement + 9 * 4, code);
    f.put(child, replacement);
    f.call(w, 14, [child, 0, 0]);
    assert_eq!((f.read(output), f.read(output + 4)), (38, 18));
    assert_eq!(f.button_child(w), 0);
    f.call(w, 1, [0; 3]);
}

#[test]
fn button_destructor_does_not_resurrect_or_repeat_release_through_free_callback() {
    for release_first in [false, true] {
        let mut f = F::new();
        let w = f.create(BUTTON);
        let table = f.read(w);
        let addref = f.read(table);
        let release = f.read(table + 4);
        let context = f.alloc(24);
        f.put(context, w);
        f.put(context + 4, if release_first { release } else { addref });
        f.put(context + 8, if release_first { addref } else { release });
        f.put(context + 12, 0xdeadbeef);
        f.put(context + 16, 0xdeadbeef);
        let code = f.alloc(44);
        for (i, op) in [
            0xe92d4010u32,
            0xe1a04000,
            0xe5940000,
            0xe5943004,
            0xe12fff33,
            0xe584000c,
            0xe5940000,
            0xe5943008,
            0xe12fff33,
            0xe5840010,
            0xe8bd8010,
        ]
        .into_iter()
        .enumerate()
        {
            f.put(code + 4 * i as u32, op);
        }
        f.put(OUT, 0);
        f.put(OUT + 4, context);
        f.put(OUT + 8, code);
        f.call(w, 4, [OUT, 0, 0]);
        assert_eq!(f.call(w, 1, [0; 3]), 0);
        assert_eq!(
            (f.read(context + 12), f.read(context + 16)),
            (0, 0),
            "base calls during destruction must not acquire or release the dead owner; release_first={release_first}"
        );
    }
}
