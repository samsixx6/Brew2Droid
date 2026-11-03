//! O IImage conserva uma origem de desenho real, com posse e campos salváveis.
use std::path::PathBuf;
use zeebx::{brew::aee::{self,Interface},cpu::{BackendPadrao,CpuBackend},loader::{self,modfile::ModImage},machine::{Machine,Outcome},storage::StoragePaths};
struct Fixture { m:Machine<BackendPadrao>, root:PathBuf, display:u32, target:u32, image:u32, out:u32 }
impl Fixture {
 fn new(retain:bool)->Self {
  static NEXT:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);
  let root=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local-tests/startup-repairs-wave21-20261031/wmapp").join(format!("image-test-{}-{}",std::process::id(),NEXT.fetch_add(1,std::sync::atomic::Ordering::Relaxed)));
  std::fs::create_dir_all(&root).unwrap(); let storage=StoragePaths::from_root(root.join("runtime"));storage.create_dirs().unwrap();
  let image=ModImage::parse([0xe3a00000u32,0xe12fff1e].into_iter().flat_map(u32::to_le_bytes).collect()).unwrap();
  let mut m=Machine::new_with_storage(BackendPadrao::new().unwrap(),loader::load_with(&image,&[]).unwrap(),&root,&storage,None);assert_eq!(m.run(1000).unwrap(),Outcome::Returned{code:0});
  let mut f=Self{m,root,display:0,target:0,image:0,out:loader::HEAP_BASE+0x4000};
  f.display=f.create(0x01001001);f.target=f.alloc(64);let vt=f.alloc(64);
  let query=loader::MODULE_BASE+0x100; let blit=query+16;
  f.m.cpu_mut().write_mem(query,&[0xe3a00014u32,0xe12fff1e].into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>()).unwrap();
  // BltIn captura a origem e as coordenadas; a variante retém sua própria referência.
  let mut code=vec![0xe59d1004u32,0xe5801004,0xe59d1008,0xe5801008,0xe59d100c,0xe580100c];
  if retain {code.extend([0xe92d4010,0xe1a04000,0xe5900004,0xe5901000,0xe5911000,0xe12fff31,0xe1a00004,0xe8bd4010]);}
  code.extend([0xe3a00000,0xe12fff1e]);f.m.cpu_mut().write_mem(blit,&code.into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>()).unwrap();
  f.m.cpu_mut().write_u32(vt+8,query).unwrap();f.m.cpu_mut().write_u32(vt+40,blit).unwrap();f.m.cpu_mut().write_u32(f.target,vt).unwrap();
  let mut bmp=vec![0;70];bmp[..2].copy_from_slice(b"BM");for(at,value)in[(2,70u32),(10,54),(14,40),(18,2),(22,2)]{bmp[at..at+4].copy_from_slice(&value.to_le_bytes());}bmp[26..28].copy_from_slice(&1u16.to_le_bytes());bmp[28..30].copy_from_slice(&24u16.to_le_bytes());bmp[54..62].copy_from_slice(&[255,0,0,255,255,255,0,0]);bmp[62..70].copy_from_slice(&[0,0,255,0,255,0,0,0]);std::fs::write(f.root.join("source.bmp"),bmp).unwrap();
  f.m.cpu_mut().write_mem(f.out,b"source.bmp\0").unwrap();f.image=f.call(Interface::Shell,"LoadResObject",[loader::OBJECT_BASE,f.out,0,0]);assert_ne!(f.image,0);
  f.call(Interface::Display,"SetDestination",[f.display,f.target,0,0]);f
 }
 fn call(&mut self,i:Interface,name:&str,args:[u32;4])->u32{let slot=(0..256).find(|s|i.method(*s)==Some(name)).unwrap();match self.m.call_guest(aee::encode(i,slot),args,10000).unwrap(){Outcome::Returned{code}=>code,x=>panic!("{name}: {x:?}")}}
 fn alloc(&mut self,n:u32)->u32{self.call(Interface::Helpers,"malloc",[n,0,0,0])}
 fn create(&mut self,cls:u32)->u32{self.call(Interface::Shell,"CreateInstance",[loader::OBJECT_BASE,cls,self.out,0]);self.m.cpu().read_u32(self.out).unwrap()}
 fn draw(&mut self)->u32{self.call(Interface::Image,"Draw",[self.image,0,0,0]);self.m.cpu().read_u32(self.target+4).unwrap()}
 fn field(&self,source:u32)->u32{(4..64).step_by(4).find_map(|at|(self.m.cpu().read_u32(self.image+at).unwrap()==source).then_some(self.image+at)).expect("a origem real deve ser um membro do IImage, descoberto pelo consumidor")}
 fn dimensions(&mut self,b:u32)->[u32;2]{self.call(Interface::Bitmap,"GetInfo",[b,self.out,12,0]);[self.m.cpu().read_u32(self.out).unwrap(),self.m.cpu().read_u32(self.out+4).unwrap()]}
}
impl Drop for Fixture{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.root);}}
#[test]fn foreign_draw_uses_a_stable_owned_bitmap_and_allows_zero_member_transfer(){
 let mut f=Fixture::new(false);let source=f.draw();assert_ne!(source,0);assert_eq!(f.draw(),source,"cada desenho deve usar o bitmap conservado pelo IImage");let field=f.field(source);f.m.cpu_mut().write_u32(field,0).unwrap();assert_eq!(f.call(Interface::Image,"Release",[f.image,0,0,0]),0);assert_eq!(f.dimensions(source),[2,2]);assert_eq!(f.call(Interface::Bitmap,"Release",[source,0,0,0]),0);assert_eq!(f.call(Interface::Bitmap,"AddRef",[source,0,0,0]),0);
}
#[test]fn image_release_drops_its_bitmap_but_preserves_a_bltin_reference(){
 let mut f=Fixture::new(true);let source=f.draw();assert_eq!(f.dimensions(source),[2,2]);assert_eq!(f.call(Interface::Image,"Release",[f.image,0,0,0]),0);assert_eq!(f.dimensions(source),[2,2]);assert_eq!(f.call(Interface::Bitmap,"Release",[source,0,0,0]),0);
}
#[test]fn invalid_image_member_does_not_transfer_ownership_or_adopt_the_replacement(){
 let mut f=Fixture::new(false);let source=f.draw();let field=f.field(source);let other=f.create(0x01001001);f.m.cpu_mut().write_u32(field,other).unwrap();f.call(Interface::Image,"Release",[f.image,0,0,0]);assert_eq!(f.call(Interface::Bitmap,"AddRef",[source,0,0,0]),0);assert_eq!(f.call(Interface::Display,"Release",[other,0,0,0]),0);
}
#[test]fn saved_image_cache_keeps_real_source_and_crop_coordinates(){
 let mut f=Fixture::new(false);let source=f.draw();let _=f.field(source);let state=f.m.grava_estado();f.m.restaura_estado(&state).unwrap();f.call(Interface::Image,"SetParm",[f.image,2,1,1]);f.call(Interface::Image,"SetParm",[f.image,1,1,1]);assert_eq!(f.draw(),source);assert_eq!(f.m.cpu().read_u32(f.target+8).unwrap(),1);assert_eq!(f.m.cpu().read_u32(f.target+12).unwrap(),1);assert_eq!(f.dimensions(source),[2,2]);
}
#[test]fn getbitmap_returns_a_caller_reference_separate_from_image_ownership(){
 let mut f=Fixture::new(false);f.call(Interface::Image,"SetParm",[f.image,10,f.out,f.out+4]);let b=f.m.cpu().read_u32(f.out).unwrap();assert_ne!(b,0);assert_eq!(f.call(Interface::Bitmap,"Release",[b,0,0,0]),1,"soltar a saída de GETBITMAP deixa a referência do IImage");assert_eq!(f.call(Interface::Image,"Release",[f.image,0,0,0]),0);assert_eq!(f.call(Interface::Bitmap,"AddRef",[b,0,0,0]),0);
}
#[test]fn ordinary_image_draw_keeps_color_and_crop_before_and_after_cache_materialization(){
 let mut f=Fixture::new(false);f.call(Interface::Display,"SetDestination",[f.display,0,0,0]);f.call(Interface::Image,"Draw",[f.image,3,4,0]);assert_eq!(f.m.screen().get_pixel(3,4),0xf800);f.call(Interface::Image,"SetParm",[f.image,10,f.out,0]);let b=f.m.cpu().read_u32(f.out).unwrap();f.call(Interface::Bitmap,"Release",[b,0,0,0]);f.call(Interface::Image,"SetParm",[f.image,2,1,1]);f.call(Interface::Image,"SetParm",[f.image,1,1,1]);f.call(Interface::Image,"Draw",[f.image,7,8,0]);assert_eq!(f.m.screen().get_pixel(7,8),0xffff);
}
#[test]fn decoded_new_image_already_owns_the_source_at_the_discovered_cache_member(){
 let mut f=Fixture::new(false);let first=f.draw();let offset=f.field(first)-f.image;f.m.cpu_mut().write_mem(f.out,b"source.bmp\0").unwrap();let next=f.call(Interface::Shell,"LoadResObject",[loader::OBJECT_BASE,f.out,0,0]);assert_ne!(next,0);let source=f.m.cpu().read_u32(next+offset).unwrap();assert_ne!(source,0,"uma nova imagem já decodificada possui o bitmap antes de Draw");f.m.cpu_mut().write_u32(next+offset,0).unwrap();f.call(Interface::Image,"Release",[next,0,0,0]);assert_eq!(f.dimensions(source),[2,2]);f.call(Interface::Bitmap,"Release",[source,0,0,0]);
}

