//! Fixture própria: GetDestination não pode republicar o IDIB de um objeto do guest.

use std::path::PathBuf;
use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend};
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

const DISPLAY: u32 = loader::OBJECT_BASE + 16;
const QUERY: u32 = loader::MODULE_BASE + 0x100;

struct Fixture {
    machine: Machine<BackendPadrao>,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests/startup-repairs-20261029")
            .join(format!(
                "foreign-destination-{}-{}",
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
        let mut machine = Machine::new_with_storage(
            BackendPadrao::new().unwrap(),
            loader::load_with(&image, &[]).unwrap(),
            &root,
            &storage,
            None,
        );
        assert_eq!(machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
        Self { machine, root }
    }

    fn call(&mut self, interface: Interface, method: &str, args: [u32; 4]) -> u32 {
        let slot = (0..256)
            .find(|&slot| interface.method(slot) == Some(method))
            .unwrap();
        match self
            .machine
            .call_guest(aee::encode(interface, slot), args, 10000)
            .unwrap()
        {
            Outcome::Returned { code } => code,
            outcome => panic!("{method}: {outcome:?}"),
        }
    }

    fn alloc(&mut self, bytes: u32) -> u32 {
        let pointer = self.call(Interface::Helpers, "malloc", [bytes, 0, 0, 0]);
        assert_ne!(pointer, 0);
        pointer
    }

    fn bytes(&self, pointer: u32, size: usize) -> Vec<u8> {
        let mut bytes = vec![0; size];
        self.machine.cpu().read_mem(pointer, &mut bytes).unwrap();
        bytes
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn foreign_destination_keeps_its_descriptor(separate_dib: bool) {
    let mut f = Fixture::new();
    let object = f.alloc(64);
    let dib = if separate_dib { f.alloc(64) } else { object };
    let vtable = f.alloc(64);
    let pixels = f.alloc(16);

    // QueryInterface retorna um IDIB do guest. A variante separada demonstra
    // que os campos públicos nem precisam estar no endereço do IBitmap.
    let query = [0xe59f3008u32, 0xe5823000, 0xe3a00000, 0xe12fff1e, dib];
    f.machine
        .cpu_mut()
        .write_mem(
            QUERY,
            &query
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    f.machine.cpu_mut().write_u32(vtable + 8, QUERY).unwrap();
    f.machine.cpu_mut().write_mem(object, &[0xa5; 64]).unwrap();
    f.machine.cpu_mut().write_u32(object, vtable).unwrap();

    let mut descriptor = vec![0xa5; 36];
    for (offset, value) in [(0, vtable), (4, 0), (8, pixels), (12, 0), (16, 0xf81f)] {
        descriptor[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    for (offset, value) in [(20, 3u16), (22, 2), (24, 8), (26, 0)] {
        descriptor[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    descriptor[28..30].copy_from_slice(&[16, 16]);
    f.machine.cpu_mut().write_mem(dib, &descriptor).unwrap();
    let raster = [0xf800u16, 0x07e0, 0x001f, 0xdead, 0xffff, 0, 0xf81f, 0xbeef]
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    f.machine.cpu_mut().write_mem(pixels, &raster).unwrap();

    assert_eq!(
        f.call(
            Interface::Display,
            "SetDestination",
            [DISPLAY, object, 0, 0]
        ),
        0
    );
    // SetDestination conclui a sonda antes de retornar. A superfície estrangeira
    // já integra o mapa de framebuffers, mas continua pertencendo ao guest.
    let saved = f.machine.grava_estado();
    let parsed = zeebx::save_state::Leitor::abre(&saved).unwrap();
    assert!(
        parsed
            .pares("tab.dib_buffers")
            .unwrap()
            .contains(&(object, pixels)),
        "o teste precisa cobrir uma superfície estrangeira cuja sonda teve sucesso"
    );
    let object_before = f.bytes(object, 64);
    let dib_before = f.bytes(dib, 64);
    let pixels_before = f.bytes(pixels, 16);

    assert_eq!(
        f.call(Interface::Display, "GetDestination", [DISPLAY, 0, 0, 0]),
        object
    );
    assert_eq!(
        f.bytes(object, 64),
        object_before,
        "GetDestination não modifica o IBitmap implementado pelo guest"
    );
    assert_eq!(
        f.bytes(dib, 64),
        dib_before,
        "GetDestination conserva pBmp, formato e campos reservados do IDIB estrangeiro"
    );
    assert_eq!(
        f.bytes(pixels, 16),
        pixels_before,
        "consultar o destino não substitui nem reescreve os pixels estrangeiros"
    );
}

#[test]
fn get_destination_preserves_foreign_idib_in_the_bitmap_object() {
    foreign_destination_keeps_its_descriptor(false);
}

#[test]
fn get_destination_preserves_foreign_query_interface_with_a_separate_idib() {
    foreign_destination_keeps_its_descriptor(true);
}
