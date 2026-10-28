//! O Zeebx no Android: a mesma emulação, outra tela.
//!
//! Este pacote é o frontend inteiro. Ele não sabe emular nada — quem faz isso é a
//! [`zeebx::session::Session`], a mesma que a janela do desktop e a linha de comando giram. O
//! que existe aqui é o que muda de plataforma: de onde vêm as ROMs, como o quadro chega à tela
//! e como o botão do aparelho vira botão do console.
//!
//! **O laço de eventos é nosso, e não do eframe.** A razão está em [`entrada`]: pelo winit os
//! botões do controle chegam como `Key::Unidentified` e os eixos do manche não chegam. Girando
//! a [`android_activity::AndroidApp`] direto, a entrada é a que o Android mandou. O preço é o
//! ciclo de vida da janela e o contexto de GL, que ficam em [`tela`].
//!
//! As telas estão uma por arquivo — [`biblioteca`], [`ajustes`], [`seletor`] e [`jogo`] —, e os
//! textos e as opções saem do próprio núcleo: o mesmo `Settings` do desktop, o mesmo catálogo
//! de idiomas, o mesmo painel de depuração.
//!
//! **A apresentação usa a placa.** O contexto GL da [`tela`] também pode atender a sessão,
//! mas os três módulos móveis entregam pixels prontos. Seus ajustes mostram escala/filtro
//! da imagem, sem prometer as melhorias GLES do console para o rasterizador ARM dos jogos.

mod ajustes;
mod biblioteca;
mod density;
mod entrada;
mod input_dispatch;
mod estado;
mod fps;
mod frame_profile;
mod audio_focus;
mod native_ui;
mod native_ui_model;
mod save_storage;
mod game_options;
mod button_mapping;
mod profile_store;
mod profile_runtime;
mod keypad_layout;
mod handset;
mod zeebo_controls;
mod zeebo_mapping;
mod zeebo_art;
mod zeebo_android;
mod jogo;
mod pacing;
mod probe;
mod seletor;
mod sistema;
mod tela;
mod tema;
mod toque;
mod widgets;

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use android_activity::{AndroidApp, MainEvent, PollEvent, WindowManagerFlags};

use zeebx::input::Pad;
use zeebx::session::Session;
use zeebx::storage::StoragePaths;
use zeebx::ui::i18n::Catalog;
use zeebx::ui::library::{self, Game};
use zeebx::ui::settings::{ModoDosControlesNaTela, Settings};

use entrada::Entrada;
use tela::{Placa, Tela};

/// O ponto em que o Android entra.
///
/// A `NativeActivity` carrega o `.so`, acha este símbolo e o chama numa linha de execução
/// própria. Daqui para baixo, tudo é nosso.
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("Zeebx"),
    );
    // **O registro do núcleo tem um destino no Android.** Até aqui ele era o único frontend que
    // não drenava o anel: o desktop imprime no `stderr`, o core manda pelo `retro_log`, o headless
    // imprime — e o Android não tinha onde. Aqui o destino é o `log`, que o `android_logger`
    // acabou de instalar e que sai em `logcat` com a etiqueta e a gravidade certas.
    //
    // O nível começa no padrão do núcleo (`Aviso`); a variável `ZEEBX_LOG` vale se existir.
    zeebx::registro::le_do_ambiente();

    // Um pânico do Rust escreve no stderr, e o Android joga o stderr fora: sem isto o
    // aplicativo morre sem deixar uma linha no `logcat`. O gancho manda a mensagem e o lugar
    // para o log do sistema antes de o processo cair.
    std::panic::set_hook(Box::new(|info| {
        let onde = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "lugar desconhecido".to_string());
        log::error!("pânico em {onde}: {info}");
    }));

    log::info!("Zeebx começando");

    let interno = app
        .internal_data_path()
        .unwrap_or_else(|| PathBuf::from("/data/local/tmp"));

    // **O Android não define `HOME`.** Sem ele o `settings::config_dir` do núcleo cai no seu
    // último recurso, o diretório corrente — que num aplicativo Android é `/`. O carregador
    // então tenta extrair os `.zip` em `/cache` e montar a raiz do aparelho em `/aparelho`, e
    // as duas coisas falham com "permission denied" num erro que *parece* ser da ROM.
    //
    // Definir `HOME` para o diretório privado do aplicativo faz todas as regras que já existem
    // apontarem para o lugar certo, sem uma linha de `cfg` no núcleo.
    unsafe { std::env::set_var("HOME", &interno) };

    // Os ajustes moram no diretório interno do aplicativo, que é só dele: o `Settings` do núcleo
    // é o mesmo do desktop, e aceita um caminho explícito justamente para isto.
    let arquivo = interno.join("zeebx.json");

    // A pasta de ROMs é escolhida pelo usuário e guardada nos ajustes. O padrão é `zeebo/roms`
    // na raiz do armazenamento, que é onde uma coleção costuma estar — e lê-la exige o "acesso a
    // todos os arquivos", que o manifesto declara e o aplicativo pede.
    let externo = app.external_data_path().map(|dir| dir.join("roms"));
    if let Some(dir) = &externo {
        let _ = std::fs::create_dir_all(dir);
    }
    let padrao = PathBuf::from("/sdcard/zeebo/roms");

    // A pasta interna do aplicativo serve de atalho no seletor: ela é a única que o Android
    // deixa ler sem permissão nenhuma.
    let minha_pasta = interno.join("roms");
    let _ = std::fs::create_dir_all(&minha_pasta);

    let launch = match native_ui::launch(&app) {
        Ok(launch) if launch.game.is_some() || launch.probe => launch,
        Ok(_) => { native_ui::finish(&app, Some("Choose a game from the native library.")); return; }
        Err(error) => { native_ui::finish(&app, Some(&error)); return; }
    };
    let gameplay = launch.game.is_some();
    native_ui::GAME_ACTIVE.store(gameplay, std::sync::atomic::Ordering::Release);
    if gameplay { native_ui_model::begin_gameplay(); }

    // Tela cheia de verdade: num aparelho de mão a barra de status só rouba altura do jogo — e
    // era ela que cobria o caminho no seletor de pastas.
    app.set_window_flags(WindowManagerFlags::FULLSCREEN, WindowManagerFlags::empty());
    // A de navegação a bandeira não tira: ela é da view, não da janela, e some por JNI.
    sistema::esconde_a_barra_de_navegacao(&app);
    sistema::controla_volume_de_midia(&app);

    let mut emulador = Emulador::novo(arquivo, padrao, externo, minha_pasta, app.clone());
    gira(app, &mut emulador, launch);
    drop(emulador);
    let _ = input_dispatch::take_back();
    if gameplay { native_ui_model::end_gameplay(); }
    native_ui::GAME_ACTIVE.store(false, std::sync::atomic::Ordering::Release);
}

