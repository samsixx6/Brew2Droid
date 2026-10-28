#[path = "../../android/src/profile_store.rs"]
mod profile_store;
#[allow(dead_code)]
#[path = "../../android/src/game_options.rs"]
mod game_options;
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
fn fresh_default_profile_anchors_at_top_while_explicit_saved_gravity_survives() {
    let f=Fixture::new();
    let game=f.ask(json!({"op":"game"}));
    let mut settings=zeebx::ui::settings::Settings::default();
    assert_eq!(profile_runtime::prepare(&game["values"],&mut settings).unwrap().options.gravity,game_options::ScreenGravity::Top);
    let new=f.create("New profile");
    assert_eq!(f.ask(json!({"op":"view","id":new}))["values"]["gravity"],"top");
    for gravity in ["center","bottom"] {
        f.ask(json!({"op":"update","id":"default","patch":{"gravity":gravity}}));
        f.ask(json!({"op":"update","id":"default","patch":{"smooth":true}}));
        let reopened=f.ask(json!({"op":"game"}));
        assert_eq!(reopened["values"]["gravity"],gravity);
        let runtime=profile_runtime::prepare(&reopened["values"],&mut settings).unwrap();
        assert_eq!(runtime.options.gravity.as_str(),gravity);
    }
}

#[test]
fn legacy_explicit_center_gravity_survives_the_new_top_factory_default() {
    let f=Fixture::new();let legacy=f.private.join("game-options");std::fs::create_dir_all(&legacy).unwrap();
    let path=legacy.join(format!("{}.json",f.content().as_str()));
    let bytes=br#"{"orientation":"auto","gravity":"center","controls":"system_default"}"#;
    std::fs::write(&path,bytes).unwrap();
    for _ in 0..2 {
        let view=f.ask(json!({"op":"game"}));
        assert_eq!(view["values"]["gravity"],"center","An explicit legacy gravity must not turn into the new factory default");
        assert_eq!(view["mode"],"custom");
    }
    assert_eq!(std::fs::read(&path).unwrap(),bytes);
    assert_eq!(f.ask(json!({"op":"game_reset"}))["values"]["gravity"],"top");
}

#[test]
fn invalid_storage_roots_and_corrupt_game_links_are_reported_without_resetting_choices() {
    let f=Fixture::new();std::fs::create_dir_all(f.public.parent().unwrap()).unwrap();std::fs::write(&f.public,b"occupied").unwrap();
    assert!(profile_store::request(&f.public,&f.private,&f.config,Some(&f.game),&json!({"op":"list"})).is_err());
    assert_eq!(std::fs::read(&f.public).unwrap(),b"occupied");std::fs::remove_file(&f.public).unwrap();
    f.ask(json!({"op":"list"}));let catalog=std::fs::read(f.public.join("profiles.json")).unwrap();
    let links=f.private.join("profile-games");std::fs::write(&links,b"occupied").unwrap();
    assert!(profile_store::request(&f.public,&f.private,&f.config,Some(&f.game),&json!({"op":"game"})).is_err(),"An inaccessible link directory must not silently use the default");
    assert_eq!(std::fs::read(&links).unwrap(),b"occupied");std::fs::remove_file(&links).unwrap();std::fs::create_dir(&links).unwrap();
    let path=links.join(format!("{}.json",f.content().as_str()));std::fs::write(&path,b"broken game choice").unwrap();
    assert!(profile_store::request(&f.public,&f.private,&f.config,Some(&f.game),&json!({"op":"game"})).is_err());
    assert_eq!(std::fs::read(&path).unwrap(),b"broken game choice");assert_eq!(std::fs::read(f.public.join("profiles.json")).unwrap(),catalog);
}

