//! O relógio: o tempo do BREW, o vsync e o salto do ocioso.

use super::*;

impl<C: CpuBackend> Machine<C> {
    /// Reconhece a espera ocupada do jogo e faz o tempo passar por ela.
    ///
    /// O Zeebo Sports Peteca não arma timer para o próximo quadro: ele fica num laço que lê o
    /// relógio e cede a vez até o prazo chegar — 69 mil leituras por quadro, 14 milhões de
    /// instruções gastas só em esperar. No console isso não custa nada, porque o tempo passa
    /// sozinho enquanto o ARM gira; aqui cada volta é emulada de verdade, e o jogo rodava seis
    /// vezes mais devagar que o aparelho.
    ///
    /// Quando o jogo só lê o relógio e cede a vez, ele não está progredindo — está esperando, e
    /// adiantar o relógio é exatamente o que o console faz com a passagem do tempo real.
    /// Qualquer outra chamada é sinal de trabalho e zera a contagem, então um quadro que lê o
    /// relógio no meio do que faz continua custando o que custa.
    pub(super) fn note_spin(&mut self, iface: Interface, slot: u32) {
        if self.handset_clock.is_some() { return; }
        let name = iface.method(slot).unwrap_or("");
        let reads_clock = iface == Interface::Helpers
            && matches!(name, "aee_GetTimeMS" | "aee_GetUpTimeMS" | "aee_GetSeconds");
        // Ceder a vez não é trabalho nem espera: é como o laço dá a volta. Não conta para o
        // limite, mas também não desfaz a contagem.
        let yields = matches!(
            (iface, name),
            (Interface::Thread, "Suspend" | "GetResumeCBK") | (Interface::Shell, "Resume")
        );
        // **A volta de um laço de espera não é só a leitura do relógio.** O Zeebo Extreme Rolima
        // gira em torno de cinco chamadas por volta — relógio, `memset`, posição do controle e
        // dois eventos de controle —, 1,35 milhão de cada uma em seis segundos de jogo: 2,45
        // **bilhões** de instruções emuladas para não avançar nada, e o jogo a 79% da velocidade
        // do console. Enquanto qualquer uma delas zerava a contagem, o atalho nunca ligava.
        //
        // Elas são neutras e não zeram a contagem. **Não contam como espera sozinhas**: quem
        // incrementa continua sendo só a leitura de relógio, e é isso que mantém o atalho preso a
        // um laço que pergunta as horas. Um jogo que só limpe memória não chega aqui.
        let neutras = matches!(
            (iface, name),
            (Interface::Helpers, "memset")
                | (
                    Interface::HidDevice,
                    "GetPositionState" | "GetNextButtonEvent"
                )
                | (Interface::Hid, "GetNextConnectEvent")
        );
        if !reads_clock {
            if !yields && !neutras {
                self.spin_polls = 0;
            }
            return;
        }
        self.spin_polls += 1;
        if self.spin_polls > SPIN_THRESHOLD {
            self.clock_us += SPIN_STEP_US;
        }
    }

    /// Adianta o relógio até o próximo timer, quando não há mais nada a fazer agora.
    ///
    /// É o que o console faz: sem trabalho pendente ele dorme, e acorda no vencimento. Pular
    /// esse trecho é o que mantém o tempo virtual honesto — o jogo vê exatamente o intervalo
    /// que pediu entre dois quadros, e não o que o emulador levou para chegar lá.
    pub(super) fn skip_idle_time(&mut self) {
        if self.handset_clock.is_some() { return; }
        let busy = !self.teclas.is_empty()
            || !self.pending_calls.is_empty()
            || !self.posted_calls.is_empty()
            || !self.pending_threads.is_empty()
            || !self.pending_probes.is_empty()
            || !self.pending_blits.is_empty()
            || !self.pending_surface_blits.is_empty()
            || !self.avisos_de_imagem.is_empty()
            || !self.avisos_de_midia.is_empty()
            || self.media.values().any(|state| state.pendente.1 != 0)
            || !self.pending_signals.is_empty()
            || !self.deferred_callbacks.is_empty()
            || self.rsa_pending();
        if busy {
            return;
        }
        let now = self.now_ms();
        let Some(remaining) = self.timers.iter()
            .map(|timer| timer.deadline_ms.wrapping_sub(now) as i32).min() else {
            return;
        };
        // O salto é de **um quadro**, não do vão inteiro. Pular até o timer mais próximo é
        // certo quando o jogo espera o quadro seguinte, e foi para isso que este atalho
        // nasceu; mas quando o único timer armado é longo, o vão não é ociosidade — é o jogo
        // esperando alguém apertar um botão. A Z-Wheel arma 120 000 ms de inatividade na tela
        // de boas-vindas: saltar o vão inteiro fazia o relógio ir a dois minutos de uma vez e
        // disparar o tempo de ocioso antes que qualquer tecla tivesse chance de chegar.
        let vao = remaining.max(0) as u64 * 1000;
        self.clock_us += vao.min(VSYNC_PERIOD_US);
    }

