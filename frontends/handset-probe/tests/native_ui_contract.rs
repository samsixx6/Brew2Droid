//! Contrato real da ponte de IO; mudar o merge para substituir o JSON perde campos e falha.
#[path = "../../android/src/native_ui_model.rs"]
mod model;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize,Ordering};
static NEXT: AtomicUsize = AtomicUsize::new(0);
#[test]
fn handset_keypad_defaults_are_fifty_percent_visible_and_vibrate() {
 let t=Temp::new();let value=model::snapshot(&t.file(),&t.0).unwrap();
 assert_eq!(value["settings"]["handset_keypad"]["transparency"],50);
 assert_eq!(value["settings"]["handset_keypad"]["vibration"],true);
 let typed=model::keypad_settings(&t.file()).unwrap();assert_eq!(typed.transparency,50);assert!(typed.vibration);
}
#[test]
fn fresh_android_audio_defaults_match_runtime_without_writing_settings() {
 let t=Temp::new();let path=t.file();
 let view=model::snapshot(&path,&t.0).unwrap();let runtime=model::settings_for_android(&path).unwrap();
 assert_eq!(view["settings"]["audio"]["enabled"],true);assert_eq!(view["settings"]["audio"]["volume"],100);
 assert!(runtime.audio.enabled);assert_eq!(runtime.audio.volume,100);assert!(!path.exists());
}
#[test]
fn partial_audio_defaults_fill_only_missing_values_and_preserve_saved_mute_or_zero() {
 let t=Temp::new();let path=t.file();
 for (raw,on,volume) in [(json!({}),true,100),(json!({"audio":{"enabled":false}}),false,100),(json!({"audio":{"volume":0}}),true,0),(json!({"audio":{"volume":63}}),true,63)]{
  std::fs::write(&path,raw.to_string()).unwrap();
  let view=model::snapshot(&path,&t.0).unwrap();let runtime=model::settings_for_android(&path).unwrap();
  assert_eq!(view["settings"]["audio"]["enabled"],on);assert_eq!(view["settings"]["audio"]["volume"],volume);
  assert_eq!(runtime.audio.enabled,on);assert_eq!(runtime.audio.volume,volume);
 }
}
#[test]
fn later_audio_and_button_choices_survive_launches_and_unrelated_edits() {
 let t=Temp::new();let path=t.file();let saved=json!({"audio":{"enabled":false,"volume":23},"handset_keypad":{"transparency":39,"vibration":false}});
 std::fs::write(&path,saved.to_string()).unwrap();
 for _ in 0..2{
  let view=model::snapshot(&path,&t.0).unwrap();let runtime=model::settings_for_android(&path).unwrap();
  assert_eq!(view["settings"]["audio"]["volume"],23);assert!(!runtime.audio.enabled);assert_eq!(runtime.audio.volume,23);
  assert_eq!(model::keypad_settings(&path).unwrap().transparency,39);
 }
 model::update(&path,&t.0,&json!({"debug":{"fps_counter":true}})).unwrap();
 let after:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
 assert_eq!(after["audio"],saved["audio"]);assert_eq!(after["handset_keypad"],saved["handset_keypad"]);
}
#[test]
fn handset_keypad_edits_persist_without_changing_mapping_or_future_fields() {
 let t=Temp::new();let p=t.file();let before=json!({"future":{"keep":42},"controles_na_tela":{"modo":"nunca","opacidade":15}});
 std::fs::write(&p,before.to_string()).unwrap();
 model::update(&p,&t.0,&json!({"handset_keypad":{"transparency":65,"vibration":true}})).unwrap();
 let saved:Value=serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
 assert_eq!(saved["handset_keypad"]["transparency"],65);assert_eq!(saved["handset_keypad"]["vibration"],true);
 assert_eq!(saved["future"],before["future"]);assert_eq!(saved["controles_na_tela"],before["controles_na_tela"]);
 let typed=model::keypad_settings(&p).unwrap();assert_eq!(typed.transparency,65);assert!(typed.vibration);
}
#[test]
fn invalid_handset_keypad_edits_do_not_change_original_bytes() {
 let t=Temp::new();let p=t.file();let bytes=b"{\"handset_keypad\":{\"transparency\":25,\"vibration\":false}}";std::fs::write(&p,bytes).unwrap();
 for patch in [json!({"handset_keypad":{"transparency":-1}}),json!({"handset_keypad":{"transparency":101}}),json!({"handset_keypad":{"transparency":0.5}}),json!({"handset_keypad":{"transparency":"50"}}),json!({"handset_keypad":{"vibration":1}})] {
  assert!(model::update(&p,&t.0,&patch).is_err());assert_eq!(std::fs::read(&p).unwrap(),bytes);
 }
}
#[test]
fn android_typed_settings_default_to_gpu_but_keep_explicit_cpu() {
 let t=Temp::new();let p=t.file();
 assert!(model::settings_for_android(&p).unwrap().graphics.gpu_rasterizer);
 std::fs::write(&p,b"{\"graphics\":{\"gpu_rasterizer\":false},\"audio\":{\"volume\":37}}").unwrap();
 let settings=model::settings_for_android(&p).unwrap();assert!(!settings.graphics.gpu_rasterizer);assert_eq!(settings.audio.volume,37);
 assert_eq!(model::snapshot(&p,&t.0).unwrap()["settings"]["graphics"]["gpu_rasterizer"],settings.graphics.gpu_rasterizer);
 std::fs::write(&p,b"invalid").unwrap();assert!(model::settings_for_android(&p).is_err());
}
#[test] fn keypad_toggle_persists_without_changing_other_touch_preferences() {
 let t=Temp::new();let p=t.file();let before=json!({"controles_na_tela":{"modo":"automatico","escala":137,"opacidade":15},"future":{"keep":true}});std::fs::write(&p,before.to_string()).unwrap();
 for mode in ["nunca","automatico"] {model::update(&p,&t.0,&json!({"controles_na_tela":{"modo":mode}})).unwrap();let value:Value=serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();assert_eq!(value["controles_na_tela"]["modo"],mode);assert_eq!(value["controles_na_tela"]["escala"],137);assert_eq!(value["controles_na_tela"]["opacidade"],15);assert_eq!(value["future"],before["future"]);}
 assert!(model::update(&p,&t.0,&json!({"controles_na_tela":{"modo":"invalid"}})).is_err());
}
struct CountAlloc;
static LIVE:AtomicUsize=AtomicUsize::new(0);
static PEAK:AtomicUsize=AtomicUsize::new(0);
// Mede alocações reais do consumidor. Reter todas as capas antes de reduzir estoura o teto.
unsafe impl std::alloc::GlobalAlloc for CountAlloc {
 unsafe fn alloc(&self,layout:std::alloc::Layout)->*mut u8 {let p=unsafe{std::alloc::GlobalAlloc::alloc(&std::alloc::System,layout)};if !p.is_null(){let current=LIVE.fetch_add(layout.size(),Ordering::SeqCst)+layout.size();PEAK.fetch_max(current,Ordering::SeqCst);}p}
 unsafe fn dealloc(&self,p:*mut u8,layout:std::alloc::Layout){LIVE.fetch_sub(layout.size(),Ordering::SeqCst);unsafe{std::alloc::GlobalAlloc::dealloc(&std::alloc::System,p,layout)}}
}
#[global_allocator]static ALLOC:CountAlloc=CountAlloc;
struct Temp(PathBuf);
#[test]
fn game_discovery_recurses_through_arbitrarily_named_group_folders() {
 let t=Temp::new();let games=t.0.join("Games");
 let groups=["176x220/Other Language", "My favourites/Anything at all/39367", "Unknown Resolution/Collection A/More games"];
 for (i,group) in groups.iter().enumerate(){let package=games.join(group).join(format!("Test Game {i} (176x220) 1.0"));let inner=package.join("id");std::fs::create_dir_all(&inner).unwrap();std::fs::write(inner.join("cryptic.mod"),[0u8;16]).unwrap();std::fs::write(package.join("id.mif"),[0u8;32]).unwrap();std::fs::write(package.join("outer-data.bin"),[0u8;64]).unwrap();}
 let result=model::scan_with_displays(&games,&t.0.join("art"),&t.0.join("profiles")).unwrap();let rows=result["games"].as_array().unwrap();assert_eq!(rows.len(),3);
 for (i,game) in rows.iter().enumerate(){assert_eq!(game["title"],format!("Test Game {i}"));assert_eq!(game["version"],"1.0");assert_eq!(game["folder_size_bytes"],112);assert_eq!(game["resolution"]["geometry"]["width"],176);let module=PathBuf::from(game["path"].as_str().unwrap());assert_eq!(zeebx::library::resolve_mif(&module).unwrap(),module.parent().unwrap().parent().unwrap().join("id.mif"));}
}
#[test] fn discovery_and_manual_display_share_the_same_identity() {
 let t=Temp::new();let module=t.0.join("game_176x220.mod");std::fs::write(&module,[0u8;16]).unwrap();let profiles=t.0.join("profiles");
 let read=||model::scan_with_displays(&t.0,&t.0.join("art"),&profiles).unwrap()["games"][0]["resolution"].clone();
 assert_eq!(read()["geometry"]["width"],176);
 let selected=model::game_display(&module,&profiles,"{\"width\":320,\"height\":240,\"depth\":16}").unwrap();
 assert_eq!(selected["source"],"manual");assert_eq!(read()["geometry"]["width"],320);
 assert_eq!(model::game_display(&module,&profiles,"").unwrap(),selected);
 assert!(model::game_display(&module,&profiles,"{\"width\":0,\"height\":240,\"depth\":16}").is_err());
 assert_eq!(model::game_display(&module,&profiles,"null").unwrap()["source"],"name");
 assert_eq!(std::fs::read(&module).unwrap(),[0u8;16]);
}
impl Temp { fn new()->Self {let p=std::env::temp_dir().join(format!("brew-native-ui-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));std::fs::create_dir_all(&p).unwrap();Self(p)} fn file(&self)->PathBuf{self.0.join("settings.json")} }
impl Drop for Temp {fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
#[test] fn patch_preserves_hidden_and_future_fields() {
 let t=Temp::new();let p=t.file();let before=json!({"audio":{"enabled":true,"volume":80},"debug":{"fps_counter":true},"graphics":{"resolucao_interna":7},"future":{"token":"keep"},"controls":{"players":[]}});
 std::fs::write(&p,before.to_string()).unwrap();
 model::update(&p,&t.0,&json!({"audio":{"volume":37},"debug":{"fps_counter":false}})).unwrap();
 let after:Value=serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap();
 assert_eq!(after["audio"]["volume"],37);assert_eq!(after["audio"]["enabled"],true);assert_eq!(after["debug"]["fps_counter"],false);
 for key in ["future","controls","graphics"] {assert_eq!(after[key],before[key]);}
}
#[test] fn invalid_json_is_not_silently_replaced() {
 let t=Temp::new();let p=t.file();std::fs::write(&p,b"broken config").unwrap();
 assert!(model::snapshot(&p,&t.0).is_err());assert!(model::update(&p,&t.0,&json!({"audio":{"volume":20}})).is_err());assert_eq!(std::fs::read(p).unwrap(),b"broken config");
}
#[test] fn invalid_or_hidden_edits_leave_original_bytes() {
 let t=Temp::new();let p=t.file();let bytes=b"{\"audio\":{\"volume\":80}}";std::fs::write(&p,bytes).unwrap();
 for patch in [json!({"audio":{"volume":101}}),json!({"graphics":{"scaling":"impossible"}}),json!({"graphics":{"resolucao_interna":2}}),json!({"audio":{"enabled":"yes"}})] {
  assert!(model::update(&p,&t.0,&patch).is_err(),"{patch}");assert_eq!(std::fs::read(&p).unwrap(),bytes);
 }
}
#[test] fn missing_settings_use_core_defaults_and_unicode_paths() {
 let t=Temp::new();let dir=t.0.join("jogos de ação");std::fs::create_dir(&dir).unwrap();
 let snap=model::snapshot(&t.file(),&dir).unwrap();assert_eq!(snap["settings"]["roms_dir"],dir.to_str().unwrap());assert!(snap["settings"]["audio"]["volume"].is_number());
 assert!(snap["languages"].as_array().unwrap().len()>=4);assert!(snap["strings"]["handset.graphics.keep_aspect"].is_string());
}
#[test] fn scan_distinguishes_empty_and_missing_directory() {
 let t=Temp::new();let empty=t.0.join("empty");std::fs::create_dir(&empty).unwrap();
 assert_eq!(model::scan(&empty,&t.0.join("art")).unwrap()["games"].as_array().unwrap().len(),0);
 assert!(model::scan(&t.0.join("missing"),&t.0.join("art")).is_err());
}
#[test] fn scan_keeps_unicode_module_path_and_bounds_real_cover_bytes() {
 let t=Temp::new();let module=t.0.join("ação de teste.mod");std::fs::write(&module,[0u8;16]).unwrap();
 let mut bmp=vec![0u8;1078+512*512];bmp[0..2].copy_from_slice(b"BM");let size=bmp.len() as u32;
 for (at,value) in [(2,size),(10,1078),(14,40),(18,512),(22,512),(34,512*512),(46,256)] {bmp[at..at+4].copy_from_slice(&value.to_le_bytes());}
 bmp[26..28].copy_from_slice(&1u16.to_le_bytes());bmp[28..30].copy_from_slice(&8u16.to_le_bytes());bmp[60]=255;bmp[1078..].fill(1);
 std::fs::write(module.with_extension("bmp"),bmp).unwrap();
 let result=model::scan(&t.0,&t.0.join("art")).unwrap();let games=result["games"].as_array().unwrap();assert_eq!(games.len(),1);assert_eq!(games[0]["path"],module.to_str().unwrap());
 let art=std::fs::read(games[0]["art"].as_str().unwrap()).unwrap();let width=u32::from_le_bytes(art[0..4].try_into().unwrap());let height=u32::from_le_bytes(art[4..8].try_into().unwrap());
 assert!(width<=192 && height<=192);assert_eq!(art.len(),8+(width*height*4) as usize);assert_eq!(&art[8..12],&[255,0,0,255]);
 assert_eq!(std::fs::read(module).unwrap(),[0u8;16]);
}
#[test] fn write_failure_is_reported_without_replacing_a_directory() {
 let t=Temp::new();let path=t.0.join("directory");std::fs::create_dir(&path).unwrap();
 assert!(model::update(&path,&t.0,&json!({"audio":{"volume":30}})).is_err());assert!(path.is_dir());
}
#[test] fn simultaneous_menu_edits_do_not_lose_other_changes() {
 let t=Temp::new();let p=t.file();let before=json!({"audio":{"enabled":true},"graphics":{"smooth":true,"keep_aspect":true,"gpu_rasterizer":true},"debug":{"fps_counter":true,"overlay":true,"timeline":true},"future":{"padding":"x".repeat(65536)}});
 std::fs::write(&p,before.to_string()).unwrap();let fields=[("audio","enabled"),("graphics","smooth"),("graphics","keep_aspect"),("graphics","gpu_rasterizer"),("debug","fps_counter"),("debug","overlay"),("debug","timeline")];
 let gate=std::sync::Arc::new(std::sync::Barrier::new(fields.len()));let mut threads=Vec::new();
 for (group,field) in fields {let path=p.clone();let root=t.0.clone();let gate=gate.clone();threads.push(std::thread::spawn(move||{let mut patch=json!({});patch[group]=json!({});patch[group][field]=json!(false);gate.wait();model::update(&path,&root,&patch).unwrap();}));}
 for thread in threads {thread.join().unwrap();}let after:Value=serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
 for (group,field) in fields {assert_eq!(after[group][field],false,"Lost {group}.{field}");}
 assert_eq!(after["future"],before["future"]);
}
#[test] fn many_large_covers_do_not_accumulate_full_images_in_memory() {
 let t=Temp::new();let side=1024u32;let mut bmp=vec![0u8;1078+(side*side) as usize];bmp[0..2].copy_from_slice(b"BM");let size=bmp.len() as u32;
 for (at,value) in [(2,size),(10,1078),(14,40),(18,side),(22,side),(34,side*side),(46,256)] {bmp[at..at+4].copy_from_slice(&value.to_le_bytes());}
 bmp[26..28].copy_from_slice(&1u16.to_le_bytes());bmp[28..30].copy_from_slice(&8u16.to_le_bytes());bmp[60]=255;bmp[1078..].fill(1);
 for i in 0..12 {let module=t.0.join(format!("game{i:02}.mod"));std::fs::write(&module,[0u8;16]).unwrap();std::fs::write(module.with_extension("bmp"),&bmp).unwrap();}
 let base=LIVE.load(Ordering::SeqCst);PEAK.store(base,Ordering::SeqCst);
 let result=model::scan(&t.0,&t.0.join("art")).unwrap();let extra=PEAK.load(Ordering::SeqCst).saturating_sub(base);
 assert_eq!(result["games"].as_array().unwrap().len(),12);
 assert!(extra<24*1024*1024,"Scanner retained full covers: peak additional allocation {extra}");
}

