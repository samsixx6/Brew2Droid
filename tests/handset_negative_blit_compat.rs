use std::path::PathBuf;
use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend, Reg},
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    storage::StoragePaths,
};

const MODULE: &str = "ec6468e48bac24002744577387db4036f4e877fa5cc4d23b250fceacb935273a";
const MIF: &str = "cd917c2c0c359cfc970d578a0a3fb2195df22c5471cc76c9fc25b6e308f0a127";

struct Fixture {
    machine: Machine<BackendPadrao>,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local-tests").join(format!(
            "negative-blit-{}-{}",std::process::id(),
            NEXT.fetch_add(1,std::sync::atomic::Ordering::Relaxed)));
        std::fs::create_dir_all(&root).unwrap();
        let image = ModImage::parse(
            [0xe3a0_0000u32, 0xe12f_ff1e].into_iter().flat_map(u32::to_le_bytes).collect(),
        ).unwrap();
        let storage = StoragePaths::from_root(root.join("runtime"));
        let mut machine = Machine::new_with_storage(
            BackendPadrao::new().unwrap(), loader::load_with(&image, &[]).unwrap(),
            &root, &storage, None,
        );
        assert_eq!(machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
        Self { machine, root }
    }

    fn call(&mut self, iface: Interface, name: &str, args: [u32; 4], stack: &[u32]) -> u32 {
        let slot = (0..256).find(|slot| iface.method(*slot) == Some(name)).unwrap();
        let old_sp = self.machine.cpu().read_reg(Reg::Sp);
        let sp = old_sp - 32;
        self.machine.cpu_mut().write_reg(Reg::Sp,sp);
        for (index,value) in stack.iter().enumerate() {
            self.machine.cpu_mut().write_u32(sp+index as u32*4,*value).unwrap();
        }
        let outcome = self.machine.call_guest(aee::encode(iface,slot),args,1000).unwrap();
        self.machine.cpu_mut().write_reg(Reg::Sp,old_sp);
        match outcome {
            Outcome::Returned { code } => code,
            other => panic!("{name}: {other:?}"),
        }
    }

    fn native_image(&mut self) -> u32 {
        // BMP própria 3x2, linhas de cima: vermelho/verde/azul e branco/magenta/amarelo.
        let mut bmp = vec![0u8; 78];
        bmp[..2].copy_from_slice(b"BM");
        for (at, value) in [(2,78u32),(10,54),(14,40),(18,3),(22,2)] {
            bmp[at..at+4].copy_from_slice(&value.to_le_bytes());
        }
        bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
        bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
        bmp[54..66].copy_from_slice(&[255,255,255,255,0,255,0,255,255,0,0,0]);
        bmp[66..78].copy_from_slice(&[0,0,255,0,255,0,255,0,0,0,0,0]);
        let ptr = self.call(Interface::Helpers,"malloc",[78,0,0,0], &[]);
        self.machine.cpu_mut().write_mem(ptr, &bmp).unwrap();
        let source = self.call(Interface::Helpers,"SetupNativeImage",[0x01004001,ptr,0,0], &[]);
        assert_ne!(source, 0);
        source
    }

    fn blit(&mut self, source: u32, size: (i32,i32), origin: (u32,u32)) {
        self.call(Interface::Display,"BitBlt",[loader::OBJECT_BASE+16,1,2,size.0 as u32],
            &[size.1 as u32,source,origin.0,origin.1,7]);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let root = self.root.canonicalize().unwrap();
        let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local-tests").canonicalize().unwrap();
        assert!(root.starts_with(parent));
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn fruit_escape_sentinel_blits_the_converted_image_with_transparency() {
    let mut f = Fixture::new();
    f.machine.set_handset_runtime_identity(MODULE,MIF);
    let source = f.native_image();
    f.blit(source,(-1,-1),(0,0));
    assert_eq!((1..4).map(|x| f.machine.screen().get_pixel(x,2)).collect::<Vec<_>>(),
        [0xf800,0x07e0,0x001f]);
    assert_eq!((1..4).map(|x| f.machine.screen().get_pixel(x,3)).collect::<Vec<_>>(),
        [0xffff,0,0xffe0]);
    assert_eq!(f.machine.screen().get_pixel(4,3),0);
}

#[test]
fn fruit_escape_sentinel_resolves_before_display_clipping() {
    let mut f = Fixture::new();
    f.machine.set_handset_runtime_identity(MODULE,MIF);
    let source = f.native_image();
    let rect = loader::STACK_BASE + 0x100;
    let bytes: Vec<_> = [2i16,2,1,1].into_iter().flat_map(i16::to_le_bytes).collect();
    f.machine.cpu_mut().write_mem(rect,&bytes).unwrap();
    f.call(Interface::Display,"SetClipRect",[loader::OBJECT_BASE+16,rect,0,0], &[]);
    f.blit(source,(-1,-1),(0,0));
    assert_eq!(f.machine.screen().get_pixel(2,2),0x07e0);
    assert_eq!(f.machine.screen().get_pixel(1,2),0);
    assert_eq!(f.machine.screen().get_pixel(3,2),0);
}

#[test]
fn fruit_escape_sentinel_requires_both_hashes_and_resets_with_identity() {
    for identity in [(MODULE,"00"),("00",MIF),("00","00")] {
        let mut f = Fixture::new();
        f.machine.set_handset_runtime_identity(MODULE,MIF);
        f.machine.set_handset_runtime_identity(identity.0,identity.1);
        let source = f.native_image();
        f.blit(source,(-1,-1),(0,0));
        assert_eq!(f.machine.screen().get_pixel(1,2),0,"identity {identity:?}");
    }
}

#[test]
fn fruit_escape_profile_preserves_other_negative_sizes_and_positive_regions() {
    let mut f = Fixture::new();
    f.machine.set_handset_runtime_identity(MODULE,MIF);
    let source = f.native_image();
    for size in [(-2,-2),(-1,2),(3,-1),(0,0)] {
        f.blit(source,size,(0,0));
        assert_eq!(f.machine.screen().get_pixel(1,2),0,"size {size:?}");
    }
    f.blit(source,(1,1),(1,0));
    assert_eq!(f.machine.screen().get_pixel(1,2),0x07e0);
    assert_eq!(f.machine.screen().get_pixel(2,2),0);
}
