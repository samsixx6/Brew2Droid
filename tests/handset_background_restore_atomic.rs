use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend},
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    save_state::{self, Leitor},
    storage::StoragePaths,
};

const PCB: u32 = loader::STACK_BASE + 0x200;
const CALLBACK: u32 = loader::MODULE_BASE;

struct Fixture {
    machine: Machine<BackendPadrao>,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "brew-background-restore-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        let storage = StoragePaths::from_root(root.join("runtime"));
        storage.create_dirs().unwrap();
        let image = ModImage::parse(
            [0xe3a00000u32, 0xe12fff1e]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect(),
        ).unwrap();
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

    fn shell(&mut self, name: &str, args: [u32; 4]) {
        let slot = (0..256).find(|&slot| Interface::Shell.method(slot) == Some(name)).unwrap();
        assert_eq!(
            self.machine.call_guest(aee::encode(Interface::Shell, slot), args, 10000).unwrap(),
            Outcome::Returned { code: 0 }
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let path = self.root.canonicalize().unwrap();
        assert!(path.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(path.file_name().unwrap().to_string_lossy().starts_with("brew-background-restore-"));
        let _ = std::fs::remove_dir_all(path);
    }
}

fn section(archive: &[u8], name: &str) -> Vec<u8> {
    Leitor::abre(archive).unwrap().secao(name).unwrap().to_vec()
}

#[test]
fn invalid_background_boolean_preserves_live_timers_callbacks_and_frame() {
    let mut f = Fixture::new();
    // O estado anterior tem tela preta e nenhuma agenda. Uma recusa posterior
    // não pode aplicar essas partes e deixar apenas CPU/heap na linha atual.
    let older = f.machine.grava_estado();
    f.shell("SetTimer", [loader::OBJECT_BASE, 60000, CALLBACK, PCB + 64]);
    f.machine.cpu_mut().write_mem(PCB, &[0; 28]).unwrap();
    f.machine.cpu_mut().write_u32(PCB + 16, CALLBACK).unwrap();
    f.machine.cpu_mut().write_u32(PCB + 20, PCB + 64).unwrap();
    f.shell("RegisterSystemCallback", [loader::OBJECT_BASE, PCB, 4, 0]);
    let pixel = 0xf81fu16.to_le_bytes();
    let pixels = pixel.repeat(
        f.machine.screen().width() as usize * f.machine.screen().height() as usize
    );
    assert!(f.machine.pinta_tela_rgb565(&pixels));
    let before = f.machine.grava_estado();
    let timers = section(&before, "agenda.timers");
    let callbacks = section(&before, "agenda.systemcallbacks");
    assert!(!timers.is_empty());
    assert!(!callbacks.is_empty());

    let reader = Leitor::abre(&older).unwrap();
    let mut sections = reader.nomes().into_iter()
        .filter(|name| *name != "esc.applet_em_background")
        .map(|name| (name.to_owned(), reader.secao(name).unwrap().to_vec()))
        .collect::<Vec<_>>();
    sections.push(("esc.applet_em_background".into(), 2u32.to_le_bytes().to_vec()));
    assert!(f.machine.restaura_estado(&save_state::escreve(&sections)).is_err());

    let after = f.machine.grava_estado();
    assert_eq!(
        (
            section(&after, "agenda.timers"),
            section(&after, "agenda.systemcallbacks"),
            f.machine.screen().to_rgb565_bytes() == pixels,
        ),
        (timers, callbacks, true),
        "um booleano inválido deve ser recusado antes de substituir estado vivo"
    );
}
