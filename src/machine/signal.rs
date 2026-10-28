//! ISignal e os callbacks: o que o jogo pediu para ser avisado.

#[cfg(test)]
mod handset_batch_tests {
    use super::*;
    use crate::cpu::BackendPadrao;

    fn batch(signals: bool) {
        let mut bytes = vec![0u8; 0x100];
        for (offset, code) in [
            (0, vec![0xe3a00000u32, 0xe12fff1e]),
            (0x20, vec![0xe3a010c8, 0xe2511001, 0x1afffffd, 0xe3a01001, 0xe5801000, 0xe12fff1e]),
            (0x80, vec![0xe3a01002, 0xe5801000, 0xe12fff1e]),
        ] {
            for (index, word) in code.iter().enumerate() {
                bytes[offset + index * 4..offset + index * 4 + 4].copy_from_slice(&word.to_le_bytes());
            }
        }
        let image = crate::loader::modfile::ModImage::parse(bytes).unwrap();
        let module = loader::load_with(&image, &[]).unwrap();
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local-tests/batch-tests");
        let storage = crate::storage::StoragePaths::from_root(&root);
        let mut machine = Machine::new_with_storage(BackendPadrao::new().unwrap(), module, &root, &storage, None);
        machine.run(1000).unwrap();
        let marker = loader::HEAP_BASE + 0x10000;
        for (function, context) in [(loader::MODULE_BASE + 0x20, marker), (loader::MODULE_BASE + 0x80, marker + 4)] {
            if signals { machine.pending_signals.push(Callback { function, context }); }
            else { machine.pending_calls.push(GuestCall { function, args: [context, 0, 0, 0] }); }
        }
        let outcomes = if signals { machine.deliver_signals(16) } else { machine.deliver_callbacks(16) }.unwrap();
        assert_eq!(outcomes, vec![Outcome::Budget]);
        assert_eq!(machine.cpu.read_u32(marker + 4).unwrap(), 0);
        for _ in 0..100 {
            machine.advance(16).unwrap();
            if machine.handset_event_ready() { break; }
        }
        assert_eq!(machine.cpu.read_u32(marker).unwrap(), 1);
        assert_eq!(machine.cpu.read_u32(marker + 4).unwrap(), 0);
        if signals { machine.deliver_signals(1000) } else { machine.deliver_callbacks(1000) }.unwrap();
        assert_eq!(machine.cpu.read_u32(marker + 4).unwrap(), 2);
    }

    #[test]
    fn signal_batch_preserves_a_budgeted_continuation() { batch(true); }

    #[test]
    fn callback_batch_preserves_a_budgeted_continuation() { batch(false); }

