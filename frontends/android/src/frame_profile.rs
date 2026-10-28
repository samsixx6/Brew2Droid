//! Sonda opt-in da bancada: tempos de parede de etapas sequenciais, sem tocar em APIs guest.

use std::time::Duration;
use zeebx::{handset::fps::FrameStamp, session::Step};

pub struct FrameProfile {
    paced: bool,
    started: Option<Duration>,
    work: Duration,
    ui: Duration,
    draw: Duration,
    steps: u64,
    incomplete: u64,
    idle: u64,
    presents: u64,
    successful: u64,
    changed: u64,
    last_frame: Option<(FrameStamp, Duration)>,
    intervals: Vec<u64>,
    dropped_intervals: u64,
}

impl FrameProfile {
    pub fn new(paced: bool) -> Self {
        Self {
            paced, started: None, work: Duration::ZERO, ui: Duration::ZERO,
            draw: Duration::ZERO, steps: 0, incomplete: 0, idle: 0, presents: 0,
            successful: 0, changed: 0, last_frame: None, intervals: Vec::new(),
            dropped_intervals: 0,
        }
    }

    pub fn reset(&mut self) { *self = Self::new(self.paced); }

    pub fn work(&mut self, now: Duration, duration: Duration, step: Step) {
        self.started.get_or_insert(now.saturating_sub(duration));
        self.work += duration;
        self.steps += 1;
        self.incomplete += u64::from(step == Step::Running);
        self.idle += u64::from(step == Step::Ahead);
    }

    pub fn ui(&mut self, duration: Duration) { self.ui += duration; }

    pub fn present(&mut self, now: Duration, duration: Duration, success: bool, frame: Option<FrameStamp>) {
        self.draw += duration;
        self.presents += 1;
        if !success { return; }
        self.successful += 1;
        if let Some(frame) = frame {
            match self.last_frame {
                Some((last, at)) if frame != last => {
                    self.changed += 1;
                    if self.intervals.len() < 2048 {
                        self.intervals.push(now.saturating_sub(at).as_micros() as u64);
                    } else { self.dropped_intervals += 1; }
                    self.last_frame = Some((frame, now));
                }
                None => self.last_frame = Some((frame, now)),
                _ => {}
            }
        }
    }

    pub fn snapshot(&mut self, now: Duration) -> Option<String> {
        let elapsed = now.saturating_sub(self.started?);
        if elapsed < Duration::from_secs(5) { return None; }
        self.intervals.sort_unstable();
        let percentile = |percent: usize| {
            let index = (self.intervals.len() * percent).div_ceil(100).saturating_sub(1);
            self.intervals.get(index).map_or_else(|| "null".into(), u64::to_string)
        };
        let line = format!(
            concat!("{{\"kind\":\"android-handset-stage-profile\",\"schema_version\":1,",
                "\"paced\":{},\"wall_us\":{},\"guest_work_us\":{},\"ui_build_us\":{},",
                "\"draw_swap_wall_us\":{},\"work_steps\":{},\"incomplete_steps\":{},",
                "\"idle_steps\":{},\"present_calls\":{},\"successful_presentations\":{},",
                "\"new_frame_revisions\":{},\"frame_interval_p50_us\":{},",
                "\"frame_interval_p95_us\":{},\"dropped_interval_samples\":{},",
                "\"gpu_time_us\":null,\"thread_cpu_time_us\":null}}\n"),
            self.paced, elapsed.as_micros(), self.work.as_micros(), self.ui.as_micros(),
            self.draw.as_micros(), self.steps, self.incomplete, self.idle, self.presents,
            self.successful, self.changed, percentile(50), percentile(95), self.dropped_intervals,
        );
        self.started = Some(now);
        self.work = Duration::ZERO;
        self.ui = Duration::ZERO;
        self.draw = Duration::ZERO;
        self.steps = 0;
        self.incomplete = 0;
        self.idle = 0;
        self.presents = 0;
        self.successful = 0;
        self.changed = 0;
        self.intervals.clear();
        self.dropped_intervals = 0;
        Some(line)
    }
}
