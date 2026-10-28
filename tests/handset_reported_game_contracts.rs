use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend, Reg};
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

#[test]
fn query_class_distinguishes_native_objects_from_applets_and_unknown_classes() {
    let mut f = Fixture::new();
    assert_eq!(
        f.call(Interface::Shell, "QueryClass", [SHELL, 0x01001003, 0, 0]),
        1
    );
    assert_eq!(
        f.call(
            Interface::Shell,
            "QueryClass",
            [SHELL, 0x01001003, BUFFER, 0]
        ),
        0
    );
    assert_eq!(
        f.call(Interface::Shell, "QueryClass", [SHELL, 0x01abcdef, 0, 0]),
        0
    );
}

#[test]
fn unsupported_notifier_returns_documented_error_without_stopping_the_applet() {
    let mut f = Fixture::new();
    assert_eq!(
        f.call(
            Interface::Shell,
            "RegisterNotify",
            [SHELL, 0x0107342f, 0x0100105a, 1]
        ),
        10
    );
    assert_eq!(
        f.call(
            Interface::Shell,
            "RegisterNotify",
            [SHELL, 0, 0x0100105a, 1]
        ),
        14
    );
    assert!(f.machine.missing_apis().is_empty());
}

#[test]
fn indexed_bmp_resource_preserves_depth_palette_and_guest_edits() {
    let mut f = Fixture::new();
    let mut bmp = vec![0u8; 122];
    bmp[..2].copy_from_slice(b"BM");
    for (at, v) in [(2, 122u32), (10, 118), (14, 40), (18, 2), (22, 1), (46, 16)] {
        bmp[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
    bmp[28..30].copy_from_slice(&4u16.to_le_bytes());
    bmp[58..62].copy_from_slice(&[0, 0, 255, 0]);
    bmp[62..66].copy_from_slice(&[0, 255, 0, 0]);
    bmp[118] = 0x12;
    std::fs::write(f.work.join("indexed.bmp"), bmp).unwrap();
    f.machine
        .cpu_mut()
        .write_mem(BUFFER, b"indexed.bmp\0")
        .unwrap();
    let bitmap = f.call(
        Interface::Shell,
        "LoadResObject",
        [SHELL, BUFFER, 0, 0x01001021],
    );
    assert_ne!(bitmap, 0);
    let mut header = [0; 32];
    f.machine.cpu().read_mem(bitmap, &mut header).unwrap();
    assert_eq!(
        header[28], 4,
        "IBitmap precisa conservar os índices em vez de anunciar RGB565"
    );
    let palette = u32::from_le_bytes(header[12..16].try_into().unwrap());
    assert_ne!(palette, 0);
    assert_eq!(
        f.call(Interface::Bitmap, "RGBToNative", [bitmap, 0x0000ff00, 0, 0]),
        1
    );
    f.machine
        .cpu_mut()
        .write_u32(palette + 8, 0x000000ff)
        .unwrap();
    let sp = f.machine.cpu().read_reg(Reg::Sp);
    f.machine.cpu_mut().write_reg(Reg::Sp, sp - 32);
    for (i, v) in [1, bitmap, 0, 0, 0].into_iter().enumerate() {
        f.machine
            .cpu_mut()
            .write_u32(sp - 32 + i as u32 * 4, v)
            .unwrap();
    }
    f.call(Interface::Display, "BitBlt", [SHELL + 16, 0, 0, 2]);
    f.machine.cpu_mut().write_reg(Reg::Sp, sp);
    let pixels = f.machine.screen().to_rgb565_bytes();
    assert_eq!(&pixels[..4], &[0, 248, 31, 0]);
}

#[test]
fn legacy_egl_announces_the_supported_uppercase_color_buffer_extension() {
    let mut f = Fixture::new();
    let ptr = f.call(Interface::EglLegacy, "eglQueryString", [1, 0x3055, 0, 0]);
    assert!(
        f.machine
            .cpu()
            .read_cstring(ptr, 1024)
            .contains("EGL_QUALCOMM_COLOR_BUFFER ")
    );
}

#[test]
fn source_util_peek_from_memory_obeys_the_sdk_slot_and_can_be_read() {
    let mut f = Fixture::new();
    let data = BUFFER + 64;
    let out = BUFFER + 100;
    f.machine.cpu_mut().write_mem(data, b"sound data").unwrap();
    f.call(
        Interface::Shell,
        "CreateInstance",
        [SHELL, 0x01001011, out, 0],
    );
    let util = f.machine.cpu().read_u32(out).unwrap();
    let sp = f.machine.cpu().read_reg(Reg::Sp);
    f.machine.cpu_mut().write_reg(Reg::Sp, sp - 32);
    f.machine.cpu_mut().write_u32(sp - 32, 0).unwrap();
    f.machine.cpu_mut().write_u32(sp - 28, out).unwrap();
    assert_eq!(
        f.machine
            .call_guest(
                aee::encode(Interface::SourceUtil, 8),
                [util, data, 10, 0],
                1000
            )
            .unwrap(),
        Outcome::Returned { code: 0 }
    );
    f.machine.cpu_mut().write_reg(Reg::Sp, sp);
    let source = f.machine.cpu().read_u32(out).unwrap();
    let table = f.machine.cpu().read_u32(source).unwrap();
    let read = f.machine.cpu().read_u32(table + 12).unwrap();
    assert_eq!(
        f.machine
            .call_guest(read, [source, data + 128, 10, 0], 1000)
            .unwrap(),
        Outcome::Returned { code: 10 }
    );
    let mut bytes = [0; 10];
    f.machine.cpu().read_mem(data + 128, &mut bytes).unwrap();
    assert_eq!(&bytes, b"sound data");
}

#[test]
fn ellipse_draws_its_outline_and_fill_without_coloring_the_corners() {
    let mut f = Fixture::new();
    f.call(
        Interface::Shell,
        "CreateInstance",
        [SHELL, 0x01002001, BUFFER, 0],
    );
    let g = f.machine.cpu().read_u32(BUFFER).unwrap();
    f.call(Interface::Graphics, "SetColor", [g, 255, 0, 0]);
    f.call(Interface::Graphics, "SetFillColor", [g, 0, 0, 255]);
    f.call(Interface::Graphics, "SetFillMode", [g, 1, 0, 0]);
    f.machine
        .cpu_mut()
        .write_mem(
            BUFFER + 16,
            &[6i16, 5, 5, 4]
                .into_iter()
                .flat_map(i16::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    assert_eq!(
        f.call(Interface::Graphics, "DrawEllipse", [g, BUFFER + 16, 0, 0]),
        0
    );
    assert_eq!(f.machine.screen().get_pixel(1, 1), 0);
    assert_eq!(f.machine.screen().get_pixel(6, 1), 0xf800);
    assert_eq!(f.machine.screen().get_pixel(6, 5), 0x001f);
}

#[test]
fn shell_resume_defers_long_callbacks_until_the_next_event_loop_turn() {
    let mut f = Fixture::new();
    let code = loader::MODULE_BASE + 0x20;
    let pcb = BUFFER + 64;
    let counter = BUFFER + 96;
    let words = [
        0xe3a01001u32,
        0xe5801000,
        0xe3a02ffa,
        0xe2522001,
        0x1afffffd,
        0xe3a00001,
        0xe12fff1e,
    ];
    f.machine
        .cpu_mut()
        .write_mem(
            code,
            &words
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    f.machine.cpu_mut().write_u32(pcb + 16, code).unwrap();
    f.machine.cpu_mut().write_u32(pcb + 20, counter).unwrap();
    assert_eq!(f.call(Interface::Shell, "Resume", [SHELL, pcb, 0, 0]), 0);
    assert_eq!(
        f.machine.cpu().read_u32(counter).unwrap(),
        0,
        "Resume não pode executar dentro de quem agendou"
    );
    assert_eq!(
        f.machine.deliver_signals(16).unwrap(),
        vec![Outcome::Budget]
    );
    for _ in 0..1000 {
        f.machine.comeca_volta();
        f.machine.advance(16).unwrap();
        f.machine.fecha_volta();
        if f.machine.handset_event_ready() {
            break;
        }
    }
    assert!(
        f.machine.handset_event_ready(),
        "pc={:x} r2={:x}",
        f.machine.cpu().read_reg(Reg::Pc),
        f.machine.cpu().read_reg(Reg::R2)
    );
    assert_eq!(f.machine.cpu().read_u32(counter).unwrap(), 1);
    assert!(f.machine.deliver_signals(1000).unwrap().is_empty());
}

#[test]
fn async_flag_without_unique_defers_the_applet_event() {
    let mut f = Fixture::new();
    let code = loader::MODULE_BASE + 0x20;
    f.machine
        .cpu_mut()
        .write_mem(
            code,
            &[0xe5801004u32, 0xe3a00001, 0xe12fff1e]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    let applet = f.call(Interface::Helpers, "malloc", [256, 0, 0, 0]);
    f.machine.cpu_mut().write_u32(applet, applet + 128).unwrap();
    f.machine.cpu_mut().write_u32(applet + 136, code).unwrap();
    f.machine.start_applet(applet, 0x01012345, 1000).unwrap();
    f.machine.cpu_mut().write_u32(applet + 4, 0).unwrap();
    let sp = f.machine.cpu().read_reg(Reg::Sp);
    f.machine.cpu_mut().write_reg(Reg::Sp, sp - 32);
    f.machine.cpu_mut().write_mem(sp - 32, &[0; 8]).unwrap();
    assert_eq!(
        f.call(Interface::Shell, "SendEvent", [SHELL, 2, 0, 0x7003]),
        1
    );
    f.machine.cpu_mut().write_reg(Reg::Sp, sp);
    assert_eq!(f.machine.cpu().read_u32(applet + 4).unwrap(), 0);
    assert_eq!(
        f.machine.deliver_callbacks(1000).unwrap(),
        vec![Outcome::Returned { code: 1 }]
    );
    assert_eq!(f.machine.cpu().read_u32(applet + 4).unwrap(), 0x7003);
}

#[test]
fn handset_device_info_reports_supported_audio_and_pointer_capabilities() {
    let mut f = Fixture::new();
    f.machine.enable_handset_clock();
    f.call(Interface::Shell, "GetDeviceInfo", [SHELL, BUFFER, 0, 0]);
    let flags = f.machine.cpu().read_u32(BUFFER + 28).unwrap();
    assert_eq!(
        flags & 0x1c0,
        0x1c0,
        "MIDI/CMX/Pen existem no aparelho virtual"
    );
}

#[test]
fn sound_player_stream_reads_encoded_audio_and_produces_real_pcm() {
    let mut f = Fixture::new();
    let mixer = zeebx::audio::Mixer::silent(8000);
    f.machine.set_audio(Some(mixer.clone()));
    let samples = [12000i16, -12000, 12000, -12000];
    let mut wav = b"RIFF".to_vec();
    wav.extend(44u32.to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(8000u32.to_le_bytes());
    wav.extend(16000u32.to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend(8u32.to_le_bytes());
    wav.extend(samples.into_iter().flat_map(i16::to_le_bytes));
    let data = BUFFER + 256;
    let out = BUFFER + 128;
    f.machine.cpu_mut().write_mem(data, &wav).unwrap();
    f.call(
        Interface::Shell,
        "CreateInstance",
        [SHELL, 0x01001011, out, 0],
    );
    let util = f.machine.cpu().read_u32(out).unwrap();
    let sp = f.machine.cpu().read_reg(Reg::Sp);
    f.machine.cpu_mut().write_reg(Reg::Sp, sp - 32);
    f.machine.cpu_mut().write_u32(sp - 32, 0).unwrap();
    f.machine.cpu_mut().write_u32(sp - 28, out).unwrap();
    f.call(
        Interface::SourceUtil,
        "SourceFromMemory",
        [util, data, wav.len() as u32, 0],
    );
    f.machine.cpu_mut().write_reg(Reg::Sp, sp);
    let source = f.machine.cpu().read_u32(out).unwrap();
    f.call(
        Interface::Shell,
        "CreateInstance",
        [SHELL, 0x01002000, out, 0],
    );
    let player = f.machine.cpu().read_u32(out).unwrap();
    assert_eq!(
        f.call(Interface::SoundPlayer, "SetStream", [player, source, 0, 0]),
        0
    );
    f.call(Interface::SoundPlayer, "Play", [player, 0, 0, 0]);
    assert!(
        mixer.render(32).iter().any(|n| n.abs() > 0.05),
        "Playing sem amostras não corrige o som"
    );
}

#[test]
fn native_sound_volume_callback_is_deferred_and_keeps_a_long_loader_alive() {
    let mut f = Fixture::new();
    let code = loader::MODULE_BASE + 0x20;
    let counter = BUFFER + 96;
    f.machine
        .cpu_mut()
        .write_mem(
            code,
            &[
                0xe3a01001u32,
                0xe5801000,
                0xe3a02ffa,
                0xe2522001,
                0x1afffffd,
                0xe3a00001,
                0xe12fff1e,
            ]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>(),
        )
        .unwrap();
    f.call(
        Interface::Shell,
        "CreateInstance",
        [SHELL, 0x01001056, BUFFER, 0],
    );
    let sound = f.machine.cpu().read_u32(BUFFER).unwrap();
    f.call(
        Interface::Sound,
        "RegisterNotify",
        [sound, code, counter, 0],
    );
    f.call(Interface::Sound, "GetVolume", [sound, 0, 0, 0]);
    assert_eq!(
        f.machine.cpu().read_u32(counter).unwrap(),
        0,
        "callback de volume não pode iniciar a carga dentro da API"
    );
    assert_eq!(
        f.machine.deliver_signals(16).unwrap(),
        vec![Outcome::Budget]
    );
    for _ in 0..1000 {
        f.machine.advance(16).unwrap();
        if f.machine.handset_event_ready() {
            break;
        }
    }
    assert!(f.machine.handset_event_ready());
    assert_eq!(f.machine.cpu().read_u32(counter).unwrap(), 1);
}

#[test]
fn native_sound_unregister_and_release_cancel_only_their_own_pending_notifications() {
    for release in [false, true] {
        let mut f = Fixture::new();
        let code = loader::MODULE_BASE + 0x20;
        f.machine
            .cpu_mut()
            .write_mem(
                code,
                &[0xe3a01001u32, 0xe5801000, 0xe12fff1e]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        let mut sounds = Vec::new();
        for counter in [BUFFER + 64, BUFFER + 68] {
            f.call(
                Interface::Shell,
                "CreateInstance",
                [SHELL, 0x01001056, BUFFER, 0],
            );
            let sound = f.machine.cpu().read_u32(BUFFER).unwrap();
            sounds.push(sound);
            f.call(
                Interface::Sound,
                "RegisterNotify",
                [sound, code, counter, 0],
            );
            f.call(Interface::Sound, "GetVolume", [sound, 0, 0, 0]);
        }
        if release {
            f.call(Interface::Sound, "Release", [sounds[0], 0, 0, 0]);
        } else {
            f.call(Interface::Sound, "RegisterNotify", [sounds[0], 0, 0, 0]);
        }
        for _ in 0..4 {
            f.machine.deliver_signals(1000).unwrap();
            f.machine.deliver_callbacks(1000).unwrap();
        }
        assert_eq!(
            f.machine.cpu().read_u32(BUFFER + 64).unwrap(),
            0,
            "aviso cancelado chamou o contexto antigo"
        );
        assert_eq!(
            f.machine.cpu().read_u32(BUFFER + 68).unwrap(),
            1,
            "outro som deve conservar seu aviso"
        );
    }
}

#[test]
fn shell_resume_replaces_and_cancels_a_callback_by_its_identity() {
    for cancel in [false, true] {
        let mut f = Fixture::new();
        let code = loader::MODULE_BASE + 0x20;
        let pcb = BUFFER + 64;
        let counter = BUFFER + 96;
        f.machine
            .cpu_mut()
            .write_mem(
                code,
                &[0xe5901000u32, 0xe2811001, 0xe5801000, 0xe12fff1e]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        f.machine.cpu_mut().write_u32(pcb + 16, code).unwrap();
        f.machine.cpu_mut().write_u32(pcb + 20, counter).unwrap();
        f.call(Interface::Shell, "Resume", [SHELL, pcb, 0, 0]);
        f.call(Interface::Shell, "Resume", [SHELL, pcb, 0, 0]);
        let hook = f.machine.cpu().read_u32(pcb + 8).unwrap();
        assert_ne!(hook, 0, "Resume deve instalar CALLBACK_Cancel");
        if cancel {
            f.machine.call_guest(hook, [pcb, 0, 0, 0], 1000).unwrap();
        }
        for _ in 0..3 {
            f.machine.deliver_signals(1000).unwrap();
            f.machine.deliver_callbacks(1000).unwrap();
        }
        assert_eq!(
            f.machine.cpu().read_u32(counter).unwrap(),
            u32::from(!cancel)
        );
        assert_eq!(
            f.machine.cpu().read_u32(pcb + 8).unwrap(),
            0,
            "entrega deve desarmar cancelamento"
        );
    }
}

#[test]
fn native_volume_arguments_and_pending_resume_survive_save_restore() {
    let mut f = Fixture::new();
    let code = loader::MODULE_BASE + 0x20;
    let pcb = BUFFER + 64;
    let values = BUFFER + 128;
    f.machine
        .cpu_mut()
        .write_mem(
            code,
            &[0xe5801000u32, 0xe5802004, 0xe5803008, 0xe12fff1e]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    f.call(
        Interface::Shell,
        "CreateInstance",
        [SHELL, 0x01001056, BUFFER, 0],
    );
    let sound = f.machine.cpu().read_u32(BUFFER).unwrap();
    f.call(Interface::Sound, "RegisterNotify", [sound, code, values, 0]);
    f.call(Interface::Sound, "GetVolume", [sound, 0, 0, 0]);
    f.machine.cpu_mut().write_u32(pcb + 16, code).unwrap();
    f.machine
        .cpu_mut()
        .write_u32(pcb + 20, values + 16)
        .unwrap();
    f.call(Interface::Shell, "Resume", [SHELL, pcb, 0, 0]);
    let state = f.machine.grava_estado();
    let mut restored = Fixture::new();
    restored.machine.restaura_estado(&state).unwrap();
    for _ in 0..3 {
        restored.machine.deliver_signals(1000).unwrap();
    }
    assert_eq!(restored.machine.cpu().read_u32(values).unwrap(), 1);
    assert_eq!(restored.machine.cpu().read_u32(values + 4).unwrap(), 1);
    assert_eq!(
        restored.machine.cpu().read_u32(values + 8).unwrap(),
        100,
        "volume nativo é inline"
    );
    assert_eq!(
        restored.machine.cpu().read_u32(pcb + 8).unwrap(),
        0,
        "Resume salvo deve ser consumido"
    );
}

#[test]
fn graphical_arcs_draw_real_ellipses_and_pie_sectors() {
    let mut f = Fixture::new();
    f.call(
        Interface::Shell,
        "CreateInstance",
        [SHELL, 0x01002001, BUFFER, 0],
    );
    let g = f.machine.cpu().read_u32(BUFFER).unwrap();
    f.call(Interface::Graphics, "SetColor", [g, 255, 0, 0]);
    f.call(Interface::Graphics, "SetFillColor", [g, 0, 0, 255]);
    f.call(Interface::Graphics, "SetFillMode", [g, 1, 0, 0]);
    f.machine
        .cpu_mut()
        .write_mem(
            BUFFER + 16,
            &[1i16, 1, 11, 9]
                .into_iter()
                .flat_map(i16::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    assert_eq!(
        f.call(
            Interface::Graphics,
            "DrawEllipticalArc",
            [g, BUFFER + 16, 0, 360]
        ),
        0
    );
    assert_eq!(f.machine.screen().get_pixel(6, 1), 0xf800);
    assert_eq!(f.machine.screen().get_pixel(6, 5), 0x001f);
    f.machine
        .cpu_mut()
        .write_mem(
            BUFFER + 16,
            &[30i16, 30, 8, 0, 90]
                .into_iter()
                .flat_map(i16::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    assert_eq!(
        f.call(Interface::Graphics, "DrawPie", [g, BUFFER + 16, 0, 0]),
        0
    );
    assert_eq!(f.machine.screen().get_pixel(33, 27), 0x001f);
    assert_eq!(f.machine.screen().get_pixel(27, 33), 0);
    assert_eq!(f.machine.screen().get_pixel(38, 30), 0xf800);
}

#[test]
fn query_class_returns_the_selected_manifest_record_and_guest_filename() {
    let mut f = Fixture::new();
    let class = 0x01017777u32;
    let mut mif = vec![0u8; 60];
    mif[..2].copy_from_slice(&0x11u16.to_le_bytes());
    mif[2..4].copy_from_slice(&1u16.to_le_bytes());
    for (at, value) in [
        (8, 32u32),
        (16, 32),
        (20, 1),
        (24, 40),
        (28, 20),
        (32, 40),
        (36, 60),
        (40, class),
    ] {
        mif[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    mif[48..50].copy_from_slice(&1000u16.to_le_bytes());
    mif[58..60].copy_from_slice(&32u16.to_le_bytes());
    assert!(f.machine.set_applet_mif_named(&mif, class, "chosen.mif"));
    assert_eq!(
        f.call(Interface::Shell, "QueryClass", [SHELL, class, BUFFER, 0]),
        1
    );
    assert_eq!(f.machine.cpu().read_u32(BUFFER).unwrap(), class);
    let name = f.machine.cpu().read_u32(BUFFER + 4).unwrap();
    assert_eq!(f.machine.cpu().read_cstring(name, 32), "chosen.mif");
    assert_eq!(f.machine.cpu().read_u32(BUFFER + 8).unwrap() & 65535, 1000);
    assert_eq!(f.machine.cpu().read_u32(BUFFER + 16).unwrap() >> 16, 32);
}

#[test]
fn review_mixed_media_batch_can_cancel_a_later_native_sound_notice() {
    let mut f = Fixture::new();
    let release = loader::MODULE_BASE + 0x20;
    let marker = loader::MODULE_BASE + 0x60;
    let holder = BUFFER + 224;
    let counter = BUFFER + 240;
    f.machine
        .cpu_mut()
        .write_mem(
            release,
            &[
                0xe92d4010u32,
                0xe5900000,
                0xe5903000,
                0xe5933004,
                0xe1a0e00f,
                0xe12fff13,
                0xe8bd8010,
            ]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>(),
        )
        .unwrap();
    f.machine
        .cpu_mut()
        .write_mem(
            marker,
            &[0xe3a01001u32, 0xe5801000, 0xe12fff1e]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    f.call(
        Interface::Shell,
        "CreateInstance",
        [SHELL, 0x01001011, BUFFER, 0],
    );
    let util = f.machine.cpu().read_u32(BUFFER).unwrap();
    let sp = f.machine.cpu().read_reg(Reg::Sp);
    f.machine.cpu_mut().write_reg(Reg::Sp, sp - 32);
    f.machine.cpu_mut().write_u32(sp - 32, 0).unwrap();
    f.machine
        .cpu_mut()
        .write_u32(sp - 28, BUFFER + 128)
        .unwrap();
    f.call(
        Interface::SourceUtil,
        "SourceFromMemory",
        [util, BUFFER + 256, 4, 0],
    );
    f.machine.cpu_mut().write_reg(Reg::Sp, sp);
    let source = f.machine.cpu().read_u32(BUFFER + 128).unwrap();
    f.call(
        Interface::Shell,
        "CreateInstance",
        [SHELL, 0x01002000, BUFFER, 0],
    );
    let player = f.machine.cpu().read_u32(BUFFER).unwrap();
    f.call(
        Interface::SoundPlayer,
        "RegisterNotify",
        [player, release, holder, 0],
    );
    f.call(Interface::SoundPlayer, "SetStream", [player, source, 0, 0]);
    f.call(
        Interface::Shell,
        "CreateInstance",
        [SHELL, 0x01001056, BUFFER, 0],
    );
    let sound = f.machine.cpu().read_u32(BUFFER).unwrap();
    f.machine.cpu_mut().write_u32(holder, sound).unwrap();
    f.call(
        Interface::Sound,
        "RegisterNotify",
        [sound, marker, counter, 0],
    );
    f.call(Interface::Sound, "GetVolume", [sound, 0, 0, 0]);
    for _ in 0..4 {
        f.machine.deliver_signals(1000).unwrap();
    }
    assert_eq!(
        f.machine.cpu().read_u32(counter).unwrap(),
        0,
        "aviso na cauda precisa continuar cancelável pelo callback anterior"
    );
}

#[test]
fn review_draw_arc_remains_an_outline_when_fill_mode_is_enabled() {
    let mut f = Fixture::new();
    f.call(
        Interface::Shell,
        "CreateInstance",
        [SHELL, 0x01002001, BUFFER, 0],
    );
    let g = f.machine.cpu().read_u32(BUFFER).unwrap();
    f.call(Interface::Graphics, "SetColor", [g, 255, 0, 0]);
    f.call(Interface::Graphics, "SetFillColor", [g, 0, 0, 255]);
    f.call(Interface::Graphics, "SetFillMode", [g, 1, 0, 0]);
    f.machine
        .cpu_mut()
        .write_mem(
            BUFFER + 16,
            &[50i16, 50, 8, 0, 90]
                .into_iter()
                .flat_map(i16::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    assert_eq!(
        f.call(Interface::Graphics, "DrawArc", [g, BUFFER + 16, 0, 0]),
        0
    );
    assert_eq!(
        f.machine.screen().get_pixel(53, 47),
        0,
        "DrawArc não preenche setor"
    );
    assert_eq!(f.machine.screen().get_pixel(50, 42), 0xf800);
}

#[test]
fn empty_dib_can_be_initialized_to_wrap_external_pixels() {
    let mut f = Fixture::new();
    let sp = f.machine.cpu().read_reg(Reg::Sp);
    f.machine.cpu_mut().write_reg(Reg::Sp, sp - 32);
    f.machine.cpu_mut().write_u32(sp - 32, 0).unwrap();
    assert_eq!(
        f.call(Interface::Display, "CreateDIBitmap", [SHELL, BUFFER, 0, 0]),
        0,
        "um descritor vazio não precisa escolher formato nem alocar pixels"
    );
    let dib = f.machine.cpu().read_u32(BUFFER).unwrap();
    assert_ne!(dib, 0);
    assert_eq!(f.machine.cpu().read_u32(dib + 8).unwrap(), 0);
    f.machine
        .cpu_mut()
        .write_u32(dib + 8, BUFFER + 256)
        .unwrap();
    f.machine
        .cpu_mut()
        .write_mem(
            dib + 20,
            &[2u16, 1, 4, 0]
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    f.machine.cpu_mut().write_mem(dib + 28, &[16, 16]).unwrap();
    f.machine
        .cpu_mut()
        .write_mem(
            BUFFER + 256,
            &[0xf800u16, 0x001f]
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    for (i, n) in [1, dib, 0, 0, 2].into_iter().enumerate() {
        f.machine
            .cpu_mut()
            .write_u32(sp - 32 + i as u32 * 4, n)
            .unwrap();
    }
    assert_eq!(f.call(Interface::Display, "BitBlt", [SHELL, 0, 0, 2]), 0);
    assert_eq!(f.machine.screen().get_pixel(0, 0), 0xf800);
    assert_eq!(f.machine.screen().get_pixel(1, 0), 0x001f);
    f.machine.cpu_mut().write_u32(sp - 32, 4).unwrap();
    assert_eq!(
        f.call(Interface::Display, "CreateDIBitmap", [SHELL, BUFFER, 0, 4]),
        20,
        "profundidade zero só é válida para o descritor totalmente vazio"
    );
    f.machine.cpu_mut().write_reg(Reg::Sp, sp);
}

#[test]
fn removing_a_packaged_save_uses_a_persistent_overlay_without_modifying_the_package() {
    let mut f = Fixture::with_overlay(true);
    let original = f.work.join("content/games");
    std::fs::write(&original, b"packaged seed").unwrap();
    f.machine
        .cpu_mut()
        .write_mem(BUFFER + 64, b"games\0")
        .unwrap();
    f.machine
        .cpu_mut()
        .write_mem(BUFFER + 80, b"fs:/~/\0")
        .unwrap();
    f.call(
        Interface::Shell,
        "CreateInstance",
        [SHELL, 0x01001003, BUFFER, 0],
    );
    let mgr = f.machine.cpu().read_u32(BUFFER).unwrap();
    assert_eq!(
        f.call(Interface::FileMgr, "Test", [mgr, BUFFER + 64, 0, 0]),
        0
    );
    assert_eq!(
        f.call(Interface::FileMgr, "Remove", [mgr, BUFFER + 64, 0, 0]),
        0,
        "Jewel Quest precisa apagar o seed antes de regravar"
    );
    assert_ne!(
        f.call(Interface::FileMgr, "Test", [mgr, BUFFER + 64, 0, 0]),
        0
    );
    assert_eq!(
        f.call(Interface::FileMgr, "OpenFile", [mgr, BUFFER + 64, 1, 0]),
        0
    );
    for dirs in [0, 1] {
        f.call(Interface::FileMgr, "EnumInit", [mgr, BUFFER + 80, dirs, 0]);
        assert_eq!(
            f.call(Interface::FileMgr, "EnumNext", [mgr, BUFFER + 512, 0, 0]),
            0,
            "nem arquivo excluído nem metadado do overlay entram na enumeração"
        );
    }
    let mut reopened = zeebx::brew::vfs::Vfs::new(f.work.join("content"));
    reopened.set_save_root(f.work.join("overlay"));
    assert!(
        reopened.resolve("GAMES").is_none(),
        "exclusão persiste em outra sessão e grafia"
    );
    assert_ne!(
        f.call(Interface::FileMgr, "Remove", [mgr, BUFFER + 64, 0, 0]),
        0,
        "excluir duas vezes não confirma arquivo inexistente"
    );
    let file = f.call(Interface::FileMgr, "OpenFile", [mgr, BUFFER + 64, 4, 0]);
    assert_ne!(file, 0);
    f.machine
        .cpu_mut()
        .write_mem(BUFFER + 256, b"new save")
        .unwrap();
    assert_eq!(
        f.call(Interface::File, "Write", [file, BUFFER + 256, 8, 0]),
        8
    );
    f.call(Interface::File, "Release", [file, 0, 0, 0]);
    assert_eq!(
        std::fs::read(reopened.resolve("games").unwrap()).unwrap(),
        b"new save"
    );
    assert_eq!(
        f.call(Interface::FileMgr, "Remove", [mgr, BUFFER + 64, 0, 0]),
        0
    );
    assert!(reopened.resolve("games").is_none());
    assert_eq!(std::fs::read(original).unwrap(), b"packaged seed");
}


#[test]
fn converted_bitmap_exposes_nearby_editable_pixels_and_releases_its_owned_allocation() {
    let mut f = Fixture::new();
    let input = f.call(Interface::Helpers, "malloc", [58, 0, 0, 0]);
    let mut bmp = vec![0u8; 58];
    bmp[..2].copy_from_slice(b"BM");
    for (at, value) in [(2,58u32),(10,54),(14,40),(18,1),(22,1)] {
        bmp[at..at+4].copy_from_slice(&value.to_le_bytes());
    }
    bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
    bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
    bmp[54..58].copy_from_slice(&[0,0,255,0]);
    f.machine.cpu_mut().write_mem(input,&bmp).unwrap();
    let bitmap=f.call(Interface::Helpers,"SetupNativeImage",[0x01004001,input,BUFFER,BUFFER+32]);
    assert_ne!(bitmap,0);
    assert_eq!(f.call(Interface::Bitmap,"QueryInterface",[bitmap,0x01001045,BUFFER+64,0]),0);
    let dib=f.machine.cpu().read_u32(BUFFER+64).unwrap();
    let pixels=f.machine.cpu().read_u32(dib+8).unwrap();
    assert!(pixels.checked_sub(bitmap).is_some_and(|n|n>0&&n<=u16::MAX as u32),
        "Blockbreaker guarda o deslocamento do raster em uint16");
    f.machine.cpu_mut().write_mem(pixels,&0x07e0u16.to_le_bytes()).unwrap();
    f.machine.cpu_mut().marca_sujo(pixels,2);
    f.call(Interface::Bitmap,"Release",[dib,0,0,0]);
    let saved=f.machine.grava_estado();
    f.machine.restaura_estado(&saved).unwrap();
    let sp=f.machine.cpu().read_reg(Reg::Sp);
    f.machine.cpu_mut().write_reg(Reg::Sp,sp-32);
    for (n,value) in [1,bitmap,0,0,2].into_iter().enumerate(){f.machine.cpu_mut().write_u32(sp-32+n as u32*4,value).unwrap();}
    assert_eq!(f.call(Interface::Display,"BitBlt",[SHELL+16,2,3,1]),0);
    assert_eq!(f.machine.screen().get_pixel(2,3),0x07e0);
    f.machine.cpu_mut().write_reg(Reg::Sp,sp);
    assert_eq!(f.call(Interface::Bitmap,"Release",[bitmap,0,0,0]),0);
    let next=f.call(Interface::Shell,"CreateInstance",[SHELL,0x01001003,BUFFER+64,0]);
    assert_eq!(next,0);
    let object=f.machine.cpu().read_u32(BUFFER+64).unwrap();
    assert!((loader::OBJECT_BASE..loader::OBJECT_BASE+loader::OBJECT_SIZE as u32).contains(&object),
        "um bitmap do heap não pode entrar na lista livre de objetos de 64 bytes");
    let state=f.machine.grava_estado();
    let reader=zeebx::save_state::Leitor::abre(&state).unwrap();
    assert!(!reader.pares("heap.live").unwrap().iter().any(|(ptr,_)| *ptr==bitmap));
    assert!(!reader.pares("objects.kinds").unwrap().iter().any(|(ptr,_)| *ptr==bitmap));
    let bitmap=f.call(Interface::Helpers,"SetupNativeImage",[0x01004001,input,BUFFER,BUFFER+32]);
    f.call(Interface::Bitmap,"QueryInterface",[bitmap,0x01001045,BUFFER+64,0]);
    f.call(Interface::Helpers,"sysfree",[bitmap,0,0,0]);
    let state=f.machine.grava_estado();
    let reader=zeebx::save_state::Leitor::abre(&state).unwrap();
    assert!(!reader.pares("heap.live").unwrap().iter().any(|(ptr,_)| *ptr==bitmap));
    assert!(!reader.pares("objects.kinds").unwrap().iter().any(|(ptr,_)| *ptr==bitmap));
    f.call(Interface::Helpers,"free",[input,0,0,0]);
}


#[test]
fn file_read_into_a_published_bitmap_updates_pixels_seen_by_the_display() {
    let mut f=Fixture::new();
    std::fs::write(f.work.join("pixel.bin"),0x07e0u16.to_le_bytes()).unwrap();
    f.machine.cpu_mut().write_mem(BUFFER+128,b"pixel.bin\0").unwrap();
    let destination=f.call(Interface::Display,"GetDestination",[SHELL+16,0,0,0]);
    assert_eq!(f.call(Interface::Bitmap,"CreateCompatibleBitmap",[destination,BUFFER,1,1]),0);
    let bitmap=f.machine.cpu().read_u32(BUFFER).unwrap();
    f.call(Interface::Bitmap,"DrawPixel",[bitmap,0,0,0x001f]);
    f.call(Interface::Bitmap,"GetPixel",[bitmap,0,0,BUFFER+64]);
    let pixels=f.machine.cpu().read_u32(bitmap+8).unwrap();
    assert_eq!(f.call(Interface::Shell,"CreateInstance",[SHELL,0x01001003,BUFFER+64,0]),0);
    let mgr=f.machine.cpu().read_u32(BUFFER+64).unwrap();
    let file=f.call(Interface::FileMgr,"OpenFile",[mgr,BUFFER+128,1,0]);
    assert_ne!(file,0);
    assert_eq!(f.call(Interface::File,"Read",[file,pixels,2,0]),2);
    let sp=f.machine.cpu().read_reg(Reg::Sp);
    f.machine.cpu_mut().write_reg(Reg::Sp,sp-32);
    for(n,value)in[1,bitmap,0,0,2].into_iter().enumerate(){f.machine.cpu_mut().write_u32(sp-32+n as u32*4,value).unwrap();}
    f.call(Interface::Display,"BitBlt",[SHELL+16,0,0,1]);
    assert_eq!(f.machine.screen().get_pixel(0,0),0x07e0,
        "IFile::Read é escrita do guest mesmo quando executada no host");
    f.machine.cpu_mut().write_reg(Reg::Sp,sp);
}
