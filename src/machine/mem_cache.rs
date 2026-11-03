//! IMemCache1 opera sobre a memória coerente do emulador, sem inventar cache físico.

use super::*;

// AEEStdErr.h do Brew MP usa 3 para classe/interface não suportada; o código
// histórico global ECLASSNOTSUPPORT=20 pertence a outro contrato deste núcleo.
const CACHE_CLASS_NOT_SUPPORTED: u32 = 3;
const CACHE_BAD_PARAMETER: u32 = 14;
const CACHE_FAILED: u32 = 1;
const CACHE_IID: u32 = 0x0106_e2e0;
const IQI_IID: u32 = 0x0100_0001;

impl<C: CpuBackend> Machine<C> {
    pub(super) fn mem_cache_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let this = self.arg(0);
        if self.objects.kind_of(this) != Some(Interface::MemCache1) {
            return Ok(Some(CACHE_BAD_PARAMETER));
        }
        let result = match slot {
            0 => self.objects.add_ref(this),
            1 => self.objects.release(this),
            2 => {
                let iid = self.arg(1);
                let out = self.arg(2);
                if out == 0 || self.cpu.write_u32(out, 0).is_err() {
                    return Ok(Some(CACHE_BAD_PARAMETER));
                }
                if matches!(iid, CACHE_IID | IQI_IID) {
                    self.cpu.write_u32(out, this)?;
                    self.objects.add_ref(this);
                    SUCCESS
                } else {
                    CACHE_CLASS_NOT_SUPPORTED
                }
            }
            3 => {
                let address = self.arg(1);
                let size = self.arg(2);
                let operation = self.arg(3);
                // O quinto argumento total não cabe nos registradores AAPCS.
                let Ok(flags) = self.stack_arg(0) else {
                    return Ok(Some(CACHE_FAILED));
                };
                let end = u64::from(address) + u64::from(size);
                if operation & !3 != 0 || flags & !3 != 0 || end > (1u64 << 32) {
                    CACHE_FAILED
                } else {
                    // Não há cópia de dados atrás de um cache: host e ARM veem os
                    // mesmos bytes. Os dois caminhos de escrita já invalidam código
                    // traduzido antes da próxima entrada no guest. Regravar a faixa
                    // aqui imporia permissão de escrita e leitura que flush não exige.
                    // Faixa vazia ou sem mapa também não contém linhas pendentes.
                    SUCCESS
                }
            }
            // O SDK admite AEE_EFAILED. Sem geometria física modelada, devolver
            // zeros ou números de um aparelho fingiria informação que não temos.
            // Em falha, o registro MemCacheInfo do chamador permanece intacto.
            4 => CACHE_FAILED,
            _ => return Ok(None),
        };
        Ok(Some(result))
    }
}
