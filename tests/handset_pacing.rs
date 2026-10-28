use std::time::Duration;
use zeebx::handset::fps::FrameStamp;

#[path = "../frontends/android/src/pacing.rs"]
mod pacing;
use pacing::HandsetPacer;

fn ms(value: u64) -> Duration { Duration::from_millis(value) }
fn frame(value: u64) -> FrameStamp {
    FrameStamp { cpu_series: value, cpu_writes: 1, gpu_frames: 0 }
}

#[test]
fn unfinished_cpu_slices_do_not_require_a_surface_swap() {
    let mut pacer = HandsetPacer::default();
    assert!(pacer.needs_present(ms(0), frame(1), false));
    pacer.presented(ms(0), frame(1), true);
    for now in [1, 8, 16, 24, 32, 40, 49] {
        assert!(!pacer.needs_present(ms(now), frame(1), false));
    }
    assert!(pacer.needs_present(ms(50), frame(1), false));
}

#[test]
fn new_complete_frame_and_touch_feedback_are_presented_immediately() {
    let mut pacer = HandsetPacer::default();
    pacer.presented(ms(0), frame(1), true);
    assert!(pacer.needs_present(ms(1), frame(2), false));
    assert!(pacer.needs_present(ms(1), frame(1), true));
}

#[test]
fn failed_swap_and_lifecycle_reset_keep_the_next_draw_pending() {
    let mut pacer = HandsetPacer::default();
    pacer.presented(ms(0), frame(1), true);
    pacer.presented(ms(10), frame(2), false);
    assert!(pacer.needs_present(ms(11), frame(2), false));
    pacer.presented(ms(12), frame(2), true);
    assert!(!pacer.needs_present(ms(13), frame(2), false));
    pacer.reset();
    assert!(pacer.needs_present(ms(13), frame(2), false));
}

#[test]
fn idle_poll_wakes_for_the_earliest_timer_or_ui_deadline() {
    let mut pacer = HandsetPacer::default();
    pacer.presented(ms(0), frame(1), true);
    assert_eq!(pacer.idle_wait(ms(10), Some(ms(15))), ms(15));
    assert_eq!(pacer.idle_wait(ms(10), Some(ms(100))), ms(40));
    assert_eq!(pacer.idle_wait(ms(49), None), ms(1));
    assert_eq!(pacer.idle_wait(ms(51), None), Duration::ZERO);
    assert_eq!(pacer.idle_wait(ms(10), Some(Duration::ZERO)), Duration::ZERO);
    // android-activity trunca o timeout para milissegundos: resto positivo não pode girar.
    assert_eq!(pacer.idle_wait(Duration::from_micros(49_500), None), ms(1));
}
