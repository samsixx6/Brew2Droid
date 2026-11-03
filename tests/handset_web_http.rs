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
#[test]
fn enabled_iweb_get_uses_the_local_server_status_and_body_asynchronously(){
 let listener=TcpListener::bind((std::net::Ipv4Addr::LOCALHOST,0)).unwrap();let address=listener.local_addr().unwrap();listener.set_nonblocking(true).unwrap();
 let server=std::thread::spawn(move||{
  let end=Instant::now()+Duration::from_secs(3);
  while Instant::now()<end{
   match listener.accept(){
    Ok((mut stream,_))=>{stream.set_nonblocking(false).unwrap();stream.set_read_timeout(Some(Duration::from_secs(1))).unwrap();let mut request=Vec::new();let mut one=[0;1];while request.len()<65536{stream.read_exact(&mut one).unwrap();request.push(one[0]);if request.ends_with(b"\r\n\r\n"){break}}
     stream.write_all(b"HTTP/1.1 201 Created\r\nContent-Type: text/plain\r\nContent-Length: 10\r\nConnection: close\r\n\r\nlocal-body").unwrap();return request;}
    Err(e)if e.kind()==std::io::ErrorKind::WouldBlock=>std::thread::sleep(Duration::from_millis(2)),Err(e)=>panic!("accept: {e}"),
   }
  }Vec::new()
 });
 let mut f=F::new();f.m.set_network(true);let web=f.create(0x01005000);callback(&mut f);
 f.m.cpu_mut().write_mem(URL,format!("http://{address}/status?from=brew\0").as_bytes()).unwrap();
 f.extra(web,6,[RESPONSE,PCB,URL],&[0]);
 assert_eq!(f.read(CONTEXT),0,"callback não reentra em GetResponse");assert_eq!(f.read(RESPONSE),0);
 let end=Instant::now()+Duration::from_secs(2);while f.read(CONTEXT)==0&&Instant::now()<end{f.m.deliver_signals(100000).unwrap();std::thread::sleep(Duration::from_millis(2));}
 assert_eq!(f.read(CONTEXT),1);let response=f.read(RESPONSE);assert_ne!(response,0);let info=f.call(response,6,[0;3]);
 assert_eq!(f.read(info),201,"nCode deve vir do servidor real, não EUNSUPPORTED local");
 let source=f.read(info+4);assert_ne!(source,0);assert_eq!(f.read(info+8),10);
 assert_eq!(f.call(source,3,[OUT+2048,32,0]),10);let mut body=[0;10];f.m.cpu().read_mem(OUT+2048,&mut body).unwrap();assert_eq!(&body,b"local-body");assert_eq!(f.call(source,3,[OUT+2048,32,0]),0);
 let request=server.join().unwrap();assert!(request.starts_with(b"GET /status?from=brew HTTP/1."));assert!(String::from_utf8(request).unwrap().contains(&format!("Host: {address}\r\n")));
 f.call(response,1,[0;3]);f.call(web,1,[0;3]);
}

