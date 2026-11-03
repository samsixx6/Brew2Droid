//! ABI público WindowMgr/DisplayWidget, sem atribuir layout privado ao provedor OEM.
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
                "window-test-{}-{}",
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
    fn setup(&mut self) -> (u32, u32, u32) {
        let mgr = self.create(0x01074e85);
        let widget = self.create(0x0103f105);
        let display = self.create(0x01001001);
        let p = self.p;
        self.wr(p, 0x0103f104);
        self.wr(p + 4, 0);
        self.wr(p + 8, display);
        assert_eq!(self.call(widget, 3, [0x801, 0x600, p]), 1);
        (mgr, widget, display)
    }
    fn open(&mut self, mgr: u32, w: u32) -> u32 {
        let p = self.p;
        for (at, v) in [
            (p, 0x0106c5ab),
            (p + 4, p + 32),
            (p + 8, 4),
            (p + 12, 0),
            (p + 32, 1),
            (p + 40, 0),
        ] {
            self.wr(at, v);
        }
        let sp = self.m.cpu().read_reg(Reg::Sp);
        self.wr(sp, p + 40);
        assert_eq!(self.call(mgr, 3, [w, p, 1]), 0);
        self.rd(p + 40)
    }
    fn pump(&mut self) {
        for x in self.m.deliver_callbacks(200000).unwrap() {
            assert!(matches!(x, Outcome::Returned { .. }), "{x:?}");
        }
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
#[test]
fn capabilities_use_sdk_or_semantics_and_typed_query_interface() {
    let mut f = F::new();
    let mgr = f.create(0x01074e85);
    let p = f.p;
    f.wr(p, 0xdeadbeef);
    f.wr(p + 4, 0x01073d57);
    assert_eq!(f.call(mgr, 5, [p, 2, p + 16]), 0);
    assert_eq!(f.rd(p + 16) & 255, 1);
    assert_eq!(f.call(mgr, 5, [p, 1, p + 16]), 0);
    assert_eq!(f.rd(p + 16) & 255, 0);
    assert_eq!(f.call(mgr, 6, [p, 1, p + 16]), 0);
    assert_eq!(f.rd(p), 0x01073d57);
    assert_eq!(f.rd(p + 16), 1);
    assert_eq!(f.call(mgr, 2, [0x01015952, p, 0]), 3);
    assert_eq!(f.rd(p), 0);
}
#[test]
fn creation_waits_for_handler_and_notifies_real_extent_model_after_restore() {
    let mut f = F::new();
    let (m, w, _) = f.setup();
    let id = f.open(m, w);
    assert_ne!(id, 0);
    let p = f.p;
    let ctx = p + 128;
    f.handler(w, ctx);
    assert_eq!(
        f.rd(ctx + 12),
        0,
        "CreateWindow não pode chamar o handler antes de retornar"
    );
    assert_eq!(f.call(w, 3, [0x800, 0x161, p]), 1);
    let model = f.rd(p);
    assert_ne!(model, w);
    assert_eq!(
        f.call(model, 2, [0x0101593c, p, 0]),
        3,
        "o view model é IModel, não IInterfaceModel"
    );
    let listener = p + 192;
    let cb = loader::MODULE_BASE + 0x280;
    let words = [
        0xe5912000u32,
        0xe5802000,
        0xe5912008,
        0xe5923000,
        0xe5803004,
        0xe5923004,
        0xe5803008,
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
    f.wr(listener + 8, cb);
    f.wr(listener + 12, p + 224);
    assert_eq!(f.call(model, 3, [listener, 0, 0]), 0);
    let s = f.m.grava_estado();
    f.m.restaura_estado(&s).unwrap();
    f.pump();
    assert_eq!(f.rd(ctx), 0x750);
    assert_eq!(f.rd(ctx + 4), 1);
    assert_eq!(f.rd(ctx + 8), 0);
    assert_eq!(f.rd(p + 224), 0x100c);
    assert_eq!(f.rd(p + 228), f.m.screen().width());
    assert_eq!(f.rd(p + 232), f.m.screen().height());
    let cancel = f.rd(listener + 16);
    f.entry(cancel, [listener, 0, 0, 0]);
    assert_eq!(f.rd(listener + 16), 0);
    f.call(model, 1, [0, 0, 0]);
}
#[test]
fn display_draws_offscreen_and_only_update_composites_owned_pixels() {
    let mut f = F::new();
    let (m, w, d) = f.setup();
    f.open(m, w);
    f.pump();
    let p = f.p;
    f.wr(p, 0x00020001);
    f.wr(p + 4, 0x00040003);
    let red = 0x0000ff00;
    let sp = f.m.cpu().read_reg(Reg::Sp);
    f.wr(sp, 0); // quinto argumento de DrawRect: flags, não o antigo winId*.
    f.api(Interface::Display, "DrawRect", [d, p, red, red]);
    assert_eq!(
        f.m.screen().get_pixel(1, 2),
        0,
        "o DisplayWidget deve envolver bitmap fora da tela"
    );
    f.api(Interface::Display, "Update", [d, 0, 0, 0]);
    assert_eq!(f.m.screen().get_pixel(1, 2), 0xf800);
    assert_eq!(f.m.screen().get_pixel(0, 0), 0);
}
#[test]
fn async_close_keeps_root_alive_until_status_then_releases_owned_refs() {
    let mut f = F::new();
    let (m, w, d) = f.setup();
    f.open(m, w);
    f.pump();
    let ctx = f.p + 128;
    f.handler(w, ctx);
    assert_eq!(f.call(w, 1, [0, 0, 0]), 1);
    assert_eq!(f.call(m, 7, [w, 0, 0]), 0);
    assert!(f.live(w));
    assert_eq!(f.rd(ctx + 12), 0);
    f.pump();
    assert_eq!(f.rd(ctx), 0x750);
    assert_eq!(f.rd(ctx + 4), 2);
    assert!(!f.live(w));
    assert_eq!(f.call(d, 1, [0, 0, 0]), 0);
    assert_eq!(f.call(m, 1, [0, 0, 0]), 0);
}
#[test]
fn unsupported_property_rolls_back_creation_and_does_not_retain_root() {
    let mut f = F::new();
    let (m, w, _) = f.setup();
    let p = f.p;
    f.wr(p, 0xdeadbeef);
    f.wr(p + 4, 0);
    f.wr(p + 8, 0);
    let sp = f.m.cpu().read_reg(Reg::Sp);
    f.wr(sp, p + 40);
    f.wr(p + 40, 77);
    assert_eq!(f.call(m, 3, [w, p, 1]), 20);
    assert_eq!(f.rd(p + 40), 0);
    assert_eq!(f.call(w, 1, [0, 0, 0]), 0);
    assert!(f.m.deliver_callbacks(200000).unwrap().is_empty());
}
#[test]
fn focus_moves_to_top_window_and_close_restores_previous_window() {
    let mut f = F::new();
    let (m, w, _) = f.setup();
    f.open(m, w);
    f.pump();
    let w2 = f.create(0x0103f105);
    f.open(m, w2);
    f.pump();
    let p = f.p;
    f.wr(p, 99);
    assert_eq!(f.call(w, 3, [0x701, 0, p]), 1);
    assert_eq!(f.rd(p) & 255, 0);
    assert_eq!(f.call(w2, 3, [0x701, 0, p]), 1);
    assert_eq!(f.rd(p) & 255, 1);
    f.call(m, 7, [w2, 0, 0]);
    f.pump();
    f.call(w, 3, [0x701, 0, p]);
    assert_eq!(f.rd(p) & 255, 1);
}
#[test]
fn normal_handset_input_is_delivered_to_focused_window_before_applet() {
    let mut f = F::new();
    let p = f.p;
    let app = p + 320;
    let vt = p + 384;
    let code = loader::MODULE_BASE + 0x300;
    let words = [0xe5801004u32, 0xe3a00000, 0xe12fff1e];
    f.m.cpu_mut()
        .write_mem(
            code,
            &words
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    f.wr(app, vt);
    f.wr(vt + 8, code);
    assert!(matches!(
        f.m.start_applet(app, 0x12345678, 100000).unwrap(),
        Outcome::Returned { .. }
    ));
    let (m, w, _) = f.setup();
    f.open(m, w);
    f.pump();
    let ctx = p + 128;
    f.handler(w, ctx);
    f.wr(app + 4, 0xabcdef);
    assert_eq!(
        f.m.deliver_handset_avk(0xe030, true, 100000).unwrap(),
        Outcome::Returned { code: 1 }
    );
    assert_eq!(f.rd(ctx), 0x100);
    assert_eq!(f.rd(ctx + 4), 0xe030);
    assert_eq!(
        f.rd(app + 4),
        0xabcdef,
        "o applet não recebe de novo uma tecla tratada pela janela"
    );
}
#[test]
fn modern_window_factories_return_two_and_rollback_partial_view_model() {
    let mut f = F::new();
    let p = f.p;
    let mut first = 0;
    loop {
        let rc = f.api(
            Interface::Shell,
            "CreateInstance",
            [loader::OBJECT_BASE, 0x01001001, p, 0],
        );
        if rc != 0 {
            break;
        }
        if first == 0 {
            first = f.rd(p);
        }
    }
    f.wr(p, 77);
    assert_eq!(
        f.api(
            Interface::Shell,
            "CreateInstance",
            [loader::OBJECT_BASE, 0x01074e85, p, 0]
        ),
        2
    );
    assert_eq!(f.rd(p), 0);
    assert_eq!(f.call(first, 1, [0, 0, 0]), 0);
    f.wr(p, 77);
    assert_eq!(
        f.api(
            Interface::Shell,
            "CreateInstance",
            [loader::OBJECT_BASE, 0x0103f105, p, 0]
        ),
        2
    );
    assert_eq!(f.rd(p), 0);
    assert_eq!(
        f.api(
            Interface::Shell,
            "CreateInstance",
            [loader::OBJECT_BASE, 0x01001001, p, 0]
        ),
        0
    );
}
#[test]
fn manager_does_not_treat_void_widget_setextent_r0_as_a_status_code() {
    let mut f = F::new();
    let (m, w, _) = f.setup();
    let p = f.p;
    let vt = p + 512;
    let old = f.rd(w);
    let mut bytes = [0u8; 60];
    f.m.cpu().read_mem(old, &mut bytes).unwrap();
    f.m.cpu_mut().write_mem(vt, &bytes).unwrap();
    let code = loader::MODULE_BASE + 0x340;
    f.m.cpu_mut()
        .write_mem(
            code,
            &[0xe3a0004du32, 0xe12fff1e]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    f.wr(vt + 7 * 4, code);
    f.wr(w, vt);
    let ctx = p + 128;
    f.handler(w, ctx);
    f.open(m, w);
    f.pump();
    assert_eq!(f.rd(ctx), 0x750);
    assert_eq!(f.rd(ctx + 4), 1);
    assert_eq!(
        f.rd(ctx + 8),
        0,
        "IWidget::SetExtent retorna void; R0 residual não é erro"
    );
}
#[test]
fn wrapped_destination_reset_keeps_drawing_offscreen_until_update() {
    let mut f = F::new();
    let (m, w, d) = f.setup();
    f.open(m, w);
    f.pump();
    let p = f.p;
    assert_eq!(f.api(Interface::Display, "SetDestination", [d, 0, 0, 0]), 0);
    let sp = f.m.cpu().read_reg(Reg::Sp);
    f.wr(sp, 0);
    f.wr(p, 0x00020001);
    f.wr(p + 4, 0x00040003);
    f.api(
        Interface::Display,
        "DrawRect",
        [d, p, 0x0000ff00, 0x0000ff00],
    );
    assert_eq!(
        f.m.screen().get_pixel(1, 2),
        0,
        "NULL restaura o destino do display envolvido, não o framebuffer físico"
    );
    f.api(Interface::Display, "Update", [d, 0, 0, 0]);
    assert_eq!(f.m.screen().get_pixel(1, 2), 0xf800);
}
#[test]
fn display_bitmap_content_model_is_distinct_from_extent_view_model_and_retains_source() {
    let mut f = F::new();
    let (_, w, d) = f.setup();
    let p = f.p;
    assert_eq!(
        f.call(w, 12, [0x0101593c, p, 0]),
        0,
        "UTDisplayWidget_SetGetModel exige IInterfaceModel"
    );
    let content = f.rd(p);
    assert_ne!(content, 0);
    assert_eq!(f.call(w, 3, [0x800, 0x161, p]), 1);
    let view = f.rd(p);
    assert_ne!(view, content);
    assert_eq!(
        f.call(view, 2, [0x0101593c, p, 0]),
        3,
        "modelo de extensão não é o modelo de bitmap"
    );
    assert_eq!(f.call(content, 6, [0x01001045, p, 0]), 0);
    let bitmap = f.rd(p);
    assert_ne!(bitmap, 0);
    assert_eq!(
        f.api(Interface::Display, "GetDestination", [d, 0, 0, 0]),
        bitmap
    );
    f.call(bitmap, 1, [0, 0, 0]);
    f.call(content, 1, [0, 0, 0]);
    f.call(view, 1, [0, 0, 0]);
    assert_eq!(f.call(w, 1, [0, 0, 0]), 0);
    assert!(f.live(bitmap), "o GetIPtr entregou uma referência própria");
    assert_eq!(f.call(bitmap, 1, [0, 0, 0]), 0);
    assert!(!f.live(bitmap));
}