    #[test]
    fn asynchronous_applet_event_waits_for_dispatch_and_resumes_after_budget_and_save() {
        for restore in [0, 1, 2] {
            let mut bytes = vec![0u8; 0x100];
            for (offset, code) in [
                (0, vec![0xe3a00000u32, 0xe12fff1e]),
                (0x20, vec![0xe5801004, 0xe5802008, 0xe580300c, 0xe3a0c0c8,
                    0xe25cc001, 0x1afffffd, 0xe3a00001, 0xe12fff1e]),
                (0x80, vec![0xe5801010, 0xe3a00000, 0xe12fff1e]),
            ] {
                for (index, word) in code.iter().enumerate() {
                    bytes[offset+index*4..offset+index*4+4].copy_from_slice(&word.to_le_bytes());
                }
            }
            let image=crate::loader::modfile::ModImage::parse(bytes).unwrap();
            let module=loader::load_with(&image,&[]).unwrap();
            let root=std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local-tests/post-event-tests");
            let storage=crate::storage::StoragePaths::from_root(&root);
            let mut machine=Machine::new_with_storage(BackendPadrao::new().unwrap(),module,&root,&storage,None);
            machine.run(1000).unwrap();
            let applet=machine.heap.alloc(0x200).unwrap();
            machine.cpu.write_u32(applet,applet+0x100).unwrap();
            machine.cpu.write_u32(applet+0x108,loader::MODULE_BASE+0x20).unwrap();
            machine.current_applet=applet;
            machine.applet_class=0x1234;
            let sp=machine.cpu.read_reg(Reg::Sp);
            machine.cpu.write_u32(sp,0x1234).unwrap();
            machine.cpu.write_u32(sp+4,0x55667788).unwrap();
            let slot=(0..256).find(|slot|Interface::Shell.method(*slot)==Some("SendEvent")).unwrap();
            let result=machine.call_guest(aee::encode(Interface::Shell,slot),
                [loader::OBJECT_BASE,3,0x1234,0x7000],1000).unwrap();
            assert_eq!(result,Outcome::Returned{code:TRUE});
            assert_eq!(machine.cpu.read_u32(applet+4).unwrap(),0,
                "PostEventEx não pode reentrar no tratador antes de quem postou retornar");
            assert!(!machine.is_idle());
            assert_eq!(machine.next_guest_work_delay(),Some(std::time::Duration::ZERO));
            machine.timers.push(Timer { deadline_ms:machine.clock_ms()+1000,
                callback:Callback{function:0,context:0} });
            let clock=machine.clock_ms();
            machine.skip_idle_time();
            assert_eq!(machine.clock_ms(),clock);
            machine.timers.clear();
            assert_eq!(machine.dispatch_applet_event(0x1234,0x7001,0,0,true).unwrap(),TRUE);
            if restore == 1 { let saved=machine.grava_estado(); machine.restaura_estado(&saved).unwrap(); }
            assert_eq!(machine.deliver_callbacks(16).unwrap(),vec![Outcome::Budget]);
            if restore == 2 { let saved=machine.grava_estado(); machine.restaura_estado(&saved).unwrap(); }
            for _ in 0..100 {
                machine.advance(16).unwrap();
                if machine.handset_event_ready() {break;}
            }
            assert!(machine.handset_event_ready(),"o evento deve terminar após retomar seu orçamento");
            assert_eq!(machine.cpu.read_u32(applet+4).unwrap(),0x7000);
            assert_eq!(machine.cpu.read_u32(applet+8).unwrap(),0x1234);
            assert_eq!(machine.cpu.read_u32(applet+12).unwrap(),0x55667788);
            assert_eq!(machine.deliver_callbacks(1000).unwrap(),vec![Outcome::Returned{code:TRUE}]);
            assert_eq!(machine.cpu.read_u32(applet+4).unwrap(),0x7001,
                "o evento seguinte deve esperar a continuação anterior");

            machine.cpu.write_u32(applet+0x108,loader::MODULE_BASE+0x80).unwrap();
            let result=machine.call_guest(aee::encode(Interface::Shell,slot),
                [loader::OBJECT_BASE,0,0x1234,0x7002],1000).unwrap();
            assert_eq!(result,Outcome::Returned{code:FALSE});
            assert_eq!(machine.cpu.read_u32(applet+16).unwrap(),0x7002,
                "flags zero devem conservar a entrega síncrona e seu resultado");
            let result=machine.call_guest(aee::encode(Interface::Shell,slot),
                [loader::OBJECT_BASE,2,0x1234,0x7003],1000).unwrap();
            assert_eq!(result,Outcome::Returned{code:TRUE});
            assert_eq!(machine.cpu.read_u32(applet+16).unwrap(),0x7002);
            assert_eq!(machine.deliver_callbacks(1000).unwrap(),vec![Outcome::Returned{code:FALSE}]);
            assert_eq!(machine.cpu.read_u32(applet+16).unwrap(),0x7003);
            assert_eq!(machine.dispatch_applet_event(0x9999,0x7004,0,0,true).unwrap(),FALSE);
        }
    }
}

use super::*;

impl<C: CpuBackend> Machine<C> {
    /// Roda os callbacks pendentes na fronteira entre duas chamadas de API."""
    ///
    /// O BREW entrega notificações pelo laço de eventos dele, quando o applet já devolveu o
    /// controle. Nosso equivalente mais próximo é este ponto: a chamada de API terminou, o
    /// resultado já está em `r0` e o guest ainda não retomou. Todo o contexto é salvo e
    /// devolvido, de forma que o chamador não perceba o desvio.
    pub(super) fn run_pending_callbacks(&mut self, budget: u64) -> Result<(), CpuError> {
        self.poll_media()?;
        let idle = self.pending_calls.is_empty()
            && self.pending_probes.is_empty()
            && self.pending_blits.is_empty()
            && self.pending_surface_blits.is_empty();
        if idle || self.nesting >= MAX_NESTING {
            return Ok(());
        }
        let saved = SAVED_REGS.map(|reg| self.cpu.read_reg(reg));
        self.nesting += 1;
        for _ in 0..CALLBACK_ROUNDS {
            for target in std::mem::take(&mut self.pending_probes) {
                self.probe_foreign_surface(target, budget)?;
            }
            for blit in std::mem::take(&mut self.pending_blits) {
                self.blit_into_foreign(blit, budget)?;
            }
            for blit in std::mem::take(&mut self.pending_surface_blits) {
                self.blit_surface_into_foreign(blit, budget)?;
            }
            let pending = std::mem::take(&mut self.pending_calls);
            if pending.is_empty() {
                break;
            }
            for call in pending {
                if call.function != 0 {
                    self.call_guest(call.function, call.args, budget)?;
                }
            }
        }
        self.nesting -= 1;
        for (reg, value) in SAVED_REGS.iter().zip(saved) {
            self.cpu.write_reg(*reg, value);
        }
        Ok(())
    }

