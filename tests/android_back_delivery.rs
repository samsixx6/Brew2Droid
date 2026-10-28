//! A fila JNI entrega um toque completo entre quadros sem perder nenhuma borda.
extern crate self as android_activity;
#[derive(Clone, Copy)]
pub enum InputStatus { Handled, Unhandled }
pub mod input {
    #[derive(Clone, Copy)]
    pub enum Keycode { VolumeUp, VolumeDown, VolumeMute, Other }
    pub struct KeyEvent;
    impl KeyEvent { pub fn key_code(&self) -> Keycode { Keycode::Other } }
    pub enum InputEvent { KeyEvent(KeyEvent), Other }
}
#[allow(dead_code)]
#[path = "../frontends/android/src/input_dispatch.rs"]
mod dispatch;
#[allow(dead_code)]
#[path = "../frontends/android/src/button_mapping.rs"]
mod mapping;
use zeebx::handset::{input::Key, touch::TouchKeypad};

#[test]
fn complete_back_tap_between_native_polls_delivers_press_then_release_and_does_not_replay() {
    dispatch::take_physical_back();
    let send = |down| dispatch::Java_io_github_brew_handset_GameActivity_nativePhysicalBack(std::ptr::null_mut(), std::ptr::null_mut(), 7, 158, down);
    let mut map = mapping::InputMap::parse(r#"{"version":1,"bindings":{"clear":"key:4:scan:158"}}"#);
    let mut keys = TouchKeypad::default();
    let mut delivered = Vec::new();
    send(1);
    send(1);
    send(0);
    dispatch::deliver_physical_back(|device, scan, down| {
        assert!(map.physical_back(device, scan, down));
        map.apply(&mut keys);
        delivered.push(keys.held());
    });
    assert_eq!(delivered, [1 << Key::Clear as u8, 0]);
    assert_eq!(keys.held(), 0);
    dispatch::deliver_physical_back(|_, _, _| panic!("Consumed Back edge replayed"));

    // Um dispositivo desconectado não pode ressuscitar uma tecla; outros continuam ativos.
    send(1);
    dispatch::Java_io_github_brew_handset_GameActivity_nativePhysicalBack(std::ptr::null_mut(), std::ptr::null_mut(), 8, 0, 1);
    dispatch::discard_physical_back_device(7);
    mapping::queue_removed_device(7);
    for device in mapping::take_removed_devices() { map.release_device(device); }
    dispatch::deliver_physical_back(|device, scan, down| {
        assert_ne!(device, 7, "Disconnected device replayed a pending Back press");
        map.physical_back(device, scan, down);
        map.apply(&mut keys);
    });
    assert_eq!(keys.held(), 1 << Key::Soft2 as u8);
    map.release_device(8);
    map.apply(&mut keys);
    assert_eq!(keys.held(), 0);
}
