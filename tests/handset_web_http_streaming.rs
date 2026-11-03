#![allow(dead_code)]
include!("common/modern_ui_fixture.rs");
use std::{net::TcpListener,io::{Read as _,Write as _},time::{Duration,Instant}};

const PCB:u32=OUT+256;
const CONTEXT:u32=OUT+288;
const URL:u32=OUT+320;
const RESPONSE:u32=OUT+64;

fn callback(f:&mut F){
 let code=[0xe5901000u32,0xe2811001,0xe5801000,0xe12fff1e];
 let function=loader::MODULE_BASE+0x100;
 f.m.cpu_mut().write_mem(function,&code.into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>()).unwrap();
 f.m.cpu_mut().write_mem(PCB,&[0;24]).unwrap();f.put(PCB+16,function);f.put(PCB+20,CONTEXT);f.put(CONTEXT,0);f.put(RESPONSE,0);
}
fn setup(f:&mut F,url:&str)->u32{f.m.set_network(true);let web=f.create(0x01005000);callback(f);f.m.cpu_mut().write_mem(URL,format!("{url}\0").as_bytes()).unwrap();web}
fn begin(f:&mut F,web:u32){f.extra(web,6,[RESPONSE,PCB,URL],&[0]);}
fn wait(f:&mut F)->u32{let end=Instant::now()+Duration::from_secs(3);while f.read(CONTEXT)==0&&Instant::now()<end{if !f.m.handset_event_ready(){f.m.advance(100000).unwrap();}else{f.m.deliver_signals(100000).unwrap();}std::thread::sleep(Duration::from_millis(2));}assert_eq!(f.read(CONTEXT),1);let response=f.read(RESPONSE);assert_ne!(response,0);response}
fn options(f:&mut F,at:u32,pairs:&[(u32,u32)]){for(i,&(id,value))in pairs.iter().enumerate(){f.put(at+i as u32*8,id);f.put(at+i as u32*8+4,value);}f.put(at+pairs.len()as u32*8,0);}
fn guest_peek(f:&mut F,bytes:&[u8],code:i32)->u32{
 let object=OUT+8192;let table=object+64;let buffer=object+128;f.m.cpu_mut().write_mem(object,&[0;64]).unwrap();f.put(object,table);f.put(object+4,1);f.put(object+8,buffer);f.put(object+12,bytes.len()as u32);f.put(object+20,code as u32);f.m.cpu_mut().write_mem(buffer,bytes).unwrap();
 let functions:[&[u32];5]=[
  &[0xe5901004,0xe2811001,0xe5801004,0xe1a00001,0xe12fff1e],
  &[0xe5901004,0xe2411001,0xe5801004,0xe1a00001,0xe12fff1e],
  &[0xe5820000,0xe5901004,0xe2811001,0xe5801004,0xe3a00000,0xe12fff1e],
  &[0xe590c018,0xe28cc001,0xe580c018,0xe590c00c,0xe5901010,0xe04cc001,0xe582c000,0xe590c014,0xe583c000,0xe5900008,0xe0800001,0xe12fff1e],
  &[0xe5902010,0xe0822001,0xe5802010,0xe590301c,0xe2833001,0xe580301c,0xe3a00000,0xe12fff1e],
 ];
 for(i,words)in functions.iter().enumerate(){let function=loader::MODULE_BASE+0x400+i as u32*128;f.m.cpu_mut().write_mem(function,&words.iter().flat_map(|w|w.to_le_bytes()).collect::<Vec<_>>()).unwrap();let slot=[0,1,2,5,7][i];f.put(table+slot*4,function);}object
}
fn waiting(f:&mut F)->u32{
 let p=guest_peek(f,b"",-2);let ready=loader::MODULE_BASE+0x800;let cancel=ready+64;
 // Peekable stores a provider-owned cancel procedure, as CALLBACK_Cancel requires.
 let words=[0xe5801020u32,0xe581000c,0xe59f2004,0xe5812008,0xe12fff1e,cancel];
 f.m.cpu_mut().write_mem(ready,&words.into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>()).unwrap();f.put(f.read(p)+24,ready);
 f.m.cpu_mut().write_mem(cancel,&[0xe590000cu32,0xe3a01000,0xe5801020,0xe5901024,0xe2811001,0xe5801024,0xe12fff1e].into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>()).unwrap();p
}
fn begin_waiting(f:&mut F,p:u32,web:u32){let opts=OUT+1024;options(f,opts,&[(0x30001,p),(0x20006,3)]);f.extra(web,7,[RESPONSE,PCB,URL],&[opts]);for _ in 0..30{f.m.deliver_signals(100000).unwrap();if f.read(p+32)!=0{break;}}assert_ne!(f.read(p+32),0);}
fn drain(f:&mut F){for _ in 0..30{if !f.m.handset_event_ready(){f.m.advance(100000).unwrap();}else{f.m.deliver_signals(100000).unwrap();}}}

