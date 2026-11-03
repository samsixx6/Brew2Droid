use zeebx::{brew::aee::{self,Interface},cpu::{BackendPadrao,CpuBackend},loader::{self,modfile::ModImage},machine::{Machine,Outcome}};
const MODULE:&str="3aee9085c06917473039cb9a30eb34d8a822f7321f304557e479f88f86c669b9";
const MIF:&str="b9645ff9c0a1a744a5fdd6a2e67732a4e95a55b837a0295a46bc04289efb1c55";
const INFO:u32=loader::STACK_BASE+0x100;

fn machine()->Machine<BackendPadrao>{
    let code=[0xe3a00000u32,0xe12fff1e].into_iter().flat_map(u32::to_le_bytes).collect();
    let mut m=Machine::new(BackendPadrao::new().unwrap(),loader::load_with(&ModImage::parse(code).unwrap(),&[]).unwrap(),".");
    m.run(1000).unwrap();m
}
fn info(m:&mut Machine<BackendPadrao>)->Vec<u8>{
    m.cpu_mut().write_mem(INFO,&[0xa5;68]).unwrap();
    m.cpu_mut().write_mem(INFO+44,&64u16.to_le_bytes()).unwrap();
    let slot=(0..256).find(|&s|Interface::Shell.method(s)==Some("GetDeviceInfo")).unwrap();
    assert_eq!(m.call_guest(aee::encode(Interface::Shell,slot),[loader::OBJECT_BASE,INFO,0,0],10000).unwrap(),Outcome::Returned{code:0});
    let mut b=vec![0;68];m.cpu().read_mem(INFO,&mut b).unwrap();b
}
#[test]
fn audited_legacy_caller_preserves_the_saved_register_after_its_sixty_byte_buffer(){
    let mut m=machine();assert!(m.set_handset_runtime_identity(MODULE,MIF));
    let b=info(&mut m);
    assert_eq!(&b[60..],&[0xa5;8]);
    assert_ne!(&b[..44],&[0xa5;44]);
    let saved=m.grava_estado();m.restaura_estado(&saved).unwrap();
    assert_eq!(&info(&mut m)[60..],&[0xa5;8]);
}
#[test]
fn an_unmatched_manifest_keeps_the_complete_declared_modern_struct(){
    let mut m=machine();assert!(!m.set_handset_runtime_identity(MODULE,&"0".repeat(64)));
    let b=info(&mut m);assert_eq!(&b[60..64],&[0;4]);assert_eq!(&b[64..],&[0xa5;4]);
}
