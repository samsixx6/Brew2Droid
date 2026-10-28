//! Ajustes do frontend móvel, sem controles herdados que não chegam aos jogos BREW.
//!
//! O formato Settings continua compartilhado: retirar um controle não apaga preferências
//! antigas nem altera outros frontends. O teclado de telefone é fixo; velocidade/rewind e
//! melhorias do rasterizador GLES do console não são oferecidos neste caminho experimental.

use zeebx::ui::settings::Scaling;

use crate::tema::ALVO;
use crate::{Emulador, Onde, sistema, widgets};

/// Somente as seções que o caminho de aparelhos móveis atende.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aba {
    Geral,
    Graficos,
    Audio,
    Depuracao,
    Sobre,
}

impl Aba {
    const TODAS: [Self; 5] = [
        Self::Geral,
        Self::Graficos,
        Self::Audio,
        Self::Depuracao,
        Self::Sobre,
    ];

    /// A chave do nome da seção no catálogo.
    fn chave(self) -> &'static str {
        match self {
            Self::Geral => "settings.tab.general",
            Self::Graficos => "settings.tab.graphics",
            Self::Audio => "settings.tab.audio",
            Self::Depuracao => "settings.tab.debug",
            Self::Sobre => "settings.tab.about",
        }
    }

    /// Um sinal para a trilha. Ícone reconhecido de longe vale mais que nome lido de perto.
    fn sinal(self) -> &'static str {
        match self {
            Self::Geral => "⛭",
            Self::Graficos => "🖵",
            Self::Audio => "🔊",
            Self::Depuracao => "⏱",
            Self::Sobre => "ℹ",
        }
    }
}