#[test]fn cancellation_calls_provider_cancel_and_release_without_network_or_publication(){
 for action in 0..3{let listener=TcpListener::bind((std::net::Ipv4Addr::LOCALHOST,0)).unwrap();listener.set_nonblocking(true).unwrap();let mut f=F::new();let web=setup(&mut f,&format!("http://{}/cancel",listener.local_addr().unwrap()));let p=waiting(&mut f);begin_waiting(&mut f,p,web);assert_eq!(f.read(p+4),2);
 if action==0{let cancel=f.read(PCB+8);f.run(cancel,[PCB,0,0,0]);}else if action==1{f.call(web,1,[0;3]);}else{assert!(f.m.encerra_applet().unwrap());}
 drain(&mut f);assert_eq!(f.read(p+4),1);assert_eq!(f.read(p+32),0);assert_eq!(f.read(p+36),1);assert_eq!(f.read(RESPONSE),0);assert_eq!(f.read(CONTEXT),0);assert!(matches!(listener.accept(),Err(e)if e.kind()==std::io::ErrorKind::WouldBlock));if action!=1{f.call(web,1,[0;3]);}}
}
#[test]fn restore_waiting_body_retires_provider_and_reports_local_failure_without_post_replay(){
 let listener=TcpListener::bind((std::net::Ipv4Addr::LOCALHOST,0)).unwrap();listener.set_nonblocking(true).unwrap();let mut f=F::new();let web=setup(&mut f,&format!("http://{}/restore",listener.local_addr().unwrap()));let p=waiting(&mut f);begin_waiting(&mut f,p,web);let state=f.m.grava_estado();f.m.restaura_estado(&state).unwrap();let r=wait(&mut f);let info=f.call(r,6,[0;3]);assert_eq!(f.read(info),(-1i32)as u32);assert_eq!(f.read(p+4),1);assert_eq!(f.read(p+36),1);assert_eq!(f.read(p+32),0);assert!(matches!(listener.accept(),Err(e)if e.kind()==std::io::ErrorKind::WouldBlock));f.call(r,1,[0;3]);f.call(web,1,[0;3]);
}
#[test]fn disabling_network_while_waiting_returns_netdown_after_balanced_provider_retirement(){
 let mut f=F::new();let web=setup(&mut f,"http://127.0.0.1:1/offline");let p=waiting(&mut f);begin_waiting(&mut f,p,web);f.m.set_network(false);let r=wait(&mut f);let info=f.call(r,6,[0;3]);assert_eq!(f.read(info),(-0x216i32)as u32);assert_eq!(f.read(p+4),1);assert_eq!(f.read(p+36),1);f.call(r,1,[0;3]);f.call(web,1,[0;3]);
}
#[test]fn restore_budget_inside_qi_resumes_that_call_before_releasing_its_reference(){
 let listener=TcpListener::bind((std::net::Ipv4Addr::LOCALHOST,0)).unwrap();listener.set_nonblocking(true).unwrap();let mut f=F::new();let web=setup(&mut f,&format!("http://{}/budget",listener.local_addr().unwrap()));let p=waiting(&mut f);let qi=f.read(f.read(p)+8);let mut code=vec![0xe3a0cc01u32,0xe25cc001,0x1afffffd];code.extend([0xe5820000,0xe5901004,0xe2811001,0xe5801004,0xe3a00000,0xe12fff1e]);f.m.cpu_mut().write_mem(qi,&code.into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>()).unwrap();let opts=OUT+1024;options(&mut f,opts,&[(0x30001,p)]);f.extra(web,7,[RESPONSE,PCB,URL],&[opts]);let outcomes=f.m.deliver_signals(1).unwrap();assert!(outcomes.iter().any(|o|matches!(o,Outcome::Budget)));assert_eq!(f.read(p+4),1);let state=f.m.grava_estado();f.m.restaura_estado(&state).unwrap();let r=wait(&mut f);let info=f.call(r,6,[0;3]);assert_eq!(f.read(info),(-1i32)as u32);assert_eq!(f.read(p+4),1);assert_eq!(f.read(p+24),0,"retired request never calls Peek");assert!(matches!(listener.accept(),Err(e)if e.kind()==std::io::ErrorKind::WouldBlock));f.call(r,1,[0;3]);f.call(web,1,[0;3]);
}
#[test]fn nonreturning_peek_is_bounded_across_budget_resumptions_and_releases_qi(){
 let mut f=F::new();let web=setup(&mut f,"http://127.0.0.1:1/endless");let p=waiting(&mut f);let endless=loader::MODULE_BASE+0x900;f.m.cpu_mut().write_u32(endless,0xeafffffe).unwrap();f.put(f.read(p)+20,endless);let opts=OUT+1024;options(&mut f,opts,&[(0x30001,p)]);f.extra(web,7,[RESPONSE,PCB,URL],&[opts]);let r=wait(&mut f);let info=f.call(r,6,[0;3]);assert_eq!(f.read(info),(-0x510i32)as u32);assert_eq!(f.read(p+4),1);assert_eq!(f.read(p+32),0);f.call(r,1,[0;3]);f.call(web,1,[0;3]);
}
#[test]fn provider_instruction_limit_includes_guest_work_between_brew_api_calls(){
 let mut f=F::new();let web=setup(&mut f,"http://127.0.0.1:1/endless-api");let p=waiting(&mut f);let endless=loader::MODULE_BASE+0x900;let slot=(0..256).find(|&s|Interface::Helpers.method(s)==Some("strlen")).unwrap();let api=aee::encode(Interface::Helpers,slot);
 let words=[0xe92d4010u32,0xe1a04000,0xe1a00004,0xe59f3014,0xe12fff33,0xe5941028,0xe2811001,0xe5841028,0xeafffff8,0xe1a00000,api];
 f.m.cpu_mut().write_mem(endless,&words.into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>()).unwrap();f.put(f.read(p)+20,endless);let opts=OUT+1024;options(&mut f,opts,&[(0x30001,p)]);f.extra(web,7,[RESPONSE,PCB,URL],&[opts]);let before=f.m.cpu().instructions();let before_sp=f.m.cpu().read_reg(Reg::Sp);let r=wait(&mut f);assert_eq!(f.m.cpu().read_reg(Reg::Sp),before_sp,"aborted provider must restore its entry stack");let consumed=f.m.cpu().instructions()-before;let info=f.call(r,6,[0;3]);assert_eq!(f.read(info),(-0x510i32)as u32);assert!(f.read(p+40)>100,"provider really crossed API boundaries");assert!(consumed<110000,"provider exceeded aggregate100k instruction cap: {consumed}");assert_eq!(f.read(p+4),1);f.call(r,1,[0;3]);f.call(web,1,[0;3]);
}
#[test]fn fault_after_budget_retires_provider_without_replaying_its_failed_peek(){
 let mut f=F::new();let web=setup(&mut f,"http://127.0.0.1:1/fault");let p=waiting(&mut f);let fault=loader::MODULE_BASE+0x900;
 let words=[0xe590c018u32,0xe28cc001,0xe580c018,0xe3a0cc01,0xe25cc001,0x1afffffd,0xe3e01000,0xe5911000];f.m.cpu_mut().write_mem(fault,&words.into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>()).unwrap();f.put(f.read(p)+20,fault);let opts=OUT+1024;options(&mut f,opts,&[(0x30001,p)]);f.extra(web,7,[RESPONSE,PCB,URL],&[opts]);f.m.deliver_signals(100000).unwrap();let first=f.m.deliver_signals(1).unwrap();assert!(first.iter().any(|o|matches!(o,Outcome::Budget)));let resumed=f.m.advance(100000).unwrap();assert!(resumed.iter().any(|o|matches!(o,Outcome::Fault{..})));let r=wait(&mut f);let info=f.call(r,6,[0;3]);assert_eq!(f.read(info),(-0x510i32)as u32);assert_eq!(f.read(p+24),1,"failed resumed Peek must not be interpreted as a normal return");assert_eq!(f.read(p+4),1);f.call(r,1,[0;3]);f.call(web,1,[0;3]);
}

