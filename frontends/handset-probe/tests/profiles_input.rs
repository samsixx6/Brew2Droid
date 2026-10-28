#[path = "../../android/src/profile_store.rs"]
mod profile_store;
#[allow(dead_code)]
#[path = "../../android/src/game_options.rs"]
mod game_options;
#[allow(dead_code)]
#[path = "../../android/src/profile_runtime.rs"]
mod profile_runtime;
use serde_json::{json, Value};
use std::{path::PathBuf, sync::atomic::{AtomicU64, Ordering}};
use zeebx::storage::ContentId;

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture { root: PathBuf, public: PathBuf, private: PathBuf, config: PathBuf, game: PathBuf }
impl Fixture {
    fn new() -> Self {
        let root=std::env::temp_dir().join(format!("brew-profiles-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
        let public=root.join("Brew2Droid/profiles");let private=root.join("private");std::fs::create_dir_all(&private).unwrap();
        let config=private.join("zeebx.json");let game=root.join("game.mod");std::fs::write(&game,b"synthetic-profile-module").unwrap();
        Self {root,public,private,config,game}
    }
    fn ask(&self, request: Value) -> Value { profile_store::request(&self.public,&self.private,&self.config,Some(&self.game),&request).unwrap() }
    fn create(&self,name:&str)->String {self.ask(json!({"op":"create","name":name}))["id"].as_str().unwrap().into()}
    fn content(&self)->ContentId {ContentId::from_reader(std::fs::File::open(&self.game).unwrap()).unwrap()}
}
impl Drop for Fixture {fn drop(&mut self){let _=std::fs::remove_dir_all(&self.root);}}

#[test]
fn volume_opt_in_inherits_and_custom_freezes_it_with_dormant_bindings() {
    let f=Fixture::new();let id=f.create("Volume");
    assert_eq!(f.ask(json!({"op":"game"}))["values"]["volume_keys_mappable"],false);
    f.ask(json!({"op":"update","id":id,"patch":{"volume_keys_mappable":true,"bindings":{"1":"key:24","clear":"key:25"}}}));
    f.ask(json!({"op":"game_profile","id":id}));
    let linked=f.ask(json!({"op":"game"}));
    assert_eq!(linked["values"]["volume_keys_mappable"],true);
    let mut settings=zeebx::ui::settings::Settings::default();
    assert!(profile_runtime::prepare(&linked["values"],&mut settings).unwrap().volume_keys_mappable);
    f.ask(json!({"op":"game_update","patch":{"smooth":true}}));
    f.ask(json!({"op":"update","id":id,"patch":{"volume_keys_mappable":false}}));
    let view=f.ask(json!({"op":"game"}));assert_eq!(view["values"]["volume_keys_mappable"],true);
    let off=f.ask(json!({"op":"game_update","patch":{"volume_keys_mappable":false}}));
    assert_eq!(off["values"]["bindings"]["1"],"key:24");
    assert!(!profile_runtime::prepare(&off["values"],&mut settings).unwrap().volume_keys_mappable);
    assert_eq!(f.ask(json!({"op":"game"}))["values"]["bindings"]["clear"],"key:25");
}

#[test]
fn runtime_missing_transparency_is_fifty_and_saved_values_are_preserved() {
    let mut settings=zeebx::ui::settings::Settings::default();
    assert_eq!(profile_runtime::prepare(&json!({}),&mut settings).unwrap().transparency,50);
    for value in [0,50,80,100] {assert_eq!(profile_runtime::prepare(&json!({"transparency":value}),&mut settings).unwrap().transparency,value as u8);}
}

#[test]
fn profile_layout_accepts_versioned_presets_and_rejects_malformed_geometry() {
    for preset in ["default","dpad","numpad"] {
        let layout=json!({"_geometry":{"version":2,"preset":preset},"enter":{"scale":1.0,"visible":true}});
        assert_eq!(profile_store::values::patch(&json!({}),&json!({"layout":layout})).unwrap()["layout"],layout);
    }
    for meta in [json!({"version":1,"preset":"default"}),json!({"version":2,"preset":"other"}),json!({"version":2,"preset":"default","extra":true}),json!({"version":2}),json!(null)] {
        assert!(profile_store::values::patch(&json!({}),&json!({"layout":{"_geometry":meta}})).is_err());
    }
}

#[test]
fn old_profile_without_volume_opt_in_keeps_saved_transparency_and_bindings() {
    let f=Fixture::new();f.ask(json!({"op":"list"}));let path=f.public.join("profiles.json");
    let mut raw:Value=serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    raw["profiles"]["default"]["values"].as_object_mut().unwrap().remove("volume_keys_mappable");
    raw["profiles"]["default"]["values"]["transparency"]=json!(50);
    raw["profiles"]["default"]["values"]["bindings"]=json!({"select":"key:24"});
    std::fs::write(&path,serde_json::to_vec(&raw).unwrap()).unwrap();
    let view=f.ask(json!({"op":"game"}));assert_eq!(view["values"]["transparency"],50);
    assert_eq!(view["values"]["volume_keys_mappable"],false);assert_eq!(view["values"]["bindings"]["select"],"key:24");
}
