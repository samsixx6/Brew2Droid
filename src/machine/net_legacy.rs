//! O INetMgr 1.1 existe sem uma conexão. Recusar sua classe impedia DRIV3R de criar o applet.
//! Não há transições de rede neste serviço offline, portanto registrar OnEvent não dispara nada.
//! IWebUtil tem ABI própria: o construtor de DRIV3R cria dois, sem ainda usar operações de URL.

use super::*;

pub(super) const MAX_NET_LEGACY_CALLBACKS: usize = 256;
const NET_PPP_CLOSED: u32 = 4;
/// `inc/AEENetErr.h`: NET_ERROR_BASE + 0x16, subsistema indisponível.
const AEE_NET_ENETDOWN: u32 = 0x216;
const EALREADY: u32 = 26;

impl<C: CpuBackend> Machine<C> {
    pub(super) fn web_util_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let Some(name) = Interface::WebUtil.method(slot) else {
            return Ok(None);
        };
        let this = self.cpu.read_reg(Reg::R0);
        if self.objects.kind_of(this) != Some(Interface::WebUtil) {
            return Ok(Some(EBADPARM));
        }
        let result = match name {
            "AddRef" => self.objects.add_ref(this),
            "Release" => self.objects.release(this),
            "QueryInterface" => {
                let requested = self.cpu.read_reg(Reg::R1);
                let out = self.cpu.read_reg(Reg::R2);
                if out == 0 { return Ok(Some(EBADPARM)); }
                // IQI_SELF de AEEQueryInterface.h: a própria interface e AEEIID_IQI.
                let supported = matches!(requested, 0x0100_5001 | 0x0100_0001);
                self.cpu.write_u32(out, if supported { this } else { 0 })?;
                if supported {
                    self.objects.add_ref(this);
                    SUCCESS
                } else { ECLASSNOTSUPPORT }
            }
            "ParseUrl" => {
                self.missing_apis.insert("IWebUtil::ParseUrl".into());
                EUNSUPPORTED
            }
            // Os demais retornam ponteiros ou contagens: 20 fingiria um dado válido.
            // None preserva a parada diagnóstica normal quando o jogo realmente os exigir.
            _ => return Ok(None),
        };
        Ok(Some(result))
    }

    pub(super) fn net_legacy_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let Some(name) = Interface::NetLegacy.method(slot) else {
            return Ok(None);
        };
        let this = self.cpu.read_reg(Reg::R0);
        if self.objects.kind_of(this) != Some(Interface::NetLegacy) {
            return Ok(Some(EBADPARM));
        }
        let a1 = self.cpu.read_reg(Reg::R1);
        let a2 = self.cpu.read_reg(Reg::R2);
        let a3 = self.cpu.read_reg(Reg::R3);
        let result = match name {
            "AddRef" => self.objects.add_ref(this),
            "Release" => {
                let remaining = self.objects.release(this);
                if remaining == 0 {
                    self.net_legacy_errors.remove(&this);
                }
                // AEENet.h: liberar o gerenciador não cancela notificações registradas.
                remaining
            }
            "OnEvent" => {
                if a3 == 0 {
                    // NULL remove todos os callbacks deste contexto, como especifica AEENet.h.
                    self.net_legacy_callbacks.retain(|&(function, context)| {
                        context != a2 || (a1 != 0 && function != a1)
                    });
                    SUCCESS
                } else if a1 == 0 {
                    EBADPARM
                } else if self.net_legacy_callbacks.contains(&(a1, a2)) {
                    EALREADY
                } else if self.net_legacy_callbacks.len() >= MAX_NET_LEGACY_CALLBACKS {
                    ENOMEMORY
                } else {
                    self.net_legacy_callbacks.insert((a1, a2));
                    SUCCESS
                }
            }
            "NetStatus" => {
                if a1 != 0 {
                    // AEENetStats é oito uint32. A rede nunca abriu: os acumuladores são zero.
                    self.cpu.write_mem(a1, &[0; 32])?;
                }
                NET_PPP_CLOSED
            }
            "GetMyIPAddr" => 0,
            "GetLastError" => self.net_legacy_errors.get(&this).copied().unwrap_or(SUCCESS),
            "OpenSocket" => {
                self.net_legacy_errors.insert(this, AEE_NET_ENETDOWN);
                // O retorno é ISocket*, não um código de erro convertido em ponteiro.
                0
            }
            _ => {
                self.net_legacy_errors.insert(this, EUNSUPPORTED);
                self.missing_apis.insert(format!("INetMgr::{name}"));
                EUNSUPPORTED
            }
        };
        Ok(Some(result))
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::cpu::BackendPadrao;

    fn maquina() -> Machine<BackendPadrao> {
        let image = loader::modfile::ModImage::parse(
            [0xe3a0_0000u32.to_le_bytes(), 0xe12f_ff1eu32.to_le_bytes()].concat(),
        ).unwrap();
        let module = loader::load(&image).unwrap();
        let mut machine = Machine::new(BackendPadrao::new().unwrap(), module, ".");
        machine.cpu.reset(&machine.module.mem).unwrap();
        machine
    }

    fn cria_rede(machine: &mut Machine<BackendPadrao>) -> u32 {
        let out = loader::HEAP_BASE;
        machine.cpu.write_reg(Reg::R0, machine.module.shell);
        machine.cpu.write_reg(Reg::R1, 0x0100_1105);
        machine.cpu.write_reg(Reg::R2, out);
        assert_eq!(machine.shell_create_instance().unwrap(), 0,
            "DRIV3R exige que o gerenciador exista mesmo sem conexão");
        let object = machine.cpu.read_u32(out).unwrap();
        assert_ne!(object, 0);
        object
    }

    fn chama(machine: &mut Machine<BackendPadrao>, object: u32, slot: u32, args: [u32; 3]) -> u32 {
        let table = machine.cpu.read_u32(object).unwrap();
        let address = machine.cpu.read_u32(table + slot * 4).unwrap();
        for (reg, value) in [Reg::R0, Reg::R1, Reg::R2, Reg::R3].into_iter().zip(
            [object, args[0], args[1], args[2]],
        ) {
            machine.cpu.write_reg(reg, value);
        }
        machine.dispatch(address).unwrap().expect("o slot do SDK é atendido")
    }

    // A fábrica recusada fazia o construtor de DRIV3R devolver ENOMEMORY antes de EVT_APP_START.
    // O literal 9 vem de AEENet.h: IBase tem dois slots, e OnEvent fica em +0x24.
    #[test]
    fn legacy_net_fabrica_registra_e_remove_callback_sem_inventar_conexao() {
        let mut machine = maquina();
        let object = cria_rede(&mut machine);
        assert_eq!(chama(&mut machine, object, 4, [0, 0, 0]), 0);
        assert_eq!(chama(&mut machine, object, 9, [0x12341, 0x45678, 1]), 0);
        assert_eq!(chama(&mut machine, object, 9, [0x12341, 0x45678, 1]), 26);
        assert_eq!(chama(&mut machine, object, 9, [0x12345, 0x45678, 1]), 0);
        assert_eq!(chama(&mut machine, object, 9, [0x12341, 0x45678, 0]), 0);
        assert_eq!(chama(&mut machine, object, 9, [0x12345, 0x45678, 1]), 26,
            "remover um par não remove outro callback do mesmo contexto");
        assert_eq!(chama(&mut machine, object, 9, [0, 0x45678, 0]), 0);
        assert_eq!(chama(&mut machine, object, 9, [0x12345, 0x45678, 1]), 0);
        assert_eq!(chama(&mut machine, object, 9, [0, 0x99999, 1]), 2);
        assert!(machine.timers.is_empty(), "registro não fabrica evento de abertura");
    }

    #[test]
    fn legacy_net_status_fechado_zera_so_a_struct_e_recusa_opcao() {
        let mut machine = maquina();
        let object = cria_rede(&mut machine);
        let stats = loader::HEAP_BASE + 0x100;
        machine.cpu.write_mem(stats, &[0xa5; 36]).unwrap();
        assert_eq!(chama(&mut machine, object, 6, [stats, 0, 0]), 4);
        assert_eq!(machine.dump(stats, 36).unwrap(), [vec![0; 32], vec![0xa5; 4]].concat());
        assert_eq!(chama(&mut machine, object, 6, [0, 0, 0]), 4);
        assert_eq!(chama(&mut machine, object, 7, [0, 0, 0]), 0);
        assert_eq!(chama(&mut machine, object, 11, [999, stats, stats + 32]), 20);
        assert_eq!(machine.cpu.read_u32(stats + 32).unwrap(), 0xa5a5_a5a5);
        assert_eq!(chama(&mut machine, object, 5, [0, 0, 0]), 0,
            "uma falha de socket jamais pode parecer ponteiro 20");
        assert_eq!(chama(&mut machine, object, 4, [0, 0, 0]), 0x216);
    }

    #[test]
    fn legacy_net_callback_sobrevive_release_e_save_state() {
        let mut before = maquina();
        let object = cria_rede(&mut before);
        assert_eq!(chama(&mut before, object, 9, [0x12341, 0x45678, 1]), 0);
        assert_eq!(chama(&mut before, object, 1, [0, 0, 0]), 0);
        let live = cria_rede(&mut before);
        assert_eq!(chama(&mut before, live, 5, [0, 0, 0]), 0);
        let state = before.grava_estado();
        let mut after = maquina();
        after.restaura_estado(&state).unwrap();
        assert_eq!(chama(&mut after, live, 4, [0, 0, 0]), 0x216);
        let object = cria_rede(&mut after);
        assert_eq!(chama(&mut after, object, 9, [0x12341, 0x45678, 1]), 26,
            "AEENet.h exige remoção explícita mesmo depois de Release");
        assert_eq!(chama(&mut after, object, 9, [0, 0x45678, 0]), 0);
        assert_eq!(chama(&mut after, object, 9, [0x12341, 0x45678, 1]), 0);
    }

    #[test]
    fn legacy_net_registro_tem_limite_sem_apagar_os_anteriores() {
        let mut machine = maquina();
        let object = cria_rede(&mut machine);
        for context in 0..256 {
            assert_eq!(chama(&mut machine, object, 9, [0x12341, context, 1]), 0);
        }
        assert_eq!(chama(&mut machine, object, 9, [0x12341, 256, 1]), 3);
        assert_eq!(chama(&mut machine, object, 9, [0x12341, 0, 1]), 26);
    }

    #[test]
    fn legacy_webutil_fabrica_iqi_e_vtable_propria() {
        let mut machine = maquina();
        let out = loader::HEAP_BASE;
        machine.cpu.write_reg(Reg::R0, machine.module.shell);
        machine.cpu.write_reg(Reg::R1, 0x0100_5001);
        machine.cpu.write_reg(Reg::R2, out);
        assert_eq!(machine.shell_create_instance().unwrap(), 0,
            "IWebUtil é uma dependência separada de IWeb no construtor de DRIV3R");
        let object = machine.cpu.read_u32(out).unwrap();
        assert_ne!(object, 0);
        assert_eq!(chama(&mut machine, object, 0, [0, 0, 0]), 2);
        assert_eq!(chama(&mut machine, object, 1, [0, 0, 0]), 1);
        assert_eq!(chama(&mut machine, object, 2, [0x0100_0001, out, 0]), 0);
        assert_eq!(machine.cpu.read_u32(out).unwrap(), object);
        assert_eq!(chama(&mut machine, object, 1, [0, 0, 0]), 1);
        assert_eq!(chama(&mut machine, object, 2, [0xdead_beef, out, 0]), 20);
        assert_eq!(machine.cpu.read_u32(out).unwrap(), 0);
        assert_eq!(chama(&mut machine, object, 3, [0, 0, 0]), 20,
            "slot3 é ParseUrl; não pode aceitar como IWeb::AddOpt");
        assert!(machine.missing_apis().contains(&"IWebUtil::ParseUrl".to_string()));
        assert_eq!(chama(&mut machine, object, 1, [0, 0, 0]), 0);
    }
}
