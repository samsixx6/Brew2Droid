//! O trabalho guest pode continuar sem reconstruir a interface e esperar outro swap EGL.

use std::time::Duration;
use zeebx::handset::fps::FrameStamp;

/// A entrada nativa acorda o poll imediatamente; o teto abaixo só atualiza a UI ociosa/FPS.
const UI_REFRESH: Duration = Duration::from_millis(50);

#[derive(Default)]
pub struct HandsetPacer {
    last_present: Option<(Duration, FrameStamp)>,
}

impl HandsetPacer {
    pub fn needs_present(&self, now: Duration, frame: FrameStamp, input_changed: bool) -> bool {
        self.last_present.is_none_or(|(last, previous)| {
            previous != frame || input_changed || now.saturating_sub(last) >= UI_REFRESH
        })
    }

    pub fn presented(&mut self, now: Duration, frame: FrameStamp, success: bool) {
        if success { self.last_present = Some((now, frame)); }
    }

    pub fn idle_wait(&self, now: Duration, next_work: Option<Duration>) -> Duration {
        let until_ui = self.last_present.map_or(Duration::ZERO, |(last, _)| {
            UI_REFRESH.saturating_sub(now.saturating_sub(last))
        });
        let wait = next_work.map_or(until_ui, |work| work.min(until_ui));
        // android-activity passa as_millis() ao ALooper. Arredondar o resto positivo evita
        // milhares de polls vazios; input continua acordando o looper antes desse prazo.
        Duration::from_millis(wait.as_nanos().div_ceil(1_000_000) as u64)
    }

    pub fn reset(&mut self) { self.last_present = None; }
}
