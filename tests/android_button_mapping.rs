#[path = "../frontends/android/src/button_mapping.rs"]
mod button_mapping;
#[path = "../frontends/android/src/keypad_art.rs"]
mod keypad_art;
#[path = "../frontends/android/src/keypad_layout.rs"]
mod keypad_layout;
use button_mapping::InputMap;
use zeebx::handset::input::Key;
fn mask(key: Key) -> u32 { 1 << key as u8 }

#[test]
fn defaults_preserve_doov_and_media_volume() {
    let mut map = InputMap::default();
    for (code, target) in [(7,Key::Num0),(16,Key::Num9),(17,Key::Star),(18,Key::Pound),
        (19,Key::Up),(20,Key::Down),(21,Key::Left),(22,Key::Right),(23,Key::Select),
        (66,Key::Select),(96,Key::Select),(1,Key::Soft1),(102,Key::Soft1),(82,Key::Soft1),
        (2,Key::Soft2),(103,Key::Soft2),(28,Key::Clear),(67,Key::Clear)] {
        assert!(map.key(1,code,0,true), "unhandled {code}");
        assert_eq!(map.held(),mask(target));
        assert!(map.key(1,code,0,false));
        assert_eq!(map.held(),0);
    }
    assert!(!map.key(-1,4,0,true));
    assert!(!map.key(1,4,0,true));
    assert!(!map.key(-1,4,158,true));
    assert!(map.key(1,4,158,true));
    assert_eq!(map.held(),mask(Key::Soft2));
    map.release_all();
    for code in [24,25,164,3,26] { assert!(!map.key(1,code,0,true)); }
}

#[test]
fn release_is_per_physical_source_and_device() {
    let mut map = InputMap::default();
    map.key(1,66,0,true);map.key(1,96,0,true);map.key(2,96,0,true);
    map.key(1,66,0,false);assert_eq!(map.held(),mask(Key::Select));
    map.key(1,96,0,true);map.key(1,96,0,false);assert_eq!(map.held(),mask(Key::Select));
    map.release_device(2);assert_eq!(map.held(),0);
    map.key(3,66,0,true);map.release_all();assert_eq!(map.held(),0);
}