fn setup(f:&mut F,url:&str)->u32{f.m.set_network(true);let web=f.create(0x01005000);callback(f);f.m.cpu_mut().write_mem(URL,format!("{url}\0").as_bytes()).unwrap();web}
fn begin(f:&mut F,web:u32){f.extra(web,6,[RESPONSE,PCB,URL],&[0]);}
fn wait(f:&mut F)->u32{let end=Instant::now()+Duration::from_secs(3);while f.read(CONTEXT)==0&&Instant::now()<end{f.m.deliver_signals(100000).unwrap();std::thread::sleep(Duration::from_millis(2));}assert_eq!(f.read(CONTEXT),1);let response=f.read(RESPONSE);assert_ne!(response,0);response}
fn options(f:&mut F,at:u32,pairs:&[(u32,u32)]){for(i,&(id,value))in pairs.iter().enumerate(){f.put(at+i as u32*8,id);f.put(at+i as u32*8+4,value);}f.put(at+pairs.len()as u32*8,0);}
fn held()->(String,std::sync::mpsc::Receiver<()>,std::sync::mpsc::Receiver<bool>){
 let listener=TcpListener::bind((std::net::Ipv4Addr::LOCALHOST,0)).unwrap();listener.set_nonblocking(true).unwrap();let address=listener.local_addr().unwrap();let(tx,connected)=std::sync::mpsc::channel();let(done,closed)=std::sync::mpsc::channel();
 std::thread::spawn(move||{let end=Instant::now()+Duration::from_secs(3);while Instant::now()<end{match listener.accept(){Ok((mut socket,_))=>{socket.set_nonblocking(false).unwrap();socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();let mut header=Vec::new();let mut byte=[0;1];while !header.ends_with(b"\r\n\r\n"){assert!(header.len()<65536);socket.read_exact(&mut byte).unwrap();header.push(byte[0]);}let _=tx.send(());let mut data=[0;4096];let result=socket.read(&mut data);let _=done.send(matches!(result,Ok(0))||matches!(result,Err(ref e)if matches!(e.kind(),std::io::ErrorKind::ConnectionReset|std::io::ErrorKind::ConnectionAborted)));return},Err(e)if e.kind()==std::io::ErrorKind::WouldBlock=>std::thread::sleep(Duration::from_millis(2)),Err(_)=>return}}});(format!("http://{address}/held"),connected,closed)
}
#[test]fn sdk_option_vector_first_entry_wins_but_new_add_call_overrides(){
 let mut f=F::new();let web=f.create(0x01005000);let opts=OUT+1024;options(&mut f,opts,&[(0x20005,1000),(0x20005,2000)]);assert_eq!(f.call(web,3,[opts,0,0]),0);
 assert_eq!(f.call(web,5,[0x20005,0,OUT]),0);assert_eq!(f.read(OUT+4),1000);assert_eq!(f.call(web,5,[0x20005,1,OUT]),0);assert_eq!(f.read(OUT+4),2000);
 options(&mut f,opts,&[(0x20005,3000)]);assert_eq!(f.call(web,3,[opts,0,0]),0);assert_eq!(f.call(web,5,[0x20005,0,OUT]),0);assert_eq!(f.read(OUT+4),3000);f.call(web,1,[0;3]);
}
#[test]fn post_binary_per_call_override_headers_and_source_lifetime_are_real(){
 let listener=TcpListener::bind((std::net::Ipv4Addr::LOCALHOST,0)).unwrap();let address=listener.local_addr().unwrap();
 let server=std::thread::spawn(move||{let(mut socket,_)=listener.accept().unwrap();socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();let mut bytes=Vec::new();let mut b=[0;1];while !bytes.ends_with(b"\r\n\r\n"){socket.read_exact(&mut b).unwrap();bytes.push(b[0]);}let mut body=[0;5];socket.read_exact(&mut body).unwrap();socket.write_all(b"HTTP/1.1 202 Accepted\r\nContent-Type: application/octet-stream\r\nX-Real: yes\r\nContent-Length: 4\r\n\r\n\x00\xffOK").unwrap();(bytes,body)});
 let mut f=F::new();let web=setup(&mut f,&format!("http://{address}/post"));let get=OUT+2048;let post=get+16;let header=post+16;let payload=OUT+3072;let opts=OUT+1024;
 f.m.cpu_mut().write_mem(get,b"GET\0POST\0").unwrap();f.m.cpu_mut().write_mem(post,b"POST\0").unwrap();f.m.cpu_mut().write_mem(header,b"X-Call: yes\r\nContent-Type: application/octet-stream\r\n\0").unwrap();f.m.cpu_mut().write_mem(payload,b"A\0\xffBZ").unwrap();
 options(&mut f,opts,&[(0x10001,get)]);assert_eq!(f.call(web,3,[opts,0,0]),0);let util=f.create(0x01001011);assert_eq!(f.extra(util,8,[payload,5,0],&[0,OUT+80]),0);let peek=f.read(OUT+80);
 options(&mut f,opts,&[(0x10001,post),(0x10001,get),(0x10002,header),(0x30001,peek),(0x20006,5)]);f.extra(web,7,[RESPONSE,PCB,URL],&[opts]);let response=wait(&mut f);let info=f.call(response,6,[0;3]);assert_eq!(f.read(info),202);assert_eq!(f.m.cpu().read_cstring(f.read(info+12),128),"application/octet-stream");
 assert_eq!(f.call(response,5,[0x10002,0,OUT]),0);assert!(f.m.cpu().read_cstring(f.read(OUT+4),256).contains(':'));
 let source=f.read(info+4);f.call(source,0,[0;3]);f.call(response,1,[0;3]);assert_eq!(f.call(source,3,[payload,32,0]),4);let mut body=[0;4];f.m.cpu().read_mem(payload,&mut body).unwrap();assert_eq!(&body,b"\0\xffOK");f.call(source,1,[0;3]);
 let(headers,body)=server.join().unwrap();assert!(headers.starts_with(b"POST /post HTTP/1.1\r\n"));assert!(String::from_utf8(headers).unwrap().contains("X-Call: yes\r\n"));assert_eq!(&body,b"A\0\xffBZ");f.call(peek,1,[0;3]);f.call(util,1,[0;3]);f.call(web,1,[0;3]);
}
#[test]fn cancel_and_release_close_pending_transport_without_publishing(){for release in [false,true]{let(url,connected,closed)=held();let mut f=F::new();let web=setup(&mut f,&url);begin(&mut f,web);connected.recv_timeout(Duration::from_secs(2)).unwrap();assert_eq!(f.read(RESPONSE),0);
 if release{assert_eq!(f.call(web,1,[0;3]),0);}else{let cancel=f.read(PCB+8);f.run(cancel,[PCB,0,0,0]);}
 f.m.deliver_signals(100000).unwrap();assert_eq!(f.read(CONTEXT),0);assert_eq!(f.read(RESPONSE),0);assert!(closed.recv_timeout(Duration::from_secs(2)).unwrap());if !release{f.call(web,1,[0;3]);}}}