#[test] fn organized_package_uses_its_title_version_and_complete_outer_folder_size() {
 let t=Temp::new();let games=t.0.join("Games");let package=games.join("Asphalt Urban GT 2 (240x320) 1.0.5");let inner=package.join("12345");
 std::fs::create_dir_all(inner.join("data/nested")).unwrap();let module=inner.join("cryptic.mod");std::fs::write(&module,[0u8;16]).unwrap();
 std::fs::write(package.join("12345.mif"),[0u8;60]).unwrap();std::fs::write(inner.join("data/nested/asset.bin"),[0u8;23]).unwrap();std::fs::write(package.join("cover.dat"),[0u8;7]).unwrap();
 let result=model::scan_with_displays(&games,&t.0.join("art"),&t.0.join("profiles")).unwrap();let game=&result["games"][0];
 assert_eq!(game["title"],"Asphalt Urban GT 2");assert_eq!(game["version"],"1.0.5");assert_eq!(game["game_folder"],package.to_str().unwrap());assert_eq!(game["folder_size_bytes"],106);
 assert_eq!(game["path"],module.to_str().unwrap());assert_eq!(game["resolution"]["geometry"]["width"],240);assert!(game.get("art").is_some());
}

#[test] fn package_title_preserves_device_and_language_tags_without_inventing_a_version() {
 let t=Temp::new();let package=t.0.join("Games/Sonic 2 (Nokia N73) [EN] (176x220)");let inner=package.join("34567");std::fs::create_dir_all(&inner).unwrap();std::fs::write(inner.join("s2.mod"),[0u8;8]).unwrap();
 let result=model::scan(&t.0.join("Games"),&t.0.join("art")).unwrap();let game=&result["games"][0];
 assert_eq!(game["title"],"Sonic 2 (Nokia N73) [EN]");assert!(game["version"].is_null());assert_eq!(game["folder_size_bytes"],8);
}