#[test]
fn custom_binding_replaces_input_route_and_reset_restores_defaults() {
    let mut map=InputMap::parse(r#"{"version":1,"bindings":{"1":"key:96","clear":"key:29","soft1":"key:4:scan:158"}}"#);
    map.key(1,96,0,true);assert_eq!(map.held(),mask(Key::Num1));
    map.key(1,96,0,false);assert!(map.key(1,29,0,true));assert_eq!(map.held(),mask(Key::Clear));
    map.release_all();assert!(!map.key(1,4,0,true));
    map.key(1,4,158,true);assert_eq!(map.held(),mask(Key::Soft1));
    map.reset();assert_eq!(map.held(),0);assert_eq!(map.target("key:96"),Some(Key::Select));
    assert!(!map.key(1,24,0,true));
}

#[test]
fn axes_use_hysteresis_and_release_before_reversing() {
    let mut map=InputMap::parse(r#"{"version":1,"bindings":{"left":"axis:0:-","right":"axis:0:+","select":"axis:17:+"}}"#);
    assert!(map.axis(2,0,-0.8));assert_eq!(map.held(),mask(Key::Left));
    map.axis(2,0,-0.5);assert_eq!(map.held(),mask(Key::Left));
    map.axis(2,0,0.8);assert_eq!(map.held(),mask(Key::Right));
    map.axis(2,0,0.2);assert_eq!(map.held(),0);
    map.axis(2,17,0.9);assert_eq!(map.held(),mask(Key::Select));
    map.axis(2,17,0.0);assert_eq!(map.held(),0);
    map.axis(2,15,-1.0);map.key(2,21,0,true);map.axis(2,15,0.0);
    assert_eq!(map.held(),mask(Key::Left));map.key(2,21,0,false);assert_eq!(map.held(),0);
    map.axis(2,15,f32::NAN);assert_eq!(map.held(),0);
}

#[test]
fn malformed_or_unknown_schema_falls_back_without_system_key_bindings() {
    for text in ["broken",r#"{"version":2,"bindings":{"select":"key:97"}}"#,
        r#"{"version":1,"bindings":{"select":"key:4","clear":"key:3","1":"axis:100:+","2":"key:26","3":"key:24","4":"key:25","5":"key:164"}}"#] {
        let mut map=InputMap::parse(text);
        assert_eq!(map.target("key:96"),Some(Key::Select));
        assert!(!map.key(-1,4,0,true));assert!(!map.key(1,3,0,true));assert!(!map.key(1,26,0,true));
        for code in [24,25,164] {assert!(!map.key(1,code,0,true),"Media volume stays reserved even in edited files");}
    }
}

#[test]
fn controller_ranges_normalize_unsigned_sticks_and_triggers() {
    let mut map=InputMap::parse(r#"{"version":1,"bindings":{"left":"axis:0:-","right":"axis:0:+","select":"axis:23:+"}}"#);
    map.set_axis_range(9,0,0.0,255.0,8.0);map.set_axis_range(9,23,0.0,255.0,10.0);
    map.axis(9,0,127.5);assert_eq!(map.held(),0,"Unsigned stick center must be neutral");
    map.axis(9,0,5.0);assert_eq!(map.held(),mask(Key::Left));
    map.axis(9,0,110.0);assert_eq!(map.held(),0);
    map.axis(9,23,200.0);assert_eq!(map.held(),mask(Key::Select));
    map.axis(9,23,9.0);assert_eq!(map.held(),0);
    assert!(!map.ranges_loaded(9));map.mark_ranges_loaded(9);assert!(map.ranges_loaded(9));
    map.release_device(9);assert!(!map.ranges_loaded(9));
}

#[test]
fn keypad_pngs_load_once_and_nineteen_regions_follow_default_preset_rows() {
    let ctx=egui::Context::default();
    let mut keys=zeebx::handset::touch::TouchKeypad::default();
    let input=|| egui::RawInput {screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(360.0,800.0))),..Default::default()};
    let first=ctx.run(input(),|ctx|keypad_art::paint(&mut keys,ctx,0));
    assert!((19..=20).contains(&first.textures_delta.set.len()),"The 18 supplied faces and regular pressed overlay must decode");
    assert_eq!(keys.regions.len(),19);
    let center=|key| {let region=keys.regions.iter().find(|r|r.key==key).unwrap();[(region.min[0]+region.max[0])/2.0,(region.min[1]+region.max[1])/2.0]};
    let one=center(Key::Num1);let two=center(Key::Num2);let three=center(Key::Num3);
    assert!(one[0]<two[0]&&two[0]<three[0]);assert_eq!(one[1],two[1]);
    assert!(center(Key::Soft1)[0]<center(Key::Select)[0]&&center(Key::Select)[0]<center(Key::Soft2)[0]);
    assert_eq!(keys.regions.iter().filter(|r|r.key==Key::Up).count(),1);
    assert_eq!(keys.regions.iter().filter(|r|r.key==Key::Down).count(),1);
    assert!(!keys.regions.iter().any(|r|r.key==Key::Clear));
    assert!(center(Key::Select)[1]<center(Key::Left)[1]);
    assert!(center(Key::Pound)[1]<center(Key::Star)[1]&&center(Key::Star)[1]<center(Key::Num0)[1]);
    let select=center(Key::Select);keys.pointers([one,select]);assert_eq!(keys.held(),mask(Key::Num1)|mask(Key::Select));
    keys.hardware(Key::Num1,true);keys.pointers([]);assert_eq!(keys.held(),mask(Key::Num1));
    let second=ctx.run(input(),|ctx|keypad_art::paint(&mut keys,ctx,0));
    assert!(second.textures_delta.set.is_empty(),"A normal frame must reuse uploaded PNG textures");
}

#[test]
fn legacy_keypad_keeps_twenty_one_hit_regions_and_independent_duplicate_directions() {
    let ctx=egui::Context::default();let mut keys=zeebx::handset::touch::TouchKeypad::default();
    let input=||egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(360.0,800.0))),..Default::default()};
    let _=ctx.run(input(),|ctx|keypad_art::paint_layout(&mut keys,ctx,80,&serde_json::json!({"digit_1":{}})));
    assert_eq!(keys.regions.len(),21);assert_eq!(keys.regions.iter().filter(|r|r.key==Key::Up).count(),2);assert_eq!(keys.regions.iter().filter(|r|r.key==Key::Down).count(),2);
    let up:Vec<_>=keys.regions.iter().filter(|r|r.key==Key::Up).collect();let points:Vec<_>=up.iter().map(|r|[(r.min[0]+r.max[0])/2.0,(r.min[1]+r.max[1])/2.0]).collect();
    keys.pointers(points);assert_eq!(keys.held(),mask(Key::Up));keys.pointers([]);assert_eq!(keys.held(),0);
    let _=ctx.run(input(),|ctx|keypad_art::paint_layout(&mut keys,ctx,80,&serde_json::json!({"up_left":{"visible":false}})));
    assert_eq!(keys.regions.len(),20);assert_eq!(keys.regions.iter().filter(|r|r.key==Key::Up).count(),1);
}