#[test]fn restore_pending_binary_post_reports_local_failure_and_never_reposts(){
 let listener=TcpListener::bind((std::net::Ipv4Addr::LOCALHOST,0)).unwrap();listener.set_nonblocking(true).unwrap();let address=listener.local_addr().unwrap();let(tx,received)=std::sync::mpsc::channel();let(done,finished)=std::sync::mpsc::channel();
 let server=std::thread::spawn(move||{let end=Instant::now()+Duration::from_secs(3);let mut count=0;let mut first=None;while Instant::now()<end{match listener.accept(){Ok((mut socket,_))=>{count+=1;socket.set_nonblocking(false).unwrap();socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();let mut head=Vec::new();let mut b=[0;1];while !head.ends_with(b"\r\n\r\n"){socket.read_exact(&mut b).unwrap();head.push(b[0]);}let mut body=[0;3];socket.read_exact(&mut body).unwrap();assert!(head.starts_with(b"POST "));assert_eq!(&body,b"P\0\xff");let _=tx.send(());first=Some(socket);break},Err(e)if e.kind()==std::io::ErrorKind::WouldBlock=>std::thread::sleep(Duration::from_millis(2)),Err(e)=>panic!("{e}")}}let mut b=[0;1];let closed=first.as_mut().map(|s|matches!(s.read(&mut b),Ok(0)|Err(_))).unwrap_or(false);let end=Instant::now()+Duration::from_millis(200);while Instant::now()<end{if listener.accept().is_ok(){count+=1;}std::thread::sleep(Duration::from_millis(2));}let _=done.send((closed,count));});
 let mut f=F::new();let web=setup(&mut f,&format!("http://{address}/once"));let method=OUT+2048;let payload=OUT+2080;f.m.cpu_mut().write_mem(method,b"POST\0").unwrap();f.m.cpu_mut().write_mem(payload,b"P\0\xff").unwrap();let util=f.create(0x01001011);assert_eq!(f.extra(util,8,[payload,3,0],&[0,OUT+80]),0);let body=f.read(OUT+80);let opts=OUT+1024;options(&mut f,opts,&[(0x10001,method),(0x30001,body),(0x20006,3)]);f.extra(web,7,[RESPONSE,PCB,URL],&[opts]);received.recv_timeout(Duration::from_secs(2)).unwrap();let saved=f.m.grava_estado();f.m.restaura_estado(&saved).unwrap();let response=wait(&mut f);let info=f.call(response,6,[0;3]);assert_eq!(f.read(info),(-1i32)as u32);assert_eq!(f.read(info+4),0);assert_eq!(finished.recv_timeout(Duration::from_secs(2)).unwrap(),(true,1));f.call(response,1,[0;3]);f.call(body,1,[0;3]);f.call(util,1,[0;3]);f.call(web,1,[0;3]);server.join().unwrap();
}
#[test]fn disabling_network_midflight_returns_real_netdown_and_closes_socket(){let(url,connected,closed)=held();let mut f=F::new();let web=setup(&mut f,&url);begin(&mut f,web);connected.recv_timeout(Duration::from_secs(2)).unwrap();f.m.set_network(false);let response=wait(&mut f);let info=f.call(response,6,[0;3]);assert_eq!(f.read(info),(-0x216i32)as u32);assert!(closed.recv_timeout(Duration::from_secs(2)).unwrap());f.call(response,1,[0;3]);f.call(web,1,[0;3]);}
#[test]fn same_callback_reuse_cancels_old_request_and_only_new_response_is_published(){let(url,connected,closed)=held();let mut f=F::new();let web=setup(&mut f,&url);begin(&mut f,web);connected.recv_timeout(Duration::from_secs(2)).unwrap();f.m.cpu_mut().write_mem(URL,b"https://127.0.0.1:1/new\0").unwrap();begin(&mut f,web);let response=wait(&mut f);let info=f.call(response,6,[0;3]);assert_eq!(f.read(info),(-0x505i32)as u32);assert!(closed.recv_timeout(Duration::from_secs(2)).unwrap());for _ in 0..3{f.m.deliver_signals(100000).unwrap();}assert_eq!(f.read(CONTEXT),1);f.call(response,1,[0;3]);f.call(web,1,[0;3]);}
#[test]fn unsupported_redirect_option_is_an_asynchronous_error_for_void_getresponse(){let mut f=F::new();let web=setup(&mut f,"http://127.0.0.1:1/");let opts=OUT+1024;options(&mut f,opts,&[(0x20001,1)]);f.extra(web,7,[RESPONSE,PCB,URL],&[opts]);assert_eq!(f.read(CONTEXT),0);let response=wait(&mut f);let info=f.call(response,6,[0;3]);assert_eq!(f.read(info),(-20i32)as u32);f.call(response,1,[0;3]);f.call(web,1,[0;3]);}
#[test]fn pending_http_has_bounded_poll_delay_and_does_not_block_other_callbacks(){let(url,connected,closed)=held();let mut f=F::new();let web=setup(&mut f,&url);begin(&mut f,web);connected.recv_timeout(Duration::from_secs(2)).unwrap();let delay=f.m.next_guest_work_delay().unwrap();assert!(delay>Duration::ZERO&&delay<=Duration::from_millis(10));
 let pcb2=PCB+128;let context2=CONTEXT+128;let out2=RESPONSE+128;let url2=URL+1024;let function=f.read(PCB+16);f.m.cpu_mut().write_mem(pcb2,&[0;24]).unwrap();f.put(pcb2+16,function);f.put(pcb2+20,context2);f.put(context2,0);f.put(out2,0);f.m.cpu_mut().write_mem(url2,b"https://127.0.0.1:1/ready\0").unwrap();f.extra(web,6,[out2,pcb2,url2],&[0]);assert_eq!(f.m.next_guest_work_delay(),Some(Duration::ZERO));f.m.deliver_signals(100000).unwrap();assert_eq!(f.read(context2),1);assert_eq!(f.read(CONTEXT),0);
 let response=f.read(out2);f.call(response,1,[0;3]);f.call(web,1,[0;3]);assert!(closed.recv_timeout(Duration::from_secs(2)).unwrap());
}