#[test] fn tagged_versions_keep_their_exact_label_and_device_tags_stay_in_the_title() {
 let t=Temp::new();let package=t.0.join("Games/Asphalt Urban GT 2 (240x320) [EN] (Nokia N73) v1.0.5_TIM");let inner=package.join("12345");std::fs::create_dir_all(&inner).unwrap();std::fs::write(inner.join("a2.mod"),[0u8;8]).unwrap();
 let result=model::scan(&t.0.join("Games"),&t.0.join("art")).unwrap();let game=&result["games"][0];assert_eq!(game["title"],"Asphalt Urban GT 2 [EN] (Nokia N73)");assert_eq!(game["version"],"v1.0.5_TIM");
}

#[test] fn resolution_collection_name_is_not_inherited_through_another_game_folder() {
 let t=Temp::new();let games=t.0.join("Games");let inner=games.join("Collection (240x320)/Actual Game/12345");std::fs::create_dir_all(&inner).unwrap();std::fs::write(inner.join("module.mod"),[0u8;8]).unwrap();std::fs::write(games.join("Collection (240x320)/other.bin"),[0u8;100]).unwrap();
 let result=model::scan(&games,&t.0.join("art")).unwrap();let game=&result["games"][0];assert_eq!(game["title"],"module");assert_eq!(PathBuf::from(game["game_folder"].as_str().unwrap()),inner);assert_eq!(game["folder_size_bytes"],8);
}