    /// Descobre a função a chamar por trás de um par `(pfn, pUser)`.
    ///
    /// O `ISHELL_SetTimerEx` do SDK é uma macro sobre o `SetTimer`, e ela passa **o mesmo
    /// ponteiro** nos dois argumentos:
    ///
    /// ```c
    /// #define ISHELL_SetTimerEx(p,s,pcb) \
    ///     GET_PVTBL(p,IShell)->SetTimer(p, s, (PFNNOTIFY)(void *)pcb, (void *)pcb)
    /// ```
    ///
    /// Quando os dois são iguais, o que chegou é um `AEECallback *`, e quem deve ser chamado
    /// está dentro dele: `pfnNotify` no offset 16 e `pNotifyData` no 20 (`inc/AEECallback.h`).
    /// Um `pfn` de verdade nunca coincide com o seu `pUser` — um é código, o outro é dado.
    pub(super) fn resolve_notify(&mut self, callback: Callback) -> Result<Callback, CpuError> {
        if callback.function == 0 || callback.function != callback.context {
            return Ok(callback);
        }
        let base = callback.function;
        Ok(Callback {
            function: self.cpu.read_u32(base + 16).unwrap_or(0),
            context: self.cpu.read_u32(base + 20).unwrap_or(0),
        })
    }

    /// Entrega os sinais disparados, chamando os callbacks do guest.
    ///
    /// Precisa rodar fora do despacho de uma chamada, quando o guest não está no meio de
    /// outra — daí a fila.
    pub fn deliver_signals(&mut self, budget: u64) -> Result<Vec<Outcome>, CpuError> {
        if self.trecho_interrompido.is_some() {
            return Ok(Vec::new());
        }
        // A resposta de rede compartilha esta fronteira pelo mesmo motivo dos sinais: entregá-la
        // pede chamar o alocador do jogo, e isso só é seguro fora do despacho.
        let rsa = self.poll_rsa(budget)?;
        if rsa.iter().any(|outcome| !matches!(outcome, Outcome::Returned { .. })) { return Ok(rsa); }
        self.flush_response()?;
        // **A interface é redesenhada por quadro, não por volta do laço.** Ver
        // [`Machine::ultimo_desenho_us`]: o desenho de um `OwnerDrawWidget` é código do jogo, e
        // chamá-lo mais vezes do que o aparelho chamaria é trabalho que ninguém pediu.
        let agora = self.now_us();
        if agora.saturating_sub(self.ultimo_desenho_us) >= VSYNC_PERIOD_US {
            self.ultimo_desenho_us = agora;
            self.pinta_widgets()?;
            self.desenha_widgets()?;
        }
        self.entrega_ativacao()?;
        self.parte_animacao()?;
        self.skip_wheel_instructions()?;
        self.flush_keys()?;
        self.relatorio_do_boomerang();
        // Os buffers de som entregues nesta volta são lidos agora, depois de o tratador do jogo
        // ter terminado de escrevê-los.
        self.resolve_midias_pendentes()?;
        // Os avisos do `IMedia` desta volta: ver [`Machine::notify_media`].
        self.poll_media()?;
        let mut outcomes = rsa;
        outcomes.extend(self.entrega_avisos_de_midia(budget)?);
        if outcomes.iter().any(|outcome| !matches!(outcome, Outcome::Returned { .. })) {
            return Ok(outcomes);
        }
        if let Some(outcome) = self.bombeia_fluxos_pcm(budget)? {
            outcomes.push(outcome);
            return Ok(outcomes);
        }
        self.bombeia_buffers_vivos()?;
        // Ver [`Machine::notify_image`]. Um aviso que nasce dentro de outro tratador fica para
        // a volta seguinte, como no aparelho.
        let mut images = std::mem::take(&mut self.avisos_de_imagem).into_iter();
        while let Some(imagem) = images.next() {
            let delivered = self.entrega_aviso_de_imagem(imagem, budget)?;
            let incomplete = delivered.iter().any(|outcome| !matches!(outcome, Outcome::Returned { .. }));
            outcomes.extend(delivered);
            if incomplete {
                let mut remaining: Vec<_> = images.collect();
                remaining.append(&mut self.avisos_de_imagem);
                self.avisos_de_imagem = remaining;
                return Ok(outcomes);
            }
        }
        if let Some(outcome) = self.deliver_deferred_callback(budget)? {
            let incomplete = !matches!(outcome, Outcome::Returned { .. });
            outcomes.push(outcome);
            if incomplete { return Ok(outcomes); }
        }
        let mut pending = std::mem::take(&mut self.pending_signals).into_iter();
        while let Some(callback) = pending.next() {
            if callback.function == 0 {
                continue;
            }
            let outcome = self.call_guest(
                callback.function,
                [callback.context, 0, 0, 0],
                budget,
            )?;
            let incomplete = !matches!(outcome, Outcome::Returned { .. });
            outcomes.push(outcome);
            if incomplete {
                let mut remaining: Vec<_> = pending.collect();
                remaining.append(&mut self.pending_signals);
                self.pending_signals = remaining;
                return Ok(outcomes);
            }
        }
        Ok(outcomes)
    }