#[test]fn quoted_charset_metadata_and_source_survive_save_and_response_release(){
 let listener=TcpListener::bind((std::net::Ipv4Addr::LOCALHOST,0)).unwrap();let address=listener.local_addr().unwrap();let server=std::thread::spawn(move||{let(mut socket,_)=listener.accept().unwrap();let mut request=[0;4096];socket.read(&mut request).unwrap();socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/plain; note=\"a;b\"; charset=\"utf-8\"\r\nContent-Length: 4\r\n\r\ntext").unwrap();});
 let mut f=F::new();let web=setup(&mut f,&format!("http://{address}/charset"));begin(&mut f,web);let response=wait(&mut f);let info=f.call(response,6,[0;3]);assert_eq!(f.m.cpu().read_cstring(f.read(info+12),128),"text/plain");assert_eq!(f.m.cpu().read_cstring(f.read(info+16),128),"utf-8");let source=f.read(info+4);f.call(source,0,[0;3]);let saved=f.m.grava_estado();f.m.restaura_estado(&saved).unwrap();let info=f.call(response,6,[0;3]);assert_eq!(f.m.cpu().read_cstring(f.read(info+16),128),"utf-8");f.call(response,1,[0;3]);assert_eq!(f.call(source,3,[OUT+2048,16,0]),4);let mut body=[0;4];f.m.cpu().read_mem(OUT+2048,&mut body).unwrap();assert_eq!(&body,b"text");f.call(source,1,[0;3]);f.call(web,1,[0;3]);server.join().unwrap();
}
#[test]fn accepted_close_discards_queued_local_error_callbacks(){let mut f=F::new();let web=setup(&mut f,"https://127.0.0.1:1/closed");begin(&mut f,web);assert!(f.m.encerra_applet().unwrap());assert!(f.m.deliver_signals(100000).unwrap().is_empty());assert_eq!(f.read(CONTEXT),0);assert_eq!(f.read(RESPONSE),0);f.call(web,1,[0;3]);}

