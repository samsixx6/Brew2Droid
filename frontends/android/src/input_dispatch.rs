//! Resposta à fila nativa, antes dos receptores do jogo e da interface.

use android_activity::{
    InputStatus,
    input::{InputEvent, Keycode},
};

static BACK_REQUESTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub const MENU_EXIT:u32=1;
pub const MENU_DISMISSED:u32=2;
pub const MENU_COUNTER:u32=4;
pub const MENU_KEYPAD:u32=8;
pub const MENU_COUNTER_VALUE:u32=16;
pub const MENU_KEYPAD_VALUE:u32=32;
static MENU_ACTIONS:std::sync::atomic::AtomicU32=std::sync::atomic::AtomicU32::new(0);
#[cfg(target_os="android")]
static WAKER:std::sync::Mutex<Option<(usize,android_activity::AndroidAppWaker)>>=std::sync::Mutex::new(None);
#[cfg(target_os="android")]
pub fn install_waker(app:&android_activity::AndroidApp){*WAKER.lock().unwrap_or_else(|p|p.into_inner())=Some((app.activity_as_ptr() as usize,app.create_waker()));}
#[cfg(target_os="android")]
pub fn remove_waker(app:&android_activity::AndroidApp){let mut current=WAKER.lock().unwrap_or_else(|p|p.into_inner());if current.as_ref().is_some_and(|(owner,_)|*owner==app.activity_as_ptr() as usize){*current=None;}}
fn wake(){#[cfg(target_os="android")]if let Some((_,waker))=WAKER.lock().unwrap_or_else(|p|p.into_inner()).as_ref(){waker.wake();}}
pub fn take_menu()->u32{MENU_ACTIONS.swap(0,std::sync::atomic::Ordering::AcqRel)}
pub fn queue_menu(action:i32){
    let accepted=MENU_ACTIONS.fetch_update(std::sync::atomic::Ordering::AcqRel,std::sync::atomic::Ordering::Acquire,|old|Some(match action{
        0=>old|MENU_DISMISSED,1=>old|MENU_EXIT,2=>(old|MENU_COUNTER)&!MENU_COUNTER_VALUE,3=>old|MENU_COUNTER|MENU_COUNTER_VALUE,
        4=>(old|MENU_KEYPAD)&!MENU_KEYPAD_VALUE,5=>old|MENU_KEYPAD|MENU_KEYPAD_VALUE,_=>return None,
    })).is_ok();if accepted{wake();}
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_brew_handset_GameActivity_nativeMenuAction(_: *mut std::ffi::c_void,_: *mut std::ffi::c_void,action:i32){queue_menu(action);}

pub fn take_back() -> bool {
    BACK_REQUESTED.swap(false,std::sync::atomic::Ordering::AcqRel)
}

// O callback Android só sinaliza; sessão e menu pertencem à linha nativa.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_brew_handset_GameActivity_nativeRequestBack(
    _: *mut std::ffi::c_void, _: *mut std::ffi::c_void,
) { BACK_REQUESTED.store(true,std::sync::atomic::Ordering::Release);wake(); }

// A Activity consome a tecla antes do fallback Back da NativeActivity.
// O sentinela (-1,-1,false) solta tudo ao perder foco ou ao exceder a fila limitada.
static PHYSICAL_BACK: std::sync::Mutex<Vec<(i32,i32,bool)>> = std::sync::Mutex::new(Vec::new());
pub fn take_physical_back() -> Vec<(i32,i32,bool)> {
    std::mem::take(&mut *PHYSICAL_BACK.lock().unwrap_or_else(|p|p.into_inner()))
}
/// Cada borda precisa alcançar a sessão antes da próxima; um toque pode caber entre quadros.
pub fn deliver_physical_back(mut receive: impl FnMut(i32,i32,bool)) {
    for (device,scan,down) in take_physical_back() { receive(device,scan,down); }
}
/// A remoção invalida também bordas que ainda não chegaram ao laço nativo.
pub fn discard_physical_back_device(device:i32) {
    PHYSICAL_BACK.lock().unwrap_or_else(|p|p.into_inner()).retain(|(id,_,_)|*id!=device);
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_brew_handset_GameActivity_nativePhysicalBack(
    _: *mut std::ffi::c_void,_: *mut std::ffi::c_void,device:i32,scan:i32,down:u8,
) {
    if device<0||scan<0{return;}
    let mut queue=PHYSICAL_BACK.lock().unwrap_or_else(|p|p.into_inner());
    let report=(device,scan,down!=0);
    if queue.last()==Some(&report){return;}
    if queue.len()>=64 {queue.clear();queue.push((-1,-1,false));}
    queue.push(report);drop(queue);wake();
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_brew_handset_GameActivity_nativeReleasePhysicalBack(
    _: *mut std::ffi::c_void,_: *mut std::ffi::c_void,
) {
    let mut queue=PHYSICAL_BACK.lock().unwrap_or_else(|p|p.into_inner());
    queue.clear();queue.push((-1,-1,false));drop(queue);wake();
}
pub fn dispatch(event: &InputEvent, receive: impl FnOnce()) -> InputStatus {
    dispatch_with_volume(event,false,receive)
}
pub fn dispatch_with_volume(event: &InputEvent, volume_keys_mappable:bool, receive: impl FnOnce()) -> InputStatus {
    // Handled impede o fallback da NativeActivity. O Android precisa receber ambas as
    // bordas e as repetições para ajustar o volume e mostrar seu painel normalmente.
    if matches!(event, InputEvent::KeyEvent(key) if matches!(
        key.key_code(), Keycode::VolumeMute
    ) || (!volume_keys_mappable && matches!(key.key_code(),Keycode::VolumeUp | Keycode::VolumeDown))) {
        return InputStatus::Unhandled;
    }
    receive();
    InputStatus::Handled
}
