//! Campo de modelo da extensão regional0x018000fe, observado nos consumidores ARM.

use super::*;

pub(super) const CLASS_ID: u32 = 0x0180_00fe;
// AEEError.h:35; a constante genérica histórica da máquina usa outro valor.
pub(super) const BAD_PARAMETER: u32 = 14;

impl<C: CpuBackend> Machine<C> {
    pub(super) fn regional_model_output_fits(&self, out: u32, size: u32) -> bool {
        let Some(end) = out.checked_add(size) else { return false; };
        out != 0 && self.module.mem.regions().iter().any(|region| {
            region.writable && out >= region.base
                && u64::from(end) <= u64::from(region.base) + region.bytes.len() as u64
        })
    }

    pub(super) fn regional_model_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let this = self.arg(0);
        let live = self.objects.kind_of(this) == Some(Interface::RegionalModelInfo);
        let result = match slot {
            0 => if live { self.objects.add_ref(this) } else { 0 },
            1 => if live { self.objects.release(this) } else { 0 },
            2 => {
                let out = self.arg(1);
                if !live || !self.regional_model_output_fits(out, 8) {
                    return Ok(Some(0));
                }
                // Icaria limpa 38 bytes, Epic 40; Minigame copia 10 do início e 16 do offset 10.
                // Só o campo inicial foi identificado por strcmp com modelos de aparelhos.
                // B2Droid é a identidade virtual e cabe com NUL nesse campo; os demais bytes
                // pertencem a dados ainda desconhecidos e precisam permanecer intocados.
                // A/B privado: vazio mantém o fault de Icaria; este nome alcança a alocação
                // normal do guest. Evidência: startup-repairs-wave15-20261030/regional.
                // Os backends validam a faixa inteira antes de copiar. Esta consulta
                // falha com FALSE; o HRESULT14 pertence à fábrica IShell, não ao slot2.
                if self.cpu.write_mem(out, b"B2Droid\0").is_err() {
                    return Ok(Some(0));
                }
                // Bowling2/Hulk/Raiden rejeitam zero e RoadRash2 exige exatamente1.
                // O retorno é booleano; HRESULT=SUCCESS fazia uma consulta válida
                // parecer indisponível. Campos de identidade não foram preenchidos.
                1
            }
            // O slot 2 não é QueryInterface. Nenhum outro método foi demonstrado.
            _ => return Ok(None),
        };
        Ok(Some(result))
    }
}
