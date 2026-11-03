//! Native foreground negotiation: EVT_APP_START_WINDOW precedes APP_START on modern handsets.
include!("common/modern_ui_fixture.rs");
const CLASS:u32=0x12345678;
const WINDOW_MOD:&str="bc89cbfd7565995672bbca0602fa9709e3b8fe1ad38eaf4636688bade5ae56ab";
const WINDOW_MIF:&str="513b4ff61e718a285346f3a50a392b433f068e62c9ac36429398e2ff2c121746";
impl F {
 fn window_capable(&mut self){self.m.enable_handset_clock();self.m.set_handset_runtime_identity(WINDOW_MOD,WINDOW_MIF);}

 fn startup_applet(&mut self,window:bool,window_delay:u32,start_delay:u32)->u32 {
  let p=self.alloc(64);let vt=self.alloc(16);let code=self.alloc(256);
  for off in (0..64).step_by(4){self.put(p+off,0);}
  self.put(p,vt);self.put(p+4,window as u32);self.put(p+32,window_delay);self.put(p+36,start_delay);self.put(vt+8,code);
  // Delay only the selected event, then record actual event order/arguments.
  let ops=[0xe92d4070,0xe1a04000,0xe3510014,0x02845020,0x12845024,
   0xe595c000,0xe35c0000,0x0a000002,0xe25cc001,0xe585c000,
   0x1afffffc,0xe5940008,0xe2800001,0xe5840008,0xe3500001,
   0x0584100c,0x05842010,0x05843014,0x15841018,0x1584301c,
   0xe3510014,0x05940004,0x13a00001,0xe8bd8070];
  for(i,op) in ops.into_iter().enumerate(){self.put(code+4*i as u32,op);}
  p
 }
 fn startup_finish(&mut self){for _ in 0..20000 {let outcomes=self.m.advance(1000).unwrap();if outcomes.iter().all(|v|matches!(v,Outcome::Returned{..})){if self.m.handset_event_ready(){return;}}}panic!("startup failed to finish");}
}
fn mode(m:&Machine<BackendPadrao>)->u32{let s=m.grava_estado();zeebx::save_state::Leitor::abre(&s).unwrap().u32("esc.applet_em_background").unwrap()}
#[test]
fn modern_handset_true_window_start_suppresses_legacy_start_and_sets_background(){
 let mut f=F::new();f.window_capable();let p=f.startup_applet(true,0,0);
 assert_eq!(f.m.start_applet(p,CLASS,500000).unwrap(),Outcome::Returned{code:1});
 assert_eq!(f.read(p+8),1);assert_eq!(f.read(p+12),20);assert_eq!(f.read(p+16),0);assert_eq!(f.read(p+20),0);assert_eq!(mode(&f.m),1);
}
#[test]
fn modern_handset_false_window_start_falls_back_once_with_real_appstart_payload(){
 let mut f=F::new();f.window_capable();let p=f.startup_applet(false,0,0);
 assert_eq!(f.m.start_applet(p,CLASS,500000).unwrap(),Outcome::Returned{code:1});
 assert_eq!((f.read(p+8),f.read(p+12),f.read(p+24)),(2,20,0));
 let start=f.read(p+28);assert_ne!(start,0);assert_eq!(f.read(start+4),CLASS);assert_ne!(f.read(start+8),0);assert_eq!(mode(&f.m),0);
}
#[test]
fn nonhandset_legacy_start_does_not_send_window_negotiation(){
 let mut f=F::new();f.m.set_handset_runtime_identity(WINDOW_MOD,WINDOW_MIF);let p=f.startup_applet(true,0,0);
 assert_eq!(f.m.start_applet(p,CLASS,500000).unwrap(),Outcome::Returned{code:1});
 assert_eq!((f.read(p+8),f.read(p+12)),(1,0));assert_eq!(mode(&f.m),0);
}
#[test]
fn budget_cut_during_window_start_survives_snapshot_without_duplicate_events(){
 for window in [false,true]{let mut f=F::new();f.window_capable();let p=f.startup_applet(window,5000,0);
  assert_eq!(f.m.start_applet(p,CLASS,4).unwrap(),Outcome::Budget);
  for _ in 0..100{if f.read(p+32)<5000&&f.read(p+32)>0{break;}f.m.advance(4).unwrap();}
  assert!(f.read(p+32)<5000&&f.read(p+32)>0);assert_eq!(f.read(p+8),0);
  let s=f.m.grava_estado();f.m.restaura_estado(&s).unwrap();f.startup_finish();
  assert_eq!(f.read(p+8),if window{1}else{2});assert_eq!(f.read(p+12),20);assert_eq!(mode(&f.m),window as u32);
 }
}
#[test]
fn false_window_start_keeps_thunk_live_while_fallback_is_budgeted_and_saved(){
 let mut f=F::new();f.window_capable();let p=f.startup_applet(false,0,5000);
 assert_eq!(f.m.start_applet(p,CLASS,4).unwrap(),Outcome::Budget);
 for _ in 0..300{if f.read(p+8)==1&&f.read(p+36)<5000&&f.read(p+36)>0{break;}f.m.advance(4).unwrap();}
 assert_eq!(f.read(p+8),1);assert!(f.read(p+36)<5000&&f.read(p+36)>0);
 let s=f.m.grava_estado();let reader=zeebx::save_state::Leitor::abre(&s).unwrap();let pending=reader.u32s("esc.window_start").unwrap();assert_eq!(pending.len(),4);
 f.m.restaura_estado(&s).unwrap();f.startup_finish();assert_eq!(f.read(p+8),2);assert_eq!(f.read(p+24),0);
 let done=f.m.grava_estado();assert!(zeebx::save_state::Leitor::abre(&done).unwrap().u32s("esc.window_start").unwrap().is_empty());
}
#[test]
fn startup_oom_preserves_active_applet_and_does_not_enter_new_handler(){
 let mut f=F::new();f.window_capable();let old=f.startup_applet(false,0,0);f.m.start_applet(old,CLASS,500000).unwrap();let next=f.startup_applet(false,0,0);
 let malloc=(0..256).find(|&s|Interface::Helpers.method(s)==Some("malloc")).unwrap();let mut n=loader::HEAP_SIZE as u32;
 while n>0{if f.run(aee::encode(Interface::Helpers,malloc),[n,0,0,0])==0{n/=2;}}
 assert!(f.m.start_applet(next,CLASS+1,500000).is_err());assert_eq!(f.read(next+8),0);
 let get=(0..256).find(|&s|Interface::Helpers.method(s)==Some("GetAppInstance")).unwrap();let entry=f.read(loader::HELPERS_BASE+4*get);assert_eq!(f.run(entry,[0;4]),old);
}
#[test]
fn optional_startup_section_is_backward_compatible_and_rejects_bad_pending_atomically(){
 let mut f=F::new();f.window_capable();let p=f.startup_applet(false,0,0);f.m.start_applet(p,CLASS,500000).unwrap();f.put(OUT,0x12345678);
 let state=f.m.grava_estado();let reader=zeebx::save_state::Leitor::abre(&state).unwrap();let mut old=zeebx::save_state::Secoes::nova();let mut bad=zeebx::save_state::Secoes::nova();
 for name in reader.nomes(){if name=="esc.window_start"{continue;}let bytes=reader.secao(name).unwrap().to_vec();old.poe(name,bytes.clone());let mut bytes=bytes;if name=="mem.heap"{let at=(OUT-loader::HEAP_BASE)as usize;bytes[at..at+4].copy_from_slice(&0xdeadbeefu32.to_le_bytes());}bad.poe(name,bytes);}
 f.m.restaura_estado(&old.fecha()).unwrap();assert_eq!(f.read(OUT),0x12345678);
 bad.poe_u32s("esc.window_start",[1,0xdead0000,loader::STACK_BASE,p]);assert!(f.m.restaura_estado(&bad.fecha()).is_err());assert_eq!(f.read(OUT),0x12345678);
}

#[test]
fn handset_clock_alone_retains_classic_start_for_unknown_apps(){
 let mut f=F::new();f.m.enable_handset_clock();let p=f.startup_applet(true,0,0);
 f.m.start_applet(p,CLASS,500000).unwrap();
 assert_eq!((f.read(p+8),f.read(p+12)),(1,0),"clock support alone does not imply Brew MP window startup");
 assert_eq!(mode(&f.m),0);
}
#[test]
fn verified_window_profile_requires_both_original_package_digests(){
 for(module,mif,windowed)in[(WINDOW_MOD,WINDOW_MIF,true),("00",WINDOW_MIF,false),(WINDOW_MOD,"00",false),(WINDOW_MOD,"",false)]{
  let mut f=F::new();f.m.enable_handset_clock();f.m.set_handset_runtime_identity(module,mif);
  let p=f.startup_applet(true,0,0);f.m.start_applet(p,CLASS,500000).unwrap();
  assert_eq!(f.read(p+12),if windowed{20}else{0},"both verified digests are required");
  assert_eq!(mode(&f.m),windowed as u32);
 }
}
