//! BODY do aplicativo: uma chamada IPeek por volta, inclusive depois de Budget.
use super::*;
use std::time::{Duration,Instant};

pub(in super::super) struct Builder {
    pub(in super::super) request:Option<http_transport::Request>,
    expected:Option<usize>,deadline:Instant,
}
impl Builder {
    pub(in super::super) fn retired()->Self{Self{request:None,expected:None,deadline:Instant::now()+Duration::from_secs(30)}}
}
const SIZE:u32=128;
const PHASE:u32=0; const OBJECT:u32=4; const PEEK:u32=8;
const AVAILABLE:u32=12; const CODE:u32=16; const WANTED:u32=20;
const COUNT:u32=24; const ERROR:u32=28; const READY:u32=32;
const PCB:u32=36; const ITERATIONS:u32=60; const MAX_ITERATIONS:u32=128;
const INSTRUCTIONS:u32=64;
const ENTRY:u32=72;
const ENTRY_REGS:[Reg;10]=[Reg::R4,Reg::R5,Reg::R6,Reg::R7,Reg::R8,Reg::R9,Reg::R10,Reg::R11,Reg::Sp,Reg::Lr];
const QI:u32=1;const READ:u32=2;const ADVANCE:u32=3;const REGISTER:u32=4;
const WAIT:u32=5;const CANCEL:u32=6;const RELEASE:u32=7;const DONE:u32=8;
const READY_STUB:u32=loader::STUB_BASE+0x100;

