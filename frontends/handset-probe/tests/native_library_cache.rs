//! Fixtures de disco: o índice deve sobreviver sem acesso à coleção e reusar linhas reais.
#[path = "../../android/src/native_ui_model.rs"]
#[allow(dead_code)]
mod model;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
static NEXT: AtomicUsize = AtomicUsize::new(0);
static FIXTURE: std::sync::Mutex<()> = std::sync::Mutex::new(());

// cancel_scan é uma barreira global. Só o teste de cancelamento deve provocá-la
// enquanto uma fixture deste executável possui uma varredura em andamento.
struct Pasta(PathBuf, #[allow(dead_code)] std::sync::MutexGuard<'static, ()>);
impl Pasta {
    fn nova() -> Self {
        let serial = FIXTURE.lock().unwrap_or_else(|poisoned|poisoned.into_inner());
        let pasta = std::env::temp_dir().join(format!("brew-indice-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&pasta).unwrap(); Self(pasta, serial)
    }
    fn raiz(&self) -> PathBuf { self.0.join("Games") }
    fn arte(&self) -> PathBuf { self.0.join("art") }
    fn perfis(&self) -> PathBuf { self.0.join("profiles") }
    fn jogo(&self, nome: &str, empresa: &str) -> PathBuf {
        let pasta = self.raiz().join(nome).join("12345");
        std::fs::create_dir_all(&pasta).unwrap();
        let modulo = pasta.join("cryptic.mod");
        // O nome da empresa no MOD é uma isca: ele nunca pode fornecer o filtro.
        std::fs::write(&modulo, b"Electronic Arts in module only").unwrap();
        std::fs::write(pasta.parent().unwrap().join("12345.mif"), manifesto(empresa)).unwrap();
        modulo
    }
}
impl Drop for Pasta { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }

fn manifesto(texto: &str) -> Vec<u8> {
    // MIF com uma seção textual válida, sem applet nem imagem proprietária.
    let texto = texto.as_bytes(); let fim = 40 + texto.len();
    let mut bytes = vec![0u8; fim]; bytes[..2].copy_from_slice(&0x11u16.to_le_bytes());
    for (at, valor) in [(0x10,32u32),(0x14,1),(32,40),(36,fim as u32)] { bytes[at..at+4].copy_from_slice(&valor.to_le_bytes()); }
    bytes[40..].copy_from_slice(texto); bytes
}

#[test]
fn empresa_vem_do_mif_e_nunca_de_texto_do_modulo() {
    let pasta = Pasta::nova();
    pasta.jogo("A (176x220) 1.0", "Copyright 2008 Gameloft S.A. All rights reserved.");
    pasta.jogo("B (240x320) 2.0", "A vendor without a recognized name");
    pasta.jogo("C (240x320) 3.0", "Gameloft and Electronic Arts");
    let lista = model::scan_with_displays(&pasta.raiz(), &pasta.arte(), &pasta.perfis()).unwrap();
    assert_eq!(lista["games"][0]["company"], "Gameloft");
    assert_eq!(lista["games"][1]["company"], "Unknown");
    assert_eq!(lista["games"][2]["company"], "Unknown");
}

fn atualizar(pasta: &Pasta, forcar: bool) -> Value {
    model::library_refresh(&pasta.raiz(), &pasta.arte(), &pasta.perfis(), forcar).unwrap()
}

#[test]
fn indice_persistido_responde_sem_colecao_e_raizes_nao_se_misturam() {
    let pasta = Pasta::nova(); let modulo = pasta.jogo("Aninhado/Grupo/Jogo (176x220) 1.0", "Gameloft");
    let inicial = atualizar(&pasta, false); assert_eq!(inicial["processed"], 1); assert_eq!(inicial["games"][0]["title_source"], "folder");
    std::fs::rename(pasta.raiz(), pasta.0.join("desmontada")).unwrap();
    let cache = model::library_cached(&pasta.raiz(), &pasta.arte()).unwrap();
    assert_eq!(cache["cached"], true); assert_eq!(cache["games"], inicial["games"]);
    assert_eq!(PathBuf::from(cache["games"][0]["path"].as_str().unwrap()), modulo);
    let outra = pasta.0.join("Outra"); std::fs::create_dir(&outra).unwrap();
    assert_eq!(model::library_cached(&outra, &pasta.arte()).unwrap()["cached"], false);
    let falha = atualizar(&pasta, false); assert_eq!(falha["games"], inicial["games"]); assert!(falha["refresh_error"].is_string());
}

#[test]
fn refresh_reusa_linhas_e_detecta_adicao_remocao_e_mif_alterado() {
    let pasta = Pasta::nova(); let a = pasta.jogo("Grupo/Jogo A (176x220) 1.0", "Gameloft");
    pasta.jogo("Grupo/Jogo B (240x320) 2.0", "Electronic Arts");
    let inicial = atualizar(&pasta, false); assert_eq!(inicial["processed"], 2);
    let reuso = atualizar(&pasta, false); assert_eq!(reuso["processed"], 0); assert_eq!(reuso["reused"], 2); assert_eq!(reuso["games"], inicial["games"]);
    let dados = a.parent().unwrap().join("data/deep/asset.bin"); std::fs::create_dir_all(dados.parent().unwrap()).unwrap(); std::fs::write(&dados, [0u8; 19]).unwrap();
    let adicao = atualizar(&pasta, false); assert_eq!(adicao["processed"], 1); assert_eq!(adicao["reused"], 1);
    assert_eq!(adicao["games"][0]["folder_size_bytes"].as_u64().unwrap(), inicial["games"][0]["folder_size_bytes"].as_u64().unwrap() + 19);
    std::fs::remove_file(&dados).unwrap(); assert_eq!(atualizar(&pasta, false)["games"], inicial["games"]);
    std::fs::write(a.parent().unwrap().parent().unwrap().join("12345.mif"), manifesto("Glu Mobile")).unwrap();
    let mif = atualizar(&pasta, false); assert_eq!(mif["processed"], 1); assert_eq!(mif["reused"], 1); assert_eq!(mif["games"][0]["company"], "Glu");
    std::fs::remove_file(&a).unwrap(); assert_eq!(atualizar(&pasta, false)["games"].as_array().unwrap().len(), 1);
}

#[test]
fn mudanca_de_mesmo_tamanho_e_rescan_explicito_reprocessam() {
    let pasta = Pasta::nova(); let modulo = pasta.jogo("Jogo (176x220) 1.0", "Gameloft");
    atualizar(&pasta, false);
    std::thread::sleep(std::time::Duration::from_millis(20));
    let tamanho = std::fs::metadata(&modulo).unwrap().len(); std::fs::write(&modulo, vec![1u8; tamanho as usize]).unwrap();
    assert_eq!(atualizar(&pasta, false)["processed"], 1);
    assert_eq!(atualizar(&pasta, false)["processed"], 0);
    assert_eq!(atualizar(&pasta, true)["processed"], 1);
}

#[test]
fn indice_corrompido_ou_de_outro_esquema_e_reconstruido() {
    let pasta = Pasta::nova(); pasta.jogo("Jogo (240x320) 1.0", "Gameloft"); atualizar(&pasta, false);
    let indice = std::fs::read_dir(pasta.arte()).unwrap().map(|e|e.unwrap().path()).find(|p|p.extension().is_some_and(|e|e == "json")).unwrap();
    std::fs::write(&indice, b"truncado").unwrap(); assert_eq!(model::library_cached(&pasta.raiz(), &pasta.arte()).unwrap()["cached"], false);
    assert_eq!(atualizar(&pasta, false)["processed"], 1);
    let mut valor: Value = serde_json::from_slice(&std::fs::read(&indice).unwrap()).unwrap(); valor["schema"] = json!(999);
    std::fs::write(&indice, valor.to_string()).unwrap(); assert_eq!(atualizar(&pasta, false)["processed"], 1);
}

#[test]
fn profundidade_de_agrupamento_nao_esconde_jogos_e_manifestos_compartilhados() {
    let pasta = Pasta::nova(); let grupo = "um/dois/tres/quatro/cinco/seis/sete/oito/nove/dez";
    pasta.jogo(&format!("{grupo}/Jogo (176x220) 1.0"), "Gameloft");
    let modulos = pasta.raiz().join("Mod/39367"); let mifs = pasta.raiz().join("Mif/outro/nivel");
    std::fs::create_dir_all(&modulos).unwrap(); std::fs::create_dir_all(&mifs).unwrap();
    std::fs::write(modulos.join("a.mod"), [0u8;16]).unwrap(); std::fs::write(mifs.join("39367.mif"), manifesto("Electronic Arts")).unwrap();
    let lista = atualizar(&pasta, false); assert_eq!(lista["games"].as_array().unwrap().len(), 2);
    assert!(lista["games"].as_array().unwrap().iter().any(|r|r["company"] == "Electronic Arts"));
    let reuso = atualizar(&pasta, false); assert_eq!(reuso["processed"], 0); assert_eq!(reuso["reused"], 2);
}

#[test]
fn invalidacao_de_um_modulo_preserva_cache_e_nao_reprocessa_outros() {
    let pasta = Pasta::nova(); let modulo = pasta.jogo("A (176x220) 1.0", "Gameloft"); pasta.jogo("B (240x320) 1.0", "Electronic Arts");
    let inicial = atualizar(&pasta, false);
    model::library_invalidate(&pasta.raiz(), &pasta.arte(), &modulo).unwrap();
    assert_eq!(model::library_cached(&pasta.raiz(), &pasta.arte()).unwrap()["games"], inicial["games"]);
    let lista = atualizar(&pasta, false); assert_eq!(lista["processed"], 1); assert_eq!(lista["reused"], 1);
}

#[test]
fn cancelamento_aguarda_io_e_conserva_o_ultimo_indice_completo() {
    let pasta = Pasta::nova(); let modulo = pasta.jogo("Jogo (176x220) 1.0", "Gameloft"); let inicial = atualizar(&pasta, false);
    let dados = modulo.parent().unwrap().join("dados"); std::fs::create_dir_all(&dados).unwrap();
    for i in 0..3000 { std::fs::write(dados.join(format!("{i:04}.bin")), [0u8;4]).unwrap(); }
    let (raiz, arte, perfis) = (pasta.raiz(), pasta.arte(), pasta.perfis());
    let worker = std::thread::spawn(move || model::library_refresh(&raiz, &arte, &perfis, false));
    let prazo = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !model::scan_in_progress() && !worker.is_finished() && std::time::Instant::now() < prazo { std::thread::yield_now(); }
    assert!(model::scan_in_progress()); model::cancel_scan();
    assert!(worker.join().unwrap().is_err()); assert!(!model::scan_in_progress());
    assert_eq!(model::library_cached(&pasta.raiz(), &pasta.arte()).unwrap()["games"], inicial["games"]);
}

#[test]
fn modulo_avulso_na_raiz_nao_depende_de_outros_pacotes() {
    let pasta=Pasta::nova(); std::fs::create_dir_all(pasta.raiz()).unwrap();
    std::fs::write(pasta.raiz().join("avulso.mod"),[0u8;16]).unwrap(); atualizar(&pasta,false);
    pasta.jogo("Outro (176x220) 1.0","Gameloft");
    let lista=atualizar(&pasta,false); assert_eq!(lista["processed"],1); assert_eq!(lista["reused"],1);
}

#[test]
fn pacote_sem_modulo_nao_vira_jogo_no_indice() {
    let pasta=Pasta::nova(); std::fs::create_dir_all(pasta.raiz()).unwrap();
    std::fs::write(pasta.raiz().join("documentos.zip"),b"PK\x05\x06\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0").unwrap();
    assert!(atualizar(&pasta,false)["games"].as_array().unwrap().is_empty());
}

#[test]
fn override_de_resolucao_invalida_somente_a_identidade_afetada() {
    let pasta=Pasta::nova(); let modulo=pasta.jogo("A (176x220) 1.0","Gameloft"); let outro=pasta.jogo("B (240x320) 1.0","Electronic Arts");
    std::fs::write(outro,[1u8;16]).unwrap(); atualizar(&pasta,false);
    model::game_display(&modulo,&pasta.perfis(),"{\"width\":320,\"height\":240,\"depth\":16}").unwrap();
    let lista=atualizar(&pasta,false); assert_eq!(lista["processed"],1); assert_eq!(lista["reused"],1);
    assert_eq!(lista["games"][0]["resolution"]["geometry"]["width"],320);
    assert_eq!(atualizar(&pasta,false)["processed"],0);
}

#[cfg(windows)]
#[test]
fn arquivo_temporariamente_bloqueado_conserva_o_indice_anterior() {
    use std::os::windows::fs::OpenOptionsExt;
    let pasta=Pasta::nova(); let modulo=pasta.jogo("Jogo (176x220) 1.0","Gameloft"); let inicial=atualizar(&pasta,false);
    let mif=modulo.parent().unwrap().parent().unwrap().join("12345.mif");
    let _bloqueio=std::fs::OpenOptions::new().read(true).share_mode(0).open(mif).unwrap();
    let falha=atualizar(&pasta,true); assert_eq!(falha["games"],inicial["games"]); assert!(falha["refresh_error"].is_string());
    assert_eq!(model::library_cached(&pasta.raiz(),&pasta.arte()).unwrap()["games"],inicial["games"]);
}

#[test]
fn rescan_explicito_recalcula_hash_sem_apagar_override_manual() {
    let pasta=Pasta::nova(); let modulo=pasta.jogo("Jogo (176x220) 1.0","Gameloft");
    model::game_display(&modulo,&pasta.perfis(),"{\"width\":320,\"height\":240,\"depth\":16}").unwrap();
    let inicial=atualizar(&pasta,false);
    let indice=std::fs::read_dir(pasta.perfis().join("index")).unwrap().next().unwrap().unwrap().path();
    let mut cache:Value=serde_json::from_slice(&std::fs::read(&indice).unwrap()).unwrap(); cache["sha256"]=json!("f".repeat(64)); std::fs::write(indice,cache.to_string()).unwrap();
    assert_eq!(atualizar(&pasta,false)["processed"],0);
    let rescan=atualizar(&pasta,true);
    assert_eq!(rescan["games"][0]["resolution"],inicial["games"][0]["resolution"]);
    assert_eq!(rescan["games"][0]["resolution"]["source"],"manual");
}

#[test]
fn android_usa_gpu_por_padrao_e_preserva_cpu_salva() {
    let pasta=Pasta::nova(); let arquivo=pasta.0.join("settings.json");
    assert_eq!(model::snapshot(&arquivo,&pasta.raiz()).unwrap()["settings"]["graphics"]["gpu_rasterizer"],true);
    std::fs::write(&arquivo,b"{\"graphics\":{\"gpu_rasterizer\":false}}").unwrap();
    assert_eq!(model::snapshot(&arquivo,&pasta.raiz()).unwrap()["settings"]["graphics"]["gpu_rasterizer"],false);
}

#[test]
fn seiscentos_jogos_reusam_todas_as_linhas_sem_reprocessar_metadados() {
    let pasta=Pasta::nova();
    for i in 0..600 { pasta.jogo(&format!("Grupo {}/Jogo {i:03} (176x220) 1.0",i%7),"Gameloft"); }
    let inicio=std::time::Instant::now(); let inicial=atualizar(&pasta,false); let frio=inicio.elapsed();
    let inicio=std::time::Instant::now(); let quente=atualizar(&pasta,false); let refresh=inicio.elapsed();
    let inicio=std::time::Instant::now(); let cache=model::library_cached(&pasta.raiz(),&pasta.arte()).unwrap(); let leitura=inicio.elapsed();
    assert_eq!(inicial["processed"],600); assert_eq!(quente["processed"],0); assert_eq!(quente["reused"],600);
    assert_eq!(cache["games"],inicial["games"]); assert_eq!(cache["cached"],true);
    eprintln!("fixture 600 jogos: primeira varredura={} ms, refresh por stat={} ms, leitura do indice={} ms",frio.as_millis(),refresh.as_millis(),leitura.as_millis());
}

#[test]
fn raiz_que_e_um_pacote_atualiza_tamanho_dos_seus_assets() {
    let pasta=Pasta::nova(); let modulo=pasta.jogo("Pacote (176x220) 1.0","Gameloft");
    let raiz=modulo.parent().unwrap().parent().unwrap();
    let inicial=model::library_refresh(raiz,&pasta.arte(),&pasta.perfis(),false).unwrap();
    std::fs::write(raiz.join("asset.bin"),[0u8;41]).unwrap();
    let atual=model::library_refresh(raiz,&pasta.arte(),&pasta.perfis(),false).unwrap();
    assert_eq!(atual["processed"],1);
    assert_eq!(atual["games"][0]["folder_size_bytes"].as_u64().unwrap(),inicial["games"][0]["folder_size_bytes"].as_u64().unwrap()+41);
}

#[test]
fn mif_da_raiz_completa_pacote_aninhado_sem_mif_local() {
    let pasta=Pasta::nova(); let modulo=pasta.raiz().join("Grupo/Titulo/Mod/99999/game.mod");
    std::fs::create_dir_all(modulo.parent().unwrap()).unwrap(); std::fs::create_dir_all(pasta.raiz().join("Mif")).unwrap();
    std::fs::write(&modulo,[0u8;16]).unwrap(); std::fs::write(pasta.raiz().join("Mif/99999.mif"),manifesto("Gameloft")).unwrap();
    let lista=atualizar(&pasta,false); assert_eq!(lista["games"][0]["company"],"Gameloft");
}

#[test]
fn troca_com_mesmo_tamanho_e_mtime_nao_reusa_hash_nem_override_antigo() {
    let pasta=Pasta::nova(); let modulo=pasta.jogo("Jogo (176x220) 1.0","Gameloft");
    model::game_display(&modulo,&pasta.perfis(),"{\"width\":320,\"height\":240,\"depth\":16}").unwrap();
    let inicial=atualizar(&pasta,false); let antes=std::fs::metadata(&modulo).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    let troca=modulo.with_extension("tmp"); std::fs::write(&troca,vec![1u8;antes.len() as usize]).unwrap();
    std::fs::OpenOptions::new().write(true).open(&troca).unwrap().set_times(std::fs::FileTimes::new().set_modified(antes.modified().unwrap())).unwrap();
    std::fs::rename(troca,&modulo).unwrap();
    assert_eq!(std::fs::metadata(&modulo).unwrap().modified().unwrap(),antes.modified().unwrap());
    let atual=atualizar(&pasta,false); assert_eq!(atual["processed"],1);
    assert_ne!(atual["games"][0]["resolution"]["module_sha256"],inicial["games"][0]["resolution"]["module_sha256"]);
    assert_eq!(atual["games"][0]["resolution"]["source"],"name");
}

#[test]
fn override_do_payload_invalida_a_linha_sem_reprocessar_outro_jogo() {
    let pasta=Pasta::nova(); let modulo=pasta.jogo("A (176x220) 1.0","Gameloft"); let outro=pasta.jogo("B (240x320) 1.0","Gameloft");
    std::fs::write(outro,[1u8;16]).unwrap(); let payload=modulo.with_file_name("z.bar"); std::fs::write(&payload,b"payload sintetico").unwrap();
    atualizar(&pasta,false);
    let identidade=zeebx::handset::display::resolve(&payload,&pasta.perfis()).unwrap().module_sha256;
    let loader="94e7329d73313e7af041dd302b7b7edfdd6d5ea9b54c6881f6bbbeddbb1456e1";
    assert_eq!(zeebx::handset::display::override_identity(&modulo,&pasta.perfis(),loader).unwrap(),identidade);
    let indice=std::fs::read_dir(pasta.arte()).unwrap().map(|e|e.unwrap().path()).find(|p|p.extension().is_some_and(|e|e=="json")).unwrap();
    let mut cache:Value=serde_json::from_slice(&std::fs::read(&indice).unwrap()).unwrap();
    // O mesmo SHA conhecido pode ser fornecido ao resolvedor sem distribuir o MOD
    // original. A fixture persiste a linha resultante para testar a dependência do índice.
    let linha=&mut cache["entries"][modulo.to_string_lossy().as_ref()]["row"];
    linha["resolution"]=json!(zeebx::handset::display::resolve_hash(&modulo,&pasta.perfis(),loader).unwrap());
    linha["resolution_override_identity"]=json!(identidade);
    std::fs::write(&indice,cache.to_string()).unwrap(); assert_eq!(atualizar(&pasta,false)["processed"],0);
    let overrides=pasta.perfis().join("overrides"); std::fs::create_dir_all(&overrides).unwrap();
    std::fs::write(overrides.join(format!("{identidade}.json")),b"{\"width\":320,\"height\":240,\"depth\":16}").unwrap();
    assert_eq!(zeebx::handset::display::resolve_hash(&modulo,&pasta.perfis(),loader).unwrap().geometry.width,320);
    let atual=atualizar(&pasta,false); assert_eq!(atual["processed"],1); assert_eq!(atual["reused"],1);
}

struct PartidaAtiva;
impl Drop for PartidaAtiva { fn drop(&mut self) { model::end_gameplay(); } }

#[test]
fn partida_rejeita_chamador_atrasado_antes_de_tocar_a_colecao() {
    let pasta=Pasta::nova(); pasta.jogo("Jogo (176x220) 1.0","Gameloft");
    let arte=pasta.0.join("arte-ainda-ausente");
    let (pronto_tx,pronto_rx)=std::sync::mpsc::channel(); let (seguir_tx,seguir_rx)=std::sync::mpsc::channel();
    let (raiz,cache,perfis)=(pasta.raiz(),arte.clone(),pasta.perfis());
    let worker=std::thread::spawn(move || {
        // A Activity já passou sua verificação de geração, mas ainda não entrou no JNI.
        pronto_tx.send(()).unwrap(); seguir_rx.recv().unwrap();
        model::library_refresh(&raiz,&cache,&perfis,false)
    });
    pronto_rx.recv().unwrap(); model::begin_gameplay(); let partida=PartidaAtiva;
    let desmontada=pasta.0.join("colecao-indisponivel"); std::fs::rename(pasta.raiz(),&desmontada).unwrap();
    seguir_tx.send(()).unwrap(); let resultado=worker.join().unwrap();
    assert_eq!(resultado.unwrap_err(),"Library refresh blocked during gameplay");
    assert!(!arte.exists()); assert!(!model::scan_in_progress());
    std::fs::rename(desmontada,pasta.raiz()).unwrap(); drop(partida);
    assert_eq!(model::library_refresh(&pasta.raiz(),&arte,&pasta.perfis(),false).unwrap()["processed"],1);
}

#[test]
fn partida_cancela_varredura_ativa_e_so_reabre_io_depois_do_fim() {
    let pasta=Pasta::nova(); let modulo=pasta.jogo("Jogo (176x220) 1.0","Gameloft"); let inicial=atualizar(&pasta,false);
    let dados=modulo.parent().unwrap().join("dados"); std::fs::create_dir_all(&dados).unwrap();
    for i in 0..3000 { std::fs::write(dados.join(format!("{i:04}.bin")),[0u8;4]).unwrap(); }
    let (raiz,arte,perfis)=(pasta.raiz(),pasta.arte(),pasta.perfis());
    let worker=std::thread::spawn(move ||model::library_refresh(&raiz,&arte,&perfis,false));
    let prazo=std::time::Instant::now()+std::time::Duration::from_secs(5);
    while !model::scan_in_progress() && !worker.is_finished() && std::time::Instant::now()<prazo { std::thread::yield_now(); }
    assert!(model::scan_in_progress()); model::begin_gameplay(); let partida=PartidaAtiva;
    assert!(worker.join().unwrap().is_err()); assert!(!model::scan_in_progress());
    assert_eq!(model::library_cached(&pasta.raiz(),&pasta.arte()).unwrap()["games"],inicial["games"]);
    assert_eq!(model::library_refresh(&pasta.raiz(),&pasta.arte(),&pasta.perfis(),false).unwrap_err(),"Library refresh blocked during gameplay");
    drop(partida); assert_eq!(atualizar(&pasta,false)["processed"],1);
}