#[test] fn unknown_resolution_package_keeps_its_readable_title_version_and_whole_folder() {
 let t=Temp::new();let package=t.0.join("Games/Unknown Resolution/Mystery Game (Unknown Resolution) 1.2.3");let inner=package.join("12345");std::fs::create_dir_all(&inner).unwrap();std::fs::write(inner.join("cryptic.mod"),[0u8;8]).unwrap();std::fs::write(package.join("12345.mif"),[0u8;60]).unwrap();std::fs::write(package.join("readme.txt"),[0u8;13]).unwrap();
 let result=model::scan(&t.0.join("Games"),&t.0.join("art")).unwrap();let game=&result["games"][0];assert_eq!(game["title"],"Mystery Game");assert_eq!(game["version"],"1.2.3");assert_eq!(game["folder_size_bytes"],81);assert_eq!(PathBuf::from(game["game_folder"].as_str().unwrap()),package);
}

#[test] fn exact_parent_manifest_identifies_an_outer_package_without_a_resolution_label() {
 let t=Temp::new();let package=t.0.join("Games/Proper Name");let inner=package.join("12345");std::fs::create_dir_all(&inner).unwrap();std::fs::write(inner.join("cryptic.mod"),[0u8;8]).unwrap();std::fs::write(package.join("12345.mif"),[0u8;60]).unwrap();std::fs::write(package.join("readme.txt"),[0u8;13]).unwrap();
 let result=model::scan(&t.0.join("Games"),&t.0.join("art")).unwrap();let game=&result["games"][0];assert_eq!(game["title"],"Proper Name");assert!(game["version"].is_null());assert_eq!(game["folder_size_bytes"],81);assert_eq!(PathBuf::from(game["game_folder"].as_str().unwrap()),package);
}

