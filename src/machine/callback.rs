//! AEECallback mantém identidade e cancelamento até a próxima volta do laço.
use super::*;

pub(super) const CANCEL_STUB: u32 = loader::STUB_BASE + 0xc0;
const SYSTEM_CANCEL_STUB: u32 = loader::STUB_BASE + 0xe0;
pub(super) const MAX_SYSTEM_CALLBACKS: usize = 256;
pub(super) const LOW_RAM_THRESHOLD: u32 = 64 * 1024;

impl<C: CpuBackend> Machine<C> {
    /// Registrar pressão de memória, armazenamento ou fechamento não executa o callback.
    pub(super) fn register_system_callback(&mut self) -> Result<Option<u32>, CpuError> {
        let (pcb, event) = (self.arg(1), self.arg(2));
        // O espaço virtual reportado pelo sistema de arquivos é estável e alto;
        // OnLowStorage continua cancelável, sem confundir falta de RAM com disco.
        // APP_CLOSED tem uma transição real em encerra_applet. Saída do BREW e
        // descarga de módulo continuam fora deste modelo de ciclo de vida.
        if !matches!(event, 2 | 3 | 4 | 7) { return Ok(None); }
        if pcb == 0 { return Ok(Some(0)); }
        let mut bytes = [0; CALLBACK_SIZE as usize];
        self.cpu.read_mem(pcb, &mut bytes)?;
        if self.nesting >= MAX_NESTING {
            return Err(CpuError("registro de callback aninhado demais".into()));
        }
        // Reserva antes de chamar qualquer cancelador, que pode registrar outro PCB.
        // Mesmo uma tentativa cancelada consome sua geração; nunca reutilizar por wrap.
        let generation = self.next_system_callback_generation.checked_add(1)
            .ok_or_else(|| CpuError("gerações de callback de sistema esgotadas".into()))?;
        self.next_system_callback_generation = generation;
        let cancel = self.cpu.read_u32(pcb + 8)?;
        if cancel != 0 {
            self.nesting += 1;
            let outer_budget = self.orcamento;
            let outcome = self.call_guest_aninhado(cancel, [pcb, 0, 0, 0], QSORT_BUDGET);
            self.orcamento = outer_budget;
            self.nesting -= 1;
            if !matches!(outcome?, Outcome::Returned { .. }) {
                return Err(CpuError("cancelamento do callback de sistema não retornou".into()));
            }
        }
        self.system_callbacks.remove(&pcb);
        self.system_callback_generations.remove(&pcb);
        // Retira entradas canceladas antes de aplicar o limite do provedor.
        self.system_callbacks.retain(|pcb, kind| {
            self.cpu.read_u32(*pcb + 8).ok() == Some(SYSTEM_CANCEL_STUB)
                && self.cpu.read_u32(*pcb + 12).ok() == Some(*kind + 1)
        });
        self.system_callback_generations.retain(|pcb, _| self.system_callbacks.contains_key(pcb));
        if self.system_callbacks.len() >= MAX_SYSTEM_CALLBACKS {
            return Err(CpuError("limite de callbacks de sistema excedido".into()));
        }
        let code = [0xe3a01000u32, 0xe5801008, 0xe580100c, 0xe12fff1e];
        self.cpu.write_mem(SYSTEM_CANCEL_STUB,
            &code.into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>())?;
        self.cpu.write_u32(pcb + 8, SYSTEM_CANCEL_STUB)?;
        self.cpu.write_u32(pcb + 12, event + 1)?;
        self.system_callbacks.insert(pcb, event);
        self.system_callback_generations.insert(pcb, generation);
        Ok(Some(0))
    }

    /// A pressão do heap entrega primeiro LOW_RAM; CRITICAL só é tentado se não bastar.
    pub(super) fn deliver_low_ram(&mut self, event: u32) -> Result<bool, CpuError> {
        self.deliver_system_callbacks(event)
    }

    /// Retira a identidade antes da chamada e visita apenas o snapshot desta transição.
    /// O mesmo callback pode se registrar de novo sem execução recursiva nesta entrega.
    pub(super) fn deliver_system_callbacks(&mut self, event: u32) -> Result<bool, CpuError> {
        if self.low_ram_dispatching || self.system_callbacks.is_empty() || self.nesting >= MAX_NESTING { return Ok(false); }
        let entries: Vec<_> = self.system_callbacks.iter()
            .filter(|(_, kind)| **kind == event)
            .filter_map(|(pcb, _)| self.system_callback_generations.get(pcb)
                .map(|generation| (*pcb, *generation))).collect();
        self.low_ram_dispatching = true;
        let result = (|| {
            let mut delivered = false;
            for (pcb, generation) in entries {
                if self.system_callbacks.get(&pcb) != Some(&event)
                    || self.system_callback_generations.get(&pcb) != Some(&generation) { continue; }
                self.system_callbacks.remove(&pcb);
                self.system_callback_generations.remove(&pcb);
                if self.cpu.read_u32(pcb + 8).ok() != Some(SYSTEM_CANCEL_STUB)
                    || self.cpu.read_u32(pcb + 12).ok() != Some(event + 1) { continue; }
                let function = self.cpu.read_u32(pcb + 16)?;
                let context = self.cpu.read_u32(pcb + 20)?;
                self.cpu.write_u32(pcb + 8, 0)?;
                self.cpu.write_u32(pcb + 12, 0)?;
                if function == 0 { continue; }
                self.nesting += 1;
                // A limpeza síncrona precisa terminar antes de continuar a transição.
                // O teto próprio não herda a pequena fatia restante do chamador.
                let outer_budget = self.orcamento;
                let outcome = self.call_guest_aninhado(function, [context, 0, 0, 0], QSORT_BUDGET);
                self.orcamento = outer_budget;
                self.nesting -= 1;
                if !matches!(outcome?, Outcome::Returned { .. }) {
                    return Err(CpuError("callback de sistema não retornou".into()));
                }
                delivered = true;
                if self.applet_privilege_denied { break; }
            }
            Ok(delivered)
        })();
        self.low_ram_dispatching = false;
        result
    }

