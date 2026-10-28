use std::path::PathBuf;

use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend};
use zeebx::handset::types::DisplayGeometry;
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

const RECT: u32 = loader::STACK_BASE + 0x200;
const OUT: u32 = RECT + 0x40;

fn machine() -> (Machine<BackendPadrao>, PathBuf, u32) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let work = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local-tests").join(format!(
        "round-rectangle-test-{}-{}",
        std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    ));
    std::fs::create_dir_all(&work).unwrap();
    let storage = StoragePaths::from_root(work.join("runtime"));
    storage.create_dirs().unwrap();
    let image = ModImage::parse([0xe3a0_0000u32, 0xe12f_ff1e].into_iter()
        .flat_map(u32::to_le_bytes).collect()).unwrap();
    let loaded = loader::load_with(&image, &[]).unwrap();
    let mut machine = Machine::new_with_storage_geometry(
        BackendPadrao::new().unwrap(), loaded, &work, &storage, None,
        DisplayGeometry { width: 12, height: 12, depth: 16 },
    ).unwrap();
    assert_eq!(machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
    assert_eq!(call(&mut machine, Interface::Shell, "CreateInstance",
        [loader::OBJECT_BASE, 0x0100_2001, OUT, 0]), Outcome::Returned { code: 0 });
    let graphics = machine.cpu().read_u32(OUT).unwrap();
    assert_ne!(graphics, 0);
    (machine, work, graphics)
}

fn call(machine: &mut Machine<BackendPadrao>, interface: Interface, name: &str, args: [u32; 4]) -> Outcome {
    let slot = (0..256).find(|slot| interface.method(*slot) == Some(name)).unwrap();
    machine.call_guest(aee::encode(interface, slot), args, 1000).unwrap()
}

fn graphics(machine: &mut Machine<BackendPadrao>, object: u32, name: &str, args: [u32; 3]) -> Outcome {
    call(machine, Interface::Graphics, name, [object, args[0], args[1], args[2]])
}

fn rect(machine: &mut Machine<BackendPadrao>, value: [i16; 4]) {
    machine.cpu_mut().write_mem(RECT, &value.into_iter().flat_map(i16::to_le_bytes).collect::<Vec<_>>()).unwrap();
}

