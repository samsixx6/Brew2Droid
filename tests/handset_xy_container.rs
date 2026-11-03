//! Contratos numéricos de CXYContainer, sem depender de nomes novos do enum.
use std::path::PathBuf;
use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend},
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    storage::StoragePaths,
};
const OUT: u32 = loader::HEAP_BASE + 0x8000;
struct F {
    m: Machine<BackendPadrao>,
    root: PathBuf,
}
impl F {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests/startup-repairs-wave21-20261031/components")
            .join(format!(
                "xy-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&root).unwrap();
        let storage = StoragePaths::from_root(root.join("runtime"));
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
        Self { m, root }
    }
    fn run(&mut self, entry: u32, args: [u32; 4]) -> u32 {
        match self.m.call_guest(entry, args, 200000).unwrap() {
            Outcome::Returned { code } => code,
            o => panic!("{entry:#x}: {o:?}"),
        }
    }
    fn call(&mut self, o: u32, slot: u32, args: [u32; 3]) -> u32 {
        let vt = self.read(o);
        let entry = self.read(vt + slot * 4);
        self.run(entry, [o, args[0], args[1], args[2]])
    }
    fn read(&self, p: u32) -> u32 {
        self.m.cpu().read_u32(p).unwrap()
    }
    fn put(&mut self, p: u32, v: u32) {
        self.m.cpu_mut().write_u32(p, v).unwrap();
    }
    fn create(&mut self, cls: u32) -> u32 {
        self.put(OUT, 0xdeadbeef);
        assert_eq!(
            self.run(
                aee::encode(Interface::Shell, 2),
                [loader::OBJECT_BASE, cls, OUT, 0]
            ),
            0,
            "a classe deve criar a interface documentada"
        );
        let p = self.read(OUT);
        assert_ne!(p, 0);
        p
    }
    fn xy(&mut self) -> (u32, u32) {
        let c = self.create(0x010551ed);
        assert_eq!(self.call(c, 2, [0x01015952, OUT, 0]), 0);
        let w = self.read(OUT);
        assert_ne!(
            w, c,
            "IXYContainer não herda IWidget: os dois contratos precisam de facets distintas"
        );
        (c, w)
    }
    fn extent(&mut self, w: u32, x: u32, y: u32) {
        self.put(OUT, x);
        self.put(OUT + 4, y);
        self.call(w, 7, [OUT, 0, 0]);
    }
    fn pos(&mut self, x: i32, y: i32, vis: bool) -> u32 {
        self.put(OUT + 32, x as u32);
        self.put(OUT + 36, y as u32);
        self.put(OUT + 40, vis as u32);
        OUT + 32
    }
    fn insert(&mut self, c: u32, w: u32, before: u32, x: i32, y: i32, vis: bool) -> u32 {
        let p = self.pos(x, y, vis);
        self.call(c, 5, [w, before, p])
    }
    fn setpos(&mut self, c: u32, w: u32, before: u32, x: i32, y: i32, vis: bool) -> u32 {
        let p = self.pos(x, y, vis);
        self.call(c, 8, [w, before, p])
    }
    fn image_widget(&mut self, name: &str, bgr: [u8; 3]) -> u32 {
        let mut bmp = vec![0; 70];
        bmp[..2].copy_from_slice(b"BM");
        for (at, v) in [(2, 70u32), (10, 54), (14, 40), (18, 2), (22, 2)] {
            bmp[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
        bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
        for at in [54, 57, 62, 65] {
            bmp[at..at + 3].copy_from_slice(&bgr);
        }
        std::fs::write(self.root.join(name), bmp).unwrap();
        self.m
            .cpu_mut()
            .write_mem(OUT, format!("{name}\0").as_bytes())
            .unwrap();
        let image = self.run(
            aee::encode(Interface::Shell, 19),
            [loader::OBJECT_BASE, OUT, 0, 0],
        );
        assert_ne!(image, 0);
        let w = self.create(0x01054a8f);
        assert_eq!(self.call(w, 12, [0x0101593c, OUT, 0]), 0);
        let model = self.read(OUT);
        self.call(model, 5, [image, 0x01013110, 0]);
        self.call(image, 1, [0; 3]);
        self.call(model, 1, [0; 3]);
        self.extent(w, 2, 2);
        w
    }
    fn canvas(&mut self) -> u32 {
        let d = self.create(0x01001001);
        self.call(d, 16, [OUT, 0, 0]);
        let b = self.read(OUT);
        assert_eq!(self.call(b, 2, [0x0101e443, OUT, 0]), 0);
        self.read(OUT)
    }
    fn handler(&mut self, w: u32, ctx: u32) {
        let cb = loader::MODULE_BASE + 0x200;
        let free = loader::MODULE_BASE + 0x300;
        let code = [
            0xe5801000u32,
            0xe5802004,
            0xe5803008,
            0xe59f2010,
            0xe1510002,
            0xe3a02001,
            0x05c32000,
            0xe3a00001,
            0xe12fff1e,
            0x702,
        ];
        self.m
            .cpu_mut()
            .write_mem(
                cb,
                &code
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        let code = [0xe590100cu32, 0xe2811001, 0xe580100c, 0xe12fff1e];
        self.m
            .cpu_mut()
            .write_mem(
                free,
                &code
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        self.put(OUT, cb);
        self.put(OUT + 4, ctx);
        self.put(OUT + 8, free);
        self.call(w, 4, [OUT, 0, 0]);
    }
}
impl Drop for F {
    fn drop(&mut self) {
        let root = self.root.canonicalize().unwrap();
        let allowed = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests/startup-repairs-wave21-20261031/components")
            .canonicalize()
            .unwrap();
        assert!(root.starts_with(allowed));
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn xy_factory_has_distinct_facets_and_one_shared_lifetime() {
    let mut f = F::new();
    let (c, w) = f.xy();
    assert_ne!(f.read(c), f.read(w));
    for iid in [0x01015954, 0x01015932, 0x01000001] {
        assert_eq!(f.call(w, 2, [iid, OUT, 0]), 0);
        assert_eq!(f.read(OUT), c);
        assert_eq!(f.call(c, 1, [0; 3]), 2);
    }
    assert_eq!(f.call(c, 2, [0xdeadbeef, OUT, 0]), 3);
    assert_eq!(f.read(OUT), 0);
    assert_eq!(f.call(c, 2, [0x01015952, 0, 0]), 14);
    assert_eq!(f.call(c, 1, [0; 3]), 1);
    f.extent(w, 7, 8);
    assert_eq!(f.call(w, 6, [OUT, 0, 0]), 0);
    assert_eq!((f.read(OUT), f.read(OUT + 4)), (7, 8));
    assert_eq!(f.call(w, 1, [0; 3]), 0);
    assert_eq!(f.call(w, 0, [0; 3]), 14);
    assert_eq!(f.call(c, 0, [0; 3]), 14);
}
#[test]
fn xy_order_and_removal_retain_children_but_not_their_parent() {
    let mut f = F::new();
    let (c, w) = f.xy();
    let a = f.create(0x01054a8f);
    let b = f.create(0x01054a8f);
    assert_eq!(f.insert(c, a, 0, 1, 2, true), 0);
    assert_eq!(f.insert(c, b, u32::MAX, 3, 4, true), 0);
    assert_eq!(f.call(c, 7, [0, 1, 0]), b);
    assert_eq!(f.call(c, 7, [b, 1, 0]), a);
    assert_eq!(f.call(c, 7, [a, 1, 0]), 0);
    assert_eq!(f.call(c, 7, [a, 1, 1]), b);
    assert_eq!(f.call(a, 0, [0; 3]), 3);
    assert_eq!(f.call(a, 1, [0; 3]), 2);
    assert_eq!(f.call(a, 8, [OUT, 0, 0]), 0);
    assert_eq!(f.read(OUT), c);
    assert_eq!(f.call(c, 1, [0; 3]), 2);
    assert_eq!(f.call(c, 6, [a, 0, 0]), 0);
    f.call(a, 8, [OUT, 0, 0]);
    assert_eq!(f.read(OUT), 0);
    assert_eq!(f.call(a, 1, [0; 3]), 0);
    assert_eq!(f.call(c, 1, [0; 3]), 1);
    assert_eq!(f.call(w, 1, [0; 3]), 0);
    f.call(b, 8, [OUT, 0, 0]);
    assert_eq!(f.read(OUT), 0);
    assert_eq!(f.call(b, 1, [0; 3]), 0);
}
#[test]
fn xy_positions_visibility_and_bad_inputs_do_not_corrupt_children() {
    let mut f = F::new();
    let (c, w) = f.xy();
    let a = f.create(0x01054a8f);
    let b = f.create(0x01054a8f);
    assert_eq!(f.insert(c, a, 0, -4, 6, true), 0);
    assert_eq!(f.insert(c, b, a, 2, 3, true), 0);
    assert_eq!(f.call(c, 7, [0, 1, 0]), b);
    assert_eq!(f.setpos(c, a, u32::MAX, 8, -7, false), 0);
    assert_eq!(f.call(c, 7, [0, 1, 0]), a);
    f.put(OUT + 44, 0xaabbccdd);
    assert_eq!(f.call(c, 9, [a, OUT + 32, 0]), 0);
    assert_eq!((f.read(OUT + 32), f.read(OUT + 36)), (8, (-7i32) as u32));
    let mut vis = [0; 1];
    f.m.cpu().read_mem(OUT + 40, &mut vis).unwrap();
    assert_eq!(vis, [0]);
    assert_eq!(f.read(OUT + 44), 0xaabbccdd);
    assert_eq!(f.insert(c, a, 0, 0, 0, true), 14);
    assert_eq!(f.insert(c, b, 0xdead0000, 0, 0, true), 14);
    assert_eq!(f.call(c, 9, [a, 0, 0]), 14);
    assert_eq!(f.call(c, 9, [a, 0xdead0000, 0]), 14);
    assert_eq!(f.call(c, 8, [a, 0, 0]), 14);
    assert_eq!(f.call(c, 6, [0xdead0000, 0, 0]), 14);
    assert_eq!(f.insert(c, w, 0, 0, 0, true), 14);
    assert_eq!(f.insert(c, c, 0, 0, 0, true), 14);
    f.call(c, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
    f.call(a, 1, [0; 3]);
    f.call(b, 1, [0; 3]);
}
#[test]
fn xy_draws_real_children_in_z_order_and_clips_to_its_extent() {
    let mut f = F::new();
    let (c, w) = f.xy();
    let a = f.image_widget("red.bmp", [0, 0, 255]);
    let b = f.image_widget("green.bmp", [0, 255, 0]);
    let canvas = f.canvas();
    f.extent(w, 3, 3);
    assert_eq!(f.call(w, 3, [0x801, 0x130, 0xff000000]), 1);
    assert_eq!(f.insert(c, a, 0, 1, 1, true), 0);
    assert_eq!(f.insert(c, b, 0, 1, 1, true), 0);
    f.call(w, 10, [canvas, 4, 5]);
    assert_eq!(f.m.screen().get_pixel(4, 5), 0x001f);
    assert_eq!(f.m.screen().get_pixel(5, 6), 0x07e0);
    assert_eq!(f.m.screen().get_pixel(7, 8), 0);
    assert_eq!(f.setpos(c, b, u32::MAX, 1, 1, true), 0);
    f.call(w, 10, [canvas, 4, 5]);
    assert_eq!(f.m.screen().get_pixel(5, 6), 0xf800);
    assert_eq!(f.setpos(c, a, 0, 2, 2, true), 0);
    assert_eq!(f.setpos(c, b, 0, 1, 1, false), 0);
    f.call(w, 10, [canvas, 4, 5]);
    assert_eq!(f.m.screen().get_pixel(5, 6), 0x001f);
    assert_eq!(f.m.screen().get_pixel(6, 7), 0xf800);
    assert_eq!(f.m.screen().get_pixel(7, 8), 0);
    f.call(c, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
    f.call(a, 1, [0; 3]);
    f.call(b, 1, [0; 3]);
}
#[test]
fn xy_locate_accumulates_nested_offsets_and_restores_owned_state() {
    let mut f = F::new();
    let (c, w) = f.xy();
    let (inner, iw) = f.xy();
    let child = f.create(0x01054a8f);
    assert_eq!(f.insert(c, iw, 0, -2, 5, true), 0);
    assert_eq!(f.insert(inner, child, 0, 7, -3, true), 0);
    let state = f.m.grava_estado();
    assert_eq!(f.setpos(c, iw, 0, 100, 100, false), 0);
    f.m.restaura_estado(&state).unwrap();
    f.m.cpu_mut()
        .write_mem(OUT + 64, &[0, 0, 0, 0, 2, 0, 2, 0])
        .unwrap();
    assert_eq!(f.call(inner, 4, [child, OUT, OUT + 64]), 0);
    assert_eq!(f.read(OUT), c);
    let mut rect = [0; 8];
    f.m.cpu().read_mem(OUT + 64, &mut rect).unwrap();
    assert_eq!(rect, [5, 0, 2, 0, 2, 0, 2, 0]);
    assert_eq!(f.call(c, 1, [0; 3]), 2);
    assert_eq!(
        f.insert(inner, w, 0, 0, 0, true),
        14,
        "a inserção não pode criar um ciclo de pais"
    );
    f.call(c, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
    f.call(inner, 1, [0; 3]);
    f.call(iw, 1, [0; 3]);
    f.call(child, 8, [OUT, 0, 0]);
    assert_eq!(f.read(OUT), 0);
    assert_eq!(f.call(child, 1, [0; 3]), 0);
}
#[test]
fn xy_focus_routes_events_and_cleanup_survives_snapshot() {
    let mut f = F::new();
    let (c, w) = f.xy();
    let a = f.create(0x01054a8f);
    let b = f.create(0x01054a8f);
    let ctx = OUT + 256;
    let ctx2 = OUT + 288;
    f.handler(a, ctx);
    f.handler(b, ctx2);
    assert_eq!(f.insert(c, a, 0, 0, 0, true), 0);
    assert_eq!(f.insert(c, b, 0, 0, 0, true), 0);
    assert_eq!(f.call(w, 3, [0x700, 1, 0]), 1);
    assert_eq!(f.call(w, 3, [0x713, 0, OUT]), 1);
    assert_eq!(f.read(OUT), a);
    f.call(a, 1, [0; 3]);
    assert_eq!(f.call(w, 3, [0x1234, 9, 10]), 1);
    assert_eq!(
        (f.read(ctx), f.read(ctx + 4), f.read(ctx + 8)),
        (0x1234, 9, 10)
    );
    assert_eq!(f.call(w, 3, [0x711, 0, 3]), 1);
    assert_eq!(f.read(ctx + 4), 0, "o widget antigo recebe perda de foco");
    assert_eq!(f.read(ctx2 + 4), 1);
    let state = f.m.grava_estado();
    f.call(w, 3, [0x711, 0, 0]);
    f.m.restaura_estado(&state).unwrap();
    assert_eq!(f.call(w, 3, [0x713, 0, OUT]), 1);
    assert_eq!(f.read(OUT), b);
    f.call(b, 1, [0; 3]);
    assert_eq!(f.call(c, 6, [b, 0, 0]), 0);
    assert_eq!(f.call(w, 3, [0x713, 0, OUT]), 1);
    assert_ne!(f.read(OUT), b);
    if f.read(OUT) != 0 {
        let p = f.read(OUT);
        f.call(p, 1, [0; 3]);
    }
    f.call(c, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
    f.call(a, 1, [0; 3]);
    f.call(b, 1, [0; 3]);
    assert_eq!(f.read(ctx + 12), 1);
    assert_eq!(f.read(ctx2 + 12), 1);
}
#[test]
fn xy_custom_handler_can_chain_to_original_and_unsupported_model_is_truthful() {
    let mut f = F::new();
    let (c, w) = f.xy();
    f.put(OUT, 0);
    f.put(OUT + 4, 0);
    f.put(OUT + 8, 0);
    f.call(w, 4, [OUT, 0, 0]);
    let default = f.read(OUT);
    let context = f.read(OUT + 4);
    assert_ne!(
        default, 0,
        "SetHandler devolve o handler anterior para encadeamento normal"
    );
    f.put(OUT + 100, 0);
    assert_eq!(f.run(default, [context, 0x800, 0x153, OUT + 100]), 1);
    assert_eq!(f.read(OUT + 100), 0);
    assert_eq!(f.call(w, 12, [0x0101593a, OUT + 100, 0]), 1);
    assert_eq!(f.read(OUT + 100), 0);
    assert_eq!(f.call(w, 13, [0, 0, 0]), 3);
    assert_eq!(f.call(w, 11, [0, 0, 0]), 0);
    f.call(c, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
}

#[test]
fn xy_factory_allocation_failure_uses_native_error_and_rolls_back_partial_owner() {
    let mut f = F::new();
    let (c, w) = f.xy();
    f.call(c, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
    let mut objects = Vec::new();
    loop {
        f.put(OUT, 0xdeadbeef);
        let status = f.run(
            aee::encode(Interface::Shell, 2),
            [loader::OBJECT_BASE, 0x01001001, OUT, 0],
        );
        if status != 0 {
            break;
        }
        objects.push(f.read(OUT));
        assert!(objects.len() <= loader::OBJECT_SIZE / 4);
    }
    assert!(objects.len() > 1000);
    assert_eq!(
        f.run(
            aee::encode(Interface::Shell, 2),
            [loader::OBJECT_BASE, 0x010551ed, OUT, 0]
        ),
        2
    );
    assert_eq!(f.read(OUT), 0);
    // Só um slot livre: a facet IWidget falha depois de alocar o dono.
    let last = objects.pop().unwrap();
    f.call(last, 1, [0; 3]);
    assert_eq!(
        f.run(
            aee::encode(Interface::Shell, 2),
            [loader::OBJECT_BASE, 0x010551ed, OUT, 0]
        ),
        2
    );
    assert_eq!(f.read(OUT), 0);
    assert_eq!(
        f.run(
            aee::encode(Interface::Shell, 2),
            [loader::OBJECT_BASE, 0x01001001, OUT, 0]
        ),
        0,
        "a criação parcial não pode consumir o slot livre"
    );
    objects.push(f.read(OUT));
    for object in objects {
        f.call(object, 1, [0; 3]);
    }
    // Repetir cria/libera verifica reciclagem das duas facets e estado no heap.
    for _ in 0..1100 {
        let (c, w) = f.xy();
        assert_eq!(f.call(c, 1, [0; 3]), 1);
        assert_eq!(f.call(w, 1, [0; 3]), 0);
    }
}

#[test]
fn xy_visibility_property_and_background_image_keep_real_references() {
    let mut f = F::new();
    let (c, w) = f.xy();
    let image = f.image_widget("bg-child.bmp", [0, 0, 255]);
    f.insert(c, image, 0, 0, 0, true);
    let canvas = f.canvas();
    f.extent(w, 2, 2);
    f.put(OUT + 160, image);
    f.put(OUT + 164, 0xaabbcc00);
    assert_eq!(f.call(w, 3, [0x801, 0x341, OUT + 160]), 1);
    assert_eq!(f.call(w, 3, [0x800, 0x341, OUT + 160]), 1);
    assert_eq!(
        f.read(OUT + 164),
        0xaabbcc00,
        "boolean público não sobrescreve padding"
    );
    f.call(w, 10, [canvas, 2, 2]);
    assert_eq!(f.m.screen().get_pixel(2, 2), 0);
    f.put(OUT + 164, 1);
    assert_eq!(f.call(w, 3, [0x801, 0x341, OUT + 160]), 1);
    f.call(w, 10, [canvas, 2, 2]);
    assert_eq!(f.m.screen().get_pixel(2, 2), 0xf800);
    assert_eq!(
        f.call(w, 3, [0x801, 0x351, 0x1234]),
        0,
        "flags de imagem não modelados falham explicitamente"
    );
    assert_eq!(f.call(w, 3, [0x801, 0x350, 0xdead0000]), 0);
    assert_eq!(
        f.call(w, 10, [0xdead0000, 0, 0]),
        20,
        "canvas estrangeiro não recebe sucesso fictício"
    );
    assert_eq!(f.call(w, 12, [0x0101593a, 0xdead0000, 0]), 14);
    assert_eq!(f.call(image, 12, [0x0101593c, OUT, 0]), 0);
    let model = f.read(OUT);
    assert_eq!(f.call(model, 6, [0x01013110, OUT, 0]), 0);
    let background = f.read(OUT);
    f.call(model, 1, [0; 3]);
    assert_eq!(f.call(w, 3, [0x801, 0x350, background]), 1);
    assert_eq!(f.call(background, 1, [0; 3]), 2);
    assert_eq!(f.call(c, 6, [image, 0, 0]), 0);
    assert_eq!(f.call(image, 1, [0; 3]), 0);
    let state = f.m.grava_estado();
    f.m.restaura_estado(&state).unwrap();
    f.call(w, 10, [canvas, 5, 5]);
    assert_eq!(f.m.screen().get_pixel(5, 5), 0xf800);
    assert_eq!(f.call(w, 3, [0x800, 0x350, OUT]), 1);
    assert_eq!(f.read(OUT), background);
    assert_eq!(f.call(background, 1, [0; 3]), 1);
    f.call(c, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
    assert_eq!(f.call(background, 0, [0; 3]), 0);
}

#[test]
fn xy_locate_requires_an_explicit_child_and_preserves_outputs_on_failure() {
    let mut f = F::new();
    let (c, w) = f.xy();
    let child = f.create(0x01054a8f);
    assert_eq!(f.insert(c, child, 0, 3, 4, true), 0);
    for invalid in [0, 1, u32::MAX, 0xdead0000] {
        let original = [1, 0, 2, 0, 5, 0, 6, 0];
        f.m.cpu_mut().write_mem(OUT + 64, &original).unwrap();
        f.put(OUT, 0xdeadbeef);
        assert_eq!(
            f.call(c, 4, [invalid, OUT, OUT + 64]),
            1,
            "Locate só aceita o ponteiro explícito do widget, diferente de Remove/GetPos"
        );
        let mut after = [0; 8];
        f.m.cpu().read_mem(OUT + 64, &mut after).unwrap();
        assert_eq!(after, original);
        assert_eq!(f.read(OUT), 0xdeadbeef);
    }
    f.call(c, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
    f.call(child, 1, [0; 3]);
}

#[test]
fn xy_focus_does_not_keep_a_child_removed_by_its_own_focus_query() {
    let mut f = F::new();
    let (c, w) = f.xy();
    let child = f.create(0x01054a8f);
    let context = OUT + 256;
    f.put(context, c);
    f.put(context + 4, child);
    let cb = loader::MODULE_BASE + 0x400;
    // No EVT_WDG_CANTAKEFOCUS, responder TRUE e remover a si mesmo do pai.
    let code = [
        0xe92d4070u32,
        0xe1a04000,
        0xe1a05003,
        0xe59f6028,
        0xe1510006,
        0x1a000006,
        0xe3a06001,
        0xe5c56000,
        0xe5940000,
        0xe5941004,
        0xe5902000,
        0xe5922018,
        0xe12fff32,
        0xe3a00001,
        0xe8bd8070,
        0x702,
    ];
    f.m.cpu_mut()
        .write_mem(
            cb,
            &code
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    f.put(OUT, cb);
    f.put(OUT + 4, context);
    f.put(OUT + 8, 0);
    f.call(child, 4, [OUT, 0, 0]);
    assert_eq!(f.insert(c, child, 0, 0, 0, true), 0);
    assert_eq!(f.call(w, 3, [0x700, 1, 0]), 1);
    assert_eq!(f.call(c, 7, [0, 1, 0]), 0);
    assert_eq!(f.call(w, 3, [0x713, 0, OUT]), 1);
    assert_eq!(
        f.read(OUT),
        0,
        "um callback que remove o filho não pode deixar foco no item desligado"
    );
    assert_eq!(
        f.call(child, 0, [0; 3]),
        2,
        "a remoção deve soltar exatamente a referência do pai"
    );
    f.call(child, 1, [0; 3]);
    f.call(c, 1, [0; 3]);
    f.call(w, 1, [0; 3]);
    f.call(child, 1, [0; 3]);
}
