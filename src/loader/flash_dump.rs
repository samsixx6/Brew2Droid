//! Extração verificada de registros incorporados ao dump de um módulo móvel.
use sha2::{Digest, Sha256};

const SOURCE: &str = "7417d2d5a4d2a9268fdf94fa570523c68d12953bf421b3402142d107359852b8";
const IMAGE: &str = "523800e13ea3b2ee71a3885194c9f789acec5a529f7eaca3c457c6b5c55ed361";

fn matches(bytes: &[u8], expected: &str) -> bool {
    Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}"))
        .collect::<String>() == expected
}

pub(super) fn recover(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() != 168764 || !matches(bytes, SOURCE) {
        return None;
    }
    // sudoku4 inclui 122 registros de 528 bytes antes da cauda normal. Os
    // registros 52..56 são estranhos à imagem; nos demais, 464+48 bytes são dados.
    // Esta extração inteira iguala byte por byte o sudoku7 intacto do mesmo
    // pacote, inclusive a cauda. Não interpretar os bytes extras como instruções,
    // nem aplicar a receita a outros dumps sem comprovar seu conteúdo completo.
    let mut image = Vec::with_capacity(164764);
    for page in 0..122 {
        if (52..56).contains(&page) {
            continue;
        }
        let offset = page * 528;
        image.extend_from_slice(&bytes[offset..offset + 464]);
        image.extend_from_slice(&bytes[offset + 466..offset + 514]);
    }
    image.extend_from_slice(&bytes[122 * 528..]);
    (image.len() == 164764 && matches(&image, IMAGE)).then_some(image)
}