#[test]
fn manual_legacy_resolution_migrates_once_and_reset_removes_its_effect() {
    let f=Fixture::new();let displays=f.private.join("handset-displays");
    zeebx::handset::display::set_override(&f.game,&displays,Some(zeebx::handset::types::DisplayGeometry{width:320,height:240,depth:16})).unwrap();
    let view=f.ask(json!({"op":"game"}));assert_eq!(view["mode"],"custom");assert_eq!(view["values"]["resolution"]["width"],320);
    f.ask(json!({"op":"game_reset"}));assert!(f.ask(json!({"op":"game"}))["values"]["resolution"].is_null());
    assert_eq!(zeebx::handset::display::resolve(&f.game,&displays).unwrap().geometry.width,320,"The old file is preserved for recovery");
}

#[test]
fn new_profile_catalog_exposes_an_editable_protected_default_and_fifty_percent_buttons() {
    let f=Fixture::new();let list=f.ask(json!({"op":"list"}));
    assert_eq!(list["default_id"],"default");assert_eq!(list["profiles"].as_array().unwrap().len(),1);
    assert_eq!(list["profiles"][0]["protected"],true);assert_eq!(list["profiles"][0]["selected"],true);
    let view=f.ask(json!({"op":"view","id":"default"}));
    assert_eq!(view["values"]["transparency"],50);assert_eq!(view["values"]["controls_enabled"],true);
    assert_eq!(view["values"]["resolution"],Value::Null);assert_eq!(view["values"]["scaling"],"fit");
    assert!(view["values"].get("vibration").is_none());assert!(view["values"].get("fps_counter").is_none());
    let changed=f.ask(json!({"op":"update","id":"default","patch":{"smooth":true}}));assert_eq!(changed["values"]["smooth"],true);
    assert!(f.public.join("profiles.json").is_file());
}

#[test]
fn stored_eighty_percent_profile_survives_reopening_and_unrelated_edits() {
    let f=Fixture::new();f.ask(json!({"op":"list"}));let path=f.public.join("profiles.json");
    let mut raw:Value=serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    raw["profiles"]["default"]["values"]["transparency"]=json!(80);
    std::fs::write(&path,serde_json::to_vec(&raw).unwrap()).unwrap();
    for _ in 0..2 {
        assert_eq!(f.ask(json!({"op":"game"}))["values"]["transparency"],80);
    }
    f.ask(json!({"op":"update","id":"default","patch":{"smooth":true}}));
    assert_eq!(f.ask(json!({"op":"game"}))["values"]["transparency"],80);
    let saved:Value=serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["profiles"]["default"]["values"]["transparency"],80);
}

#[test]
fn profile_rename_preserves_identity_and_selected_default() {
    let f=Fixture::new();let id=f.create("Profile 3");f.ask(json!({"op":"set_default","id":id}));
    f.ask(json!({"op":"rename","id":id,"name":"Portrait"}));
    let list=f.ask(json!({"op":"list"}));assert_eq!(list["default_id"],id);
    let view=f.ask(json!({"op":"game"}));assert_eq!(view["profile_name"],"Portrait");assert_eq!(view["mode"],"default");
    assert_eq!(f.ask(json!({"op":"view","id":id}))["name"],"Portrait");
}

#[test]
fn untouched_games_follow_the_checked_default_without_materializing_a_snapshot() {
    let f=Fixture::new();assert_eq!(f.ask(json!({"op":"game"}))["profile_name"],"Default");
    let id=f.create("Wide");f.ask(json!({"op":"update","id":id,"patch":{"orientation":"landscape","transparency":85}}));
    f.ask(json!({"op":"set_default","id":id}));let view=profile_store::effective(&f.public,&f.private,&f.config,&f.game,&f.content()).unwrap();
    assert_eq!(view["values"]["orientation"],"landscape");assert_eq!(view["values"]["transparency"],85);
    assert!(!f.private.join("profile-games").join(format!("{}.json",f.content().as_str())).exists());
}

