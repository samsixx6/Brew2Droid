//! Saída Oboe: taxa negociada pelo aparelho e callback que só copia PCM.

use super::{Mixer, queued::AudioPump};
use crate::handset::audio::{AudioMonitor, PcmConsumer, pcm_queue};
use oboe::{
    AudioOutputCallback, AudioOutputStream, AudioOutputStreamSafe, AudioStream, AudioStreamAsync,
    AudioStreamBase, AudioStreamBuilder, AudioStreamSafe, DataCallbackResult, PerformanceMode,
    SharingMode, Stereo, Usage,
};
use std::time::{Duration, Instant};

struct Callback {
    consumer: PcmConsumer,
    monitor: AudioMonitor,
}
impl AudioOutputCallback for Callback {
    type FrameType = (f32, Stereo);
    fn on_audio_ready(
        &mut self,
        _: &mut dyn AudioOutputStreamSafe,
        output: &mut [(f32, f32)],
    ) -> DataCallbackResult {
        self.consumer.fill_stereo(output);
        DataCallbackResult::Continue
    }
    fn on_error_after_close(&mut self, _: &mut dyn AudioOutputStreamSafe, _: oboe::Error) {
        self.monitor.disconnect();
    }
}

pub struct Output {
    // O fluxo fecha antes de o worker destruir sua ponta da fila.
    stream: AudioStreamAsync<oboe::Output, Callback>,
    pump: AudioPump,
    mixer: Mixer,
    rate: u32,
    burst: i32,
    native_xruns: i32,
    last_service: Instant,
    paused: bool,
}

impl Output {
    pub fn open(volume: f32, muted: bool) -> Result<Self, String> {
        let _ = oboe::DefaultStreamValues::init();
        Self::open_with_mixer(volume, muted, None, false)
    }

    fn open_with_mixer(
        volume: f32,
        muted: bool,
        previous: Option<Mixer>,
        paused: bool,
    ) -> Result<Self, String> {
        let (producer, consumer, monitor) = pcm_queue(4096);
        let mut stream = AudioStreamBuilder::default()
            .set_output()
            .set_stereo()
            .set_format::<f32>()
            .set_performance_mode(PerformanceMode::LowLatency)
            .set_sharing_mode(SharingMode::Shared)
            .set_usage(Usage::Game)
            .set_callback(Callback {
                consumer,
                monitor: monitor.clone(),
            })
            .open_stream()
            .map_err(|error| error.to_string())?;
        let rate = u32::try_from(stream.get_sample_rate()).map_err(|_| "taxa nativa inválida")?;
        if rate == 0 {
            return Err("taxa nativa zero".into());
        }
        let burst = stream.get_frames_per_burst().max(1);
        let _ = stream.set_buffer_size_in_frames(burst.saturating_mul(2));
        let mixer = previous.unwrap_or_else(|| Mixer::new(rate, volume, muted));
        mixer.define_taxa(rate);
        let pump = AudioPump::from_queue(mixer.clone(), producer, monitor)?;
        pump.set_paused(paused);
        // Só a abertura espera: o callback nunca espera o produtor.
        if !paused {
            let deadline = Instant::now() + Duration::from_millis(20);
            while pump.snapshot().queued_frames < 512 && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        if !paused {
            stream.request_start().map_err(|error| error.to_string())?;
        }
        Ok(Self {
            stream,
            pump,
            mixer,
            rate,
            burst,
            native_xruns: 0,
            last_service: Instant::now(),
            paused,
        })
    }

    pub fn mixer(&self) -> Mixer {
        self.mixer.clone()
    }
    pub fn set_paused(&mut self, paused: bool) {
        if self.paused == paused {
            return;
        }
        self.paused = paused;
        self.pump.set_paused(paused);
        let _ = if paused {
            self.stream.request_pause()
        } else {
            self.stream.request_start()
        };
    }

    /// Reabertura e ajuste pertencem ao loop do emulador, nunca ao callback de erro.
    pub fn service(&mut self) {
        if self.last_service.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.last_service = Instant::now();
        let stats = self.pump.snapshot();
        self.paused = stats.paused;
        if stats.disconnected {
            match Self::open_with_mixer(1.0, false, Some(self.mixer.clone()), self.paused) {
                Ok(replacement) => {
                    *self = replacement;
                }
                Err(error) => crate::registro!(
                    crate::registro::Nivel::Aviso,
                    "audio",
                    "reabertura: {error}"
                ),
            }
            return;
        }
        if let Ok(xruns) = self.stream.get_xrun_count() {
            if xruns > self.native_xruns {
                let requested = self
                    .stream
                    .get_buffer_size_in_frames()
                    .saturating_add(self.burst);
                let _ = self
                    .stream
                    .set_buffer_size_in_frames(requested.min(self.burst * 6));
            }
            self.native_xruns = xruns;
        }
    }

    pub fn snapshot(&mut self) -> serde_json::Value {
        serde_json::json!({"sample_rate": self.rate, "channels": 2, "frames_per_burst": self.burst,
            "buffer_frames": self.stream.get_buffer_size_in_frames(),
            "buffer_capacity_frames": self.stream.get_buffer_capacity_in_frames(),
            "performance_mode": format!("{:?}", self.stream.get_performance_mode()),
            "audio_api": format!("{:?}", self.stream.get_audio_api()),
            "native_xruns": self.stream.get_xrun_count().ok(), "queue": self.pump.snapshot()})
    }
}