    pub(super) fn schedule_callback(&mut self, pcb: u32, owner: u32) -> Result<(), CpuError> {
        if pcb == 0 {
            return Ok(());
        }
        pcb.checked_add(CALLBACK_SIZE)
            .ok_or_else(|| CpuError("callback fora da memória".into()))?;
        let replaced: Vec<_> = self.deferred_callbacks.iter()
            .filter(|(old, owner)| *old == pcb && self.objects.kind_of(*owner) == Some(Interface::WebResp))
            .map(|(_, owner)| *owner).collect();
        for owner in replaced { self.discard_pending_web_response(owner)?; }
        let cancel = self.cpu.read_u32(pcb + 8)?;
        if cancel != 0 && cancel != CANCEL_STUB {
            // AEECallback pode vir de outro provedor; desregistrá-lo antes de tomar posse.
            self.nesting += 1;
            let result = self.call_guest_aninhado(cancel, [pcb, 0, 0, 0], 100_000);
            self.nesting -= 1;
            if !matches!(result?, Outcome::Returned { .. }) {
                return Err(CpuError("cancelamento de callback não retornou".into()));
            }
        }
        self.deferred_callbacks.retain(|(old, _)| *old != pcb);
        // CALLBACK_Cancel(pcb) chama pfnCancel(pcb). A marca fica na memória do guest e,
        // portanto, acompanha saves e restaurações sem guardar ponteiros do host.
        let code = [0xe3a01000u32, 0xe5801008, 0xe580100c, 0xe12fff1e];
        self.cpu.write_mem(
            CANCEL_STUB,
            &code
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )?;
        self.cpu.write_u32(pcb + 8, CANCEL_STUB)?;
        self.cpu.write_u32(pcb + 12, pcb)?;
        self.deferred_callbacks.push((pcb, owner));
        Ok(())
    }

    pub(super) fn cancel_callbacks_of(&mut self, owner: u32) -> Result<(), CpuError> {
        let owned: Vec<_> = self
            .deferred_callbacks
            .iter()
            .filter(|(_, id)| *id == owner)
            .map(|(pcb, _)| *pcb)
            .collect();
        self.deferred_callbacks.retain(|(_, id)| *id != owner);
        for pcb in owned {
            if self.cpu.read_u32(pcb + 8).ok() == Some(CANCEL_STUB) {
                self.cpu.write_u32(pcb + 8, 0)?;
                self.cpu.write_u32(pcb + 12, 0)?;
            }
        }
        Ok(())
    }

    pub(super) fn deliver_deferred_callback(
        &mut self,
        budget: u64,
    ) -> Result<Option<Outcome>, CpuError> {
        self.finish_web_handler()?;
        self.poll_web_requests()?;
        if let Some(outcome)=self.deliver_web_body(budget)?{return Ok(Some(outcome));}
        // Uma transação esperando socket não bloqueia callbacks de outros provedores.
        for _ in 0..self.deferred_callbacks.len() {
            let (pcb, owner) = self.deferred_callbacks.remove(0);
            if self.cpu.read_u32(pcb + 8).ok() != Some(CANCEL_STUB)
                || self.cpu.read_u32(pcb + 12).ok() != Some(pcb)
            {
                if self.objects.kind_of(owner) == Some(Interface::WebResp) {
                    self.discard_pending_web_response(owner)?;
                }
                continue;
            }
            if self.web_event_ready(owner){
                self.deferred_callbacks.push((pcb,owner));
                return self.deliver_web_event(owner,budget);
            }
            if self.http_requests.contains_key(&owner)||self.http_bodies.contains_key(&owner) {
                self.deferred_callbacks.push((pcb, owner));
                continue;
            }
            let function = self.cpu.read_u32(pcb + 16)?;
            let context = self.cpu.read_u32(pcb + 20)?;
            self.cpu.write_u32(pcb + 8, 0)?;
            self.cpu.write_u32(pcb + 12, 0)?;
            if function == 0 {
                if self.objects.kind_of(owner) == Some(Interface::WebResp) {
                    self.discard_pending_web_response(owner)?;
                }
                continue;
            }
            if self.objects.kind_of(owner) == Some(Interface::WebResp) {
                // A saída é publicada antes de passar a posse ao callback.
                self.publish_pending_web_response(owner)?;
            }
            // Só uma entrega por volta: a cauda continua cancelável e um Resume dentro
            // deste callback só pode voltar no próximo giro, inclusive após Budget.
            return self
                .call_guest(function, [context, 0, 0, 0], budget)
                .map(Some);
        }
        Ok(None)
    }
}
