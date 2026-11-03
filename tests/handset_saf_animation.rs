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
            "animation-test-{}-{}", std::process::id(),
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

fn animated() -> Vec<u8> {
    let mut bytes = saf_rgb332()[..96].to_vec();
    bytes[26] = 3;
    // Cada quadro reinicia o fundo. O segundo desloca o objeto à esquerda e
    // preserva azul sob o magenta transparente; o terceiro sobe sem máscara.
    bytes.extend_from_slice(&[
        6,0,9,0,1,0,5,0,0,0,0,0,
        6,0,9,3,1,0,5,0,0x80,2,1,0,
        6,0,9,28,1,0,5,0,0x40,0,0,1,
        8,0,0,
    ]);
    let size = bytes.len() as u16;
    bytes[5..7].copy_from_slice(&size.to_be_bytes());
    bytes
}

#[test]
fn animated_saf_exposes_every_composed_frame_with_signed_placement() {
    let mut f = Fixture::new();
    let bytes = animated();
    assert!(zeebx::video::icon::decode(&bytes).is_err(), "ícone não pode fingir que a animação é estática");
    let image = f.resource(&bytes);
    f.call(Interface::Image, "GetInfo", [image, SCRATCH + 64, 0, 0]);
    let mut info = [0; 10];
    f.machine.cpu().read_mem(SCRATCH + 64, &mut info).unwrap();
    assert_eq!(u16::from_le_bytes([info[0], info[1]]), 6);
    assert_eq!(u16::from_le_bytes([info[2], info[3]]), 2);
    assert_eq!(u16::from_le_bytes([info[8], info[9]]), 2);
    for (frame, x, expected) in [
        (0, 0, [0xf800, 0x07e0, 0x001f, 0xf81f]),
        (1, 4, [0x07e0, 0x001f, 0x001f, 0x001f]),
        (2, 8, [0x001f, 0xf81f, 0x07e0, 0x07e0]),
    ] {
        f.call(Interface::Image, "DrawFrame", [image, frame, x, 0]);
        let actual = [
            f.machine.screen().get_pixel(x as i32, 0),
            f.machine.screen().get_pixel(x as i32 + 1, 0),
            f.machine.screen().get_pixel(x as i32, 1),
            f.machine.screen().get_pixel(x as i32 + 1, 1),
        ];
        assert_eq!(actual, expected, "quadro {frame}");
    }
}

#[test]
fn animated_saf_rejects_incomplete_sequences_and_unsupported_motion() {
    let original = animated();
    let mut f = Fixture::new();
    for (at, value) in [(26,2), (26,4), (104,0x10), (105,1)] {
        let mut bytes=original.clone();
        bytes[at]=value;
        std::fs::write(f.root.join("rejected.saf"), &bytes).unwrap();
        f.machine.cpu_mut().write_mem(SCRATCH, b"rejected.saf\0").unwrap();
        let image=f.call(Interface::Shell,"LoadResObject",[loader::OBJECT_BASE,SCRATCH,0,0]);
        assert_eq!(image,0,"campo {at}={value}");
    }
}


#[test]
fn starting_a_frame_strip_draws_only_its_first_frame() {
    let mut f=Fixture::new();
    let image=f.resource(&animated());
    f.call(Interface::Image,"Start",[image,0,0,0]);
    assert_eq!(f.machine.screen().get_pixel(0,0),0xf800);
    assert_eq!(f.machine.screen().get_pixel(1,0),0x07e0);
    assert_eq!(f.machine.screen().get_pixel(2,0),0,"o quadro seguinte não pode aparecer ao lado do primeiro");
    assert_eq!(f.machine.screen().get_pixel(3,0),0);
}
