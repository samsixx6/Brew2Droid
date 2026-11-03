use std::path::PathBuf;
use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend},
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    storage::StoragePaths,
};

const APPLET: u32 = loader::HEAP_BASE + 0x10000;
const VTABLE: u32 = APPLET + 0x100;
const PCB: u32 = loader::STACK_BASE + 0x200;
const CONTEXT: u32 = PCB + 64;
const FUNCTION: u32 = loader::MODULE_BASE + 0x200;

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
                "app-close-callback-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&work).unwrap();
        let storage = StoragePaths::from_root(work.join("runtime"));
        storage.create_dirs().unwrap();
        // HandleEvent registra evt+1 no objeto: START=1, STOP=2.
        let words = [
            0xe3a00000u32,
            0xe12fff1e,
            0xe2811001,
            0xe5801004,
            0xe3a00001,
            0xe12fff1e,
        ];
        let image =
            ModImage::parse(words.into_iter().flat_map(u32::to_le_bytes).collect()).unwrap();
        let mut loaded = loader::load_with(&image, &[]).unwrap();
        loaded.mem.write_u32(APPLET, VTABLE).unwrap();
        loaded
            .mem
            .write_u32(VTABLE + 8, loader::MODULE_BASE + 8)
            .unwrap();
        let mut m =
            Machine::new_with_storage(BackendPadrao::new().unwrap(), loaded, &work, &storage, None);
        assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
        assert_eq!(
            m.start_applet(APPLET, 0x12345678, 1000).unwrap(),
            Outcome::Returned { code: 1 }
        );
        let mut f = Self { m, work };
        f.words(
            FUNCTION,
            &[
                0xe5901000, 0xe2811001, 0xe5801000, 0xe59f2018, 0xe5922004, 0xe5802008, 0xe59f2010,
                0xe5922008, 0xe580200c, 0xe3a00000, 0xe12fff1e, APPLET, PCB,
            ],
        );
        f.m.cpu_mut().write_mem(PCB, &[0; 28]).unwrap();
        f.m.cpu_mut().write_mem(CONTEXT, &[0; 16]).unwrap();
        f.m.cpu_mut().write_u32(PCB + 16, FUNCTION).unwrap();
        f.m.cpu_mut().write_u32(PCB + 20, CONTEXT).unwrap();
        f
    }

    fn words(&mut self, at: u32, words: &[u32]) {
        self.m
            .cpu_mut()
            .write_mem(
                at,
                &words
                    .iter()
                    .flat_map(|word| word.to_le_bytes())
                    .collect::<Vec<_>>(),
            )
            .unwrap();
    }

    fn api(&mut self, name: &str, args: [u32; 4]) -> u32 {
        let slot = (0..256)
            .find(|&s| Interface::Shell.method(s) == Some(name))
            .unwrap();
        match self
            .m
            .call_guest(aee::encode(Interface::Shell, slot), args, 10000)
            .unwrap()
        {
            Outcome::Returned { code } => code,
            other => panic!("{name}: {other:?}"),
        }
    }

    fn register(&mut self, event: u32) {
        self.api(
            "RegisterSystemCallback",
            [loader::OBJECT_BASE, PCB, event, 0],
        );
    }

    fn read(&self, at: u32) -> u32 {
        self.m.cpu().read_u32(at).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.work);
    }
}

fn replacing_later_snapshot_entry(changed_identity: bool) {
    let mut f = Fixture::new();
    let b = PCB + 128;
    let old_context = PCB + 192;
    let new_context = if changed_identity {
        PCB + 224
    } else {
        old_context
    };
    let old_function = FUNCTION + 0x200;
    let new_function = if changed_identity {
        FUNCTION + 0x240
    } else {
        old_function
    };
    let counter = [0xe5901000, 0xe2811001, 0xe5801000, 0xe3a00000, 0xe12fff1e];
    f.words(old_function, &counter);
    f.words(new_function, &counter);
    f.m.cpu_mut().write_mem(b, &[0; 28]).unwrap();
    f.m.cpu_mut().write_mem(old_context, &[0; 16]).unwrap();
    f.m.cpu_mut().write_mem(new_context, &[0; 16]).unwrap();
    f.m.cpu_mut().write_u32(b + 16, old_function).unwrap();
    f.m.cpu_mut().write_u32(b + 20, old_context).unwrap();
    let slot = (0..256)
        .find(|&s| Interface::Shell.method(s) == Some("RegisterSystemCallback"))
        .unwrap();
    // A cancela B pelo cancelador real, troca os dados e registra a nova identidade.
    // B entra primeiro no provedor, mas A tem endereço menor e é visitado primeiro.
    f.words(
        FUNCTION,
        &[
            0xe92d4070,
            0xe1a04000,
            0xe5941000,
            0xe2811001,
            0xe5841000,
            0xe59f5034,
            0xe595c008,
            0xe1a00005,
            0xe12fff3c,
            0xe59f1028,
            0xe5851010,
            0xe59f1024,
            0xe5851014,
            0xe59f0020,
            0xe1a01005,
            0xe3a02004,
            0xe59fc018,
            0xe12fff3c,
            0xe3a00000,
            0xe8bd8070,
            b,
            new_function,
            new_context,
            loader::OBJECT_BASE,
            aee::encode(Interface::Shell, slot),
        ],
    );
    f.api("RegisterSystemCallback", [loader::OBJECT_BASE, b, 4, 0]);
    f.register(4);
    f.m.encerra_applet().unwrap();
    assert_eq!(f.read(CONTEXT), 1);
    assert_eq!(f.read(old_context), 0, "B antigo foi cancelado durante A");
    assert_eq!(
        f.read(new_context),
        0,
        "um novo registro não pertence ao snapshot de fechamento em curso"
    );
    assert_ne!(f.read(b + 8), 0, "novo B continua pendente e cancelável");
    f.m.encerra_applet().unwrap();
    assert_eq!(
        f.read(new_context),
        1,
        "novo B recebe somente a próxima transição"
    );
    assert_eq!(f.read(b + 8), 0);
}

#[test]
fn replacing_later_close_callback_defers_new_function_and_context() {
    replacing_later_snapshot_entry(true);
}

#[test]
fn cancelling_and_rearming_same_function_and_context_is_a_new_registration() {
    replacing_later_snapshot_entry(false);
}