#[test]fn sdk_void_status_and_header_handlers_receive_typed_data_before_response(){
 let listener=TcpListener::bind((std::net::Ipv4Addr::LOCALHOST,0)).unwrap();let address=listener.local_addr().unwrap();let server=std::thread::spawn(move||{let(mut socket,_)=listener.accept().unwrap();let mut request=[0;4096];socket.read(&mut request).unwrap();socket.write_all(b"HTTP/1.1 200 OK\r\nX-Seen: yes\r\nContent-Length: 2\r\n\r\nok").unwrap();});
 let mut f=F::new();let web=setup(&mut f,&format!("http://{address}/handlers"));let data=OUT+4096;f.m.cpu_mut().write_mem(data,&[0;32]).unwrap();let status=loader::MODULE_BASE+0x180;let header=loader::MODULE_BASE+0x200;
 f.m.cpu_mut().write_mem(status,&[0xe5901000u32,0xe2811001,0xe5801000,0xe3a000ff,0xe12fff1e].into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>()).unwrap();
 f.m.cpu_mut().write_mem(header,&[0xe5903004u32,0xe2833001,0xe5803004,0xe5923004,0xe5803008,0xe5d23008,0xe580300c,0xe3a000ff,0xe12fff1e].into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>()).unwrap();
 let opts=OUT+1024;options(&mut f,opts,&[(0x20003,status),(0x20004,header),(0x20002,data),(0x20007,120)]);f.extra(web,7,[RESPONSE,PCB,URL],&[opts]);assert_eq!(f.read(data),0);assert_eq!(f.read(CONTEXT),0);let response=wait(&mut f);let info=f.call(response,6,[0;3]);assert!(f.read(data)>=1,"status é um callback void de verdade");assert_eq!(f.read(data+4),2);assert!(f.read(data+8)>0);assert_eq!(f.read(data+12),0,"GetLine completo não é leftover/truncated");assert_eq!(f.read(info),200,"R0 residual255 dos handlers void não altera nCode");f.call(response,1,[0;3]);f.call(web,1,[0;3]);server.join().unwrap();
}

