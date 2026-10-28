//! Cargas codificadas são snapshots próprios; este worker nunca recebe memória ou callbacks guest.

use super::wav::Sound;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};

#[derive(Default)]
pub struct DecodeSettings {
    #[cfg(feature = "soundfont")]
    pub bank: Option<Arc<super::soundfont::Banco>>,
}

pub struct Decoded {
    pub key: u64,
    pub sound: Result<Sound, String>,
    pub silent_us: Option<u64>,
    pub midi: bool,
    pub soundfont: bool,
    pub encoded_bytes: usize,
    pub elapsed_ms: f64,
}
struct Request {
    key: u64,
    bytes: Vec<u8>,
}

pub struct DecodeWorker {
    requests: mpsc::SyncSender<Request>,
    results: mpsc::Receiver<Decoded>,
    reserved: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    backlog: std::collections::VecDeque<Request>,
}

impl DecodeWorker {
    pub fn new(settings: DecodeSettings) -> Result<Self, String> {
        let (requests, incoming) = mpsc::sync_channel::<Request>(16);
        let (outgoing, results) = mpsc::sync_channel(2);
        let reserved = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker_reserved = reserved.clone();
        let thread = std::thread::Builder::new()
            .name("brew-decode".into())
            .spawn(move || {
                while !worker_stop.load(Ordering::Acquire) {
                    let request = match incoming.recv_timeout(std::time::Duration::from_millis(10))
                    {
                        Ok(request) => request,
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(_) => break,
                    };
                    let now = std::time::Instant::now();
                    let midi = request.bytes.starts_with(b"MThd")
                        || super::cmf::contains_music(&request.bytes);
                    let (sound, soundfont) = match decode(&request.bytes, &settings) {
                        Ok((sound, bank)) => (Ok(sound), bank),
                        Err(error) => (Err(error), false),
                    };
                    let silent_us = sound
                        .as_ref()
                        .err()
                        .and_then(|_| super::mp3::probe(&request.bytes))
                        .map(|p| p.duration_us());
                    let mut result = Decoded {
                        key: request.key,
                        sound,
                        silent_us,
                        midi,
                        soundfont,
                        encoded_bytes: request.bytes.len(),
                        elapsed_ms: now.elapsed().as_secs_f64() * 1000.0,
                    };
                    worker_reserved.fetch_sub(request.bytes.len(), Ordering::AcqRel);
                    drop(request);
                    loop {
                        if worker_stop.load(Ordering::Acquire) {
                            return;
                        }
                        match outgoing.try_send(result) {
                            Ok(()) => break,
                            Err(mpsc::TrySendError::Disconnected(_)) => return,
                            Err(mpsc::TrySendError::Full(pending)) => {
                                result = pending;
                                std::thread::park_timeout(std::time::Duration::from_millis(2));
                            }
                        }
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            requests,
            results,
            reserved,
            stop,
            thread: Some(thread),
            backlog: Default::default(),
        })
    }

    pub fn submit(&mut self, key: u64, bytes: Vec<u8>) -> Result<(), String> {
        let size = bytes.len();
        if size > 4 * 1024 * 1024 {
            return Err("carga de áudio excede 4 MiB".into());
        }
        if self.backlog.len() >= 64 {
            return Err("fila de espera de 64 cargas ocupada".into());
        }
        self.reserved
            .try_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                count
                    .checked_add(size)
                    .filter(|sum| *sum <= 8 * 1024 * 1024)
            })
            .map_err(|_| "fila codificada excede 8 MiB")?;
        self.backlog.push_back(Request { key, bytes });
        self.flush();
        Ok(())
    }
    fn flush(&mut self) {
        while let Some(request) = self.backlog.pop_front() {
            match self.requests.try_send(request) {
                Ok(()) => {}
                Err(mpsc::TrySendError::Full(request)) => {
                    self.backlog.push_front(request);
                    break;
                }
                Err(mpsc::TrySendError::Disconnected(request)) => {
                    self.reserved
                        .fetch_sub(request.bytes.len(), Ordering::AcqRel);
                }
            }
        }
    }
    pub fn try_recv(&mut self) -> Option<Decoded> {
        let result = self.results.try_recv().ok();
        self.flush();
        result
    }
    pub fn is_finished(&self) -> bool {
        self.thread
            .as_ref()
            .is_none_or(|worker| worker.is_finished())
    }
}
impl Drop for DecodeWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.thread.take() {
            worker.thread().unpark();
            let _ = worker.join();
        }
    }
}

fn decode(bytes: &[u8], settings: &DecodeSettings) -> Result<(Sound, bool), String> {
    let mut used_bank = false;
    let mut music = |midi: &[u8]| {
        #[cfg(feature = "soundfont")]
        if let Some(sound) = settings
            .bank
            .as_ref()
            .and_then(|bank| super::soundfont::toca(bank, midi, super::soundfont::taxa()))
        {
            used_bank = true;
            return Some(sound);
        }
        #[cfg(not(feature = "soundfont"))]
        let _ = (&settings, &mut used_bank);
        super::midi::decode(midi)
    };
    let result = if bytes.starts_with(b"cmid") {
        super::cmf::decode_with(bytes, &mut music)
    } else if bytes.get(8..12) == Some(b"QLCM") {
        super::qcp::decode(bytes)
    } else if bytes.starts_with(b"MThd") {
        music(bytes).ok_or_else(|| "MIDI não decodificado".to_string())
    } else {
        super::wav::parse(bytes).or_else(|_| super::mp3::decode_detalhado(bytes))
    };
    let sound = result?;
    if sound.samples.len() > 16 * 1024 * 1024 {
        return Err("PCM de uma carga excede 64 MiB".into());
    }
    Ok((sound, used_bank))
}
