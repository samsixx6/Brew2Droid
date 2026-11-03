//! HTTP puro fora do guest: pool fixo, filas limitadas e resultados sem ponteiros guest.
use std::{sync::{Arc,Mutex,atomic::{AtomicBool,AtomicU8,Ordering},mpsc},time::{Duration,Instant}};

pub(super) const MAX_BODY:usize=1024*1024;
pub(super) const MAX_RESPONSE:usize=4*1024*1024;
const MAX_HEADERS:usize=65536;
pub(super) const BAD_URL:i32=0x502;
pub(super) const UNSUP_SCHEME:i32=0x505;
const DNS:i32=0x508;
const CONNECT:i32=0x509;
const SEND:i32=0x50a;
const RECV:i32=0x50b;
const BAD_RESPONSE:i32=0x50c;

pub(super) struct Request {
    pub url:String,pub method:String,pub headers:Vec<(String,String)>,pub body:Vec<u8>,
    pub connect_timeout:Duration,pub destination:Option<String>,pub http10:bool,
}
pub(super) struct Response {pub status:u16,pub headers:Vec<(String,String)>,pub body:Vec<u8>}
#[derive(Debug)]
pub(super) struct Failure {pub code:i32,pub reason:String}
fn fail(code:i32,reason:impl Into<String>)->Failure{Failure{code,reason:reason.into()}}

#[cfg(not(target_os="emscripten"))]
struct Control {cancelled:AtomicBool,phase:AtomicU8,socket:Mutex<Option<std::net::TcpStream>>}
#[cfg(not(target_os="emscripten"))]
impl Control {
    fn cancel(&self){self.cancelled.store(true,Ordering::Release);if let Ok(socket)=self.socket.lock(){if let Some(s)=socket.as_ref(){let _=s.shutdown(std::net::Shutdown::Both);}}}
}
pub(super) struct Pending {
    #[cfg(not(target_os="emscripten"))] control:Arc<Control>,
    receiver:mpsc::Receiver<Result<Response,Failure>>,deadline:Instant,
    #[cfg(not(target_os="emscripten"))] connect_deadline:Instant,
}
impl Pending {
    pub(super) fn started(&self)->bool{
        #[cfg(not(target_os="emscripten"))] {self.control.phase.load(Ordering::Acquire)>0}
        #[cfg(target_os="emscripten")] {false}
    }
    pub fn poll(&self)->Option<Result<Response,Failure>>{
        match self.receiver.try_recv(){
            Ok(result)=>Some(result),
            Err(mpsc::TryRecvError::Disconnected)=>Some(Err(fail(RECV,"worker HTTP terminou sem resultado"))),
            #[cfg(not(target_os="emscripten"))]
            Err(mpsc::TryRecvError::Empty)if self.control.phase.load(Ordering::Acquire)<3&&Instant::now()>=self.connect_deadline=>Some(Err(fail(if self.control.phase.load(Ordering::Acquire)==1{0x507}else{CONNECT},"prazo DNS/conexão HTTP esgotado"))),
            Err(mpsc::TryRecvError::Empty)if Instant::now()>=self.deadline=>Some(Err(fail(RECV,"prazo HTTP esgotado"))),
            Err(mpsc::TryRecvError::Empty)=>None,
        }
    }
}
impl Drop for Pending {
    fn drop(&mut self){#[cfg(not(target_os="emscripten"))] self.control.cancel();}
}

#[cfg(not(target_os="emscripten"))]
struct Job {request:Request,control:Arc<Control>,result:mpsc::SyncSender<Result<Response,Failure>>,deadline:Instant,connect_deadline:Instant}
#[cfg(not(target_os="emscripten"))]
fn pool()->Option<&'static mpsc::SyncSender<Job>>{
    static POOL:std::sync::OnceLock<Option<mpsc::SyncSender<Job>>>=std::sync::OnceLock::new();
    POOL.get_or_init(||{
        let(sender,receiver)=mpsc::sync_channel::<Job>(16);let receiver=Arc::new(Mutex::new(receiver));let mut workers=0;
        for index in 0..4 {
            let receiver=receiver.clone();
            if std::thread::Builder::new().name(format!("zeebx-http-{index}")).spawn(move||loop{
                let job=match receiver.lock(){Ok(r)=>match r.recv(){Ok(job)=>job,Err(_)=>return},Err(_)=>return};
                if job.control.cancelled.load(Ordering::Acquire)||Instant::now()>=job.deadline{continue;}
                let result=transact(&job.request,&job.control,job.deadline,job.connect_deadline);
                if !job.control.cancelled.load(Ordering::Acquire){let _=job.result.try_send(result);}
            }).is_ok(){workers+=1;}
        }
        (workers!=0).then_some(sender)
    }).as_ref()
}
pub(super) fn start(request:Request)->Result<Pending,Failure>{
    #[cfg(target_os="emscripten")]
    {let _=request;Err(fail(UNSUP_SCHEME,"HTTP std::net indisponível no WebAssembly"))}
    #[cfg(not(target_os="emscripten"))]
    {
        // Validar antes de DNS, inclusive o destino alternativo escolhido pelo frontend.
        parse_url(&request.url)?;
        let now=Instant::now();let deadline=now+Duration::from_secs(30);let connect_deadline=now+request.connect_timeout.min(Duration::from_secs(30));let control=Arc::new(Control{cancelled:AtomicBool::new(false),phase:AtomicU8::new(0),socket:Mutex::new(None)});
        let(sender,receiver)=mpsc::sync_channel(1);let job=Job{request,control:control.clone(),result:sender,deadline,connect_deadline};
        pool().ok_or_else(||fail(2,"sem worker HTTP"))?.try_send(job).map_err(|_|fail(2,"fila HTTP cheia"))?;
        Ok(Pending{control,receiver,deadline,connect_deadline})
    }
}

