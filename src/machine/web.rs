//! IWeb/IWebResp públicos: transporte HTTP limitado, assíncrono e cancelável.
//! Não são respostas de um servidor. O caminho privado observado no slot11
//! permanece no despachante antigo e não define a vtable do SDK móvel.
use super::*;
#[path="http_body.rs"]
pub(super) mod http_body;
const BAD_PARAMETER:u32=14;
// AEEStdErr.h do Brew MP; o valor legado global do core é diferente.
const OUT_OF_MEMORY:u32=2;
const CLASS_NOT_SUPPORTED:u32=3;
const UNSUPPORTED:u32=20;
const NET_DOWN:i32=0x216;
const PENDING:u32=4;
const OPTIONS:u32=8;
const COPY_OPTIONS:u32=12;
// Campos privados do IWebResp; WebRespInfo ocupa28bytes a partir de+28.
const NEXT:u32=4;
const PREV:u32=8;
const OWNER:u32=12;
const CALLBACK:u32=16;
const OUTPUT:u32=20;
const RESPONSE_OPTIONS:u32=24;
const INFO:u32=28;
const RESPONSE_COPY:u32=60;
const RESPONSE_META:u32=56;
const MAX_OPTIONS:usize=256;
// O byte ff não pode iniciar o MIME UTF-8 simples guardado por saves antigos.
const META_MAGIC:u32=0xff454257;
// A fila e o GetLine vivem no heap salvo do guest, inclusive durante Budget.
const META_SIZE:u32=40;
const M_BODY:u32=36;
const M_STARTED:u32=32;
const M_STATUS:u32=4; const M_HEADER:u32=8; const M_CONTEXT:u32=12;
const M_HEAD:u32=16; const M_TAIL:u32=20; const M_FLIGHT:u32=24; const M_CONTENT:u32=28;

