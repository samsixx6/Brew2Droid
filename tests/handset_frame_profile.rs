use std::time::Duration;
use zeebx::{handset::fps::FrameStamp, session::Step};
#[path = "../frontends/android/src/frame_profile.rs"]
mod frame_profile;
use frame_profile::FrameProfile;

fn frame(value: u64) -> FrameStamp {
    FrameStamp { cpu_series: value, cpu_writes: 1, gpu_frames: 0 }
}

#[test]
fn stage_report_separates_work_from_swap_wall_time_and_never_invents_gpu_time() {
    let mut profile = FrameProfile::new(true);
    profile.work(Duration::from_millis(2), Duration::from_millis(2), Step::Running);
    profile.ui(Duration::from_millis(3));
    profile.present(Duration::from_millis(10), Duration::from_millis(5), true, Some(frame(1)));
    profile.present(Duration::from_millis(100), Duration::from_millis(1), false, Some(frame(2)));
    profile.present(Duration::from_millis(200), Duration::from_millis(1), true, Some(frame(2)));
    profile.present(Duration::from_millis(300), Duration::from_millis(1), true, Some(frame(2)));
    assert!(profile.snapshot(Duration::from_secs(4)).is_none());
    let value: serde_json::Value = serde_json::from_str(&profile.snapshot(Duration::from_secs(5)).unwrap()).unwrap();
    assert_eq!(value["guest_work_us"], 2000);
    assert_eq!(value["ui_build_us"], 3000);
    assert_eq!(value["draw_swap_wall_us"], 8000);
    assert_eq!(value["incomplete_steps"], 1);
    assert_eq!(value["successful_presentations"], 3);
    assert_eq!(value["new_frame_revisions"], 1);
    assert_eq!(value["frame_interval_p95_us"], 190000);
    assert!(value["gpu_time_us"].is_null());
    assert!(value["thread_cpu_time_us"].is_null());
}

#[test]
fn windows_keep_frame_identity_and_pause_reset_excludes_the_background_gap() {
    let mut profile = FrameProfile::new(false);
    profile.work(Duration::ZERO, Duration::ZERO, Step::Ahead);
    profile.present(Duration::ZERO, Duration::ZERO, true, Some(frame(1)));
    profile.snapshot(Duration::from_secs(5)).unwrap();
    profile.present(Duration::from_secs(6), Duration::ZERO, true, Some(frame(1)));
    let value: serde_json::Value = serde_json::from_str(&profile.snapshot(Duration::from_secs(10)).unwrap()).unwrap();
    assert_eq!(value["new_frame_revisions"], 0);
    assert!(value["frame_interval_p95_us"].is_null());
    profile.reset();
    assert!(profile.snapshot(Duration::from_secs(100)).is_none());
    profile.work(Duration::from_secs(100), Duration::ZERO, Step::Ahead);
    let value: serde_json::Value = serde_json::from_str(&profile.snapshot(Duration::from_secs(105)).unwrap()).unwrap();
    assert_eq!(value["wall_us"], 5_000_000);
}