    /// Entrega os callbacks enfileirados (som, imagem, o que vier).
    ///
    /// Mesma razão da fila de sinais: eles rodam no guest, e não dá para reentrar no guest no
    /// meio do despacho de uma chamada dele.
    pub fn deliver_callbacks(&mut self, budget: u64) -> Result<Vec<Outcome>, CpuError> {
        if self.trecho_interrompido.is_some() {
            return Ok(Vec::new());
        }
        let mut outcomes = Vec::new();
        // Só o lote que já estava pronto entra nesta volta. Um evento que posta a si mesmo
        // precisa devolver a vez à UI, e não reentrar pela fronteira de uma API do tratador.
        let mut posted = std::mem::take(&mut self.posted_calls).into_iter();
        while let Some(call) = posted.next() {
            let result = self.call_guest(call.function, call.args, budget);
            if !matches!(result, Ok(Outcome::Returned { .. })) {
                let mut remaining: Vec<_> = posted.collect();
                remaining.append(&mut self.posted_calls);
                self.posted_calls = remaining;
                outcomes.push(result?);
                return Ok(outcomes);
            }
            outcomes.push(result?);
        }
        // Um callback pode enfileirar outro, então drena até esvaziar — com teto, para que um
        // ciclo entre callbacks não prenda o emulador.
        for _ in 0..CALLBACK_ROUNDS {
            let mut pending = std::mem::take(&mut self.pending_calls).into_iter();
            if pending.len() == 0 {
                break;
            }
            while let Some(call) = pending.next() {
                if call.function == 0 {
                    continue;
                }
                let outcome = self.call_guest(call.function, call.args, budget)?;
                let incomplete = !matches!(outcome, Outcome::Returned { .. });
                outcomes.push(outcome);
                if incomplete {
                    let mut remaining: Vec<_> = pending.collect();
                    remaining.append(&mut self.pending_calls);
                    self.pending_calls = remaining;
                    return Ok(outcomes);
                }
            }
        }
        Ok(outcomes)
    }

    /// `ISignal`, `ISignalCtl` e `ISignalCBFactory`.
    ///
    /// Um sinal é o aviso "aconteceu alguma coisa". O app cria um pela fábrica, passando uma
    /// função de callback e um contexto, e entrega o `ISignal` a quem for notificá-lo — o
    /// gamepad, por exemplo, via `RegisterForButtonEvent`. Quando o sistema chama
    /// `ISIGNAL_Set`, o callback do app roda.
    ///
    /// Aqui o disparo é adiado em vez de imediato: chamar o guest de dentro do despacho de uma
    /// chamada do guest seria reentrância, e o BREW também não dispara na hora — ele agenda
    /// para o laço de eventos do app. Os sinais pendentes ficam registrados até termos esse
    /// laço.
    pub(super) fn signal_call(
        &mut self,
        iface: Interface,
        slot: u32,
    ) -> Result<Option<u32>, CpuError> {
        let Some(name) = iface.method(slot) else {
            return Ok(None);
        };
        let this = self.cpu.read_reg(Reg::R0);
        let (a1, a2, a3) = (
            self.cpu.read_reg(Reg::R1),
            self.cpu.read_reg(Reg::R2),
            self.cpu.read_reg(Reg::R3),
        );
        let result = match name {
            "AddRef" => self.objects.add_ref(this),
            "Release" => {
                let remaining = self.objects.release(this);
                if remaining == 0 {
                    self.signals.remove(&this);
                    // Um sinal solto não pode continuar registrado para a entrada: o endereço
                    // volta como outro sinal, e o toque dispararia o callback dele.
                    self.input_signals.retain(|_, sinal| *sinal != this);
                }
                remaining
            }
            "QueryInterface" => {
                if a2 != 0 {
                    self.cpu.write_u32(a2, this)?;
                }
                SUCCESS
            }
            // int CreateSignal(ISignalCBFactory *, void (*pfn)(void *pCx), void *pCx,
            //                  ISignal **ppiSig, ISignalCtl **ppiSigCtl)
            "CreateSignal" => {
                let signal = self.new_object(Interface::Signal)?;
                let control = self.new_object(Interface::SignalCtl)?;
                if signal == 0 || control == 0 {
                    return Ok(Some(ENOMEMORY));
                }
                // Os dois objetos falam do mesmo sinal, então compartilham o callback.
                let callback = Callback {
                    function: a1,
                    context: a2,
                };
                self.signals.insert(signal, callback);
                self.signals.insert(control, callback);
                if a3 != 0 {
                    self.cpu.write_u32(a3, signal)?;
                }
                let out_control = self.stack_arg(0)?;
                if out_control != 0 {
                    self.cpu.write_u32(out_control, control)?;
                }
                SUCCESS
            }
            "Set" => {
                if let Some(&callback) = self.signals.get(&this) {
                    self.pending_signals.push(callback);
                }
                SUCCESS
            }
            // `Enable` rearma o sinal e `Detach` o desliga da fonte. Sem laço de eventos ainda,
            // os dois são registro de estado.
            "Enable" => SUCCESS,
            "Detach" => {
                self.signals.remove(&this);
                SUCCESS
            }
            _ => return Ok(None),
        };
        Ok(Some(result))
    }

