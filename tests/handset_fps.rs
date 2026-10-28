use std::time::Duration;

use zeebx::handset::fps::{FpsCounter, FrameStamp};
use zeebx::ui::settings::DebugView;

#[path = "../frontends/android/src/fps.rs"]
mod android_fps;

fn frame(writes: u64, gpu_frames: u32) -> FrameStamp {
    FrameStamp {
        cpu_series: 1,
        cpu_writes: writes,
        gpu_frames,
    }
}

#[test]
fn screen_repaints_do_not_inflate_game_fps() {
    let mut counter = FpsCounter::default();
    for tick in 0..=120 {
        counter.presented(
            Duration::from_millis(tick * 1000 / 120),
            Some(frame(tick / 8, 0)),
        );
    }
    assert_eq!(counter.value(), Some(15));
    for tick in 121..=240 {
        counter.presented(Duration::from_millis(tick * 1000 / 120), Some(frame(15, 0)));
    }
    assert_eq!(counter.value(), Some(0));
}

#[test]
fn gpu_and_cpu_revisions_count_once_per_successful_presentation() {
    let mut counter = FpsCounter::default();
    counter.presented(Duration::ZERO, Some(frame(1, 0)));
    counter.presented(Duration::from_millis(100), Some(frame(1, 7)));
    counter.presented(Duration::from_millis(200), Some(frame(50, 7)));
    // Nenhum quadro entregue: swap falhou, jogo pausado ou outra tela da interface.
    counter.presented(Duration::from_millis(300), None);
    counter.presented(Duration::from_secs(1), Some(frame(50, 7)));
    assert_eq!(counter.value(), Some(2));
    assert_eq!(counter.label(), "2 FPS");
}

#[test]
fn label_refresh_uses_elapsed_host_time_at_most_once_per_second() {
    let mut counter = FpsCounter::default();
    counter.presented(Duration::ZERO, Some(frame(1, 0)));
    counter.presented(Duration::from_millis(500), Some(frame(2, 0)));
    counter.presented(Duration::from_millis(999), Some(frame(3, 0)));
    assert_eq!(counter.value(), None);
    assert_eq!(counter.label(), "FPS --");
    counter.presented(Duration::from_secs(2), None);
    assert_eq!(counter.value(), Some(1));
    let text = counter.label().as_ptr();
    counter.presented(Duration::from_millis(2100), Some(frame(4, 0)));
    assert_eq!(counter.label(), "1 FPS");
    assert_eq!(counter.label().as_ptr(), text);
}

#[test]
fn restart_or_resume_does_not_include_an_old_window_or_background_time() {
    let mut counter = FpsCounter::default();
    counter.presented(Duration::ZERO, Some(frame(1, 0)));
    counter.presented(Duration::from_secs(1), Some(frame(2, 0)));
    counter.reset();
    assert_eq!(counter.value(), None);
    assert_eq!(counter.label(), "FPS --");
    counter.presented(Duration::from_secs(100), Some(frame(2, 0)));
    counter.presented(Duration::from_secs(101), None);
    assert_eq!(counter.value(), Some(0));
}

#[test]
fn light_fps_setting_is_independent_and_old_settings_still_load() {
    let old: DebugView = serde_json::from_str(r#"{"overlay":false,"speed":true}"#).unwrap();
    assert!(!old.fps_counter);
    let mut light = old;
    light.fps_counter = true;
    let encoded = serde_json::to_string(&light).unwrap();
    let loaded: DebugView = serde_json::from_str(&encoded).unwrap();
    assert!(loaded.fps_counter);
    assert!(!loaded.overlay);
    assert!(!loaded.log);
}