#[test]
fn zero_arcs_draw_the_translated_filled_rectangle_at_the_screen_edge() {
    let (mut machine, work, object) = machine();
    graphics(&mut machine, object, "SetColor", [255, 0, 0]);
    graphics(&mut machine, object, "SetFillColor", [0, 0, 255]);
    graphics(&mut machine, object, "SetFillMode", [1, 0, 0]);
    graphics(&mut machine, object, "Translate", [(-2i32) as u32, 1, 0]);
    rect(&mut machine, [0, 1, 6, 5]);
    assert_eq!(graphics(&mut machine, object, "DrawRoundRectangle", [RECT, 0, 0]), Outcome::Returned { code: 0 });
    // O pedido vira [-2,2..4,7): a borda esquerda fica fora da tela e não reaparece em x=0.
    assert_eq!(machine.screen().get_pixel(0, 2), 0xf800);
    assert_eq!(machine.screen().get_pixel(0, 3), 0x001f);
    assert_eq!(machine.screen().get_pixel(2, 5), 0x001f);
    assert_eq!(machine.screen().get_pixel(3, 5), 0xf800);
    assert_eq!(machine.screen().get_pixel(0, 6), 0xf800);
    assert_eq!(machine.screen().get_pixel(4, 5), 0);
    assert_eq!(machine.screen().get_pixel(0, 7), 0);
    drop(machine);
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn zero_arcs_without_fill_leave_the_interior_unchanged() {
    let (mut machine, work, object) = machine();
    graphics(&mut machine, object, "SetColor", [0, 255, 0]);
    graphics(&mut machine, object, "SetFillMode", [0, 0, 0]);
    rect(&mut machine, [2, 2, 5, 4]);
    assert_eq!(graphics(&mut machine, object, "DrawRoundRectangle", [RECT, 0, 0]), Outcome::Returned { code: 0 });
    assert_eq!(machine.screen().get_pixel(2, 2), 0x07e0);
    assert_eq!(machine.screen().get_pixel(6, 5), 0x07e0);
    assert_eq!(machine.screen().get_pixel(3, 3), 0);
    assert_eq!(machine.screen().get_pixel(7, 5), 0);
    drop(machine);
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn eight_pixel_corner_ellipses_draw_a_rounded_outline_and_filled_interior() {
    let (mut machine, work, object) = machine();
    graphics(&mut machine, object, "SetColor", [255, 0, 0]);
    graphics(&mut machine, object, "SetFillColor", [0, 0, 255]);
    graphics(&mut machine, object, "SetFillMode", [1, 0, 0]);
    rect(&mut machine, [1, 1, 10, 10]);
    assert_eq!(graphics(&mut machine, object, "DrawRoundRectangle", [RECT, 8, 8]), Outcome::Returned { code: 0 });
    // arcWidth/Height são diâmetros: o arco de oito pixels deixa dois
    // pixels livres em cada canto da primeira linha, e não um raio de oito.
    assert_eq!((1..11).map(|x| machine.screen().get_pixel(x, 1)).collect::<Vec<_>>(),
        [0, 0, 0xf800, 0xf800, 0xf800, 0xf800, 0xf800, 0xf800, 0, 0]);
    assert_eq!(machine.screen().get_pixel(2, 2), 0xf800);
    assert_eq!(machine.screen().get_pixel(3, 3), 0x001f);
    assert_eq!(machine.screen().get_pixel(6, 6), 0x001f);
    assert_eq!(machine.screen().get_pixel(1, 5), 0xf800);
    assert_eq!(machine.screen().get_pixel(10, 5), 0xf800);
    assert_eq!(machine.screen().get_pixel(10, 10), 0);
    drop(machine);
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn rounded_outline_without_fill_preserves_the_center_and_corner_pixels() {
    let (mut machine, work, object) = machine();
    graphics(&mut machine, object, "SetColor", [0, 255, 0]);
    graphics(&mut machine, object, "SetFillMode", [0, 0, 0]);
    rect(&mut machine, [1, 1, 10, 10]);
    assert_eq!(graphics(&mut machine, object, "DrawRoundRectangle", [RECT, 8, 8]), Outcome::Returned { code: 0 });
    assert_eq!(machine.screen().get_pixel(3, 1), 0x07e0);
    assert_eq!(machine.screen().get_pixel(2, 2), 0x07e0);
    assert_eq!(machine.screen().get_pixel(3, 2), 0);
    assert_eq!(machine.screen().get_pixel(6, 6), 0);
    assert_eq!(machine.screen().get_pixel(1, 1), 0);
    drop(machine);
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn rounded_rectangle_translation_and_graphics_clip_preserve_the_original_boundary() {
    let (mut machine, work, object) = machine();
    graphics(&mut machine, object, "SetColor", [255, 0, 0]);
    graphics(&mut machine, object, "SetFillColor", [0, 0, 255]);
    graphics(&mut machine, object, "SetFillMode", [1, 0, 0]);
    graphics(&mut machine, object, "Translate", [1, 1, 0]);
    let clip = OUT + 0x20;
    let mut descriptor = vec![1, 0, 0, 0]; // CLIPPING_RECT; zero significa CLIPPING_NONE.
    descriptor.extend([4i16, 3, 4, 4].into_iter().flat_map(i16::to_le_bytes));
    machine.cpu_mut().write_mem(clip, &descriptor).unwrap();
    graphics(&mut machine, object, "SetClip", [clip, 0, 0]);
    rect(&mut machine, [0, 0, 10, 10]);
    assert_eq!(graphics(&mut machine, object, "DrawRoundRectangle", [RECT, 8, 8]), Outcome::Returned { code: 0 });
    assert_eq!(machine.screen().get_pixel(4, 3), 0x001f);
    assert_eq!(machine.screen().get_pixel(4, 4), 0x001f, "recortar não cria uma nova borda");
    assert_eq!(machine.screen().get_pixel(7, 6), 0x001f);
    assert_eq!(machine.screen().get_pixel(3, 4), 0);
    assert_eq!(machine.screen().get_pixel(4, 2), 0);
    assert_eq!(machine.screen().get_pixel(8, 4), 0);
    drop(machine);
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn rounded_rectangle_rejects_negative_corner_diameters_without_drawing() {
    let (mut machine, work, object) = machine();
    rect(&mut machine, [1, 1, 10, 10]);
    assert_eq!(graphics(&mut machine, object, "DrawRoundRectangle", [RECT, (-1i32) as u32, 8]), Outcome::Returned { code: 14 });
    assert_eq!(machine.screen().escritas(), 0);
    drop(machine);
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn a_large_rounded_rectangle_only_rasterizes_the_visible_target_region() {
    let (mut machine, work, object) = machine();
    graphics(&mut machine, object, "SetFillColor", [0, 0, 255]);
    graphics(&mut machine, object, "SetFillMode", [1, 0, 0]);
    rect(&mut machine, [-100, -100, i16::MAX, i16::MAX]);
    assert_eq!(graphics(&mut machine, object, "DrawRoundRectangle", [RECT, 8, 8]), Outcome::Returned { code: 0 });
    assert_eq!(machine.screen().get_pixel(0, 0), 0x001f);
    assert_eq!(machine.screen().get_pixel(11, 11), 0x001f);
    assert!(machine.screen().escritas() <= 12 * 12);
    drop(machine);
    std::fs::remove_dir_all(work).unwrap();
}
