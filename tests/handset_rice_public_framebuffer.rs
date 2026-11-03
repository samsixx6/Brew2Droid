use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend, Reg},
    handset::types::DisplayGeometry,
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    storage::StoragePaths,
};
const OLD: &[u8] = &[
    0x70, 0x40, 0x2d, 0xe9, 0xe4, 0xf6, 0xff, 0xeb, 0x00, 0x50, 0xa0, 0xe1, 0x8f, 0x4f, 0x80, 0xe2,
    0x44, 0x04, 0x90, 0xe5, 0xb0, 0x10, 0x9f, 0xe5, 0x01, 0x00, 0x50, 0xe1, 0x07, 0x00, 0x00, 0x2a,
    0x44, 0x03, 0x95, 0xe5, 0x78, 0x00, 0x50, 0xe3, 0x04, 0x00, 0x00, 0xaa, 0x03, 0x0c, 0x85, 0xe2,
    0xb2, 0x18, 0xd0, 0xe1, 0x03, 0x00, 0x51, 0xe3, 0x08, 0x10, 0xa0, 0x03, 0xb2, 0x18, 0xc0, 0x01,
    0x00, 0x00, 0xa0, 0xe3, 0x71, 0x11, 0x00, 0xeb, 0x00, 0x60, 0xb0, 0xe1, 0x01, 0x00, 0x76, 0x13,
    0x0c, 0x00, 0x00, 0x0a, 0x06, 0x10, 0xa0, 0xe1, 0x00, 0x00, 0xa0, 0xe3, 0xc6, 0x2f, 0x00, 0xeb,
    0x00, 0x00, 0x50, 0xe3, 0x07, 0x00, 0x00, 0x0a, 0x01, 0x00, 0xa0, 0xe3, 0x67, 0x11, 0x00, 0xeb,
    0x30, 0x00, 0x84, 0xe5, 0x03, 0x00, 0xa0, 0xe3, 0x24, 0x00, 0xa4, 0xe5, 0x08, 0x60, 0x84, 0xe5,
    0x70, 0x40, 0xbd, 0xe8, 0x1e, 0xff, 0x2f, 0xe1, 0x02, 0x00, 0xa0, 0xe3, 0x5f, 0x11, 0x00, 0xeb,
    0x01, 0x10, 0x80, 0xe2, 0x02, 0x00, 0xa0, 0xe3, 0x28, 0x10, 0x84, 0xe5, 0x3d, 0x33, 0x00, 0xeb,
    0x06, 0x09, 0x45, 0xe2,
];
const FIXED: &[u8] = &[
    0x01, 0xc0, 0x8f, 0xe2, 0x1c, 0xff, 0x2f, 0xe1, 0xf0, 0xb5, 0x85, 0xb0, 0x23, 0x4b, 0x98, 0x47,
    0x04, 0x00, 0xa5, 0x6a, 0x00, 0x26, 0x00, 0x27, 0x00, 0x96, 0x01, 0x96, 0x28, 0x00, 0x69, 0x46,
    0x03, 0x68, 0x1b, 0x6c, 0x98, 0x47, 0x02, 0x90, 0x00, 0x28, 0x24, 0xd1, 0x00, 0x9e, 0x00, 0x2e,
    0x1f, 0xd0, 0x30, 0x00, 0x18, 0x49, 0x01, 0xaa, 0x03, 0x68, 0x9b, 0x68, 0x98, 0x47, 0x02, 0x90,
    0x00, 0x28, 0x18, 0xd1, 0x01, 0x9f, 0x00, 0x2f, 0x13, 0xd0, 0x38, 0x7f, 0x10, 0x28, 0x10, 0xd1,
    0x78, 0x7f, 0x10, 0x28, 0x0d, 0xd1, 0xb8, 0x68, 0x00, 0x28, 0x0a, 0xd0, 0x10, 0x49, 0x61, 0x18,
    0x88, 0x60, 0x3a, 0x8b, 0x82, 0x18, 0xca, 0x60, 0x03, 0x20, 0x08, 0x60, 0x00, 0x20, 0x02, 0x90,
    0x01, 0xe0, 0x14, 0x20, 0x02, 0x90, 0x00, 0x2f, 0x03, 0xd0, 0x38, 0x00, 0x03, 0x68, 0x5b, 0x68,
    0x98, 0x47, 0x00, 0x2e, 0x03, 0xd0, 0x30, 0x00, 0x03, 0x68, 0x5b, 0x68, 0x98, 0x47, 0x02, 0x98,
    0x05, 0xb0, 0xf0, 0xbc, 0x08, 0xbc, 0x18, 0x47, 0x45, 0x10, 0x00, 0x01, 0x70, 0xdd, 0x01, 0x00,
    0x60, 0x02, 0x00, 0x00,
];
const GET_APP: &[u8] = &[
    0x0c, 0x00, 0x9f, 0xe5, 0x00, 0x00, 0x8f, 0xe0, 0x04, 0x00, 0x10, 0xe5, 0xc0, 0x00, 0x90, 0xe5,
    0x10, 0xff, 0x2f, 0xe1, 0x84, 0x22, 0xff, 0xff,
];
const MOD: &str = "6c655333625da5b0f0ef78bff6ec23533572fb69f695afa3c756b885bef8577d";
const MIF: &str = "1ee6a767df2833f4139ea3789d0be8507c34a8f5b7242d13733e8ec7fb7279fe";
const AT: u32 = 0x201d4;
const END: u32 = 0x202a8;
const APP: u32 = 0x10001000;
const OUT: u32 = 0x10000200;
struct Fixture {
    m: Machine<BackendPadrao>,
    display: u32,
}
impl Fixture {
    fn new(w: u16, h: u16, template_change: Option<usize>) -> Self {
        let mut data = vec![0; 0x11000];
        for (at, word) in [
            (0, 0xe3a00000u32),
            (4, 0xe12fff1e),
            (0x40, 0xe3a00000),
            (0x44, 0xe12fff1e),
            (0x80, 0xe59f2008),
            (0x84, 0xe5812000),
            (0x88, 0xe3a00000),
            (0x8c, 0xe12fff1e),
            (0x90, 0x10003400),
            (0xa0, 0xe3a00014),
            (0xa4, 0xe12fff1e),
            (0xb0, 0xe59f1010),
            (0xb4, 0xe5912000),
            (0xb8, 0xe2822001),
            (0xbc, 0xe5812000),
            (0xc0, 0xe3a00000),
            (0xc4, 0xe12fff1e),
            (0xc8, 0x10003600),
        ] {
            data[at..at + 4].copy_from_slice(&word.to_le_bytes());
        }
        data[(AT - 0x10000) as usize..(AT - 0x10000) as usize + OLD.len()].copy_from_slice(OLD);
        data[0xdd70..0xdd70 + GET_APP.len()].copy_from_slice(GET_APP);
        if let Some(n) = template_change {
            data[(AT - 0x10000) as usize + n] ^= 0x10;
        }
        let image = ModImage::parse(data).unwrap();
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests/rice-public-fixture");
        let storage = StoragePaths::from_root(root.join("runtime"));
        let mut m = Machine::new_with_storage_geometry(
            BackendPadrao::new().unwrap(),
            loader::load_with(&image, &[]).unwrap(),
            &root,
            &storage,
            None,
            DisplayGeometry {
                width: w,
                height: h,
                depth: 16,
            },
        )
        .unwrap();
        assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
        let mut f = Self { m, display: 0 };
        assert_eq!(
            f.call(Interface::Helpers, "malloc", [0x4000, 0, 0, 0]),
            loader::HEAP_BASE
        );
        assert_eq!(
            f.call(
                Interface::Shell,
                "CreateInstance",
                [loader::OBJECT_BASE, 0x01001001, OUT, 0]
            ),
            0
        );
        f.display = f.m.cpu().read_u32(OUT).unwrap();
        f.m.cpu_mut().write_u32(APP + 40, f.display).unwrap();
        let module = f.m.module().out_module;
        for (at, word) in [
            (module, 0x10000100),
            (0x10000100, 0x10000140),
            (0x10000148, 0x10040),
            (APP, 0x10000180),
            (0x10000188, 0x10040),
        ] {
            f.m.cpu_mut().write_u32(at, word).unwrap();
        }
        f
    }
    fn call(&mut self, i: Interface, name: &str, args: [u32; 4]) -> u32 {
        let slot = (0..256).find(|s| i.method(*s) == Some(name)).unwrap();
        match self.m.call_guest(aee::encode(i, slot), args, 1000).unwrap() {
            Outcome::Returned { code } => code,
            o => panic!("{name}: {o:?}"),
        }
    }
    fn activate(&mut self) {
        assert!(self.m.set_handset_runtime_identity(MOD, MIF));
        assert!(
            format!("{:?}", self.m.create_applet(1, 1000).unwrap()).starts_with("Called { code: 0")
        );
        assert_eq!(
            self.m.start_applet(APP, 1, 1000).unwrap(),
            Outcome::Returned { code: 0 }
        );
    }
    fn getter(&mut self, budget: u64) -> Outcome {
        self.m.call_guest(AT, [0; 4], budget).unwrap()
    }
    fn bitmap(&mut self) -> u32 {
        assert_eq!(
            self.call(
                Interface::Display,
                "GetDeviceBitmap",
                [self.display, OUT, 0, 0]
            ),
            0
        );
        self.m.cpu().read_u32(OUT).unwrap()
    }
    fn refs(&mut self, b: u32) -> u32 {
        let n = self.call(Interface::Bitmap, "AddRef", [b, 0, 0, 0]);
        assert_eq!(self.call(Interface::Bitmap, "Release", [b, 0, 0, 0]), n - 1);
        n - 1
    }
}
#[test]
fn real_public_rows_preserve_gameplay_fields_cache_and_arm_caller_abi() {
    let mut f = Fixture::new(120, 146, None);
    let b = f.bitmap();
    let public = f.m.dump(b, 36).unwrap();
    let baseline = f.refs(b);
    f.m.cpu_mut()
        .write_mem(APP + 0x23c, &[0xa5; 0x234])
        .unwrap();
    f.m.cpu_mut().write_u32(APP + 40, f.display).unwrap();
    let before = f.m.dump(APP + 0x23c, 0x234).unwrap();
    f.activate();
    assert_eq!(f.m.dump(AT, 164).unwrap(), FIXED);
    let sp = f.m.cpu().read_reg(Reg::Sp);
    let regs = [
        Reg::R4,
        Reg::R5,
        Reg::R6,
        Reg::R7,
        Reg::R8,
        Reg::R9,
        Reg::R10,
        Reg::R11,
    ];
    for (n, r) in regs.into_iter().enumerate() {
        f.m.cpu_mut().write_reg(r, 0xabc00000 + n as u32);
    }
    assert_eq!(f.getter(1000), Outcome::Returned { code: 0 });
    let mut expected = before;
    expected[0x24..0x28].copy_from_slice(&3u32.to_le_bytes());
    let pixels = u32::from_le_bytes(public[8..12].try_into().unwrap());
    let pitch = u16::from_le_bytes(public[24..26].try_into().unwrap());
    expected[0x2c..0x30].copy_from_slice(&pixels.to_le_bytes());
    expected[0x30..0x34].copy_from_slice(&(pixels + u32::from(pitch)).to_le_bytes());
    assert_eq!(f.m.dump(APP + 0x23c, 0x234).unwrap(), expected);
    for (n, r) in regs.into_iter().enumerate() {
        assert_eq!(f.m.cpu().read_reg(r), 0xabc00000 + n as u32);
    }
    assert_eq!(f.m.cpu().read_reg(Reg::Sp), sp);
    assert_eq!(f.refs(b), baseline);
}
#[test]
fn repeated_getters_use_public_pitch_for_each_manual_geometry_without_ref_growth() {
    for (w, h) in [(120, 146), (176, 220), (240, 320)] {
        let mut f = Fixture::new(w, h, None);
        let b = f.bitmap();
        let before = f.refs(b);
        f.activate();
        for _ in 0..12 {
            assert_eq!(f.getter(1000), Outcome::Returned { code: 0 });
        }
        assert_eq!(f.refs(b), before);
        assert_eq!(
            f.m.cpu().read_u32(APP + 0x26c).unwrap() - f.m.cpu().read_u32(APP + 0x268).unwrap(),
            u32::from(w) * 2
        );
    }
}
#[test]
fn public_display_color_calibration_updates_the_same_borrowed_pixels() {
    let mut f = Fixture::new(120, 146, None);
    f.activate();
    assert_eq!(f.getter(1000), Outcome::Returned { code: 0 });
    let pixels = f.m.cpu().read_u32(APP + 0x268).unwrap();
    for (x, color, expected) in [
        (0, 0x0000ff00, 0xf800u16),
        (1, 0x00ff0000, 0x07e0),
        (2, 0xff000000, 0x001f),
    ] {
        f.m.cpu_mut()
            .write_mem(OUT, &[x as u8, 0, 0, 0, 1, 0, 1, 0])
            .unwrap();
        let sp = f.m.cpu().read_reg(Reg::Sp);
        f.m.cpu_mut().write_u32(sp, 2).unwrap();
        assert_eq!(
            f.call(
                Interface::Display,
                "DrawRect",
                [f.display, OUT, u32::MAX, color]
            ),
            0
        );
        assert_eq!(f.m.dump(pixels + x * 2, 2).unwrap(), expected.to_le_bytes());
    }
}
#[test]
fn preapp_loader_integrity_phase_and_template_remain_original() {
    let mut f = Fixture::new(120, 146, None);
    assert!(f.m.set_handset_runtime_identity(MOD, MIF));
    f.m.call_guest(0x10000, [0; 4], 1000).unwrap();
    assert_eq!(f.m.dump(AT, 164).unwrap(), OLD);
    f.activate();
    assert_eq!(f.m.module().mem.read(AT, 164).unwrap(), OLD);
    assert_eq!(f.m.dump(AT, 164).unwrap(), FIXED);
}
#[test]
fn each_unknown_template_or_live_byte_prevents_any_rewrite() {
    let mut f = Fixture::new(120, 146, None);
    f.activate();
    for n in 0..164 {
        let mut changed = OLD.to_vec();
        changed[n] ^= 0x10;
        f.m.cpu_mut().write_mem(AT, &changed).unwrap();
        f.m.call_guest(0x10000, [0; 4], 1000).unwrap();
        assert_eq!(f.m.dump(AT, 164).unwrap(), changed);
    }
    for n in 0..164 {
        let mut f = Fixture::new(120, 146, Some(n));
        f.m.set_handset_runtime_identity(MOD, MIF);
        f.m.cpu_mut().write_mem(AT, OLD).unwrap();
        f.m.create_applet(1, 1000).unwrap();
        assert_eq!(f.m.dump(AT, 164).unwrap(), OLD);
    }
}
#[test]
fn neither_hash_alone_can_enable_the_port() {
    for (m, f) in [(MOD, "unknown"), ("unknown", MIF)] {
        let mut x = Fixture::new(120, 146, None);
        assert!(!x.m.set_handset_runtime_identity(m, f));
        x.m.create_applet(1, 1000).unwrap();
        assert_eq!(x.m.dump(AT, 164).unwrap(), OLD);
    }
}
#[test]
fn qi_failure_releases_bitmap_and_preserves_all_gameplay_and_cached_fields() {
    let mut f = Fixture::new(120, 146, None);
    f.activate();
    for (at, word) in [
        (0x10003000, 0x10003100),
        (0x10003140, 0x10080),
        (0x10003400, 0x10003500),
        (0x10003508, 0x100a0),
        (0x10003504, 0x100b0),
        (0x10003600, 0),
        (APP + 40, 0x10003000),
    ] {
        f.m.cpu_mut().write_u32(at, word).unwrap();
    }
    let before = f.m.dump(APP + 0x23c, 0x234).unwrap();
    assert_eq!(f.getter(1000), Outcome::Returned { code: 20 });
    assert_eq!(f.m.cpu().read_u32(0x10003600).unwrap(), 1);
    assert_eq!(f.m.dump(APP + 0x23c, 0x234).unwrap(), before);
}
#[test]
fn old_saved_initializer_upgrades_and_new_sdk_midcall_resumes() {
    let mut f = Fixture::new(120, 146, None);
    f.activate();
    f.m.cpu_mut().write_mem(AT, OLD).unwrap();
    let old = f.m.grava_estado();
    f.m.call_guest(0x10000, [0; 4], 1000).unwrap();
    f.m.restaura_estado(&old).unwrap();
    assert_eq!(f.m.dump(AT, 164).unwrap(), FIXED);
    let b = f.bitmap();
    let baseline = f.refs(b);
    assert_eq!(f.getter(1), Outcome::Budget);
    let saved = f.m.grava_estado();
    f.m.restaura_estado(&saved).unwrap();
    for _ in 0..20 {
        if f.m
            .advance(1000)
            .unwrap()
            .iter()
            .any(|o| matches!(o, Outcome::Returned { .. }))
        {
            break;
        }
    }
    assert_eq!(
        f.m.cpu().read_u32(APP + 0x268).unwrap(),
        f.m.cpu().read_u32(b + 8).unwrap()
    );
    assert_eq!(f.refs(b), baseline);
}
#[test]
fn changed_saved_initializer_or_preserved_app_getter_is_rejected_atomically() {
    use zeebx::save_state::{self, Leitor};
    for at in [AT + 60, 0x1dd74] {
        let mut f = Fixture::new(120, 146, None);
        f.activate();
        let saved = f.m.grava_estado();
        let reader = Leitor::abre(&saved).unwrap();
        let base =
            f.m.module()
                .mem
                .regions()
                .iter()
                .find(|r| r.name == "module")
                .unwrap()
                .base;
        let sections = reader
            .nomes()
            .into_iter()
            .map(|name| {
                let mut bytes = reader.secao(name).unwrap().to_vec();
                if name == "mem.module" {
                    bytes[(at - base) as usize] ^= 0x10;
                }
                (name.to_owned(), bytes)
            })
            .collect::<Vec<_>>();
        f.m.cpu_mut().write_reg(Reg::R4, 0xdecafbad);
        let before = f.m.grava_estado();
        assert!(
            f.m.restaura_estado(&save_state::escreve(&sections))
                .is_err()
        );
        assert_eq!(f.m.grava_estado(), before);
    }
}
#[test]
fn old_mid_initializer_pc_and_return_lr_cannot_be_upgraded_in_place() {
    for (reg, at) in [(Reg::Pc, AT + 12), (Reg::Lr, END - 4)] {
        let mut f = Fixture::new(120, 146, None);
        f.activate();
        f.m.cpu_mut().write_mem(AT, OLD).unwrap();
        f.m.cpu_mut().write_reg(reg, at | 1);
        let old = f.m.grava_estado();
        f.m.cpu_mut().write_reg(reg, 0);
        f.m.cpu_mut().write_mem(AT, FIXED).unwrap();
        let before = f.m.grava_estado();
        assert!(f.m.restaura_estado(&old).is_err());
        assert_eq!(f.m.grava_estado(), before);
    }
}
