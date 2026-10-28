// A fila nativa só existe no Android; o adaptador deixa executar a política real no host.
extern crate self as android_activity;

#[derive(Debug, PartialEq)]
pub enum InputStatus {
    Handled,
    Unhandled,
}

#[allow(dead_code)]
pub mod input {
    #[derive(Clone, Copy)]
    pub enum Keycode {
        VolumeUp,
        VolumeDown,
        VolumeMute,
        Back,
        Menu,
        DpadUp,
        Unknown,
    }

    #[derive(Clone, Copy)]
    pub enum KeyAction {
        Down,
        Up,
    }

    pub struct KeyEvent {
        pub code: Keycode,
        pub action: KeyAction,
        pub repeat_count: i32,
    }

    impl KeyEvent {
        pub fn key_code(&self) -> Keycode {
            self.code
        }
    }

    pub enum InputEvent {
        KeyEvent(KeyEvent),
        MotionEvent,
        Other,
    }
}

#[path = "../frontends/android/src/input_dispatch.rs"]
mod input_dispatch;

#[test]
fn native_menu_actions_preserve_latest_checked_states_and_dismiss_without_replaying() {
    use input_dispatch::*;
    let send=|action|Java_io_github_brew_handset_GameActivity_nativeMenuAction(std::ptr::null_mut(),std::ptr::null_mut(),action);
    take_menu();send(3);send(2);send(5);send(0);
    let actions=take_menu();assert_eq!(actions&MENU_COUNTER_VALUE,0);assert_ne!(actions&MENU_COUNTER,0);assert_ne!(actions&MENU_KEYPAD,0);assert_ne!(actions&MENU_KEYPAD_VALUE,0);assert_ne!(actions&MENU_DISMISSED,0);
    assert_eq!(take_menu(),0,"Consumed actions must not replay");send(1);send(99);assert_eq!(take_menu(),MENU_EXIT);
}

#[test]
fn system_back_callback_reaches_native_menu_once_per_consumption() {
    assert!(!input_dispatch::take_back());
    input_dispatch::Java_io_github_brew_handset_GameActivity_nativeRequestBack(std::ptr::null_mut(),std::ptr::null_mut());
    assert!(input_dispatch::take_back(),"System Back must reach the native menu loop");
    assert!(!input_dispatch::take_back(),"A consumed Back must not close the menu next frame");
    input_dispatch::Java_io_github_brew_handset_GameActivity_nativeRequestBack(std::ptr::null_mut(),std::ptr::null_mut());
    input_dispatch::Java_io_github_brew_handset_GameActivity_nativeRequestBack(std::ptr::null_mut(),std::ptr::null_mut());
    assert!(input_dispatch::take_back());
    assert!(!input_dispatch::take_back());
}

use input::{InputEvent, KeyAction, KeyEvent, Keycode};

#[test]
fn volume_press_repeat_and_release_return_to_android_without_game_input() {
    for code in [Keycode::VolumeUp, Keycode::VolumeDown, Keycode::VolumeMute] {
        for (action, repeat_count) in [
            (KeyAction::Down, 0),
            (KeyAction::Down, 1),
            (KeyAction::Down, 5),
            (KeyAction::Up, 0),
        ] {
            let event = InputEvent::KeyEvent(KeyEvent {
                code,
                action,
                repeat_count,
            });
            let mut gameplay_calls = 0;
            let status = input_dispatch::dispatch(&event, || gameplay_calls += 1);
            assert_eq!(
                status,
                InputStatus::Unhandled,
                "Android must apply the volume key"
            );
            assert_eq!(
                gameplay_calls, 0,
                "volume must bypass both gameplay receivers"
            );
        }
    }
}

#[test]
fn navigation_game_keys_and_touch_keep_the_existing_input_path() {
    for event in [
        InputEvent::KeyEvent(KeyEvent {
            code: Keycode::Back,
            action: KeyAction::Up,
            repeat_count: 0,
        }),
        InputEvent::KeyEvent(KeyEvent {
            code: Keycode::Menu,
            action: KeyAction::Down,
            repeat_count: 0,
        }),
        InputEvent::KeyEvent(KeyEvent {
            code: Keycode::DpadUp,
            action: KeyAction::Down,
            repeat_count: 0,
        }),
        InputEvent::KeyEvent(KeyEvent {
            code: Keycode::Unknown,
            action: KeyAction::Down,
            repeat_count: 0,
        }),
        InputEvent::MotionEvent,
        InputEvent::Other,
    ] {
        let mut gameplay_calls = 0;
        let status = input_dispatch::dispatch(&event, || gameplay_calls += 1);
        assert_eq!(status, InputStatus::Handled);
        assert_eq!(gameplay_calls, 1);
    }
}

#[test]
fn opted_in_volume_edges_and_repeats_go_to_game_while_mute_stays_android() {
    for code in [Keycode::VolumeUp,Keycode::VolumeDown] {
        for (action,repeat_count) in [(KeyAction::Down,0),(KeyAction::Down,4),(KeyAction::Up,0)] {
            let event=InputEvent::KeyEvent(KeyEvent{code,action,repeat_count});let mut calls=0;
            assert_eq!(input_dispatch::dispatch_with_volume(&event,true,||calls+=1),InputStatus::Handled);
            assert_eq!(calls,1);
        }
    }
    let mute=InputEvent::KeyEvent(KeyEvent{code:Keycode::VolumeMute,action:KeyAction::Down,repeat_count:0});
    assert_eq!(input_dispatch::dispatch_with_volume(&mute,true,||panic!("Mute reached game")),InputStatus::Unhandled);
}

#[test]
fn physical_back_queue_is_bounded_and_focus_reset_cannot_replay_held_keys() {
    use input_dispatch::*;
    take_physical_back();
    let send=|device,scan,down|Java_io_github_brew_handset_GameActivity_nativePhysicalBack(std::ptr::null_mut(),std::ptr::null_mut(),device,scan,down);
    send(-1,158,1);send(1,-1,1);assert!(take_physical_back().is_empty());
    for scan in 0..65 {send(1,scan,1);}
    let reports=take_physical_back();assert!(reports.len()<=64);assert_eq!(reports[0],(-1,-1,false));assert_eq!(reports.last(),Some(&(1,64,true)));
    send(1,158,1);send(1,158,1);assert_eq!(take_physical_back(),vec![(1,158,true)]);
    send(1,158,1);Java_io_github_brew_handset_GameActivity_nativeReleasePhysicalBack(std::ptr::null_mut(),std::ptr::null_mut());
    assert_eq!(take_physical_back(),vec![(-1,-1,false)]);assert!(take_physical_back().is_empty());
}
