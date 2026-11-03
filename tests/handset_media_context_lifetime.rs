//! Vida útil do contexto em notificações de mídia e sua liberação pelo guest.
//! O guest ARM real registra e executa callbacks; nenhuma API/factory e substituida.
use std::{path::PathBuf, sync::atomic::{AtomicU64, Ordering}};
use zeebx::{brew::aee::{self, Interface}, cpu::{BackendPadrao, CpuBackend}, loader::{self, modfile::ModImage}, machine::{Machine, Outcome}, storage::StoragePaths};
const MARK: u32 = loader::STACK_BASE + 0x2000;
const OUT: u32 = loader::STACK_BASE + 0x2100;
const COUNT: u32 = loader::MODULE_BASE + 0x40;
const FREE_CALLBACK: u32 = loader::MODULE_BASE + 0x80;
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture { m: Machine<BackendPadrao>, _root: PathBuf }
fn api(iface: Interface, name: &str) -> u32 { let slot=(0..256).find(|slot|iface.method(*slot)==Some(name)).expect("metodo conhecido"); aee::encode(iface,slot) }
fn call(m: &mut Machine<BackendPadrao>, iface: Interface, name: &str, args:[u32;4]) -> u32 { match m.call_guest(api(iface,name),args,100_000).unwrap() { Outcome::Returned{code}=>code, other=>panic!("{name}: {other:?}") } }
impl Fixture {
 fn new()->Self {
  let root=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local-tests/media-context-regression").join(format!("{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));std::fs::create_dir_all(&root).unwrap();
  let mut bytes=vec![0u8;0x200];
  let count=[0xe59f2010u32,0xe5923000,0xe2833001,0xe5823000,0xe3a00000,0xe12fff1e,MARK];
  let free=[0xe92d4010u32,0xe59f3008,0xe12fff33,0xe3a00000,0xe8bd8010,api(Interface::Helpers,"free")];
  for (offset,words) in [(0,vec![0xe3a00000,0xe12fff1e]),(0x40,count.to_vec()),(0x80,free.to_vec())] { for (i,word) in words.into_iter().enumerate(){bytes[offset+i*4..offset+i*4+4].copy_from_slice(&word.to_le_bytes());} }
  let image=ModImage::parse(bytes).unwrap();let loaded=loader::load_with(&image,&[]).unwrap();let storage=StoragePaths::from_root(root.join("runtime"));storage.create_dirs().unwrap();
  let mut m=Machine::new_with_storage(BackendPadrao::new().unwrap(),loaded,&root,&storage,None);assert_eq!(m.run(1000).unwrap(),Outcome::Returned{code:0});m.cpu_mut().write_u32(MARK,0).unwrap();Self{m,_root:root}
 }
 fn instance(&mut self,class:u32)->u32 { assert_eq!(call(&mut self.m,Interface::Shell,"CreateInstance",[loader::OBJECT_BASE,class,OUT,0]),0);self.m.cpu().read_u32(OUT).unwrap() }
 fn alloc(&mut self,n:u32)->u32 { let p=call(&mut self.m,Interface::Helpers,"malloc",[n,0,0,0]);assert_ne!(p,0);p }
 fn media(&mut self,ctx:u32,callback:u32)->u32 { let util=self.instance(0x0100550d);assert_eq!(call(&mut self.m,Interface::MediaUtil,"CreateMedia",[util,0,OUT,0]),0);let media=self.m.cpu().read_u32(OUT).unwrap();self.register(media,ctx,callback);media }
 fn register(&mut self,media:u32,ctx:u32,callback:u32) { assert_eq!(call(&mut self.m,Interface::Media,"RegisterNotify",[media,callback,ctx,0]),0); }
 fn queue(&mut self,media:u32) { // Sem fonte, o tocador encerra e enfileira DONE pelo caminho real.
  assert_eq!(call(&mut self.m,Interface::Media,"Play",[media,0,0,0]),0);
 }
 fn deliver(&mut self) { let results=self.m.entrega_avisos_de_midia(100_000).unwrap();assert!(results.iter().all(|o|matches!(o,Outcome::Returned{..})),"{results:?}");assert!(self.m.swallowed_faults().is_empty()); }
 fn count(&self)->u32 {self.m.cpu().read_u32(MARK).unwrap()}
}
#[test]
fn freeing_callback_context_cancels_pending_notice_through_every_guest_free_api() {
 for mode in 0..5 { let mut f=Fixture::new();let ctx=f.alloc(24);let media=f.media(ctx,COUNT);f.queue(media);
  match mode { 0=>{call(&mut f.m,Interface::Helpers,"free",[ctx,0,0,0]);},1=>{call(&mut f.m,Interface::Helpers,"sysfree",[ctx,0,0,0]);},2=>{let h=f.instance(0x01001002);call(&mut f.m,Interface::Heap,"Free",[h,ctx,0,0]);},3=>{let t=f.instance(0x01001017);call(&mut f.m,Interface::Thread,"Free",[t,ctx,0,0]);},_=>{call(&mut f.m,Interface::Shell,"FreeResData",[loader::OBJECT_BASE,ctx,0,0]);} }
  f.deliver();assert_eq!(f.count(),0,"free route {mode}");
 }
}
#[test]
fn free_cancels_an_interior_context_but_not_the_adjacent_live_allocation() {
 let mut f=Fixture::new();let base=f.alloc(24);let next=f.alloc(24);let a=f.media(base+8,COUNT);let b=f.media(next,COUNT);f.queue(a);f.queue(b);call(&mut f.m,Interface::Helpers,"free",[base,0,0,0]);f.deliver();assert_eq!(f.count(),1);
}
#[test]
fn successful_moving_realloc_and_realloc_zero_end_the_old_context_lifetime() {
 for size in [0,128] {let mut f=Fixture::new();let ctx=f.alloc(24);let a=f.media(ctx+4,COUNT);f.queue(a);let moved=call(&mut f.m,Interface::Helpers,"realloc",[ctx,size,0,0]);if size==0{assert_eq!(moved,0)}else{assert_ne!(moved,0);assert_ne!(moved,ctx)}f.deliver();assert_eq!(f.count(),0,"realloc {size}");}
}
#[test]
fn failed_realloc_and_invalid_free_leave_the_live_context_and_notice_valid() {
 let mut f=Fixture::new();let ctx=f.alloc(24);let a=f.media(ctx,COUNT);f.queue(a);assert_eq!(call(&mut f.m,Interface::Helpers,"realloc",[ctx,0x70000000,0,0]),0);call(&mut f.m,Interface::Helpers,"free",[ctx+4,0,0,0]);f.deliver();assert_eq!(f.count(),1);
}
#[test]
fn release_only_keeps_snapshot_and_null_or_global_contexts_survive_unrelated_free() {
 for context in [None,Some(0),Some(MARK+32)] {let mut f=Fixture::new();let other=f.alloc(24);let ctx=context.unwrap_or_else(||f.alloc(24));let a=f.media(ctx,COUNT);f.queue(a);assert_eq!(call(&mut f.m,Interface::Media,"Release",[a,0,0,0]),0);call(&mut f.m,Interface::Helpers,"free",[other,0,0,0]);f.deliver();assert_eq!(f.count(),1);}
}
#[test]
fn free_clears_old_registration_before_the_address_is_reused() {
 let mut f=Fixture::new();let ctx=f.alloc(24);let a=f.media(ctx,COUNT);call(&mut f.m,Interface::Helpers,"free",[ctx,0,0,0]);let reused=f.alloc(24);assert_eq!(reused,ctx);f.queue(a);f.deliver();assert_eq!(f.count(),0,"old registration must not follow ABA reuse");f.register(a,reused,COUNT);f.queue(a);f.deliver();assert_eq!(f.count(),1,"explicit new registration remains valid");
}
#[test]
fn free_from_first_arm_callback_cancels_the_detached_tail_before_aba_reuse() {
 let mut f=Fixture::new();let ctx=f.alloc(24);let a=f.media(ctx,FREE_CALLBACK);let b=f.media(ctx+4,COUNT);f.queue(a);f.queue(b);f.deliver();assert_eq!(f.count(),0,"local batch tail was queued before FREE");let reused=f.alloc(24);assert_eq!(reused,ctx);f.register(b,reused,COUNT);f.queue(b);f.deliver();assert_eq!(f.count(),1);
}
#[test]
fn cancellation_and_pending_live_notice_have_ordinary_save_state_roundtrips() {
 let mut f=Fixture::new();let ctx=f.alloc(24);let a=f.media(ctx,COUNT);f.queue(a);let pending=f.m.grava_estado();f.m.restaura_estado(&pending).unwrap();call(&mut f.m,Interface::Helpers,"free",[ctx,0,0,0]);let canceled=f.m.grava_estado();f.m.restaura_estado(&canceled).unwrap();f.deliver();assert_eq!(f.count(),0);f.m.restaura_estado(&pending).unwrap();f.deliver();assert_eq!(f.count(),1,"uncancelled snapshot restores independently");
}