    /// Entrega um evento ao applet em execução, e devolve o que ele respondeu.
    ///
    /// Só há um applet aqui, então um `cls` que não seja o dele é evento para alguém que não
    /// existe — e a resposta certa nesse caso é "ninguém tratou", que é o que o BREW responde.
    ///
    /// Chamar o guest daqui é reentrância, com o mesmo cuidado do `qsort` e da entrega de
    /// linhas de SQL: salva os registradores, respeita o teto de aninhamento, devolve tudo.
    pub(super) fn send_applet_event(
        &mut self, cls: u32, evt: u32, w: u16, dw: u32,
    ) -> Result<u32, CpuError> {
        self.dispatch_applet_event(cls, evt, w, dw, false)
    }

    pub(super) fn dispatch_applet_event(
        &mut self,
        cls: u32,
        evt: u32,
        w: u16,
        dw: u32,
        post: bool,
    ) -> Result<u32, CpuError> {
        // O `current_applet` só é preenchido quando o `EVT_APP_START` é despachado, e há
        // evento antes disso: a Z-Wheel monta o banco de preferências **durante a
        // construção** do applet, e para isso manda um evento para a própria classe. Nesse
        // instante o objeto já existe — o `AEEApplet_New` escreveu o ponteiro de saída antes
        // de o código do jogo rodar —, então lê-lo de lá é o que o console faz: para o shell,
        // o applet passa a existir quando é registrado, não quando é iniciado.
        //
        // Sem isto o evento voltava "ninguém tratou", e a Z-Wheel imprimia
        // `SendEvent to get PrefsDB failed` onze vezes seguidas antes de desistir da
        // configuração inteira.
        let applet = match self.current_applet {
            0 => self.cpu.read_u32(self.module.out_module + 4).unwrap_or(0),
            vivo => vivo,
        };
        if applet == 0 || (cls != 0 && cls != self.applet_class) {
            return Ok(FALSE);
        }
        let vtable = self.cpu.read_u32(applet)?;
        let handle_event = self.cpu.read_u32(vtable + 2 * 4)?;
        if post {
            if handle_event == 0 { return Ok(FALSE); }
            self.posted_calls.push(GuestCall {
                function: handle_event,
                args: [applet, evt, u32::from(w), dw],
            });
            return Ok(TRUE);
        }
        if self.nesting >= MAX_NESTING {
            self.assumptions
                .insert("um evento de applet não foi entregue por aninhamento profundo");
            return Ok(FALSE);
        }
        let saved = SAVED_REGS.map(|reg| self.cpu.read_reg(reg));
        self.nesting += 1;
        let outcome = self.call_guest(handle_event, [applet, evt, u32::from(w), dw], QSORT_BUDGET);
        self.nesting -= 1;
        for (reg, value) in SAVED_REGS.iter().zip(saved) {
            self.cpu.write_reg(*reg, value);
        }
        Ok(match outcome? {
            Outcome::Returned { code } => code,
            // Um tratador que se perde não derruba quem mandou o evento: para ele, ninguém
            // tratou.
            _ => FALSE,
        })
    }
}
