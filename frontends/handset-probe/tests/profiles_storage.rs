//! Migração conservadora e projeção quente com ROMs sintéticas removidas.
#[allow(dead_code,unused_imports)]
#[path="../../android/src/profile_store.rs"] mod profile_store;
#[allow(dead_code,unused_imports)]
#[path="../../android/src/native_ui_model.rs"] mod model;
use serde_json::{json,Value};
use std::{path::PathBuf,sync::atomic::{AtomicU64,Ordering}};
static NEXT:AtomicU64=AtomicU64::new(0);
struct Fixture{root:PathBuf,public:PathBuf,private:PathBuf,config:PathBuf}
impl Fixture{
 fn new()->Self{let root=std::env::temp_dir().join(format!("brew-storage-batch-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));let public=root.join("current/profiles");let private=root.join("private");std::fs::create_dir_all(&private).unwrap();let config=private.join("zeebx.json");Self{root,public,private,config}}
 fn ask(&self,module:Option<&std::path::Path>,request:Value)->Value{profile_store::request(&self.public,&self.private,&self.config,module,&request).unwrap()}
}
impl Drop for Fixture{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.root);}}
#[test]
fn migrated_source_is_retained_while_new_catalog_remains_editable(){
 let f=Fixture::new();f.ask(None,json!({"op":"list"}));let old=f.root.join("old/profiles");
 std::fs::create_dir_all(&old).unwrap();std::fs::copy(f.public.join("profiles.json"),old.join("profiles.json")).unwrap();
 profile_store::migrate_catalog(&old,&f.public).unwrap();
 f.ask(None,json!({"op":"rename","id":"default","name":"My choice"}));
 profile_store::migrate_catalog(&old,&f.public).unwrap();
 assert_eq!(f.ask(None,json!({"op":"view","id":"default"}))["name"],"My choice");
 assert_ne!(std::fs::read(old.join("profiles.json")).unwrap(),std::fs::read(f.public.join("profiles.json")).unwrap());
}
#[test]
fn migration_same_directory_alias_does_not_create_a_conflict(){
 let f=Fixture::new();f.ask(None,json!({"op":"list"}));
 profile_store::migrate_catalog(&f.public,&f.public.join(".")).unwrap();
 assert!(!f.public.join(".legacy-profiles-import.json").exists());
}
#[test]
fn corrupt_legacy_catalog_never_creates_a_fresh_replacement(){
 let f=Fixture::new();let old=f.root.join("old");std::fs::create_dir_all(&old).unwrap();std::fs::write(old.join("profiles.json"),b"broken").unwrap();
 assert!(profile_store::migrate_catalog(&old,&f.public).is_err());assert!(!f.public.join("profiles.json").exists());assert_eq!(std::fs::read(old.join("profiles.json")).unwrap(),b"broken");
}
#[test]
fn cached_manual_resolution_migrates_to_custom_without_opening_missing_module(){
 let f=Fixture::new();std::fs::create_dir(f.private.join("handset-displays")).unwrap();let id=zeebx::storage::ContentId::from_reader(&b"missing-game"[..]).unwrap();
 let mut library=json!({"games":[{"path":f.root.join("missing.mod"),"content_id":id.as_str(),"resolution":{"source":"manual","geometry":{"width":480,"height":800,"depth":16},"automatic":{"source":"fallback","geometry":{"width":240,"height":320,"depth":16}}}}]});
 profile_store::project_library(&f.public,&f.private,&f.config,&mut library).unwrap();
 assert_eq!(library["games"][0]["profile_name"],"Custom");assert_eq!(library["games"][0]["resolution"]["geometry"]["width"],480);
 let raw:Value=serde_json::from_slice(&std::fs::read(f.private.join("profile-games").join(format!("{}.json",id.as_str()))).unwrap()).unwrap();assert_eq!(raw["mode"],"custom");
}
#[test]
fn six_hundred_cached_rows_follow_default_linked_and_custom_resolutions_without_roms(){
 let f=Fixture::new();let games=f.root.join("games");let art=f.root.join("art");let displays=f.private.join("handset-displays");std::fs::create_dir(&games).unwrap();
 for i in 0..600{std::fs::write(games.join(format!("g-{i:03}.mod")),format!("synthetic-content-{i}")).unwrap();}
 f.ask(None,json!({"op":"update","id":"default","patch":{"resolution":{"width":320,"height":240,"depth":16}}}));
 let linked=f.ask(None,json!({"op":"create","name":"Linked"}))["id"].as_str().unwrap().to_owned();
 f.ask(None,json!({"op":"update","id":linked,"patch":{"resolution":{"width":480,"height":800,"depth":16}}}));
 f.ask(Some(&games.join("g-000.mod")),json!({"op":"game_update","patch":{"resolution":{"width":800,"height":480,"depth":16}}}));
 f.ask(Some(&games.join("g-001.mod")),json!({"op":"game_profile","id":linked}));
 let cold=model::library_refresh(&games,&art,&displays,false).unwrap();assert_eq!(cold["processed"],600);
 assert!(cold["games"].as_array().unwrap().iter().all(|g|zeebx::storage::ContentId::parse(g["content_id"].as_str().unwrap()).is_some()));
 std::fs::remove_dir_all(&games).unwrap();let mut hot=model::library_cached(&games,&art).unwrap();assert_eq!(hot["processed"],0);assert_eq!(hot["games"].as_array().unwrap().len(),600);
 let start=std::time::Instant::now();profile_store::project_library(&f.public,&f.private,&f.config,&mut hot).unwrap();eprintln!("600 cached profile resolutions without ROMs: {} ms",start.elapsed().as_millis());
 let rows=hot["games"].as_array().unwrap();assert_eq!(rows[0]["resolution"]["geometry"]["width"],800);assert_eq!(rows[1]["resolution"]["geometry"]["width"],480);assert!(rows[2..].iter().all(|g|g["resolution"]["geometry"]["width"]==320));
 f.ask(None,json!({"op":"update","id":"default","patch":{"resolution":{"width":128,"height":160,"depth":16}}}));
 f.ask(None,json!({"op":"update","id":linked,"patch":{"resolution":{"width":176,"height":220,"depth":16}}}));
 profile_store::project_library(&f.public,&f.private,&f.config,&mut hot).unwrap();let rows=hot["games"].as_array().unwrap();assert_eq!(rows[0]["resolution"]["geometry"]["width"],800);assert_eq!(rows[1]["resolution"]["geometry"]["width"],176);assert!(rows[2..].iter().all(|g|g["resolution"]["geometry"]["width"]==128));
}

#[test]
fn missing_cached_content_identity_requires_one_refresh_then_reuses_metadata(){
 let f=Fixture::new();let games=f.root.join("games");let art=f.root.join("art");let displays=f.private.join("handset-displays");std::fs::create_dir(&games).unwrap();std::fs::write(games.join("single.mod"),b"synthetic cache identity").unwrap();
 model::library_refresh(&games,&art,&displays,false).unwrap();
 let index=std::fs::read_dir(&art).unwrap().map(|e|e.unwrap().path()).find(|p|p.file_name().unwrap().to_string_lossy().starts_with("library-v5-")).unwrap();
 let mut raw:Value=serde_json::from_slice(&std::fs::read(&index).unwrap()).unwrap();for row in raw["entries"].as_object_mut().unwrap().values_mut(){row["row"].as_object_mut().unwrap().remove("content_id");}
 std::fs::write(&index,serde_json::to_vec(&raw).unwrap()).unwrap();
 assert_eq!(model::library_cached(&games,&art).unwrap()["cached"],false);
 let repaired=model::library_refresh(&games,&art,&displays,false).unwrap();assert_eq!(repaired["processed"],1);assert!(repaired["games"][0]["content_id"].is_string());
 let hot=model::library_refresh(&games,&art,&displays,false).unwrap();assert_eq!(hot["processed"],0);assert_eq!(hot["reused"],1);
}

#[test]
fn obstructed_legacy_display_directory_does_not_silently_drop_custom_resolution(){
 let f=Fixture::new();std::fs::write(f.private.join("handset-displays"),b"occupied").unwrap();let id=zeebx::storage::ContentId::from_reader(&b"missing-game"[..]).unwrap();
 let mut library=json!({"games":[{"path":f.root.join("missing.mod"),"content_id":id.as_str(),"resolution":{"source":"manual","geometry":{"width":480,"height":800,"depth":16},"automatic":{"source":"fallback","geometry":{"width":240,"height":320,"depth":16}}}}]});
 assert!(profile_store::project_library(&f.public,&f.private,&f.config,&mut library).is_err());
 assert_eq!(std::fs::read(f.private.join("handset-displays")).unwrap(),b"occupied");
}
