//! AEECallback mantém identidade e cancelamento até a próxima volta do laço.
use super::*;

const CANCEL_STUB: u32 = loader::STUB_BASE + 0xc0;

impl<C: CpuBackend> Machine<C> {
    pub(super) fn schedule_callback(&mut self, pcb: u32, owner: u32) -> Result<(), CpuError> {
        if pcb == 0 {
            return Ok(());
        }
        pcb.checked_add(CALLBACK_SIZE)
            .ok_or_else(|| CpuError("callback fora da memória".into()))?;
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
        while !self.deferred_callbacks.is_empty() {
            let (pcb, _) = self.deferred_callbacks.remove(0);
            if self.cpu.read_u32(pcb + 8).ok() != Some(CANCEL_STUB)
                || self.cpu.read_u32(pcb + 12).ok() != Some(pcb)
            {
                continue;
            }
            let function = self.cpu.read_u32(pcb + 16)?;
            let context = self.cpu.read_u32(pcb + 20)?;
            self.cpu.write_u32(pcb + 8, 0)?;
            self.cpu.write_u32(pcb + 12, 0)?;
            if function == 0 {
                continue;
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
