use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend},
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    storage::StoragePaths,
};

const CLASS: u32 = 0x0180_00fe;
const OUT: u32 = loader::STACK_BASE + 0x1000;
const RECORD: u32 = OUT + 0x100;
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "brew-regional-model-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("module")).unwrap();
        std::fs::create_dir(root.join("overlay")).unwrap();
        Self(root)
    }
    fn machine(&self, provider: Option<bool>) -> Machine<BackendPadrao> {
        let mut code = vec![0u32; 40];
        code[0] = 0xe3a0_0000;
        code[1] = 0xe12f_ff1e;
        if let Some(working) = provider {
            // Um IModule ARM próprio: Load escreve o módulo e CreateInstance entrega seu objeto.
            code[2..7].copy_from_slice(&[
                0xe59f_0008,
                0xe582_0000,
                if working { 0xe3a0_0000 } else { 0xe3a0_0001 },
                0xe12f_ff1e,
                loader::MODULE_BASE + 0x60,
            ]);
            code[7..12].copy_from_slice(&[
                0xe59f_0008,
                0xe583_0000,
                0xe3a0_0000,
                0xe12f_ff1e,
                loader::MODULE_BASE + 0x64,
            ]);
            code[0x60 / 4] = loader::MODULE_BASE + 0x70;
            code[0x64 / 4] = loader::MODULE_BASE + 0x80;
            code[0x70 / 4..0x7c / 4].copy_from_slice(&[
                loader::MODULE_BASE + 0x90,
                loader::MODULE_BASE + 0x90,
                loader::MODULE_BASE + 0x1c,
            ]);
            code[0x80 / 4..0x8c / 4].fill(loader::MODULE_BASE + 0x90);
            code[0x90 / 4..0x98 / 4].copy_from_slice(&[0xe3a0_0001, 0xe12f_ff1e]);
        }
        let image = ModImage::parse(code.into_iter().flat_map(u32::to_le_bytes).collect()).unwrap();
        let mut loaded = loader::load_with(&image, &[]).unwrap();
        if provider.is_some() {
            loaded.extensions.push(loader::LoadedExtension {
                entry: loader::MODULE_BASE + 8,
                out_module: loaded.out_module + 8,
                classes: vec![CLASS],
            });
        }
        let storage = StoragePaths::from_root(self.0.join("runtime"));
        storage.create_dirs().unwrap();
        let mut m = Machine::new_with_storage(
            BackendPadrao::new().unwrap(),
            loaded,
            self.0.join("module"),
            &storage,
            Some(self.0.join("overlay")),
        );
        assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
        m
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let root = self.0.canonicalize().unwrap();
        assert!(root.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(
            root.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("brew-regional-model-")
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
fn shell(m: &mut Machine<BackendPadrao>, name: &str, args: [u32; 3]) -> u32 {
    let slot = (0..256)
        .find(|&s| Interface::Shell.method(s) == Some(name))
        .unwrap();
    match m
        .call_guest(
            aee::encode(Interface::Shell, slot),
            [loader::OBJECT_BASE, args[0], args[1], args[2]],
            10000,
        )
        .unwrap()
    {
        Outcome::Returned { code } => code,
        o => panic!("{name}: {o:?}"),
    }
}
fn create(m: &mut Machine<BackendPadrao>) -> u32 {
    assert_eq!(
        shell(m, "CreateInstance", [CLASS, OUT, 0]),
        0,
        "a classe deve entregar somente a consulta de modelo verificada"
    );
    m.cpu().read_u32(OUT).unwrap()
}
fn raw(m: &mut Machine<BackendPadrao>, object: u32, slot: u32, buffer: u32) -> Outcome {
    let table = m.cpu().read_u32(object).unwrap();
    let method = m.cpu().read_u32(table + slot * 4).unwrap();
    m.call_guest(method, [object, buffer, 0xdead_beef, 0xcafe_babe], 10000)
        .unwrap()
}
fn call(m: &mut Machine<BackendPadrao>, object: u32, slot: u32, buffer: u32) -> u32 {
    match raw(m, object, slot, buffer) {
        Outcome::Returned { code } => code,
        o => panic!("slot {slot}: {o:?}"),
    }
}

#[test]
fn consulta_regional_retorna_booleano_verdadeiro_ao_entregar_modelo() {
    let f = Fixture::new();
    let mut m = f.machine(None);
    let object = create(&mut m);
    // Bowling2, Hulk e Raiden rejeitam R0==0; RoadRash2 exige R0==1.
    // Este consumidor ARM chama a tabela real e conserva o ramo de sucesso.
    let at = loader::MODULE_BASE + 0x200;
    let table = m.cpu().read_u32(object).unwrap();
    let method = m.cpu().read_u32(table + 8).unwrap();
    let code = [
        0xe92d4010u32,
        0xe59fc018,
        0xe12fff3c,
        0xe3500001,
        0x13a00000,
        0x03a00001,
        0xe8bd8010,
        0xe1a00000,
        0xe1a00000,
        method,
    ];
    m.cpu_mut()
        .write_mem(
            at,
            &code
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    m.cpu_mut().write_mem(RECORD, &[0xa5; 48]).unwrap();
    assert_eq!(
        m.call_guest(at, [object, RECORD + 4, 0, 0], 10000).unwrap(),
        Outcome::Returned { code: 1 }
    );
    let bytes = m.dump(RECORD, 48).unwrap();
    assert_eq!(&bytes[..4], &[0xa5; 4]);
    assert_eq!(&bytes[4..12], b"B2Droid\0");
    assert_eq!(&bytes[12..], &[0xa5; 36]);
}
#[test]
fn consulta_regional_falha_com_booleano_falso_sem_escrever_payload() {
    let f = Fixture::new();
    let mut m = f.machine(None);
    let object = create(&mut m);
    for bad in [0, 0x7fff_fffc, loader::VTABLE_BASE] {
        assert_eq!(call(&mut m, object, 2, bad), 0);
    }
    m.cpu_mut().write_mem(RECORD, &[0x5a; 48]).unwrap();
    assert_eq!(call(&mut m, object, 1, 0), 0);
    assert_eq!(call(&mut m, object, 2, RECORD), 0);
    assert_eq!(m.dump(RECORD, 48).unwrap(), [0x5a; 48]);
    assert_eq!(
        shell(&mut m, "CreateInstance", [CLASS, 0, 0]),
        14,
        "a fabrica IShell conserva HRESULT"
    );
}