// Um literal por uso mantem o programa ARM do teste independente do tamanho das APIs.
fn install_arm(m:&mut Machine<BackendPadrao>,base:u32,mut words:Vec<u32>,literals:Vec<(usize,u32,u32)>) {
 for (index,reg,value) in literals {let target=base+words.len() as u32*4;let pc=base+index as u32*4+8;let delta=target.checked_sub(pc).unwrap();assert!(delta<4096);words[index]=0xe59f0000|(reg<<12)|delta;words.push(value);}
 let bytes=words.into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>();m.cpu_mut().write_mem(base,&bytes).unwrap();
}
#[test]
fn free_inside_callback_drops_only_old_tail_and_keeps_new_same_address_registration() {
 let mut f=Fixture::new();let ctx=f.alloc(24);let payload=f.alloc(24);let b=f.media(ctx+4,COUNT);let callback=loader::MODULE_BASE+0xc0;
 // Free(ctx); new=Malloc(24); RegisterNotify(b,COUNT,new); Play(b).
 let words=vec![0xe92d4070,0xe1a04000,0xe5940000,0,0xe12fff3c,0xe3a00018,0,0xe12fff3c,0xe584000c,0xe1a02000,0xe5940004,0,0,0xe12fff3c,0xe5940004,0,0xe12fff3c,0xe3a00000,0xe8bd8070];
 let literals=vec![(3,12,api(Interface::Helpers,"free")),(6,12,api(Interface::Helpers,"malloc")),(11,1,COUNT),(12,12,api(Interface::Media,"RegisterNotify")),(15,12,api(Interface::Media,"Play"))];install_arm(&mut f.m,callback,words,literals);
 f.m.cpu_mut().write_u32(payload,ctx).unwrap();f.m.cpu_mut().write_u32(payload+4,b).unwrap();let a=f.media(payload,callback);f.queue(a);f.queue(b);f.deliver();assert_eq!(f.m.cpu().read_u32(payload+12).unwrap(),ctx,"real allocator ABA");assert_eq!(f.count(),0,"old tail must be canceled during first callback");f.deliver();assert_eq!(f.count(),1,"fresh notice queued after FREE/new registration must survive");
}
#[test]
fn budget_after_free_leaves_no_stale_tail_and_roundtrips_ordinary_continuation() {
 let mut f=Fixture::new();let ctx=f.alloc(24);let callback=loader::MODULE_BASE+0xc0;
 // O primeiro callback libera o contexto e demora para devolver o controle.
 install_arm(&mut f.m,callback,vec![0xe92d4010,0,0xe12fff3c,0xe3a02a02,0xe2522001,0x1afffffd,0xe3a00000,0xe8bd8010],vec![(1,12,api(Interface::Helpers,"free"))]);
 let a=f.media(ctx,callback);let b=f.media(ctx+4,COUNT);f.queue(a);f.queue(b);let outcomes=f.m.entrega_avisos_de_midia(32).unwrap();assert!(outcomes.iter().any(|o|matches!(o,Outcome::Budget)),"{outcomes:?}");let state=f.m.grava_estado();f.m.restaura_estado(&state).unwrap();for _ in 0..10 {f.m.advance(100_000).unwrap();if f.m.handset_event_ready(){break;}}f.deliver();assert_eq!(f.count(),0);
}