#[test]
fn on_screen_buttons_never_reserve_game_space_or_move_its_gravity_area() {
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(591.0,1280.0));
    for layout in [serde_json::json!({}),serde_json::json!({"_geometry":{"version":2,"preset":"numpad"}}),
        serde_json::json!({"_geometry":{"version":2,"preset":"dpad"}}),serde_json::json!({"soft_left":{"x":0.5,"y":0.3,"scale":1.4}})] {
        for (enabled,alpha) in [(false,80),(true,0),(true,80),(true,100)] {
            let ctx=egui::Context::default();let mut keys=zeebx::handset::touch::TouchKeypad::default();keys.set_enabled(enabled);
            let mut available=egui::Rect::NOTHING;
            let _=ctx.run(egui::RawInput{screen_rect:Some(viewport),..Default::default()},|ctx|{
                keypad_art::paint_layout(&mut keys,ctx,alpha,&layout);
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{available=ui.available_rect_before_wrap();});
            });
            assert_eq!(available,viewport,"Buttons must not shrink the area used for game Fit or gravity: enabled={enabled}, alpha={alpha}, layout={layout}");
            assert_eq!(!keys.regions.is_empty(),enabled);
        }
    }
}

#[test]
fn hardware_sync_preserves_other_devices_and_touch_after_disconnect() {
    use zeebx::handset::touch::{TouchKeypad,KeyRegion};
    let mut map=InputMap::default();let mut keys=TouchKeypad::default();
    keys.regions.push(KeyRegion {key:Key::Num1,min:[0.0,0.0],max:[10.0,10.0]});keys.pointers([[5.0,5.0]]);
    map.key(1,96,0,true);map.key(2,66,0,true);map.apply(&mut keys);
    assert_eq!(keys.held(),mask(Key::Select)|mask(Key::Num1));
    map.release_device(1);map.apply(&mut keys);assert_eq!(keys.held(),mask(Key::Select)|mask(Key::Num1));
    map.release_device(2);map.apply(&mut keys);assert_eq!(keys.held(),mask(Key::Num1));
}