    /// Relógio virtual do guest, em milissegundos.
    pub fn clock_ms(&self) -> u32 {
        self.now_ms()
    }

    /// O relógio que o guest enxerga: o tempo adiantado pelo laço de quadros mais o tempo que
    /// o próprio guest gastou executando instruções.
    ///
    /// As duas parcelas medem coisas diferentes e por isso somam. A primeira é o tempo ocioso
    /// que o emulador pula entre quadros — no console ele passaria de verdade. A segunda faz o
    /// relógio andar *durante* uma fatia de execução, e sem ela um laço de espera do jogo
    /// ("fique aqui até passarem 1,5 s") nunca terminaria: ele lê o relógio, não avança nada e
    /// lê de novo.
    ///
    /// O tempo de execução vem da contagem de instruções, não do relógio do host: assim duas
    /// execuções da mesma ROM dão exatamente o mesmo resultado, o que é o que torna possível
    /// comparar dois quadros e saber que a diferença foi a mudança que fizemos.
    pub(super) fn now_ms(&self) -> u32 {
        (self.now_us() / 1000) as u32
    }

    /// O mesmo relógio, em microssegundos — a resolução em que ele é mantido.
    ///
    /// Milissegundos não bastam: um quadro a 60 Hz dura 16,67 ms, e arredondar isso a cada
    /// quadro acumularia um erro de vários por cento.
    pub(super) fn now_us(&self) -> u64 {
        if let Some(clock) = &self.handset_clock {
            return clock.now_us(std::time::Instant::now());
        }
        self.clock_us + self.cpu.instructions() / INSTRUCTIONS_PER_US
    }

    /// Só a sessão móvel escolhe este modo; as sondas continuam determinísticas.
    pub fn enable_handset_clock(&mut self) {
        if self.font.is_none() {
            // Jogos móveis usam DrawText sem trazer a fonte do console. Reutiliza o TTF
            // já incluído pelo egui; carregar só aqui evita qualquer custo por quadro.
            self.font = egui::FontDefinitions::default().font_data.get("Ubuntu-Light")
                .and_then(|data| crate::video::font::Font::load(data.font.to_vec(), "Ubuntu-Light".into()));
        }
        self.handset_clock = Some(crate::handset::clock::RealtimeClock::new(
            std::time::Instant::now(), self.now_us(),
        ));
    }

    pub fn pause_handset_clock(&mut self, paused: bool) {
        if let Some(clock) = &mut self.handset_clock {
            clock.set_paused(paused, std::time::Instant::now());
        }
    }

    pub fn handset_event_ready(&self) -> bool {
        self.trecho_interrompido.is_none() && self.nesting == 0 && self.current_thread.is_none()
    }

    pub fn clock_us_for_input(&self) -> u64 { self.now_us() }

    /// Espera o retraço vertical, como o `eglSwapBuffers` do console faz.
    ///
    /// Se o quadro demorou mais que um período, o próximo retraço é o primeiro que ainda não
    /// passou — perder um retraço custa o quadro inteiro, e é assim no aparelho também.
    pub(super) fn wait_for_vsync(&mut self) {
        // A apresentação do Android já espera o vsync; não inventar tempo em handset.
        if self.handset_clock.is_some() { return; }
        let now = self.now_us();
        if self.next_vsync_us <= now {
            let missed = (now - self.next_vsync_us) / VSYNC_PERIOD_US + 1;
            self.next_vsync_us += missed * VSYNC_PERIOD_US;
        }
        self.clock_us += self.next_vsync_us - now;
        self.next_vsync_us += VSYNC_PERIOD_US;
    }