impl<C:CpuBackend> Machine<C>{
    fn web_string(&self,pointer:u32,limit:u32)->Result<String,i32>{
        if pointer==0{return Ok(String::new());}let mut bytes=Vec::new();
        for offset in 0..limit{let mut byte=[0];self.cpu.read_mem(pointer.checked_add(offset).ok_or(14)?,&mut byte).map_err(|_|14)?;if byte[0]==0{return String::from_utf8(bytes).map_err(|_|14);}bytes.push(byte[0]);}Err(14)
    }
    fn snapshot_web_request(&mut self,web:u32,slot:u32,url:&str)->Result<(http_transport::Request,[u32;3],u32,Option<usize>),i32>{
        let read=|p|self.cpu.read_u32(p).map_err(|_|14);
        let mut pairs=Vec::new();let mut node=read(web+OPTIONS)?;
        for _ in 0..MAX_OPTIONS{if node==0{break;}pairs.push((read(node+4)?,read(node+8)?));node=read(node)?;}
        if node!=0{return Err(2);}
        let defaults=pairs;let mut pairs=Vec::new();
        let sp=self.cpu.read_reg(Reg::Sp);let mut pointer=if slot==7{read(sp)?}else{sp};
        if pointer!=0{let mut ended=false;for _ in 0..=MAX_OPTIONS{let id=read(pointer)?;if id==0{ended=true;break;}let value=read(pointer.checked_add(4).ok_or(14)?)?;pairs.push((id,value));pointer=pointer.checked_add(8).ok_or(14)?;}if !ended{return Err(14);}}
        pairs.extend(defaults);
        let mut values=BTreeMap::new();let mut headers=Vec::new();let mut header_bytes=0usize;
        for(id,value)in pairs{
            if id==0x10002{
                for line in self.web_string(value,65536)?.split("\r\n").filter(|s|!s.is_empty()){
                    let(name,value)=line.split_once(':').ok_or(14)?;let value=value.trim();
                    if !http_transport::token(name)||value.bytes().any(|b|b<32&&b!=9||b==127){return Err(14);}
                    header_bytes+=line.len()+2;if header_bytes>65536||headers.len()>=256{return Err(2);}
                    headers.push((name.to_string(),value.to_string()));
                }
            }else{values.entry(id).or_insert(value);}
        }
        for key in [0x10003,0x10004,0x10006,0x10007]{if values.get(&key).copied().unwrap_or(0)!=0{return Err(if key==0x10003{0x50e}else{20});}}
        let method=match values.get(&0x10001){Some(&p)=>self.web_string(p,16)?.to_ascii_uppercase(),None=>"GET".into()};if !matches!(method.as_str(),"GET"|"POST"){return Err(20);}
        let flags=values.get(&0x20001).copied().unwrap_or(0);if flags&!0x23e!=0{return Err(20);}
        let timeout=match values.get(&0x20005).copied().unwrap_or(0){0=>5000,n=>n.min(30000).max(1)};
        let body_pointer=values.get(&0x30001).copied().unwrap_or(0);
        let guest_body=if body_pointer!=0&&!self.peeks.contains_key(&body_pointer)&&!self.sources.contains_key(&body_pointer){body_pointer}else{0};
        let body=if guest_body==0{self.copy_web_body(body_pointer)?}else{Vec::new()};
        let mut expected=values.get(&0x20006).map(|&length|length as usize);
        if guest_body==0&&expected.is_some_and(|length|length!=body.len()){return Err(0x50d);}
        if expected.is_some_and(|length|length>http_transport::MAX_BODY){return Err(2);}
        for(name,value)in &headers{
            if name.eq_ignore_ascii_case("Host")||name.eq_ignore_ascii_case("Transfer-Encoding"){return Err(20);}
            if name.eq_ignore_ascii_case("Connection")&&!value.eq_ignore_ascii_case("close"){return Err(20);}
            if name.eq_ignore_ascii_case("Content-Length"){let length=value.parse::<usize>().map_err(|_|0x50d)?;if length>http_transport::MAX_BODY||expected.is_some_and(|n|n!=length)||guest_body==0&&length!=body.len(){return Err(0x50d);}expected=Some(length);}
        }
        headers.retain(|(name,_)|!name.eq_ignore_ascii_case("Content-Length")&&!name.eq_ignore_ascii_case("Connection"));
        if !headers.iter().any(|(n,_)|n.eq_ignore_ascii_case("User-Agent")){
            let user_agent=match values.get(&0x10005){Some(&0)=>None,Some(&p)=>Some(self.web_string(p,8192)?),None=>Some("Zeebx".into())};
            if let Some(agent)=user_agent{if agent.bytes().any(|b|b<32||b==127){return Err(14);}headers.push(("User-Agent".into(),agent));}
        }
        if flags&4!=0{headers.push(("Pragma".into(),"no-cache".into()));}
        if headers.len()>256||headers.iter().map(|(n,v)|n.len()+v.len()+4).sum::<usize>()>65536{return Err(2);}
        for(&id,&value)in &values{if !matches!(id,3|0x10001|0x10005|0x20001..=0x20007|0x30001)&&value!=0{return Err(20);}}
        // IDLECONNTIMEOUT não tem efeito em GetResponse (AEEWeb.h1177–1181).
        // AddOpt só controla conexões ociosas; este transporte sempre as fecha.
        let handlers=[values.get(&0x20003).copied().unwrap_or(0),values.get(&0x20004).copied().unwrap_or(0),values.get(&0x20002).copied().unwrap_or(0)];
        Ok((http_transport::Request{url:url.into(),method,headers,body,connect_timeout:std::time::Duration::from_millis(timeout as u64),destination:self.network_to.clone(),http10:flags&0x200!=0},handlers,guest_body,expected))
    }
    fn web_provider_call(&mut self,object:u32,slot:u32,args:[u32;3])->Result<u32,i32>{
        if self.nesting>=MAX_NESTING{return Err(0x510);}
        let table=self.cpu.read_u32(object).map_err(|_|0x510)?;
        let function=self.cpu.read_u32(table.checked_add(slot*4).ok_or(0x510)?).map_err(|_|0x510)?;
        if function==0{return Err(0x510);}
        let cpsr=self.cpu.cpsr();let budget=self.orcamento;self.nesting+=1;
        let outcome=self.call_guest_aninhado(function,[object,args[0],args[1],args[2]],100_000);
        self.nesting-=1;self.orcamento=budget;self.cpu.set_cpsr(cpsr);
        match outcome{Ok(Outcome::Returned{code})=>Ok(code),_=>Err(0x510)}
    }
    fn copy_web_body(&mut self,object:u32)->Result<Vec<u8>,i32>{
        if object==0{return Ok(Vec::new());}
        // Fontes nativas antigas já pertencem ao core. O IPeek público também
        // aceita uma implementação do aplicativo: nunca supor seu layout privado.
        if let Some(peek)=self.peeks.get_mut(&object){let body=peek.bytes.get(peek.posicao..).ok_or(0x510)?;if body.len()>http_transport::MAX_BODY{return Err(2);}let body=body.to_vec();peek.posicao=peek.bytes.len();return Ok(body);}
        if let Some(bytes)=self.sources.get(&object){if bytes.len()>http_transport::MAX_BODY{return Err(2);}return Ok(bytes.clone());}
        let scratch=self.heap.alloc(16).ok_or(2)?;
        self.cpu.write_mem(scratch,&[0;16]).map_err(|_|0x510)?;
        let qi=self.web_provider_call(object,2,[0x0100101c,scratch,0]);
        let peek=self.cpu.read_u32(scratch).unwrap_or(0);
        if qi!=Ok(0)||peek==0{self.heap.free(scratch);return Err(0x510);}
        let result=(||{
            let mut body=Vec::new();
            for _ in 0..33{
                self.cpu.write_u32(scratch+4,0).map_err(|_|0x510)?;
                self.cpu.write_u32(scratch+8,(-4i32)as u32).map_err(|_|0x510)?;
                let pointer=self.web_provider_call(peek,5,[32768,scratch+4,scratch+8])?;
                let available=self.cpu.read_u32(scratch+4).map_err(|_|0x510)? as usize;
                let code=self.cpu.read_u32(scratch+8).map_err(|_|0x510)? as i32;
                if available>http_transport::MAX_BODY-body.len()||available>32768{return Err(2);}
                if available!=0{
                    if pointer==0{return Err(0x510);}let start=body.len();body.resize(start+available,0);
                    self.cpu.read_mem(pointer,&mut body[start..]).map_err(|_|0x510)?;
                    self.web_provider_call(peek,7,[available as u32,0,0])?;
                }
                match code{0=>return Ok(body),-1=>return Err(0x50a),-2|-4=>return Err(0x510),-3 if available!=0=>{},_=>return Err(0x510)}
            }Err(2)
        })();
        // QI concedeu exatamente uma referência, mesmo se Peek/Advance falhar.
        let released=self.web_provider_call(peek,1,[0;3]);self.heap.free(scratch);
        if released.is_err(){Err(0x510)}else{result}
    }
    fn is_web_meta(&self,meta:u32)->bool{
        self.heap.size_of(meta).is_some_and(|size|size>=36)
            &&self.cpu.read_u32(meta).ok()==Some(META_MAGIC)
    }
    fn response_meta(&mut self,response:u32,handlers:[u32;3])->Result<bool,CpuError>{
        let Some(meta)=self.heap.alloc(META_SIZE)else{return Ok(false);};self.cpu.write_mem(meta,&[0;META_SIZE as usize])?;
        for(at,value)in[(0,META_MAGIC),(M_STATUS,handlers[0]),(M_HEADER,handlers[1]),(M_CONTEXT,handlers[2])]{self.cpu.write_u32(meta+at,value)?;}
        self.cpu.write_u32(response+RESPONSE_META,meta)?;Ok(true)
    }
    fn queue_web_event(&mut self,response:u32,status:Option<u32>,header:Option<(&str,&str)>)->Result<bool,CpuError>{
        let meta=self.cpu.read_u32(response+RESPONSE_META)?;if !self.is_web_meta(meta){return Ok(true);}
        let function=self.cpu.read_u32(meta+if header.is_some(){M_HEADER}else{M_STATUS})?;if function==0{return Ok(true);}
        let context=self.cpu.read_u32(meta+M_CONTEXT)?;
        let extra=header.map(|(n,v)|12+n.len()+1+v.len()+1).unwrap_or(0);
        let Some(node)=self.heap.alloc(24+extra as u32)else{self.cpu.write_u32(response+INFO,(-2i32)as u32)?;return Ok(false);};
        self.cpu.write_mem(node,&vec![0;24+extra])?;
        let(a1,a2)=if let Some((name,value))=header{
            let line=node+24;let namep=node+36;let valuep=namep+name.len()as u32+1;
            self.cpu.write_mem(namep,name.as_bytes())?;self.cpu.write_mem(valuep,value.as_bytes())?;
            self.cpu.write_u32(line,valuep)?;self.cpu.write_u32(line+4,value.len()as u32)?;(namep,line)
        }else{(status.unwrap_or(0),0)};
        for(at,value)in[(4,function),(8,context),(12,a1),(16,a2)]{self.cpu.write_u32(node+at,value)?;}
        let tail=self.cpu.read_u32(meta+M_TAIL)?;
        if tail==0{self.cpu.write_u32(meta+M_HEAD,node)?;}else{self.cpu.write_u32(tail,node)?;}
        self.cpu.write_u32(meta+M_TAIL,node)?;Ok(true)
    }
    pub(super) fn web_event_ready(&self,response:u32)->bool{
        if self.objects.kind_of(response)!=Some(Interface::WebResp){return false;}
        let meta=self.cpu.read_u32(response+RESPONSE_META).unwrap_or(0);
        self.is_web_meta(meta)&&self.cpu.read_u32(meta+M_HEAD).unwrap_or(0)!=0
    }
    pub(super) fn finish_web_handler(&mut self)->Result<(),CpuError>{
        self.finish_web_body_call()?;
        let response=std::mem::take(&mut self.http_callback_pin);if response==0{return Ok(());}
        if self.objects.kind_of(response)!=Some(Interface::WebResp){return Err(CpuError("pin IWebResp perdeu identidade".into()));}
        let meta=self.cpu.read_u32(response+RESPONSE_META)?;
        if self.is_web_meta(meta){let node=self.cpu.read_u32(meta+M_FLIGHT)?;self.cpu.write_u32(meta+M_FLIGHT,0)?;if node!=0{self.heap.free(node);}}
        self.release_web_response_reference(response)?;Ok(())
    }
    pub(super) fn deliver_web_event(&mut self,response:u32,budget:u64)->Result<Option<Outcome>,CpuError>{
        if !self.web_event_ready(response){return Ok(None);}
        let meta=self.cpu.read_u32(response+RESPONSE_META)?;let node=self.cpu.read_u32(meta+M_HEAD)?;let next=self.cpu.read_u32(node)?;
        self.cpu.write_u32(meta+M_HEAD,next)?;if next==0{self.cpu.write_u32(meta+M_TAIL,0)?;}
        self.cpu.write_u32(meta+M_FLIGHT,node)?;
        let function=self.cpu.read_u32(node+4)?;let mut args=[0;4];for(i,arg)in args.iter_mut().enumerate(){*arg=self.cpu.read_u32(node+8+i as u32*4)?;}
        self.objects.add_ref(response);self.http_callback_pin=response;
        let outcome=self.call_guest(function,args,budget);
        if !matches!(outcome,Ok(Outcome::Budget)){self.finish_web_handler()?;}
        outcome.map(Some)
    }
    fn release_web_response_reference(&mut self,response:u32)->Result<u32,CpuError>{
        if self.objects.contagem(response)==1{self.clear_web_options(response,Interface::WebResp)?;self.clear_web_response_payload(response)?;}
        Ok(self.objects.release(response))
    }
    fn clear_web_response_payload(&mut self,response:u32)->Result<(),CpuError>{
        let source=self.cpu.read_u32(response+INFO+4)?;self.cpu.write_u32(response+INFO+4,0)?;if source!=0{self.release_source(source)?;}
        let meta=self.cpu.read_u32(response+RESPONSE_META)?;self.cpu.write_u32(response+RESPONSE_META,0)?;
        if meta!=0{
            if self.is_web_meta(meta){
                let mut node=self.cpu.read_u32(meta+M_HEAD)?;
                for _ in 0..260{if node==0{break;}let next=self.cpu.read_u32(node)?;self.heap.free(node);node=next;}
                for at in[M_FLIGHT,M_CONTENT]{let pointer=self.cpu.read_u32(meta+at)?;if pointer!=0{self.heap.free(pointer);}}
            }self.heap.free(meta);
        }Ok(())
    }
    fn response_option(&mut self,response:u32,id:u32,value:u32,owned:u32)->Result<bool,CpuError>{
        let Some(node)=self.heap.alloc(16)else{if owned!=0{self.heap.free(owned);}return Ok(false);};
        let head=self.cpu.read_u32(response+RESPONSE_OPTIONS)?;
        for(at,value)in[(0,head),(4,id),(8,value),(12,owned)]{self.cpu.write_u32(node+at,value)?;}self.cpu.write_u32(response+RESPONSE_OPTIONS,node)?;Ok(true)
    }
    fn finish_http_response(&mut self,response:u32,result:Result<http_transport::Response,http_transport::Failure>)->Result<(),CpuError>{
        match result{
            Err(error)=>{self.cpu.write_u32(response+INFO,(-error.code)as u32)?;self.web_requests.insert(format!("HTTP -> falhou {}: {}",error.code,error.reason));}
            Ok(result)=>{
                if !self.queue_web_event(response,Some(5),None)?{return Ok(());}
                let content_type=result.headers.iter().find(|(name,_)|name.eq_ignore_ascii_case("Content-Type")).map(|(_,value)|value.clone());
                for(name,value)in &result.headers{
                    if !self.queue_web_event(response,None,Some((name,value)))?{return Ok(());}
                    let text=format!("{name}: {value}\r\n\0");let Some(buffer)=self.heap.alloc(text.len()as u32)else{self.cpu.write_u32(response+INFO,(-2i32)as u32)?;return Ok(());};self.cpu.write_mem(buffer,text.as_bytes())?;
                    if !self.response_option(response,0x10002,buffer,buffer)?{self.cpu.write_u32(response+INFO,(-2i32)as u32)?;return Ok(());}
                }
                if let Some(content_type)=content_type{
                    let mut pieces=Vec::new();let(mut start,mut quoted,mut escaped)=(0,false,false);
                    for(index,byte)in content_type.bytes().enumerate(){if escaped{escaped=false;continue;}if quoted&&byte==b'\\'{escaped=true;continue;}if byte==b'"'{quoted=!quoted;}if byte==b';'&&!quoted{pieces.push(&content_type[start..index]);start=index+1;}}
                    pieces.push(&content_type[start..]);let mime=pieces[0].trim();
                    let charset=pieces.iter().skip(1).find_map(|part|{let(name,value)=part.split_once('=')?;if !name.trim().eq_ignore_ascii_case("charset"){return None;}let value=value.trim();Some(if value.starts_with('"')&&value.ends_with('"')&&value.len()>=2{value[1..value.len()-1].replace("\\\"","\"").replace("\\\\","\\")}else{value.to_string()})});
                    let mut bytes=mime.as_bytes().to_vec();bytes.push(0);let charset_offset=bytes.len();if let Some(charset)=charset.as_ref(){bytes.extend_from_slice(charset.as_bytes());bytes.push(0);}
                    let Some(buffer)=self.heap.alloc(bytes.len()as u32)else{self.cpu.write_u32(response+INFO,(-2i32)as u32)?;return Ok(());};self.cpu.write_mem(buffer,&bytes)?;let meta=self.cpu.read_u32(response+RESPONSE_META)?;if self.is_web_meta(meta){self.cpu.write_u32(meta+M_CONTENT,buffer)?;}else{self.cpu.write_u32(response+RESPONSE_META,buffer)?;}self.cpu.write_u32(response+INFO+12,buffer)?;if charset.is_some(){self.cpu.write_u32(response+INFO+16,buffer+charset_offset as u32)?;}
                }
                let source=self.new_object(Interface::Source)?;if source==0{self.cpu.write_u32(response+INFO,(-2i32)as u32)?;return Ok(());}
                let length=result.body.len()as u32;self.sources.insert(source,result.body);
                self.cpu.write_u32(response+INFO,result.status as u32)?;self.cpu.write_u32(response+INFO+4,source)?;self.cpu.write_u32(response+INFO+8,length)?;
                self.web_requests.insert(format!("HTTP -> {} ({length} bytes)",result.status));
            }
        }Ok(())
    }
    pub(super) fn poll_web_requests(&mut self)->Result<(),CpuError>{
        for response in self.http_requests.keys().copied().collect::<Vec<_>>(){
            let pcb=self.cpu.read_u32(response+CALLBACK)?;
            // Cancel só muda AEECallback; observar antes de consultar o worker libera
            // socket e slot mesmo quando o outro lado nunca envia um único byte.
            if self.cpu.read_u32(pcb+8).ok()!=Some(callback::CANCEL_STUB)||self.cpu.read_u32(pcb+12).ok()!=Some(pcb){self.discard_pending_web_response(response)?;continue;}
            let meta=self.cpu.read_u32(response+RESPONSE_META)?;
            if self.is_web_meta(meta)&&self.cpu.read_u32(meta+M_STARTED)?==0
                &&self.http_requests.get(&response).is_some_and(|p|p.started()){
                self.cpu.write_u32(meta+M_STARTED,1)?;
                if !self.queue_web_event(response,Some(0),None)?{self.http_requests.remove(&response);continue;}
            }
            let result=if !self.network{Some(Err(http_transport::Failure{code:NET_DOWN,reason:"rede desligada durante pedido".into()}))}else{self.http_requests.get(&response).and_then(|pending|pending.poll())};
            if let Some(result)=result{self.http_requests.remove(&response);self.finish_http_response(response,result)?;}
        }Ok(())
    }
    pub(super) fn cancel_http_requests(&mut self)->Result<(),CpuError>{
        // Inclui falhas locais e respostas concluídas ainda não publicadas. A
        // posse do applet só passa ao chamador quando seu callback é entregue.
        let mut responses=self.http_requests.keys().copied().collect::<BTreeSet<_>>();
        responses.extend(self.http_bodies.keys().copied());
        responses.extend(self.deferred_callbacks.iter().filter_map(|&(_,owner)|
            (self.objects.kind_of(owner)==Some(Interface::WebResp)).then_some(owner)));
        for response in responses{self.discard_pending_web_response(response)?;}Ok(())
    }
    fn release_web_option(&mut self,id:u32,owned:u32)->Result<(),CpuError>{
        if owned!=0{if id==0x30001{if self.objects.kind_of(owned).is_some(){self.release_source(owned)?;}else{self.web_provider_call(owned,1,[0;3]).map_err(|_|CpuError("Release do BODY não retornou".into()))?;}}else{self.heap.free(owned);}}Ok(())
    }
    fn web_option_fields(iface:Interface)->(u32,u32){
        if iface==Interface::Web {(OPTIONS,COPY_OPTIONS)}else{(RESPONSE_OPTIONS,RESPONSE_COPY)}
    }
    fn clear_web_options(&mut self,object:u32,iface:Interface)->Result<(),CpuError>{
        let (field,_)=Self::web_option_fields(iface);let mut node=self.cpu.read_u32(object+field)?;
        self.cpu.write_u32(object+field,0)?;
        for _ in 0..MAX_OPTIONS {
            if node==0{break;}
            let next=self.cpu.read_u32(node)?;let owned=self.cpu.read_u32(node+12)?;let id=self.cpu.read_u32(node+4)?;
            self.release_web_option(id,owned)?;
            self.heap.free(node);node=next;
        }
        Ok(())
    }
    fn web_options_call(&mut self,object:u32,iface:Interface,slot:u32)->Result<u32,CpuError>{
        let (field,copy_field)=Self::web_option_fields(iface);
        let (id,index,out)=(self.arg(1),self.arg(2),self.arg(3));
        if slot==3{
            if id==0{return Ok(BAD_PARAMETER);}
            // O limite pertence ao objeto inteiro, não a cada AddOpt. Validar
            // antes de alocar conserva o estado anterior quando o pedido não cabe.
            let mut existing=0usize;
            let mut current=self.cpu.read_u32(object+field)?;
            while current!=0 && existing<MAX_OPTIONS {
                existing+=1;current=self.cpu.read_u32(current)?;
            }
            if current!=0{return Ok(OUT_OF_MEMORY);}
            let mut staged=Vec::<(u32,u32,u32)>::new();
            let mut copy=self.cpu.read_u32(object+copy_field)?!=0;
            let result=(||->Result<u32,CpuError>{
                for at in 0..=MAX_OPTIONS{
                    let item=id.checked_add(at as u32*8).ok_or_else(||CpuError("xOpt fora da memória".into()))?;
                    let option=self.cpu.read_u32(item)?;
                    if option==0{return Ok(SUCCESS);}
                    let value=self.cpu.read_u32(item+4)?;
                    if option==3{
                        if value==0&&(!staged.is_empty()||self.cpu.read_u32(object+field)?!=0){return Ok(EFAILED);}
                        if !copy&&value!=0&&self.cpu.read_u32(object+field)?!=0{return Ok(UNSUPPORTED);}
                        copy=value!=0;continue;
                    }
                    if existing+staged.len()>=MAX_OPTIONS{return Ok(OUT_OF_MEMORY);}
                    let (value,owned)=match option{
                        0x0002_0000..=0x0002_ffff=>(value,0),
                        0x0001_0000..=0x0001_ffff=>{
                            if !copy||value==0{(value,0)}else{
                                let mut bytes=Vec::new();
                                for offset in 0..65536u32{
                                    let mut byte=[0];self.cpu.read_mem(value.checked_add(offset).ok_or_else(||CpuError("xOpt fora da memória".into()))?,&mut byte)?;
                                    bytes.push(byte[0]);if byte[0]==0{break;}
                                }
                                if bytes.last()!=Some(&0){return Ok(BAD_PARAMETER);}
                                let buffer=self.malloc(bytes.len()as u32)?;
                                if buffer==0{return Ok(OUT_OF_MEMORY);}
                                self.cpu.write_mem(buffer,&bytes)?;(buffer,buffer)
                            }
                        }
                        0x30001=>{
                            if value==0{(0,0)}else if matches!(self.objects.kind_of(value),Some(Interface::Source|Interface::Peek)){
                                if copy{self.objects.add_ref(value);(value,value)}else{(value,0)}
                            }else if copy{if self.web_provider_call(value,0,[0;3]).is_err(){return Ok(UNSUPPORTED);}(value,value)}else{(value,0)}
                        }
                        // Tipos de interface/buffer/ASN.1 precisam de seu contrato de
                        // cópia e liberação. Não aceitamos sem efetuar essa posse.
                        _=>return Ok(UNSUPPORTED),
                    };
                    staged.push((option,value,owned));
                }
                Ok(BAD_PARAMETER)
            })();
            if !matches!(result,Ok(SUCCESS)){
                for (id,_,owned)in staged{self.release_web_option(id,owned)?;}
                return result;
            }
            let mut nodes=Vec::new();
            for _ in 0..staged.len(){
                let Some(node)=self.heap.alloc(16)else{
                    for node in nodes{self.heap.free(node);}for(id,_,owned)in staged{self.release_web_option(id,owned)?;}
                    return Ok(OUT_OF_MEMORY);
                };nodes.push(node);
            }
            let mut head=self.cpu.read_u32(object+field)?;
            // IxOpts conserva a primeira entrada do vetor como índice0. Um AddOpt
            // posterior ganha prioridade sobre os vetores já existentes.
            for ((id,value,owned),node)in staged.into_iter().zip(nodes).rev(){
                for(at,value)in[(0,head),(4,id),(8,value),(12,owned)]{self.cpu.write_u32(node+at,value)?;}
                head=node;
            }
            self.cpu.write_u32(object+field,head)?;self.cpu.write_u32(object+copy_field,u32::from(copy))?;
            return Ok(SUCCESS);
        }
        if id==3{
            if index!=0||self.cpu.read_u32(object+copy_field)?==0{return Ok(EFAILED);}
            if slot==4{
                if self.cpu.read_u32(object+field)?!=0{return Ok(EFAILED);}
                self.cpu.write_u32(object+copy_field,0)?;return Ok(SUCCESS);
            }
            if out==0{return Ok(BAD_PARAMETER);}
            self.cpu.write_u32(out,3)?;self.cpu.write_u32(out+4,1)?;return Ok(SUCCESS);
        }
        let(mut node,mut prev,mut found)=(self.cpu.read_u32(object+field)?,0,0);
        for _ in 0..MAX_OPTIONS{
            if node==0{break;}
            let next=self.cpu.read_u32(node)?;let option=self.cpu.read_u32(node+4)?;
            if id==1||option==id{
                if found==index{
                    if slot==4{
                        self.cpu.write_u32(if prev==0{object+field}else{prev},next)?;
                        let owned=self.cpu.read_u32(node+12)?;self.release_web_option(option,owned)?;self.heap.free(node);
                    }else{
                        if out==0{return Ok(BAD_PARAMETER);}
                        let value=self.cpu.read_u32(node+8)?;
                        self.cpu.write_u32(out,option)?;self.cpu.write_u32(out+4,value)?;
                    }
                    return Ok(SUCCESS);
                }found+=1;
            }prev=node;node=next;
        }
        Ok(EFAILED)
    }
    fn unlink_web_response(&mut self,response:u32)->Result<(),CpuError>{
        let owner=self.cpu.read_u32(response+OWNER)?;if owner==0{return Ok(());}
        let next=self.cpu.read_u32(response+NEXT)?;let prev=self.cpu.read_u32(response+PREV)?;
        if prev==0{if self.cpu.read_u32(owner+PENDING)?==response{self.cpu.write_u32(owner+PENDING,next)?;}}
        else{self.cpu.write_u32(prev+NEXT,next)?;}
        if next!=0{self.cpu.write_u32(next+PREV,prev)?;}
        for at in[NEXT,PREV,OWNER]{self.cpu.write_u32(response+at,0)?;}Ok(())
    }
    pub(super) fn discard_pending_web_response(&mut self,response:u32)->Result<(),CpuError>{
        self.http_requests.remove(&response);
        if let Some(body)=self.http_bodies.get_mut(&response){body.request=None;}
        if self.objects.kind_of(response)!=Some(Interface::WebResp)||self.cpu.read_u32(response+OWNER)?==0{return Ok(());}
        self.unlink_web_response(response)?;self.cancel_callbacks_of(response)?;
        self.release_web_response_reference(response)?;Ok(())
    }
    pub(super) fn publish_pending_web_response(&mut self,response:u32)->Result<(),CpuError>{
        if self.objects.kind_of(response)!=Some(Interface::WebResp){return Ok(());}
        let out=self.cpu.read_u32(response+OUTPUT)?;
        self.unlink_web_response(response)?;
        if out!=0{self.cpu.write_u32(out,response)?;}
        self.cpu.write_u32(response+CALLBACK,0)?;self.cpu.write_u32(response+OUTPUT,0)?;
        Ok(())
    }
    fn begin_web_response(&mut self,web:u32,slot:u32)->Result<u32,CpuError>{
        let(out,pcb,url_pointer)=(self.arg(1),self.arg(2),self.arg(3));
        if out==0||pcb==0||self.cpu.read_u32(pcb+16).unwrap_or(0)==0{return Ok(BAD_PARAMETER);}
        self.cpu.write_u32(out,0)?;
        let url=self.cpu.read_cstring(url_pointer,65536);
        // Um save de pedido em voo retoma com falha local, jamais reenvia um POST.
        let initial=if !url.starts_with("http://")&&!url.starts_with("https://"){-0x502}
            else if !self.network{-NET_DOWN}else{-1};
        let response=self.new_object(Interface::WebResp)?;if response==0{return Ok(OUT_OF_MEMORY);}
        self.cpu.write_mem(response+4,&[0;60])?;
        let head=self.cpu.read_u32(web+PENDING)?;
        for(at,value)in[(NEXT,head),(OWNER,web),(CALLBACK,pcb),(OUTPUT,out),(INFO,initial as u32),(INFO+8,u32::MAX),(INFO+20,u32::MAX),(INFO+24,u32::MAX),(RESPONSE_COPY,1)]{
            self.cpu.write_u32(response+at,value)?;
        }
        if head!=0{self.cpu.write_u32(head+PREV,response)?;}
        self.cpu.write_u32(web+PENDING,response)?;
        if let Err(error)=self.schedule_callback(pcb,response){self.discard_pending_web_response(response)?;return Err(error);}
        if initial==-1{
            // O BODY pode reentrar no IWeb. A referência impede reutilizar o ID;
            // depois de cada extração não iniciamos rede de uma inscrição retirada.
            self.objects.add_ref(response);
            let request=self.snapshot_web_request(web,slot,&url);
            let alive=self.cpu.read_u32(response+OWNER)?==web&&self.deferred_callbacks.iter().any(|&(p,o)|p==pcb&&o==response)
                &&self.cpu.read_u32(pcb+8).ok()==Some(callback::CANCEL_STUB)&&self.cpu.read_u32(pcb+12).ok()==Some(pcb);
            if !alive{if self.cpu.read_u32(response+OWNER)?!=0{self.discard_pending_web_response(response)?;}self.release_web_response_reference(response)?;return Ok(SUCCESS);}
            let pending=match request{
                Ok((request,handlers,body,expected))if self.http_requests.len()+self.http_bodies.len()<4=>{
                    if !self.response_meta(response,handlers)?{Err(http_transport::Failure{code:2,reason:"metadados HTTP sem memória".into()})}else if body!=0{
                        if !self.start_web_body(response,body,request,expected)?{Err(http_transport::Failure{code:2,reason:"builder BODY sem memória".into()})}else{self.release_web_response_reference(response)?;return Ok(SUCCESS);}
                    }else{http_transport::start(request)}
                },
                Ok(_)=>Err(http_transport::Failure{code:2,reason:"limite de pedidos HTTP ativos".into()}),
                Err(code)=>Err(http_transport::Failure{code,reason:"opção ou pedido HTTP não suportado/inválido".into()}),
            };
            match pending{Ok(pending)=>{self.http_requests.insert(response,pending);},Err(error)=>{self.cpu.write_u32(response+INFO,(-error.code)as u32)?;self.web_requests.insert(format!("{url} -> falha local {}: {}",error.code,error.reason));}}
            self.release_web_response_reference(response)?;
        }else{self.web_requests.insert(format!("{url} -> falha local {}",-initial));}
        Ok(SUCCESS)
    }
    pub(super) fn web_call(&mut self,iface:Interface,slot:u32)->Result<Option<u32>,CpuError>{
        if iface==Interface::Web&&slot>=8{return self.crypto_call(iface,slot);}
        let this=self.arg(0);
        if self.objects.kind_of(this)!=Some(iface){return Ok(Some(BAD_PARAMETER));}
        let result=match slot{
            0=>self.objects.add_ref(this),
            1=>{
                if iface==Interface::WebResp{
                    let refs=self.objects.contagem(this);
                    if self.cpu.read_u32(this+OWNER)?!=0&&refs<=1+u32::from(self.http_callback_pin==this)+u32::from(self.http_bodies.contains_key(&this)){
                        self.http_requests.remove(&this);if let Some(body)=self.http_bodies.get_mut(&this){body.request=None;}self.unlink_web_response(this)?;self.cancel_callbacks_of(this)?;
                    }
                    return self.release_web_response_reference(this).map(Some);
                }
                if self.objects.contagem(this)==1{
                    if iface==Interface::Web{
                        for _ in 0..1024{let response=self.cpu.read_u32(this+PENDING)?;if response==0{break;}self.discard_pending_web_response(response)?;}
                    }else if self.cpu.read_u32(this+OWNER)?!=0{
                        self.http_requests.remove(&this);
                        self.unlink_web_response(this)?;self.cancel_callbacks_of(this)?;
                    }
                    self.clear_web_options(this,iface)?;
                    if iface==Interface::WebResp{
                        self.clear_web_response_payload(this)?;
                    }
                }
                self.objects.release(this)
            }
            2=>{
                let iid=self.arg(1);let out=self.arg(2);if out==0{return Ok(Some(BAD_PARAMETER));}
                let expected=if iface==Interface::Web{0x01005000}else{0x01005002};
                let supported=matches!(iid,0x01000001|0x0102c269)||iid==expected;
                self.cpu.write_u32(out,if supported{this}else{0})?;
                if supported{self.objects.add_ref(this);SUCCESS}else{CLASS_NOT_SUPPORTED}
            }
            3..=5=>self.web_options_call(this,iface,slot)?,
            6 if iface==Interface::WebResp=>this+INFO,
            6|7 if iface==Interface::Web=>self.begin_web_response(this,slot)?,
            _=>return Ok(None),
        };Ok(Some(result))
    }
}