#[test]
fn review_replacing_stream_refreshes_the_foreign_source_pixels() {
 let mut f=Fixture::new(false);
 let old=f.draw();
 let before=f.m.cpu().read_u32(old+8).unwrap();let mut pixel=[0;3];f.m.cpu().read_mem(before,&mut pixel).unwrap();assert_eq!(pixel,[0,0,255]);
 let mut bmp=std::fs::read(f.root.join("source.bmp")).unwrap();bmp[62..65].copy_from_slice(&[0,255,0]);
 let buffer=f.alloc(bmp.len()as u32);f.m.cpu_mut().write_mem(buffer,&bmp).unwrap();
 let stream=f.create(0x0100100c);f.call(Interface::MemAStream,"SetEx",[stream,buffer,bmp.len()as u32,0]);
 f.call(Interface::Image,"SetStream",[f.image,stream,0,0]);
 let current=f.draw();let after=f.m.cpu().read_u32(current+8).unwrap();f.m.cpu().read_mem(after,&mut pixel).unwrap();
 assert_eq!(pixel,[0,255,0],"SetStream decodes the replacement, so foreign Draw must not keep the old red source");
}
#[test]
fn review_nonzero_frame_uses_full_sheet_source_and_correct_coordinates() {
 let mut f=Fixture::new(false);f.call(Interface::Image,"SetParm",[f.image,2,1,0]);
 f.call(Interface::Image,"DrawFrame",[f.image,1,0,0]);
 let source=f.m.cpu().read_u32(f.target+4).unwrap();
 assert_eq!(f.m.cpu().read_u32(f.target+8).unwrap(),1);
 assert_eq!(f.m.cpu().read_u32(f.target+12).unwrap(),0);
 assert_eq!(f.dimensions(source),[2,2]);
}
#[test]fn replacing_stream_keeps_an_external_reference_to_the_old_pixels_and_retires_both_caches(){
 let mut f=Fixture::new(true);let old=f.draw();let old_buffer=f.m.cpu().read_u32(old+8).unwrap();let mut bmp=std::fs::read(f.root.join("source.bmp")).unwrap();bmp[62..65].copy_from_slice(&[0,255,0]);let buffer=f.alloc(bmp.len()as u32);f.m.cpu_mut().write_mem(buffer,&bmp).unwrap();let stream=f.create(0x0100100c);f.call(Interface::MemAStream,"SetEx",[stream,buffer,bmp.len()as u32,0]);f.call(Interface::Image,"SetStream",[f.image,stream,0,0]);let next=f.draw();assert_ne!(next,old);let mut pixel=[0;3];f.m.cpu().read_mem(old_buffer,&mut pixel).unwrap();assert_eq!(pixel,[0,0,255],"o novo stream não modifica uma origem ainda retida pelo consumidor");assert_eq!(f.dimensions(old),[2,2]);assert_eq!(f.call(Interface::Bitmap,"Release",[old,0,0,0]),0);assert_eq!(f.call(Interface::Image,"Release",[f.image,0,0,0]),0);assert_eq!(f.call(Interface::Bitmap,"Release",[next,0,0,0]),0);assert_eq!(f.call(Interface::Bitmap,"AddRef",[next,0,0,0]),0);
}
