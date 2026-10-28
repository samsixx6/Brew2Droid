use std::path::PathBuf;

use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend};
use zeebx::handset::types::DisplayGeometry;
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

const APPLET: u32 = loader::HEAP_BASE + 0x10000;
const APPLET_VTABLE: u32 = APPLET + 0x100;
const INFO: u32 = loader::STACK_BASE + 0x200;
const OUT: u32 = INFO + 0x100;

fn scratch() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    let path = workspace.join("local-tests").join(format!(
        "display-test-{}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    assert!(path.canonicalize().unwrap().starts_with(workspace));
    path
}

fn loaded() -> loader::LoadedModule {
    let words = [
        0xe3a0_0000u32,
        0xe12f_ff1e, // AEEMod_Load retorna0.
        0xe593_1010,
        0xe580_1004,
        0xe3a0_0001,
        0xe12f_ff1e,
    ];
    let image = ModImage::parse(words.into_iter().flat_map(u32::to_le_bytes).collect()).unwrap();
    let mut loaded = loader::load_with(&image, &[]).unwrap();
    loaded.mem.write_u32(APPLET, APPLET_VTABLE).unwrap();
    loaded
        .mem
        .write_u32(APPLET_VTABLE + 8, loader::MODULE_BASE + 8)
        .unwrap();
    loaded
}

fn call(
    machine: &mut Machine<BackendPadrao>,
    interface: Interface,
    name: &str,
    args: [u32; 4],
) -> u32 {
    let slot = (0..256)
        .find(|slot| interface.method(*slot) == Some(name))
        .unwrap();
    match machine
        .call_guest(aee::encode(interface, slot), args, 1000)
        .unwrap()
    {
        Outcome::Returned { code } => code,
        other => panic!("{name}: {other:?}"),
    }
}

#[test]
fn moldura_box_desenha_recortada_e_devolve_o_retangulo_interno() {
    let work = scratch();
    let storage = StoragePaths::from_root(work.join("runtime"));
    storage.create_dirs().unwrap();
    let mut machine = Machine::new_with_storage_geometry(
        BackendPadrao::new().unwrap(), loaded(), &work, &storage, None,
        DisplayGeometry { width: 12, height: 12, depth: 16 },
    ).unwrap();
    machine.run(1000).unwrap();
    let display = loader::OBJECT_BASE + 0x10;
    call(&mut machine, Interface::Display, "SetColor", [display, 3, 0x0000ff00, 0]);
    let rect = |x: i16, y: i16, w: i16, h: i16| {
        [x, y, w, h].into_iter().flat_map(i16::to_le_bytes).collect::<Vec<_>>()
    };
    machine.cpu_mut().write_mem(INFO, &rect(2, 2, 8, 8)).unwrap();
    assert_eq!(call(&mut machine, Interface::Display, "DrawFrame", [display, INFO, 5, 0xff000000]), 1);
    let mut interior = [0u8; 8];
    machine.cpu().read_mem(INFO, &mut interior).unwrap();
    assert_eq!(interior.as_slice(), rect(3, 3, 6, 6));
    assert_eq!(machine.screen().get_pixel(2, 2), 0xf800);
    assert_eq!(machine.screen().get_pixel(9, 9), 0xf800);
    assert_eq!(machine.screen().get_pixel(3, 3), 0x001f);

    // RGB_NONE conserva o preenchimento. O recorte não pode criar outra borda
    // dentro da moldura; o retângulo devolvido depende do pedido original.
    call(&mut machine, Interface::Display, "SetColor", [display, 3, 0x00ff0000, 0]);
    machine.cpu_mut().write_mem(OUT, &rect(4, 4, 6, 6)).unwrap();
    call(&mut machine, Interface::Display, "SetClipRect", [display, OUT, 0, 0]);
    machine.cpu_mut().write_mem(INFO, &rect(2, 2, 8, 8)).unwrap();
    assert_eq!(call(&mut machine, Interface::Display, "DrawFrame", [display, INFO, 5, 0xffffffff]), 1);
    assert_eq!(machine.screen().get_pixel(4, 4), 0x001f);
    assert_eq!(machine.screen().get_pixel(9, 9), 0x07e0);
    assert_eq!(machine.screen().get_pixel(2, 2), 0xf800);
    machine.cpu().read_mem(INFO, &mut interior).unwrap();
    assert_eq!(interior.as_slice(), rect(3, 3, 6, 6));
    assert_eq!(call(&mut machine, Interface::Display, "DrawFrame", [display, 0, 5, 0]), 1);
    assert_eq!(call(&mut machine, Interface::Display, "DrawFrame", [display, 0, 2, 0]), 2);
    assert_eq!(call(&mut machine, Interface::Display, "DrawFrame", [display, 0, 0, 0]), 0);
    drop(machine);
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn device_bitmap_exposes_live_pixels_before_any_dib_query() {
    for (width,height) in [(176,220),(240,320),(241,321)] {
        let work=scratch();
        let storage=StoragePaths::from_root(work.join("runtime"));
        storage.create_dirs().unwrap();
        let mut machine=Machine::new_with_storage_geometry(BackendPadrao::new().unwrap(),loaded(),
            &work,&storage,None,DisplayGeometry{width,height,depth:16}).unwrap();
        machine.run(1000).unwrap();
        assert_eq!(call(&mut machine,Interface::Display,"GetDeviceBitmap",
            [loader::OBJECT_BASE+0x10,OUT,0,0]),0);
        let bitmap=machine.cpu().read_u32(OUT).unwrap();
        call(&mut machine,Interface::Bitmap,"Release",[bitmap,0,0,0]);
        // Asphalt 3 lê pBmp imediatamente após GetDeviceBitmap/Release e guarda esse
        // endereço antes de pedir QueryInterface. Publicá-lo só depois conserva o nulo.
        let pixels=machine.cpu().read_u32(bitmap+8).unwrap();
        assert_ne!(pixels,0,"o bitmap da tela já deve expor seus pixels");
        assert_eq!(machine.cpu().read_u32(bitmap+20).unwrap(),u32::from(width)|(u32::from(height)<<16));
        let pitch=(u32::from(width)*2).div_ceil(4)*4;
        let last=pixels+pitch*(u32::from(height)-1)+2*(u32::from(width)-1);
        // Escrever pelo ARM também exercita a vigilância de páginas; write_mem do host
        // prepara memória e deliberadamente não se apresenta como uma escrita do guest.
        let writer=loader::MODULE_BASE+0x80;
        machine.cpu_mut().write_mem(writer,&[0xe1c010b0u32.to_le_bytes(),0xe12fff1eu32.to_le_bytes()].concat()).unwrap();
        machine.call_guest(writer,[last,0x07e0,0,0],1000).unwrap();
        call(&mut machine,Interface::Display,"GetDeviceBitmap",[loader::OBJECT_BASE+0x10,OUT,0,0]);
        assert_eq!(machine.cpu().read_u32(bitmap+8).unwrap(),pixels);
        let mut color=[0u8;2];
        machine.cpu().read_mem(last,&mut color).unwrap();
        assert_eq!(u16::from_le_bytes(color),0x07e0,"consultar o bitmap não apaga a escrita direta");
        call(&mut machine,Interface::Display,"Update",[loader::OBJECT_BASE+0x10,0,0,0]);
        assert_eq!(machine.screen().get_pixel(i32::from(width)-1,i32::from(height)-1),0x07e0);
        drop(machine);
        std::fs::remove_dir_all(work).unwrap();
    }
}

#[test]
fn phone_geometry_reaches_device_info_app_start_and_direct_bitmap_pixels() {
    for (width, height) in [(240, 320), (241, 321)] {
        let work = scratch();
        let storage = StoragePaths::from_root(work.join("runtime"));
        storage.create_dirs().unwrap();
        let geometry = DisplayGeometry {
            width,
            height,
            depth: 16,
        };
        let mut machine = Machine::new_with_storage_geometry(
            BackendPadrao::new().unwrap(),
            loaded(),
            &work,
            &storage,
            None,
            geometry,
        )
        .unwrap();
        assert_eq!(machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
        let packed = u32::from(width) | (u32::from(height) << 16);
        call(
            &mut machine,
            Interface::Shell,
            "GetDeviceInfo",
            [loader::OBJECT_BASE, INFO, 0, 0],
        );
        assert_eq!(machine.cpu().read_u32(INFO).unwrap(), packed);
        let bitmap = call(
            &mut machine,
            Interface::Display,
            "GetDestination",
            [loader::OBJECT_BASE + 0x10, 0, 0, 0],
        );
        assert_ne!(bitmap, 0);
        assert_eq!(machine.screen().width(), u32::from(width));
        assert_eq!(machine.screen().height(), u32::from(height));
        // GetDestination transfere a tela ao bitmap. O tamanho continua o do aparelho.
        call(
            &mut machine,
            Interface::Shell,
            "GetDeviceInfo",
            [loader::OBJECT_BASE, INFO, 0, 0],
        );
        assert_eq!(machine.cpu().read_u32(INFO).unwrap(), packed);
        assert_eq!(
            call(
                &mut machine,
                Interface::Bitmap,
                "QueryInterface",
                [bitmap, 0x01001045, OUT, 0]
            ),
            0
        );
        let dib = machine.cpu().read_u32(OUT).unwrap();
        assert_eq!(machine.cpu().read_u32(dib + 20).unwrap(), packed);
        assert_eq!(
            machine.cpu().read_u32(dib + 24).unwrap() & 0xffff,
            (u32::from(width) * 2).div_ceil(4) * 4
        );
        assert_eq!(machine.cpu().read_u32(dib + 28).unwrap() & 0xff, 16);
        assert_eq!(
            machine.start_applet(APPLET, 0x12345678, 1000).unwrap(),
            Outcome::Returned { code: 1 }
        );
        assert_eq!(machine.cpu().read_u32(APPLET + 4).unwrap(), packed);
        drop(machine);
        std::fs::remove_dir_all(work).unwrap();
    }
}

#[test]
fn existing_frontends_keep_their_default_geometry() {
    let work = scratch();
    let storage = StoragePaths::from_root(work.join("runtime"));
    storage.create_dirs().unwrap();
    let mut machine = Machine::new_with_storage(
        BackendPadrao::new().unwrap(),
        loaded(),
        &work,
        &storage,
        None,
    );
    machine.run(1000).unwrap();
    call(
        &mut machine,
        Interface::Shell,
        "GetDeviceInfo",
        [loader::OBJECT_BASE, INFO, 0, 0],
    );
    assert_eq!(machine.cpu().read_u32(INFO).unwrap(), 640 | (480 << 16));
    assert_eq!(
        (machine.screen().width(), machine.screen().height()),
        (640, 480)
    );
    drop(machine);
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn undercover_fullscreen_preferences_use_the_byte_count_and_preserve_the_game_display() {
    let work = scratch();
    let storage = StoragePaths::from_root(work.join("runtime"));
    storage.create_dirs().unwrap();
    let mut machine = Machine::new_with_storage_geometry(
        BackendPadrao::new().unwrap(), loaded(), &work, &storage, None,
        DisplayGeometry { width: 800, height: 480, depth: 16 },
    ).unwrap();
    assert_eq!(machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
    // O Undercover passa três bytes de "a:0", sem incluir o terminador. O resto
    // do buffer não pertence ao pedido e não pode virar configuração de vídeo.
    machine.cpu_mut().write_mem(INFO, b"a:0w:120,h:130").unwrap();
    let revision = (machine.screen().serie(), machine.screen().escritas());
    assert_eq!(call(&mut machine, Interface::Display, "SetPrefs",
        [loader::OBJECT_BASE + 0x10, INFO, 3, 0]), 0);
    assert_eq!((machine.screen().width(), machine.screen().height()), (800, 480));
    assert_eq!((machine.screen().serie(), machine.screen().escritas()), revision);
    call(&mut machine, Interface::Shell, "GetDeviceInfo", [loader::OBJECT_BASE, OUT, 0, 0]);
    assert_eq!(machine.cpu().read_u32(OUT).unwrap(), 800 | (480 << 16));
    // Preferências não suportadas são erros BREW normais, não APIs inexistentes
    // nem promessas de uma geometria/rotação que o renderizador não aplicou.
    for (bytes, length, expected) in [
        (b"".as_slice(), 0, 0),
        (b"w:120,h:130".as_slice(), 11, 20),
        (b"a:1".as_slice(), 3, 20),
        (b"a:0".as_slice(), u32::MAX, 2),
        (b"a:0".as_slice(), 1_000_000_000, 2),
    ] {
        machine.cpu_mut().write_mem(INFO, bytes).unwrap();
        assert_eq!(call(&mut machine, Interface::Display, "SetPrefs",
            [loader::OBJECT_BASE + 0x10, if length == 0 { 0 } else { INFO }, length, 0]), expected);
        assert_eq!((machine.screen().width(), machine.screen().height()), (800, 480));
    }
    assert_eq!(call(&mut machine, Interface::Display, "SetPrefs",
        [loader::OBJECT_BASE + 0x10, 0, 3, 0]), 2);
    drop(machine);
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn unsupported_geometry_is_rejected_before_entering_guest_code() {
    let work = scratch();
    let storage = StoragePaths::from_root(work.join("runtime"));
    for geometry in [
        DisplayGeometry {
            width: 0,
            height: 320,
            depth: 16,
        },
        DisplayGeometry {
            width: 240,
            height: 320,
            depth: 32,
        },
        DisplayGeometry {
            width: 65535,
            height: 65535,
            depth: 16,
        },
    ] {
        assert!(
            Machine::new_with_storage_geometry(
                BackendPadrao::new().unwrap(),
                loaded(),
                &work,
                &storage,
                None,
                geometry
            )
            .is_err()
        );
    }
    assert!(!work.join("runtime").exists());
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn rendered_gl_generation_survives_skipped_swaps_and_later_skip_decisions() {
    let work = scratch();
    let storage = StoragePaths::from_root(work.join("runtime"));
    storage.create_dirs().unwrap();
    let mut machine = Machine::new_with_storage(
        BackendPadrao::new().unwrap(),
        loaded(),
        &work,
        &storage,
        None,
    );
    machine.run(1000).unwrap();
    let mut generations = Vec::new();
    for skipped in [false, true, true, false] {
        machine.define_pula_desenho(skipped);
        call(
            &mut machine,
            Interface::EglLegacy,
            "eglSwapBuffers",
            [1, 1, 0, 0],
        );
        // A decisão da fatia seguinte não pode reclassificar o quadro já entregue.
        machine.define_pula_desenho(!skipped);
        generations.push(machine.rendered_gl_frames());
    }
    assert_eq!(
        machine.gl_swaps(),
        4,
        "o diagnóstico guest continua contando todos"
    );
    assert_eq!(generations, [1, 1, 1, 2]);
    drop(machine);
    std::fs::remove_dir_all(work).unwrap();
}