/// Manda para o `logcat` o que o núcleo registrou desde a última volta.
///
/// A gravidade é preservada: um aviso do núcleo sai como `WARN` no `logcat`, e não como `INFO`
/// como tudo o mais. É o que faz `adb logcat *:W` mostrar o que interessa sem o resto.
fn despeja_o_registro() {
    use zeebx::registro::Nivel;
    for linha in zeebx::registro::drena() {
        let texto = format!("[{}] {}: {}", linha.nivel.etiqueta(), linha.alvo, linha.texto);
        match linha.nivel {
            Nivel::Depuracao => log::debug!("{texto}"),
            Nivel::Informacao => log::info!("{texto}"),
            Nivel::Aviso => log::warn!("{texto}"),
            Nivel::Erro | Nivel::Fatal => log::error!("{texto}"),
        }
    }
}

/// O laço: colhe eventos, deixa o jogo andar, desenha, repete.
fn gira(app: AndroidApp, emulador: &mut Emulador, launch: native_ui::Launch) {
    let mut pending_launch = Some(launch);
    let ctx = egui::Context::default();
    // A interface do núcleo foi desenhada para um monitor. Num aparelho de mão, à distância de
    // um braço, o mesmo tamanho em pontos fica ilegível: a densidade do aparelho é o piso, e
    // daí para cima é o que o dedo precisa para acertar um botão.
    let mut pontos_por_pixel = escala(&app);
    ctx.set_pixels_per_point(pontos_por_pixel);
    tema::aplica(&ctx);

    let mut entrada = Entrada::nova(pontos_por_pixel);
    // A placa nasce com a primeira janela e não morre mais: ver [`tela::Placa`]. A tela é a
    // superfície, e essa vai e vem.
    let mut placa: Option<Placa> = None;
    let mut tela: Option<Tela> = None;
    let mut visivel = false;
    let mut sair = false;
    let comeco = Instant::now();
    let mut handset_pacer = pacing::HandsetPacer::default();
    input_dispatch::install_waker(&app);
    let _=input_dispatch::take_menu();
    let mut idle_wait = Duration::ZERO;
    // Marcador privado da bancada para repetir A/B no mesmo APK, sem mudar o jogo/relógio.
    let pacing_enabled = !emulador.arquivo.with_file_name(".brew-dev-pacing-baseline").is_file();
    let mut frame_profile = emulador.arquivo.with_file_name(".brew-dev-frame-profile").is_file()
        .then(|| frame_profile::FrameProfile::new(pacing_enabled));
    let audio_profile = emulador.arquivo.with_file_name(".brew-dev-audio-profile").is_file();
    let mut audio_profile_at = Instant::now();
    let mut audio_focus = audio_focus::AudioFocus::default();

    while !sair {
        // O registro do núcleo sai por aqui, uma vez por volta do laço. Com o anel vazio — o
        // caso comum, no nível padrão — isto é um cadeado e uma leitura.
        despeja_o_registro();
        // Com janela, o laço gira o mais rápido que a troca de buffers deixar; sem ela, não há
        // o que desenhar e esperar é o certo — é o que mantém o aplicativo parado em segundo
        // plano em vez de queimar bateria.
        let espera = match visivel && tela.is_some() {
            true => Some(idle_wait),
            false => None,
        };

        app.poll_events(espera, |evento| match evento {
            PollEvent::Main(MainEvent::InitWindow { .. }) => {
                handset_pacer.reset();
                if let Some(profile) = &mut frame_profile { profile.reset(); }
                pontos_por_pixel = escala(&app);
                ctx.set_pixels_per_point(pontos_por_pixel);
                entrada.define_escala(pontos_por_pixel);
                let limitar = emulador.settings.velocidade.limite_fps.limita_velocidade();
                let feita = match placa.as_mut() {
                    // A segunda janela em diante reaproveita o contexto: é o que mantém vivos
                    // os objetos de GL que a sessão criou.
                    Some(placa) => placa.refaz_a_tela(&app, limitar),
                    None => Placa::nova(&app, limitar).map(|(nova, primeira)| {
                        emulador.gl = Some(nova.gl.clone());
                        placa = Some(nova);
                        primeira
                    }),
                };
                match feita {
                    Ok(nova) => {
                        tela = Some(nova);
                        // Abertura única, com EGL pronto. Os menus nativos não varrem jogos
                        // em paralelo com a sessão nem atravessam JNI durante os quadros.
                        if let Some(launch) = pending_launch.take() {
                            if launch.probe { native_ui::finish(&app, None); }
                            else if let Some(path) = launch.game {
                                emulador.abre(&path);
                                if emulador.sessao.is_none() { native_ui::finish(&app, emulador.erro.as_deref()); }
                            }
                        }
                    }
                    Err(erro) => { log::error!("a tela não subiu: {erro}"); native_ui::finish(&app, Some(&erro)); }
                }
            }
            PollEvent::Main(MainEvent::TerminateWindow { .. }) => {
                emulador.handset_routes.clear();emulador.handset_touch.cancel();
                emulador.handset_touch.viewport = None;
                emulador.release_zeebo_inputs();emulador.zeebo_controls.resize(0.,0.);
                if let Some(session) = &mut emulador.sessao { session.release_handset_keys(); }
                handset_pacer.reset();
                if let Some(profile) = &mut frame_profile { profile.reset(); }
                tela = None;
                emulador.fps.reset();
            }
            PollEvent::Main(MainEvent::WindowResized { .. })
            | PollEvent::Main(MainEvent::ContentRectChanged { .. })
            | PollEvent::Main(MainEvent::ConfigChanged { .. }) => {
                // A fila é lida antes da próxima pintura: não usar o retângulo/densidade antigos.
                emulador.handset_routes.clear();emulador.handset_touch.cancel();
                emulador.handset_touch.viewport = None;
                emulador.release_zeebo_inputs();emulador.zeebo_controls.resize(0.,0.);
                if let Some(session) = &mut emulador.sessao { session.release_handset_keys(); }
                handset_pacer.reset();
                if let Some(profile) = &mut frame_profile { profile.reset(); }
                pontos_por_pixel = escala(&app);
                ctx.set_pixels_per_point(pontos_por_pixel);
                entrada.define_escala(pontos_por_pixel);
                if let (Some(placa), Some(tela)) = (&placa, &mut tela) {
                    placa.redimensiona(tela, &app);
                }
            }
            PollEvent::Main(MainEvent::Resume { .. }) | PollEvent::Main(MainEvent::GainedFocus) => {
                handset_pacer.reset();
                if let Some(profile) = &mut frame_profile { profile.reset(); }
                visivel = true;
                emulador.fps.reset();
                if let (Some(placa), Some(tela)) = (&placa, &tela) {
                    placa.retoma(tela);
                }
            }
            PollEvent::Main(MainEvent::Pause) | PollEvent::Main(MainEvent::LostFocus) => {
                handset_pacer.reset();
                if let Some(profile) = &mut frame_profile { profile.reset(); }
                // O Android pode tirar o foco antes de entregar o KeyUp/MotionEvent final do
                // controle. Se mantivermos o estado, a direção fica presa quando a atividade
                // volta. No console, perder o aparelho equivale a soltar tudo.
                emulador.pad = Pad::default();
                emulador.sobreposicao.solta();
                emulador.handset_keys.clear();
                emulador.handset_input.release_all();
                emulador.handset_routes.clear();emulador.handset_touch.cancel();
                emulador.handset_touch.viewport = None;
                emulador.release_zeebo_inputs();emulador.zeebo_controls.resize(0.,0.);
                if let Some(session) = &mut emulador.sessao {
                    session.release_handset_keys();
                    session.congela_o_som(true);
                }
                entrada.fisicos.clear();
                emulador.fps.reset();
                visivel = false
            }
            PollEvent::Main(MainEvent::Destroy) => sair = true,
            _ => {}
        });
        idle_wait = Duration::ZERO;

        if sair {
            break;
        }
        emulador.apply_native_menu(&ctx,input_dispatch::take_menu());
        let focus_paused = audio_focus.update(&app,visivel && emulador.onde == Onde::Jogo
            && emulador.sessao.is_some() && emulador.settings.audio.enabled);
        if focus_paused != emulador.audio_focus_paused {
            emulador.audio_focus_paused = focus_paused;
            emulador.handset_keys.clear();
                emulador.handset_input.release_all();
            emulador.handset_routes.clear();emulador.handset_touch.cancel();
            emulador.release_zeebo_inputs();
            if let Some(session) = &mut emulador.sessao {
                session.release_handset_keys();
                session.congela_o_som(focus_paused || !visivel || emulador.pausado || emulador.confirmando);
            }
            handset_pacer.reset();
        }
        if audio_profile && audio_profile_at.elapsed() >= Duration::from_secs(5) {
            audio_profile_at = Instant::now();
            if let Some(snapshot) = emulador.sessao.as_mut().and_then(Session::audio_snapshot) {
                use std::io::Write;
                let path = emulador.arquivo.with_file_name("handset-audio-profile.jsonl");
                let result = std::fs::OpenOptions::new().append(true).create(true).open(path)
                    .and_then(|mut file| writeln!(file,"{}",snapshot));
                if let Err(error) = result { log::warn!("audio profile: {error}"); }
            }
        }

        let held_before = emulador.handset_keys.held();
        let mut pen_changed = false;
        entrada.comeca_quadro();
        // O dedo vai para os controles na tela só com o jogo rodando de fato. Com a pergunta do
        // "voltar" aberta ele tem de alcançar os botões dela, que são do egui.
        let no_jogo = emulador.onde == Onde::Jogo
            && !emulador.confirmando
            && emulador.settings.controles_na_tela.modo != ModoDosControlesNaTela::Nunca;
        let handset_game = emulador.onde == Onde::Jogo && !emulador.confirmando && !emulador.pausado && !emulador.audio_focus_paused
            && emulador.sessao.as_ref().is_some_and(Session::is_handset);
        if !handset_game {
            emulador.handset_keys.clear();
                emulador.handset_input.release_all();
            emulador.handset_routes.clear();emulador.handset_touch.cancel();
            emulador.release_zeebo_inputs();
            emulador.handset_touch.viewport = None;
            if let Some(session) = &mut emulador.sessao { session.release_handset_keys(); }
        }
        if !no_jogo {
            emulador.sobreposicao.solta();
        }
        let removed = button_mapping::take_removed_devices();
        if !removed.is_empty() {
            for device in removed { emulador.handset_input.release_device(device);emulador.zeebo_input.release_device(device); }
            emulador.handset_input.apply(&mut emulador.handset_keys);
            if handset_game { emulador.sync_handset_keys(); }
        }
        // O Back físico já foi consumido pela Activity; entrega cada borda ao jogo uma vez.
        // O sentinela também vale fora do jogo para impedir teclas presas após perder foco.
        input_dispatch::deliver_physical_back(|device, scan, down| {
            let changed = if device < 0 {
                emulador.handset_input.release_all();emulador.zeebo_input.release_all();
                true
            } else if handset_game && emulador.zeebo_mode {
                emulador.zeebo_input.physical_back(device,scan,down)
            } else if handset_game {
                emulador.handset_input.physical_back(device, scan, down)
            } else {
                false
            };
            if changed {
                emulador.handset_input.apply(&mut emulador.handset_keys);
                if handset_game { emulador.sync_handset_keys(); }
            }
        });
        if let Ok(mut fila) = app.input_events_iter() {
            // Somente o perfil opt-in captura Volume Up/Down durante o jogo ativo.
            let volume_keys_mappable = handset_game && if emulador.zeebo_mode {emulador.zeebo_input.volume_keys_mappable()}else{emulador.handset_input.volume_keys_mappable()};
            while fila.next(|evento| input_dispatch::dispatch_with_volume(evento, volume_keys_mappable, || {
                if handset_game && emulador.zeebo_mode {
                    if zeebo_android::receive(&mut emulador.zeebo_controls,&mut emulador.zeebo_input,evento,pontos_por_pixel,&app,emulador.keypad_settings.vibration) {emulador.sync_handset_keys();return;}
                    // Nenhuma entrada do console atravessa os defaults numéricos do telefone.
                    return;
                }
                if handset_game && let Some(pen) = handset::receive_touch(&mut emulador.handset_keys,&mut emulador.handset_touch,&mut emulador.handset_routes,evento,pontos_por_pixel) {
                    if let Some(event)=pen {
                        pen_changed=true;
                        if let Some(session)=&mut emulador.sessao && let Err(error)=session.set_handset_pen(event){
                            log::warn!("{error}");emulador.handset_routes.clear();emulador.handset_touch.cancel();emulador.handset_routes.clear();session.release_handset_keys();
                        }
                    }
                    if handset::vibration_press(&emulador.handset_keys,evento,pontos_por_pixel,emulador.keypad_settings.vibration){sistema::vibra_teclado(&app);}
                    emulador.sync_handset_keys();return;
                }
                if handset_game && handset::receive_with_app(&mut emulador.handset_keys, &mut emulador.handset_input, evento, pontos_por_pixel, &app) {
                    if handset::vibration_press(&emulador.handset_keys,evento,pontos_por_pixel,emulador.keypad_settings.vibration){sistema::vibra_teclado(&app);}
                    emulador.sync_handset_keys();
                    return;
                }
                let sobreposicao = no_jogo.then_some(&mut emulador.sobreposicao);
                entrada.recebe(evento, &mut emulador.pad, sobreposicao);
            })) {}
        }
        // O estado dos botões físicos continua disponível ao caminho de entrada.
        emulador.fisicos.clone_from(&entrada.fisicos);
        // O "voltar" do Android vale em qualquer tela. O botão 2 do controle vale em todas menos
        // no jogo, onde ele é do jogador -- ali quem fecha é o "voltar" do aparelho.
        if input_dispatch::take_back() || entrada.voltar || (entrada.voltar_da_interface && emulador.onde != Onde::Jogo) {
            emulador.voltar();
        }

        let (Some(placa), Some(tela)) = (placa.as_mut(), tela.as_ref()) else {
            continue;
        };
        if !visivel {
            continue;
        }

        let active_handset = emulador.onde == Onde::Jogo
            && !emulador.confirmando && !emulador.pausado && !emulador.audio_focus_paused
            && emulador.sessao.as_ref().is_some_and(Session::is_handset);
        let mut handset_frame = None;
        if active_handset {
            let work_started = frame_profile.as_ref().map(|_| Instant::now());
            let step = emulador.avanca_jogo();
            if let (Some(profile), Some(started)) = (&mut frame_profile, work_started) {
                profile.work(comeco.elapsed(), started.elapsed(), step);
            }
            if let Some(session) = emulador.sessao.as_ref() {
                let screen = session.screen();
                let frame = zeebx::handset::fps::FrameStamp {
                    cpu_series: screen.serie(), cpu_writes: screen.escritas(),
                    gpu_frames: session.rendered_gl_frames(),
                };
                let now = comeco.elapsed();
                let input_changed = pen_changed || held_before != emulador.handset_keys.held();
                if pacing_enabled && !handset_pacer.needs_present(now, frame, input_changed) {
                    if step == zeebx::session::Step::Ahead {
                        idle_wait = handset_pacer.idle_wait(now, session.next_guest_work_delay());
                    }
                    continue;
                }
                handset_frame = Some(frame);
            }
        } else {
            handset_pacer.reset();
            if let Some(profile) = &mut frame_profile { profile.reset(); }
        }

        let [largura, altura] = tela.tamanho;
        let cru = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(
                    largura as f32 / pontos_por_pixel,
                    altura as f32 / pontos_por_pixel,
                ),
            )),
            time: Some(comeco.elapsed().as_secs_f64()),
            // O limite da GPU do aparelho, e não o palpite do egui: é ele que decide até onde o
            // atlas de fontes pode crescer.
            max_texture_side: Some(placa.pincel.max_texture_side()),
            events: std::mem::take(&mut entrada.eventos),
            ..Default::default()
        };

        let ui_started = frame_profile.as_ref().filter(|_| active_handset).map(|_| Instant::now());
        let saida = ctx.run(cru, |ctx| emulador.desenha(ctx));
        let primitivas = ctx.tessellate(saida.shapes, saida.pixels_per_point);
        if let (Some(profile), Some(started)) = (&mut frame_profile, ui_started) {
            profile.ui(started.elapsed());
        }
        let draw_started = frame_profile.as_ref().filter(|_| active_handset).map(|_| Instant::now());
        let presented = placa.pinta(
            tela,
            &primitivas,
            &saida.textures_delta,
            saida.pixels_per_point,
            emulador.settings.velocidade.limite_fps.limita_velocidade(),
        );
        if let Some(frame) = handset_frame {
            handset_pacer.presented(comeco.elapsed(), frame, presented);
        }
        if let (Some(profile), Some(started)) = (&mut frame_profile, draw_started) {
            profile.present(comeco.elapsed(), started.elapsed(), presented, handset_frame);
            if let Some(line) = profile.snapshot(comeco.elapsed()) {
                use std::io::Write;
                let path = emulador.arquivo.with_file_name("handset-frame-profile.jsonl");
                let result = std::fs::OpenOptions::new().append(true).create(true).open(path)
                    .and_then(|mut file| file.write_all(line.as_bytes()));
                if let Err(error) = result { log::warn!("frame profile: {error}"); }
            }
        }
        // A taxa pertence ao jogo entregue à janela, não ao número de repaints do Android.
        if emulador.settings.debug.fps_counter && emulador.onde == Onde::Jogo {
            emulador.fps.presented(comeco.elapsed(), presented);
        }
    }

    input_dispatch::remove_waker(&app);
    audio_focus.update(&app,false);
    log::info!("Zeebx encerrando");
}