#[cfg(all(test,not(target_os="emscripten")))]
mod tests{
    use super::*;
    use crate::cpu::BackendPadrao;
    use std::{net::TcpListener,io::{Read,Write},time::{Duration,Instant}};
    #[test]
    fn fechar_applet_retira_resposta_http_concluida_que_ainda_aguarda_callback(){
        let listener=TcpListener::bind((std::net::Ipv4Addr::LOCALHOST,0)).unwrap();let address=listener.local_addr().unwrap();
        let server=std::thread::spawn(move||{for _ in 0..2{let(mut socket,_)=listener.accept().unwrap();socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();let mut request=Vec::new();let mut b=[0;1];while !request.ends_with(b"\r\n\r\n"){socket.read_exact(&mut b).unwrap();request.push(b[0]);}socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").unwrap();}});
        let root=std::env::temp_dir().join(format!("zeebx-http-close-{}",std::process::id()));let storage=crate::storage::StoragePaths::from_root(root.clone());storage.create_dirs().unwrap();
        let image=loader::modfile::ModImage::parse([0xe3a00000u32,0xe12fff1e].into_iter().flat_map(u32::to_le_bytes).collect()).unwrap();let mut m=Machine::new_with_storage(BackendPadrao::new().unwrap(),loader::load_with(&image,&[]).unwrap(),&root,&storage,None);assert!(matches!(m.run(1000).unwrap(),Outcome::Returned{code:0}));m.set_network(true);
        let area=m.malloc(4096).unwrap();let out=area;let entry=aee::encode(Interface::Shell,2);assert!(matches!(m.call_guest(entry,[loader::OBJECT_BASE,0x01005000,out,0],100000).unwrap(),Outcome::Returned{code:0}));let web=m.cpu.read_u32(out).unwrap();
        let callback=loader::MODULE_BASE+0x100;m.cpu.write_mem(callback,&[0xe5901000u32,0xe2811001,0xe5801000,0xe12fff1e].into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>()).unwrap();let url=area+256;m.cpu.write_mem(url,format!("http://{address}/close\0").as_bytes()).unwrap();
        let mut pcbs=Vec::new();let mut counters=Vec::new();let mut outputs=Vec::new();
        for i in 0..2{let pcb=area+32+i*32;let counter=area+160+i*4;let output=area+192+i*4;m.cpu.write_mem(pcb,&[0;24]).unwrap();m.cpu.write_u32(pcb+16,callback).unwrap();m.cpu.write_u32(pcb+20,counter).unwrap();m.cpu.write_u32(m.cpu.read_reg(Reg::Sp),0).unwrap();assert!(matches!(m.call_guest(aee::encode(Interface::Web,6),[web,output,pcb,url],100000).unwrap(),Outcome::Returned{..}));pcbs.push(pcb);counters.push(counter);outputs.push(output);}
        let end=Instant::now()+Duration::from_secs(2);while !m.http_requests.is_empty()&&Instant::now()<end{m.poll_web_requests().unwrap();std::thread::sleep(Duration::from_millis(2));}assert!(m.http_requests.is_empty());assert_eq!(m.deferred_callbacks.len(),2);
        assert_eq!(m.deliver_signals(100000).unwrap().len(),1);assert_eq!(m.cpu.read_u32(counters[0]).unwrap(),1);let second=m.deferred_callbacks[0].1;assert_eq!(m.cpu.read_u32(second+INFO).unwrap(),200);let source=m.cpu.read_u32(second+INFO+4).unwrap();assert_ne!(source,0);
        assert!(m.encerra_applet().unwrap());assert!(m.deliver_signals(100000).unwrap().is_empty());assert_eq!(m.cpu.read_u32(counters[1]).unwrap(),0);assert_eq!(m.cpu.read_u32(outputs[1]).unwrap(),0);assert!(m.objects.kind_of(second).is_none());assert!(!m.sources.contains_key(&source));assert_eq!(m.cpu.read_u32(pcbs[1]+8).unwrap(),0);
        server.join().unwrap();drop(m);std::fs::remove_dir_all(root).unwrap();
    }
}
