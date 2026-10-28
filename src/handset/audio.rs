//! Transporte PCM entre um produtor e o callback nativo: capacidade fixa, sem locks ou alocação.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
};

struct Frame {
    samples: AtomicU64,
    generation: AtomicU64,
}

#[repr(align(64))]
struct Cursor(AtomicUsize);

struct Shared {
    frames: Box<[Frame]>,
    read: Cursor,
    write: Cursor,
    generation: AtomicU64,
    paused: AtomicBool,
    disconnected: AtomicBool,
    requested: AtomicU64,
    consumed: AtomicU64,
    underrun_frames: AtomicU64,
    underrun_callbacks: AtomicU64,
    discarded: AtomicU64,
    nonzero: AtomicU64,
    callbacks: AtomicU64,
    max_callback_frames: AtomicUsize,
}

impl Shared {
    fn distance(&self, from: usize, to: usize) -> usize {
        (to + self.frames.len() * 2 - from) % (self.frames.len() * 2)
    }
    fn advance(&self, cursor: usize, frames: usize) -> usize {
        (cursor + frames) % (self.frames.len() * 2)
    }
}

/// As pontas não são Clone: somente o worker escreve e somente o callback consome.
pub struct PcmProducer {
    shared: Arc<Shared>,
}
pub struct PcmConsumer {
    shared: Arc<Shared>,
}

#[derive(Clone)]
pub struct AudioMonitor {
    shared: Arc<Shared>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AudioSnapshot {
    pub capacity_frames: usize,
    pub queued_frames: usize,
    pub requested_frames: u64,
    pub consumed_frames: u64,
    pub underrun_frames: u64,
    pub underrun_callbacks: u64,
    pub discarded_frames: u64,
    pub nonzero_samples: u64,
    pub callback_count: u64,
    pub max_callback_frames: usize,
    pub paused: bool,
    pub disconnected: bool,
}

pub fn pcm_queue(capacity_frames: usize) -> (PcmProducer, PcmConsumer, AudioMonitor) {
    assert!(
        (1..=16_384).contains(&capacity_frames),
        "invalid PCM capacity"
    );
    let shared = Arc::new(Shared {
        frames: (0..capacity_frames)
            .map(|_| Frame {
                samples: AtomicU64::new(0),
                generation: AtomicU64::new(0),
            })
            .collect(),
        read: Cursor(AtomicUsize::new(0)),
        write: Cursor(AtomicUsize::new(0)),
        generation: AtomicU64::new(1),
        paused: AtomicBool::new(false),
        disconnected: AtomicBool::new(false),
        requested: AtomicU64::new(0),
        consumed: AtomicU64::new(0),
        underrun_frames: AtomicU64::new(0),
        underrun_callbacks: AtomicU64::new(0),
        discarded: AtomicU64::new(0),
        nonzero: AtomicU64::new(0),
        callbacks: AtomicU64::new(0),
        max_callback_frames: AtomicUsize::new(0),
    });
    (
        PcmProducer {
            shared: shared.clone(),
        },
        PcmConsumer {
            shared: shared.clone(),
        },
        AudioMonitor { shared },
    )
}

impl PcmProducer {
    pub fn free_frames(&self) -> usize {
        let shared = &self.shared;
        let write = shared.write.0.load(Ordering::Relaxed);
        let read = shared.read.0.load(Ordering::Acquire);
        shared.frames.len() - shared.distance(read, write)
    }

    /// O epoch é capturado ANTES de misturar: uma pausa durante o trabalho invalida o bloco.
    pub fn push(&mut self, stereo: &[f32], generation: u64) -> usize {
        let shared = &self.shared;
        let write = shared.write.0.load(Ordering::Relaxed);
        let count = (stereo.len() / 2).min(self.free_frames());
        for (index, pair) in stereo.chunks_exact(2).take(count).enumerate() {
            let frame = &shared.frames[(write + index) % shared.frames.len()];
            let bits = u64::from(pair[0].to_bits()) | (u64::from(pair[1].to_bits()) << 32);
            frame.samples.store(bits, Ordering::Relaxed);
            frame.generation.store(generation, Ordering::Relaxed);
        }
        shared
            .write
            .0
            .store(shared.advance(write, count), Ordering::Release);
        count
    }
}

impl PcmConsumer {
    /// O callback não possui mixer, memória guest ou payload que precise destruir.
    pub fn fill(&mut self, output: &mut [f32], channels: usize) {
        output.fill(0.0);
        if channels == 0 {
            return;
        }
        let requested = output.len() / channels;
        self.consume(requested, |index, pair| {
            let mut nonzero = 0;
            for (channel, sample) in output[index * channels..(index + 1) * channels]
                .iter_mut()
                .enumerate()
            {
                *sample = pair[channel.min(1)];
                nonzero += u64::from(*sample != 0.0);
            }
            nonzero
        });
    }