/// Qual tela está no ar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Onde {
    /// A grade de jogos.
    Biblioteca,
    /// As configurações.
    Ajustes,
    /// O navegador de pastas, na pasta dada, para escolher a de ROMs.
    Seletor(PathBuf),
    /// O mesmo navegador, para escolher o banco `.sf2` da música MIDI.
    SeletorDeBanco(PathBuf),
    /// Um jogo rodando.
    Jogo,
    /// O editor dos controles na tela.
    EditaToque,
}

/// O aplicativo inteiro: o que está na tela e o que sustenta cada tela.
pub struct Emulador {
    /// Onde os ajustes são lidos e gravados. É um arquivo do mesmo formato do desktop.
    arquivo: PathBuf,
    settings: Settings,
    /// Os textos da interface, do mesmo catálogo que o desktop usa. Os idiomas de fábrica
    /// vêm embutidos no binário, então funcionam sem nenhum arquivo no aparelho.
    catalogo: Catalog,
    onde: Onde,
    aba: ajustes::Aba,
    /// Qual dica de configuração está aberta. Os textos de ajuda do Zeebx são longos, e todos
    /// abertos ao mesmo tempo viram uma parede: só um por vez.
    dica: widgets::Aberta,
    /// A pasta privada do aplicativo. Sempre legível, sem permissão nenhuma — é o atalho que
    /// funciona mesmo quando o "acesso a todos os arquivos" não foi concedido.
    minha_pasta: PathBuf,
    /// Raiz privada do aplicativo. Os save states ficam aqui, separados das ROMs escolhidas pelo
    /// usuário: atualizar/trocar a coleção não apaga o ponto em que cada jogo estava.
    estados_raiz: PathBuf,
    /// Cache, sistema compartilhado e saves persistentes fornecidos explicitamente ao núcleo.
    storage: StoragePaths,
    /// A ponte para a atividade: é por ela que se pergunta e se pede a permissão.
    app: AndroidApp,
    /// O contexto de GL da tela, quando ela já subiu. É o que a sessão usa para preencher o 3D
    /// na placa; sem ele, o 3D é da CPU.
    gl: Option<std::sync::Arc<glow::Context>>,
    /// Quem põe o quadro da placa na tela. Mora atrás de um `Arc<Mutex<_>>` porque quem o usa é
    /// um fecho que o egui guarda e chama no meio da pintura — ele não pode emprestar do
    /// `Emulador`. Nasce na primeira pintura, que é a primeira vez que há contexto corrente.
    pintor: std::sync::Arc<std::sync::Mutex<Option<zeebx::ui::gpu::Pintor>>>,
    /// Os jogos da pasta escolhida, com título e capa, lidos pelo mesmo `library::scan` do
    /// desktop.
    jogos: Vec<Game>,
    /// Resultado de uma varredura em segundo plano. Enquanto existe, a lista antiga continua
    /// utilizavel e a interface nao trava lendo ZIPs/MIFs.
    varredura: Option<
        std::sync::mpsc::Receiver<(PathBuf, Vec<Game>, Vec<(u32, String)>)>,
    >,
    /// Títulos normalizados uma vez por varredura, para a busca não refazer lowercase por frame.
    titulos_busca: Vec<String>,
    /// Índices que a busca atual deixou passar.
    filtrados: Vec<usize>,
    /// Applets conhecidos pela biblioteca, preparados fora da linha de UI para o boot da Z-Wheel.
    instalados: Vec<(u32, String)>,
    /// O filtro da busca.
    busca: String,
    /// Qual cartão o direcional está apontando, entre os que a busca deixou passar.
    selecionado: usize,
    /// O escolhido mudou e precisa entrar na área visível no próximo quadro.
    rolar: bool,
    /// Quantas colunas a grade desenhou da última vez. É o passo do direcional para cima e
    /// para baixo — e só a pintura sabe esse número.
    colunas: usize,
    /// As capas já subidas para a placa, por caminho do jogo. Sem isto cada quadro refaria a
    /// textura de todos os cartões à vista.
    capas: HashMap<PathBuf, egui::TextureHandle>,
    sessao: Option<Session>,
    /// O quadro do console como textura do egui, trocada a cada quadro novo.
    textura: Option<egui::TextureHandle>,
    /// O que está subido na textura: a série e as escritas do quadro, mais o filtro. A tela
    /// repinta mais vezes que o jogo desenha, e sem esta chave cada repintura refaria a
    /// conversão inteira.
    quadro: Option<(u64, u64, bool)>,
    /// Contador opcional sem tracing, leitura da GPU ou histórico adicional.
    fps: fps::FpsOverlay,
    /// Quem recebe o foco quando a seta sobe da primeira fila da grade.
    ///
    /// É um widget do egui, e por isso a barra de cima só é alcançável por foco do egui -- a
    /// grade tem cursor próprio e os dois não podem andar juntos. A `Id` sai da pintura da
    /// barra, porque é ela quem cria o botão.
    foco_da_barra: Option<egui::Id>,
    /// O quadro 2D em RGB565 para o caminho direto pela placa.
    ///
    /// O Android pode repintar a janela várias vezes sem o jogo tocar no framebuffer. Guardar a
    /// conversão evita recriar/copiar cerca de 600 KiB por repaint numa tela 640x480.
    ///
    /// **São dois caches, e nenhum sobra.** Este guarda os bytes e poupa a conversão na CPU; o
    /// `ultimo_quadro` do `Pintor` guarda o que já subiu e poupa a ida ao barramento. A mesma
    /// chave `(série, escritas)` governa os dois.
    quadro_565: Option<(u64, u64, std::sync::Arc<[u8]>)>,
    /// Os botões apertados agora, alimentados pela fila nativa.
    pad: Pad,
    /// Os controles desenhados na tela, e os botões que os dedos apertam neles.
    sobreposicao: toque::Sobreposicao,
    handset_keys: zeebx::handset::touch::TouchKeypad,
    handset_input: button_mapping::InputMap,
    zeebo_mode: bool,
    zeebo_controls: zeebo_controls::Controls,
    zeebo_input: zeebo_mapping::Map,
    zeebo_last_pad: Pad,
    game_options: game_options::GameOptions,
    keypad_settings: native_ui_model::KeypadSettings,
    keypad_layout: serde_json::Value,
    native_menu_shown: bool,
    handset_routes: handset::TouchRoutes,
    handset_touch: zeebx::handset::touch::Touchscreen,
    /// A peça que o editor está arrastando, e onde o dedo a pegou em relação ao centro dela.
    arrastando: Option<(zeebx::input::toque::Peca, egui::Vec2)>,
    /// A peça escolhida no editor, a que a barra de tamanho cresce e encolhe.
    escolhida: Option<zeebx::input::toque::Peca>,
    /// O "voltar" foi apertado com um jogo aberto: a pergunta está na tela.
    confirmando: bool,
    /// O jogo está parado por escolha, e não por falha.
    pausado: bool,
    audio_focus_paused: bool,
    /// Identidade BLAKE3 do arquivo que abriu a sessão, a mesma regra do armazenamento do núcleo.
    estado_id: Option<String>,
    /// De onde o jogo aberto veio: é o que o "Reiniciar" da caixa do voltar abre de novo.
    caminho_aberto: Option<std::path::PathBuf>,
    /// Os cinco slots persistentes do jogo atual.
    estados: [Option<estado::Slot>; estado::SLOTS],
    /// Miniaturas já convertidas/subidas ao egui, por slot.
    miniaturas_estado: HashMap<usize, egui::TextureHandle>,
    /// Slot ocupado que pediu uma segunda confirmação antes de ser substituído.
    estado_sobrescrever: Option<usize>,
    /// Resultado da última operação, mostrado no menu sem fechar a janela.
    estado_mensagem: Option<String>,
    /// Por que o último jogo não abriu, quando não abriu.
    erro: Option<String>,
    ultimo: Instant,
    /// Os botões do controle físico apertados agora, pelo nome do desktop. Ver
    /// [`entrada::Entrada::fisicos`].
    fisicos: std::collections::HashSet<&'static str>,
    /// O atalho do fast-forward, que se segura ou se alterna.
    avanco: zeebx::velocidade::Interruptor,
    /// Os pontos de retorno e o atalho do rewind.
    rewind: zeebx::velocidade::rewind::ControleDoRewind,
    /// O turbo do jogador 1, o único que o Android tem.
    turbo: zeebx::velocidade::turbo::TurboDaPorta,
    /// Se o fast-forward valeu na última volta, para o "▶▶" na tela.
    avancando: bool,
}

