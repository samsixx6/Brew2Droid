use std::path::PathBuf;
use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend},
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    storage::StoragePaths,
};

const SCRATCH: u32 = loader::STACK_BASE + 0x100;

struct Fixture {
    machine: Machine<BackendPadrao>,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local-tests/startup-repairs-wave21-20261031/saf").join(format!(
            "mirror-test-{}-{}", std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        std::fs::create_dir_all(&root).unwrap();
        let storage = StoragePaths::from_root(root.join("runtime"));
        storage.create_dirs().unwrap();
        let image = ModImage::parse(
            [0xe3a0_0000u32, 0xe12f_ff1e].into_iter().flat_map(u32::to_le_bytes).collect(),
        ).unwrap();
        let mut machine = Machine::new_with_storage(
            BackendPadrao::new().unwrap(), loader::load_with(&image, &[]).unwrap(),
            &root, &storage, None,
        );
        assert_eq!(machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
        Self { machine, root }
    }

    fn call(&mut self, iface: Interface, name: &str, args: [u32; 4]) -> u32 {
        let slot = (0..256).find(|slot| iface.method(*slot) == Some(name)).unwrap();
        match self.machine.call_guest(aee::encode(iface, slot), args, 1000).unwrap() {
            Outcome::Returned { code } => code,
            other => panic!("{name}: {other:?}"),
        }
    }

    fn resource(&mut self, bytes: &[u8]) -> u32 {
        std::fs::write(self.root.join("preview.bmp"), bytes).unwrap();
        self.machine.cpu_mut().write_mem(SCRATCH, b"preview.bmp\0").unwrap();
        let image = self.call(Interface::Shell, "LoadResObject", [loader::OBJECT_BASE, SCRATCH, 0, 0]);
        assert_ne!(image, 0);
        image
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let root = self.root.canonicalize().unwrap();
        assert!(root.starts_with(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local-tests").canonicalize().unwrap()));
        std::fs::remove_dir_all(root).unwrap();
    }
}

fn saf_rgb332() -> Vec<u8> {
    // Fixture própria: tabela DEFLATE canônica externa, quatro literais e fim.
    // Os literais são vermelho, verde, azul e magenta; não há arte de um jogo.
    vec![
        0x53,0x41,0x46,0x00,0x02,0x00,0x6f,0x07,0x00,0x02,0x00,0x15,
        0x01,0x06,0x02,0x02,0x08,0x04,0x00,0x00,0x02,0x02,0x01,0x00,
        0x03,0x03,0x01,0x00,0x07,0x0a,0x02,0x01,0x90,0x0b,0x00,0x2a,
        0x00,0x38,0x80,0x00,0x00,0x00,0x40,0x00,0x84,0x01,0x00,0x00,
        0x06,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,
        0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,
        0xc0,0x0c,0x00,0x00,0x00,0x05,0x04,0x00,0x05,0x00,
        0x02,0x02,0x01,0xe3,0x00,0x08,0x78,0x9c,0x5d,0x73,0x04,0xc1,
        0x01,0xe3,0x06,0x00,0x09,0xe3,0x01,0x00,0x05,0x00,0x00,0x00,
        0x00,0x00,0x08,0x00,0x00,
    ]
}

fn mirrored() -> Vec<u8> {let mut bytes=saf_rgb332();bytes[104]=0x20;bytes}
#[test]
fn saf_horizontal_mirror_reverses_columns_without_reversing_rows() {
    let image=zeebx::video::icon::decode(&mirrored()).unwrap();
    assert_eq!(image.rgba,[0,255,0,255,255,0,0,255,255,0,255,255,0,0,255,255]);
}
#[test]
fn saf_horizontal_mirror_preserves_transparency_and_signed_clipping() {
    for (flags,mode,left,top,background,want) in [
        (0x20,2,0,0,3,[0,255,0,255,255,0,0,255,0,0,255,255,0,0,255,255]),
        (0xa0,0,1,0,28,[255,0,0,255,0,255,0,255,0,0,255,255,0,255,0,255]),
        (0x60,0,0,1,28,[255,0,255,255,0,0,255,255,0,255,0,255,0,255,0,255]),
    ] {
        let mut bytes=mirrored();bytes[99]=background;bytes[104]=flags;bytes[105]=mode;bytes[106]=left;bytes[107]=top;
        let image=zeebx::video::icon::decode(&bytes).unwrap();assert_eq!(image.rgba,want,"flags={flags:x}");
    }
}
#[test]
fn saf_horizontal_mirror_draws_real_native_iimage_pixels() {
    let mut f=Fixture::new();let image=f.resource(&mirrored());f.call(Interface::Image,"Draw",[image,0,0,0]);
    assert_eq!([f.machine.screen().get_pixel(0,0),f.machine.screen().get_pixel(1,0),f.machine.screen().get_pixel(0,1),f.machine.screen().get_pixel(1,1)],
        [0x07e0,0xf800,0xf81f,0x001f]);
}
#[test]
fn saf_mirror_does_not_enable_other_transformations() {
    for flags in [0x10,0x08,0x04,0x02,0x01,0x30,0x28] {
        let mut bytes=mirrored();bytes[104]=flags;assert!(zeebx::video::icon::decode(&bytes).is_err(),"flags={flags:x}");
    }
}