#[test]
fn keypad_transparency_applies_to_faces_and_pressed_overlay_without_background_or_touch_changes() {
    let ctx=egui::Context::default();let mut keys=zeebx::handset::touch::TouchKeypad::default();keys.hardware(Key::Select,true);
    let mut positions=None;
    for (amount,alpha) in [(0,0),(50,127),(100,255)] {
        let output=ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(360.0,800.0))),..Default::default()},|ctx|keypad_art::paint(&mut keys,ctx,amount));
        let meshes:Vec<_>=output.shapes.iter().filter_map(|s|if let egui::Shape::Mesh(mesh)=&s.shape{Some(mesh)}else{None}).collect();
        assert_eq!(meshes.len(),20,"19 button faces and the pressed OK overlay");
        assert!(meshes.iter().flat_map(|m|&m.vertices).all(|v|v.color.a()==alpha),"Transparency must apply uniformly to faces and overlays");
        assert!(!output.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Rect(rect) if rect.fill.a()>0)),"The keypad must not paint a panel background");
        let current:Vec<_>=keys.regions.iter().map(|r|(r.min,r.max)).collect();if let Some(previous)=&positions{assert_eq!(previous,&current);}else{positions=Some(current);}
    }
    let one=keys.regions.iter().find(|r|r.key==Key::Num1).unwrap();keys.pointers([[(one.min[0]+one.max[0])/2.0,(one.min[1]+one.max[1])/2.0]]);
    assert_eq!(keys.held(),mask(Key::Num1)|mask(Key::Select));keys.set_enabled(false);assert_eq!(keys.held(),mask(Key::Select));
    let output=ctx.run(egui::RawInput::default(),|ctx|keypad_art::paint(&mut keys,ctx,0));assert!(keys.regions.is_empty());assert!(output.shapes.iter().all(|s|!matches!(s.shape,egui::Shape::Mesh(_))));
}

#[test]
fn keypad_preserves_supplied_aspect_ratios_and_fits_small_and_landscape_screens() {
    for size in [egui::vec2(240.0,320.0),egui::vec2(800.0,360.0)] {
        for (layout,count) in [(serde_json::json!({}),19),(serde_json::json!({"_geometry":{"version":2,"preset":"dpad"}}),9),(serde_json::json!({"_geometry":{"version":2,"preset":"numpad"}}),15),(serde_json::json!({"digit_1":{}}),21)] {
            let ctx=egui::Context::default();let mut keys=zeebx::handset::touch::TouchKeypad::default();
            let _=ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,size)),..Default::default()},|ctx|keypad_art::paint_layout(&mut keys,ctx,0,&layout));
            assert_eq!(keys.regions.len(),count);
            for region in &keys.regions {
                assert!(region.min[0]>=0.0&&region.max[0]<=size.x+0.01&&region.min[1]>=size.y*0.5&&region.max[1]<=size.y+0.01);
                let ratio=(region.max[0]-region.min[0])/(region.max[1]-region.min[1]);assert!((ratio-708.0/405.0).abs()<0.001);
            }
        }
    }
}

#[test]
fn disconnect_notifications_are_deduplicated_and_consumed_once() {
    button_mapping::take_removed_devices();
    button_mapping::queue_removed_device(7);button_mapping::queue_removed_device(8);button_mapping::queue_removed_device(7);
    assert_eq!(button_mapping::take_removed_devices(),vec![7,8]);
    assert!(button_mapping::take_removed_devices().is_empty());
}

