use std::time::{Duration, Instant};
use zeebx::handset::clock::RealtimeClock;

#[test]
fn refresh_frequency_does_not_change_game_time() {
    let start = Instant::now();
    let clock = RealtimeClock::new(start, 0);
    for hz in [30, 60, 120, 144] {
        for frame in 0..=hz {
            let now = start + Duration::from_secs_f64(frame as f64 / hz as f64);
            let _ = clock.now_us(now);
        }
        assert_eq!(clock.now_us(start + Duration::from_secs(1)), 1_000_000);
    }
}

#[test]
fn background_time_is_excluded_and_duplicate_pause_events_are_idempotent() {
    let start = Instant::now();
    let mut clock = RealtimeClock::new(start, 123_000);
    clock.set_paused(true, start + Duration::from_secs(1));
    clock.set_paused(true, start + Duration::from_secs(2));
    assert_eq!(clock.now_us(start + Duration::from_secs(20)), 1_123_000);
    clock.set_paused(false, start + Duration::from_secs(20));
    clock.set_paused(false, start + Duration::from_secs(21));
    assert_eq!(clock.now_us(start + Duration::from_secs(22)), 3_123_000);
}

#[test]
fn uptime_keeps_precision_past_brew_millisecond_wrap() {
    let start = Instant::now();
    let base = u64::from(u32::MAX) * 1000;
    let clock = RealtimeClock::new(start, base);
    assert_eq!(
        clock.now_us(start + Duration::from_millis(10)),
        base + 10_000
    );
    assert_eq!(
        (clock.now_us(start + Duration::from_millis(10)) / 1000) as u32,
        9
    );
}

#[test]
fn rebase_preserves_a_paused_frontend_and_resume_advances_from_the_restored_base() {
    let start = Instant::now();
    let mut clock = RealtimeClock::new(start, 123);
    clock.set_paused(true, start + Duration::from_secs(1));
    clock.rebase(start + Duration::from_secs(20), 63_000_123);
    assert_eq!(clock.now_us(start + Duration::from_secs(40)), 63_000_123);
    clock.set_paused(false, start + Duration::from_secs(40));
    assert_eq!(clock.now_us(start + Duration::from_secs(42)), 65_000_123);
}

#[test]
fn rebase_preserves_a_running_frontend_and_discards_the_old_wall_time_anchor() {
    let start = Instant::now();
    let mut clock = RealtimeClock::new(start, 123);
    clock.rebase(start + Duration::from_secs(20), 63_000_123);
    assert_eq!(clock.now_us(start + Duration::from_secs(22)), 65_000_123);
}