impl Emulador {
    fn release_zeebo_inputs(&mut self) {
        self.zeebo_controls.clear();self.zeebo_input.release_all();
        if self.zeebo_mode {self.sync_zeebo_inputs();}
    }
    fn sync_zeebo_inputs(&mut self) {
        let pad=zeebo_controls::merge(self.zeebo_input.pad(),self.zeebo_controls.pad());
        if let Some(session)=&mut self.sessao {
            session.set_port_pad(0,pad);
            for down in [false,true] {for (name,avk) in zeebo_controls::KEY_BUTTONS {
                let i=zeebo_controls::button_index(name).unwrap();
                if pad.is_down(i)==down && pad.is_down(i)!=self.zeebo_last_pad.is_down(i) {session.set_key(avk,down);}
            }}
        }
        self.zeebo_last_pad=pad;self.pad=pad;
    }
    fn sync_handset_keys(&mut self) {
        if self.zeebo_mode {self.sync_zeebo_inputs();return;}

        let Some(session) = &mut self.sessao else { return; };
        let held = self.handset_keys.held();
        // Soltar antes de apertar evita opostos transitórios ao inverter o eixo.
        for (key, down) in [false, true].into_iter().flat_map(|down| {
            zeebx::handset::input::Key::ALL.into_iter()
                .filter(move |key| (held & (1 << *key as u8) != 0) == down)
                .map(move |key| (key, down))
        }) {
            if let Err(error) = session.set_handset_key(key, down) {
                log::warn!("{error}");
                self.handset_keys.clear();
                self.handset_input.release_all();
                session.release_handset_keys();
                break;
            }
        }
    }

