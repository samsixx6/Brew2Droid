//! ViewportWidget: IDecorator e IContainer distintos, recorte e scroll reais.
use std::path::PathBuf;
use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend, Reg},
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    storage::StoragePaths,
};
struct F {
    m: Machine<BackendPadrao>,
    root: PathBuf,
    p: u32,
}
impl F {
    fn new() -> Self {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests/startup-repairs-wave21-20261031/wmapp")
            .join(format!(
                "viewport-test-{}-{}",
                std::process::id(),
                N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&root).unwrap();
        let s = StoragePaths::from_root(root.join("runtime"));
        s.create_dirs().unwrap();
        let im = ModImage::parse(
            [0xe3a00000u32, 0xe12fff1e]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect(),
        )
        .unwrap();
        let mut m = Machine::new_with_storage(
            BackendPadrao::new().unwrap(),
            loader::load_with(&im, &[]).unwrap(),
            &root,
            &s,
            None,
        );
        assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
        Self {
            m,
            root,
            p: loader::HEAP_BASE + 0x6000,
        }
    }
    fn wr(&mut self, p: u32, v: u32) {
        self.m.cpu_mut().write_u32(p, v).unwrap();
    }
    fn rd(&self, p: u32) -> u32 {
        self.m.cpu().read_u32(p).unwrap()
    }
    fn entry(&mut self, e: u32, a: [u32; 4]) -> u32 {
        match self.m.call_guest(e, a, 200000).unwrap() {
            Outcome::Returned { code } => code,
            x => panic!("{e:#x}: {x:?}"),
        }
    }
    fn call(&mut self, o: u32, s: u32, a: [u32; 3]) -> u32 {
        let e = self.rd(self.rd(o) + s * 4);
        self.entry(e, [o, a[0], a[1], a[2]])
    }
    fn api(&mut self, i: Interface, n: &str, a: [u32; 4]) -> u32 {
        let s = (0..256).find(|s| i.method(*s) == Some(n)).unwrap();
        self.entry(aee::encode(i, s), a)
    }
    fn create(&mut self, c: u32) -> u32 {
        let p = self.p;
        assert_eq!(
            self.api(
                Interface::Shell,
                "CreateInstance",
                [loader::OBJECT_BASE, c, p, 0]
            ),
            0,
            "a classe documentada deve produzir sua interface pública"
        );
        let o = self.rd(p);
        assert_ne!(o, 0);
        o
    }

    fn handler(&mut self, w: u32, ctx: u32) {
        let code = loader::MODULE_BASE + 0x200;
        let words = [
            0xe92d4010u32,
            0xe1a04000,
            0xe5801000,
            0xe5802004,
            0xe5803008,
            0xe590c00c,
            0xe28cc001,
            0xe580c00c,
            0xe3510c01,
            0x03a00001,
            0x08bd8010,
            0xe594c010,
            0xe5940014,
            0xe12fff3c,
            0xe8bd8010,
        ];
        self.m
            .cpu_mut()
            .write_mem(
                code,
                &words
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        let p = self.p + 80;
        self.wr(p, code);
        self.wr(p + 4, ctx);
        self.wr(p + 8, 0);
        assert_eq!(self.call(w, 4, [p, 0, 0]), 0);
        self.wr(ctx + 16, self.rd(p));
        self.wr(ctx + 20, self.rd(p + 4));
    }
    fn live(&self, o: u32) -> bool {
        let s = self.m.grava_estado();
        zeebx::save_state::Leitor::abre(&s)
            .unwrap()
            .pares("objects.kinds")
            .unwrap()
            .iter()
            .any(|(p, _)| *p == o)
    }
}
impl Drop for F {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl F {
    fn size(&mut self, w: u32, x: u32, y: u32) {
        let p = self.p;
        self.wr(p, x);
        self.wr(p + 4, y);
        self.call(w, 7, [p, 0, 0]);
    }
    fn prop(&mut self, w: u32, k: u32) -> u32 {
        let p = self.p;
        assert_eq!(self.call(w, 3, [0x800, k, p]), 1);
        self.rd(p)
    }
    fn set(&mut self, w: u32, k: u32, v: u32) {
        assert_eq!(self.call(w, 3, [0x801, k, v]), 1);
    }
    fn image_child(&mut self, width: u32, height: u32) -> u32 {
        let stride = (width * 3 + 3) & !3;
        let mut bmp = vec![0u8; (54 + stride * height) as usize];
        bmp[..2].copy_from_slice(b"BM");
        let len = bmp.len() as u32;
        for (at, v) in [(2, len), (10, 54), (14, 40), (18, width), (22, height)] {
            bmp[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
        bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
        for y in 0..height {
            for x in 0..width {
                let p = (54 + (height - 1 - y) * stride + x * 3) as usize;
                bmp[p..p + 3].copy_from_slice(&[0, (y * 16) as u8, (x * 16) as u8]);
            }
        }
        std::fs::write(self.root.join("viewport.bmp"), bmp).unwrap();
        let p = self.p;
        self.m.cpu_mut().write_mem(p, b"viewport.bmp\0").unwrap();
        let image = self.api(
            Interface::Shell,
            "LoadResObject",
            [loader::OBJECT_BASE, p, 0, 0],
        );
        assert_ne!(image, 0);
        let child = self.create(0x01054a8f);
        assert_eq!(self.call(child, 12, [0x0101593c, p, 0]), 0);
        let model = self.rd(p);
        self.call(model, 5, [image, 0x01013110, 0]);
        self.call(model, 1, [0, 0, 0]);
        self.call(image, 1, [0, 0, 0]);
        self.size(child, width, height);
        child
    }
    fn canvas(&mut self) -> u32 {
        let d = self.create(0x01001001);
        let p = self.p;
        self.call(d, 16, [p, 0, 0]);
        let b = self.rd(p);
        self.call(b, 2, [0x0101e443, p, 0]);
        self.rd(p)
    }
}
#[test]
fn decorator_and_container_have_distinct_typed_facets_and_shared_lifetime() {
    let mut f = F::new();
    let v = f.create(0x010551eb);
    let p = f.p;
    for iid in [0x01015934, 0x01015952, 0x01015956] {
        assert_eq!(f.call(v, 2, [iid, p, 0]), 0);
        assert_eq!(f.rd(p), v);
        f.call(v, 1, [0, 0, 0]);
    }
    assert_eq!(f.call(v, 2, [0x01015932, p, 0]), 0);
    let c = f.rd(p);
    assert_ne!(c, v, "IContainer::Invalidate não é HandleEvent");
    assert_eq!(f.call(c, 2, [0x01015952, p, 0]), 0);
    assert_eq!(f.rd(p), v);
    assert_eq!(f.call(v, 1, [0, 0, 0]), 2);
    assert_eq!(f.call(v, 2, [0xdeadbeef, p, 0]), 3);
    assert_eq!(f.rd(p), 0);
    assert_eq!(f.call(v, 1, [0, 0, 0]), 1);
    assert_eq!(f.call(c, 1, [0, 0, 0]), 0);
    assert!(!f.live(v));
    assert!(!f.live(c));
}
#[test]
fn decorator_retains_one_child_sets_its_real_parent_and_releases_on_detach() {
    let mut f = F::new();
    let v = f.create(0x010551eb);
    let child = f.create(0x01054a8f);
    let p = f.p;
    f.call(v, 14, [child, 0, 0]);
    assert_eq!(f.call(child, 1, [0, 0, 0]), 1);
    f.call(child, 8, [p, 0, 0]);
    let parent = f.rd(p);
    assert_ne!(parent, v);
    assert_ne!(parent, 0);
    f.call(parent, 1, [0, 0, 0]);
    f.call(v, 15, [p, 0, 0]);
    assert_eq!(f.rd(p), child);
    assert_eq!(f.call(child, 1, [0, 0, 0]), 1);
    f.call(v, 14, [0, 0, 0]);
    assert!(!f.live(child));
    f.call(v, 1, [0, 0, 0]);
}
#[test]
fn offsets_render_actual_child_pixels_inside_viewport_and_survive_restore() {
    let mut f = F::new();
    let v = f.create(0x010551eb);
    let child = f.image_child(12, 10);
    f.size(v, 8, 6);
    f.call(v, 14, [child, 0, 0]);
    f.set(v, 0x154, 3);
    f.set(v, 0x155, 4);
    let canvas = f.canvas();
    let s = f.m.grava_estado();
    f.m.restaura_estado(&s).unwrap();
    f.call(v, 10, [canvas, 7, 9]);
    for y in 0..20 {
        for x in 0..20 {
            let expected = if (7..15).contains(&x) && (9..15).contains(&y) {
                let r = ((x - 7 + 3) * 16) as u16;
                let g = ((y - 9 + 4) * 16) as u16;
                ((r & 0xf8) << 8) | ((g & 0xfc) << 3)
            } else {
                0
            };
            assert_eq!(f.m.screen().get_pixel(x, y), expected, "pixel({x},{y})");
        }
    }
}
#[test]
fn offsets_clamp_to_content_and_real_direction_keys_use_increment() {
    let mut f = F::new();
    let v = f.create(0x010551eb);
    let child = f.create(0x01054a8f);
    f.size(child, 20, 18);
    f.size(v, 8, 6);
    f.call(v, 14, [child, 0, 0]);
    f.set(v, 0x154, 999);
    f.set(v, 0x155, 999);
    assert_eq!(f.prop(v, 0x154), 12);
    assert_eq!(f.prop(v, 0x155), 12);
    f.set(v, 0x156, 3);
    assert_eq!(f.call(v, 3, [0x100, 0xe033, 0]), 1);
    assert_eq!(f.prop(v, 0x154), 9);
    assert_eq!(f.call(v, 3, [0x100, 0xe031, 0]), 1);
    assert_eq!(f.prop(v, 0x155), 9);
    f.size(child, 4, 3);
    assert_eq!(f.prop(v, 0x154), 0);
    assert_eq!(f.prop(v, 0x155), 0);
}
#[test]
fn scroll_listener_receives_primary_twenty_byte_event_and_can_cancel() {
    let mut f = F::new();
    let v = f.create(0x010551eb);
    let child = f.create(0x01054a8f);
    f.size(child, 20, 18);
    f.size(v, 8, 6);
    f.call(v, 14, [child, 0, 0]);
    let p = f.p;
    let model = f.prop(v, 0x161);
    let l = p + 192;
    let ctx = p + 256;
    let cb = loader::MODULE_BASE + 0x380;
    let words = [
        0xe5912000u32,
        0xe5802000,
        0xe5912004,
        0xe5802004,
        0xe591200c,
        0xe5802008,
        0xe5912010,
        0xe580200c,
        0xe12fff1e,
    ];
    f.m.cpu_mut()
        .write_mem(
            cb,
            &words
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    f.wr(l + 8, cb);
    f.wr(l + 12, ctx);
    assert_eq!(f.call(model, 3, [l, 0, 0]), 0);
    f.set(v, 0x154, 3);
    f.set(v, 0x155, 4);
    f.set(v, 0x183, 0);
    assert_eq!(f.rd(ctx), 0x1064);
    assert_eq!(f.rd(ctx + 4), model);
    assert_eq!(f.rd(ctx + 8), 1 | (18 << 16));
    assert_eq!(f.rd(ctx + 12), 6 | (4 << 16));
    let state = f.m.grava_estado();
    f.m.restaura_estado(&state).unwrap();
    let cancel = f.rd(l + 16);
    f.entry(cancel, [l, 0, 0, 0]);
    f.wr(ctx, 0xdeadbeef);
    f.set(v, 0x155, 5);
    assert_eq!(f.rd(ctx), 0xdeadbeef);
    f.call(model, 1, [0, 0, 0]);
}
#[test]
fn disabled_key_handling_forwards_to_decorated_child_and_focus_is_real() {
    let mut f = F::new();
    let v = f.create(0x010551eb);
    let child = f.create(0x010551e0);
    f.size(child, 20, 18);
    f.size(v, 8, 6);
    f.call(v, 14, [child, 0, 0]);
    let ctx = f.p + 128;
    f.handler(child, ctx);
    f.set(v, 0x153, 2);
    assert_eq!(f.call(v, 3, [0x100, 0xe034, 0]), 1);
    assert_eq!(f.rd(ctx), 0x100);
    assert_eq!(f.prop(v, 0x154), 0);
    assert_eq!(f.call(v, 3, [0x700, 1, 0]), 1);
    assert_eq!(f.rd(ctx), 0x700);
    let p = f.p;
    assert_eq!(f.call(v, 3, [0x701, 0, p]), 1);
    assert_eq!(f.rd(p) & 255, 1);
}
#[test]
fn attach_rejects_cycles_and_child_already_parented_elsewhere() {
    let mut f = F::new();
    let a = f.create(0x010551eb);
    let b = f.create(0x010551eb);
    let p = f.p;
    f.call(a, 14, [b, 0, 0]);
    f.call(b, 14, [a, 0, 0]);
    f.call(b, 15, [p, 0, 0]);
    assert_eq!(f.rd(p), 0);
    let child = f.create(0x01054a8f);
    f.call(b, 14, [child, 0, 0]);
    f.call(a, 14, [child, 0, 0]);
    f.call(a, 15, [p, 0, 0]);
    assert_eq!(f.rd(p), b, "falha não pode descartar o filho anterior");
}
#[test]
fn size_to_fit_resizes_small_child_and_container_locate_reports_offsets() {
    let mut f = F::new();
    let v = f.create(0x010551eb);
    let child = f.create(0x01054a8f);
    f.size(child, 4, 3);
    f.size(v, 8, 6);
    f.set(v, 0x153, 1);
    f.call(v, 14, [child, 0, 0]);
    let p = f.p;
    f.call(child, 6, [p, 0, 0]);
    assert_eq!((f.rd(p), f.rd(p + 4)), (8, 6));
    f.call(v, 2, [0x01015932, p, 0]);
    let c = f.rd(p);
    let sp = f.m.cpu().read_reg(Reg::Sp);
    f.wr(sp, p + 32);
    assert_eq!(f.call(c, 4, [child, p + 16, p + 32]), 0);
    assert_eq!(f.rd(p + 16), c);
    let mut r = [0; 8];
    f.m.cpu().read_mem(p + 32, &mut r).unwrap();
    assert_eq!(r, [0, 0, 0, 0, 8, 0, 6, 0]);
    f.call(c, 1, [0, 0, 0]);
}
#[test]
fn viewport_view_model_is_only_iqi_imodel_and_never_a_widget_or_interface_model() {
    let mut f = F::new();
    let v = f.create(0x010551eb);
    let p = f.p;
    let model = f.prop(v, 0x161);
    assert_eq!(f.call(model, 2, [0x0101593a, p, 0]), 0);
    assert_eq!(f.rd(p), model);
    f.call(model, 1, [0, 0, 0]);
    for iid in [0x0101593c, 0x01015952, 0x0101593b, 0xdeadbeef] {
        f.wr(p, 77);
        assert_eq!(f.call(model, 2, [iid, p, 0]), 3);
        assert_eq!(f.rd(p), 0);
    }
    assert_eq!(
        f.call(v, 3, [0x801, 0x161, model]),
        0,
        "o ViewModel não se substitui por esta propriedade"
    );
    f.call(model, 1, [0, 0, 0]);
    f.call(v, 1, [0, 0, 0]);
}

#[test]
fn scroll_request_notifies_when_guest_heap_is_exhausted() {
    let mut f = F::new();
    let reserved = f.api(Interface::Helpers, "malloc", [0x10000, 0, 0, 0]);
    assert!(reserved <= f.p && reserved + 0x10000 > f.p + 1024);
    let v = f.create(0x010551eb);
    let model = f.prop(v, 0x161);
    let p = f.p;
    let listener = p + 192;
    let context = p + 256;
    let cb = loader::MODULE_BASE + 0x380;
    let words = [0xe5912000u32, 0xe5802000, 0xe12fff1e];
    f.m.cpu_mut()
        .write_mem(
            cb,
            &words
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    f.wr(listener + 8, cb);
    f.wr(listener + 12, context);
    assert_eq!(f.call(model, 3, [listener, 0, 0]), 0);
    let mut size = loader::HEAP_SIZE as u32;
    let mut blocks = Vec::new();
    while size > 0 {
        let block = f.api(Interface::Helpers, "malloc", [size | 0x80000000, 0, 0, 0]);
        if block == 0 {
            size /= 2;
        } else {
            blocks.push(block);
        }
        assert!(blocks.len() < 256);
    }
    assert_eq!(f.call(v, 3, [0x801, 0x183, 0]), 1);
    let observed = f.rd(context);
    for block in blocks {
        f.api(Interface::Helpers, "free", [block, 0, 0, 0]);
    }
    assert_eq!(
        observed, 0x1064,
        "synchronous scalar notification must not depend on guest heap"
    );
}
