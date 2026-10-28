//! Toda chave de texto que a interface usa existe no catálogo.
//!
//! Uma chave que falta não quebra nada: o `Catalog` a devolve como ela mesma, e a tela mostra
//! `saves.confirm.yes` no lugar de "Excluir". O teste de `i18n.rs` confere que os idiomas têm as
//! mesmas chaves entre si; este confere o outro lado, que o código só pede chaves que existem.
//!
//! Esta distribuição traz a interface Android e o estado compartilhado em `src/ui/`.
//! Só esses diretórios são varridos: fora da interface, `.get("a.b")` pode consultar outra
//! coisa, e `i18n.rs` pede de propósito uma chave inexistente.

use std::path::{Path, PathBuf};

use zeebx::ui::i18n::Catalog;

/// `settings.discord.on`: minúsculas, dígitos e `_`, com ao menos um ponto. Nome de arquivo não.
fn tem_cara_de_chave(texto: &str) -> bool {
    const ARQUIVOS: [&str; 7] = [".h", ".png", ".log", ".qml", ".json", ".rs", ".svg"];
    texto.contains('.')
        && !texto.starts_with('.')
        && !texto.ends_with('.')
        && !texto.contains("..")
        && texto.starts_with(|c: char| c.is_ascii_lowercase())
        && texto
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.')
        && !ARQUIVOS.iter().any(|ext| texto.ends_with(ext))
}

fn arquivos(pasta: &Path, extensao: &str, achados: &mut Vec<PathBuf>) {
    let Ok(entradas) = std::fs::read_dir(pasta) else {
        return;
    };
    for entrada in entradas.flatten() {
        let caminho = entrada.path();
        if caminho.is_dir() {
            arquivos(&caminho, extensao, achados);
        } else if caminho.extension().is_some_and(|e| e == extensao) {
            achados.push(caminho);
        }
    }
}

/// As chaves pedidas pelo Rust ao catálogo, com o arquivo e a linha.
fn chaves_do_rust(pasta: &Path) -> Vec<(String, String)> {
    const CHAMADAS: [&str; 4] = [".get(\"", ".format(\"", ".tr(\"", "texto(\""];
    let mut rust = Vec::new();
    arquivos(pasta, "rs", &mut rust);
    let mut chaves = Vec::new();
    for arquivo in rust {
        if arquivo.ends_with("i18n.rs") {
            continue;
        }
        let texto = std::fs::read_to_string(&arquivo).unwrap();
        for (n, linha) in texto.lines().enumerate() {
            for chamada in CHAMADAS {
                for (i, _) in linha.match_indices(chamada) {
                    let resto = &linha[i + chamada.len()..];
                    let Some(fim) = resto.find('"') else { continue };
                    if tem_cara_de_chave(&resto[..fim]) {
                        chaves.push((
                            resto[..fim].to_string(),
                            format!("{}:{}", arquivo.display(), n + 1),
                        ));
                    }
                }
            }
        }
    }
    chaves
}

#[test]
fn toda_chave_usada_pela_interface_existe_no_catalogo() {
    let raiz = Path::new(env!("CARGO_MANIFEST_DIR"));
    let compartilhadas = chaves_do_rust(&raiz.join("src/ui"));
    let android = chaves_do_rust(&raiz.join("frontends/android/src"));
    // Cada origem precisa contribuir: sem isso, uma pasta ausente passaria sem ser verificada.
    assert!(
        !compartilhadas.is_empty(),
        "nenhuma chave da interface compartilhada encontrada"
    );
    assert!(
        !android.is_empty(),
        "nenhuma chave da interface Android encontrada"
    );
    let chaves: Vec<_> = compartilhadas.into_iter().chain(android).collect();

    let catalogo = Catalog::default();
    let faltando: Vec<String> = chaves
        .iter()
        .filter(|(chave, _)| catalogo.get(chave) == chave)
        .map(|(chave, onde)| format!("{onde}: {chave}"))
        .collect();
    assert!(
        faltando.is_empty(),
        "chaves fora do catálogo:\n{}",
        faltando.join("\n")
    );
}

#[test]
fn a_extracao_reconhece_chave_e_ignora_o_resto() {
    assert!(tem_cara_de_chave("saves.confirm.yes"));
    assert!(tem_cara_de_chave("settings.z_wheel_eol"));
    assert!(!tem_cara_de_chave("zeebx.log"));
    assert!(!tem_cara_de_chave("quadro.h"));
    assert!(!tem_cara_de_chave("Zeebx"));
    assert!(!tem_cara_de_chave("assets/controller.png"));
}
