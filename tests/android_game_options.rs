#[path = "../frontends/android/src/game_options.rs"]
mod game_options;

use game_options::{GameOptions, OnScreenControls, ScreenGravity, ScreenOrientation};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use zeebx::storage::ContentId;

struct Pasta(PathBuf);
impl Pasta {
    fn nova() -> Self {
        static PROXIMA: AtomicU64 = AtomicU64::new(0);
        let raiz = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests/pending-ui-20261022")
            .join(format!(
                "game-options-{}-{}",
                std::process::id(),
                PROXIMA.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&raiz).unwrap();
        Self(raiz)
    }
    fn modulo(&self, nome: &str, bytes: &[u8]) -> PathBuf {
        let caminho = self.0.join(nome);
        std::fs::create_dir_all(caminho.parent().unwrap()).unwrap();
        std::fs::write(&caminho, bytes).unwrap();
        caminho
    }
    fn opcoes(&self) -> PathBuf {
        self.0.join("game-options")
    }
}
impl Drop for Pasta {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn identidade(caminho: &Path) -> ContentId {
    ContentId::from_reader(std::fs::File::open(caminho).unwrap()).unwrap()
}

#[test]
fn leitura_ausente_usa_defaults_sem_escrever_configuracao_global() {
    let pasta = Pasta::nova();
    let modulo = pasta.modulo("jogo.mod", b"jogo-a");
    let global = pasta.modulo("zeebx.json", b"{\"graphics\":{\"smooth\":true}}");
    assert_eq!(
        game_options::view(&pasta.opcoes(), &modulo, "").unwrap(),
        json!({
            "orientation":"auto", "gravity":"center", "controls":"system_default"
        })
    );
    let opcoes = GameOptions::load(&pasta.opcoes(), &identidade(&modulo)).unwrap();
    assert_eq!(opcoes.orientation, ScreenOrientation::Auto);
    assert_eq!(opcoes.gravity, ScreenGravity::Center);
    assert_eq!(opcoes.controls, OnScreenControls::SystemDefault);
    assert!(!pasta.opcoes().exists());
    assert_eq!(
        std::fs::read(global).unwrap(),
        b"{\"graphics\":{\"smooth\":true}}"
    );
}

#[test]
fn patches_parciais_persistem_sem_perder_outros_campos() {
    let pasta = Pasta::nova();
    let modulo = pasta.modulo("jogo.mod", b"jogo-b");
    let raiz = pasta.opcoes();
    game_options::view(
        &raiz,
        &modulo,
        r#"{"orientation":"landscape","controls":"off"}"#,
    )
    .unwrap();
    assert_eq!(
        game_options::view(&raiz, &modulo, r#"{"gravity":"bottom"}"#).unwrap(),
        json!({
            "orientation":"landscape", "gravity":"bottom", "controls":"off"
        })
    );
    let opcoes = GameOptions::load(&raiz, &identidade(&modulo)).unwrap();
    assert_eq!(opcoes.orientation, ScreenOrientation::Landscape);
    assert_eq!(opcoes.gravity, ScreenGravity::Bottom);
    assert_eq!(opcoes.controls, OnScreenControls::Off);
    assert_eq!(std::fs::read_dir(&raiz).unwrap().count(), 1);
    assert_eq!(
        game_options::view(&raiz, &modulo, "").unwrap(),
        opcoes.as_json()
    );
}

#[test]
fn todas_as_escolhas_voltam_do_arquivo() {
    let pasta = Pasta::nova();
    let modulo = pasta.modulo("jogo.mod", b"jogo-c");
    let raiz = pasta.opcoes();
    for (orientacao, gravidade, controles) in [
        ("portrait", "top", "on"),
        ("landscape", "bottom", "off"),
        ("auto", "center", "system_default"),
    ] {
        let esperado = json!({"orientation":orientacao,"gravity":gravidade,"controls":controles});
        assert_eq!(
            game_options::view(&raiz, &modulo, &esperado.to_string()).unwrap(),
            esperado
        );
        assert_eq!(
            GameOptions::load(&raiz, &identidade(&modulo))
                .unwrap()
                .as_json(),
            esperado
        );
    }
}

#[test]
fn patch_invalido_nao_altera_opcoes_salvas() {
    let pasta = Pasta::nova();
    let modulo = pasta.modulo("jogo.mod", b"jogo-d");
    let raiz = pasta.opcoes();
    game_options::view(&raiz, &modulo, r#"{"orientation":"portrait"}"#).unwrap();
    let caminho = raiz.join(format!("{}.json", identidade(&modulo).as_str()));
    let original = std::fs::read(&caminho).unwrap();
    for invalido in [
        "null",
        "[]",
        "false",
        "{",
        r#"{"orientation":"sensor"}"#,
        r#"{"gravity":"left"}"#,
        r#"{"controls":true}"#,
        r#"{"controls":null}"#,
        r#"{"resolution":{"width":1}}"#,
        r#"{"orientation":"landscape","gravity":23}"#,
    ] {
        assert!(
            game_options::view(&raiz, &modulo, invalido).is_err(),
            "{invalido}"
        );
        assert_eq!(std::fs::read(&caminho).unwrap(), original);
    }
}

#[test]
fn conteudos_distintos_com_mesmo_nome_tem_opcoes_isoladas() {
    let pasta = Pasta::nova();
    let primeiro = pasta.modulo("a/jogo.mod", b"primeira-versao");
    let segundo = pasta.modulo("b/jogo.mod", b"segunda-versao");
    let raiz = pasta.opcoes();
    game_options::view(&raiz, &primeiro, r#"{"controls":"on"}"#).unwrap();
    assert_eq!(
        game_options::view(&raiz, &segundo, "").unwrap()["controls"],
        "system_default"
    );
    game_options::view(&raiz, &segundo, r#"{"controls":"off"}"#).unwrap();
    assert_eq!(
        game_options::view(&raiz, &primeiro, "").unwrap()["controls"],
        "on"
    );
}

#[test]
fn renomear_pasta_e_modulo_preserva_a_identidade_do_save() {
    let pasta = Pasta::nova();
    let antes = pasta.modulo("antes/jogo.mod", b"conteudo-imutavel");
    let content = identidade(&antes);
    game_options::view(&pasta.opcoes(), &antes, r#"{"gravity":"top"}"#).unwrap();
    std::fs::rename(pasta.0.join("antes"), pasta.0.join("depois")).unwrap();
    let depois = pasta.0.join("depois/renomeado.mod");
    std::fs::rename(pasta.0.join("depois/jogo.mod"), &depois).unwrap();
    assert_eq!(identidade(&depois), content);
    assert_eq!(
        game_options::view(&pasta.opcoes(), &depois, "").unwrap()["gravity"],
        "top"
    );
    assert_eq!(
        GameOptions::load(&pasta.opcoes(), &content)
            .unwrap()
            .gravity,
        ScreenGravity::Top
    );
}

#[test]
fn arquivo_corrompido_informa_erro_e_preserva_bytes() {
    let pasta = Pasta::nova();
    let modulo = pasta.modulo("jogo.mod", b"jogo-e");
    let raiz = pasta.opcoes();
    std::fs::create_dir_all(&raiz).unwrap();
    let content = identidade(&modulo);
    let caminho = raiz.join(format!("{}.json", content.as_str()));
    for quebrado in [
        "{",
        r#"{"controls":"sometimes"}"#,
        r#"{"orientation":"portrait","unknown":true}"#,
    ] {
        std::fs::write(&caminho, quebrado).unwrap();
        assert!(
            GameOptions::load(&raiz, &content)
                .unwrap_err()
                .contains("Invalid game options")
        );
        assert!(game_options::view(&raiz, &modulo, r#"{"gravity":"top"}"#).is_err());
        assert_eq!(std::fs::read_to_string(&caminho).unwrap(), quebrado);
    }
}

#[test]
fn falha_de_leitura_ou_escrita_nao_passa_como_default() {
    let pasta = Pasta::nova();
    let modulo = pasta.modulo("jogo.mod", b"jogo-f");
    let raiz = pasta.modulo("game-options", b"arquivo-no-lugar-da-pasta");
    assert!(GameOptions::load(&raiz, &identidade(&modulo)).is_err());
    assert!(game_options::view(&raiz, &modulo, r#"{"gravity":"bottom"}"#).is_err());
    assert!(game_options::view(&pasta.0.join("outras"), &pasta.0.join("ausente.mod"), "").is_err());
}
