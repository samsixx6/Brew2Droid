use std::path::PathBuf;
use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend};
use zeebx::handset::types::DisplayGeometry;
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

const OUT: u32 = loader::STACK_BASE + 0x200;
const DISPLAY: u32 = loader::OBJECT_BASE + 16;

struct Fixture {
    m: Machine<BackendPadrao>,
    work: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let work = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests")
            .join(format!(
                "destination-dib-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&work).unwrap();
        let storage = StoragePaths::from_root(work.join("runtime"));
        storage.create_dirs().unwrap();
        let image = ModImage::parse(
            [0xe3a00000u32, 0xe12fff1e]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect(),
        )
        .unwrap();
        let mut m = Machine::new_with_storage_geometry(
            BackendPadrao::new().unwrap(),
            loader::load_with(&image, &[]).unwrap(),
            &work,
            &storage,
            None,
            DisplayGeometry {
                width: 13,
                height: 7,
                depth: 16,
            },
        )
        .unwrap();
        assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
        Self { m, work }
    }

    fn call(&mut self, interface: Interface, name: &str, args: [u32; 4]) -> u32 {
        let slot = (0..256)
            .find(|s| interface.method(*s) == Some(name))
            .unwrap();
        match self
            .m
            .call_guest(aee::encode(interface, slot), args, 1000)
            .unwrap()
        {
            Outcome::Returned { code } => code,
            other => panic!("{name}: {other:?}"),
        }
    }

    fn graphics(&mut self) -> u32 {
        assert_eq!(
            self.call(
                Interface::Shell,
                "CreateInstance",
                [loader::OBJECT_BASE, 0x01002001, OUT, 0]
            ),
            0
        );
        let graphics = self.m.cpu().read_u32(OUT).unwrap();
        assert_ne!(graphics, 0);
        graphics
    }

    fn guest_pixel(&mut self, address: u32, color: u16) {
        // STRH real: a escrita do host não deve simular o sinalizador de sujeira do guest.
        let writer = loader::MODULE_BASE + 0x80;
        self.m
            .cpu_mut()
            .write_mem(
                writer,
                &[
                    0xe1c010b0u32.to_le_bytes(),
                    0xe3a00000u32.to_le_bytes(),
                    0xe12fff1eu32.to_le_bytes(),
                ]
                .concat(),
            )
            .unwrap();
        assert_eq!(
            self.m
                .call_guest(writer, [address, u32::from(color), 0, 0], 1000)
                .unwrap(),
            Outcome::Returned { code: 0 }
        );
    }

    fn check_destination(&mut self, interface: Interface, object: u32) {
        // O consumidor lê IDIB diretamente, sem QueryInterface: é o contrato de um
        // bitmap de software já devolvido pelo fornecedor da superfície.
        let bitmap = self.call(interface, "GetDestination", [object, 0, 0, 0]);
        assert_ne!(bitmap, 0);
        assert_eq!(self.m.cpu().read_u32(bitmap + 20).unwrap(), 13 | (7 << 16));
        let mut fields = [0; 12];
        self.m.cpu().read_mem(bitmap + 24, &mut fields).unwrap();
        assert_eq!(i16::from_le_bytes([fields[0], fields[1]]), 28);
        assert_eq!(&fields[4..6], &[16, 16]);
        let pixels = self.m.cpu().read_u32(bitmap + 8).unwrap();
        assert_ne!(pixels, 0);
        let last = pixels + 6 * 28 + 12 * 2;
        self.guest_pixel(last, 0xf800);
        // Consultar novamente não pode copiar a versão antiga do host por cima do guest.
        assert_eq!(
            self.call(interface, "GetDestination", [object, 0, 0, 0]),
            bitmap
        );
        assert_eq!(self.m.cpu().read_u32(bitmap + 8).unwrap(), pixels);
        let mut color = [0; 2];
        self.m.cpu().read_mem(last, &mut color).unwrap();
        assert_eq!(u16::from_le_bytes(color), 0xf800);
        self.call(Interface::Display, "Update", [DISPLAY, 0, 0, 0]);
        assert_eq!(self.m.screen().get_pixel(12, 6), 0xf800);
        self.guest_pixel(last, 0x07e0);
        self.call(Interface::Display, "Update", [DISPLAY, 0, 0, 0]);
        assert_eq!(self.m.screen().get_pixel(12, 6), 0x07e0);
        // Cada consulta transfere uma referência, sem destruir a superfície persistente.
        let second = self.call(Interface::Bitmap, "Release", [bitmap, 0, 0, 0]);
        let first = self.call(Interface::Bitmap, "Release", [bitmap, 0, 0, 0]);
        assert_eq!(second, first + 1);
        assert!(first >= 1);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.work);
    }
}

#[test]
fn display_destination_exposes_pixels_before_any_interface_query() {
    let mut f = Fixture::new();
    f.check_destination(Interface::Display, DISPLAY);
}

#[test]
fn graphics_destination_exposes_pixels_and_preserves_pending_guest_writes() {
    let mut f = Fixture::new();
    let graphics = f.graphics();
    f.check_destination(Interface::Graphics, graphics);
}
