//! CTextModel a partir dos contratos públicos e das UTs do SDK.
include!("common/modern_ui_fixture.rs");
impl F {
    fn tm(&mut self) -> u32 {
        self.create(0x010551e5)
    }
    fn tm_qi(&mut self, p: u32, id: u32) -> u32 {
        assert_eq!(self.call(p, 2, [id, OUT, 0]), 0);
        self.read(OUT)
    }
    fn tm_info(&mut self, p: u32) -> [u32; 4] {
        assert_eq!(self.call(p, 7, [OUT, 0, 0]), p, "GetTextInfo is void");
        std::array::from_fn(|i| self.read(OUT + i as u32 * 4))
    }
    fn tm_string(&self, p: u32, n: u32) -> Vec<u16> {
        let mut b = vec![0; n as usize * 2];
        self.m.cpu().read_mem(p, &mut b).unwrap();
        b.chunks_exact(2)
            .map(|x| u16::from_le_bytes([x[0], x[1]]))
            .collect()
    }
    fn tm_text(&mut self, p: u32) -> String {
        let i = self.tm_info(p);
        String::from_utf16(&self.tm_string(i[0], i[1])).unwrap()
    }
    fn tm_set(&mut self, p: u32, s: &str) {
        self.call(p, 8, [0, u32::MAX, 0]);
        let n = self.text(s);
        assert_eq!(self.call(p, 5, [TEXT, n, 0]), 0);
    }
}
#[test]
fn text_factory_has_typed_inherited_text_and_distinct_rich_lifetimes() {
    let mut f = F::new();
    let p = f.tm();
    let t = f.tm_qi(p, 0x01015946);
    assert_eq!(t, p);
    let r = f.tm_qi(p, 0x0103969b);
    assert_ne!(r, p);
    assert_eq!(f.call(p, 2, [0x0101593b, OUT, 0]), 3);
    assert_eq!(f.read(OUT), 0);
    f.call(p, 1, [0; 3]);
    f.call(t, 1, [0; 3]);
    let t = f.tm_qi(r, 0x01015946);
    f.tm_set(t, "alive");
    assert_eq!(f.tm_text(t), "alive");
    f.call(t, 1, [0; 3]);
    assert_eq!(f.call(r, 1, [0; 3]), 0);
    assert_eq!(f.call(p, 0, [0; 3]), 14);
}
#[test]
fn text_selection_replacement_utf16_copy_and_primary_end_bounds() {
    let mut f = F::new();
    let p = f.tm();
    f.tm_set(p, "A한😀BC");
    f.m.cpu_mut().write_mem(TEXT, &[0; 20]).unwrap();
    assert_eq!(f.tm_text(p), "A한😀BC");
    assert_eq!(f.call(p, 8, [5, 1, 0]), p);
    let i = f.tm_info(p);
    assert_eq!(&i[2..], &[5, 1]);
    let n = f.text("xy");
    assert_eq!(f.call(p, 5, [TEXT, n, 0]), 0);
    assert_eq!(f.tm_text(p), "AxyC");
    assert_eq!(&f.tm_info(p)[2..], &[3, 3]);
    f.call(p, 8, [u32::MAX, u32::MAX - 1, 0]);
    assert_eq!(&f.tm_info(p)[2..], &[4, 4]);
    let n = f.text("!");
    assert_eq!(f.call(p, 5, [TEXT, n, 0]), 0);
    assert_eq!(f.tm_text(p), "AxyC!");
    f.call(p, 8, [999, 999, 0]);
    assert_eq!(&f.tm_info(p)[2..], &[5, 5]);
    f.call(p, 8, [0, u32::MAX, 0]);
    assert_eq!(f.call(p, 5, [0, 0, 0]), 0);
    assert_eq!(f.tm_text(p), "");
    f.call(p, 1, [0; 3]);
}
#[test]
fn selected_text_returns_full_length_but_honors_capacity_and_nul() {
    let mut f = F::new();
    let p = f.tm();
    f.tm_set(p, "abcdef");
    f.call(p, 8, [5, 2, 0]);
    let buffer = OUT + 2048;
    f.put(buffer, 0xaaaa5555);
    assert_eq!(f.call(p, 6, [buffer, 0, 0]), 3);
    assert_eq!(f.read(buffer), 0xaaaa5555);
    assert_eq!(f.call(p, 6, [buffer, u32::MAX, 0]), 3);
    assert_eq!(f.read(buffer), 0xaaaa5555);
    assert_eq!(f.call(p, 6, [buffer, 2, 0]), 3);
    assert_eq!(f.tm_string(buffer, 2), vec![b'c' as u16, 0]);
    assert_eq!(f.call(p, 6, [buffer, 1, 0]), 3);
    assert_eq!(f.tm_string(buffer, 1), vec![0]);
    assert_eq!(f.call(p, 6, [buffer, 4, 0]), 3);
    assert_eq!(f.tm_string(buffer, 4), vec![99, 100, 101, 0]);
    f.call(p, 1, [0; 3]);
}
#[test]
fn text_events_include_primary_codes_and_minimum_selection_parameter() {
    let mut f = F::new();
    let p = f.tm();
    f.tm_set(p, "abcdef");
    let listener = OUT + 1024;
    let context = OUT + 2048;
    let callback = loader::MODULE_BASE + 0x500;
    // Copy ModelEvent to caller storage and increment a count.
    let code = [
        0xe5912000u32,
        0xe5802000,
        0xe5912004,
        0xe5802004,
        0xe5912008,
        0xe5802008,
        0xe590200c,
        0xe2822001,
        0xe580200c,
        0xe12fff1e,
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
    for at in (0..24).step_by(4) {
        f.put(listener + at, 0);
    }
    f.put(listener + 8, callback);
    f.put(listener + 12, context);
    assert_eq!(f.call(p, 3, [listener, 0, 0]), 0);
    assert_eq!(f.call(p, 8, [5, 2, 0]), p);
    assert_eq!(
        [f.read(context), f.read(context + 4), f.read(context + 8)],
        [0x1003, p, 2]
    );
    let n = f.text("X");
    assert_eq!(f.call(p, 5, [TEXT, n, 0]), 0);
    assert_eq!(
        [f.read(context), f.read(context + 4), f.read(context + 8)],
        [0x1002, p, 2]
    );
    let cancel = f.read(listener + 16);
    f.run(cancel, [listener, 0, 0, 0]);
    let count = f.read(context + 12);
    f.call(p, 8, [0, 0, 0]);
    assert_eq!(f.read(context + 12), count);
    f.call(p, 1, [0; 3]);
}
#[test]
fn rich_markup_and_copied_tags_share_real_text_and_survive_snapshot() {
    let mut f = F::new();
    let p = f.tm();
    let r = f.tm_qi(p, 0x0103969b);
    let n = f.text("A[b]한[][[Z]");
    assert_eq!(f.call(r, 6, [TEXT, n, 0]), 0);
    assert_eq!(f.tm_text(p), "A한[Z]");
    assert_eq!(f.call(r, 8, [0; 3]), 1);
    assert_eq!(f.call(r, 9, [0, OUT, 0]), 0);
    assert_eq!([f.read(OUT), f.read(OUT + 4), f.read(OUT + 8)], [1, 1, 1]);
    assert_eq!(f.tm_string(f.read(OUT + 12), 1), vec![98]);
    f.put(OUT, 0);
    assert_eq!(f.call(r, 5, [0, OUT, 0]), 0);
    let size = f.read(OUT);
    assert!(size >= 2);
    f.put(OUT, size);
    assert_eq!(f.call(r, 5, [OUT + 2048, OUT, 0]), 0);
    let markup = String::from_utf16(&f.tm_string(OUT + 2048, size / 2 - 1)).unwrap();
    assert_eq!(markup, "A[b]한[][[Z]");
    let saved = f.m.grava_estado();
    let n = f.text("[a][b]x[][]");
    assert_eq!(f.call(r, 6, [TEXT, n, 0]), 20);
    assert_eq!(f.tm_text(p), "A한[Z]");
    assert_eq!(f.call(r, 11, [0; 3]), r);
    assert_eq!(f.call(r, 8, [0; 3]), 0);
    f.m.restaura_estado(&saved).unwrap();
    assert_eq!(f.call(r, 8, [0; 3]), 1);
    assert_eq!(f.tm_text(p), "A한[Z]");
    f.call(r, 1, [0; 3]);
    f.call(p, 1, [0; 3]);
}
#[test]
fn text_replace_aliases_borrowed_storage_safely_and_rejects_invalid_inputs_atomically() {
    let mut f = F::new();
    let p = f.tm();
    f.tm_set(p, "abcd");
    let i = f.tm_info(p);
    f.call(p, 8, [4, 4, 0]);
    assert_eq!(f.call(p, 5, [i[0], 2, 0]), 0);
    assert_eq!(f.tm_text(p), "abcdab");
    assert_eq!(f.call(p, 5, [0xdead0000, 2, 0]), 14);
    assert_eq!(f.tm_text(p), "abcdab");
    assert_eq!(f.call(p, 5, [0, 1, 0]), 14);
    assert_eq!(f.call(p, 6, [0, 1, 0]), 14);
    f.call(p, 1, [0; 3]);
}
#[test]
fn text_model_factory_oom_rolls_back_two_native_identities() {
    let mut f = F::new();
    let p = f.tm();
    f.call(p, 1, [0; 3]);
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
    let p = live.pop().unwrap();
    f.call(p, 1, [0; 3]);
    assert_eq!(
        f.run(
            aee::encode(Interface::Shell, 2),
            [loader::OBJECT_BASE, 0x010551e5, OUT, 0]
        ),
        2
    );
    assert_eq!(f.read(OUT), 0);
    live.push(f.create(0x01001001));
    for p in live {
        f.call(p, 1, [0; 3]);
    }
    assert_eq!(
        f.run(
            aee::encode(Interface::Shell, 2),
            [loader::OBJECT_BASE, 0x010551e5, 0, 0]
        ),
        14
    );
}

#[test]
fn rich_tags_copy_names_validate_overlap_and_support_delete_at() {
    let mut f = F::new();
    let p = f.tm();
    let r = f.tm_qi(p, 0x0103969b);
    f.tm_set(p, "hello");
    let name = TEXT;
    let n = f.text("bold");
    for (at, v) in [(0, 1u32), (4, 2), (8, n), (12, name)] {
        f.put(OUT + 1024 + at, v);
    }
    assert_eq!(f.call(r, 7, [OUT + 1024, 0, 0]), 0);
    f.m.cpu_mut().write_mem(name, &[0; 16]).unwrap();
    assert_eq!(f.call(r, 9, [0, OUT, 0]), 0);
    assert_eq!(
        f.tm_string(f.read(OUT + 12), 4),
        "bold".encode_utf16().collect::<Vec<_>>()
    );
    let n = f.text("other");
    for (at, v) in [(0, 2u32), (4, 2), (8, n), (12, name)] {
        f.put(OUT + 1024 + at, v);
    }
    assert_eq!(f.call(r, 7, [OUT + 1024, 0, 0]), 14);
    f.put(OUT + 1024, 4);
    f.put(OUT + 1028, 1);
    f.put(OUT + 1032, u32::MAX);
    assert_eq!(
        f.call(r, 7, [OUT + 1024, 0, 0]),
        14,
        "Tag names have explicit positive lengths"
    );
    assert_eq!(f.call(r, 8, [0; 3]), 1);
    assert_eq!(f.call(r, 10, [1, 0, 0]), 14);
    assert_eq!(f.call(r, 10, [0, 0, 0]), 0);
    assert_eq!(f.call(r, 8, [0; 3]), 0);
    assert_eq!(f.tm_text(p), "hello");
    f.call(r, 1, [0; 3]);
    f.call(p, 1, [0; 3]);
}
#[test]
fn text_growth_oom_is_atomic_but_selection_and_deletion_still_notify() {
    let mut f = F::new();
    let p = f.tm();
    f.tm_set(p, "A");
    f.call(p, 8, [u32::MAX, u32::MAX, 0]);
    let n = f.text("01234567890123456789012345678901");
    let listener = OUT + 1024;
    let context = OUT + 2048;
    let callback = loader::MODULE_BASE + 0x500;
    let code = [0xe5902000u32, 0xe2822001, 0xe5802000, 0xe12fff1e];
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
    assert_eq!(f.call(p, 3, [listener, 0, 0]), 0);
    let malloc = (0..256)
        .find(|&s| Interface::Helpers.method(s) == Some("malloc"))
        .unwrap();
    let free = (0..256)
        .find(|&s| Interface::Helpers.method(s) == Some("free"))
        .unwrap();
    let mut allocations = Vec::new();
    let mut size = loader::HEAP_SIZE as u32;
    while size > 0 {
        let at = f.run(aee::encode(Interface::Helpers, malloc), [size, 0, 0, 0]);
        if at == 0 {
            size /= 2;
        } else {
            allocations.push(at);
        }
        assert!(allocations.len() < 256);
    }
    assert_eq!(f.call(p, 5, [TEXT, n, 0]), 2);
    assert_eq!(f.tm_text(p), "A");
    assert_eq!(&f.tm_info(p)[2..], &[1, 1]);
    assert_eq!(f.read(context), 0);
    assert_eq!(f.call(p, 8, [0, u32::MAX, 0]), p);
    assert_eq!(f.call(p, 5, [0, 0, 0]), 0);
    assert_eq!(f.tm_text(p), "");
    assert_eq!(
        f.read(context),
        2,
        "required events must not allocate from exhausted heap"
    );
    for at in allocations {
        f.run(aee::encode(Interface::Helpers, free), [at, 0, 0, 0]);
    }
    f.call(p, 1, [0; 3]);
}

fn text_listener(f: &mut F, p: u32, ops: &[u32], words: &[u32]) -> (u32, u32) {
    let l = OUT + 1024;
    let ctx = OUT + 2048;
    let code = loader::MODULE_BASE + 0x600;
    for offset in (0..24).step_by(4) {
        f.put(l + offset, 0);
    }
    for (i, op) in ops.iter().copied().enumerate() {
        f.put(code + 4 * i as u32, op);
    }
    for (i, value) in words.iter().copied().enumerate() {
        f.put(ctx + 4 * i as u32, value);
    }
    f.put(l + 8, code);
    f.put(l + 12, ctx);
    assert_eq!(f.call(p, 3, [l, 0, 0]), 0);
    (l, ctx)
}
#[test]
fn text_callback_can_release_last_external_model_reference_safely() {
    let mut f = F::new();
    let p = f.tm();
    f.tm_set(p, "abc");
    let release = f.read(f.read(p) + 4);
    let (listener, ctx) = text_listener(
        &mut f,
        p,
        &[
            0xe92d4010, 0xe1a04000, 0xe5940000, 0xe5943004, 0xe12fff33, 0xe3a02001, 0xe5842008,
            0xe8bd8010,
        ],
        &[p, release, 0],
    );
    f.call(p, 8, [0, 1, 0]);
    assert_eq!(f.read(ctx + 8), 1);
    assert_eq!(f.read(listener + 16), 0);
    assert_eq!(
        f.call(p, 0, [0; 3]),
        14,
        "outer call pins model only through the live callback"
    );
}
#[test]
fn text_reentrant_replace_sees_committed_selection_and_survives_restore() {
    let mut f = F::new();
    let p = f.tm();
    f.tm_set(p, "A");
    f.text("B");
    let replace = f.read(f.read(p) + 20);
    let (_, ctx) = text_listener(
        &mut f,
        p,
        &[
            0xe92d4010, 0xe1a04000, 0xe5942010, 0xe2822001, 0xe5842010, 0xe5942008, 0xe3520000,
            0x1a000005, 0xe3a02001, 0xe5842008, 0xe5940000, 0xe594100c, 0xe5943004, 0xe12fff33,
            0xe8bd8010,
        ],
        &[p, replace, 0, TEXT, 0],
    );
    f.call(p, 8, [0, 1, 0]);
    assert_eq!(f.read(ctx + 8), 1);
    assert_eq!(
        f.read(ctx + 16),
        2,
        "outer selection callback and nested text callback both execute"
    );
    assert_eq!(f.tm_text(p), "B");
    let saved = f.m.grava_estado();
    f.m.restaura_estado(&saved).unwrap();
    assert_eq!(f.tm_text(p), "B");
    f.call(p, 1, [0; 3]);
}
#[test]
fn rich_markup_second_allocation_oom_rolls_back_text_and_frees_staged_buffer() {
    let mut f = F::new();
    let p = f.tm();
    let r = f.tm_qi(p, 0x0103969b);
    f.tm_set(p, "A");
    let markup = format!("[{}]0123456789[]", "n".repeat(60));
    let n = f.text(&markup);
    let hole = f.alloc(63);
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
    f.run(aee::encode(Interface::Helpers, free), [hole, 0, 0, 0]);
    assert_eq!(
        f.call(r, 6, [TEXT, n, 0]),
        2,
        "text staging fits the64byte hole; tag block does not"
    );
    assert_eq!(f.tm_text(p), "A");
    assert_eq!(&f.tm_info(p)[2..], &[1, 1]);
    assert_eq!(f.call(r, 8, [0; 3]), 0);
    let available = f.run(aee::encode(Interface::Helpers, malloc), [63, 0, 0, 0]);
    assert_eq!(
        available, hole,
        "second-allocation failure returns temporary text allocation"
    );
    f.run(aee::encode(Interface::Helpers, free), [available, 0, 0, 0]);
    for at in live {
        f.run(aee::encode(Interface::Helpers, free), [at, 0, 0, 0]);
    }
    f.call(r, 1, [0; 3]);
    f.call(p, 1, [0; 3]);
}