    fn novo(
        arquivo: PathBuf,
        padrao: PathBuf,
        externo: Option<PathBuf>,
        minha_pasta: PathBuf,
        app: AndroidApp,
    ) -> Self {
        let estados_raiz = minha_pasta
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| minha_pasta.clone());
        // Abre o perfil histórico para recuperar saves. Antes da sessão, a raiz persistente
        // muda para brew2droid/data; apenas o cache continua privado.
        let storage_base = estados_raiz.join(".config");
        let storage = StoragePaths::for_frontend(&storage_base, Some(&storage_base));
        if let Err(erro) = storage.create_dirs() {
            log::error!("nao criou o armazenamento persistente: {erro}");
        }
        let mut settings = native_ui_model::settings_for_android(&arquivo).unwrap_or_else(|error| {
            log::warn!("configuração Android: {error}");
            Settings::load_from(&arquivo)
        });
        settings.debug.overlay = false;
        let handset_games = minha_pasta.join("handset-games");
        if settings.roms_dir.is_none() && handset_games.is_dir() { settings.roms_dir = Some(handset_games); }
        // Sem pasta escolhida ainda: o padrão, se ele existir, senão o diretório do aplicativo,
        // que sempre existe e nunca pede permissão.
        if settings.roms_dir.is_none() {
            settings.roms_dir = match padrao.is_dir() {
                true => Some(padrao),
                false => externo,
            };
        }