#[test]
fn editing_one_game_field_freezes_all_effective_values_until_a_profile_is_loaded_again() {
    let f=Fixture::new();let id=f.create("Profile 3");
    f.ask(json!({"op":"update","id":id,"patch":{"gravity":"top","transparency":75,"bindings":{"5":"key:97"},"layout":{"enter":{"scale":1.5,"visible":false}}}}));
    f.ask(json!({"op":"game_profile","id":id}));
    let custom=f.ask(json!({"op":"game_update","patch":{"smooth":true}}));
    assert_eq!(custom["mode"],"custom");assert_eq!(custom["profile_name"],"Custom");
    f.ask(json!({"op":"update","id":id,"patch":{"gravity":"bottom","transparency":15,"bindings":{},"layout":{}}}));
    let view=f.ask(json!({"op":"game"}));assert_eq!(view["values"]["gravity"],"top");assert_eq!(view["values"]["transparency"],75);
    assert_eq!(view["values"]["bindings"]["5"],"key:97");assert_eq!(view["values"]["layout"]["enter"]["visible"],false);
    f.ask(json!({"op":"game_profile","id":id}));f.ask(json!({"op":"update","id":id,"patch":{"transparency":90}}));
    let view=f.ask(json!({"op":"game"}));assert_eq!(view["mode"],"profile");assert_eq!(view["profile_name"],"Profile 3");assert_eq!(view["values"]["transparency"],90);
}

#[test]
fn reset_reconnects_to_current_default_and_future_default_changes() {
    let f=Fixture::new();f.ask(json!({"op":"game_update","patch":{"gravity":"bottom"}}));
    let id=f.create("Portrait");f.ask(json!({"op":"set_default","id":id}));
    assert_eq!(f.ask(json!({"op":"game_reset"}))["profile_name"],"Portrait");
    f.ask(json!({"op":"set_default","id":"default"}));assert_eq!(f.ask(json!({"op":"game"}))["profile_name"],"Default");
}

#[test]
fn invalid_profile_edits_and_protected_delete_leave_catalog_bytes_unchanged() {
    let f=Fixture::new();f.ask(json!({"op":"list"}));let original=std::fs::read(f.public.join("profiles.json")).unwrap();
    for request in [json!({"op":"delete","id":"default"}),json!({"op":"create","name":" default "}),json!({"op":"create","name":""}),json!({"op":"update","id":"default","patch":{"transparency":101}}),json!({"op":"update","id":"default","patch":{"vibration":false}}),json!({"op":"update","id":"default","patch":{"layout":{"unknown":{"scale":1,"visible":true}}}}),json!({"op":"update","id":"default","patch":{"layout":{"enter":{"scale":0,"visible":true}}}}),json!({"op":"update","id":"default","patch":{"bindings":{"5":"key:97","6":"key:97"}}})] {
        assert!(profile_store::request(&f.public,&f.private,&f.config,Some(&f.game),&request).is_err(),"accepted {request}");
        assert_eq!(std::fs::read(f.public.join("profiles.json")).unwrap(),original);
    }
}

#[test]
fn deleting_a_named_profile_rehomes_its_links_without_changing_custom_games() {
    let f=Fixture::new();let id=f.create("Temporary");f.ask(json!({"op":"set_default","id":id}));f.ask(json!({"op":"game_profile","id":id}));
    f.ask(json!({"op":"delete","id":id}));assert_eq!(f.ask(json!({"op":"game"}))["profile_name"],"Default");
    assert_eq!(f.ask(json!({"op":"list"}))["default_id"],"default");
    f.ask(json!({"op":"game_update","patch":{"transparency":67}}));let id=f.create("Other");f.ask(json!({"op":"set_default","id":id}));f.ask(json!({"op":"delete","id":id}));
    assert_eq!(f.ask(json!({"op":"game"}))["values"]["transparency"],67);
}

