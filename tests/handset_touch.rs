use zeebx::handset::input::Key;
use zeebx::handset::touch::{KeyRegion, TouchKeypad};

#[test]
fn disabled_keypad_releases_touch_and_ignores_invisible_regions_but_keeps_hardware() {
    let mut touch=TouchKeypad::default();assert!(touch.is_enabled());
    touch.regions.push(KeyRegion{key:Key::Num5,min:[0.0,0.0],max:[40.0,40.0]});
    touch.pointers([[10.0,10.0]]);touch.hardware(Key::Left,true);
    touch.set_enabled(false);assert!(!touch.is_enabled());
    assert_eq!(touch.held(),1<<Key::Left as u8);
    touch.pointers([[10.0,10.0]]);assert_eq!(touch.held(),1<<Key::Left as u8);
    touch.hardware(Key::Left,false);assert_eq!(touch.held(),0);
    touch.set_enabled(true);touch.regions.push(KeyRegion{key:Key::Num5,min:[0.0,0.0],max:[40.0,40.0]});
    touch.pointers([[10.0,10.0]]);assert_eq!(touch.held(),1<<Key::Num5 as u8);
}

#[test]
fn lifting_one_pointer_keeps_the_other_key_held() {
    let mut touch = TouchKeypad::default();
    touch.regions = vec![
        KeyRegion {
            key: Key::Up,
            min: [0.0, 0.0],
            max: [40.0, 40.0],
        },
        KeyRegion {
            key: Key::Num5,
            min: [50.0, 0.0],
            max: [90.0, 40.0],
        },
    ];
    touch.pointers([[20.0, 20.0], [70.0, 20.0]]);
    assert_eq!(touch.held(), (1 << Key::Up as u8) | (1 << Key::Num5 as u8));
    touch.pointers([[70.0, 20.0]]);
    assert_eq!(touch.held(), 1 << Key::Num5 as u8);
    touch.hardware(Key::Left, true);
    touch.pointers([]);
    assert_eq!(touch.held(), 1 << Key::Left as u8);
    touch.clear();
    assert_eq!(touch.held(), 0);
}

#[test]
fn touch_release_does_not_release_the_same_hardware_key() {
    let mut touch = TouchKeypad::default();
    touch.regions.push(KeyRegion {
        key: Key::Select,
        min: [0.0, 0.0],
        max: [40.0, 40.0],
    });
    touch.hardware(Key::Select, true);
    touch.pointers([[10.0, 10.0]]);
    touch.pointers([]);
    assert_eq!(touch.held(), 1 << Key::Select as u8);
    touch.hardware(Key::Select, false);
    assert_eq!(touch.held(), 0);
}