impl<C:CpuBackend> Machine<C>{
    pub(in super::super) fn web_body_resume_budget(&self,budget:u64)->u64{
        if self.http_body_pin==0{return budget;}
        let record=self.body_record(self.http_body_pin);
        let start=u64::from(self.cpu.read_u32(record+INSTRUCTIONS).unwrap_or(0))|(u64::from(self.cpu.read_u32(record+INSTRUCTIONS+4).unwrap_or(0))<<32);
        if self.http_bodies.get(&self.http_body_pin).is_none_or(|b|Instant::now()>=b.deadline&&self.cpu.read_u32(record+PHASE).unwrap_or(0)<CANCEL){return 0;}
        budget.min(100_000u64.saturating_sub(self.cpu.instructions().saturating_sub(start)))
    }
    pub(in super::super) fn abort_web_body_call(&mut self)->Result<(),CpuError>{
        let response=std::mem::take(&mut self.http_body_pin);if response==0{return Ok(());}
        let record=self.body_record(response);let phase=self.cpu.read_u32(record+PHASE)?;
        self.body_failure(record,0x510)?;
        // Uma chamada inválida que já alterou sua posse não é repetida.
        if phase==RELEASE{self.cpu.write_u32(record+PEEK,0)?;self.cpu.write_u32(record+PHASE,DONE)?;}
        if phase==CANCEL{self.cpu.write_u32(record+PCB+8,0)?;self.cpu.write_u32(record+PCB+12,0)?;self.cpu.write_u32(record+PHASE,RELEASE)?;}
        for(i,reg)in ENTRY_REGS.into_iter().enumerate(){let value=self.cpu.read_u32(record+ENTRY+i as u32*4)?;self.cpu.write_reg(reg,value);}
        self.cpu.set_cpsr(self.cpu.read_u32(record+ENTRY+40)?);
        Ok(())
    }
    fn body_record(&self,response:u32)->u32{
        let meta=self.cpu.read_u32(response+RESPONSE_META).unwrap_or(0);
        if self.is_web_meta(meta)&&self.heap.size_of(meta).is_some_and(|n|n>=40){self.cpu.read_u32(meta+M_BODY).unwrap_or(0)}else{0}
    }
    pub(in super::super) fn start_web_body(&mut self,response:u32,object:u32,request:http_transport::Request,expected:Option<usize>)->Result<bool,CpuError>{
        let Some(record)=self.heap.alloc(SIZE)else{return Ok(false);};
        self.cpu.write_mem(record,&[0;SIZE as usize])?;
        for(at,value)in[(PHASE,QI),(OBJECT,object),(WANTED,32768),(PCB+16,READY_STUB),(PCB+20,record+READY)]{self.cpu.write_u32(record+at,value)?;}
        // Le pointeur de contexte appartient au registre unique de ce provider;
        // aucun PCB de l'application ou autre requête n'est réutilisé.
        self.cpu.write_mem(READY_STUB,&[0xe3a01001u32,0xe5801000,0xe12fff1e].into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>())?;
        let meta=self.cpu.read_u32(response+RESPONSE_META)?;self.cpu.write_u32(meta+M_BODY,record)?;
        self.objects.add_ref(response);
        self.http_bodies.insert(response,Builder{request:Some(request),expected,deadline:Instant::now()+Duration::from_secs(30)});Ok(true)
    }
    fn body_failure(&mut self,record:u32,code:u32)->Result<(),CpuError>{
        self.cpu.write_u32(record+ERROR,code)?;self.cpu.write_u32(record+PHASE,CANCEL)
    }
    fn body_live(&self,response:u32)->bool{
        let pcb=self.cpu.read_u32(response+CALLBACK).unwrap_or(0);
        self.cpu.read_u32(response+OWNER).unwrap_or(0)!=0
            &&self.cpu.read_u32(pcb+8).ok()==Some(callback::CANCEL_STUB)
            &&self.cpu.read_u32(pcb+12).ok()==Some(pcb)
    }
    pub(in super::super) fn web_body_ready(&self,response:u32)->bool{
        let Some(builder)=self.http_bodies.get(&response)else{return false;};let record=self.body_record(response);
        builder.request.is_none()||!self.network||!self.body_live(response)||Instant::now()>=builder.deadline
            ||self.cpu.read_u32(record+PHASE).ok()!=Some(WAIT)||self.cpu.read_u32(record+READY).ok()==Some(1)
    }
    pub(in super::super) fn finish_web_body_call(&mut self)->Result<(),CpuError>{
        let response=std::mem::take(&mut self.http_body_pin);if response==0{return Ok(());}
        let record=self.body_record(response);if record==0{return Err(CpuError("continuação BODY perdeu scratch".into()));}
        let phase=self.cpu.read_u32(record+PHASE)?;let returned=self.cpu.read_reg(Reg::R0);
        match phase{
            QI=>{let peek=self.cpu.read_u32(record+PEEK)?;if returned==0&&peek!=0{self.cpu.write_u32(record+PHASE,READ)?;}else{self.cpu.write_u32(record+PEEK,0)?;self.body_failure(record,0x510)?;}},
            READ=>{
                let available=self.cpu.read_u32(record+AVAILABLE)? as usize;
                let code=self.cpu.read_u32(record+CODE)? as i32;
                let count=self.cpu.read_u32(record+COUNT)? as usize;
                if available>http_transport::MAX_BODY.saturating_sub(count){self.body_failure(record,2)?;}
                else if available!=0&&returned==0{self.body_failure(record,0x510)?;}
                else{
                    let mut bytes=vec![0;available];
                    if available!=0{self.cpu.read_mem(returned,&mut bytes)?;}
                    if let Some(request)=self.http_bodies.get_mut(&response).and_then(|b|b.request.as_mut()){request.body.extend_from_slice(&bytes);}
                    self.cpu.write_u32(record+COUNT,(count+available)as u32)?;
                    if available!=0{self.cpu.write_u32(record+PHASE,ADVANCE)?;}else{self.body_after_advance(record,code)?;}
                }
            },
            ADVANCE=>{let code=self.cpu.read_u32(record+CODE)? as i32;self.body_after_advance(record,code)?;},
            REGISTER=>{self.cpu.write_u32(record+PHASE,WAIT)?;},
            CANCEL=>{self.cpu.write_u32(record+PCB+8,0)?;self.cpu.write_u32(record+PCB+12,0)?;self.cpu.write_u32(record+PHASE,RELEASE)?;},
            RELEASE=>{self.cpu.write_u32(record+PEEK,0)?;self.cpu.write_u32(record+PHASE,DONE)?;},
            _=>return Err(CpuError("fase inválida na continuação BODY".into())),
        }Ok(())
    }
    fn body_after_advance(&mut self,record:u32,code:i32)->Result<(),CpuError>{
        match code{
            0=>self.cpu.write_u32(record+PHASE,CANCEL)?,
            -1=>self.body_failure(record,0x50a)?,
            -2=>self.cpu.write_u32(record+PHASE,REGISTER)?,
            -3 if self.cpu.read_u32(record+AVAILABLE)?!=0=>self.cpu.write_u32(record+PHASE,READ)?,
            -4=>{if self.cpu.read_u32(record+WANTED)?==0x10000000&&self.cpu.read_u32(record+AVAILABLE)?==0{self.body_failure(record,0x510)?;}else{self.cpu.write_u32(record+WANTED,0x10000000)?;self.cpu.write_u32(record+PHASE,READ)?;}},
            _=>self.body_failure(record,0x510)?,
        }Ok(())
    }
    pub(in super::super) fn deliver_web_body(&mut self,budget:u64)->Result<Option<Outcome>,CpuError>{
        let Some(response)=self.http_bodies.keys().copied().find(|&r|self.web_body_ready(r))else{return Ok(None);};
        let record=self.body_record(response);if record==0{return Err(CpuError("BODY perdeu registro salvo".into()));}
        let live=self.body_live(response);
        if !live{if let Some(builder)=self.http_bodies.get_mut(&response){builder.request=None;}if self.cpu.read_u32(response+OWNER)?!=0{self.discard_pending_web_response(response)?;}}
        let builder=self.http_bodies.get(&response).unwrap();let mut phase=self.cpu.read_u32(record+PHASE)?;
        if phase<CANCEL&&(builder.request.is_none()||!self.network||Instant::now()>=builder.deadline){self.body_failure(record,if !self.network{NET_DOWN as u32}else{1})?;phase=CANCEL;}
        // Transições sem código guest não consomem outra entrega na mesma volta.
        if phase==WAIT{self.cpu.write_u32(record+READY,0)?;self.cpu.write_u32(record+PHASE,READ)?;phase=READ;}
        if phase==CANCEL&&self.cpu.read_u32(record+PCB+8)?==0{self.cpu.write_u32(record+PHASE,RELEASE)?;phase=RELEASE;}
        if phase==RELEASE&&self.cpu.read_u32(record+PEEK)?==0{self.cpu.write_u32(record+PHASE,DONE)?;phase=DONE;}
        if phase==DONE{
            let builder=self.http_bodies.remove(&response).unwrap();let mut error=self.cpu.read_u32(record+ERROR)?;
            if let Some(request)=builder.request{
                if error==0&&builder.expected.is_some_and(|n|n!=request.body.len()){error=0x50d;}
                if live&&error==0{match http_transport::start(request){Ok(pending)=>{self.http_requests.insert(response,pending);},Err(failure)=>{error=failure.code as u32;}}}
            }else{error=1;}
            if live&&error!=0{self.cpu.write_u32(response+INFO,(-(error as i32))as u32)?;}
            let meta=self.cpu.read_u32(response+RESPONSE_META)?;self.cpu.write_u32(meta+M_BODY,0)?;self.heap.free(record);
            self.release_web_response_reference(response)?;return Ok(None);
        }
        let (object,slot,args)=match phase{
            QI=>(self.cpu.read_u32(record+OBJECT)?,2,[0x0100101c,record+PEEK,0]),
            READ=>{let iteration=self.cpu.read_u32(record+ITERATIONS)?;if iteration>=MAX_ITERATIONS{self.body_failure(record,2)?;return Ok(None);}self.cpu.write_u32(record+ITERATIONS,iteration+1)?;self.cpu.write_u32(record+AVAILABLE,0)?;self.cpu.write_u32(record+CODE,(-4i32)as u32)?;(self.cpu.read_u32(record+PEEK)?,5,[self.cpu.read_u32(record+WANTED)?,record+AVAILABLE,record+CODE])},
            ADVANCE=>(self.cpu.read_u32(record+PEEK)?,7,[self.cpu.read_u32(record+AVAILABLE)?,0,0]),
            REGISTER=>{self.cpu.write_u32(record+READY,0)?;(self.cpu.read_u32(record+PEEK)?,6,[record+PCB,0,0])},
            CANCEL=>(record+PCB,u32::MAX,[0;3]),
            RELEASE=>(self.cpu.read_u32(record+PEEK)?,1,[0;3]),
            _=>return Err(CpuError("fase BODY desconhecida".into())),
        };
        let function=if slot==u32::MAX{self.cpu.read_u32(object+8)?}else{let table=self.cpu.read_u32(object)?;self.cpu.read_u32(table.checked_add(slot*4).ok_or_else(||CpuError("vtable BODY inválida".into()))?)?};
        if function==0{self.body_failure(record,0x510)?;if phase==RELEASE{self.cpu.write_u32(record+PHASE,DONE)?;}return Ok(None);}
        self.http_body_pin=response;
        for(i,reg)in ENTRY_REGS.into_iter().enumerate(){self.cpu.write_u32(record+ENTRY+i as u32*4,self.cpu.read_reg(reg))?;}
        self.cpu.write_u32(record+ENTRY+40,self.cpu.cpsr())?;
        let instructions=self.cpu.instructions();self.cpu.write_u32(record+INSTRUCTIONS,instructions as u32)?;self.cpu.write_u32(record+INSTRUCTIONS+4,(instructions>>32)as u32)?;
        let result=self.call_guest(function,[object,args[0],args[1],args[2]],budget.min(100_000));
        match result{Ok(Outcome::Budget)=>{},Ok(Outcome::Returned{..})=>self.finish_web_body_call()?,_=>self.abort_web_body_call()?,}
        result.map(Some)
    }
}
