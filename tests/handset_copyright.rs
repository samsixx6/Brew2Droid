use std::path::PathBuf;

use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend};
use zeebx::handset::input::{Key, KeyEvent, KeyKind};
use zeebx::handset::types::DisplayGeometry;
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::save_state::{self, Leitor};
use zeebx::storage::StoragePaths;

const APPLET: u32 = loader::HEAP_BASE + 0x20000;

fn fixture() -> (Machine<BackendPadrao>, PathBuf) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("local-tests")
        .join(format!(
            "copyright-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
    std::fs::create_dir_all(&root).unwrap();
    let storage = StoragePaths::from_root(root.join("runtime"));
    storage.create_dirs().unwrap();
    let image = ModImage::parse(
        [0xe3a0_0000u32, 0xe12f_ff1e]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect(),
    )
    .unwrap();
    let mut machine = Machine::new_with_storage_geometry(
        BackendPadrao::new().unwrap(),
        loader::load_with(&image, &[]).unwrap(),
        &root,
        &storage,
        None,
        DisplayGeometry {
            width: 160,
            height: 200,
            depth: 16,
        },
    )
    .unwrap();
    machine.enable_handset_clock();
    assert_eq!(machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
    let malloc = (0..256)
        .find(|slot| Interface::Helpers.method(*slot) == Some("malloc"))
        .unwrap();
    assert!(
        matches!(machine.call_guest(aee::encode(Interface::Helpers, malloc),
        [0x20200, 0, 0, 0], 1000).unwrap(), Outcome::Returned { code } if code != 0)
    );
    let handler = loader::MODULE_BASE + 0x20;
    let instructions = [
        0xe5801004u32,
        0xe5802008,
        0xe580300c,
        0xe5901010,
        0xe2811001,
        0xe5801010,
        0xe3a00001,
        0xe12fff1e,
    ];
    machine
        .cpu_mut()
        .write_mem(
            handler,
            &instructions
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    machine.cpu_mut().write_u32(APPLET, APPLET + 0x100).unwrap();
    machine
        .cpu_mut()
        .write_u32(APPLET + 0x108, handler)
        .unwrap();
    assert_eq!(
        machine.start_applet(APPLET, 0x01017777, 1000).unwrap(),
        Outcome::Returned { code: 1 }
    );
    machine.cpu_mut().write_mem(APPLET + 4, &[0; 16]).unwrap();
    assert!(machine.pinta_tela_rgb565(&0x1234u16.to_le_bytes().repeat(160 * 200)));
    (machine, root)
}

fn prompt(machine: &mut Machine<BackendPadrao>, pointer: u32) -> u32 {
    let slot = (0..256)
        .find(|slot| Interface::Shell.method(*slot) == Some("Prompt"))
        .unwrap();
    match machine
        .call_guest(
            aee::encode(Interface::Shell, slot),
            [loader::OBJECT_BASE, pointer, 0, 0],
            10000,
        )
        .unwrap()
    {
        Outcome::Returned { code } => code,
        other => panic!("Prompt: {other:?}"),
    }
}

fn key(machine: &mut Machine<BackendPadrao>, key: Key, kind: KeyKind) {
    assert!(matches!(
        machine
            .deliver_handset_event(
                KeyEvent {
                    key,
                    kind,
                    timestamp_us: 0
                },
                10000
            )
            .unwrap(),
        Outcome::Returned { .. }
    ));
}

fn count(machine: &Machine<BackendPadrao>) -> u32 {
    machine.cpu().read_u32(APPLET + 16).unwrap()
}

fn cleanup(machine: Machine<BackendPadrao>, root: PathBuf) {
    drop(machine);
    assert!(
        root.canonicalize().unwrap().starts_with(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .canonicalize()
                .unwrap()
        )
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn copyright_is_visible_and_waits_for_clear_without_autodismissal() {
    let (mut machine, root) = fixture();
    let before = machine.screen().pixels().to_vec();
    assert_eq!(
        prompt(&mut machine, 0),
        1,
        "ShowCopyright deve criar um diálogo real"
    );
    assert_ne!(machine.screen().pixels(), before);
    assert!(machine.drawn_text().any(|row| row.3.contains("Copyright")));
    assert_eq!(
        count(&machine),
        0,
        "criar diálogo não fabrica EVT_COPYRIGHT_END"
    );
    for kind in [KeyKind::Press, KeyKind::Key, KeyKind::Release] {
        key(&mut machine, Key::Select, kind);
    }
    assert_eq!(
        count(&machine),
        0,
        "Select não é CLR e a entrada pertence ao diálogo"
    );
    cleanup(machine, root);
}

#[test]
fn clear_restores_the_frame_and_emits_copyright_end_once_without_key_leaks() {
    let (mut machine, root) = fixture();
    let before = machine.screen().pixels().to_vec();
    assert_eq!(prompt(&mut machine, 0), 1);
    key(&mut machine, Key::Select, KeyKind::Press);
    key(&mut machine, Key::Clear, KeyKind::Press);
    assert_eq!(count(&machine), 0);
    key(&mut machine, Key::Clear, KeyKind::Key);
    assert_eq!(count(&machine), 1);
    assert_eq!(machine.cpu().read_u32(APPLET + 4).unwrap(), 0x303);
    assert_eq!(machine.cpu().read_u32(APPLET + 8).unwrap(), 0);
    assert_eq!(machine.cpu().read_u32(APPLET + 12).unwrap(), 0);
    assert_eq!(
        machine.screen().pixels(),
        before,
        "fechar conserva o quadro anterior"
    );
    key(&mut machine, Key::Clear, KeyKind::Key);
    key(&mut machine, Key::Clear, KeyKind::Release);
    key(&mut machine, Key::Select, KeyKind::Release);
    assert_eq!(
        count(&machine),
        1,
        "repeat e solturas da entrada modal não chegam ao applet"
    );
    key(&mut machine, Key::Select, KeyKind::Press);
    assert_eq!(count(&machine), 2, "uma tecla nova volta ao applet");
    cleanup(machine, root);
}

#[test]
fn copyright_modal_and_held_keys_roundtrip_and_old_states_have_no_modal() {
    let (mut machine, root) = fixture();
    assert_eq!(prompt(&mut machine, 0), 1);
    key(&mut machine, Key::Select, KeyKind::Press);
    let saved = machine.grava_estado();
    key(&mut machine, Key::Clear, KeyKind::Key);
    machine.restaura_estado(&saved).unwrap();
    assert_eq!(count(&machine), 0);
    key(&mut machine, Key::Select, KeyKind::Key);
    assert_eq!(count(&machine), 0);
    key(&mut machine, Key::Clear, KeyKind::Key);
    key(&mut machine, Key::Clear, KeyKind::Release);
    key(&mut machine, Key::Select, KeyKind::Release);
    assert_eq!(count(&machine), 1);
    let reader = Leitor::abre(&saved).unwrap();
    assert!(reader.secao("tab.copyright").is_some());
    let old = save_state::escreve(
        &reader
            .nomes()
            .into_iter()
            .filter(|name| *name != "tab.copyright")
            .map(|name| (name.to_owned(), reader.secao(name).unwrap().to_vec()))
            .collect::<Vec<_>>(),
    );
    machine.restaura_estado(&old).unwrap();
    key(&mut machine, Key::Select, KeyKind::Press);
    assert_eq!(
        count(&machine),
        1,
        "estado antigo sem seção não fica preso num modal"
    );
    cleanup(machine, root);
}

#[test]
fn unsupported_general_prompt_stays_a_visible_failure() {
    let (mut machine, root) = fixture();
    let before = machine.screen().pixels().to_vec();
    assert_eq!(prompt(&mut machine, loader::STACK_BASE + 0x800), 0);
    assert_eq!(machine.screen().pixels(), before);
    cleanup(machine, root);
}

#[test]
fn copyright_displays_the_selected_mifs_name_company_copyright_and_version() {
    let (mut machine, root) = fixture();
    let texts = ["Chess", "Penultimate Inc.", "Copyright 2002,", "1.0"];
    let mut sections = texts
        .into_iter()
        .map(|text| {
            let mut section = vec![3];
            section.extend_from_slice(text.as_bytes());
            section
        })
        .collect::<Vec<_>>();
    let mut applet = vec![0; 20];
    applet[..4].copy_from_slice(&0x01017777u32.to_le_bytes());
    applet[8..12].copy_from_slice(&1000u32.to_le_bytes());
    sections.push(applet);
    let mut mif = vec![0; 80];
    mif[..2].copy_from_slice(&0x11u16.to_le_bytes());
    mif[2..4].copy_from_slice(&1u16.to_le_bytes());
    mif[6..8].copy_from_slice(&3u16.to_le_bytes());
    for (offset, value) in [(8, 32u32), (12, 24), (16, 56), (20, 5), (24, 80)] {
        mif[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    for (index, fields) in [[1u16, 1000, 0, 0], [1, 6, 2, 1], [0x5000, 2, 0, 4]]
        .into_iter()
        .enumerate()
    {
        mif[32 + index * 8..40 + index * 8].copy_from_slice(
            &fields
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        );
    }
    for (index, section) in sections.into_iter().enumerate() {
        let offset = mif.len() as u32;
        mif[56 + index * 4..60 + index * 4].copy_from_slice(&offset.to_le_bytes());
        mif.extend_from_slice(&section);
    }
    let end = mif.len() as u32;
    mif[76..80].copy_from_slice(&end.to_le_bytes());
    assert!(machine.set_applet_mif(&mif, 0x01017777));
    assert_eq!(prompt(&mut machine, 0), 1);
    for text in texts {
        assert!(
            machine.drawn_text().any(|row| row.3 == text),
            "metadado MIF ausente: {text}"
        );
    }
    cleanup(machine, root);
}

#[test]
fn modal_open_and_close_publish_frames_after_a_previous_display_update() {
    let (mut machine, root) = fixture();
    let update = (0..256)
        .find(|slot| Interface::Display.method(*slot) == Some("Update"))
        .unwrap();
    assert_eq!(
        machine
            .call_guest(
                aee::encode(Interface::Display, update),
                [loader::OBJECT_BASE + 16, 0, 0, 0],
                10000
            )
            .unwrap(),
        Outcome::Returned { code: 0 }
    );
    let previous = machine.toma_quadro_do_update().unwrap();
    assert!(machine.toma_quadro_do_update().is_none());
    assert_eq!(prompt(&mut machine, 0), 1);
    let visible = machine
        .toma_quadro_do_update()
        .expect("a Session precisa receber o quadro do modal");
    assert_ne!(visible.pixels(), previous.pixels());
    key(&mut machine, Key::Clear, KeyKind::Key);
    let restored = machine
        .toma_quadro_do_update()
        .expect("fechar publica o quadro antes de EVT_COPYRIGHT_END");
    assert_eq!(restored.pixels(), previous.pixels());
    assert_eq!(count(&machine), 1);
    cleanup(machine, root);
}
