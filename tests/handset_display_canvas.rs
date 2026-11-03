//! CDisplayCanvas: ABI público, bitmap real e clip do IDisplay envolvido.
use std::path::PathBuf;
use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend, Reg},
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    storage::StoragePaths,
};
const OUT: u32 = loader::HEAP_BASE + 0x10000;
struct F {
    m: Machine<BackendPadrao>,
    root: PathBuf,
}
impl F {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests/wave21-delivery-notes/display-canvas/runtime")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&root).unwrap();
        let storage = StoragePaths::from_root(root.join("storage"));
        storage.create_dirs().unwrap();
        let image = ModImage::parse(
            [0xe3a00000u32, 0xe12fff1e]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect(),
        )
        .unwrap();
        let mut m = Machine::new_with_storage(
            BackendPadrao::new().unwrap(),
            loader::load_with(&image, &[]).unwrap(),
            &root,
            &storage,
            None,
        );
        assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
        let mut f = Self { m, root };
        let slot = (0..256)
            .find(|&s| Interface::Helpers.method(s) == Some("malloc"))
            .unwrap();
        let at = f.run(aee::encode(Interface::Helpers, slot), [0x20000, 0, 0, 0]);
        assert!(at <= OUT && at + 0x20000 > OUT + 4096);
        f
    }
    fn run(&mut self, entry: u32, args: [u32; 4]) -> u32 {
        match self.m.call_guest(entry, args, 500000).unwrap() {
            Outcome::Returned { code } => code,
            other => panic!("{entry:#x}:{other:?}"),
        }
    }
    fn read(&self, p: u32) -> u32 {
        self.m.cpu().read_u32(p).unwrap()
    }
    fn put(&mut self, p: u32, v: u32) {
        self.m.cpu_mut().write_u32(p, v).unwrap();
    }
    fn call(&mut self, p: u32, slot: u32, a: [u32; 3]) -> u32 {
        let vt = self.read(p);
        let entry = self.read(vt + slot * 4);
        self.run(entry, [p, a[0], a[1], a[2]])
    }
    fn create(&mut self, id: u32) -> u32 {
        self.put(OUT, u32::MAX);
        assert_eq!(
            self.run(
                aee::encode(Interface::Shell, 2),
                [loader::OBJECT_BASE, id, OUT, 0]
            ),
            0,
            "real factory"
        );
        self.read(OUT)
    }
    fn canvas(&mut self) -> u32 {
        self.create(0x010551c8)
    }
    fn rect(&mut self, r: [i16; 4]) {
        self.m
            .cpu_mut()
            .write_mem(
                OUT,
                &r.into_iter().flat_map(i16::to_le_bytes).collect::<Vec<_>>(),
            )
            .unwrap();
    }
    fn bitmap(&mut self, d: u32) -> u32 {
        assert_eq!(self.call(d, 16, [OUT, 0, 0]), 0);
        self.read(OUT)
    }
    fn compatible(&mut self, device: u32, w: u32, h: u32) -> u32 {
        assert_eq!(self.call(device, 13, [OUT, w, h]), 0);
        let b = self.read(OUT);
        self.rect([0, 0, w as i16, h as i16]);
        assert_eq!(self.call(b, 9, [OUT, 0, 2]), 0);
        b
    }
    fn pixel(&mut self, b: u32, x: u32, y: u32) -> u32 {
        assert_eq!(self.call(b, 6, [x, y, OUT]), 0);
        self.read(OUT)
    }
    fn get_bitmap(&mut self, c: u32) -> u32 {
        assert_eq!(self.call(c, 3, [OUT, 0, 0]), 0);
        self.read(OUT)
    }
    fn clip(&mut self, c: u32) -> [i16; 4] {
        assert_eq!(self.call(c, 5, [OUT, 0, 0]), 0);
        let mut b = [0; 8];
        self.m.cpu().read_mem(OUT, &mut b).unwrap();
        std::array::from_fn(|i| i16::from_le_bytes([b[2 * i], b[2 * i + 1]]))
    }
}
impl Drop for F {
    fn drop(&mut self) {
        let actual = self.root.canonicalize().unwrap();
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests/wave21-delivery-notes/display-canvas/runtime")
            .canonicalize()
            .unwrap();
        assert!(actual.starts_with(base));
        std::fs::remove_dir_all(actual).unwrap();
    }
}
#[test]
fn display_canvas_primary_interfaces_are_real_and_unbound_methods_fail() {
    let mut f = F::new();
    let c = f.canvas();
    for id in [0x0101e3f2, 0x0101e443] {
        assert_eq!(f.call(c, 2, [id, OUT, 0]), 0);
        assert_eq!(f.read(OUT), c);
        f.call(c, 1, [0; 3]);
    }
    assert_eq!(f.call(c, 2, [0x01055241, OUT, 0]), 3);
    assert_eq!(f.read(OUT), 0);
    assert_ne!(f.call(c, 7, [OUT, 0, 0]), 0);
    assert_eq!(f.read(OUT), 0);
    assert_ne!(f.call(c, 3, [OUT, 0, 0]), 0);
    assert_ne!(f.call(c, 4, [0; 3]), 0);
    assert_ne!(f.call(c, 5, [0; 3]), 0);
    assert_ne!(f.call(c, 6, [0; 3]), 0);
    assert_eq!(f.call(c, 8, [0; 3]), 0);
    assert_eq!(f.call(c, 1, [0; 3]), 0);
}
#[test]
fn display_canvas_retains_wrapped_display_and_returns_acquired_device_bitmap() {
    let mut f = F::new();
    let c = f.canvas();
    let d = f.create(0x01001001);
    let device = f.bitmap(d);
    assert_eq!(f.call(c, 8, [d, 0, 0]), 0);
    assert_eq!(f.call(d, 1, [0; 3]), 1);
    assert_eq!(f.call(c, 7, [OUT, 0, 0]), 0);
    assert_eq!(f.read(OUT), d);
    assert_eq!(f.call(d, 1, [0; 3]), 1);
    let b = f.get_bitmap(c);
    assert_eq!(b, device);
    f.call(b, 1, [0; 3]);
    f.call(device, 1, [0; 3]);
    assert_eq!(f.call(c, 1, [0; 3]), 0);
}
#[test]
fn display_canvas_null_display_detaches_and_can_rebind_per_primary_ut() {
    let mut f = F::new();
    let c = f.canvas();
    let d = f.create(0x01001001);
    assert_eq!(f.call(c, 8, [d, 0, 0]), 0);
    assert_eq!(f.call(c, 8, [0; 3]), 0);
    for slot in [3, 4, 5, 6, 7] {
        assert_ne!(f.call(c, slot, [OUT, 0, 0]), 0);
    }
    assert_eq!(f.call(c, 8, [d, 0, 0]), 0);
    assert_eq!(f.call(c, 7, [OUT, 0, 0]), 0);
    assert_eq!(f.read(OUT), d);
    f.call(d, 1, [0; 3]);
    f.call(c, 1, [0; 3]);
    f.call(d, 1, [0; 3]);
}
#[test]
fn display_canvas_bitmap_replacement_keeps_real_offscreen_surface_alive() {
    let mut f = F::new();
    let c = f.canvas();
    let d = f.create(0x01001001);
    let device = f.bitmap(d);
    let b = f.compatible(device, 8, 8);
    assert_eq!(f.call(c, 8, [d, 0, 0]), 0);
    assert_eq!(f.call(c, 4, [b, 0, 0]), 0);
    f.call(b, 1, [0; 3]);
    let returned = f.get_bitmap(c);
    assert_eq!(returned, b);
    assert_eq!(f.call(returned, 5, [2, 3, 0xf800]), 0);
    assert_eq!(f.pixel(b, 2, 3), 0xf800);
    assert_eq!(f.call(c, 8, [0; 3]), 0);
    assert_eq!(f.pixel(returned, 2, 3), 0xf800);
    f.call(returned, 1, [0; 3]);
    f.call(d, 14, [device, 0, 0]);
    f.call(device, 1, [0; 3]);
    f.call(c, 1, [0; 3]);
    f.call(d, 1, [0; 3]);
}
#[test]
fn native_widget_draw_observes_external_display_destination_and_clip_changes() {
    let mut f = F::new();
    let c = f.canvas();
    let d = f.create(0x01001001);
    let device = f.bitmap(d);
    let b = f.compatible(device, 6, 6);
    let w = f.create(0x010551e0);
    f.put(OUT, 6);
    f.put(OUT + 4, 6);
    f.call(w, 7, [OUT, 0, 0]);
    f.call(w, 3, [0x801, 0x130, 0x0000ffff]);
    assert_eq!(f.call(c, 8, [d, 0, 0]), 0);
    f.rect([1, 2, 2, 2]);
    f.call(d, 18, [OUT, 0, 0]);
    f.call(w, 10, [c, 0, 0]);
    assert_eq!(f.pixel(device, 1, 2), 0xf800);
    assert_eq!(f.pixel(device, 0, 0), 0);
    f.call(d, 14, [b, 0, 0]);
    f.rect([3, 1, 2, 2]);
    f.call(d, 18, [OUT, 0, 0]);
    f.call(w, 10, [c, 1, 1]);
    assert_eq!(
        f.pixel(b, 3, 1),
        0xf800,
        "no GetBitmap call precedes this draw: helper must follow current wrapped Display"
    );
    assert_eq!(f.pixel(b, 0, 0), 0);
    assert_eq!(f.pixel(b, 4, 2), 0xf800);
    assert_eq!(f.pixel(b, 2, 1), 0);
    assert_eq!(f.pixel(device, 3, 1), 0);
    assert_eq!(f.clip(c), [3, 1, 2, 2]);
    f.call(w, 1, [0; 3]);
    f.call(c, 1, [0; 3]);
    f.call(d, 14, [device, 0, 0]);
    f.call(b, 1, [0; 3]);
    f.call(device, 1, [0; 3]);
    f.call(d, 1, [0; 3]);
}
#[test]
fn display_canvas_uses_the_wrapped_display_destination_instead_of_the_global_target() {
    let mut f = F::new();
    let device_display = f.create(0x01001001);
    let device = f.bitmap(device_display);
    let d1 = f.create(0x01001001);
    let d2 = f.create(0x01001001);
    let w1 = f.create(0x0103f105);
    let w2 = f.create(0x0103f105);
    for (w, d) in [(w1, d1), (w2, d2)] {
        f.put(OUT, 0x0103f104);
        f.put(OUT + 4, 0);
        f.put(OUT + 8, d);
        assert_eq!(f.call(w, 3, [0x801, 0x600, OUT]), 1);
    }
    let b1 = f.call(d1, 15, [0; 3]);
    let b2 = f.call(d2, 15, [0; 3]);
    assert_ne!(b1, b2);
    assert_ne!(b1, device);
    let c = f.canvas();
    assert_eq!(f.call(c, 8, [d1, 0, 0]), 0);
    let w = f.create(0x010551e0);
    f.put(OUT, 4);
    f.put(OUT + 4, 4);
    f.call(w, 7, [OUT, 0, 0]);
    f.call(w, 3, [0x801, 0x130, 0x0000ffff]);
    f.rect([1, 1, 2, 2]);
    f.call(d1, 18, [OUT, 0, 0]);
    // Uma chamada no segundo display deixa outro destino global antes do desenho.
    f.call(d2, 15, [0; 3]);
    f.call(b2, 1, [0; 3]);
    f.call(w, 10, [c, 0, 0]);
    assert_eq!(f.pixel(b1, 1, 1), 0xf800);
    assert_eq!(f.pixel(b1, 0, 0), 0);
    assert_eq!(f.pixel(b2, 1, 1), 0);
    assert_eq!(f.pixel(device, 1, 1), 0);
    for p in [w, c, b1, b2, w1, w2, d1, d2, device, device_display] {
        f.call(p, 1, [0; 3]);
    }
}
#[test]
fn display_canvas_binding_bitmap_clip_and_references_survive_save_restore() {
    let mut f = F::new();
    let c = f.canvas();
    let d = f.create(0x01001001);
    let device = f.bitmap(d);
    let a = f.compatible(device, 8, 8);
    let b = f.compatible(device, 8, 8);
    f.call(c, 8, [d, 0, 0]);
    f.call(c, 4, [a, 0, 0]);
    f.rect([1, 1, 3, 2]);
    assert_eq!(f.call(c, 6, [OUT, 0, 0]), 0);
    let saved = f.m.grava_estado();
    f.call(c, 4, [b, 0, 0]);
    f.rect([0, 0, 1, 1]);
    f.call(c, 6, [OUT, 0, 0]);
    f.m.restaura_estado(&saved).unwrap();
    assert_eq!(f.get_bitmap(c), a);
    f.call(a, 1, [0; 3]);
    assert_eq!(f.clip(c), [1, 1, 3, 2]);
    f.call(c, 1, [0; 3]);
    f.call(d, 14, [device, 0, 0]);
    for p in [a, b, device, d] {
        f.call(p, 1, [0; 3]);
    }
}
#[test]
fn display_canvas_factory_oom_and_invalid_parameters_preserve_bound_state() {
    let mut f = F::new();
    let c = f.canvas();
    let d = f.create(0x01001001);
    f.call(c, 8, [d, 0, 0]);
    assert_ne!(f.call(c, 8, [0xdead0000, 0, 0]), 0);
    assert_eq!(f.call(c, 7, [OUT, 0, 0]), 0);
    assert_eq!(f.read(OUT), d);
    assert_ne!(f.call(c, 3, [0; 3]), 0);
    f.call(c, 1, [0; 3]);
    f.call(d, 1, [0; 3]);
    let mut live = Vec::new();
    loop {
        let status = f.run(
            aee::encode(Interface::Shell, 2),
            [loader::OBJECT_BASE, 0x01001001, OUT, 0],
        );
        if status != 0 {
            break;
        }
        live.push(f.read(OUT));
        assert!(live.len() <= loader::OBJECT_SIZE / 4);
    }
    assert_eq!(
        f.run(
            aee::encode(Interface::Shell, 2),
            [loader::OBJECT_BASE, 0x010551c8, OUT, 0]
        ),
        2
    );
    assert_eq!(f.read(OUT), 0);
    let p = live.pop().unwrap();
    f.call(p, 1, [0; 3]);
    let c = f.canvas();
    assert_eq!(f.call(c, 1, [0; 3]), 0);
    for p in live {
        f.call(p, 1, [0; 3]);
    }
    assert_eq!(
        f.run(
            aee::encode(Interface::Shell, 2),
            [loader::OBJECT_BASE, 0x010551c8, 0, 0]
        ),
        14
    );
}
#[test]
fn bitmap_canvas_exposes_get_bitmap_and_clip_for_window_root_composition() {
    let mut f = F::new();
    let d = f.create(0x01001001);
    let device = f.bitmap(d);
    assert_eq!(f.call(device, 2, [0x0101e443, OUT, 0]), 0);
    let c = f.read(OUT);
    let count = f.call(device, 0, [0; 3]);
    f.call(device, 1, [0; 3]);
    assert_eq!(f.call(c, 3, [0; 3]), 14);
    for _ in 0..128 {
        assert_eq!(f.get_bitmap(c), device);
        assert_eq!(f.call(device, 1, [0; 3]), count - 1);
    }
    assert_eq!(f.get_bitmap(c), device);
    f.call(device, 1, [0; 3]);
    f.rect([1, 2, 2, 3]);
    assert_eq!(f.call(c, 6, [OUT, 0, 0]), 0);
    assert_eq!(f.clip(c), [1, 2, 2, 3]);
    let w = f.create(0x010551e0);
    f.put(OUT, 6);
    f.put(OUT + 4, 6);
    f.call(w, 7, [OUT, 0, 0]);
    f.call(w, 3, [0x801, 0x130, 0x0000ffff]);
    f.call(w, 10, [c, 0, 0]);
    assert_eq!(f.pixel(device, 1, 2), 0xf800);
    assert_eq!(f.pixel(device, 0, 0), 0);
    for p in [w, c, device, d] {
        f.call(p, 1, [0; 3]);
    }
}
#[test]
fn window_manager_composes_root_widget_through_its_temporary_bitmap_canvas() {
    let mut f = F::new();
    let d = f.create(0x01001001);
    let device = f.bitmap(d);
    let offscreen = f.compatible(device, 8, 8);
    let c = f.canvas();
    f.call(c, 8, [d, 0, 0]);
    f.call(c, 4, [offscreen, 0, 0]);
    let root = f.create(0x01055241);
    assert_eq!(f.call(root, 2, [0x01015952, OUT, 0]), 0);
    let widget = f.read(OUT);
    f.rect([0, 0, 8, 8]);
    f.call(root, 10, [c, OUT, 0]);
    let child = f.create(0x010551e0);
    f.put(OUT, 4);
    f.put(OUT + 4, 4);
    f.call(child, 7, [OUT, 0, 0]);
    f.call(child, 3, [0x801, 0x130, 0x0000ffff]);
    f.put(OUT, 2);
    f.put(OUT + 4, 3);
    f.put(OUT + 8, 1);
    assert_eq!(f.call(root, 5, [child, 0, OUT]), 0);
    f.call(widget, 10, [c, 0, 0]);
    assert_eq!(f.pixel(offscreen, 2, 3), 0xf800);
    f.rect([0, 0, 8, 8]);
    f.call(device, 9, [OUT, 0, 2]);
    let manager = f.create(0x01074e85);
    for (at, value) in [
        (OUT, 0x0106c5ab),
        (OUT + 4, OUT + 32),
        (OUT + 8, 4),
        (OUT + 12, 0),
        (OUT + 32, 1),
        (OUT + 40, 0),
    ] {
        f.put(at, value);
    }
    let sp = f.m.cpu().read_reg(Reg::Sp);
    f.put(sp, OUT + 40);
    assert_eq!(f.call(manager, 3, [widget, OUT, 1]), 0);
    for _ in 0..3 {
        for outcome in f.m.deliver_callbacks(500000).unwrap() {
            assert!(matches!(outcome, Outcome::Returned { .. }), "{outcome:?}");
        }
    }
    assert_eq!(
        f.pixel(device, 2, 3),
        0xf800,
        "Root's retained canvas targets offscreen; only WindowMgr's temporary canvas paints screen"
    );
    for p in [manager, child, root, widget, c] {
        f.call(p, 1, [0; 3]);
    }
    f.call(d, 14, [device, 0, 0]);
    for p in [offscreen, device, d] {
        f.call(p, 1, [0; 3]);
    }
}
