//! Alterar som antes de abrir perfis não materializa nem inverte defaults de controles.
#[allow(dead_code)]
#[path = "../../android/src/native_ui_model.rs"]
mod model;
#[allow(dead_code)]
#[path = "../../android/src/profile_store.rs"]
mod profiles;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::atomic::{AtomicUsize, Ordering}};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("brew-default-integration-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }
}
impl Drop for Temp { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }

#[test]
fn editing_global_audio_before_first_profile_keeps_new_fifty_percent_default() {
    let temp = Temp::new();
    let config = temp.0.join("private/settings.json");
    let games = temp.0.join("brew2droid/games");
    model::update(&config, &games, &json!({"audio":{"enabled":false,"volume":0}})).unwrap();
    let raw: Value = serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
    assert!(raw.pointer("/handset_keypad/transparency").is_none());
    let profile = profiles::request(&temp.0.join("brew2droid/profiles"), config.parent().unwrap(), &config, None, &json!({"op":"view","id":"default"})).unwrap();
    assert_eq!(profile["values"]["transparency"], 50);
    let runtime = model::settings_for_android(&config).unwrap();
    assert!(!runtime.audio.enabled);
    assert_eq!(runtime.audio.volume, 0);
}

#[test]
fn editing_global_audio_keeps_existing_legacy_control_value_for_one_time_migration() {
    let temp = Temp::new();
    let config = temp.0.join("settings.json");
    std::fs::write(&config, br#"{"handset_keypad":{"transparency":61},"audio":{"enabled":false,"volume":23}}"#).unwrap();
    model::update(&config, &temp.0, &json!({"audio":{"volume":44}})).unwrap();
    let raw: Value = serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
    assert_eq!(raw["handset_keypad"]["transparency"], 61);
    let public = temp.0.join("brew2droid/profiles");
    let read_profile = || profiles::request(&public, &temp.0, &config, None, &json!({"op":"view","id":"default"})).unwrap();
    assert_eq!(read_profile()["values"]["transparency"], 39);
    assert_eq!(read_profile()["values"]["transparency"], 39);
    assert!(!model::settings_for_android(&config).unwrap().audio.enabled);
    assert_eq!(model::settings_for_android(&config).unwrap().audio.volume, 44);
}