#[test]
fn profile_names_are_data_not_paths_and_content_links_survive_module_rename() {
    let mut f=Fixture::new();let id=f.create("../portable profile");f.ask(json!({"op":"game_profile","id":id}));
    f.ask(json!({"op":"game_update","patch":{"orientation":"portrait"}}));let moved=f.root.join("renamed.mod");std::fs::rename(&f.game,&moved).unwrap();f.game=moved;
    assert_eq!(f.ask(json!({"op":"game"}))["profile_name"],"Custom");assert_eq!(f.ask(json!({"op":"game"}))["values"]["orientation"],"portrait");
    std::fs::write(&f.game,b"different content at same path").unwrap();assert_eq!(f.ask(json!({"op":"game"}))["mode"],"default");
    assert!(!f.root.join("portable profile").exists());
}

#[test]
fn legacy_global_preferences_and_mapping_migrate_without_modifying_the_source() {
    let f=Fixture::new();let raw=json!({"graphics":{"scaling":"integer","smooth":true},"controles_na_tela":{"modo":"nunca"},"handset_keypad":{"transparency":61,"vibration":false},"future":{"value":42}}).to_string();
    std::fs::write(&f.config,&raw).unwrap();std::fs::write(f.private.join("button-mapping.json"),r#"{"version":1,"bindings":{"5":"key:97"}}"#).unwrap();
    let view=f.ask(json!({"op":"view","id":"default"}));assert_eq!(view["values"]["scaling"],"integer");assert_eq!(view["values"]["smooth"],true);
    assert_eq!(view["values"]["controls_enabled"],false);assert_eq!(view["values"]["transparency"],39);assert_eq!(view["values"]["bindings"]["5"],"key:97");
    assert_eq!(std::fs::read_to_string(&f.config).unwrap(),raw);
}

#[test]
fn existing_game_options_migrate_to_custom_once_and_reset_does_not_resurrect_them() {
    let f=Fixture::new();std::fs::create_dir_all(f.private.join("game-options")).unwrap();
    let old=f.private.join("game-options").join(format!("{}.json",f.content().as_str()));
    std::fs::write(&old,r#"{"orientation":"portrait","gravity":"bottom","controls":"off"}"#).unwrap();
    let view=f.ask(json!({"op":"game"}));assert_eq!(view["mode"],"custom");assert_eq!(view["values"]["orientation"],"portrait");assert_eq!(view["values"]["controls_enabled"],false);
    f.ask(json!({"op":"game_reset"}));assert_eq!(f.ask(json!({"op":"game"}))["mode"],"default");assert!(old.is_file());
}

#[test]
fn corrupt_or_unsupported_catalog_is_reported_without_overwriting_it() {
    for bytes in [b"{broken".as_slice(),br#"{"version":999}"#.as_slice()] {
        let f=Fixture::new();std::fs::create_dir_all(&f.public).unwrap();std::fs::write(f.public.join("profiles.json"),bytes).unwrap();
        assert!(profile_store::request(&f.public,&f.private,&f.config,Some(&f.game),&json!({"op":"create","name":"New"})).is_err());
        assert_eq!(std::fs::read(f.public.join("profiles.json")).unwrap(),bytes);
    }
}

#[test]
fn serialized_profile_updates_preserve_both_concurrent_patches() {
    let f=Fixture::new();f.ask(json!({"op":"list"}));
    std::thread::scope(|scope| {for patch in [json!({"orientation":"landscape"}),json!({"transparency":81})] {let f=&f;scope.spawn(move||{f.ask(json!({"op":"update","id":"default","patch":patch}));});}});
    let view=f.ask(json!({"op":"view","id":"default"}));assert_eq!(view["values"]["orientation"],"landscape");assert_eq!(view["values"]["transparency"],81);
}

#[test]
fn runtime_profile_applies_only_profile_owned_fields_and_keeps_global_audio_renderer_and_fps() {
    let f=Fixture::new();let view=f.ask(json!({"op":"update","id":"default","patch":{"orientation":"landscape","gravity":"top","scaling":"stretch","keep_aspect":false,"smooth":true,"controls_enabled":false,"transparency":82,"resolution":{"width":176,"height":220,"depth":16},"bindings":{"5":"key:97"},"layout":{"enter":{"visible":false}}}}));
    let mut settings=zeebx::ui::settings::Settings::default();settings.audio.volume=31;settings.graphics.gpu_rasterizer=true;settings.debug.fps_counter=true;
    let prepared=profile_runtime::prepare(&view["values"],&mut settings).unwrap();
    assert_eq!(prepared.options.orientation,game_options::ScreenOrientation::Landscape);assert_eq!(prepared.options.gravity,game_options::ScreenGravity::Top);assert_eq!(prepared.options.controls,game_options::OnScreenControls::Off);
    assert_eq!(prepared.transparency,82);assert_eq!(prepared.resolution.unwrap().width,176);assert_eq!(prepared.bindings["5"],"key:97");assert_eq!(prepared.layout["enter"]["visible"],false);
    assert!(settings.graphics.smooth);assert!(!settings.graphics.keep_aspect);assert_eq!(serde_json::to_value(settings.graphics.scaling).unwrap(),"stretch");assert_eq!(settings.audio.volume,31);assert!(settings.graphics.gpu_rasterizer&&settings.debug.fps_counter);
}

#[test]
fn runtime_auto_ignores_legacy_manual_overrides_after_reset_and_manual_profile_still_wins() {
    let f=Fixture::new();let module=f.root.join("variant_176x220.mod");std::fs::copy(&f.game,&module).unwrap();
    let old=zeebx::handset::display::set_override(&module,&f.private.join("handset-displays"),Some(zeebx::handset::types::DisplayGeometry{width:640,height:480,depth:16})).unwrap();
    assert_eq!(old.geometry.width,640);let automatic=profile_runtime::geometry(&module,&old.module_sha256,None);assert_eq!((automatic.width,automatic.height),(176,220));
    let manual=profile_runtime::geometry(&module,&old.module_sha256,Some(zeebx::handset::types::DisplayGeometry{width:320,height:240,depth:16}));assert_eq!((manual.width,manual.height),(320,240));
}

#[test]
fn lowercase_root_migration_preserves_sources_conflicts_and_corruption() {
    let f=Fixture::new();f.ask(json!({"op":"update","id":"default","patch":{"resolution":{"width":480,"height":800,"depth":16}}}));
    let target=f.root.join("new/profiles");
    profile_store::migrate_catalog(&f.public,&target).unwrap();
    assert_eq!(std::fs::read(f.public.join("profiles.json")).unwrap(),std::fs::read(target.join("profiles.json")).unwrap());
    profile_store::migrate_catalog(&f.public,&target).unwrap();
    f.ask(json!({"op":"rename","id":"default","name":"Changed"}));
    let before=std::fs::read(target.join("profiles.json")).unwrap();
    assert!(profile_store::migrate_catalog(&f.public,&target).is_err());
    assert_eq!(std::fs::read(target.join("profiles.json")).unwrap(),before);
    std::fs::write(target.join("profiles.json"),b"corrupt").unwrap();
    assert!(profile_store::migrate_catalog(&f.public,&target).is_err());
    assert_eq!(std::fs::read(target.join("profiles.json")).unwrap(),b"corrupt");
}
#[test]
fn effective_library_uses_cached_identity_after_module_is_removed() {
    let f=Fixture::new();let id=f.create("Tall");
    f.ask(json!({"op":"update","id":id,"patch":{"resolution":{"width":480,"height":800,"depth":16}}}));
    f.ask(json!({"op":"game_profile","id":id}));
    let content=f.content();let mut library=json!({"games":[{"path":f.game,"content_id":content.as_str(),"resolution":{"geometry":{"width":240,"height":320,"depth":16},"source":"fallback","automatic":{"geometry":{"width":240,"height":320,"depth":16},"source":"fallback"}}}]});
    std::fs::remove_file(&f.game).unwrap();
    profile_store::project_library(&f.public,&f.private,&f.config,&mut library).unwrap();
    assert_eq!(library["games"][0]["resolution"]["geometry"]["width"],480);
    profile_store::request(&f.public,&f.private,&f.config,None,&json!({"op":"update","id":id,"patch":{"resolution":null}})).unwrap();
    profile_store::project_library(&f.public,&f.private,&f.config,&mut library).unwrap();
    assert_eq!(library["games"][0]["resolution"]["geometry"]["width"],240);
}

#[test]
fn zeebo_banks_follow_profiles_and_custom_games_keep_both_snapshots() {
    let f=Fixture::new();
    f.ask(json!({"op":"update","id":"default","patch":{"button_type":"zeebo","bindings":{"1":"key:96"},"zeebo_bindings":{"b4":"key:96"},"layout":{"digit_1":{"scale":1.4}}}}));
    let inherited=f.ask(json!({"op":"game"}));assert_eq!(inherited["button_type"],"zeebo");assert_eq!(inherited["automatic_resolution"]["width"],640);
    f.ask(json!({"op":"game_update","patch":{"zeebo_bindings":{"b2":"key:96"}}}));
    f.ask(json!({"op":"update","id":"default","patch":{"zeebo_bindings":{"b1":"key:96"}}}));
    let custom=f.ask(json!({"op":"game"}));assert_eq!(custom["mode"],"custom");assert_eq!(custom["values"]["zeebo_bindings"],json!({"b2":"key:96"}));assert_eq!(custom["values"]["bindings"],json!({"1":"key:96"}));
    let phone=f.ask(json!({"op":"game_update","patch":{"button_type":"cellphone"}}));assert_eq!(phone["button_type"],"cellphone");assert_eq!(phone["values"]["layout"],inherited["values"]["layout"]);
    let console=f.ask(json!({"op":"game_update","patch":{"button_type":"zeebo"}}));assert_eq!(console["values"]["zeebo_bindings"],custom["values"]["zeebo_bindings"]);
    let reset=f.ask(json!({"op":"game_update","patch":{"zeebo_bindings":{}}}));assert_eq!(reset["values"]["bindings"],phone["values"]["bindings"]);
    let inherited=f.ask(json!({"op":"game_reset"}));assert_eq!(inherited["values"]["zeebo_bindings"],json!({"b1":"key:96"}));
}

#[test]
fn explicit_zeebo_library_resolution_is_projected_without_reopening_rom() {
    let f=Fixture::new();let content=f.content();f.ask(json!({"op":"game_update","patch":{"button_type":"zeebo"}}));
    let mut library=json!({"games":[{"content_id":content.as_str(),"path":f.game,"resolution":{"geometry":{"width":240,"height":320,"depth":16},"automatic":{"geometry":{"width":240,"height":320,"depth":16},"source":"fallback"},"module_sha256":"unknown"}}]});
    std::fs::remove_file(&f.game).unwrap();
    profile_store::project_library(&f.public,&f.private,&f.config,&mut library).unwrap();assert_eq!(library["games"][0]["button_type"],"zeebo");assert_eq!(library["games"][0]["resolution"]["geometry"]["width"],640);
    // A projeção repetida precisa usar o cache original ao voltar para Cellphone.
    let path=f.private.join("profile-games").join(format!("{}.json",content.as_str()));let mut raw:Value=serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();raw["values"]["button_type"]=json!("cellphone");std::fs::write(&path,serde_json::to_vec(&raw).unwrap()).unwrap();
    profile_store::project_library(&f.public,&f.private,&f.config,&mut library).unwrap();assert_eq!(library["games"][0]["resolution"]["geometry"]["width"],240);
}
