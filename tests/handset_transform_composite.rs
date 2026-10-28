use std::path::PathBuf;
use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend, Reg};
use zeebx::handset::types::DisplayGeometry;
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

const OUT: u32 = loader::STACK_BASE + 0x200;
const MATRIX: u32 = OUT + 16;

struct Fixture { m: Machine<BackendPadrao>, work: PathBuf, source: u32, transform: u32 }

impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let work=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local-tests").join(format!(
            "transform-composite-{}-{}",std::process::id(),NEXT.fetch_add(1,std::sync::atomic::Ordering::Relaxed)));
        std::fs::create_dir_all(&work).unwrap();
        let storage=StoragePaths::from_root(work.join("runtime")); storage.create_dirs().unwrap();
        let image=ModImage::parse([0xe3a0_0000u32,0xe12f_ff1e].into_iter().flat_map(u32::to_le_bytes).collect()).unwrap();
        let mut m=Machine::new_with_storage_geometry(BackendPadrao::new().unwrap(),loader::load_with(&image,&[]).unwrap(),&work,&storage,None,
            DisplayGeometry {width:8,height:8,depth:16}).unwrap();
        assert_eq!(m.run(1000).unwrap(),Outcome::Returned {code:0});
        let device=call(&mut m,Interface::Display,"GetDestination",[loader::OBJECT_BASE+16,0,0,0],&[]);
        assert_ne!(device,0);
        assert_eq!(call(&mut m,Interface::Bitmap,"CreateCompatibleBitmap",[device,OUT,3,2],&[]),0);
        let source=m.cpu().read_u32(OUT).unwrap();
        for (i,pixel) in [0xf800,0xf81f,0x07e0,0x001f,0xffe0,0xffff].into_iter().enumerate() {
            call(&mut m,Interface::Bitmap,"DrawPixel",[source,(i%3) as u32,(i/3) as u32,pixel],&[]);
        }
        assert_eq!(call(&mut m,Interface::Bitmap,"QueryInterface",[device,0x0100_1029,OUT,0],&[]),0);
        let transform=m.cpu().read_u32(OUT).unwrap();
        Self {m,work,source,transform}
    }

    fn simple(&mut self,flags: u32,composite: u32) -> u32 {
        call(&mut self.m,Interface::Transform,"TransformBltSimple",[self.transform,2,2,self.source],&[0,0,3,2,flags,composite])
    }

    fn complex(&mut self,composite: u32) -> u32 {
        self.m.cpu_mut().write_mem(MATRIX,&[256i16,0,0,256].into_iter().flat_map(i16::to_le_bytes).collect::<Vec<_>>()).unwrap();
        call(&mut self.m,Interface::Transform,"TransformBltComplex",[self.transform,2,2,self.source],&[0,0,3,2,MATRIX,composite])
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let actual=self.work.canonicalize().unwrap();
        assert!(actual.starts_with(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local-tests").canonicalize().unwrap()));
        std::fs::remove_dir_all(actual).unwrap();
    }
}

fn call(m: &mut Machine<BackendPadrao>,interface: Interface,name: &str,args:[u32;4],stack:&[u32]) -> u32 {
    let slot=(0..256).find(|s|interface.method(*s)==Some(name)).unwrap();
    let previous_sp=m.cpu().read_reg(Reg::Sp);
    let sp=loader::STACK_BASE+0x400;
    for (i,value) in stack.iter().enumerate() {m.cpu_mut().write_u32(sp+i as u32*4,*value).unwrap();}
    m.cpu_mut().write_reg(Reg::Sp,sp);
    let result=m.call_guest(aee::encode(interface,slot),args,1000).unwrap();
    m.cpu_mut().write_reg(Reg::Sp,previous_sp);
    match result {
        Outcome::Returned {code} => code,
        other => panic!("{name}: {other:?}"),
    }
}

#[test]
fn simple_keycolor_transform_preserves_the_destination_under_magenta() {
    let mut f=Fixture::new();
    assert_eq!(f.simple(0,0),0);
    assert_eq!(f.m.screen().get_pixel(2,2),0xf800);
    assert_eq!(f.m.screen().get_pixel(3,2),0);
    assert_eq!(f.m.screen().get_pixel(4,2),0x07e0);
}

#[test]
fn minimap_flags_six_mirror_horizontally_and_skip_the_color_key() {
    let mut f=Fixture::new();
    assert_eq!(f.simple(6,0),0);
    for (x,y,pixel) in [(2,2,0x07e0),(3,2,0),(4,2,0xf800),(2,3,0xffff),(3,3,0xffe0),(4,3,0x001f)] {
        assert_eq!(f.m.screen().get_pixel(x,y),pixel,"pixel ({x},{y})");
    }
}

#[test]
fn flip_x_reflects_across_the_horizontal_axis() {
    let mut f=Fixture::new();
    assert_eq!(f.simple(4,255),0);
    assert_eq!(f.m.screen().get_pixel(2,2),0x001f);
    assert_eq!(f.m.screen().get_pixel(4,2),0xffff);
    assert_eq!(f.m.screen().get_pixel(2,3),0xf800);
    assert_eq!(f.m.screen().get_pixel(4,3),0x07e0);
}

#[test]
fn quarter_turn_is_counter_clockwise() {
    let mut f=Fixture::new();
    assert_eq!(call(&mut f.m,Interface::Transform,"TransformBltSimple",[f.transform,2,2,f.source],&[0,0,2,2,1,255]),0);
    for (x,y,pixel) in [(2,2,0xf81f),(3,2,0xffe0),(2,3,0xf800),(3,3,0x001f)] {
        assert_eq!(f.m.screen().get_pixel(x,y),pixel,"pixel ({x},{y})");
    }
}

#[test]
fn complex_keycolor_uses_the_source_custom_native_color() {
    let mut f=Fixture::new();
    call(&mut f.m,Interface::Bitmap,"SetTransparencyColor",[f.source,0x07e0,0,0],&[]);
    assert_eq!(f.complex(0),0);
    assert_eq!(f.m.screen().get_pixel(3,2),0xf81f,"magenta deixou de ser a chave");
    assert_eq!(f.m.screen().get_pixel(4,2),0);
}

#[test]
fn opaque_and_undefined_composites_copy_the_key_color() {
    for composite in [255,42] {
        let mut f=Fixture::new();
        assert_eq!(f.simple(0,composite),0);
        assert_eq!(f.m.screen().get_pixel(3,2),0xf81f);
        let mut complex=Fixture::new();
        assert_eq!(complex.complex(composite),0);
        assert_eq!(complex.m.screen().get_pixel(3,2),0xf81f);
    }
}