#[test] fn per_package_mod_mif_layout_counts_the_whole_outer_package_and_excludes_siblings() {
 let t=Temp::new();let games=t.0.join("Games");let package=games.join("Proper Name (176x220) 1.0");let inner=package.join("Mod/12345");std::fs::create_dir_all(inner.join("data")).unwrap();std::fs::create_dir_all(package.join("Mif")).unwrap();let module=inner.join("game.mod");std::fs::write(&module,[0u8;16]).unwrap();std::fs::write(inner.join("data/asset.bin"),[0u8;23]).unwrap();std::fs::write(package.join("Mif/12345.mif"),[0u8;60]).unwrap();std::fs::write(package.join("readme.txt"),[0u8;11]).unwrap();
 let sibling=games.join("Other Game (240x320) 2.0");std::fs::create_dir_all(&sibling).unwrap();std::fs::write(sibling.join("other.mod"),[0u8;200]).unwrap();std::fs::write(sibling.join("other.dat"),[0u8;300]).unwrap();
 let result=model::scan_with_displays(&games,&t.0.join("art"),&t.0.join("profiles")).unwrap();let rows=result["games"].as_array().unwrap();assert_eq!(rows.len(),2);let game=rows.iter().find(|game|PathBuf::from(game["path"].as_str().unwrap())==module).unwrap();assert_eq!(game["title"],"Proper Name");assert_eq!(game["version"],"1.0");assert_eq!(game["folder_size_bytes"],110);assert_eq!(PathBuf::from(game["game_folder"].as_str().unwrap()),package);assert_eq!(game["resolution"]["geometry"]["width"],176);
}