#[test]fn wait_registers_peekable_then_posts_only_the_available_binary_body(){
 let listener=TcpListener::bind((std::net::Ipv4Addr::LOCALHOST,0)).unwrap();listener.set_nonblocking(true).unwrap();let address=listener.local_addr().unwrap();
 let mut f=F::new();let web=setup(&mut f,&format!("http://{address}/stream"));let p=guest_peek(&mut f,b"",-2);
 // The independent guest provider records Peekable's PCB; it becomes readable
 // only when this test explicitly supplies data and invokes that callback.
 let peekable=loader::MODULE_BASE+0x800;f.m.cpu_mut().write_mem(peekable,&[0xe5801020u32,0xe12fff1e].into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>()).unwrap();f.put(f.read(p)+24,peekable);
 let method=OUT+2048;f.m.cpu_mut().write_mem(method,b"POST\0").unwrap();let opts=OUT+1024;options(&mut f,opts,&[(0x10001,method),(0x30001,p),(0x20006,3)]);f.extra(web,7,[RESPONSE,PCB,URL],&[opts]);
 for _ in 0..20{f.m.deliver_signals(100000).unwrap();if f.read(p+32)!=0{break;}}
 assert_ne!(f.read(p+32),0,"WAIT must call documented IPeek_Peekable instead of failing locally");assert_eq!(f.read(CONTEXT),0);assert_eq!(f.read(p+4),2,"QI reference lives while waiting");assert!(matches!(listener.accept(),Err(e)if e.kind()==std::io::ErrorKind::WouldBlock));
 let buffer=f.read(p+8);f.m.cpu_mut().write_mem(buffer,b"A\0\xff").unwrap();f.put(p+12,3);f.put(p+20,0);let ready=f.read(p+32);f.run(f.read(ready+16),[f.read(ready+20),0,0,0]);
 let server=std::thread::spawn(move||{let end=Instant::now()+Duration::from_secs(3);while Instant::now()<end{match listener.accept(){Ok((mut s,_))=>{s.set_nonblocking(false).unwrap();s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();let mut h=Vec::new();let mut b=[0;1];while !h.ends_with(b"\r\n\r\n"){s.read_exact(&mut b).unwrap();h.push(b[0]);}let mut body=[0;3];s.read_exact(&mut body).unwrap();s.write_all(b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\n\r\n").unwrap();return body;},Err(e)if e.kind()==std::io::ErrorKind::WouldBlock=>std::thread::sleep(Duration::from_millis(2)),Err(e)=>panic!("{e}")}}panic!("POST did not reach loopback server")});
 let r=wait(&mut f);let info=f.call(r,6,[0;3]);assert_eq!(f.read(info),202);assert_eq!(server.join().unwrap(),*b"A\0\xff");assert_eq!(f.read(p+4),1);assert_eq!(f.read(p+16),3);f.call(r,1,[0;3]);f.call(web,1,[0;3]);
}