#[test]
fn native_and_legacy_sound_free_cancel_pending_and_future_notices_until_new_registration() {
 for (class,iface) in [(0x01001056,Interface::Sound),(0x0100100b,Interface::SoundLegacy)] {
  let mut f=Fixture::new();let ctx=f.alloc(24);let sound=f.instance(class);
  call(&mut f.m,iface,"RegisterNotify",[sound,COUNT,ctx,0]);
  for method in ["SetDevice","SetVolume","GetVolume"] {call(&mut f.m,iface,method,[sound,3,0,0]);}
  call(&mut f.m,Interface::Helpers,"free",[ctx,0,0,0]);
  for _ in 0..4 {f.deliver();}assert_eq!(f.count(),0,"pending {iface:?}");
  call(&mut f.m,iface,"SetDevice",[sound,0,0,0]);f.deliver();assert_eq!(f.count(),0,"registration {iface:?}");
  let reused=f.alloc(24);assert_eq!(reused,ctx);call(&mut f.m,iface,"RegisterNotify",[sound,COUNT,reused,0]);
  call(&mut f.m,iface,"GetVolume",[sound,0,0,0]);f.deliver();assert_eq!(f.count(),1,"fresh {iface:?}");
 }
}

#[test]
fn sound_contexts_survive_unrelated_free_and_keep_existing_release_cancellation() {
 for (class,iface) in [(0x01001056,Interface::Sound),(0x0100100b,Interface::SoundLegacy)] {
  let mut f=Fixture::new();let ctx=f.alloc(24);let other=f.alloc(24);let sound=f.instance(class);
  call(&mut f.m,iface,"RegisterNotify",[sound,COUNT,ctx,0]);call(&mut f.m,iface,"SetVolume",[sound,3,0,0]);
  call(&mut f.m,Interface::Helpers,"free",[other,0,0,0]);f.deliver();assert_eq!(f.count(),1);
  call(&mut f.m,iface,"GetVolume",[sound,0,0,0]);call(&mut f.m,iface,"Release",[sound,0,0,0]);
  f.deliver();assert_eq!(f.count(),1,"ISound Release still cancels its own notice");
 }
}

