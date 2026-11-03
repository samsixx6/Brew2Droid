// Contrato público de AEEWidgetProperties.h: gradiente RGBA por estado.
use std::path::PathBuf;
use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend},
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    storage::StoragePaths,
};
const OUT: u32 = loader::HEAP_BASE + 0x10000;
struct F {
    m: Machine<BackendPadrao>,
}
impl F {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests/wave26-work-20261102/vlingo/runtime")
            .join(format!(
                "gradient-{}-{}",
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
        let mut f = Self { m };
        let slot = (0..256)
            .find(|s| Interface::Helpers.method(*s) == Some("malloc"))
            .unwrap();
        let p = f.run(aee::encode(Interface::Helpers, slot), [0x20000, 0, 0, 0]);
        assert!(p <= OUT && p + 0x20000 > OUT + 128);
        f
    }
    fn run(&mut self, entry: u32, args: [u32; 4]) -> u32 {
        match self.m.call_guest(entry, args, 500000).unwrap() {
            Outcome::Returned { code } => code,
            o => panic!("{o:?}"),
        }
    }
    fn read(&self, p: u32) -> u32 {
        self.m.cpu().read_u32(p).unwrap()
    }
    fn put(&mut self, p: u32, v: u32) {
        self.m.cpu_mut().write_u32(p, v).unwrap()
    }
    fn call(&mut self, w: u32, slot: u32, a: [u32; 3]) -> u32 {
        let entry = self.read(self.read(w) + slot * 4);
        self.run(entry, [w, a[0], a[1], a[2]])
    }
    fn create(&mut self, id: u32) -> u32 {
        assert_eq!(
            self.run(
                aee::encode(Interface::Shell, 2),
                [loader::OBJECT_BASE, id, OUT, 0]
            ),
            0
        );
        self.read(OUT)
    }
    fn set(&mut self, w: u32, p: u32, v: u32) {
        assert_eq!(self.call(w, 3, [0x801, p, v]), 1, "propriedade {p:#x}")
    }
    fn canvas(&mut self) -> u32 {
        let d = self.create(0x01001001);
        self.call(d, 16, [OUT, 0, 0]);
        let b = self.read(OUT);
        assert_eq!(self.call(b, 2, [0x0101e443, OUT, 0]), 0);
        self.read(OUT)
    }
    fn extent(&mut self, w: u32) {
        self.put(OUT, 9);
        self.put(OUT + 4, 9);
        self.call(w, 7, [OUT, 0, 0]);
    }
    fn pixel(&self, x: i32, y: i32) -> u16 {
        self.m.screen().get_pixel(x, y)
    }
}
fn progression(class: u32) {
    let mut f = F::new();
    let w = f.create(class);
    let canvas = f.canvas();
    f.extent(w);
    f.set(w, 0x130, 0x0000ffff);
    f.set(w, 0x238, 0xff0000ff);
    for style in 0..4 {
        f.set(w, 0x210, style);
        f.call(w, 10, [canvas, 4, 5]);
        let vertical = style % 2 == 0;
        let center = style >= 2;
        let at = |f: &F, n: i32| {
            if vertical {
                f.pixel(8, 5 + n)
            } else {
                f.pixel(4 + n, 9)
            }
        };
        assert_eq!(at(&f, 0), 0xf800);
        assert_eq!(at(&f, 8), if center { 0xf800 } else { 0x001f });
        if center {
            assert_eq!(at(&f, 4), 0x001f)
        } else {
            assert_ne!(at(&f, 4), 0xf800);
            assert_ne!(at(&f, 4), 0x001f)
        }
    }
    f.set(w, 0x210, 4);
    f.call(w, 10, [canvas, 4, 5]);
    assert_eq!(f.pixel(8, 9), 0xf800);
}
#[test]
fn button_gradient_progression_uses_the_widget_extent() {
    progression(0x01053c8b)
}
#[test]
fn static_gradient_progression_uses_the_widget_extent() {
    progression(0x010551e0)
}
#[test]
fn button_gradient_style_get_writes_only_the_sdk_uint16() {
    let mut f = F::new();
    let w = f.create(0x01053c8b);
    f.put(OUT, 0xdeadbeef);
    assert_eq!(f.call(w, 3, [0x800, 0x210, OUT]), 1);
    assert_eq!(f.read(OUT), 0xdead0004);
    f.set(w, 0x210, 2);
    assert_eq!(f.call(w, 3, [0x801, 0x210, 5]), 0);
    f.put(OUT, 0xdeadbeef);
    assert_eq!(f.call(w, 3, [0x800, 0x210, OUT]), 1);
    assert_eq!(f.read(OUT), 0xdead0002);
}
#[test]
fn button_gradient_state_and_snapshot_are_real_properties() {
    let mut f = F::new();
    let w = f.create(0x01053c8b);
    let c = f.canvas();
    f.extent(w);
    f.set(w, 0x130, 0x0000ffff);
    f.set(w, 0x238, 0xff0000ff);
    f.set(w, 0x239, 0x00ff00ff);
    f.set(w, 0x210, 0);
    f.set(w, 0x151, 1);
    f.call(w, 10, [c, 4, 5]);
    assert_eq!(f.pixel(8, 13), 0x07e0);
    f.set(w, 0x242, 0xffffffff);
    f.call(w, 3, [0x700, 1, 0]);
    f.call(w, 10, [c, 4, 5]);
    assert_eq!(f.pixel(8, 13), 0xffff);
    let save = f.m.grava_estado();
    f.set(w, 0x242, 0);
    f.m.restaura_estado(&save).unwrap();
    f.call(w, 10, [c, 4, 5]);
    assert_eq!(f.pixel(8, 13), 0xffff);
}
#[test]
fn clipped_gradient_keeps_the_original_progression_and_rgba_alpha() {
    for class in [0x01053c8b, 0x010551e0] {
        let mut f = F::new();
        let w = f.create(class);
        let c = f.canvas();
        f.extent(w);
        f.set(w, 0x130, 0x0000ffff);
        f.set(w, 0x238, 0xff0000ff);
        f.set(w, 0x210, 0);
        f.call(w, 10, [c, 4, 5]);
        let expected = f.pixel(8, 9);
        f.set(w, 0x210, 4);
        f.set(w, 0x130, 0x00ff00ff);
        f.call(w, 10, [c, 4, 5]);
        f.m.cpu_mut()
            .write_mem(
                OUT,
                &[7i16, 8, 4, 3]
                    .into_iter()
                    .flat_map(i16::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        f.call(c, 6, [OUT, 0, 0]);
        f.set(w, 0x130, 0x0000ffff);
        f.set(w, 0x210, 0);
        f.call(w, 10, [c, 4, 5]);
        assert_eq!(f.pixel(8, 9), expected);
        assert_eq!(f.pixel(8, 5), 0x07e0);
        assert_eq!(f.pixel(8, 13), 0x07e0);
    }
    let mut f = F::new();
    let w = f.create(0x01053c8b);
    let c = f.canvas();
    f.extent(w);
    f.set(w, 0x130, 0x00ff00ff);
    f.call(w, 10, [c, 4, 5]);
    f.set(w, 0x130, 0x0000ff00);
    f.set(w, 0x238, 0xff0000ff);
    f.set(w, 0x210, 0);
    f.call(w, 10, [c, 4, 5]);
    assert_eq!(f.pixel(8, 5), 0x07e0);
    assert_eq!(f.pixel(8, 13), 0x001f);
    assert_ne!(f.pixel(8, 9), 0x07e0);
    assert_ne!(f.pixel(8, 9), 0x001f);
}
#[test]
fn gradient_preserves_the_rounded_button_boundary() {
    let mut f = F::new();
    let w = f.create(0x01053c8b);
    let c = f.canvas();
    f.extent(w);
    f.set(w, 0x119, 2);
    f.set(w, 0x103, 3);
    f.set(w, 0x130, 0x0000ffff);
    f.set(w, 0x238, 0xff0000ff);
    f.set(w, 0x210, 2);
    f.call(w, 10, [c, 4, 5]);
    assert_eq!(f.pixel(4, 5), 0);
    assert_eq!(f.pixel(8, 5), 0xf800);
    assert_eq!(f.pixel(8, 9), 0x001f);
    assert_eq!(f.pixel(4, 13), 0);
}