#[test] fn nested_mod_mif_pair_identifies_an_unlabelled_outer_package() {
 let t=Temp::new();let games=t.0.join("Games");let package=games.join("Proper Name");let inner=package.join("Mod/12345");std::fs::create_dir_all(&inner).unwrap();std::fs::create_dir_all(package.join("Mif")).unwrap();std::fs::write(inner.join("cryptic.mod"),[0u8;16]).unwrap();std::fs::write(package.join("Mif/12345.mif"),[0u8;60]).unwrap();std::fs::write(package.join("readme.txt"),[0u8;11]).unwrap();
 let result=model::scan(&games,&t.0.join("art")).unwrap();let game=&result["games"][0];assert_eq!(game["title"],"Proper Name");assert!(game["version"].is_null());assert_eq!(game["folder_size_bytes"],87);assert_eq!(PathBuf::from(game["game_folder"].as_str().unwrap()),package);
}

#[test] fn unrecognized_nested_mod_scope_does_not_report_an_inner_partial_total() {
 let t=Temp::new();let games=t.0.join("Games");let package=games.join("Unlabelled Package");let inner=package.join("Mod/12345");std::fs::create_dir_all(&inner).unwrap();std::fs::write(inner.join("cryptic.mod"),[0u8;16]).unwrap();std::fs::write(package.join("readme.txt"),[0u8;11]).unwrap();
 let result=model::scan(&games,&t.0.join("art")).unwrap();let game=&result["games"][0];assert!(game["folder_size_bytes"].is_null(),"{game}");
}

