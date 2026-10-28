use std::path::PathBuf;
use std::time::Duration;
use zeebx::handset::{input::Key, types::DisplayGeometry};
use zeebx::loader::MODULE_BASE;
use zeebx::session::{Session, Step};
use zeebx::storage::StoragePaths;

fn fixture(handler: &[u32]) -> (PathBuf, StoragePaths) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("local-tests")
        .join(format!(
            "session-{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    std::fs::create_dir_all(&root).unwrap();
    let mut image = vec![0u8; 0x300];
    for (offset, words) in [
        (
            0,
            vec![
                0xe59f3008,
                0xe5823000,
                0xe3a00000,
                0xe12fff1e,
                MODULE_BASE + 0x200,
            ],
        ),
        (
            0x40,
            vec![
                0xe59fc008,
                0xe583c000,
                0xe3a00000,
                0xe12fff1e,
                MODULE_BASE + 0x240,
            ],
        ),
        (0x80, handler.to_vec()),
        (0x200, vec![MODULE_BASE + 0x220]),
        (0x228, vec![MODULE_BASE + 0x40]),
        (0x240, vec![MODULE_BASE + 0x260]),
        (0x268, vec![MODULE_BASE + 0x80]),
    ] {
        for (index, word) in words.iter().enumerate() {
            image[offset + index * 4..offset + index * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
    }
    std::fs::write(root.join("test.mod"), image).unwrap();
    let mut mif = vec![0u8; 60];
    mif[0..2].copy_from_slice(&0x11u16.to_le_bytes());
    for (offset, value) in [
        (16, 32u32),
        (20, 1),
        (32, 40),
        (36, 60),
        (40, 0x1234),
        (48, 1),
    ] {
        mif[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    std::fs::write(root.join("test.mif"), mif).unwrap();
    let storage = StoragePaths::from_root(root.join("runtime"));
    storage.create_dirs().unwrap();
    (root, storage)
}

#[test]
fn handset_waiting_for_keys_stays_alive_at_its_own_geometry() {
    let (root, storage) = fixture(&[0xe3a00001, 0xe12fff1e]);
    let original = std::fs::read(root.join("test.mod")).unwrap();
    let mut session = Session::start_handset_with_storage(
        &root.join("test.mod"),
        DisplayGeometry {
            width: 176,
            height: 220,
            depth: 16,
        },
        &storage,
        None,
    )
    .unwrap();
    assert!(session.is_handset());
    assert_eq!(
        (session.screen().width(), session.screen().height()),
        (176, 220)
    );
    for _ in 0..3 {
        assert_ne!(session.step(Duration::from_millis(1), true), Step::Stopped);
    }
    session.set_handset_key(Key::Num5, true).unwrap();
    session.set_handset_key(Key::Num5, false).unwrap();
    for _ in 0..5 {
        assert_ne!(session.step(Duration::from_millis(1), true), Step::Stopped);
    }
    session.release_handset_keys();
    assert_eq!(std::fs::read(root.join("test.mod")).unwrap(), original);
    drop(session);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn touchscreen_input_wakes_an_idle_handset_and_reaches_its_applet() {
    use zeebx::handset::input::{PenEvent,PenKind};
    use zeebx::brew::aee::{self,Interface};
    let mut handler=vec![0xe92d4010,0xe24dd008,0xe3a04000,0xe58d4000];
    let mut branches=Vec::new();
    for (compare,value) in [(0xe151000c,0x504),(0xe153000c,0x019000f0)] {
        handler.extend([0xe59fc000,0xea000000,value,compare,0x1a000000]);
        branches.push(handler.len()-1);
    }
    handler.extend([0xe3a01000,0xe3e020ff,0xe1a03002]);
    for method in ["DrawRect","Update"] {
        let slot=(0..256).find(|slot|Interface::Display.method(*slot)==Some(method)).unwrap();
        handler.extend([0xe59fc000,0xea000000,aee::encode(Interface::Display,slot),0xe12fff3c]);
    }
    handler.extend([0xe3a00001,0xea000000]);
    // Recusar POINTER/qualquer entrada não tratada pede o fallback PEN ao shell.
    for index in branches { handler[index] |= (handler.len()-index-2) as u32; }
    handler.extend([0xe3a00000,0xe28dd008,0xe8bd8010]);
    let (root,storage)=fixture(&handler);
    let mut session=Session::start_handset_with_storage(&root.join("test.mod"),
        DisplayGeometry{width:800,height:480,depth:16},&storage,None).unwrap();
    for _ in 0..3 { assert_ne!(session.step(Duration::from_millis(1),true),Step::Stopped); }
    assert!(session.screen().pixels().iter().all(|pixel|*pixel==0));
    session.set_handset_pen(PenEvent{kind:PenKind::Down,x:400,y:240}).unwrap();
    assert_eq!(session.next_guest_work_delay(),Some(Duration::ZERO));
    assert_eq!(session.step(Duration::from_millis(8),true),Step::Presented);
    assert!(session.screen().pixels().iter().all(|pixel|*pixel==0xffff));
    drop(session);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn handset_launches_with_a_nested_shared_mif_named_after_its_folder() {
    let (root,storage)=fixture(&[0xe3a00001,0xe12fff1e]);
    let module=root.join("games/Mod/more games/39367/main.mod");
    let mif=root.join("games/Mif/more mifs/39367.mif");
    std::fs::create_dir_all(module.parent().unwrap()).unwrap();
    std::fs::create_dir_all(mif.parent().unwrap()).unwrap();
    std::fs::rename(root.join("test.mod"),&module).unwrap();
    std::fs::rename(root.join("test.mif"),&mif).unwrap();
    let geometry=DisplayGeometry{width:176,height:220,depth:16};
    let mut session=Session::start_handset_with_storage(&module,geometry,&storage,None).unwrap();
    assert!(session.is_handset());
    assert_eq!((session.screen().width(),session.screen().height()),(176,220));
    assert_ne!(session.step(Duration::from_millis(1),true),Step::Stopped);
    assert_eq!(zeebx::library::applet_clsid(&module),Some(0x1234));
    assert!(!module.with_extension("mif").exists());
    drop(session);std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn empty_display_update_and_idle_repaints_do_not_create_visible_frames() {
    use zeebx::brew::aee::{self, Interface};
    let slot = (0..256)
        .find(|slot| Interface::Display.method(*slot) == Some("Update"))
        .unwrap();
    let (root, storage) = fixture(&[
        0xe92d4010,
        0xe59f400c,
        0xe12fff34,
        0xe3a00001,
        0xe8bd8010,
        0,
        aee::encode(Interface::Display, slot),
    ]);
    let mut session = Session::start_handset_with_storage(
        &root.join("test.mod"),
        DisplayGeometry {
            width: 176,
            height: 220,
            depth: 16,
        },
        &storage,
        None,
    )
    .unwrap();
    let original_series = session.screen().serie();
    session.step(Duration::from_millis(1), true);
    assert_eq!(
        session.screen().serie(),
        original_series,
        "an Update of the initial black image creates no visual change"
    );
    let first = (session.screen().serie(), session.screen().escritas());
    session.step(Duration::from_millis(1), true);
    assert_eq!(
        (session.screen().serie(), session.screen().escritas()),
        first
    );
    drop(session);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn identical_completed_updates_keep_one_visible_revision_and_changed_images_advance_it() {
    use zeebx::brew::aee::{self, Interface};
    let mut handler = vec![0xe92d4010, 0xe24dd008, 0xe3a04000, 0xe58d4000];
    let call = |handler: &mut Vec<u32>, method: &str| {
        let slot = (0..256)
            .find(|slot| Interface::Display.method(*slot) == Some(method))
            .unwrap();
        handler.extend([0xe59fc000, 0xea000000, aee::encode(Interface::Display, slot), 0xe12fff3c]);
    };
    // Branco, branco repetido, preto: exercita Update real e a fila da sessão.
    handler.extend([0xe3a01000, 0xe3e020ff, 0xe1a03002]);
    call(&mut handler, "DrawRect");
    call(&mut handler, "Update");
    call(&mut handler, "Update");
    handler.extend([0xe3a01000, 0xe3a02000, 0xe3a03000]);
    call(&mut handler, "DrawRect");
    call(&mut handler, "Update");
    handler.extend([0xe3a00001, 0xe28dd008, 0xe8bd8010]);
    let (root, storage) = fixture(&handler);
    let mut session = Session::start_handset_with_storage(
        &root.join("test.mod"),
        DisplayGeometry { width: 176, height: 220, depth: 16 },
        &storage, None,
    ).unwrap();
    assert_eq!(session.step(Duration::from_millis(1), true), Step::Presented);
    assert!(session.screen().pixels().iter().all(|pixel| *pixel == 0xffff));
    let first = (session.screen().serie(), session.screen().escritas());
    assert_eq!(session.step(Duration::from_millis(1), true), Step::Presented);
    assert_eq!(
        (session.screen().serie(), session.screen().escritas()), first,
        "an identical Update must not inflate visible FPS or upload the same image again",
    );
    assert_eq!(session.step(Duration::from_millis(1), true), Step::Presented);
    assert!(session.screen().pixels().iter().all(|pixel| *pixel == 0));
    assert_ne!((session.screen().serie(), session.screen().escritas()), first);
    drop(session);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn gles_swap_replaces_the_handset_snapshot_including_an_earlier_2d_update() {
    use zeebx::brew::aee::{self, Interface};
    for (update_before_swap, update_after_swap) in [(false, false), (true, false), (true, true)] {
        let mut handler = vec![0xe92d4010, 0xe24dd008, 0xe3a04000, 0xe58d4000];
        let call = |handler: &mut Vec<u32>, interface: Interface, method: &str| {
            let slot = (0..256).find(|slot| interface.method(*slot) == Some(method)).unwrap();
            handler.extend([0xe59fc000, 0xea000000, aee::encode(interface, slot), 0xe12fff3c]);
        };
        if update_before_swap {
            // Uma tela branca de abertura fica obsoleta quando o GL apresenta o verde.
            handler.extend([0xe3a01000, 0xe3e020ff, 0xe1a03002]);
            call(&mut handler, Interface::Display, "DrawRect");
            call(&mut handler, Interface::Display, "Update");
        }
        handler.extend([0xe3a00000, 0xe3a01801, 0xe3a02000, 0xe1a03001]);
        call(&mut handler, Interface::GlLegacy, "glClearColorx");
        handler.push(0xe3a00901); // GL_COLOR_BUFFER_BIT = 0x4000.
        call(&mut handler, Interface::GlLegacy, "glClear");
        handler.extend([0xe3a00001, 0xe3a01001]);
        call(&mut handler, Interface::EglLegacy, "eglSwapBuffers");
        if update_after_swap {
            handler.extend([0xe3a01000, 0xe3e020ff, 0xe1a03002]);
            call(&mut handler, Interface::Display, "DrawRect");
            call(&mut handler, Interface::Display, "Update");
        }
        handler.extend([0xe3a00001, 0xe28dd008, 0xe8bd8010]);
        let (root, storage) = fixture(&handler);
        let mut session = Session::start_handset_with_storage(
            &root.join("test.mod"),
            DisplayGeometry { width: 176, height: 220, depth: 16 },
            &storage, None,
        ).unwrap();
        assert_eq!(session.step(Duration::from_millis(8), true), Step::Presented);
        assert_eq!(session.rendered_gl_frames(), 1);
        let expected = if update_after_swap { 0xffff } else { 0x07e0 };
        assert!(session.screen().pixels().iter().all(|pixel| *pixel == expected),
            "só o Update posterior pode cobrir o verde (antes={update_before_swap}, depois={update_after_swap})");
        session.step(Duration::from_millis(1), true);
        assert!(session.screen().pixels().iter().all(|pixel| *pixel == expected),
            "o Update anterior ao swap não pode reaparecer no repaint seguinte");
        drop(session);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn zeebo_avk_loading_handler_survives_budget_and_finishes_before_next_key() {
    use zeebx::brew::aee::{self,Interface};
    let mut handler=vec![0xe92d4010,0xe24dd008,0xe3a04000,0xe58d4000];let mut branches=Vec::new();
    for (compare,value) in [(0xe151000c,0x100),(0xe152000c,0xe064)] {
        handler.extend([0xe59fc000,0xea000000,value,compare,0x1a000000]);branches.push(handler.len()-1);
    }
    // 2^24 iterações ultrapassam o orçamento antigo do despacho síncrono (10M).
    handler.extend([0xe3a04401,0xe2544001,0x1afffffd,0xe3a01000,0xe3e020ff,0xe1a03002]);
    for method in ["DrawRect","Update"] {let slot=(0..256).find(|slot|Interface::Display.method(*slot)==Some(method)).unwrap();handler.extend([0xe59fc000,0xea000000,aee::encode(Interface::Display,slot),0xe12fff3c]);}
    for index in branches {handler[index]|=(handler.len()-index-2) as u32;}
    handler.extend([0xe3a00001,0xe28dd008,0xe8bd8010]);
    let (root,storage)=fixture(&handler);let mut session=Session::start_handset_with_storage(&root.join("test.mod"),DisplayGeometry{width:640,height:480,depth:16},&storage,None).unwrap();
    for _ in 0..3{assert_ne!(session.step(Duration::from_millis(1),false),Step::Stopped);}
    assert!(session.screen().pixels().iter().all(|p|*p==0));session.set_key(0xe064,true);session.set_key(0xe064,false);
    for _ in 0..200{assert_ne!(session.step(Duration::from_millis(1),false),Step::Stopped);if session.screen().pixels().iter().all(|p|*p==0xffff){break;}}
    assert!(session.screen().pixels().iter().all(|p|*p==0xffff),"Zeebo handler was abandoned before drawing after its long load");drop(session);std::fs::remove_dir_all(root).unwrap();
}
