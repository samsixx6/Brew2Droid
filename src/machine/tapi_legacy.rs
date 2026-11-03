//! Estado de telefonia virtual do SDK móvel. Não consulta a telefonia do anfitrião.

use super::*;

const TAPI_CLASS: u32 = 0x0100_1007;
const MAX_REGISTRATIONS: usize = 256;

impl<C: CpuBackend> Machine<C> {
    pub(super) fn tapi_legacy_register_notify(&mut self) -> Result<Option<u32>, CpuError> {
        let app = self.cpu.read_reg(Reg::R1);
        let class = self.cpu.read_reg(Reg::R2);
        let mask = self.cpu.read_reg(Reg::R3);
        if class != TAPI_CLASS {
            return Ok(None);
        }
        // Só NMASK_TAPI_STATUS, sem Notify Value. Não oferecer SMS/WAP por aceitar o registro.
        if app == 0 || ![0, 1].contains(&mask) {
            return Ok(Some(14));
        }
        if mask == 0 {
            if let Some(object) = self.tapi_registrations.remove(&app) {
                self.objects.release(object);
            }
            return Ok(Some(SUCCESS));
        }
        if self.tapi_registrations.contains_key(&app) {
            return Ok(Some(SUCCESS));
        }
        if self.tapi_registrations.len() >= MAX_REGISTRATIONS {
            return Ok(Some(2)); // ENOMEMORY móvel.
        }
        // AEEShell.h: registrar cria/retém o INotifier e cancelar libera sua referência.
        // O estado é offline e não muda; registrar não é uma mudança nem dispara EVT_NOTIFY.
        let object = self.new_object(Interface::TapiLegacy)?;
        if object == 0 {
            return Ok(Some(2));
        }
        self.tapi_registrations.insert(app, object);
        Ok(Some(SUCCESS))
    }

    pub(super) fn grava_tapi_legado(&self, sections: &mut crate::save_state::Secoes) {
        sections.poe_u32s(
            "tab.tapi_legacy",
            self.tapi_registrations
                .iter()
                .flat_map(|(&app, &object)| [app, object]),
        );
    }

    pub(super) fn restaura_tapi_legado(
        &mut self,
        reader: &crate::save_state::Leitor<'_>,
    ) -> Result<(), crate::save_state::Erro> {
        let invalid = || crate::save_state::Erro::Secao {
            nome: "tab.tapi_legacy".into(),
            motivo: "registro de notificação TAPI inválido".into(),
        };
        let Some(bytes) = reader.secao("tab.tapi_legacy") else {
            self.tapi_registrations.clear();
            return Ok(());
        };
        if bytes.len() > 4 + MAX_REGISTRATIONS * 8 {
            return Err(invalid());
        }
        let mut restored = BTreeMap::new();
        let mut owned = std::collections::HashSet::new();
        for (app, object) in reader.pares("tab.tapi_legacy")? {
            if app == 0
                || self.objects.kind_of(object) != Some(Interface::TapiLegacy)
                || self.objects.contagem(object) == 0
                || !owned.insert(object)
                || restored.insert(app, object).is_some()
            {
                return Err(invalid());
            }
        }
        self.tapi_registrations = restored;
        Ok(())
    }

    pub(super) fn tapi_legacy_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let this = self.cpu.read_reg(Reg::R0);
        if self.objects.kind_of(this) != Some(Interface::TapiLegacy) {
            return Ok(Some(14)); // EBADPARM do SDK móvel.
        }
        let Some(name) = Interface::TapiLegacy.method(slot) else {
            return Ok(None);
        };
        let result = match name {
            "AddRef" => self.objects.add_ref(this),
            "Release" => self.objects.release(this),
            "GetStatus" => {
                let out = self.cpu.read_reg(Reg::R1);
                if out == 0 {
                    return Ok(Some(14));
                }
                // AEETAPI.h + AEEComdef.h: char MIN[16], byte state e flg de 32 bits.
                // No ARM, state fica em +16, flags em +17/+18 e sizeof é 20 (align 4).
                // O aparelho virtual está PS_OFFLINE, sem MIN provisionado, chamada ou rede.
                // Melange/OEMTAPI também informa PS_OFFLINE com os indicadores zerados.
                self.cpu.write_mem(out, &[0; 20])?;
                SUCCESS
            }
            "IsVoiceCall" | "IsDataSupported" => FALSE,
            "SendSMS" => {
                let (destination, message, class) = (self.arg(1), self.arg(2), self.arg(3));
                if message == 0 || (destination == 0 && class == 0) {
                    14
                } else if destination == 0 {
                    // AEETAPI.h: sem número, envia EVT_APP_MESSAGE local e devolve
                    // o resultado do tratador; não há SMS remoto nem callback de entrega.
                    // AEEEvent.h: evento 8; AEESMSEncoding LATIN=8 nesta telefonia virtual.
                    self.dispatch_applet_event(class, 8, 8, message, false)?
                } else {
                    // A telefonia virtual está offline. Rejeitar a solicitação é uma
                    // falha síncrona documentada; não emitir notificação de envio aceito.
                    EFAILED
                }
            }
            // Não simular discagem nem aceitar notificações sem implementar o contrato.
            _ => return Ok(None),
        };
        Ok(Some(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::BackendPadrao;

    #[test]
    fn local_sms_delivers_the_real_applet_message_and_preserves_its_result() {
        let words = [0xe3a00000u32,0xe12fff1e,0xe5801004,0xe5802008,0xe580300c,0xe3a00001,0xe12fff1e];
        let image = loader::modfile::ModImage::parse(words.into_iter().flat_map(u32::to_le_bytes).collect()).unwrap();
        let mut m = Machine::new(BackendPadrao::new().unwrap(),loader::load_with(&image,&[]).unwrap(),".");
        m.run(1000).unwrap();
        let applet=m.heap.alloc(64).unwrap();
        m.cpu.write_u32(applet,applet+32).unwrap();
        m.cpu.write_u32(applet+40,loader::MODULE_BASE+8).unwrap();
        m.current_applet=applet;
        m.applet_class=0x1234;
        let object=m.new_object(Interface::TapiLegacy).unwrap();
        let slot=(0..256).find(|&s| Interface::TapiLegacy.method(s)==Some("SendSMS")).unwrap();
        let message=loader::STACK_BASE+0x100;
        m.cpu.write_mem(message,b"local\0").unwrap();
        let result=m.call_guest(aee::encode(Interface::TapiLegacy,slot),[object,0,message,0x1234],10000).unwrap();
        assert_eq!(result,Outcome::Returned{code:TRUE});
        assert_eq!(m.cpu.read_u32(applet+4).unwrap(),8);
        assert_eq!(m.cpu.read_u32(applet+8).unwrap(),8);
        assert_eq!(m.cpu.read_u32(applet+12).unwrap(),message);
        assert!(m.pending_calls.is_empty(),"SMS local não chama a notificação de SMS remoto");
    }
}
