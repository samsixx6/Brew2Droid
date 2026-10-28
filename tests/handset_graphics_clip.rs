use std::path::PathBuf;
use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend};
use zeebx::handset::types::DisplayGeometry;
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

const DATA: u32 = loader::STACK_BASE + 0x200;
const OUT: u32 = DATA + 0x80;

fn setup() -> (Machine<BackendPadrao>, PathBuf, u32) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let work = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local-tests").join(format!(
        "graphics-clip-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    ));
    std::fs::create_dir_all(&work).unwrap();
    let storage = StoragePaths::from_root(work.join("runtime"));
    storage.create_dirs().unwrap();
    let image = ModImage::parse([0xe3a0_0000u32, 0xe12f_ff1e].into_iter().flat_map(u32::to_le_bytes).collect()).unwrap();
    let mut machine = Machine::new_with_storage_geometry(BackendPadrao::new().unwrap(),
        loader::load_with(&image, &[]).unwrap(), &work, &storage, None,
        DisplayGeometry {width:12,height:12,depth:16}).unwrap();
    assert_eq!(machine.run(1000).unwrap(), Outcome::Returned {code:0});
    call(&mut machine, Interface::Shell, "CreateInstance", [loader::OBJECT_BASE,0x0100_2001,OUT,0]);
    let graphics = machine.cpu().read_u32(OUT).unwrap();
    assert_ne!(graphics,0);
    (machine,work,graphics)
}

fn call(m: &mut Machine<BackendPadrao>, interface: Interface, name: &str, args: [u32;4]) -> u32 {
    let slot=(0..256).find(|s|interface.method(*s)==Some(name)).unwrap();
    match m.call_guest(aee::encode(interface,slot),args,1000).unwrap() {
        Outcome::Returned {code} => code,
        other => panic!("{name}: {other:?}"),
    }
}

fn words(m: &mut Machine<BackendPadrao>, at: u32, values: &[i16]) {
    m.cpu_mut().write_mem(at,&values.iter().flat_map(|v|v.to_le_bytes()).collect::<Vec<_>>()).unwrap();
}

fn clip(m: &mut Machine<BackendPadrao>, graphics: u32, rect: [i16;4]) {
    // AEEClip tem discriminante byte e união alinhada em quatro bytes.
    m.cpu_mut().write_u32(DATA,1).unwrap();
    words(m,DATA+4,&rect);
    call(m,Interface::Graphics,"SetClip",[graphics,DATA,0,0]);
}

#[test]
fn minimap_lines_stay_inside_the_graphics_clip() {
    let (mut m,work,g)=setup();
    clip(&mut m,g,[3,3,4,4]);
    call(&mut m,Interface::Graphics,"SetColor",[g,255,0,0]);
    words(&mut m,DATA+32,&[0,5,11,5]);
    call(&mut m,Interface::Graphics,"DrawLine",[g,DATA+32,0,0]);
    for y in 0..12 { for x in 0..12 {
        let want=if y==5 && (3..7).contains(&x) {0xf800} else {0};
        assert_eq!(m.screen().get_pixel(x,y),want,"pixel ({x},{y})");
    }}
    drop(m); std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn clipping_keeps_the_original_diagonal_pixel_phase() {
    let (mut m,work,g)=setup();
    clip(&mut m,g,[3,2,4,4]);
    call(&mut m,Interface::Graphics,"SetColor",[g,255,0,0]);
    words(&mut m,DATA+32,&[0,0,11,7]);
    call(&mut m,Interface::Graphics,"DrawLine",[g,DATA+32,0,0]);
    for y in 0..12 { for x in 0..12 {
        let want=if [(3,2),(4,3),(5,3),(6,4)].contains(&(x,y)) {0xf800} else {0};
        assert_eq!(m.screen().get_pixel(x,y),want,"pixel ({x},{y})");
    }}
    drop(m); std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn clipped_markers_keep_their_real_border_and_fill() {
    let (mut m,work,g)=setup();
    clip(&mut m,g,[3,3,4,4]);
    call(&mut m,Interface::Graphics,"SetColor",[g,255,0,0]);
    call(&mut m,Interface::Graphics,"SetFillColor",[g,0,255,0]);
    call(&mut m,Interface::Graphics,"SetFillMode",[g,1,0,0]);
    words(&mut m,DATA+32,&[0,0,5,5]);
    call(&mut m,Interface::Graphics,"DrawRect",[g,DATA+32,0,0]);
    assert_eq!(m.screen().get_pixel(3,3),0x07e0);
    for (x,y) in [(4,3),(3,4),(4,4)] {assert_eq!(m.screen().get_pixel(x,y),0xf800);}
    for (x,y) in [(0,0),(2,3),(3,2),(3,5),(6,6)] {assert_eq!(m.screen().get_pixel(x,y),0);}
    drop(m); std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn graphics_clip_does_not_leak_to_bitmap_writes() {
    let (mut m,work,g)=setup();
    clip(&mut m,g,[3,3,4,4]);
    words(&mut m,DATA+32,&[3,3,4,3]);
    call(&mut m,Interface::Graphics,"DrawLine",[g,DATA+32,0,0]);
    let bitmap=call(&mut m,Interface::Display,"GetDestination",[loader::OBJECT_BASE+16,0,0,0]);
    call(&mut m,Interface::Bitmap,"DrawPixel",[bitmap,0,0,0x001f]);
    assert_eq!(m.screen().get_pixel(0,0),0x001f);
    drop(m); std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn null_clip_restores_the_entire_destination() {
    let (mut m,work,g)=setup();
    clip(&mut m,g,[3,3,0,4]);
    call(&mut m,Interface::Graphics,"SetColor",[g,255,0,0]);
    words(&mut m,DATA+32,&[0,0,11,0]);
    call(&mut m,Interface::Graphics,"DrawLine",[g,DATA+32,0,0]);
    assert_eq!(m.screen().get_pixel(0,0),0);
    assert_eq!(call(&mut m,Interface::Graphics,"SetClip",[g,0,0,0]),1);
    call(&mut m,Interface::Graphics,"DrawLine",[g,DATA+32,0,0]);
    assert_eq!(m.screen().get_pixel(0,0),0xf800);
    assert_eq!(m.screen().get_pixel(11,0),0xf800);
    drop(m); std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn clip_none_restores_the_window_and_unsupported_shapes_preserve_the_clip() {
    let (mut m,work,g)=setup();
    clip(&mut m,g,[3,3,4,4]);
    m.cpu_mut().write_u32(DATA,2).unwrap();
    assert_eq!(call(&mut m,Interface::Graphics,"SetClip",[g,DATA,0,0]),0);
    call(&mut m,Interface::Graphics,"SetColor",[g,255,0,0]);
    words(&mut m,DATA+32,&[0,5,11,5]);
    call(&mut m,Interface::Graphics,"DrawLine",[g,DATA+32,0,0]);
    assert_eq!(m.screen().get_pixel(0,5),0);
    assert_eq!(m.screen().get_pixel(3,5),0xf800);
    m.cpu_mut().write_u32(DATA,0).unwrap();
    assert_eq!(call(&mut m,Interface::Graphics,"SetClip",[g,DATA,0,0]),1);
    call(&mut m,Interface::Graphics,"DrawLine",[g,DATA+32,0,0]);
    assert_eq!(m.screen().get_pixel(0,5),0xf800);
    assert_eq!(m.screen().get_pixel(11,5),0xf800);
    drop(m); std::fs::remove_dir_all(work).unwrap();
}