    /// Se não há mais nada para o guest fazer: nenhum timer armado, nenhum callback na fila,
    /// nenhuma thread esperando a vez e nenhum interesse registrado em entrada.
    ///
    /// O laço de quadros para aqui. Olhar só para os timers não bastava: quando o jogo passa a
    /// viver dentro de uma thread cooperativa — como o Quake faz depois de carregar o mapa —,
    /// ele cancela o timer e o laço encerrava com o jogo no meio.
    pub fn is_idle(&self) -> bool {
        self.trecho_interrompido.is_none()
            && self.teclas.is_empty()
            && self.timers.is_empty()
            && self.pending_calls.is_empty()
            && self.posted_calls.is_empty()
            && self.pending_threads.is_empty()
            && self.pending_probes.is_empty()
            && self.pending_blits.is_empty()
            && self.pending_surface_blits.is_empty()
            && self.avisos_de_imagem.is_empty()
            && self.pending_signals.is_empty()
            && self.deferred_callbacks.is_empty()
            // **Um jogo que registrou interesse em entrada está esperando o jogador.** Chamar isso
            // de ocioso encerrava a sessão no meio da tela de título: o Crash Nitro Kart parava
            // aos 19,6 s, exatamente quando o intro acaba e o jogo passa a esperar "aperte start"
            // — sem timer armado e com o callback de entrada registrado, ele ficava "ocioso" para
            // esta função. No console o jogo continua na tela esperando; aqui também.
            && self.input_signals.is_empty()
            && !self.rsa_pending()
    }

    /// Quantos timers estão armados.
    pub fn armed_timers(&self) -> usize {
        self.timers.len()
    }

    /// Consulta o prazo sem adiantar o relógio nem consumir eventos. O poll nativo acorda
    /// antes se chegar entrada; uma continuação/fila pronta nunca pode dormir até um timer.
    pub fn next_guest_work_delay(&self) -> Option<std::time::Duration> {
        let ready = self.trecho_interrompido.is_some()
            || !self.quadros_do_update.is_empty()
            || !self.teclas.is_empty()
            || !self.pending_calls.is_empty()
            || !self.posted_calls.is_empty()
            || !self.pending_threads.is_empty()
            || !self.pending_probes.is_empty()
            || !self.pending_blits.is_empty()
            || !self.pending_surface_blits.is_empty()
            || !self.avisos_de_imagem.is_empty()
            || !self.avisos_de_midia.is_empty()
            || self.media.values().any(|state| state.pendente.1 != 0)
            || !self.pending_signals.is_empty()
            || !self.deferred_callbacks.is_empty()
            || self.rsa_pending();
        if ready { return Some(std::time::Duration::ZERO); }
        let now = self.now_ms();
        let timer = self.timers.iter()
            .map(|timer| (timer.deadline_ms.wrapping_sub(now) as i32).max(0) as u64)
            .min().map(std::time::Duration::from_millis);
        let now_us = self.now_us();
        let media = self.media.values()
            .filter(|state| state.state == MM_STATE_PLAY && state.ends_us != u64::MAX)
            .map(|state| std::time::Duration::from_micros(state.ends_us.saturating_sub(now_us)))
            .min();
        let decode = (!self.cargas_em_decodificacao.is_empty()).then_some(std::time::Duration::from_millis(5));
        timer.into_iter().chain(media).chain(decode).min()
    }

    /// O calendário do guest, em segundos desde 6 de janeiro de 1980 GMT.
    ///
    /// A data vem do relógio do host, capturada **uma vez** na criação da máquina, e daí em
    /// diante anda com o relógio virtual. É o meio-termo entre as duas coisas que o projeto
    /// quer: um jogo que pergunta a data recebe uma que existe, e o tempo que ele mede
    /// continua sendo o virtual, sem depender de quanto o emulador demorou.
    pub(super) fn brew_seconds(&self) -> u32 {
        self.epoch_seconds + self.elapsed_ms() / 1000
    }

    /// Milissegundos desde o início da execução.
    pub(super) fn elapsed_ms(&self) -> u32 {
        self.now_ms()
    }
}