#[test]
fn missing_mapping_file_uses_defaults_and_load_is_not_repeated_during_input() {
    let path=std::env::temp_dir().join(format!("brew-mapping-{}.json",std::process::id()));
    let _=std::fs::remove_file(&path);
    let mut defaults=InputMap::load(&path);defaults.key(1,96,0,true);assert_eq!(defaults.held(),mask(Key::Select));
    std::fs::write(&path,r#"{"version":1,"bindings":{"1":"key:96"}}"#).unwrap();
    let mut custom=InputMap::load(&path);std::fs::remove_file(&path).unwrap();
    custom.key(1,96,0,true);assert_eq!(custom.held(),mask(Key::Num1));
}

#[test]
fn saved_layout_controls_painted_hit_regions_and_visibility_preserves_game_area() {
    let ctx=egui::Context::default();let mut keys=zeebx::handset::touch::TouchKeypad::default();
    let input=||egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(360.0,800.0))),..Default::default()};
    let layout=serde_json::json!({"digit_1":{"x":0.5,"y":0.2,"scale":1.5},"up_left":{"visible":false}});
    let mut area=egui::Rect::NOTHING;
    let out=ctx.run(input(),|ctx|{keypad_art::paint_layout(&mut keys,ctx,100,&layout);area=ctx.available_rect();});
    assert_eq!(keys.regions.len(),20);assert_eq!(keys.regions.iter().filter(|r|r.key==Key::Up).count(),1);
    let one=keys.regions.iter().find(|r|r.key==Key::Num1).unwrap();
    assert!(((one.min[0]+one.max[0])/2.0-180.0).abs()<0.01);
    assert!(((one.min[1]+one.max[1])/2.0-160.0).abs()<0.01);
    assert!(out.shapes.iter().any(|s|matches!(s.shape,egui::Shape::Mesh(_))));assert_eq!(area.height(),800.0);
    keys.pointers([[180.0,160.0]]);assert_eq!(keys.held(),mask(Key::Num1));
    keys.pointers([]);keys.hardware(Key::Select,true);
    let hidden=serde_json::Value::Object(keypad_layout::BUTTONS.iter().map(|(id,_,_)|((*id).to_owned(),serde_json::json!({"visible":false}))).collect());
    let out=ctx.run(input(),|ctx|{keypad_art::paint_layout(&mut keys,ctx,50,&hidden);area=ctx.available_rect();});
    assert!(keys.regions.is_empty());assert_eq!(area.height(),800.0);
    assert!(out.shapes.iter().all(|s|!matches!(s.shape,egui::Shape::Mesh(_))));
    assert_eq!(keys.held(),mask(Key::Select),"Hidden touch buttons must not disable hardware input");
}

#[test]
fn volume_mapping_requires_profile_opt_in_and_retains_dormant_bindings() {
    let text=r#"{"version":1,"volume_keys_mappable":true,"bindings":{"1":"key:24","clear":"key:25"}}"#;
    let mut map=InputMap::parse(text);
    assert!(map.key(1,24,0,true), "Opted-in volume up must reach the saved BREW target");
    assert_eq!(map.held(),mask(Key::Num1));
    map.key(1,24,0,true);map.key(1,24,0,false);assert_eq!(map.held(),0);
    assert!(map.key(1,25,0,true));assert_eq!(map.held(),mask(Key::Clear));
    map.set_volume_keys_mappable(false);assert_eq!(map.held(),0);assert!(!map.key(1,25,0,true));
    map.set_volume_keys_mappable(true);assert!(map.key(1,25,0,true));assert_eq!(map.held(),mask(Key::Clear));
    map.release_all();assert_eq!(map.held(),0);
    let mut dormant=InputMap::parse(r#"{"version":1,"bindings":{"1":"key:24"}}"#);
    assert!(!dormant.key(1,24,0,true));assert_eq!(dormant.held(),0);
}

#[test]
fn confirmed_physical_back_handles_oem_scans_without_accepting_virtual_back() {
    let mut map=InputMap::default();
    for scan in [0,1,158,217,999] {
        assert!(map.physical_back(1,scan,true));assert_eq!(map.held(),mask(Key::Soft2));
        map.physical_back(1,scan,true);map.physical_back(1,scan,false);assert_eq!(map.held(),0);
    }
    assert!(!map.physical_back(-1,0,true));assert!(!map.physical_back(1,-1,true));
    assert!(!map.key(1,4,0,true));
    let mut custom=InputMap::parse(r#"{"version":1,"bindings":{"clear":"key:4:scan:0"}}"#);
    custom.physical_back(1,0,true);assert_eq!(custom.held(),mask(Key::Clear));
    custom.release_device(1);assert_eq!(custom.held(),0);
}
