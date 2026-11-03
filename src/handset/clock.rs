//! Tempo monotônico de handset, sem supor a frequência de CPU do console.

use std::time::Instant;

pub struct RealtimeClock {
    anchor: Instant,
    base_us: u64,
    paused: bool,
}

impl RealtimeClock {
    pub fn new(now: Instant, base_us: u64) -> Self {
        Self {
            anchor: now,
            base_us,
            paused: false,
        }
    }

    pub fn now_us(&self, now: Instant) -> u64 {
        self.base_us.saturating_add(if self.paused {
            0
        } else {
            now.saturating_duration_since(self.anchor)
                .as_micros()
                .min(u128::from(u64::MAX)) as u64
        })
    }

    /// Reancora o tempo do guest sem alterar a pausa escolhida pelo frontend.
    pub fn rebase(&mut self, now: Instant, base_us: u64) {
        self.anchor = now;
        self.base_us = base_us;
    }

    pub fn set_paused(&mut self, paused: bool, now: Instant) {
        if self.paused != paused {
            self.base_us = self.now_us(now);
            self.anchor = now;
            self.paused = paused;
        }
    }
}
