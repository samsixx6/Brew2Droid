use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend},
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    storage::StoragePaths,
};
#[path = "common/smaf.rs"]
mod fixtures;
use fixtures::*;
const OUT: u32 = loader::STACK_BASE + 0x1000;
const DATA: u32 = loader::STACK_BASE + 0x4000;
fn call(m: &mut Machine<BackendPadrao>, iface: Interface, name: &str, args: [u32; 4]) -> u32 {
    let slot = (0..256).find(|&s| iface.method(s) == Some(name)).unwrap();
    match m.call_guest(aee::encode(iface, slot), args, 1000).unwrap() {
        Outcome::Returned { code } => code,
        o => panic!("{o:?}"),
    }
}
struct Fixture {
    m: Machine<BackendPadrao>,
    root: std::path::PathBuf,
    player: u32,
}
impl Fixture {
    fn new() -> Self {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "brew-smaf-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        let storage = StoragePaths::from_root(root.join("runtime"));
        storage.create_dirs().unwrap();
        let image = ModImage::parse(
            [0xe3a00000u32, 0xe12fff1e]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect(),
        )
        .unwrap();
        let mut m = Machine::new_with_storage(
            BackendPadrao::new().unwrap(),
            loader::load_with(&image, &[]).unwrap(),
            &root,
            &storage,
            None,
        );
        assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
        assert_eq!(
            call(
                &mut m,
                Interface::Shell,
                "CreateInstance",
                [loader::OBJECT_BASE, 0x01002000, OUT, 0]
            ),
            0
        );
        let player = m.cpu().read_u32(OUT).unwrap();
        Self { m, root, player }
    }
    fn set(&mut self, b: &[u8]) -> u32 {
        self.m.cpu_mut().write_mem(DATA, b).unwrap();
        call(
            &mut self.m,
            Interface::SoundPlayer,
            "Set",
            [self.player, 2, DATA, 0],
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let p = self.root.canonicalize().unwrap();
        assert!(p.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("brew-smaf-")
        );
        let _ = std::fs::remove_dir_all(p);
    }
}
#[test]
fn soundplayer_infers_smaf_buffer_size_and_retains_exact_source() {
    let mut f = Fixture::new();
    let bytes = file(&left(&[0, 1, 2, 2, 0xff, 0, 0, 0, 0, 0]), &[0x70; 64]);
    assert_eq!(f.set(&bytes), 0);
    assert_eq!(
        call(
            &mut f.m,
            Interface::SoundPlayer,
            "GetInfo",
            [f.player, OUT, 0, 0]
        ),
        0
    );
    assert_eq!(f.m.cpu().read_u32(OUT + 4).unwrap(), DATA);
    assert_eq!(f.m.cpu().read_u32(OUT + 8).unwrap(), bytes.len() as u32);
}
#[test]
fn soundplayer_rejects_score_crc_and_unsupported_magic_without_claiming_success() {
    let mut f = Fixture::new();
    let mut bytes = file(&[0, 1, 2], &[0x70]);
    bytes[20] ^= 1;
    assert_eq!(f.set(&bytes), 20);
    let score = seal(
        [
            chunk(*b"CNTI", &[0, 1, 2, 0, 0]),
            chunk([b'M', b'T', b'R', 0], &[0; 6]),
        ]
        .concat(),
    );
    assert_eq!(f.set(&score), 20);
    assert_eq!(f.set(b"MMSF\0\0\0\x10paddingpadding"), 20);
    assert_eq!(f.set(b"MMMD\xff\xff\xff\xffpaddingpadding"), 2);
}
#[test]
fn decode_worker_routes_supported_atr_to_real_pcm() {
    use zeebx::audio::decode::{DecodeSettings, DecodeWorker};
    let mut worker = DecodeWorker::new(DecodeSettings::default()).unwrap();
    worker
        .submit(77, file(&left(&[0, 1, 2, 2, 0xff, 0, 0, 0, 0, 0]), &[0x70]))
        .unwrap();
    let start = std::time::Instant::now();
    loop {
        if let Some(d) = worker.try_recv() {
            assert_eq!(d.key, 77);
            let s = d.sound.unwrap();
            assert_eq!(s.rate, 8000);
            assert_eq!(pcm(s.samples[0]), 15);
            assert_eq!(pcm(s.samples[2]), 253);
            break;
        }
        assert!(start.elapsed().as_secs() < 5);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[test]
fn soundplayer_rejects_zero_duration_source_consistently_with_worker() {
    use zeebx::audio::decode::{DecodeSettings, DecodeWorker};
    let bytes = file(&[0, 0xff, 0, 0, 0, 0, 0], &[0x70]);
    let mut worker = DecodeWorker::new(DecodeSettings::default()).unwrap();
    worker.submit(91, bytes.clone()).unwrap();
    let start = std::time::Instant::now();
    let decode_error = loop {
        if let Some(result) = worker.try_recv() {
            assert_eq!(result.key, 91);
            break result.sound.unwrap_err();
        }
        assert!(start.elapsed().as_secs() < 5);
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    assert!(decode_error.contains("duração"));
    let mut f = Fixture::new();
    assert_eq!(
        f.set(&bytes),
        20,
        "Set must reject the same unsupported source as the worker"
    );
    assert_eq!(
        call(
            &mut f.m,
            Interface::SoundPlayer,
            "GetInfo",
            [f.player, OUT, 0, 0]
        ),
        20,
        "rejected source must not publish a buffer"
    );
}