fn authority(authority:&str)->Result<(String,u16),Failure>{
    if authority.is_empty()||authority.contains('@')||authority.bytes().any(|b|b<=32||b==127){return Err(fail(BAD_URL,"autoridade HTTP inválida"));}
    let(host,port)=if let Some(rest)=authority.strip_prefix('['){
        let(end,tail)=rest.split_once(']').ok_or_else(||fail(BAD_URL,"IPv6 sem fechamento"))?;
        (end,if tail.is_empty(){80}else{tail.strip_prefix(':').ok_or_else(||fail(0x504,"porta inválida"))?.parse().map_err(|_|fail(0x504,"porta inválida"))?})
    }else if let Some((host,port))=authority.rsplit_once(':'){
        if host.contains(':'){return Err(fail(BAD_URL,"IPv6 exige colchetes"));}
        (host,port.parse().map_err(|_|fail(0x504,"porta inválida"))?)
    }else{(authority,80)};
    if host.is_empty()||port==0{return Err(fail(BAD_URL,"host/porta vazio"));}Ok((host.into(),port))
}
fn parse_url(url:&str)->Result<(String,u16,String,String),Failure>{
    if url.len()>8192||url.bytes().any(|b|b<=32||b==127){return Err(fail(BAD_URL,"URL fora do limite ou com controles"));}
    let rest=url.strip_prefix("http://").ok_or_else(||fail(UNSUP_SCHEME,"somente HTTP puro é suportado"))?;
    let end=rest.find(['/', '?', '#']).unwrap_or(rest.len());let original=&rest[..end];let(host,port)=authority(original)?;
    let path=rest[end..].split('#').next().unwrap_or("");let path=if path.is_empty(){"/".into()}else if path.starts_with('?'){format!("/{path}")}else{path.into()};
    Ok((host,port,path,original.into()))
}

