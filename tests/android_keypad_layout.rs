#[path="../frontends/android/src/keypad_layout.rs"]
mod keypad_layout;
use keypad_layout::regions;
use serde_json::json;
use zeebx::handset::input::Key;

#[test]
fn default_layout_has_nineteen_buttons_in_the_new_five_rows() {
    let all=regions(360.0,800.0,&json!({}));assert_eq!(all.len(),21);
    let enter=all.iter().find(|r|r.id=="enter").unwrap();let one=all.iter().find(|r|r.id=="digit_1").unwrap();
    assert_eq!(enter.key,Key::Select);assert!((enter.max[0]-enter.min[0]-(one.max[0]-one.min[0])).abs()<0.001);
    assert!(((enter.min[0]+enter.max[0])/2.0-180.0).abs()<0.001);
    assert_eq!(all.iter().filter(|r|r.visible).count(),19);
    assert_eq!(all.iter().filter(|r|r.visible&&r.key==Key::Up).count(),1);assert_eq!(all.iter().filter(|r|r.visible&&r.key==Key::Down).count(),1);
    assert!(enter.max[1]<all.iter().find(|r|r.id=="left").unwrap().min[1]);
}

#[test]
fn a_saved_center_and_uniform_scale_move_only_the_selected_button() {
    let original=regions(360.0,800.0,&json!({"digit_5":{}}));let changed=regions(360.0,800.0,&json!({"digit_5":{"x":0.4,"y":0.2,"scale":1.5,"visible":true}}));
    assert_eq!(original.len(),21);assert_eq!(changed.len(),21);
    for (a,b) in original.iter().zip(&changed) {
        if a.id=="digit_5" {
            assert!(((b.min[0]+b.max[0])/2.0-144.0).abs()<0.001);assert!(((b.min[1]+b.max[1])/2.0-160.0).abs()<0.001);
            assert!((b.max[0]-b.min[0]-(a.max[0]-a.min[0])*1.5).abs()<0.001);
        }else{assert_eq!(a.min,b.min);assert_eq!(a.max,b.max);}
    }
}

#[test]
fn hiding_one_of_two_up_buttons_does_not_hide_the_other_source() {
    let all=regions(360.0,800.0,&json!({"up_left":{"visible":false}}));assert_eq!(all.iter().filter(|r|r.visible).count(),20);
    assert!(!all.iter().find(|r|r.id=="up_left").unwrap().visible);assert!(all.iter().find(|r|r.id=="up_right").unwrap().visible);
}

#[test]
fn scaling_near_edges_preserves_art_aspect_and_clamps_inside_rotated_viewports() {
    for (w,h) in [(240.0,320.0),(800.0,360.0),(360.0,800.0)] {
        let all=regions(w,h,&json!({"enter":{"x":0.0,"y":1.0,"scale":3.0}}));assert_eq!(all.len(),21);
        for r in all {assert!(r.min[0]>=0.0&&r.min[1]>=0.0&&r.max[0]<=w+0.001&&r.max[1]<=h+0.001);
            assert!(((r.max[0]-r.min[0])/(r.max[1]-r.min[1])-708.0/405.0).abs()<0.002);}
    }
}

#[test]
fn hidden_all_buttons_reclaims_the_keypad_panel_but_keeps_default_geometry_available_to_editor() {
    let all=regions(360.0,800.0,&json!({}));assert_eq!(all.len(),21);
    let hidden:serde_json::Map<_,_>=all.iter().map(|r|(r.id.to_owned(),json!({"visible":false}))).collect();
    assert_eq!(keypad_layout::reserved_height(360.0,800.0,&hidden.into()),0.0);
    assert!(keypad_layout::reserved_height(360.0,800.0,&json!({}))>100.0);
}

#[test]
fn invalid_viewport_never_produces_nan_touch_regions() {
    for (w,h) in [(0.0,800.0),(360.0,0.0),(f32::NAN,800.0),(360.0,f32::INFINITY)] {assert!(regions(w,h,&json!({})).is_empty());}
}

#[test]
fn versioned_presets_have_exact_rows_visibility_and_art_in_both_orientations() {
    let cases=[("default",vec![vec!["soft_left","enter","soft_right"],vec!["left","right","down_right","up_left"],vec!["pound","digit_1","digit_2","digit_3"],vec!["star","digit_4","digit_5","digit_6"],vec!["digit_0","digit_7","digit_8","digit_9"]]),
        ("dpad",vec![vec!["soft_left","up_left","soft_right"],vec!["left","enter","right"],vec!["star","down_right","pound"]]),
        ("numpad",vec![vec!["soft_left","enter","soft_right"],vec!["digit_1","digit_2","digit_3"],vec!["digit_4","digit_5","digit_6"],vec!["digit_7","digit_8","digit_9"],vec!["star","digit_0","pound"]])];
    for (preset,rows) in cases {for (w,h) in [(360.0,800.0),(800.0,360.0),(240.0,320.0)] {
        let all=regions(w,h,&json!({"_geometry":{"version":2,"preset":preset}}));
        assert_eq!(all.iter().filter(|r|r.visible).count(),rows.iter().map(|r|r.len()).sum::<usize>());
        let mut previous_y=-1.0;
        for row in &rows {let mut previous_x=-1.0;let mut row_y:Option<f32>=None;for id in row {
            let r=all.iter().find(|r|r.id==*id).unwrap();assert!(r.visible);
            let x=(r.min[0]+r.max[0])/2.0;let y=(r.min[1]+r.max[1])/2.0;
            assert!(x>previous_x&&y>previous_y);previous_x=x;
            if let Some(expected)=row_y {assert!((y-expected).abs()<0.001);}else{row_y=Some(y);}
            assert!(r.min[0]>=0.0&&r.min[1]>=0.0&&r.max[0]<=w+0.001&&r.max[1]<=h+0.001);
        }previous_y=row_y.unwrap();}
        assert_eq!(all.iter().find(|r|r.id=="soft_right").unwrap().art,"button-left-soft-key");
        if preset!="default" {let a=all.iter().find(|r|r.id=="soft_left").unwrap();let b=all.iter().find(|r|r.id==rows[0][1]).unwrap();assert!((a.max[0]-a.min[0])/((b.min[0]+b.max[0]-a.min[0]-a.max[0])/2.0)<0.8);}
    }}
}

#[test]
fn legacy_partial_layout_keeps_implicit_positions_scale_baseline_and_duplicate_ids() {
    let all=regions(360.0,800.0,&json!({"up_left":{"x":0.2,"y":0.4,"scale":1.6,"visible":false}}));
    assert_eq!(all.iter().filter(|r|r.visible).count(),20);
    let five=all.iter().find(|r|r.id=="digit_5").unwrap();assert!((five.max[0]-five.min[0]-65.10345).abs()<0.01);
    let enter=all.iter().find(|r|r.id=="enter").unwrap();let zero=all.iter().find(|r|r.id=="digit_0").unwrap();assert!(enter.min[1]>zero.max[1]);
    let up=all.iter().find(|r|r.id=="up_left").unwrap();assert!(((up.min[0]+up.max[0])/2.0-72.0).abs()<0.001);assert!(((up.min[1]+up.max[1])/2.0-320.0).abs()<0.001);
    assert!(all.iter().find(|r|r.id=="up_right").unwrap().visible);
}

