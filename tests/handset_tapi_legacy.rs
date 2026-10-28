use std::path::PathBuf;

use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend, Reg};
use zeebx::handset::types::DisplayGeometry;
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::save_state::{self, Leitor};
use zeebx::storage::StoragePaths;

const TAPI: u32 = 0x0100_1007;
const OUT: u32 = loader::STACK_BASE + 0x800;

fn machine() -> (Machine<BackendPadrao>, PathBuf) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("local-tests")
        .join(format!(
            "legacy-tapi-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
    std::fs::create_dir_all(&root).unwrap();
    let storage = StoragePaths::from_root(root.join("runtime"));
    storage.create_dirs().unwrap();
    let image = ModImage::parse(
        [0xe3a0_0000u32, 0xe12f_ff1e]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect(),
    )
    .unwrap();
    let mut machine = Machine::new_with_storage_geometry(
        BackendPadrao::new().unwrap(),
        loader::load_with(&image, &[]).unwrap(),
        &root,
        &storage,
        None,
        DisplayGeometry {
            width: 240,
            height: 320,
            depth: 16,
        },
    )
    .unwrap();
    machine.enable_handset_clock();
    assert_eq!(machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
    (machine, root)
}

fn create(machine: &mut Machine<BackendPadrao>, class: u32) -> u32 {
    let slot = (0..256)
        .find(|slot| Interface::Shell.method(*slot) == Some("CreateInstance"))
        .unwrap();
    assert_eq!(
        machine
            .call_guest(
                aee::encode(Interface::Shell, slot),
                [loader::OBJECT_BASE, class, OUT, 0],
                1000
            )
            .unwrap(),
        Outcome::Returned { code: 0 },
        "classe BREW legada {class:#x}"
    );
    let object = machine.cpu().read_u32(OUT).unwrap();
    assert_ne!(object, 0);
    object
}

fn raw_call(machine: &mut Machine<BackendPadrao>, object: u32, slot: u32, args: &[u32]) -> Outcome {
    let vtable = machine.cpu().read_u32(object).unwrap();
    let function = machine.cpu().read_u32(vtable + slot * 4).unwrap();
    let sp = machine.cpu().read_reg(Reg::Sp);
    let scratch = loader::STACK_BASE + 0x400;
    for (index, value) in args.iter().skip(3).enumerate() {
        machine
            .cpu_mut()
            .write_u32(scratch + index as u32 * 4, *value)
            .unwrap();
    }
    machine.cpu_mut().write_reg(Reg::Sp, scratch);
    let arguments = [
        object,
        args.first().copied().unwrap_or(0),
        args.get(1).copied().unwrap_or(0),
        args.get(2).copied().unwrap_or(0),
    ];
    let result = machine.call_guest(function, arguments, 10000).unwrap();
    machine.cpu_mut().write_reg(Reg::Sp, sp);
    result
}

fn call(machine: &mut Machine<BackendPadrao>, object: u32, slot: u32, args: &[u32]) -> u32 {
    match raw_call(machine, object, slot, args) {
        Outcome::Returned { code } => code,
        other => panic!("slot {slot}: {other:?}"),
    }
}

fn cleanup(machine: Machine<BackendPadrao>, root: PathBuf) {
    drop(machine);
    assert!(
        root.canonicalize().unwrap().starts_with(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .canonicalize()
                .unwrap()
        )
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_tapi_reports_only_offline_status_without_overwriting_the_arm_struct() {
    let (mut machine, root) = machine();
    let tapi = create(&mut machine, TAPI);
    machine
        .cpu_mut()
        .write_mem(OUT + 0x40, &[0xa5; 28])
        .unwrap();
    assert_eq!(call(&mut machine, tapi, 3, &[OUT + 0x44]), 0);
    let mut bytes = [0; 28];
    machine.cpu().read_mem(OUT + 0x40, &mut bytes).unwrap();
    assert_eq!(&bytes[..4], &[0xa5; 4]);
    assert_eq!(
        &bytes[4..24],
        &[0; 20],
        "MIN vazio, PS_OFFLINE, sem serviço digital/dados ou chamada"
    );
    assert_eq!(
        &bytes[24..],
        &[0xa5; 4],
        "TAPIStatus ARM ocupa 20 bytes, não a estrutura Win32"
    );
    assert_eq!(call(&mut machine, tapi, 3, &[0]), 14);
    assert_eq!(
        call(&mut machine, tapi, 8, &[]),
        0,
        "nenhuma chamada de voz"
    );
    assert_eq!(
        call(&mut machine, tapi, 9, &[]),
        0,
        "o emulador não oferece rede celular"
    );
    assert_eq!(call(&mut machine, tapi, 0, &[]), 2);
    assert_eq!(call(&mut machine, tapi, 1, &[]), 1);
    let saved = machine.grava_estado();
    machine.restaura_estado(&saved).unwrap();
    assert_eq!(call(&mut machine, tapi, 3, &[OUT + 0x44]), 0);
    assert!(
        matches!(
            raw_call(&mut machine, tapi, 4, &[0, 0]),
            Outcome::Unimplemented { .. }
        ),
        "discagem continua explicitamente não suportada"
    );
    assert_eq!(call(&mut machine, tapi, 1, &[]), 0);
    cleanup(machine, root);
}

#[test]
fn tapi_status_registration_owns_one_notifier_reference_and_roundtrips() {
    let (mut machine, root) = machine();
    let app = 0x0101_3eca;
    let shell = loader::OBJECT_BASE;
    let register = (0..256)
        .find(|slot| Interface::Shell.method(*slot) == Some("RegisterNotify"))
        .unwrap();
    assert_eq!(call(&mut machine, shell, register, &[app, TAPI, 1]), 0);
    assert_eq!(call(&mut machine, shell, register, &[app, TAPI, 1]), 0);
    assert_eq!(
        call(&mut machine, shell, register, &[app, TAPI, 2]),
        14,
        "SMS não está implementado"
    );
    let saved = machine.grava_estado();
    let registrations = Leitor::abre(&saved)
        .unwrap()
        .pares("tab.tapi_legacy")
        .unwrap();
    assert_eq!(
        registrations.len(),
        1,
        "repetir o mesmo registro não vaza objetos"
    );
    assert_eq!(registrations[0].0, app);
    let notifier = registrations[0].1;
    assert_eq!(
        call(&mut machine, notifier, 0, &[]),
        2,
        "registro possui uma referência ao notificador"
    );
    assert_eq!(call(&mut machine, notifier, 1, &[]), 1);
    assert!(
        machine.advance(1000).unwrap().is_empty(),
        "estado offline estável não inventa mudanças de status"
    );
    assert_eq!(call(&mut machine, shell, register, &[app, TAPI, 0]), 0);
    assert_eq!(
        call(&mut machine, notifier, 3, &[OUT + 0x40]),
        14,
        "desregistro libera o notificador"
    );
    machine.restaura_estado(&saved).unwrap();
    assert_eq!(call(&mut machine, shell, register, &[app, TAPI, 0]), 0);
    assert_eq!(
        call(&mut machine, notifier, 3, &[OUT + 0x40]),
        14,
        "registro restaurado também pode ser cancelado"
    );
    assert_eq!(
        call(&mut machine, shell, register, &[app, 0x0100_5000, 1]),
        10,
        "uma classe ausente é recusada com EBADCLASS, sem inventar notificações"
    );
    cleanup(machine, root);
}

#[test]
fn old_save_without_tapi_section_clears_the_notification_book() {
    let (mut machine, root) = machine();
    let app = 0x0101_3eca;
    let slot = (0..256)
        .find(|slot| Interface::Shell.method(*slot) == Some("RegisterNotify"))
        .unwrap();
    call(&mut machine, loader::OBJECT_BASE, slot, &[app, TAPI, 1]);
    let saved = machine.grava_estado();
    let reader = Leitor::abre(&saved).unwrap();
    let object = reader.pares("tab.tapi_legacy").unwrap()[0].1;
    let old = save_state::escreve(
        &reader
            .nomes()
            .into_iter()
            .filter(|name| *name != "tab.tapi_legacy")
            .map(|name| (name.to_owned(), reader.secao(name).unwrap().to_vec()))
            .collect::<Vec<_>>(),
    );
    machine.restaura_estado(&old).unwrap();
    call(&mut machine, loader::OBJECT_BASE, slot, &[app, TAPI, 0]);
    assert_eq!(
        call(&mut machine, object, 3, &[OUT + 0x40]),
        0,
        "cancelar registro inexistente não libera um objeto do save antigo"
    );
    cleanup(machine, root);
}

#[test]
fn malformed_save_cannot_give_two_tapi_registrations_one_owned_reference() {
    let (mut machine, root) = machine();
    let app = 0x0101_3eca;
    let slot = (0..256)
        .find(|slot| Interface::Shell.method(*slot) == Some("RegisterNotify"))
        .unwrap();
    call(&mut machine, loader::OBJECT_BASE, slot, &[app, TAPI, 1]);
    let saved = machine.grava_estado();
    let reader = Leitor::abre(&saved).unwrap();
    let object = reader.pares("tab.tapi_legacy").unwrap()[0].1;
    let duplicate = [4u32, app, object, app + 1, object]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    let malformed = save_state::escreve(
        &reader
            .nomes()
            .into_iter()
            .map(|name| {
                (
                    name.to_owned(),
                    if name == "tab.tapi_legacy" {
                        duplicate.clone()
                    } else {
                        reader.secao(name).unwrap().to_vec()
                    },
                )
            })
            .collect::<Vec<_>>(),
    );
    assert!(
        machine.restaura_estado(&malformed).is_err(),
        "cancelar uma assinatura não pode invalidar o notificador da outra"
    );
    cleanup(machine, root);
}

#[test]
fn restored_tapi_registration_requires_its_owned_reference() {
    let (mut machine, root) = machine();
    let slot = (0..256)
        .find(|slot| Interface::Shell.method(*slot) == Some("RegisterNotify"))
        .unwrap();
    call(
        &mut machine,
        loader::OBJECT_BASE,
        slot,
        &[0x01013eca, TAPI, 1],
    );
    let saved = machine.grava_estado();
    let reader = Leitor::abre(&saved).unwrap();
    let notifier = reader.pares("tab.tapi_legacy").unwrap()[0].1;
    let refs = reader.pares("objects.refs").unwrap();
    let replacement = std::iter::once(refs.len() as u32 * 2)
        .chain(
            refs.into_iter()
                .flat_map(|(object, count)| [object, if object == notifier { 0 } else { count }]),
        )
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    let malformed = save_state::escreve(
        &reader
            .nomes()
            .into_iter()
            .map(|name| {
                (
                    name.to_owned(),
                    if name == "objects.refs" {
                        replacement.clone()
                    } else {
                        reader.secao(name).unwrap().to_vec()
                    },
                )
            })
            .collect::<Vec<_>>(),
    );
    assert!(
        machine.restaura_estado(&malformed).is_err(),
        "registry cannot retain an unreferenced notifier"
    );
    cleanup(machine, root);
}