impl Emulador {
    pub(crate) fn ajustes(&mut self, ctx: &egui::Context) {
        // A trilha à esquerda: o "voltar" em cima e as seções embaixo, cada uma um alvo de
        // toque inteiro. Em paisagem ela custa 150 pontos de largura e devolve a tela toda de
        // altura para o conteúdo — bem melhor que uma fila de abas que rola de lado.
        egui::SidePanel::left("trilha")
            .exact_width(150.0)
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(10.0);
                let voltar =
                    ui.add_sized([ui.available_width(), ALVO], egui::Button::new(format!("◀  {}", self.catalogo.get("handset.settings.back"))));
                if voltar.clicked() {
                    self.onde = Onde::Biblioteca;
                }
                // **Sem foco não há navegação por setas.** O egui move o foco na direção da seta,
                // mas só a partir de um foco que já exista -- do nada, quem o concede é o `Tab`,
                // que nenhum controle produz. Entrando na tela sem foco, o direcional não movia
                // coisa nenhuma aqui. Damos o primeiro, e daí o egui se vira: seta anda, Enter
                // aperta.
                if ctx.memory(|m| m.focused()).is_none() {
                    voltar.request_focus();
                }
                ui.add_space(12.0);
                for aba in Aba::TODAS {
                    let nome = format!("{}  {}", aba.sinal(), self.catalogo.get(aba.chave()));
                    let botao = egui::Button::new(nome)
                        .min_size(egui::vec2(ui.available_width(), ALVO))
                        .selected(self.aba == aba);
                    if ui.add(botao).clicked() {
                        self.aba = aba;
                        // Trocar de seção fecha a dica aberta: ela era de outra tela.
                        self.dica = None;
                    }
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let titulo = self.catalogo.get(self.aba.chave()).to_string();
            ui.add_space(10.0);
            ui.heading(titulo);
            ui.add_space(6.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                let mudou = match self.aba {
                    Aba::Geral => self.aba_geral(ui),
                    Aba::Graficos => self.aba_graficos(ui),
                    Aba::Audio => self.aba_audio(ui),
                    Aba::Depuracao => self.aba_depuracao(ui),
                    Aba::Sobre => self.aba_sobre(ui),
                };
                if mudou {
                    self.salva();
                }
                ui.add_space(24.0);
            });
        });
    }

    /// Idioma e pasta de ROMs.
    fn aba_geral(&mut self, ui: &mut egui::Ui) -> bool {
        let mut mudou = false;

        widgets::secao(ui, self.catalogo.get("settings.roms_folder"));
        let pasta = self.roms();
        let onde = match pasta.as_os_str().is_empty() {
            true => self.catalogo.get("library.no_folder").to_string(),
            false => pasta.display().to_string(),
        };
        if widgets::navega(ui, self.catalogo.get("settings.browse"), &onde) {
            self.onde = Onde::Seletor(match pasta.is_dir() {
                true => pasta,
                false => std::path::PathBuf::from("/sdcard"),
            });
        }
        let contagem = self
            .catalogo
            .format("library.count", &[("count", &self.jogos.len().to_string())]);
        let pode_varrer = self.varredura.is_none();
        if ui
            .add_enabled_ui(pode_varrer, |ui| {
                widgets::navega(ui, self.catalogo.get("library.rescan"), &contagem)
            })
            .inner
        {
            self.recarrega();
        }

        widgets::secao(ui, self.catalogo.get("settings.language"));
        // Um idioma por faixa, marcado o que vale. São dois ou três: uma lista de escolha é
        // mais direta que uma caixa que abre por cima da tela.
        let idiomas: Vec<(String, String)> = self
            .catalogo
            .languages()
            .iter()
            .map(|idioma| (idioma.code.clone(), idioma.name.clone()))
            .collect();
        let atual = self.catalogo.current().to_string();
        let mut escolhido = None;
        for (codigo, nome) in &idiomas {
            let botao = egui::Button::new(nome)
                .min_size(egui::vec2(ui.available_width(), ALVO))
                .selected(*codigo == atual);
            if ui.add(botao).clicked() && *codigo != atual {
                escolhido = Some(codigo.clone());
            }
        }
        if let Some(codigo) = escolhido {
            self.catalogo.select(&codigo);
            self.settings.language = Some(codigo);
            mudou = true;
        }

        ui.add_space(14.0);
        ui.label(
            egui::RichText::new(self.catalogo.format(
                "settings.saved_at",
                &[("path", &self.arquivo.display().to_string())],
            ))
            .small()
            .color(ui.visuals().weak_text_color()),
        );
        mudou
    }

    /// Apresentação do framebuffer móvel; o 3D interno do jogo não passa pelo GLES HLE.
    fn aba_graficos(&mut self, ui: &mut egui::Ui) -> bool {
        let catalogo = &self.catalogo;
        let graficos = &mut self.settings.graphics;
        let dica = &mut self.dica;
        let mut mudou = false;
        ui.weak(catalogo.get("handset.graphics.native_resolution"));
        ui.add_space(8.0);
        let modos: Vec<(Scaling, String)> = Scaling::ALL
            .iter()
            .map(|modo| (*modo, catalogo.get(modo.key()).to_string()))
            .collect();
        mudou |= widgets::segmentado(
            ui, "scaling", catalogo.get("graphics.scaling"),
            Some(catalogo.get("graphics.scaling.integer.hint")), dica,
            &mut graficos.scaling, &modos,
        );
        if graficos.scaling == Scaling::Stretch {
            mudou |= widgets::interruptor(
                ui, "keep_aspect", catalogo.get("handset.graphics.keep_aspect"),
                None, dica, &mut graficos.keep_aspect,
            );
        }
        mudou |= widgets::interruptor(
            ui, "smooth", catalogo.get("graphics.smooth"),
            None, dica, &mut graficos.smooth,
        );
        // O campo herdado também escolhe RGB565 na GPU ou conversão/texture upload no
        // frontend. Mantê-lo com o nome correto preserva a preferência e o caminho rápido.
        mudou |= widgets::interruptor(
            ui, "gpu_presentation", catalogo.get("handset.graphics.gpu_presentation"),
            Some(catalogo.get("handset.graphics.gpu_presentation.hint")), dica,
            &mut graficos.gpu_rasterizer,
        );
        mudou
    }

    /// Música e efeitos; opções de SoundFont afetam somente a trilha MIDI.
    fn aba_audio(&mut self, ui: &mut egui::Ui) -> bool {
        let catalogo = &self.catalogo;
        let audio = &mut self.settings.audio;
        let dica = &mut self.dica;
        let mut mudou = widgets::interruptor(
            ui,
            "audio",
            catalogo.get("audio.enabled"),
            Some(catalogo.get("handset.audio.hint")),
            dica,
            &mut audio.enabled,
        );
        mudou |= widgets::deslizante(
            ui,
            "volume",
            catalogo.get("audio.volume"),
            None,
            dica,
            &mut audio.volume,
            0..=100,
            |valor| format!("{valor}%"),
        );

        // O banco é aberto quando o jogo abre: a dica diz que a troca vale para o próximo.
        widgets::secao(ui, self.catalogo.get("audio.soundfont"));
        ui.weak(self.catalogo.get("handset.audio.soundfont.hint"));
        let banco = self.settings.audio.soundfont.clone();
        let mostrado = match &banco {
            Some(caminho) => caminho.display().to_string(),
            None => self.catalogo.get("handset.audio.soundfont.auto").to_string(),
        };
        if widgets::navega(ui, self.catalogo.get("settings.browse"), &mostrado) {
            let inicio = banco
                .as_deref()
                .and_then(std::path::Path::parent)
                .filter(|pasta| pasta.is_dir())
                .map(std::path::Path::to_path_buf)
                .unwrap_or_else(|| std::path::PathBuf::from("/sdcard"));
            self.onde = Onde::SeletorDeBanco(inicio);
        }
        if banco.is_some()
            && ui
                .add_sized(
                    [ui.available_width(), 52.0],
                    egui::Button::new(self.catalogo.get("audio.soundfont.clear")),
                )
                .clicked()
        {
            self.settings.audio.soundfont = None;
            mudou = true;
        }
        mudou |= widgets::interruptor(
            ui,
            "efeitos_midi",
            self.catalogo.get("audio.midi_effects"),
            Some(self.catalogo.get("handset.audio.midi_effects.hint")),
            &mut self.dica,
            &mut self.settings.audio.midi_effects,
        );

        if mudou {
            let audio = self.settings.audio.clone();
            if let Some(sessao) = &mut self.sessao {
                if let Some(erro) = sessao.set_audio(audio.enabled, audio.volume) {
                    log::error!("sem som: {erro}");
                }
            }
        }
        mudou
    }

    fn aba_depuracao(&mut self, ui: &mut egui::Ui) -> bool {
        let catalogo = &self.catalogo;
        let debug = &mut self.settings.debug;
        let dica = &mut self.dica;
        let fps_changed = widgets::interruptor(
            ui,
            "fps-counter",
            catalogo.get("debug.fps_counter"),
            Some(catalogo.get("debug.fps_counter.hint")),
            dica,
            &mut debug.fps_counter,
        );
        if fps_changed {
            self.fps.reset();
        }
        let mut mudou = widgets::interruptor(
            ui,
            "overlay",
            catalogo.get("debug.overlay"),
            Some(catalogo.get("debug.hint")),
            dica,
            &mut debug.overlay,
        ) || fps_changed;
        // As linhas do painel só fazem sentido com o painel ligado.
        ui.add_enabled_ui(debug.overlay, |ui| {
            widgets::secao(ui, catalogo.get("handset.debug.fields"));
            for (chave, rotulo, valor) in [
                ("speed", "debug.speed", &mut debug.speed),
                ("clock", "debug.clock", &mut debug.clock),
                ("memory", "debug.memory", &mut debug.memory),
                ("timeline", "debug.timeline", &mut debug.timeline),
            ] {
                mudou |= widgets::interruptor(ui, chave, catalogo.get(rotulo), None, dica, valor);
            }
        });
        mudou
    }

    fn aba_sobre(&mut self, ui: &mut egui::Ui) -> bool {
        ui.add_space(6.0);
        ui.label(self.tr("handset.about.tagline"));
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(
                self.catalogo
                    .format("about.version", &[("version", env!("CARGO_PKG_VERSION"))]),
            )
            .small()
            .color(ui.visuals().weak_text_color()),
        );
        ui.add_space(16.0);
        // Um link aqui não abre nada sozinho: é a `Intent` de VIEW que manda o endereço para o
        // navegador do aparelho.
        for (chave, endereco) in [
            ("about.repository", zeebx::ui::REPOSITORIO),
            ("about.discord", zeebx::ui::DISCORD),
        ] {
            if widgets::navega(ui, self.tr(chave), endereco) {
                sistema::abre_endereco(&self.app, endereco);
            }
        }
        false
    }
}
