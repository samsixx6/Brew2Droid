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
            "palette-test-{}-{}", std::process::id(),
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

fn explicit_palette() -> Vec<u8> {
    let original=saf_rgb332();
    let mut palette=vec![0u8;768];
    for (index,rgb) in [(224,[17,34,51]),(28,[250,10,100]),(3,[0,128,255]),(227,[255,255,255])] {
        palette[index*3..index*3+3].copy_from_slice(&rgb);
    }
    let mut bytes=original[..24].to_vec();
    bytes[23]=0x20;
    bytes.extend_from_slice(&768u16.to_be_bytes());
    bytes.extend_from_slice(&palette);
    bytes.extend_from_slice(&original[24..]);
    bytes[10..12].copy_from_slice(&791u16.to_be_bytes());
    let size=bytes.len() as u16;bytes[5..7].copy_from_slice(&size.to_be_bytes());
    bytes
}

#[test]
fn saf_explicit_palette_uses_its_rgb_triples_instead_of_native_rgb332() {
    for flags in [0x20,0xa0] {
        let mut bytes=explicit_palette();bytes[23]=flags;
        let image=zeebx::video::icon::decode(&bytes).unwrap();
        assert_eq!((image.width,image.height),(2,2));
        assert_eq!(image.rgba,[17,34,51,255,250,10,100,255,0,128,255,255,255,255,255,255]);
    }
}

#[test]
fn saf_explicit_palette_reaches_iimage_with_native_rgb565_colors() {
    let mut f=Fixture::new();let image=f.resource(&explicit_palette());
    f.call(Interface::Image,"Draw",[image,0,0,0]);
    assert_eq!([
        f.machine.screen().get_pixel(0,0),f.machine.screen().get_pixel(1,0),
        f.machine.screen().get_pixel(0,1),f.machine.screen().get_pixel(1,1),
    ],[0x1106,0xf84c,0x041f,0xffff]);
}

#[test]
fn saf_palette_transparency_compares_indices_before_mapping_rgb() {
    let mut bytes=explicit_palette();
    // Dois índices têm a mesma cor; somente o índice-chave é transparente.
    bytes[26+227*3..26+227*3+3].copy_from_slice(&[17,34,51]);
    bytes[99+770]=3;bytes[105+770]=2;
    let image=zeebx::video::icon::decode(&bytes).unwrap();
    assert_eq!(image.rgba,[17,34,51,255,250,10,100,255,0,128,255,255,0,128,255,255]);
}

#[test]
fn saf_explicit_palette_requires_all_256_rgb_entries_and_rejects_other_palette_types() {
    let original=explicit_palette();
    for length in [0u16,1,767,769,1024,u16::MAX] {
        let mut bytes=original.clone();bytes[24..26].copy_from_slice(&length.to_be_bytes());
        assert!(zeebx::video::icon::decode(&bytes).is_err(),"paleta com {length} bytes");
    }
    for flags in [0x40,0x60,0xc0,0xe0,0x21] {
        let mut bytes=original.clone();bytes[23]=flags;
        assert!(zeebx::video::icon::decode(&bytes).is_err(),"paleta {flags:x}");
    }
    for end in [24,25,26,27,792,793] {
        let mut bytes=original[..end].to_vec();bytes[5..7].copy_from_slice(&(end as u16).to_be_bytes());
        assert!(zeebx::video::icon::decode(&bytes).is_err(),"paleta truncada em {end}");
    }
}
