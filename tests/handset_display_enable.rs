use std::path::PathBuf;

use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend};
use zeebx::handset::types::DisplayGeometry;
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

const DISPLAY: u32 = loader::OBJECT_BASE + 0x10;
const CALLBACK: u32 = loader::STACK_BASE + 0x200;
const MARKER: u32 = CALLBACK + 0x80;
const OUT: u32 = CALLBACK + 0x100;

fn machine() -> (Machine<BackendPadrao>, PathBuf) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let work = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local-tests").join(format!(
        "display-enable-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    ));
    std::fs::create_dir_all(&work).unwrap();
    let storage = StoragePaths::from_root(work.join("runtime"));
    storage.create_dirs().unwrap();
    let words = [
        0xe3a0_0000u32, 0xe12f_ff1e, // AEEMod_Load retorna zero.
        0xe590_1000, 0xe281_1001, 0xe580_1000, 0xe12f_ff1e, // callback incrementa *pUser.
    ];
    let image = ModImage::parse(words.into_iter().flat_map(u32::to_le_bytes).collect()).unwrap();
    let loaded = loader::load_with(&image, &[]).unwrap();
    let mut machine = Machine::new_with_storage_geometry(
        BackendPadrao::new().unwrap(), loaded, &work, &storage, None,
        DisplayGeometry { width: 12, height: 12, depth: 16 },
    ).unwrap();
    assert_eq!(machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
    (machine, work)
}

fn interface_call(machine: &mut Machine<BackendPadrao>, interface: Interface, name: &str, arguments: [u32; 4]) -> u32 {
    let slot = (0..256).find(|slot| interface.method(*slot) == Some(name)).unwrap();
    match machine.call_guest(aee::encode(interface, slot), arguments, 1000).unwrap() {
        Outcome::Returned { code } => code,
        outcome => panic!("{name}: {outcome:?}"),
    }
}

fn call(machine: &mut Machine<BackendPadrao>, name: &str, argument: u32) -> u32 {
    interface_call(machine, Interface::Display, name, [DISPLAY, argument, 0, 0])
}

#[test]
fn display_is_enabled_for_the_live_device_and_offscreen_destination() {
    let (mut machine, work) = machine();
    assert_eq!(call(&mut machine, "IsEnabled", 0), 1);
    // Um bitmap fora da tela também é sempre habilitado, segundo o SDK.
    let device = call(&mut machine, "GetDestination", 0);
    assert_eq!(interface_call(&mut machine, Interface::Bitmap, "CreateCompatibleBitmap", [device, OUT, 4, 4]), 0);
    let bitmap = machine.cpu().read_u32(OUT).unwrap();
    assert_ne!(bitmap, 0);
    assert_eq!(call(&mut machine, "SetDestination", bitmap), 0);
    assert_eq!(call(&mut machine, "IsEnabled", 0), 1);
    drop(machine);
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn display_enable_notification_rejects_a_null_callback() {
    let (mut machine, work) = machine();
    assert_eq!(call(&mut machine, "NotifyEnable", 0), 14); // EBADPARM de AEEError.h.
    drop(machine);
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn display_enable_notification_waits_for_an_actual_state_change() {
    let (mut machine, work) = machine();
    machine.cpu_mut().write_mem(CALLBACK, &[0; 32]).unwrap();
    machine.cpu_mut().write_u32(CALLBACK + 16, loader::MODULE_BASE + 8).unwrap();
    machine.cpu_mut().write_u32(CALLBACK + 20, MARKER).unwrap();
    assert_eq!(call(&mut machine, "NotifyEnable", CALLBACK), 0);
    // Registrar não é uma mudança de estado: chamar imediatamente reentraria
    // indevidamente no jogo. O display permanece habilitado durante seu tempo de vida.
    assert_eq!(machine.cpu().read_u32(MARKER).unwrap(), 0);
    assert!(machine.deliver_callbacks(1000).unwrap().is_empty());
    assert!(machine.deliver_signals(1000).unwrap().is_empty());
    for _ in 0..3 { assert!(machine.advance(1000).unwrap().is_empty()); }
    assert_eq!(machine.cpu().read_u32(MARKER).unwrap(), 0);
    assert_eq!(call(&mut machine, "IsEnabled", 0), 1);
    drop(machine);
    std::fs::remove_dir_all(work).unwrap();
}
