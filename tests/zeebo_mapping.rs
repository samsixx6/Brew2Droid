#[path = "../frontends/android/src/zeebo_mapping.rs"]
mod mapping;
#[path = "../frontends/android/src/zeebo_controls.rs"]
mod zeebo_controls;
use mapping::Map;
use serde_json::json;
fn bit(n: &str) -> u32 {
    1 << zeebo_controls::button_index(n).unwrap()
}
#[test]
fn controller_defaults_are_zeebo_buttons_not_phone_digits() {
    let mut m = Map::parse(&json!({}), false);
    assert!(m.key(1, 96, 0, true));
    assert_eq!(m.pad().buttons, bit("b1"));
    m.key(1, 96, 0, false);
    assert_eq!(m.pad().buttons, 0);
    assert!(!m.key(1, 8, 0, true));
    for (c, n) in [
        (99, "b2"),
        (100, "b3"),
        (97, "b4"),
        (102, "zl"),
        (103, "zr"),
        (108, "home"),
    ] {
        m.key(1, c, 0, true);
        assert_ne!(m.pad().buttons & bit(n), 0);
        m.key(1, c, 0, false);
    }
}
#[test]
fn inactive_phone_mapping_never_leaks_and_custom_zeebo_mapping_wins() {
    let mut m = Map::parse(&json!({"b2":"key:96","b1":"key:8"}), false);
    m.key(1, 96, 0, true);
    assert_eq!(m.pad().buttons, bit("b2"));
    m.key(1, 8, 0, true);
    assert_eq!(m.pad().buttons, bit("b2") | bit("b1"));
}
#[test]
fn separate_devices_and_touch_do_not_release_each_other() {
    let mut m = Map::parse(&json!({}), false);
    m.key(1, 96, 0, true);
    m.key(2, 96, 0, true);
    m.release_device(1);
    assert_eq!(m.pad().buttons, bit("b1"));
    let mut touch = zeebx::input::Pad::default();
    touch.buttons = bit("b1");
    m.release_device(2);
    assert_eq!(zeebo_controls::merge(m.pad(), touch).buttons, bit("b1"));
}
#[test]
fn analogue_axes_are_continuous_and_can_be_mapped_to_other_stick() {
    let mut m = Map::parse(&json!({"rx+":"axis:0:+"}), false);
    m.axis(1, 0, 0.5);
    assert_eq!(m.pad().axes[0], 0);
    assert!(m.pad().axes[2] > 0 && m.pad().axes[2] < 128);
    m.axis(1, 0, 0.);
    assert_eq!(m.pad().axes, [0; 4]);
    m.axis(1, 1, -1.);
    assert_eq!(m.pad().axes[1], -128);
    m.release_all();
    assert_eq!(m.pad().axes, [0; 4]);
}
#[test]
fn volume_optin_and_axis_calibration_are_respected() {
    let mut m = Map::parse(&json!({"b3":"key:24"}), false);
    assert!(!m.key(1, 24, 0, true));
    m.set_volume_keys_mappable(true);
    assert!(m.key(1, 24, 0, true));
    assert_ne!(m.pad().buttons & bit("b3"), 0);
    m.set_axis_range(1, 0, 0., 255., 5.);
    m.axis(1, 0, 127.5);
    assert_eq!(m.pad().axes[0], 0);
    m.axis(1, 0, 255.);
    assert_eq!(m.pad().axes[0], 128);
}

#[test]
fn trigger_aliases_agree_with_left_brake_and_right_gas(){
    let mut m=Map::parse(&json!({}),false);
    for (first,alias,name) in [(17,23,"zl"),(18,22,"zr")] {
        m.axis(1,first,1.);m.axis(1,alias,1.);assert_eq!(m.pad().buttons,bit(name));
        m.axis(1,first,0.);assert_eq!(m.pad().buttons,bit(name));m.axis(1,alias,0.);assert_eq!(m.pad().buttons,0);
    }
}
