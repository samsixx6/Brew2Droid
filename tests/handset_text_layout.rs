//! ITextLayout usa medidas e glyphs reais, modelos adquiridos e cache salvo.
include!("common/modern_ui_fixture.rs");
impl F {
    fn value(&mut self, text: &str) -> u32 {
        let m = self.create(0x010551e9);
        let p = self.alloc(2 * (text.encode_utf16().count() as u32 + 1));
        self.m
            .cpu_mut()
            .write_mem(
                p,
                &text
                    .encode_utf16()
                    .chain([0])
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        self.call(m, 5, [p, text.encode_utf16().count() as u32, 0]);
        m
    }
    fn layout(&mut self, l: u32, m: u32, w: u32, h: u32, flags: u32) -> u32 {
        self.put(OUT, w);
        self.put(OUT + 4, h);
        self.call(l, 5, [m, OUT, flags])
    }
    fn canvas(&mut self) -> u32 {
        let b = self.bitmap();
        self.call(b, 2, [0x0101e443, OUT, 0]);
        self.read(OUT)
    }
    fn layout_font(&mut self, l: u32) -> u32 {
        let font = self.font();
        assert_eq!(self.call(l, 3, [0x801, 0x152, font]), 1);
        font
    }
}
#[test]
fn text_layout_factory_exposes_only_real_text_layout_and_validates_outputs() {
    let mut f = F::new();
    let l = f.create(0x0103bda6);
    assert_eq!(f.call(l, 2, [0x0103bd09, OUT, 0]), 0);
    assert_eq!(f.read(OUT), l);
    f.call(l, 1, [0; 3]);
    assert_eq!(f.call(l, 2, [0x01015952, OUT, 0]), 3);
    assert_eq!(f.read(OUT), 0);
    assert_eq!(f.call(l, 2, [0x0103bd09, 0, 0]), 14);
    assert_eq!(f.call(l, 1, [0; 3]), 0);
}
#[test]
fn text_layout_value_measure_draw_and_saved_cache_have_real_glyphs_and_bounds() {
    let mut f = F::new();
    let l = f.create(0x0103bda6);
    let font = f.layout_font(l);
    let m = f.value("AB");
    assert_eq!(f.call(l, 4, [m, 0, OUT]), 0);
    assert_eq!(f.read(OUT), F::reference().width("AB", 16.0));
    assert_eq!(f.layout(l, m, 24, 22, 0), 0);
    f.call(l, 7, [OUT, OUT + 8, 0]);
    assert_eq!(f.read(OUT), F::reference().width("AB", 16.0));
    assert_eq!(f.read(OUT + 8), 1);
    let state = f.m.grava_estado();
    f.call(m, 5, [0, 0, 0]);
    f.m.restaura_estado(&state).unwrap();
    f.call(m, 1, [0; 3]);
    f.call(font, 1, [0; 3]);
    let c = f.canvas();
    f.call(l, 3, [0x801, 0x140, 0x0000ffff]);
    f.call(l, 3, [0x801, 0x130, 0xff0000ff]);
    assert_eq!(f.call(l, 6, [c, 3, 4]), 0);
    assert!((4..26).any(|y| (3..27).any(|x| f.m.screen().get_pixel(x, y) == 0xf800)));
    assert_eq!(f.m.screen().get_pixel(26, 25), 0x001f);
    assert_eq!(f.m.screen().get_pixel(27, 25), 0);
    f.call(l, 1, [0; 3]);
}
#[test]
fn text_layout_wraps_at_utf16_indices_and_reports_real_caret_geometry() {
    let mut f = F::new();
    let l = f.create(0x0103bda6);
    let font = f.layout_font(l);
    let m = f.value("AB\nCD");
    assert_eq!(f.layout(l, m, 100, 100, 0x10000000), 0);
    assert_eq!(f.call(l, 7, [OUT, OUT + 8, 0]), 0);
    assert_eq!(f.read(OUT + 8), 2);
    assert_eq!(f.call(l, 8, [3, OUT, 0]), 0);
    assert_eq!(f.read(OUT), 1);
    assert_eq!(f.call(l, 9, [1, OUT, 0]), 0);
    assert_eq!(f.read(OUT), 3);
    assert_eq!(f.call(l, 10, [1, OUT, 0]), 0);
    let mut rect = [0u8; 8];
    f.m.cpu().read_mem(OUT, &mut rect).unwrap();
    assert_eq!(
        i16::from_le_bytes([rect[0], rect[1]]) as u32,
        F::reference().width("A", 16.0)
    );
    assert!(i16::from_le_bytes([rect[6], rect[7]]) > 0);
    assert_eq!(f.call(l, 10, [99, OUT, 0]), 14);
    f.call(m, 1, [0; 3]);
    f.call(font, 1, [0; 3]);
    f.call(l, 1, [0; 3]);
}
#[test]
fn text_layout_rich_model_resolves_named_fontmap_and_keeps_separate_abi() {
    let mut f = F::new();
    let l = f.create(0x0103bda6);
    let map = f.create(0x010551cb);
    let normal = f.font();
    let bold = f.font();
    f.call(bold, 6, [0x801, 0x1001, 1]);
    f.call(bold, 6, [0x801, 0x1004, 0x0000ffff]);
    f.call(map, 8, [0, normal, 0]);
    f.text("b");
    f.call(map, 8, [TEXT, bold, 0]);
    f.call(l, 3, [0x801, 0x418, map]);
    let model = f.create(0x010551e5);
    f.call(model, 2, [0x0103969b, OUT, 0]);
    let rich = f.read(OUT);
    let n = f.text("A[b]B[]");
    assert_eq!(f.call(rich, 6, [TEXT, n, 0]), 0);
    assert_eq!(f.layout(l, model, 100, 30, 0), 0);
    f.call(l, 7, [OUT, OUT + 8, 0]);
    assert_eq!(f.read(OUT), F::reference().width("AB", 16.0) + 1);
    f.call(model, 1, [0; 3]);
    f.call(rich, 1, [0; 3]);
    f.call(map, 1, [0; 3]);
    f.call(normal, 1, [0; 3]);
    f.call(bold, 1, [0; 3]);
    let c = f.canvas();
    f.call(l, 6, [c, 0, 0]);
    assert!((0..30).any(|y| (8..30).any(|x| f.m.screen().get_pixel(x, y) == 0xf800)));
    f.call(l, 1, [0; 3]);
}
#[test]
fn text_layout_bad_layout_does_not_replace_saved_geometry_or_accept_unsupported_flags() {
    let mut f = F::new();
    let l = f.create(0x0103bda6);
    let m = f.value("AB");
    assert_eq!(f.layout(l, m, 50, 30, 0), 0);
    assert_eq!(f.call(l, 5, [m, 0, 0]), 14);
    assert_eq!(f.layout(l, m, 50, 30, 1), 20);
    f.put(OUT, 0xdeadbeef);
    assert_eq!(f.call(l, 7, [OUT, OUT + 8, 0]), 0);
    assert!(f.read(OUT) < 50);
    assert_eq!(f.call(l, 3, [0x801, 0x9999, 1]), 0);
    assert_eq!(f.call(l, 6, [0xdead0000, 0, 0]), 20);
    f.call(m, 1, [0; 3]);
    f.call(l, 1, [0; 3]);
}
#[test]
fn static_selected_text_layout_accepts_typed_text_model_without_value_model_alias() {
    let mut f = F::new();
    let w = f.create(0x010551e0);
    let model = f.create(0x010551e5);
    f.call(model, 2, [0x0103969b, OUT, 0]);
    let rich = f.read(OUT);
    let n = f.text("AB");
    assert_eq!(f.call(rich, 6, [TEXT, n, 0]), 0);
    assert_eq!(f.call(w, 13, [rich, 0, 0]), 0);
    assert_eq!(f.call(w, 3, [0x801, 0x417, 0x0103bda6]), 1);
    assert_eq!(f.call(w, 12, [0x01015946, OUT, 0]), 0);
    f.call(f.read(OUT), 1, [0; 3]);
    assert_eq!(f.call(w, 12, [0x0101593b, OUT, 0]), 3);
    assert_eq!(f.call(w, 5, [OUT, 0, 0]), 0);
    assert!(f.read(OUT) > 0);
    f.call(model, 1, [0; 3]);
    f.call(rich, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
}
#[test]
fn text_layout_heap_oom_preserves_previous_cache_and_native_factory_zeroes_output() {
    let mut f = F::new();
    let l = f.create(0x0103bda6);
    let m = f.value("AB");
    assert_eq!(f.layout(l, m, 50, 30, 0), 0);
    f.call(l, 7, [OUT, OUT + 8, 0]);
    let before = (f.read(OUT), f.read(OUT + 4), f.read(OUT + 8));
    let slot = (0..256)
        .find(|&s| Interface::Helpers.method(s) == Some("malloc"))
        .unwrap();
    for n in [1024 * 1024, 65536, 4096, 256, 32, 1] {
        while f.run(aee::encode(Interface::Helpers, slot), [n, 0, 0, 0]) != 0 {}
    }
    assert_eq!(f.layout(l, m, 70, 40, 0), 2);
    assert_eq!(f.call(l, 7, [OUT, OUT + 8, 0]), 0);
    assert_eq!((f.read(OUT), f.read(OUT + 4), f.read(OUT + 8)), before);
    f.put(OUT, 0xdeadbeef);
    assert_eq!(
        f.run(
            aee::encode(Interface::Shell, 2),
            [loader::OBJECT_BASE, 0x0103bda6, OUT, 0]
        ),
        2
    );
    assert_eq!(f.read(OUT), 0);
    f.call(m, 1, [0; 3]);
    f.call(l, 1, [0; 3]);
}
#[test]
fn static_and_text_layout_do_not_accept_legacy_font_as_a_text_model() {
    let mut f = F::new();
    let w = f.create(0x010551e0);
    let legacy = f.create(0x01012786);
    assert_eq!(
        f.call(w, 13, [legacy, 0, 0]),
        3,
        "IFont legado nao e um modelo apesar de seu QI antigo permissivo"
    );
    let l = f.create(0x0103bda6);
    assert_eq!(f.layout(l, legacy, 30, 20, 0), 3);
    f.call(l, 1, [0; 3]);
    f.call(legacy, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
}

#[test]
fn review_ellipsis_getter_writes_only_one_aechar() {
    let mut f = F::new();
    let l = f.create(0x0103bda6);
    assert_eq!(f.call(l, 3, [0x801, 0x349, 0x2026]), 1);
    f.put(OUT, 0xaabbccdd);
    assert_eq!(f.call(l, 3, [0x800, 0x349, OUT]), 1);
    assert_eq!(
        f.read(OUT),
        0xaabb2026,
        "primary GetEllipsis accepts AECHAR*, not uint32 output"
    );
    f.call(l, 1, [0; 3]);
}
#[test]
fn review_failed_relayout_preserves_cached_geometry_model_and_font_references() {
    let mut f = F::new();
    let l = f.create(0x0103bda6);
    let font = f.layout_font(l);
    let m = f.value("AB");
    assert_eq!(f.layout(l, m, 40, 30, 0), 0);
    let c = f.canvas();
    f.call(l, 3, [0x801, 0x130, 0x00ff00ff]);
    f.call(l, 3, [0x801, 0x140, 0]);
    f.call(font, 0, [0; 3]);
    let font_refs = f.call(font, 1, [0; 3]);
    f.call(m, 0, [0; 3]);
    let model_refs = f.call(m, 1, [0; 3]);
    let malloc = (0..256)
        .find(|&s| Interface::Helpers.method(s) == Some("malloc"))
        .unwrap();
    let free = (0..256)
        .find(|&s| Interface::Helpers.method(s) == Some("free"))
        .unwrap();
    let mut live = Vec::new();
    let mut size = loader::HEAP_SIZE as u32;
    while size > 0 {
        let at = f.run(aee::encode(Interface::Helpers, malloc), [size, 0, 0, 0]);
        if at == 0 {
            size /= 2;
        } else {
            live.push(at);
        }
        assert!(live.len() < 256);
    }
    assert_eq!(f.layout(l, m, 60, 40, 0), 2);
    f.call(font, 0, [0; 3]);
    assert_eq!(f.call(font, 1, [0; 3]), font_refs);
    f.call(m, 0, [0; 3]);
    assert_eq!(f.call(m, 1, [0; 3]), model_refs);
    for at in live {
        f.run(aee::encode(Interface::Helpers, free), [at, 0, 0, 0]);
    }
    assert_eq!(f.call(l, 6, [c, 0, 0]), 0);
    assert_eq!(f.m.screen().get_pixel(39, 29), 0x07e0);
    assert_eq!(
        f.m.screen().get_pixel(40, 29),
        0,
        "failed relayout must keep prior40x30 draw extent"
    );
    f.call(m, 1, [0; 3]);
    f.call(font, 1, [0; 3]);
    f.call(l, 1, [0; 3]);
}
#[test]
fn review_textinfo_void_residual_does_not_reject_real_text_model() {
    let mut f = F::new();
    let l = f.create(0x0103bda6);
    let model = f.create(0x010551e5);
    let n = f.text("AB");
    assert_eq!(f.call(model, 5, [TEXT, n, 0]), 0);
    f.call(model, 7, [OUT, 0, 0]);
    let info = OUT + 2048;
    for i in 0..4 {
        let value = f.read(OUT + 4 * i);
        f.put(info + 4 * i, value);
    }
    let vt = f.read(model);
    let cloned = f.alloc(128);
    for i in 0..9 {
        let value = f.read(vt + 4 * i);
        f.put(cloned + 4 * i, value);
    }
    let code = cloned + 48;
    let ops = [
        0xe59f2024, 0xe5923000, 0xe5813000, 0xe5923004, 0xe5813004, 0xe5923008, 0xe5813008,
        0xe592300c, 0xe581300c, 0xe3a0004d, 0xe12fff1e, info,
    ];
    for (i, op) in ops.into_iter().enumerate() {
        f.put(code + 4 * i as u32, op);
    }
    f.put(cloned + 28, code);
    f.put(model, cloned);
    assert_eq!(
        f.layout(l, model, 40, 30, 0),
        0,
        "GetTextInfo is void; copied text is authoritative"
    );
    f.call(model, 1, [0; 3]);
    f.call(l, 1, [0; 3]);
}

struct Arm {
    words: Vec<u32>,
    literals: Vec<(usize, u32)>,
}
impl Arm {
    fn new() -> Self {
        Self {
            words: vec![],
            literals: vec![],
        }
    }
    fn op(&mut self, v: u32) {
        self.words.push(v);
    }
    fn lit(&mut self, r: u32, v: u32) {
        let at = self.words.len();
        self.words.push(0xe59f0000 | (r << 12));
        self.literals.push((at, v));
    }
    fn branch(&mut self, at: usize, target: usize, condition: u32) {
        self.words[at] =
            (condition << 28) | 0x0a000000 | ((target as i32 - at as i32 - 2) as u32 & 0xffffff);
    }
    fn install(mut self, f: &mut F) -> u32 {
        for (at, v) in self.literals {
            let li = self.words.len();
            self.words[at] |= (li * 4 - at * 4 - 8) as u32;
            self.words.push(v);
        }
        let p = f.alloc(self.words.len() as u32 * 4);
        for (i, v) in self.words.into_iter().enumerate() {
            f.put(p + 4 * i as u32, v);
        }
        p
    }
}
fn counter_fn(f: &mut F, add: bool) -> u32 {
    let mut a = Arm::new();
    a.op(0xe5901004);
    a.op(if add { 0xe2811001 } else { 0xe2411001 });
    a.op(0xe5801004);
    a.op(0xe1a00001);
    a.op(0xe12fff1e);
    a.install(f)
}
fn copy_info_fn(f: &mut F, data: u32, outreg: u32) -> u32 {
    let mut a = Arm::new();
    a.lit(3, data);
    for off in [0, 4, 8, 12] {
        a.op(0xe5930000 | off);
        a.op(0xe5800000 | (outreg << 16) | off);
    }
    a.op(0xe3a00000);
    a.op(0xe12fff1e);
    a.install(f)
}
fn reentrant_layout_snapshot(replace_font: bool) {
    let mut f = F::new();
    let layout = f.create(0x0103bda6);
    let old_map = f.create(0x010551cb);
    let replacement = f.create(0x010551cb);
    let normal = f.font();
    let named = f.font();
    assert_eq!(f.call(named, 6, [0x801, 0x1001, 1]), 1);
    let name = f.alloc(4);
    f.put(name, b'b' as u32);
    if !replace_font {
        assert_eq!(f.call(old_map, 8, [0, normal, 0]), 0);
        assert_eq!(f.call(old_map, 8, [name, named, 0]), 0);
    }
    assert_eq!(f.call(layout, 3, [0x801, 0x418, old_map]), 1);
    assert_eq!(f.call(old_map, 1, [0; 3]), 1);
    let model = f.alloc(16);
    let rich = f.alloc(16);
    let vt = f.alloc(48);
    let rich_vt = f.alloc(48);
    f.put(model, vt);
    f.put(model + 4, 1);
    f.put(rich, rich_vt);
    f.put(rich + 4, 1);
    let add = counter_fn(&mut f, true);
    let release = counter_fn(&mut f, false);
    for table in [vt, rich_vt] {
        f.put(table, add);
        f.put(table + 4, release);
    }
    let text = f.alloc(8);
    f.put(text, b'A' as u32 | ((b'B' as u32) << 16));
    let info = f.alloc(16);
    f.put(info, text);
    f.put(info + 4, 2);
    f.put(info + 8, 0);
    f.put(info + 12, 0);
    let get_info = copy_info_fn(&mut f, info, 1);
    f.put(vt + 28, get_info);
    let tag = f.alloc(16);
    f.put(tag, 0);
    f.put(tag + 4, 2);
    f.put(tag + 8, 1);
    f.put(tag + 12, name);
    let get_tag = copy_info_fn(&mut f, tag, 2);
    f.put(rich_vt + 36, get_tag);
    let mut one = Arm::new();
    one.op(0xe3a00001);
    one.op(0xe12fff1e);
    let count = one.install(&mut f);
    f.put(rich_vt + 32, count);
    let fresh_out = f.alloc(16);
    let mut qi = Arm::new();
    qi.op(0xe92d4070);
    qi.op(0xe1a04000);
    qi.op(0xe1a05002);
    qi.lit(6, 0x0103969b);
    qi.op(0xe1510006);
    let to_text = qi.words.len();
    qi.op(0);
    qi.lit(0, layout);
    qi.lit(1, 0x801);
    qi.lit(2, if replace_font { 0x152 } else { 0x418 });
    qi.lit(3, if replace_font { named } else { replacement });
    qi.lit(12, aee::encode(Interface::TextLayout, 3));
    qi.op(0xe12fff3c);
    qi.lit(0, loader::OBJECT_BASE);
    qi.lit(1, 0x010551cb);
    qi.lit(2, fresh_out);
    qi.op(0xe3a03000);
    qi.lit(12, aee::encode(Interface::Shell, 2));
    qi.op(0xe12fff3c);
    qi.lit(0, rich);
    let to_return = qi.words.len();
    qi.op(0);
    let text_label = qi.words.len();
    qi.op(0xe1a00004);
    let return_label = qi.words.len();
    qi.op(0xe5850000);
    qi.op(0xe5901004);
    qi.op(0xe2811001);
    qi.op(0xe5801004);
    qi.op(0xe3a00000);
    qi.op(0xe8bd8070);
    qi.branch(to_text, text_label, 1);
    qi.branch(to_return, return_label, 14);
    let query = qi.install(&mut f);
    f.put(vt + 8, query);
    f.put(OUT, 100);
    f.put(OUT + 4, 40);
    assert_eq!(f.call(layout, 5, [model, OUT, 0]), 0);
    assert_eq!(f.call(layout, 7, [OUT, OUT + 8, 0]), 0);
    let fresh = f.read(fresh_out);
    let width = f.read(OUT);
    eprintln!("old_map={old_map:#x} callback_new_map={fresh:#x} width={width}");
    assert_eq!(
        width,
        F::reference().width("AB", 16.0) + if replace_font { 0 } else { 2 },
        "mapa usado pela operacao precisa sobreviver a substituicao reentrante"
    );
    assert_ne!(
        fresh, old_map,
        "identidade do mapa em uso nao pode ser reciclada dentro da operacao"
    );
    f.call(fresh, 1, [0; 3]);
    f.call(layout, 1, [0; 3]);
    f.call(replacement, 1, [0; 3]);
    f.call(normal, 1, [0; 3]);
    f.call(named, 1, [0; 3]);
    assert_eq!(f.read(model + 4), 1);
    assert_eq!(f.read(rich + 4), 1);
}

#[test]
fn layout_holds_fontmap_snapshot_across_foreign_model_rich_qi_replacement() {
    reentrant_layout_snapshot(false);
}
#[test]
fn layout_missing_tag_keeps_captured_fallback_when_model_replaces_font() {
    reentrant_layout_snapshot(true);
}

#[test]
fn text_layout_empty_line_uses_resolved_fontmap_default_metrics() {
    let mut f = F::new();
    let layout = f.create(0x0103bda6);
    let map = f.create(0x010551cb);
    let mapped = f.font();
    assert_eq!(f.call(mapped, 6, [0x801, 0x1011, 20]), 1);
    assert_eq!(f.call(mapped, 5, [OUT, 4, 0]), 0);
    let info = f.read(OUT);
    let expected = (info as u16 as u32) + (info >> 16);
    assert_eq!(f.call(map, 8, [0, mapped, 0]), 0);
    assert_eq!(f.call(layout, 3, [0x801, 0x418, map]), 1);
    let model = f.value("");
    assert_eq!(f.call(layout, 4, [model, 0, OUT]), 0);
    assert_eq!(f.read(OUT + 4), expected);
    assert_eq!(f.layout(layout, model, 100, 100, 0), 0);
    assert_eq!(f.call(layout, 7, [OUT, OUT + 8, 0]), 0);
    assert_eq!(f.read(OUT + 4), expected);
    for object in [model, layout, map, mapped] {
        f.call(object, 1, [0; 3]);
    }
}
