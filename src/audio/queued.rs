//! O mixer pode tomar locks e liberar vozes; tudo isso fica fora do callback do aparelho.

use super::Mixer;
use crate::handset::audio::{AudioMonitor, AudioSnapshot, PcmConsumer, PcmProducer, pcm_queue};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub struct AudioPump {
    mixer: Mixer,
    monitor: AudioMonitor,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl AudioPump {
    pub fn start(mixer: Mixer, capacity: usize) -> Result<(Self, PcmConsumer), String> {
        let (producer, consumer, monitor) = pcm_queue(capacity);
        Ok((Self::from_queue(mixer, producer, monitor)?, consumer))
    }

    /// O fluxo nativo pode escolher sua taxa antes de o mixer começar a produzir.
    pub fn from_queue(
        mixer: Mixer,
        mut producer: PcmProducer,
        monitor: AudioMonitor,
    ) -> Result<Self, String> {
        let capacity = monitor.snapshot().capacity_frames;
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker_monitor = monitor.clone();
        let worker_mixer = mixer.clone();
        let worker = std::thread::Builder::new()
            .name("brew-pcm".into())
            .spawn(move || {
                let mut samples = [0.0f32; 512];
                while !worker_stop.load(Ordering::Acquire) {
                    if worker_monitor.is_suspended() {
                        std::thread::park_timeout(std::time::Duration::from_millis(2));
                        continue;
                    }
                    let stats = worker_monitor.snapshot();
                    // O tamanho observado é do callback, não uma alegação de burst do hardware.
                    // Underruns aumentam o alvo até a capacidade fixa; nenhum crescimento no callback.
                    let multiplier = 2 + stats.underrun_callbacks.min(4) as usize;
                    let target = (stats.max_callback_frames.max(256) * multiplier).min(capacity);
                    let free = producer.free_frames();
                    let queued = capacity - free;
                    let frames = target
                        .saturating_sub(queued)
                        .min(free)
                        .min(samples.len() / 2);
                    if frames == 0 {
                        std::thread::park_timeout(std::time::Duration::from_millis(2));
                        continue;
                    }
                    let generation = worker_monitor.generation();
                    worker_mixer.fill(&mut samples[..frames * 2], 2);
                    producer.push(&samples[..frames * 2], generation);
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            mixer,
            monitor,
            stop,
            worker: Some(worker),
        })
    }

    pub fn monitor(&self) -> AudioMonitor {
        self.monitor.clone()
    }
    pub fn snapshot(&self) -> AudioSnapshot {
        self.monitor.snapshot()
    }

    pub fn set_paused(&self, paused: bool) {
        if paused {
            self.monitor.set_paused(true);
            self.mixer.congela(true);
        } else {
            self.mixer.congela(false);
            self.monitor.set_paused(false);
        }
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
    }
}

impl Drop for AudioPump {
    fn drop(&mut self) {
        self.monitor.disconnect();
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            let _ = worker.join();
        }
    }
}
