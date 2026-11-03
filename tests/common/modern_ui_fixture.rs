use std::{io::Read, path::PathBuf};
use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend, Reg},
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    storage::StoragePaths,
    video::font::Font,
};
const OUT: u32 = loader::HEAP_BASE + 0x10000;
const TEXT: u32 = OUT + 128;
struct F {
    m: Machine<BackendPadrao>,
    root: PathBuf,
}
impl F {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests/wave21-delivery-notes/runtime-tests")
            .join(format!(
                "modern-font-test-{}-{}",
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
        let mut f = Self { m, root };
        let reserved = f.alloc(0x20000);
        assert!(reserved <= OUT && reserved + 0x20000 > OUT + 4096);
        f
    }
    fn alloc(&mut self, size: u32) -> u32 {
        let slot = (0..256)
            .find(|&s| Interface::Helpers.method(s) == Some("malloc"))
            .unwrap();
        let p = self.run(aee::encode(Interface::Helpers, slot), [size, 0, 0, 0]);
        assert_ne!(p, 0);
        p
    }
    fn run(&mut self, entry: u32, args: [u32; 4]) -> u32 {
        match self.m.call_guest(entry, args, 500000).unwrap() {
            Outcome::Returned { code } => code,
            o => panic!("{entry:#x}: {o:?}"),
        }
    }
    fn read(&self, p: u32) -> u32 {
        self.m.cpu().read_u32(p).unwrap()
    }
    fn put(&mut self, p: u32, v: u32) {
        self.m.cpu_mut().write_u32(p, v).unwrap();
    }
    fn call(&mut self, o: u32, slot: u32, a: [u32; 3]) -> u32 {
        let vt = self.read(o);
        let entry = self.read(vt + slot * 4);
        self.run(entry, [o, a[0], a[1], a[2]])
    }
    fn extra(&mut self, o: u32, slot: u32, a: [u32; 3], stack: &[u32]) -> u32 {
        let old = self.m.cpu().read_reg(Reg::Sp);
        let sp = (old - stack.len() as u32 * 4 - 8) & !7;
        self.m.cpu_mut().write_reg(Reg::Sp, sp);
        for (i, v) in stack.iter().enumerate() {
            self.put(sp + i as u32 * 4, *v);
        }
        let r = self.call(o, slot, a);
        self.m.cpu_mut().write_reg(Reg::Sp, old);
        r
    }
    fn create(&mut self, cls: u32) -> u32 {
        self.put(OUT, 0xdeadbeef);
        assert_eq!(
            self.run(
                aee::encode(Interface::Shell, 2),
                [loader::OBJECT_BASE, cls, OUT, 0]
            ),
            0,
            "fábrica pública deve devolver sua interface funcional"
        );
        let p = self.read(OUT);
        assert_ne!(p, 0);
        p
    }
    fn font(&mut self) -> u32 {
        self.create(0x0103fb4b)
    }
    fn text(&mut self, s: &str) -> u32 {
        let units: Vec<_> = s.encode_utf16().collect();
        self.m
            .cpu_mut()
            .write_mem(
                TEXT,
                &units
                    .iter()
                    .copied()
                    .chain([0])
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        units.len() as u32
    }
    fn measure(&mut self, font: u32, text: &str, max: i32) -> (u32, u32) {
        let n = self.text(text);
        assert_eq!(
            self.extra(font, 4, [TEXT, n, max as u32], &[OUT + 32, OUT + 36]),
            0
        );
        (self.read(OUT + 32), self.read(OUT + 36))
    }
    fn bitmap(&mut self) -> u32 {
        let d = self.create(0x01001001);
        self.call(d, 16, [OUT, 0, 0]);
        self.read(OUT)
    }
    fn draw(
        &mut self,
        font: u32,
        bitmap: u32,
        x: i32,
        y: i32,
        text: &str,
        fg: u32,
        bg: u32,
        clip: [i16; 4],
        flags: u32,
    ) -> u32 {
        let n = self.text(text);
        self.m
            .cpu_mut()
            .write_mem(
                OUT + 64,
                &clip
                    .into_iter()
                    .flat_map(i16::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        self.extra(
            font,
            3,
            [bitmap, x as u32, y as u32],
            &[TEXT, n, fg, bg, OUT + 64, flags],
        )
    }
    fn reference() -> Font {
        Font::load(raw_font(), "licensed NeoDunggeunmo test reference".into()).unwrap()
    }
}
impl Drop for F {
    fn drop(&mut self) {
        let p = self.root.canonicalize().unwrap();
        let allowed = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests/wave21-delivery-notes/runtime-tests")
            .canonicalize()
            .unwrap();
        assert!(p.starts_with(allowed));
        std::fs::remove_dir_all(p).unwrap();
    }
}
fn raw_font() -> Vec<u8> {
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(&include_bytes!("../../assets/fonts/neodgm-v1.601.ttf.gz")[..])
        .read_to_end(&mut bytes)
        .unwrap();
    bytes
}