#[test] fn legacy_package_size_includes_only_its_matching_external_manifest() {
 let t=Temp::new();let games=t.0.join("Games");let folder=games.join("Mod/39367");let shared=games.join("Mif");std::fs::create_dir_all(folder.join("data")).unwrap();std::fs::create_dir_all(&shared).unwrap();
 std::fs::write(folder.join("game.mod"),[0u8;8]).unwrap();std::fs::write(folder.join("data/asset.bin"),[0u8;17]).unwrap();std::fs::write(shared.join("39367.mif"),[0u8;60]).unwrap();std::fs::write(shared.join("other.mif"),[0u8;100]).unwrap();
 std::fs::create_dir_all(games.join("Mod/other")).unwrap();std::fs::write(games.join("Mod/other/sibling.dat"),[0u8;200]).unwrap();
 let result=model::scan(&games,&t.0.join("art")).unwrap();let game=&result["games"][0];assert_eq!(PathBuf::from(game["game_folder"].as_str().unwrap()),folder);assert_eq!(game["folder_size_bytes"],85);assert!(game["version"].is_null());
}

#[test] fn collection_root_and_incomplete_package_size_stay_unknown() {
 let t=Temp::new();let games=t.0.join("Games");std::fs::create_dir_all(&games).unwrap();std::fs::write(games.join("flat.mod"),[0u8;8]).unwrap();
 let package=games.join("Too Deep (240x320) 1.0");let inner=package.join("12345");std::fs::create_dir_all(&inner).unwrap();std::fs::write(inner.join("deep.mod"),[0u8;8]).unwrap();
 let mut deep=inner;for _ in 0..34 {deep=deep.join("data");}std::fs::create_dir_all(&deep).unwrap();std::fs::write(deep.join("asset.bin"),[0u8;5]).unwrap();
 let result=model::scan(&games,&t.0.join("art")).unwrap();let rows=result["games"].as_array().unwrap();assert_eq!(rows.len(),2);for row in rows {assert!(row.get("folder_size_bytes").unwrap().is_null(),"{row}");}
}

#[cfg(windows)]
fn directory_link(link:&std::path::Path,target:&std::path::Path) {
 let output=std::process::Command::new("cmd.exe").args(["/d","/c","mklink","/J"]).arg(link).arg(target).output().unwrap();assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
}
#[cfg(unix)]
fn directory_link(link:&std::path::Path,target:&std::path::Path) {std::os::unix::fs::symlink(target,link).unwrap();}

#[test] fn discovery_skips_linked_directories_and_package_links_make_totals_unknown() {
 let t=Temp::new();let games=t.0.join("Games");let outside=t.0.join("outside");std::fs::create_dir_all(&games).unwrap();std::fs::create_dir_all(&outside).unwrap();std::fs::write(outside.join("external.mod"),[0u8;8]).unwrap();directory_link(&games.join("linked-collection"),&outside);
 assert!(model::scan(&games,&t.0.join("art")).unwrap()["games"].as_array().unwrap().is_empty());
 let package=games.join("Linked Game (240x320) 1.0");let inner=package.join("12345");std::fs::create_dir_all(&inner).unwrap();std::fs::write(inner.join("game.mod"),[0u8;8]).unwrap();directory_link(&inner.join("linked-assets"),&outside);
 let result=model::scan(&games,&t.0.join("art")).unwrap();let rows=result["games"].as_array().unwrap();assert_eq!(rows.len(),1);assert!(rows[0]["folder_size_bytes"].is_null());
}