        // O idioma guardado, ou o do aparelho. O catálogo só conhece os embutidos mais o que
        // houver em `lang/` dentro da pasta do aplicativo.
        let mut catalogo = Catalog::new(&[minha_pasta.with_file_name("lang")]);
        match &settings.language {
            Some(codigo) => {
                catalogo.select(codigo);
            }
            None => {
                catalogo.select_best(&idioma_do_aparelho(&app));
            }
        }

        let mut emulador = Self {
            onde: Onde::Biblioteca,
            aba: ajustes::Aba::Geral,
            dica: None,
            minha_pasta,
            estados_raiz,
            storage,
            app,
            gl: None,
            pintor: Default::default(),
            arquivo,
            settings,
            catalogo,
            jogos: Vec::new(),
            varredura: None,
            titulos_busca: Vec::new(),
            filtrados: Vec::new(),
            instalados: Vec::new(),
            busca: String::new(),
            selecionado: 0,
            rolar: false,
            colunas: 4,
            capas: HashMap::new(),
            sessao: None,
            textura: None,
            quadro: None,
            fps: Default::default(),
            foco_da_barra: None,
            quadro_565: None,
            pad: Pad::default(),
            sobreposicao: Default::default(),
            handset_keys: Default::default(),
            handset_input: Default::default(),
            zeebo_mode: false,
            zeebo_controls: Default::default(),
            zeebo_input: Default::default(),
            zeebo_last_pad: Default::default(),
            game_options: Default::default(),
            keypad_settings: Default::default(),
            keypad_layout: serde_json::Value::Null,
            native_menu_shown: false,
            handset_routes: Default::default(),
            handset_touch: Default::default(),
            arrastando: None,
            escolhida: None,
            confirmando: false,
            pausado: false,
            audio_focus_paused: false,
            estado_id: None,
            caminho_aberto: None,
            estados: std::array::from_fn(|_| None),
            miniaturas_estado: HashMap::new(),
            estado_sobrescrever: None,
            estado_mensagem: None,
            erro: None,
            ultimo: Instant::now(),
            fisicos: Default::default(),
            avanco: Default::default(),
            rewind: Default::default(),
            turbo: Default::default(),
            avancando: false,
        };
        // A biblioteca pertence à Activity Java; abrir um jogo não inicia outra varredura.
        emulador.handset_keys.set_enabled(emulador.settings.controles_na_tela.modo != ModoDosControlesNaTela::Nunca);
        emulador
    }

    /// Um texto da interface.
    fn tr<'a>(&'a self, chave: &'a str) -> &'a str {
        self.catalogo.get(chave)
    }

    /// Relê a pasta escolhida sem bloquear a linha que desenha a interface.
    fn recarrega(&mut self) {
        let pasta = self.roms();
        if pasta.as_os_str().is_empty() {
            self.varredura = None;
            self.aplica_jogos(pasta, Vec::new(), Vec::new());
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let pasta_da_thread = pasta.clone();
        match std::thread::Builder::new()
            .name("zeebx-library".to_string())
            .spawn(move || {
                let jogos = library::scan(&pasta_da_thread);
                let instalados = jogos
                    .iter()
                    .filter_map(|jogo| {
                        Some((jogo.clsid?, library::id_do_modulo(&jogo.path)?))
                    })
                    .collect();
                let _ = tx.send((pasta_da_thread, jogos, instalados));
            })
        {
            Ok(_) => self.varredura = Some(rx),
            Err(erro) => {
                log::error!("nao iniciou a varredura da biblioteca: {erro}");
                self.varredura = None;
            }
        }
    }

    /// Publica uma varredura concluída, ignorando resultado de uma pasta que já deixou de valer.
    fn conclui_recarrega(&mut self) {
        let resultado = match self.varredura.as_ref() {
            Some(rx) => match rx.try_recv() {
                Ok(resultado) => Some(Ok(resultado)),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err(())),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
            },
            None => None,
        };
        match resultado {
            Some(Ok((pasta, jogos, instalados))) => {
                self.varredura = None;
                if pasta == self.roms() {
                    self.aplica_jogos(pasta, jogos, instalados);
                }
            }
            Some(Err(())) => {
                self.varredura = None;
                log::error!("a varredura da biblioteca terminou sem resultado");
            }
            None => {}
        }
    }

    fn aplica_jogos(
        &mut self,
        pasta: PathBuf,
        jogos: Vec<Game>,
        instalados: Vec<(u32, String)>,
    ) {
        self.jogos = jogos;
        self.instalados = instalados;
        self.titulos_busca = self
            .jogos
            .iter()
            .map(|jogo| jogo.title.to_lowercase())
            .collect();
        self.atualiza_filtro();
        self.capas.clear();
        self.selecionado = 0;
        self.rolar = true;
        log::info!("{} jogos em {}", self.jogos.len(), pasta.display());
    }

    fn atualiza_filtro(&mut self) {
        let busca = self.busca.trim().to_lowercase();
        self.filtrados.clear();
        self.filtrados.extend(
            self.titulos_busca
                .iter()
                .enumerate()
                .filter(|(_, titulo)| busca.is_empty() || titulo.contains(&busca))
                .map(|(indice, _)| indice),
        );
        if self.filtrados.is_empty() {
            self.selecionado = 0;
        } else {
            self.selecionado = self.selecionado.min(self.filtrados.len() - 1);
        }
    }

    /// A pasta escolhida agora. Vazia quando nenhuma foi escolhida ainda.
    fn roms(&self) -> PathBuf {
        self.settings.roms_dir.clone().unwrap_or_default()
    }

    /// Grava as preferências. Toda mudança de ajuste passa por aqui.
    fn salva(&self) {
        if let Err(erro) = self.settings.save_to(&self.arquivo) {
            log::error!("não gravou os ajustes: {erro}");
        }
    }

    /// Abre um jogo, com os ajustes de gráficos e som que estão valendo.
    fn abre(&mut self, caminho: &std::path::Path) {
        self.erro = None;
        let content = match self.storage.content_id(caminho) {
            Ok(id) => id,
            Err(erro) => {
                let motivo = erro.to_string();
                log::error!("nao identificou o conteudo: {motivo}");
                self.erro = Some(
                    self.catalogo
                        .format("play.failed", &[("reason", &motivo)]),
                );
                return;
            }
        };
        let prepared=(||->Result<profile_runtime::Prepared,String>{
            let public=sistema::pasta_de_perfis(&self.app)?;
            let private=self.arquivo.parent().ok_or("Settings have no private directory")?;
            let effective=profile_store::effective(&public,private,&self.arquivo,caminho,&content)?;
            let mut prepared=profile_runtime::prepare(&effective["values"],&mut self.settings)?;
            prepared.zeebo=effective["button_type"]=="zeebo";Ok(prepared)
        })();
        let prepared=match prepared{Ok(value)=>value,Err(error)=>{self.erro=Some(error);return;}};
        self.release_zeebo_inputs();self.zeebo_mode=prepared.zeebo;
        self.zeebo_input=zeebo_mapping::Map::parse(&prepared.zeebo_bindings,prepared.volume_keys_mappable);
        self.zeebo_controls=Default::default();self.zeebo_last_pad=Default::default();self.pad=Default::default();self.sobreposicao.solta();
        self.handset_input=button_mapping::InputMap::parse(&serde_json::json!({"version":1,"bindings":&prepared.bindings}).to_string());
        self.handset_input.set_volume_keys_mappable(prepared.volume_keys_mappable);
        self.keypad_settings=native_ui_model::keypad_settings(&self.arquivo)
            .unwrap_or_else(|error|{log::warn!("ajustes globais do teclado: {error}");Default::default()});
        self.keypad_settings.transparency=prepared.transparency;
        self.keypad_layout=prepared.layout;
        self.game_options=prepared.options;
        self.native_menu_shown=false;
        let controls = match self.game_options.controls {
            game_options::OnScreenControls::On => true,
            game_options::OnScreenControls::Off => false,
            game_options::OnScreenControls::SystemDefault => self.settings.controles_na_tela.modo != ModoDosControlesNaTela::Nunca,
        };
        self.handset_keys.set_enabled(controls);self.zeebo_controls.set_enabled(controls);
        sistema::orientacao(&self.app, match self.game_options.orientation {
            game_options::ScreenOrientation::Auto => -1,
            game_options::ScreenOrientation::Portrait => 1,
            game_options::ScreenOrientation::Landscape => 0,
        });
        let estado_id = Some(content.as_str().to_string());
        let storage_base = self.estados_raiz.join(".config");
        let legacy = StoragePaths::for_frontend(&storage_base, Some(&storage_base));
        let prepared_storage = sistema::pasta_de_dados(&self.app).and_then(|data| {
            save_storage::prepare_game(&legacy, &data, caminho, &content)
                .map_err(|e| format!("Could not prepare game saves in {}: {e}", data.display()))
        });
        match prepared_storage {
            Ok(storage) => self.storage = storage,
            Err(error) => { self.erro = Some(error); return; }
        }
        zeebx::audio::soundfont::define_banco(self.settings.audio.soundfont.clone());
        zeebx::audio::soundfont::define_efeitos(self.settings.audio.midi_effects);
        let graficos = self.settings.graphics.clone();
        // O 3D na placa vale só se houver placa: antes da primeira janela não há contexto, e a
        // sessão aberta sem ele cai no rasterizador de software sozinha.
        let na_placa = graficos.gpu_rasterizer && self.gl.is_some();
        let geometry = match if prepared.zeebo && prepared.resolution.is_none() {Ok(zeebx::handset::types::DisplayGeometry{width:640,height:480,depth:16})}else{handset::profile_geometry(caminho, prepared.resolution)} {
            Ok(geometry) => geometry,
            Err(error) => { self.erro = Some(error); return; }
        };
        match Session::start_handset_with_storage(caminho, geometry, &self.storage,
            self.gl.clone().filter(|_| na_placa)) {
            Ok(mut sessao) => {
                log::info!("abriu {}", sessao.title());
                sessao.define_resolucao_interna(graficos.resolucao_interna as usize);
                sessao.define_proporcao(graficos.proporcao.aspecto(16.0 / 9.0));
                sessao.define_melhorias(
                    graficos.antialias as usize,
                    graficos.anisotropico as usize,
                );
                sessao.define_neblina(graficos.neblina);
                // Ligar o som aqui é seguro **porque o jogo ainda não começou**: o `start_with`
                // só prepara, e o `EVT_APP_START` sai na primeira volta do laço.
                let audio = self.settings.audio.clone();
                if let Some(erro) = sessao.set_audio(audio.enabled, audio.volume) {
                    log::error!("sem som: {erro}");
                }
                self.sessao = Some(sessao);
                self.fps.reset();
                self.caminho_aberto = Some(caminho.to_path_buf());
                self.estado_id = estado_id;
                self.estados = self
                    .estado_id
                    .as_deref()
                    .map(|id| estado::lista(&self.estados_raiz, id))
                    .unwrap_or_else(|| std::array::from_fn(|_| None));
                self.miniaturas_estado.clear();
                self.estado_sobrescrever = None;
                self.estado_mensagem = None;
                self.pausado = false;
                self.confirmando = false;
                self.onde = Onde::Jogo;
                self.ultimo = Instant::now();
            }
            Err(erro) => {
                log::error!("não abriu {}: {erro:?}", caminho.display());
                self.erro = Some(
                    self.catalogo
                        .format("play.failed", &[("reason", &erro.to_string())]),
                );
            }
        }
    }

    /// Fecha o jogo e volta para a biblioteca.
    fn fecha(&mut self) {
        self.release_zeebo_inputs();self.zeebo_mode=false;
        self.sessao = None;
        self.handset_keys.clear();
        self.handset_input.release_all();
        self.handset_touch = Default::default();
        self.fps.reset();
        self.caminho_aberto = None;
        self.textura = None;
        self.quadro = None;
        self.quadro_565 = None;
        self.estado_id = None;
        self.estados = std::array::from_fn(|_| None);
        self.miniaturas_estado.clear();
        self.estado_sobrescrever = None;
        self.estado_mensagem = None;
        self.confirmando = false;
        self.pausado = false;
        self.pad = Pad::default();
        // Os pontos de retorno são do jogo que fechou; o alternar do turbo e do fast-forward
        // não passam para o próximo.
        self.rewind = Default::default();
        self.avanco.desliga();
        self.turbo.desliga();
        self.onde = Onde::Biblioteca;
    }

    /// Somente saída definitiva: reset fecha a sessão sem terminar a Activity.
    fn finaliza_jogo(&mut self) {
        let error = self.erro.clone();
        self.fecha();
        native_ui::finish(&self.app, error.as_deref());
    }

    /// Recomeça o jogo aberto, do zero: fecha e abre o mesmo caminho, como o botão de reset de
    /// um console. O que o jogo gravou no disco fica, e os save states também.
    fn reinicia(&mut self) {
        let Some(caminho) = self.caminho_aberto.clone() else {
            return;
        };
        self.fecha();
        self.abre(&caminho);
        if self.sessao.is_none() { native_ui::finish(&self.app, self.erro.as_deref()); }
    }

    /// O botão "voltar" do Android.
    ///
    /// Ele nunca fecha o aplicativo por conta própria: com um jogo aberto ele pergunta; nas
    /// outras telas ele sobe um nível. Na biblioteca, sair é jogo do sistema.
    fn voltar(&mut self) {
        self.release_zeebo_inputs();
        self.handset_keys.clear();
        self.handset_input.release_all();
        self.handset_routes.clear();self.handset_touch.cancel();
        if let Some(session) = &mut self.sessao { session.release_handset_keys(); }
        match &self.onde {
            Onde::Jogo if self.confirmando => {self.confirmando=false;if self.native_menu_shown{sistema::fecha_menu_do_jogo(&self.app);self.native_menu_shown=false;}},
            Onde::Jogo => self.confirmando = true,
            // No seletor, subir uma pasta é o que o "voltar" quer dizer; na primeira, sair dele.
            Onde::Seletor(atual) => {
                self.onde = match atual.parent() {
                    Some(acima) if atual != &self.minha_pasta => Onde::Seletor(acima.to_path_buf()),
                    _ => Onde::Ajustes,
                }
            }
            Onde::SeletorDeBanco(atual) => {
                self.onde = match atual.parent() {
                    Some(acima) if atual != &self.minha_pasta => {
                        Onde::SeletorDeBanco(acima.to_path_buf())
                    }
                    _ => Onde::Ajustes,
                }
            }
            Onde::Ajustes => self.onde = Onde::Biblioteca,
            Onde::EditaToque => {
                self.arrastando = None;
                self.escolhida = None;
                self.onde = Onde::Ajustes;
            }
            Onde::Biblioteca => {}
        }
    }

    /// Qual das telas está no ar.
    fn desenha(&mut self, ctx: &egui::Context) {
        self.conclui_recarrega();
        match self.onde.clone() {
            Onde::Jogo => self.jogo(ctx),
            Onde::Ajustes => self.ajustes(ctx),
            Onde::EditaToque => self.edita_toque(ctx),
            Onde::Seletor(atual) => self.seletor(ctx, &atual),
            Onde::SeletorDeBanco(atual) => self.seletor_de_banco(ctx, &atual),
            Onde::Biblioteca => {},
        }
    }
}