// Implementação ARM independente do ObjectStore: só o contrato público IPeek.
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
#[test]fn guest_defined_ipeek_end_includes_buffered_bytes_and_balances_qi_reference(){
 let listener=TcpListener::bind((std::net::Ipv4Addr::LOCALHOST,0)).unwrap();let address=listener.local_addr().unwrap();let server=std::thread::spawn(move||{let(mut socket,_)=listener.accept().unwrap();socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();let mut head=Vec::new();let mut byte=[0;1];while !head.ends_with(b"\r\n\r\n"){socket.read_exact(&mut byte).unwrap();head.push(byte[0]);}let mut body=[0;3];socket.read_exact(&mut body).unwrap();socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").unwrap();body});
 let mut f=F::new();let web=setup(&mut f,&format!("http://{address}/guestbody"));let provider=guest_peek(&mut f,b"A\0\xff",0);let method=OUT+2048;f.m.cpu_mut().write_mem(method,b"POST\0").unwrap();let opts=OUT+1024;options(&mut f,opts,&[(0x10001,method),(0x30001,provider),(0x20006,3)]);f.extra(web,7,[RESPONSE,PCB,URL],&[opts]);assert_eq!(f.read(provider+24),0,"provider waits for the event loop");
 let response=wait(&mut f);assert_eq!(f.read(provider+4),1);assert_eq!(f.read(provider+16),3);assert_eq!(f.read(provider+24),1);assert_eq!(f.read(provider+28),1);let info=f.call(response,6,[0;3]);assert_eq!(f.read(info),200);assert_eq!(&server.join().unwrap(),b"A\0\xff");f.call(response,1,[0;3]);f.call(web,1,[0;3]);
}
#[test]fn invalid_guest_ipeek_missing_peekable_repeated_unknown_and_error_are_local_failures(){
 for(code,bytes,expected)in[(-2,&b""[..],0x510),(-4,&b""[..],0x510),(-1,&b"x"[..],0x50a)]{
  let listener=TcpListener::bind((std::net::Ipv4Addr::LOCALHOST,0)).unwrap();listener.set_nonblocking(true).unwrap();let address=listener.local_addr().unwrap();let mut f=F::new();let web=setup(&mut f,&format!("http://{address}/unsupportedstream"));let provider=guest_peek(&mut f,bytes,code);let opts=OUT+1024;options(&mut f,opts,&[(0x30001,provider)]);f.extra(web,7,[RESPONSE,PCB,URL],&[opts]);assert_eq!(f.read(CONTEXT),0);let response=wait(&mut f);assert_eq!(f.read(provider+4),1);assert_eq!(f.read(provider+24),if code==-4{2}else{1});assert_eq!(f.read(provider+16),bytes.len()as u32);let info=f.call(response,6,[0;3]);assert_eq!(f.read(info),(-expected as i32)as u32);assert_eq!(f.read(info+4),0);assert!(matches!(listener.accept(),Err(e)if e.kind()==std::io::ErrorKind::WouldBlock));f.call(response,1,[0;3]);f.call(web,1,[0;3]);
 }
}
#[test]fn handler_queue_oom_is_an_async_local_error_instead_of_http_success(){
 for handler_option in [0x20003,0x20004]{
  let listener=TcpListener::bind((std::net::Ipv4Addr::LOCALHOST,0)).unwrap();let address=listener.local_addr().unwrap();let(tx,accepted)=std::sync::mpsc::channel();let server=std::thread::spawn(move||{let(mut socket,_)=listener.accept().unwrap();socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();let mut head=Vec::new();let mut byte=[0;1];while !head.ends_with(b"\r\n\r\n"){assert!(head.len()<65536);socket.read_exact(&mut byte).unwrap();head.push(byte[0]);}let response=if handler_option==0x20004{&b"HTTP/1.1 200 OK\r\nX: v\r\n\r\n"[..]}else{&b"HTTP/1.1 200 OK\r\n\r\n"[..]};let _=socket.write_all(response);tx.send(()).unwrap();});
  let mut f=F::new();let web=setup(&mut f,&format!("http://{address}/oom"));let function=loader::MODULE_BASE+0x180;f.m.cpu_mut().write_mem(function,&0xe12fff1eu32.to_le_bytes()).unwrap();let opts=OUT+1024;options(&mut f,opts,&[(handler_option,function)]);f.extra(web,7,[RESPONSE,PCB,URL],&[opts]);accepted.recv_timeout(Duration::from_secs(5)).unwrap();
  // malloc inclui um byte de folga. Consumir os blocos livres e o avanço deixa
  // a falta de espaço precisamente na fila do handler, depois do transporte real.
  let saved=f.m.grava_estado();let reader=zeebx::save_state::Leitor::abre(&saved).unwrap();let mut fillers=Vec::new();for(_,size)in reader.pares("heap.free_list").unwrap(){fillers.push(f.alloc(size-1));}let remaining=reader.u32("heap.end").unwrap()-reader.u32("heap.next").unwrap();let reserved=if handler_option==0x20004{24}else{0};if remaining>reserved{fillers.push(f.alloc(remaining-reserved-1));}
  assert_eq!(f.read(CONTEXT),0);let response=wait(&mut f);let info=f.call(response,6,[0;3]);assert_eq!(f.read(info),(-2i32)as u32,"o servidor200 não pode sobrescrever a falha de entrega do handler");assert_eq!(f.read(info+4),0);f.call(response,1,[0;3]);let free=(0..256).find(|&slot|Interface::Helpers.method(slot)==Some("free")).unwrap();for filler in fillers{f.run(aee::encode(Interface::Helpers,free),[filler,0,0,0]);}f.call(web,1,[0;3]);server.join().unwrap();
 }
}