#[cfg(not(target_os="emscripten"))]
fn transact(request:&Request,control:&Control,deadline:Instant,connect_deadline:Instant)->Result<Response,Failure>{
    use std::{io::Write,net::{TcpStream,ToSocketAddrs}};
    control.phase.store(1,Ordering::Release);
    let(host,port,path,original)=parse_url(&request.url)?;
    let(target,port)=match request.destination.as_deref(){Some(target)if target.contains(':')=>authority(target)?,Some(target)=>(target.into(),port),None=>(host,port)};
    let addresses=(target.as_str(),port).to_socket_addrs().map_err(|e|fail(DNS,e.to_string()))?;
    control.phase.store(2,Ordering::Release);
    let mut stream=None;
    for address in addresses.take(16){
        if control.cancelled.load(Ordering::Acquire){return Err(fail(1,"cancelado"));}
        let remaining=connect_deadline.saturating_duration_since(Instant::now());if remaining.is_zero(){break;}
        if let Ok(socket)=TcpStream::connect_timeout(&address,remaining){stream=Some(socket);break;}
    }
    let mut stream=stream.ok_or_else(||fail(CONNECT,"conexão HTTP recusada ou prazo esgotado"))?;
    control.phase.store(3,Ordering::Release);
    stream.set_read_timeout(Some(Duration::from_millis(100))).map_err(|e|fail(RECV,e.to_string()))?;
    stream.set_write_timeout(Some(Duration::from_millis(100))).map_err(|e|fail(SEND,e.to_string()))?;
    *control.socket.lock().map_err(|_|fail(CONNECT,"socket cancelado"))?=Some(stream.try_clone().map_err(|e|fail(CONNECT,e.to_string()))?);
    if control.cancelled.load(Ordering::Acquire){return Err(fail(1,"cancelado"));}
    let version=if request.http10{"1.0"}else{"1.1"};
    let mut wire=format!("{} {path} HTTP/{version}\r\nHost: {original}\r\nConnection: close\r\n",request.method).into_bytes();
    for(name,value)in &request.headers{wire.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());}
    if request.method=="POST"||!request.body.is_empty(){wire.extend_from_slice(format!("Content-Length: {}\r\n",request.body.len()).as_bytes());}
    wire.extend_from_slice(b"\r\n");wire.extend_from_slice(&request.body);
    let mut sent=0;while sent<wire.len(){
        check(control,deadline,SEND)?;
        match stream.write(&wire[sent..]){Ok(0)=>return Err(fail(SEND,"socket fechou durante envio")),Ok(n)=>sent+=n,Err(e)if matches!(e.kind(),std::io::ErrorKind::WouldBlock|std::io::ErrorKind::TimedOut)=>{},Err(e)=>return Err(fail(SEND,e.to_string()))}
    }
    let mut reader=Reader{stream,bytes:Vec::new(),position:0,received:0,control,deadline};
    for _ in 0..5{
        let line=reader.line(8192)?;let line=std::str::from_utf8(&line).map_err(|_|fail(BAD_RESPONSE,"status não ASCII"))?;
        let mut parts=line.splitn(3,' ');let version=parts.next().unwrap_or("");let code=parts.next().unwrap_or("");
        if !matches!(version,"HTTP/1.0"|"HTTP/1.1")||code.len()!=3{return Err(fail(BAD_RESPONSE,"status inválido"));}
        let status:u16=code.parse().map_err(|_|fail(BAD_RESPONSE,"status inválido"))?;if !(100..=599).contains(&status){return Err(fail(BAD_RESPONSE,"status fora do intervalo"));}
        let mut headers=Vec::<(String,String)>::new();let mut total=line.len()+2;
        loop{let line=reader.line(MAX_HEADERS)?;total+=line.len()+2;if total>MAX_HEADERS{return Err(fail(BAD_RESPONSE,"cabeçalhos excedem limite"));}if line.is_empty(){break;}if headers.len()>=256{return Err(fail(BAD_RESPONSE,"cabeçalhos excedem limite"));}
            let line=String::from_utf8(line).map_err(|_|fail(BAD_RESPONSE,"header inválido"))?;let(name,value)=line.split_once(':').ok_or_else(||fail(BAD_RESPONSE,"header sem dois-pontos"))?;
            if !token(name){return Err(fail(BAD_RESPONSE,"nome de header inválido"));}let value=value.trim();if value.bytes().any(|b|b<32&&b!=9||b==127){return Err(fail(BAD_RESPONSE,"header com controle"));}headers.push((name.into(),value.into()));
        }
        if status<200 {if status==101{return Err(fail(BAD_RESPONSE,"troca de protocolo indisponível"));}continue;}
        let lengths=headers.iter().filter(|(n,_)|n.eq_ignore_ascii_case("Content-Length")).map(|(_,v)|v.parse::<usize>().map_err(|_|fail(BAD_RESPONSE,"Content-Length inválido"))).collect::<Result<Vec<_>,_>>()?;
        if lengths.iter().any(|n|Some(n)!=lengths.first()){return Err(fail(BAD_RESPONSE,"Content-Length conflitante"));}
        let encodings=headers.iter().filter(|(n,_)|n.eq_ignore_ascii_case("Transfer-Encoding")).map(|(_,v)|v.as_str()).collect::<Vec<_>>();
        if encodings.len()>1||(!encodings.is_empty()&&(!encodings[0].eq_ignore_ascii_case("chunked")||!lengths.is_empty())){return Err(fail(BAD_RESPONSE,"framing HTTP ambíguo ou não suportado"));}
        let mut body=Vec::new();
        if matches!(status,204|304){}else if !encodings.is_empty(){
            loop{let line=reader.line(8192)?;let size=std::str::from_utf8(&line).map_err(|_|fail(BAD_RESPONSE,"chunk inválido"))?.split(';').next().unwrap_or("");
                if size.is_empty()||size.len()>16||!size.bytes().all(|b|b.is_ascii_hexdigit()){return Err(fail(BAD_RESPONSE,"chunk inválido"));}
                let size=usize::from_str_radix(size,16).map_err(|_|fail(BAD_RESPONSE,"chunk inválido"))?;
                if size==0{let mut trailers=0;loop{let line=reader.line(MAX_HEADERS)?;trailers+=line.len()+2;if trailers>MAX_HEADERS{return Err(fail(BAD_RESPONSE,"trailers excedem limite"));}if line.is_empty(){break;}}break;}
                if size>MAX_RESPONSE-body.len(){return Err(fail(BAD_RESPONSE,"corpo excede limite"));}body.extend(reader.exact(size)?);if reader.exact(2)?!=b"\r\n"{return Err(fail(BAD_RESPONSE,"chunk sem CRLF"));}
            }
        }else if let Some(&length)=lengths.first(){if length>MAX_RESPONSE{return Err(fail(BAD_RESPONSE,"corpo excede limite"));}body=reader.exact(length)?;
        }else{loop{let remaining=&reader.bytes[reader.position..];body.extend_from_slice(remaining);reader.position=reader.bytes.len();if body.len()>MAX_RESPONSE{return Err(fail(BAD_RESPONSE,"corpo excede limite"));}if !reader.fill()?{break;}}}
        return Ok(Response{status,headers,body});
    }
    Err(fail(BAD_RESPONSE,"respostas intermediárias excedem limite"))
}
pub(super) fn token(value:&str)->bool{!value.is_empty()&&value.bytes().all(|b|b.is_ascii_alphanumeric()||b"!#$%&'*+-.^_`|~".contains(&b))}
#[cfg(not(target_os="emscripten"))]
fn check(control:&Control,deadline:Instant,code:i32)->Result<(),Failure>{if control.cancelled.load(Ordering::Acquire){Err(fail(1,"cancelado"))}else if Instant::now()>=deadline{Err(fail(code,"prazo HTTP esgotado"))}else{Ok(())}}
#[cfg(not(target_os="emscripten"))]
struct Reader<'a>{stream:std::net::TcpStream,bytes:Vec<u8>,position:usize,received:usize,control:&'a Control,deadline:Instant}
#[cfg(not(target_os="emscripten"))]
impl Reader<'_>{
    fn fill(&mut self)->Result<bool,Failure>{use std::io::Read;if self.position!=0{self.bytes.drain(..self.position);self.position=0;}let mut bytes=[0;8192];loop{check(self.control,self.deadline,RECV)?;match self.stream.read(&mut bytes){Ok(0)=>return Ok(false),Ok(n)=>{self.received+=n;if self.received>MAX_RESPONSE*2+MAX_HEADERS*2{return Err(fail(BAD_RESPONSE,"resposta HTTP bruta excede limite"));}self.bytes.extend_from_slice(&bytes[..n]);return Ok(true)},Err(e)if matches!(e.kind(),std::io::ErrorKind::WouldBlock|std::io::ErrorKind::TimedOut)=>{},Err(e)=>return Err(fail(RECV,e.to_string()))}}}
    fn line(&mut self,limit:usize)->Result<Vec<u8>,Failure>{loop{check(self.control,self.deadline,RECV)?;let bytes=&self.bytes[self.position..];if let Some(end)=bytes.windows(2).position(|b|b==b"\r\n"){if end>limit{return Err(fail(BAD_RESPONSE,"linha HTTP excede limite"));}let line=bytes[..end].to_vec();self.position+=end+2;return Ok(line);}if bytes.len()>limit{return Err(fail(BAD_RESPONSE,"linha HTTP excede limite"));}if !self.fill()?{return Err(fail(RECV,"resposta truncada"));}}}
    fn exact(&mut self,count:usize)->Result<Vec<u8>,Failure>{let mut out=Vec::with_capacity(count);while out.len()<count{check(self.control,self.deadline,RECV)?;let n=(count-out.len()).min(self.bytes.len()-self.position);out.extend_from_slice(&self.bytes[self.position..self.position+n]);self.position+=n;if out.len()<count&&!self.fill()?{return Err(fail(RECV,"corpo HTTP truncado"));}}Ok(out)}
}

