#[path = "../frontends/android/src/density.rs"]
mod android_density;

#[test]
fn s25_density_is_preserved_before_and_after_the_native_window_exists() {
    let density = 600.0 / 160.0;
    assert_eq!(android_density::scale(density, None), density);
    assert_eq!(android_density::scale(density, Some(1440.0)), density);
}

#[test]
fn vivo_portrait_controls_use_phone_density() {
    assert_eq!(android_density::scale(2.0, None), 2.0);
    assert_eq!(android_density::scale(2.0, Some(720.0)), 2.0);
}

#[test]
fn narrow_windows_keep_a_usable_layout_without_upscaling_low_density_screens() {
    assert_eq!(android_density::scale(3.75, Some(720.0)), 2.0);
    assert_eq!(android_density::scale(1.0, Some(720.0)), 1.0);
}
