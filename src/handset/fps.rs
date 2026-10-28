//! Taxa dos quadros novos entregues à janela, sem ler pixels nem tocar no relógio guest.

use std::fmt::Write;
use std::time::Duration;

/// Revisões já disponíveis na apresentação; repintar a mesma revisão não cria outro quadro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameStamp {
    pub cpu_series: u64,
    pub cpu_writes: u64,
    pub gpu_frames: u32,
}

#[derive(Default)]
pub struct FpsCounter {
    window: Option<Duration>,
    last_frame: Option<FrameStamp>,
    frames: u32,
    value: Option<u32>,
    label: String,
}

impl FpsCounter {
    /// `now` vem de um relógio monotônico do host; `None` não contou apresentação de jogo.
    /// O primeiro quadro só estabelece a referência, evitando inflar a primeira janela.
    pub fn presented(&mut self, now: Duration, frame: Option<FrameStamp>) {
        let Some(start) = self.window else {
            self.window = Some(now);
            self.last_frame = frame;
            return;
        };
        if let Some(frame) = frame
            && self.last_frame != Some(frame)
        {
            self.last_frame = Some(frame);
            self.frames = self.frames.saturating_add(1);
        }
        let elapsed = now.saturating_sub(start);
        if elapsed < Duration::from_secs(1) {
            return;
        }
        let fps = (f64::from(self.frames) / elapsed.as_secs_f64()).round() as u32;
        if self.value != Some(fps) {
            self.label.clear();
            // Uma formatação por janela, e só se o valor mudou. O buffer fica para a próxima.
            write!(&mut self.label, "{fps} FPS").expect("format into String");
            self.value = Some(fps);
        }
        self.frames = 0;
        self.window = Some(now);
    }

    pub fn value(&self) -> Option<u32> {
        self.value
    }

    pub fn label(&self) -> &str {
        if self.label.is_empty() {
            "FPS --"
        } else {
            &self.label
        }
    }

    /// Abrir outra sessão, alternar a opção ou voltar do background começa uma janela nova.
    pub fn reset(&mut self) {
        self.window = None;
        self.last_frame = None;
        self.frames = 0;
        self.value = None;
        self.label.clear();
    }
}