#[test]
fn recycled_media_owner_as_sound_does_not_hide_the_original_context_lifetime() {
 let mut f=Fixture::new();let ctx=f.alloc(24);let live=f.alloc(24);let media=f.media(ctx,COUNT);f.queue(media);
 call(&mut f.m,Interface::Media,"Release",[media,0,0,0]);let sound=f.instance(0x01001056);
 assert_eq!(sound,media,"real object-slot reuse is required by this control");
 call(&mut f.m,Interface::Helpers,"free",[ctx,0,0,0]);f.deliver();f.deliver();
 assert_eq!(f.count(),0,"a new owner kind cannot hide the old queued context");
 call(&mut f.m,Interface::Sound,"RegisterNotify",[sound,COUNT,live,0]);
 call(&mut f.m,Interface::Sound,"SetDevice",[sound,0,0,0]);
 f.deliver();f.deliver();assert_eq!(f.count(),1,"only the newly registered live context survives");
}

#[test]
fn sound_callback_free_cancels_mixed_tail_and_both_registered_contexts() {
 let mut f=Fixture::new();let ctx=f.alloc(24);let sound=f.instance(0x01001056);let media=f.media(ctx+4,COUNT);
 call(&mut f.m,Interface::Sound,"RegisterNotify",[sound,FREE_CALLBACK,ctx,0]);
 call(&mut f.m,Interface::Sound,"SetDevice",[sound,0,0,0]);f.queue(media);f.deliver();f.deliver();
 assert_eq!(f.count(),0,"mixed sound/media tail");
 call(&mut f.m,Interface::Sound,"SetDevice",[sound,0,0,0]);f.queue(media);f.deliver();f.deliver();
 assert_eq!(f.count(),0,"neither map may recapture a freed context");
}

