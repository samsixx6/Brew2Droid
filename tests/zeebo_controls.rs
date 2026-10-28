#[path = "../frontends/android/src/zeebo_controls.rs"]
mod controls;
use controls::{Controls, Layout};
fn bit(name: &str) -> u32 {
    1 << zeebx::input::Pad::button_by_name(name).unwrap()
}
#[test]
fn layouts_match_reference_centers_and_keep_art_inside_screen() {
    let a = Layout::new(1679., 938.);
    let b = a.button("b1").unwrap();
    assert!((b.center()[0] - 1464.).abs() < 1.);
    assert!((b.center()[1] - 704.).abs() < 1.);
    let p = Layout::new(591., 1280.);
    assert!((p.sticks[0].center()[0] - 208.).abs() < 1.);
    assert!((p.sticks[0].center()[1] - 1176.).abs() < 1.);
    for (w, h) in [(360., 800.), (800., 360.), (320., 480.), (2000., 900.)] {
        let l = Layout::new(w, h);
        for (_, r) in &l.buttons {
            assert!(r.x >= 0. && r.y >= 0. && r.x + r.w <= w + 1. && r.y + r.h <= h + 1.);
        }
    }
}
#[test]
fn stick_is_radially_clamped_with_inner_fully_inside_outer() {
    let mut c = Controls::default();
    c.resize(1679., 938.);
    let r = c.layout.sticks[0];
    let p = r.center();
    assert!(c.begin(1, p));
    c.move_to(1, [p[0] + 1000., p[1] + 1000.]);
    let inner = c.inner_center(0);
    let distance = (inner[0] - p[0]).hypot(inner[1] - p[1]);
    assert!((distance + c.layout.inner_sizes[0] * 0.5 - r.radius()).abs() < 0.1);
    let a = c.pad().axes;
    assert!((a[0] - 90).abs() <= 1 && (a[1] - 90).abs() <= 1);
    c.end(1);
    assert_eq!(c.pad().axes, [0; 4]);
}
#[test]
fn both_sticks_and_multiple_buttons_stay_independent() {
    let mut c = Controls::default();
    c.resize(591., 1280.);
    let l = c.layout.sticks[0].center();
    let r = c.layout.sticks[1].center();
    c.begin(1, l);
    c.begin(2, r);
    c.move_to(1, [l[0] + 300., l[1]]);
    c.move_to(2, [r[0], r[1] - 300.]);
    let b = c.layout.button("b1").unwrap().center();
    c.begin(3, b);
    c.begin(4, b);
    assert_eq!(c.pad().axes, [128, 0, 0, -128]);
    assert_ne!(c.pad().buttons & bit("b1"), 0);
    c.end(3);
    assert_ne!(c.pad().buttons & bit("b1"), 0);
    c.end(4);
    assert_eq!(c.pad().buttons & bit("b1"), 0);
    assert_eq!(c.pad().axes, [128, 0, 0, -128]);
}
#[test]
fn resize_disable_and_cancel_release_every_touch() {
    let mut c = Controls::default();
    c.resize(591., 1280.);
    let p = c.layout.button("home").unwrap().center();
    c.begin(1, p);
    assert_ne!(c.pad().buttons, 0);
    assert!(c.resize(1280., 591.));
    assert_eq!(c.pad().buttons, 0);
    c.begin(2, c.layout.button("b2").unwrap().center());
    c.set_enabled(false);
    assert_eq!(c.pad().buttons, 0);
    assert!(!c.begin(3, p));
    c.set_enabled(true);
    c.begin(4, c.layout.button("b3").unwrap().center());
    c.clear();
    assert_eq!(c.pad().buttons, 0);
}
#[test]
fn dpad_can_hold_diagonals_and_neutral_center() {
    let mut c = Controls::default();
    c.resize(1679., 938.);
    let p = c.layout.dpad.center();
    c.begin(1, [p[0] + 80., p[1] - 80.]);
    assert_eq!(
        c.pad().buttons & (bit("up") | bit("right")),
        bit("up") | bit("right")
    );
    c.move_to(1, p);
    assert_eq!(c.pad().buttons, 0);
    c.end(1);
}

#[test]
fn viewport_origin_and_rotation_do_not_reuse_stale_touch_regions() {
    let mut c = Controls::default();
    assert!(!c.begin(1, [0., 0.]));
    c.viewport(591., 1280., [10., 24.]);
    let b = c.layout.button("b1").unwrap().center();
    assert!(c.begin(1, c.local([b[0] + 10., b[1] + 24.])));
    assert_ne!(c.pad().buttons, 0);
    assert!(c.viewport(591., 1280., [0., 0.]));
    assert_eq!(c.pad().buttons, 0);
    c.resize(0., 0.);
    assert!(!c.begin(2, [0., 0.]));
}