#[cfg(all(test,not(target_os="emscripten")))]
mod tests{
    use super::*;
    #[test]
    fn fragmentos_de_chunks_nao_acumulam_prefixo_ja_consumido(){
        use std::{net::{TcpListener,TcpStream},io::Write};
        let listener=TcpListener::bind((std::net::Ipv4Addr::LOCALHOST,0)).unwrap();let address=listener.local_addr().unwrap();
        let server=std::thread::spawn(move||{let(mut socket,_)=listener.accept().unwrap();let line=format!("1;ext={}\r\nx\r\n","a".repeat(64));let wire=line.repeat(2000);for fragment in wire.as_bytes().chunks(37){socket.write_all(fragment).unwrap();}});
        let stream=TcpStream::connect(address).unwrap();stream.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        let control=Control{cancelled:AtomicBool::new(false),phase:AtomicU8::new(3),socket:Mutex::new(None)};
        let mut reader=Reader{stream,bytes:Vec::new(),position:0,received:0,control:&control,deadline:Instant::now()+Duration::from_secs(5)};
        for _ in 0..2000{assert!(reader.line(8192).unwrap().starts_with(b"1;ext="));assert_eq!(reader.exact(1).unwrap(),b"x");assert_eq!(reader.exact(2).unwrap(),b"\r\n");assert!(reader.bytes.len()<=16384,"buffer não retém todos os chunks antigos");}
        server.join().unwrap();
    }
}