/// Quantos pixels vale um ponto do egui.
///
/// Em retrato o teclado precisa conservar os pontos do telefone. A regra herdada de 600
/// pontos e a janela ausente no início fixavam o S25 em 1x, com alvos físicos minúsculos.
fn escala(app: &AndroidApp) -> f32 {
    let densidade = app
        .config()
        .density()
        .map(|dpi| dpi as f32 / 160.0)
        .unwrap_or(2.0);
    let largura = app
        .native_window()
        .map(|janela| janela.width() as f32);
    density::scale(densidade, largura)
}

/// O idioma do aparelho, como `pt-BR`. Vazio quando o Android não diz.
fn idioma_do_aparelho(app: &AndroidApp) -> String {
    let config = app.config();
    match (config.language(), config.country()) {
        (Some(lingua), Some(pais)) => format!("{lingua}-{pais}"),
        (Some(lingua), None) => lingua.to_string(),
        _ => String::new(),
    }
}

/// Uma imagem do núcleo virando textura do egui.
fn textura_de(ctx: &egui::Context, nome: &str, imagem: &zeebx::video::icon::Image) -> egui::TextureHandle {
    let cor = egui::ColorImage::from_rgba_unmultiplied(
        [imagem.width, imagem.height],
        &imagem.rgba,
    );
    // As capas são ícones de `.mif` ampliados: interpolar é o que evita o bloco gigante.
    ctx.load_texture(nome, cor, egui::TextureOptions::LINEAR)
}
