use serde_json::json;
#[path = "../frontends/android/src/profile_values.rs"]
mod values;
#[test]
fn old_profiles_gain_auto_type_without_losing_phone_mapping_or_layout() {
    let old = json!({"bindings":{"1":"key:96"},"layout":{"digit_1":{"x":0.3,"y":0.8}}});
    let next = values::normalize(&old).unwrap();
    assert_eq!(next["button_type"], "auto");
    assert_eq!(next["zeebo_bindings"], json!({}));
    assert_eq!(next["bindings"], old["bindings"]);
    assert_eq!(next["layout"], old["layout"]);
}
#[test]
fn switching_types_keeps_both_mapping_banks_and_phone_layout() {
    let p=values::patch(&values::defaults(),&json!({"bindings":{"1":"key:96"},"zeebo_bindings":{"b2":"key:96","lx+":"axis:0:+"},"layout":{"digit_1":{"scale":1.5}},"button_type":"zeebo"})).unwrap();
    let q = values::patch(&p, &json!({"button_type":"cellphone"})).unwrap();
    assert_eq!(q["bindings"], p["bindings"]);
    assert_eq!(q["zeebo_bindings"], p["zeebo_bindings"]);
    assert_eq!(q["layout"], p["layout"]);
    let reset = values::patch(&q, &json!({"zeebo_bindings":{}})).unwrap();
    assert_eq!(reset["bindings"], q["bindings"]);
}
#[test]
fn zeebo_mapping_rejects_bad_targets_tokens_and_duplicates() {
    for patch in [
        json!({"button_type":"psx"}),
        json!({"zeebo_bindings":{"0":"key:96"}}),
        json!({"zeebo_bindings":{"b1":"key:3"}}),
        json!({"zeebo_bindings":{"b1":"key:96","b2":"key:96"}}),
    ] {
        assert!(values::patch(&values::defaults(), &patch).is_err());
    }
}
#[test]
fn rally_auto_screen_is_native_and_not_guessed_from_its_folder() {
    let d = zeebx::handset::display::automatic(
        "12252627d50bbd0646cdf1fb8a8a87b271714578b8e0e58a62e0fe0c719b94d7",
        std::path::Path::new("Games/renamed-176x220/rmp.mod"),
    );
    assert_eq!((d.geometry.width, d.geometry.height), (640, 480));
}
