use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend, Reg},
    handset::types::DisplayGeometry,
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    storage::StoragePaths,
};
const ONE: &[u8] = &[
    0x70, 0x40, 0x2d, 0xe9, 0x00, 0x60, 0xa0, 0xe1, 0x10, 0x00, 0xb1, 0xe5, 0x08, 0x50, 0x91, 0xe5,
    0x00, 0x10, 0x90, 0xe5, 0x02, 0x40, 0xa0, 0xe1, 0x10, 0x20, 0x91, 0xe5, 0x40, 0xd0, 0x4d, 0xe2,
    0x04, 0x10, 0x8d, 0xe2, 0x0f, 0xe0, 0xa0, 0xe1, 0x12, 0xff, 0x2f, 0xe1, 0x98, 0x02, 0x9f, 0xe5,
    0x00, 0x00, 0x8f, 0xe0, 0x04, 0x00, 0x10, 0xe5, 0x14, 0x20, 0xa0, 0xe3, 0x04, 0x30, 0x90, 0xe5,
    0x04, 0x00, 0xa0, 0xe1, 0x00, 0x10, 0xa0, 0xe3, 0x0f, 0xe0, 0xa0, 0xe1, 0x13, 0xff, 0x2f, 0xe1,
    0xb4, 0x00, 0xdd, 0xe1, 0x01, 0x10, 0xa0, 0xe3, 0x03, 0x20, 0xa0, 0xe3, 0xb4, 0x00, 0xc4, 0xe1,
    0xb6, 0x00, 0xdd, 0xe1, 0xb6, 0x00, 0xc4, 0xe1, 0x0d, 0x30, 0xd6, 0xe5, 0x10, 0x00, 0xa0, 0xe3,
    0x0b, 0x00, 0x53, 0xe3, 0x03, 0xf1, 0x8f, 0x90, 0x6a, 0x00, 0x00, 0xea, 0x69, 0x00, 0x00, 0xea,
    0x09, 0x00, 0x00, 0xea, 0x0f, 0x00, 0x00, 0xea, 0x14, 0x00, 0x00, 0xea, 0x13, 0x00, 0x00, 0xea,
    0x21, 0x00, 0x00, 0xea, 0x53, 0x00, 0x00, 0xea, 0x58, 0x00, 0x00, 0xea, 0x15, 0x00, 0x00, 0xea,
    0x78, 0x00, 0x00, 0xea, 0x1c, 0x00, 0x00, 0xea, 0x7f, 0x00, 0x00, 0xea,
];
const TWO: &[u8] = &[
    0x70, 0x40, 0x2d, 0xe9, 0x00, 0x60, 0xa0, 0xe1, 0x10, 0x00, 0xb1, 0xe5, 0x08, 0x50, 0x91, 0xe5,
    0x00, 0x10, 0x90, 0xe5, 0x02, 0x40, 0xa0, 0xe1, 0x10, 0x20, 0x91, 0xe5, 0x40, 0xd0, 0x4d, 0xe2,
    0x04, 0x10, 0x8d, 0xe2, 0x0f, 0xe0, 0xa0, 0xe1, 0x12, 0xff, 0x2f, 0xe1, 0x98, 0x02, 0x9f, 0xe5,
    0x00, 0x00, 0x8f, 0xe0, 0x04, 0x00, 0x10, 0xe5, 0x14, 0x20, 0xa0, 0xe3, 0x04, 0x30, 0x90, 0xe5,
    0x04, 0x00, 0xa0, 0xe1, 0x00, 0x10, 0xa0, 0xe3, 0x0f, 0xe0, 0xa0, 0xe1, 0x13, 0xff, 0x2f, 0xe1,
    0xb4, 0x00, 0xdd, 0xe1, 0x01, 0x10, 0xa0, 0xe3, 0x03, 0x20, 0xa0, 0xe3, 0xb4, 0x00, 0xc4, 0xe1,
    0xb6, 0x00, 0xdd, 0xe1, 0xb6, 0x00, 0xc4, 0xe1, 0x0d, 0x30, 0xd6, 0xe5, 0x10, 0x00, 0xa0, 0xe3,
    0x0b, 0x00, 0x53, 0xe3, 0x03, 0xf1, 0x8f, 0x90, 0x6a, 0x00, 0x00, 0xea, 0x69, 0x00, 0x00, 0xea,
    0x09, 0x00, 0x00, 0xea, 0x0f, 0x00, 0x00, 0xea, 0x14, 0x00, 0x00, 0xea, 0x13, 0x00, 0x00, 0xea,
    0x21, 0x00, 0x00, 0xea, 0x53, 0x00, 0x00, 0xea, 0x58, 0x00, 0x00, 0xea, 0x15, 0x00, 0x00, 0xea,
    0x78, 0x00, 0x00, 0xea, 0x1c, 0x00, 0x00, 0xea, 0x7f, 0x00, 0x00, 0xea,
];
const FIX_ONE: &[u8] = &[
    0x01, 0xc0, 0x8f, 0xe2, 0x1c, 0xff, 0x2f, 0xe1, 0xf0, 0xb5, 0x85, 0xb0, 0x14, 0x00, 0x8d, 0x69,
    0x00, 0x20, 0x20, 0x60, 0x60, 0x60, 0xa0, 0x60, 0xe0, 0x60, 0x20, 0x61, 0xc0, 0x46, 0xc0, 0x46,
    0x00, 0x26, 0x00, 0x27, 0x00, 0x96, 0x01, 0x96, 0x28, 0x00, 0x69, 0x46, 0x03, 0x68, 0x1b, 0x6c,
    0x98, 0x47, 0x02, 0x90, 0x00, 0x28, 0x23, 0xd1, 0x00, 0x9e, 0x00, 0x2e, 0x1e, 0xd0, 0x30, 0x00,
    0x18, 0x49, 0x01, 0xaa, 0x03, 0x68, 0x9b, 0x68, 0x98, 0x47, 0x02, 0x90, 0x00, 0x28, 0x17, 0xd1,
    0x01, 0x9f, 0x00, 0x2f, 0x12, 0xd0, 0x38, 0x7f, 0x10, 0x28, 0x0f, 0xd1, 0x78, 0x7f, 0x10, 0x28,
    0x0c, 0xd1, 0xb8, 0x68, 0x00, 0x28, 0x09, 0xd0, 0x20, 0x60, 0x78, 0x69, 0x60, 0x60, 0x38, 0x8b,
    0x20, 0x81, 0x0d, 0x48, 0x60, 0x81, 0x00, 0x20, 0x02, 0x90, 0x01, 0xe0, 0x14, 0x20, 0x02, 0x90,
    0x00, 0x2f, 0x03, 0xd0, 0x38, 0x00, 0x03, 0x68, 0x5b, 0x68, 0x98, 0x47, 0x00, 0x2e, 0x03, 0xd0,
    0x30, 0x00, 0x03, 0x68, 0x5b, 0x68, 0x98, 0x47, 0x02, 0x98, 0x05, 0xb0, 0xf0, 0xbc, 0x08, 0xbc,
    0x18, 0x47, 0xc0, 0x46, 0x45, 0x10, 0x00, 0x01, 0x10, 0x41, 0x00, 0x00,
];
const FIX_TWO: &[u8] = &[
    0x01, 0xc0, 0x8f, 0xe2, 0x1c, 0xff, 0x2f, 0xe1, 0xf0, 0xb5, 0x85, 0xb0, 0x14, 0x00, 0x8d, 0x69,
    0x00, 0x20, 0x20, 0x60, 0x60, 0x60, 0xa0, 0x60, 0xe0, 0x60, 0x20, 0x61, 0xc0, 0x46, 0xc0, 0x46,
    0x00, 0x26, 0x00, 0x27, 0x00, 0x96, 0x01, 0x96, 0x28, 0x00, 0x69, 0x46, 0x03, 0x68, 0x1b, 0x6c,
    0x98, 0x47, 0x02, 0x90, 0x00, 0x28, 0x23, 0xd1, 0x00, 0x9e, 0x00, 0x2e, 0x1e, 0xd0, 0x30, 0x00,
    0x18, 0x49, 0x01, 0xaa, 0x03, 0x68, 0x9b, 0x68, 0x98, 0x47, 0x02, 0x90, 0x00, 0x28, 0x17, 0xd1,
    0x01, 0x9f, 0x00, 0x2f, 0x12, 0xd0, 0x38, 0x7f, 0x10, 0x28, 0x0f, 0xd1, 0x78, 0x7f, 0x10, 0x28,
    0x0c, 0xd1, 0xb8, 0x68, 0x00, 0x28, 0x09, 0xd0, 0x20, 0x60, 0x78, 0x69, 0x60, 0x60, 0x38, 0x8b,
    0x20, 0x81, 0x0d, 0x48, 0x60, 0x81, 0x00, 0x20, 0x02, 0x90, 0x01, 0xe0, 0x14, 0x20, 0x02, 0x90,
    0x00, 0x2f, 0x03, 0xd0, 0x38, 0x00, 0x03, 0x68, 0x5b, 0x68, 0x98, 0x47, 0x00, 0x2e, 0x03, 0xd0,
    0x30, 0x00, 0x03, 0x68, 0x5b, 0x68, 0x98, 0x47, 0x02, 0x98, 0x05, 0xb0, 0xf0, 0xbc, 0x08, 0xbc,
    0x18, 0x47, 0xc0, 0x46, 0x45, 0x10, 0x00, 0x01, 0x10, 0x41, 0x00, 0x00,
];
#[derive(Clone, Copy)]
struct Case {
    m: &'static str,
    f: &'static str,
    two: bool,
}
const CASES: [Case; 1] = [Case {
    m: "87f5f3791c500e8b1707f7ab39f331e434231bea108e751afdfe905ca27d459d",
    f: "3f336a8f7bbabd533794a1f94885c035e087d063c261dd3203309dd2299c67ec",
    two: true,
}];
impl Case {
    fn at(self) -> u32 {
        if self.two { 0x10002ee8 } else { 0x10002ee8 }
    }
    fn end(self) -> u32 {
        if self.two { 0x100031b4 } else { 0x100031b4 }
    }
    fn original(self) -> &'static [u8] {
        if self.two { TWO } else { ONE }
    }
    fn fixed(self) -> &'static [u8] {
        if self.two { FIX_TWO } else { FIX_ONE }
    }
}
const OUT: u32 = 0x10001000;
const DESC: u32 = 0x10002000;
const CONTEXT: u32 = 0x10002100;
struct Fixture {
    m: Machine<BackendPadrao>,
    case: Case,
    display: u32,
}
impl Fixture {
    fn new(case: Case, w: u16, h: u16) -> Self {
        let mut data = vec![0; 0x200];
        for (at, word) in [
            (0, 0xe3a00000u32),
            (4, 0xe12fff1e),
            (0x40, 0xe3a00000),
            (0x44, 0xe12fff1e),
            (0x80, 0xe59f2008),
            (0x84, 0xe5812000),
            (0x88, 0xe3a00000),
            (0x8c, 0xe12fff1e),
            (0x90, 0x10000b00),
            (0xa0, 0xe3a00014),
            (0xa4, 0xe12fff1e),
            (0xb0, 0xe59f1010),
            (0xb4, 0xe5912000),
            (0xb8, 0xe2822001),
            (0xbc, 0xe5812000),
            (0xc0, 0xe3a00000),
            (0xc4, 0xe12fff1e),
            (0xc8, 0x10000d00),
        ] {
            data[at..at + 4].copy_from_slice(&word.to_le_bytes());
        }
        let image = ModImage::parse(data).unwrap();
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests/dark-public-fixture");
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
        let mut f = Self {
            m,
            case,
            display: 0,
        };
        if case.two {
            assert_eq!(
                f.call(Interface::Helpers, "malloc", [0x24100, 0, 0, 0], &[]),
                loader::HEAP_BASE
            );
            f.m.cpu_mut().write_u32(0x10000000, 0x10000).unwrap();
            f.m.cpu_mut()
                .write_u32(0x10000004, loader::HELPERS_BASE)
                .unwrap();
        } else {
            assert_eq!(
                f.call(Interface::Helpers, "malloc", [0x337f0, 0, 0, 0], &[]),
                loader::HEAP_BASE
            );
            assert_eq!(
                f.call(Interface::Helpers, "malloc", [0x24e0, 0, 0, 0], &[]),
                0x100337f8
            );
            f.m.cpu_mut()
                .write_u32(0x100337f8, loader::HELPERS_BASE)
                .unwrap();
            f.m.cpu_mut().write_u32(0x100337fc, 0x100337c8).unwrap();
        }
        f.display = f.call(
            Interface::Shell,
            "CreateInstance",
            [loader::OBJECT_BASE, 0x01001001, OUT, 0],
            &[],
        );
        assert_eq!(f.display, 0);
        f.display = f.m.cpu().read_u32(OUT).unwrap();
        f.m.cpu_mut().write_u32(CONTEXT + 24, f.display).unwrap();
        f.m.cpu_mut().write_mem(case.at(), case.original()).unwrap();
        let module = f.m.module().out_module;
        f.m.cpu_mut().write_u32(module, 0x10000100).unwrap();
        f.m.cpu_mut().write_u32(0x10000100, 0x10000200).unwrap();
        f.m.cpu_mut().write_u32(0x10000208, 0x10040).unwrap();
        f
    }
    fn call(&mut self, i: Interface, name: &str, args: [u32; 4], extra: &[u32]) -> u32 {
        let slot = (0..256).find(|s| i.method(*s) == Some(name)).unwrap();
        let sp = self.m.cpu().read_reg(Reg::Sp);
        let next = (sp - 32) & !7;
        self.m.cpu_mut().write_reg(Reg::Sp, next);
        for (n, v) in extra.iter().enumerate() {
            self.m.cpu_mut().write_u32(next + n as u32 * 4, *v).unwrap();
        }
        let result = self.m.call_guest(aee::encode(i, slot), args, 1000).unwrap();
        self.m.cpu_mut().write_reg(Reg::Sp, sp);
        match result {
            Outcome::Returned { code } => code,
            o => panic!("{name}: {o:?}"),
        }
    }
    fn activate(&mut self) {
        assert!(
            self.m
                .set_handset_runtime_identity(self.case.m, self.case.f)
        );
        let result = self.m.create_applet(0x01000001, 1000).unwrap();
        assert!(format!("{result:?}").starts_with("Called { code: 0"));
    }
    fn getter(&mut self, budget: u64, display: u32) -> Outcome {
        let sp = self.m.cpu().read_reg(Reg::Sp);
        let next = (sp - 32) & !7;
        self.m.cpu_mut().write_reg(Reg::Sp, next);
        for (n, v) in [OUT + 16, OUT + 32, DESC].into_iter().enumerate() {
            self.m.cpu_mut().write_u32(next + n as u32 * 4, v).unwrap();
        }
        self.m.cpu_mut().write_u32(CONTEXT + 24, display).unwrap();
        let args = if self.case.two {
            [OUT + 64, CONTEXT, DESC, 0]
        } else {
            [0x04000200, display, 0, 0]
        };
        let result = self.m.call_guest(self.case.at(), args, budget).unwrap();
        if result != Outcome::Budget {
            self.m.cpu_mut().write_reg(Reg::Sp, sp);
        }
        result
    }
    fn bitmap(&mut self) -> u32 {
        assert_eq!(
            self.call(
                Interface::Display,
                "GetDeviceBitmap",
                [self.display, OUT, 0, 0],
                &[]
            ),
            0
        );
        self.m.cpu().read_u32(OUT).unwrap()
    }
    fn refs(&mut self, b: u32) -> u32 {
        let n = self.call(Interface::Bitmap, "AddRef", [b, 0, 0, 0], &[]);
        assert_eq!(
            self.call(Interface::Bitmap, "Release", [b, 0, 0, 0], &[]),
            n - 1
        );
        n - 1
    }
}
#[test]
fn exact_pair_get_real_public_pixels_and_preserve_caller_abi() {
    for case in CASES {
        let mut f = Fixture::new(case, 240, 320);
        let b = f.bitmap();
        let public = f.m.dump(b, 36).unwrap();
        let baseline = f.refs(b);
        f.activate();
        assert_eq!(f.m.dump(case.at(), 172).unwrap(), case.fixed());
        f.m.cpu_mut().write_mem(DESC - 8, &[0xa5; 48]).unwrap();
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
        assert_eq!(f.getter(1000, f.display), Outcome::Returned { code: 0 });
        let d = f.m.dump(DESC, 28).unwrap();
        assert_eq!(&d[..4], &public[8..12]);
        assert_eq!(&d[4..10], &public[20..26]);
        assert_eq!(&d[10..12], &[0x10, 0x41]);
        assert_eq!(f.m.dump(DESC - 8, 8).unwrap(), [0xa5; 8]);
        assert_eq!(f.m.dump(DESC + 28, 12).unwrap(), [0xa5; 12]);
        assert_eq!(&d[12..], &[vec![0; 8], vec![0xa5; 8]].concat()[..]);
        for (n, r) in regs.into_iter().enumerate() {
            assert_eq!(f.m.cpu().read_reg(r), 0xabc00000 + n as u32);
        }
        assert_eq!(f.refs(b), baseline);
    }
}
#[test]
fn repeated_getters_follow_real_manual_geometry_without_reference_growth() {
    for case in [CASES[0]] {
        for (w, h) in [(176, 220), (320, 240)] {
            let mut f = Fixture::new(case, w, h);
            let b = f.bitmap();
            let before = f.refs(b);
            f.activate();
            for _ in 0..12 {
                assert_eq!(f.getter(1000, f.display), Outcome::Returned { code: 0 });
            }
            assert_eq!(f.refs(b), before);
            assert_eq!(
                f.m.dump(DESC + 4, 6).unwrap(),
                [w.to_le_bytes(), h.to_le_bytes(), (w * 2).to_le_bytes()].concat()
            );
        }
    }
}
#[test]
fn original_code_waits_until_module_integrity_load_phase_has_ended() {
    for case in [CASES[0]] {
        let mut f = Fixture::new(case, 240, 320);
        assert!(f.m.set_handset_runtime_identity(case.m, case.f));
        assert_eq!(
            f.m.call_guest(0x10000, [0; 4], 1000).unwrap(),
            Outcome::Returned { code: 0 }
        );
        assert_eq!(f.m.dump(case.at(), 172).unwrap(), case.original());
        f.activate();
        assert_eq!(f.m.dump(case.at(), 172).unwrap(), case.fixed());
    }
}
#[test]
fn every_unknown_live_opcode_byte_is_left_untouched() {
    for case in [CASES[0]] {
        let mut f = Fixture::new(case, 240, 320);
        f.activate();
        for at in 0..172 {
            let mut changed = case.original().to_vec();
            changed[at] ^= 0x10;
            f.m.cpu_mut().write_mem(case.at(), &changed).unwrap();
            assert_eq!(
                f.m.call_guest(0x10000, [0; 4], 1000).unwrap(),
                Outcome::Returned { code: 0 }
            );
            assert_eq!(f.m.dump(case.at(), 172).unwrap(), changed);
        }
    }
}
#[test]
fn neither_single_hash_can_enable_the_guest_port() {
    for case in [CASES[0]] {
        for (mif, module) in [(case.f, "unknown"), ("unknown", case.m)] {
            let mut f = Fixture::new(case, 240, 320);
            assert!(!f.m.set_handset_runtime_identity(module, mif));
            f.m.create_applet(1, 1000).unwrap();
            assert_eq!(f.m.dump(case.at(), 172).unwrap(), case.original());
        }
    }
}
#[test]
fn qi_failure_is_propagated_and_acquired_bitmap_is_released() {
    for case in [CASES[0]] {
        let mut f = Fixture::new(case, 240, 320);
        f.activate();
        for (at, value) in [
            (0x10000900, 0x10000a00),
            (0x10000a40, 0x10080),
            (0x10000b00, 0x10000c00),
            (0x10000c08, 0x100a0),
            (0x10000c04, 0x100b0),
            (0x10000d00, 0),
        ] {
            f.m.cpu_mut().write_u32(at, value).unwrap();
        }
        f.m.cpu_mut().write_mem(DESC, &[0xa5; 28]).unwrap();
        assert_eq!(f.getter(1000, 0x10000900), Outcome::Returned { code: 20 });
        assert_eq!(f.m.cpu().read_u32(0x10000d00).unwrap(), 1);
        assert_eq!(
            f.m.dump(DESC, 28).unwrap(),
            [vec![0; 20], vec![0xa5; 8]].concat()
        );
    }
}
#[test]
fn old_original_saved_getter_is_upgraded_before_restore() {
    for case in [CASES[0]] {
        let mut f = Fixture::new(case, 240, 320);
        f.activate();
        f.m.cpu_mut().write_mem(case.at(), case.original()).unwrap();
        let old = f.m.grava_estado();
        f.m.call_guest(0x10000, [0; 4], 1000).unwrap();
        f.m.restaura_estado(&old).unwrap();
        assert_eq!(f.m.dump(case.at(), 172).unwrap(), case.fixed());
        assert_eq!(f.getter(1000, f.display), Outcome::Returned { code: 0 });
    }
}
#[test]
fn changed_saved_getter_is_rejected_atomically() {
    use zeebx::save_state::{self, Leitor};
    for case in [CASES[0]] {
        let mut f = Fixture::new(case, 240, 320);
        f.activate();
        let saved = f.m.grava_estado();
        let reader = Leitor::abre(&saved).unwrap();
        let sections = reader
            .nomes()
            .into_iter()
            .map(|name| {
                let mut bytes = reader.secao(name).unwrap().to_vec();
                if name == "mem.heap" {
                    bytes[(case.at() - loader::HEAP_BASE) as usize + 60] ^= 0x10;
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
fn legacy_mid_getter_pc_and_sdk_return_lr_are_rejected_with_thumb_bit() {
    for case in [CASES[0]] {
        for (reg, at) in [(Reg::Pc, case.at() + 10), (Reg::Lr, case.end() - 4)] {
            let mut f = Fixture::new(case, 240, 320);
            f.activate();
            f.m.cpu_mut().write_mem(case.at(), case.original()).unwrap();
            f.m.cpu_mut().write_reg(reg, at | 1);
            let old = f.m.grava_estado();
            f.m.cpu_mut().write_reg(reg, 0);
            f.m.cpu_mut().write_mem(case.at(), case.fixed()).unwrap();
            let before = f.m.grava_estado();
            assert!(f.m.restaura_estado(&old).is_err());
            assert_eq!(f.m.grava_estado(), before);
        }
    }
}
#[test]
fn new_saved_public_getter_can_resume_during_sdk_call() {
    for case in [CASES[0]] {
        let mut f = Fixture::new(case, 240, 320);
        f.activate();
        let b = f.bitmap();
        let baseline = f.refs(b);
        assert_eq!(f.getter(1, f.display), Outcome::Budget);
        let saved = f.m.grava_estado();
        f.m.restaura_estado(&saved).unwrap();
        for _ in 0..20 {
            let out = f.m.advance(1000).unwrap();
            if out.iter().any(|o| matches!(o, Outcome::Returned { .. })) {
                break;
            }
        }
        assert_eq!(
            f.m.cpu().read_u32(DESC).unwrap(),
            f.m.cpu().read_u32(b + 8).unwrap()
        );
        assert_eq!(f.refs(b), baseline);
    }
}

#[test]
fn unpublished_loaded_image_is_rejected_atomically_in_active_save() {
    for case in [CASES[0]] {
        let mut f = Fixture::new(case, 240, 320);
        f.activate();
        let marker = if case.two { 0x10000004 } else { 0x100337f8 };
        f.m.cpu_mut().write_u32(marker, 0xeafffffe).unwrap();
        let old = f.m.grava_estado();
        f.m.cpu_mut()
            .write_u32(marker, loader::HELPERS_BASE)
            .unwrap();
        let before = f.m.grava_estado();
        assert!(f.m.restaura_estado(&old).is_err());
        assert_eq!(f.m.grava_estado(), before);
    }
}