    /// Oboe representa estéreo como tuplas, sem prometer layout C entre os dois floats.
    pub fn fill_stereo(&mut self, output: &mut [(f32, f32)]) {
        output.fill((0.0, 0.0));
        self.consume(output.len(), |index, pair| {
            output[index] = (pair[0], pair[1]);
            u64::from(pair[0] != 0.0) + u64::from(pair[1] != 0.0)
        });
    }

    fn consume(&mut self, requested: usize, mut write_frame: impl FnMut(usize, [f32; 2]) -> u64) {
        let shared = &self.shared;
        let mut read = shared.read.0.load(Ordering::Relaxed);
        let write = shared.write.0.load(Ordering::Acquire);
        let available = shared.distance(read, write);
        shared
            .requested
            .fetch_add(requested as u64, Ordering::Relaxed);
        shared.callbacks.fetch_add(1, Ordering::Relaxed);
        shared
            .max_callback_frames
            .fetch_max(requested, Ordering::Relaxed);
        if shared.paused.load(Ordering::Acquire) || shared.disconnected.load(Ordering::Acquire) {
            shared.read.0.store(write, Ordering::Release);
            shared
                .discarded
                .fetch_add(available as u64, Ordering::Relaxed);
            return;
        }
        let generation = shared.generation.load(Ordering::Acquire);
        let mut copied = 0;
        let mut discarded = 0;
        let mut nonzero = 0;
        for _ in 0..available {
            if copied == requested {
                break;
            }
            let frame = &shared.frames[read % shared.frames.len()];
            if frame.generation.load(Ordering::Relaxed) == generation {
                let bits = frame.samples.load(Ordering::Relaxed);
                let pair = [
                    f32::from_bits(bits as u32),
                    f32::from_bits((bits >> 32) as u32),
                ];
                nonzero += write_frame(copied, pair);
                copied += 1;
            } else {
                discarded += 1;
            }
            read = shared.advance(read, 1);
        }
        shared.read.0.store(read, Ordering::Release);
        shared.consumed.fetch_add(copied as u64, Ordering::Relaxed);
        shared.discarded.fetch_add(discarded, Ordering::Relaxed);
        shared.nonzero.fetch_add(nonzero, Ordering::Relaxed);
        let missing = requested - copied;
        if missing != 0 {
            shared
                .underrun_frames
                .fetch_add(missing as u64, Ordering::Relaxed);
            shared.underrun_callbacks.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl AudioMonitor {
    pub fn generation(&self) -> u64 {
        self.shared.generation.load(Ordering::Acquire)
    }
    pub fn set_paused(&self, paused: bool) {
        if self.shared.paused.load(Ordering::Acquire) != paused {
            if paused {
                self.shared.paused.store(true, Ordering::Release);
            }
            self.shared.generation.fetch_add(1, Ordering::AcqRel);
            if !paused {
                self.shared.paused.store(false, Ordering::Release);
            }
        }
    }
    pub fn disconnect(&self) {
        self.shared.disconnected.store(true, Ordering::Release);
        self.shared.generation.fetch_add(1, Ordering::AcqRel);
    }
    pub fn is_suspended(&self) -> bool {
        self.shared.paused.load(Ordering::Acquire)
            || self.shared.disconnected.load(Ordering::Acquire)
    }
    pub fn snapshot(&self) -> AudioSnapshot {
        let shared = &self.shared;
        let read = shared.read.0.load(Ordering::Acquire);
        let write = shared.write.0.load(Ordering::Acquire);
        AudioSnapshot {
            capacity_frames: shared.frames.len(),
            queued_frames: shared.distance(read, write).min(shared.frames.len()),
            requested_frames: shared.requested.load(Ordering::Relaxed),
            consumed_frames: shared.consumed.load(Ordering::Relaxed),
            underrun_frames: shared.underrun_frames.load(Ordering::Relaxed),
            underrun_callbacks: shared.underrun_callbacks.load(Ordering::Relaxed),
            discarded_frames: shared.discarded.load(Ordering::Relaxed),
            nonzero_samples: shared.nonzero.load(Ordering::Relaxed),
            callback_count: shared.callbacks.load(Ordering::Relaxed),
            max_callback_frames: shared.max_callback_frames.load(Ordering::Relaxed),
            paused: shared.paused.load(Ordering::Acquire),
            disconnected: shared.disconnected.load(Ordering::Acquire),
        }
    }
}
