use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend};
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

const SHELL: u32 = loader::OBJECT_BASE;
const BUFFER: u32 = loader::STACK_BASE + 0x100;

struct Fixture {
    machine: Machine<BackendPadrao>,
    work: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        Self::with_overlay(false)
    }
    fn with_overlay(overlay: bool) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let work = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests")
            .join(format!(
                "reported-contract-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&work).unwrap();
        let storage = StoragePaths::from_root(work.join("runtime"));
        storage.create_dirs().unwrap();
        let image = ModImage::parse(
            [0xe3a0_0000u32, 0xe12f_ff1e]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect(),
        )
        .unwrap();
        let content = if overlay {
            work.join("content")
        } else {
            work.clone()
        };
        std::fs::create_dir_all(&content).unwrap();
        let mut machine = Machine::new_with_storage(
            BackendPadrao::new().unwrap(),
            loader::load_with(&image, &[]).unwrap(),
            &content,
            &storage,
            overlay.then(|| work.join("overlay")),
        );
        assert_eq!(machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
        Self { machine, work }
    }

    fn call(&mut self, interface: Interface, name: &str, arguments: [u32; 4]) -> u32 {
        let slot = (0..256)
            .find(|slot| interface.method(*slot) == Some(name))
            .unwrap();
        match self
            .machine
            .call_guest(aee::encode(interface, slot), arguments, 1000)
            .unwrap()
        {
            Outcome::Returned { code } => code,
            outcome => panic!("{name}: {outcome:?}"),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let private_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests")
            .canonicalize()
            .unwrap();
        let actual = self.work.canonicalize().unwrap();
        assert!(actual.starts_with(private_root));
        std::fs::remove_dir_all(actual).unwrap();
    }
}

impl Fixture {
    fn slow_pcm(&mut self) -> (zeebx::audio::Mixer, u32, u32) {
        let mixer = zeebx::audio::Mixer::silent(1000);
        let (source, media) = self.add_slow_pcm(&mixer, 0);
        (mixer, source, media)
    }
    fn add_slow_pcm(&mut self, mixer: &zeebx::audio::Mixer, offset: u32) -> (u32, u32) {
        self.machine.set_audio(Some(mixer.clone()));
        let source = BUFFER + 256 + offset;
        let table = source + 64;
        let read = loader::MODULE_BASE + 0x100;
        self.machine.cpu_mut().write_u32(source, table).unwrap();
        self.machine.cpu_mut().write_u32(table + 12, read).unwrap();
        let code = [
            0xe92d4010u32,
            0xe5903004,
            0xe2833001,
            0xe5803004,
            0xe1a04001,
            0xe3a030c8,
            0xe2533001,
            0x1afffffd,
            0xe3a03901,
            0xe1c430b0,
            0xe3a03903,
            0xe1c430b2,
            0xe3a00004,
            0xe8bd4010,
            0xe12fff1e,
        ];
        self.machine
            .cpu_mut()
            .write_mem(
                read,
                &code
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        let data = BUFFER + 64 + offset;
        let spec = BUFFER + 128 + offset;
        for (offset, value) in [(0, 0x01001012), (4, source), (20, 1), (24, spec), (32, 4)] {
            self.machine
                .cpu_mut()
                .write_u32(data + offset, value)
                .unwrap();
        }
        self.machine
            .cpu_mut()
            .write_mem(spec + 8, &1u16.to_le_bytes())
            .unwrap();
        self.machine.cpu_mut().write_u32(spec + 12, 1000).unwrap();
        self.machine
            .cpu_mut()
            .write_mem(spec + 16, &16u16.to_le_bytes())
            .unwrap();
        assert_eq!(
            self.call(
                Interface::Shell,
                "CreateInstance",
                [SHELL, 0x01005511, BUFFER, 0]
            ),
            0
        );
        let media = self.machine.cpu().read_u32(BUFFER).unwrap();
        assert_eq!(
            self.call(Interface::Media, "SetMediaParm", [media, 1, data, 1]),
            0
        );
        assert_eq!(self.call(Interface::Media, "Play", [media, 0, 0, 0]), 0);
        (source, media)
    }
    fn queue_frame_timer(&mut self) -> u32 {
        let function = loader::MODULE_BASE + 0x180;
        let marker = BUFFER + 768;
        self.machine
            .cpu_mut()
            .write_mem(
                function,
                &[0xe3a01001u32, 0xe5801000, 0xe12fff1e]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        assert_eq!(
            self.call(Interface::Shell, "SetTimer", [SHELL, 0, function, marker]),
            0
        );
        marker
    }
}

#[test]
fn budgeted_pcm_read_feeds_audio_and_allows_the_game_frame_timer_to_run() {
    let mut f = Fixture::new();
    let (mixer, source, _) = f.slow_pcm();
    let marker = f.queue_frame_timer();
    f.machine.deliver_signals(16).unwrap();
    assert!(
        !f.machine.handset_event_ready(),
        "o mixer está no meio de Read"
    );
    for _ in 0..1500 {
        f.machine.advance(16).unwrap();
        f.machine.deliver_signals(16).unwrap();
        f.machine.deliver_callbacks(16).unwrap();
        if f.machine.cpu().read_u32(marker).unwrap() == 1 {
            break;
        }
    }
    assert_eq!(
        f.machine.cpu().read_u32(marker).unwrap(),
        1,
        "o Read de áudio não pode impedir o próximo quadro"
    );
    assert!(
        mixer.render(32).iter().any(|v| v.abs() > 0.1),
        "o resultado de Read retomado precisa alimentar PCM"
    );
    assert!(
        f.machine.cpu().read_u32(source + 4).unwrap() < 10,
        "não reiniciar Read sem contabilizar sua conclusão"
    );
}

#[test]
fn pending_pcm_read_survives_save_restore_without_restarting_or_losing_samples() {
    let mut f = Fixture::new();
    let (_, source, _) = f.slow_pcm();
    let marker = f.queue_frame_timer();
    f.machine.deliver_signals(16).unwrap();
    let state = f.machine.grava_estado();
    let mut restored = Fixture::new();
    let mixer = zeebx::audio::Mixer::silent(1000);
    restored.machine.set_audio(Some(mixer.clone()));
    restored.machine.restaura_estado(&state).unwrap();
    for _ in 0..1500 {
        restored.machine.advance(16).unwrap();
        restored.machine.deliver_signals(16).unwrap();
        restored.machine.deliver_callbacks(16).unwrap();
        if restored.machine.cpu().read_u32(marker).unwrap() == 1 {
            break;
        }
    }
    assert_eq!(restored.machine.cpu().read_u32(marker).unwrap(), 1);
    assert!(mixer.render(32).iter().any(|v| v.abs() > 0.1));
    assert!(restored.machine.cpu().read_u32(source + 4).unwrap() < 10);
}

#[test]
fn two_budgeted_pcm_sources_both_receive_service_before_the_first_catches_up() {
    let mut f = Fixture::new();
    let mixer = zeebx::audio::Mixer::silent(1000);
    let (a, _) = f.add_slow_pcm(&mixer, 0);
    let (b, _) = f.add_slow_pcm(&mixer, 1024);
    f.machine.deliver_signals(16).unwrap();
    for _ in 0..100 {
        f.machine.advance(16).unwrap();
        f.machine.deliver_signals(16).unwrap();
    }
    assert!(
        f.machine.cpu().read_u32(a + 4).unwrap() > 0,
        "a primeira fonte precisa receber Read"
    );
    assert!(
        f.machine.cpu().read_u32(b + 4).unwrap() > 0,
        "a segunda fonte não pode esperar a primeira zerar sua dívida"
    );
}

#[test]
fn completed_pcm_result_survives_save_before_the_mixer_consumes_it() {
    let mut f = Fixture::new();
    let (_, source, _) = f.slow_pcm();
    f.machine.deliver_signals(16).unwrap();
    for _ in 0..100 {
        f.machine.advance(16).unwrap();
        if f.machine.handset_event_ready() {
            break;
        }
    }
    assert!(f.machine.handset_event_ready());
    assert_eq!(f.machine.cpu().read_u32(source + 4).unwrap(), 1);
    let state = f.machine.grava_estado();
    let mut restored = Fixture::new();
    let mixer = zeebx::audio::Mixer::silent(1000);
    restored.machine.set_audio(Some(mixer.clone()));
    restored.machine.restaura_estado(&state).unwrap();
    restored.machine.deliver_signals(16).unwrap();
    assert_eq!(restored.machine.cpu().read_u32(source + 4).unwrap(), 1);
    assert!(mixer.render(32).iter().any(|v| v.abs() > 0.1));
}

#[test]
fn stopping_media_during_budgeted_read_discards_the_completed_audio() {
    let mut f = Fixture::new();
    let (mixer, _, media) = f.slow_pcm();
    f.machine.deliver_signals(16).unwrap();
    assert!(!f.machine.handset_event_ready());
    assert_eq!(f.call(Interface::Media, "Stop", [media, 0, 0, 0]), 0);
    for _ in 0..100 {
        f.machine.advance(16).unwrap();
        if f.machine.handset_event_ready() {
            break;
        }
    }
    assert!(f.machine.handset_event_ready());
    f.machine.deliver_signals(16).unwrap();
    assert!(mixer.render(32).iter().all(|v| *v == 0.));
}

#[test]
fn pausing_media_during_budgeted_read_preserves_samples_for_resume() {
    let mut f = Fixture::new();
    let (mixer, _, media) = f.slow_pcm();
    f.machine.deliver_signals(16).unwrap();
    assert!(!f.machine.handset_event_ready());
    assert_eq!(f.call(Interface::Media, "Pause", [media, 0, 0, 0]), 0);
    for _ in 0..100 {
        f.machine.advance(16).unwrap();
        if f.machine.handset_event_ready() {
            break;
        }
    }
    assert!(f.machine.handset_event_ready());
    f.machine.deliver_signals(16).unwrap();
    assert!(mixer.render(32).iter().all(|v| *v == 0.));
    assert_eq!(f.call(Interface::Media, "Resume", [media, 0, 0, 0]), 0);
    assert!(
        mixer.render(32).iter().any(|v| v.abs() > 0.1),
        "Pause deve conservar os bytes já produzidos para Resume"
    );
}
