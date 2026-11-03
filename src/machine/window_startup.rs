//! Negociação de início em janela do Brew MP, antes do APP_START clássico.
//! O adaptador e o AEEAppStart pertencem ao emulador, nunca ao módulo original.
use super::*;
#[derive(Clone, Copy)]
pub(super) struct WindowStartup {
    pub(super) block: u32,
    pub(super) stack: u32,
    pub(super) applet: u32,
}
const BLOCK_BYTES: u32 = 112;
const DECISION: u32 = 24;
const CODE: u32 = 32;
// AAPCS: seis registradores preservam alinhamento de oito bytes nas duas chamadas.
const THUNK: [u32; 20] = [
    0xe92d_41f0, // push {r4-r8,lr}
    0xe1a0_4000, // applet
    0xe1a0_5001, // handler ARM ou Thumb
    0xe1a0_6002, // AEEAppStart
    0xe1a0_7003, // decisão persistente
    0xe3a0_1014, // EVT_APP_START_WINDOW
    0xe3a0_2000,
    0xe3a0_3000,
    0xe12f_ff35, // blx r5
    0xe350_0000,
    0x13a0_1001, // TRUE -> janela, sem APP_START
    0x03a0_1002, // FALSE -> APP_START
    0xe587_1000,
    0x1a00_0004,
    0xe1a0_0004,
    0xe3a0_1000,
    0xe3a0_2000,
    0xe1a0_3006,
    0xe12f_ff35,
    0xe8bd_81f0, // pop {r4-r8,pc}
];
impl<C: CpuBackend> Machine<C> {
    /// Seleção explícita do ambiente virtual/pacote. Relógio handset não implica esta ABI.
    pub fn set_window_start_capability(&mut self, enabled: bool) {
        self.window_start_capable = enabled;
    }

    pub(super) fn cancel_window_startup(&mut self) {
        if let Some(start) = self.window_start.take() {
            self.heap.free(start.block);
        }
    }
    pub(super) fn observe_window_startup(&mut self, returned: bool) -> Result<(), CpuError> {
        let Some(start) = self.window_start else { return Ok(()); };
        match self.cpu.read_u32(start.block + DECISION)? {
            0 => {},
            1 => self.applet_em_background = true,
            2 => self.applet_em_background = false,
            _ => return Err(CpuError("decisão de início em janela inválida".into())),
        }
        // Um retorno de callback aninhado não conclui o adaptador exterior.
        // FALSE pode ter terminado a negociação e ainda estar no APP_START.
        if returned && self.cpu.read_reg(Reg::Sp) == start.stack {
            self.cancel_window_startup();
        }
        Ok(())
    }
    pub(super) fn start_window_capable_applet(
        &mut self, applet: u32, clsid: u32, budget: u64,
    ) -> Result<Outcome, CpuError> {
        if self.window_start.is_some() || self.trecho_interrompido.is_some()
            || self.nesting != 0 || self.current_thread.is_some() {
            return Err(CpuError("início do applet aguarda retorno do guest".into()));
        }
        let stack = self.cpu.read_reg(Reg::Sp);
        if stack & 7 != 0 || stack < loader::STACK_BASE + 24
            || stack > loader::STACK_BASE + loader::STACK_SIZE as u32 {
            return Err(CpuError("pilha de início do applet inválida".into()));
        }
        let handler = self.cpu.read_u32(self.cpu.read_u32(applet)? + 8)?;
        let block = self.heap.alloc(BLOCK_BYTES)
            .ok_or_else(|| CpuError("sem memória para início do applet".into()))?;
        let prepare = (|| {
            let display = self.default_applet_display()?;
            self.cpu.write_mem(block, &[0; BLOCK_BYTES as usize])?;
            self.cpu.write_u32(block + 4, clsid)?;
            self.cpu.write_u32(block + 8, display)?;
            for (i, n) in [0i16, 0, self.screen().width() as i16, self.screen().height() as i16]
                .into_iter().enumerate() {
                self.cpu.write_mem(block + 12 + i as u32 * 2, &n.to_le_bytes())?;
            }
            self.cpu.write_mem(block + CODE,
                &THUNK.into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>())?;
            self.install_app_instance_stub(applet)?;
            Ok::<(),CpuError>(())
        })();
        if let Err(error) = prepare { self.heap.free(block); return Err(error); }
        // Só depois de toda alocação: OOM não substitui a identidade ainda ativa.
        self.current_applet = applet;
        self.applet_class = clsid;
        self.applet_em_background = false;
        if !self.app_history.managed {
            self.app_history.shared.lock().unwrap_or_else(|e|e.into_inner()).classes=vec![clsid];
        }
        self.window_start = Some(WindowStartup {block,stack,applet});
        let result = self.call_guest(block + CODE,[applet,handler,block,block+DECISION],budget);
        match result {
            Ok(outcome) => {
                self.observe_window_startup(matches!(outcome,Outcome::Returned{..}))?;
                Ok(outcome)
            }
            Err(error) => { self.cancel_window_startup(); Err(error) }
        }
    }
    pub(super) fn validate_window_start_state(
        reader: &crate::save_state::Leitor<'_>,
    ) -> Result<Option<WindowStartup>,crate::save_state::Erro> {
        use crate::save_state::Erro;
        let bad = |reason: &str| Erro::Secao {nome:"esc.window_start".into(),motivo:reason.into()};
        if reader.secao("esc.window_start").is_none() {return Ok(None);}
        let values = reader.u32s("esc.window_start")?;
        if values.is_empty() {return Ok(None);}
        if values.len()!=4 || values[0]!=1 {return Err(bad("esperava versão1 e três campos"));}
        let start=WindowStartup{block:values[1],stack:values[2],applet:values[3]};
        if start.stack & 7 != 0 || start.stack < loader::STACK_BASE+24
            || start.stack > loader::STACK_BASE+loader::STACK_SIZE as u32 {
            return Err(bad("pilha de início fora do intervalo"));
        }
        let fields=reader.u32s("esc.numeros")?;
        if start.applet==0 || fields.get(6)!=Some(&start.applet) {
            return Err(bad("applet de início não corresponde ao ativo"));
        }
        if !reader.pares("heap.live")?.iter().any(|&(p,n)|p==start.block&&n>=BLOCK_BYTES) {
            return Err(bad("adaptador de início sem alocação própria"));
        }
        let offset=start.block.checked_sub(loader::HEAP_BASE)
            .ok_or_else(||bad("adaptador fora do heap"))? as usize;
        let heap=reader.secao("mem.heap").ok_or_else(||bad("heap ausente"))?;
        let data=heap.get(offset..offset.checked_add(BLOCK_BYTES as usize).ok_or_else(||bad("adaptador fora do heap"))?)
            .ok_or_else(||bad("adaptador fora do heap"))?;
        let decision=u32::from_le_bytes(data[DECISION as usize..DECISION as usize+4].try_into().unwrap());
        let code=THUNK.into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>();
        if decision>2 || data[CODE as usize..] != code {
            return Err(bad("decisão ou código do adaptador inválido"));
        }
        Ok(Some(start))
    }
}
