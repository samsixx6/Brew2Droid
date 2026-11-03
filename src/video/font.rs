//! O texto que os jogos mandam desenhar.
//!
//! Até aqui o `IDISPLAY_DrawText` recebia a frase certa e não tinha com o que desenhá-la: o
//! texto ia para uma lista que só aparecia no relatório. Isso é o suficiente para saber que um
//! jogo *quer* escrever, e insuficiente para qualquer tela com menu.
//!
//! A fonte principal continua sendo a escolhida pelo jogo ou aparelho. Só um caractere sem
//! glifo nela procura os contornos das fontes redistribuíveis de apoio. Isso não muda a forma,
//! o avanço ou as métricas dos caracteres que a fonte principal já conhece.

use ab_glyph::{Font as _, FontVec, GlyphId, PxScale, ScaleFont as _};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::sync::OnceLock;

struct FallbackAsset {
    compressed: &'static [u8],
    raw_bytes: usize,
    raw_sha256: &'static str,
}

const FALLBACK_ASSETS: &[FallbackAsset] = &[
    FallbackAsset {
        compressed: include_bytes!("../../assets/fonts/neodgm-v1.601.ttf.gz"),
        raw_bytes: 651044,
        raw_sha256: "77305267996073aae07bad9313dad2e306a4128e55bfafbed4c41558fee57b4d",
    },
    FallbackAsset {
        compressed: include_bytes!("../../assets/fonts/misaki_gothic.ttf.gz"),
        raw_bytes: 1171204,
        raw_sha256: "0963e11e89d5fa409a7a048a52e8ea2d623d764a7998488f8a4291111611c4e1",
    },
];

fn fallback_fonts() -> &'static [FontVec] {
    static FONTS: OnceLock<Vec<FontVec>> = OnceLock::new();
    FONTS.get_or_init(|| {
        FALLBACK_ASSETS
            .iter()
            .filter_map(|asset| {
                // O limite é do TTF conhecido, não do cabeçalho gzip. Nenhum arquivo do host
                // ou rede participa da escolha; a primeira falta de glifo infla cada fonte uma vez.
                let mut bytes = Vec::with_capacity(asset.raw_bytes + 1);
                flate2::read::GzDecoder::new(asset.compressed)
                    .take((asset.raw_bytes + 1) as u64)
                    .read_to_end(&mut bytes)
                    .ok()?;
                if bytes.len() != asset.raw_bytes {
                    return None;
                }
                let digest = Sha256::digest(&bytes)
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>();
                if digest != asset.raw_sha256 {
                    return None;
                }
                FontVec::try_from_vec(bytes).ok()
            })
            .collect()
    })
}

/// Uma fonte principal carregada de um `.ttf`, com apoio apenas para glifos ausentes.
pub struct Font {
    face: FontVec,
    /// De onde ela veio, para o relatório.
    pub source: String,
}

/// Um glifo já rasterizado: onde pôr e com que cobertura.
pub struct Glyph {
    /// Deslocamento em relação ao ponto de desenho, já com o `bearing` aplicado.
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// Cobertura por pixel, de 0 a 255, na ordem de leitura.
    pub coverage: Vec<u8>,
}

impl Font {
    /// Lê um `.ttf`. `None` quando o arquivo não é uma fonte que saibamos abrir.
    pub fn load(bytes: Vec<u8>, source: String) -> Option<Self> {
        FontVec::try_from_vec(bytes)
            .ok()
            .map(|face| Self { face, source })
    }

    fn glyph_face(&self, character: char) -> (&FontVec, GlyphId) {
        let primary_id = self.face.glyph_id(character);
        if primary_id.0 != 0 {
            return (&self.face, primary_id);
        }
        for face in fallback_fonts() {
            let id = face.glyph_id(character);
            if id.0 != 0 {
                return (face, id);
            }
        }
        (&self.face, primary_id)
    }

    /// Altura da parte de cima da linha, no tamanho pedido. É o que o `GetFontMetrics` chama de
    /// `ascent`.
    pub fn ascent(&self, size: f32) -> u32 {
        self.face.as_scaled(PxScale::from(size)).ascent().ceil() as u32
    }

    /// Altura da parte de baixo, em valor positivo — o `descent` do BREW é positivo, e o da
    /// fonte é negativo.
    pub fn descent(&self, size: f32) -> u32 {
        (-self.face.as_scaled(PxScale::from(size)).descent())
            .ceil()
            .max(0.0) as u32
    }

    /// Quanto o texto ocupa na horizontal, incluindo o espaçamento entre letras.
    pub fn width(&self, text: &str, size: f32) -> u32 {
        self.width_with_cells(text, size, None)
    }

    /// Avanço lógico de fonte de aparelho, separado do contorno TTF substituto.
    pub(crate) fn width_with_cells(&self, text: &str, size: f32, cells: Option<(u32, u32)>) -> u32 {
        if let Some(cells) = cells {
            return text
                .chars()
                .fold(0u32, |sum, c| sum.saturating_add(cell_advance(c, cells)));
        }
        let mut largura = 0.0;
        let mut anterior: Option<(&FontVec, GlyphId)> = None;
        for c in text.chars() {
            let (face, id) = self.glyph_face(c);
            let escala = face.as_scaled(PxScale::from(size));
            if let Some((previous_face, previous_id)) = anterior {
                if std::ptr::eq(previous_face, face) {
                    largura += escala.kern(previous_id, id);
                }
            }
            largura += escala.h_advance(id);
            anterior = Some((face, id));
        }
        largura.ceil().max(0.0) as u32
    }

    /// Rasteriza `text` e devolve um glifo por caractere que tenha desenho.
    ///
    /// A origem é o **canto superior esquerdo** do texto, que é como o BREW posiciona — a
    /// linha de base entra aqui, somando o `ascent`, para quem chama não precisar saber disso.
    pub fn layout(&self, text: &str, size: f32) -> Vec<Glyph> {
        self.layout_with_cells(text, size, None)
    }

    /// A mesma grade usada na medida posiciona inclusive espaços sem contorno.
    pub(crate) fn layout_with_cells(
        &self,
        text: &str,
        size: f32,
        cells: Option<(u32, u32)>,
    ) -> Vec<Glyph> {
        let base = self.face.as_scaled(PxScale::from(size)).ascent();
        let mut saida = Vec::new();
        let mut caneta = 0.0;
        let mut anterior: Option<(&FontVec, GlyphId)> = None;
        for c in text.chars() {
            let (face, id) = self.glyph_face(c);
            let escala = face.as_scaled(PxScale::from(size));
            if let Some((previous_face, previous_id)) = anterior {
                if cells.is_none() && std::ptr::eq(previous_face, face) {
                    caneta += escala.kern(previous_id, id);
                }
            }
            let glifo =
                id.with_scale_and_position(PxScale::from(size), ab_glyph::point(caneta, base));
            caneta += cells
                .map(|cells| cell_advance(c, cells) as f32)
                .unwrap_or_else(|| escala.h_advance(id));
            anterior = Some((face, id));
            // Espaço não tem contorno: avança a caneta e não desenha nada.
            let Some(desenho) = face.outline_glyph(glifo) else {
                continue;
            };
            let caixa = desenho.px_bounds();
            let (largura, altura) = (caixa.width() as u32, caixa.height() as u32);
            if largura == 0 || altura == 0 {
                continue;
            }
            let mut coverage = vec![0u8; (largura * altura) as usize];
            desenho.draw(|x, y, v| {
                if x < largura && y < altura {
                    coverage[(y * largura + x) as usize] =
                        (v * 255.0).round().clamp(0.0, 255.0) as u8;
                }
            });
            saida.push(Glyph {
                x: caixa.min.x as i32,
                y: caixa.min.y as i32,
                width: largura,
                height: altura,
                coverage,
            });
        }
        saida
    }
}

fn cell_advance(c: char, (ascii, wide): (u32, u32)) -> u32 {
    if c.is_ascii() {
        ascii
    } else {
        wide.saturating_mul(c.len_utf16() as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fonte que a Z-Wheel empacota. O teste só roda quando ela está no cache — é material do
    /// usuário, não do repositório.
    fn tectoy() -> Option<Font> {
        let cache = dirs_cache()?;
        let ttf = std::fs::read(cache).ok()?;
        Font::load(ttf, "tectoy.ttf".into())
    }

    fn dirs_cache() -> Option<std::path::PathBuf> {
        let base = std::env::var_os("HOME")?;
        let raiz = std::path::Path::new(&base).join(".config/zeebx/cache");
        std::fs::read_dir(raiz)
            .ok()?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .find(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("Z-Wheel"))
            })
            .map(|p| p.join("mod/274755/tectoy.ttf"))
            .filter(|p| p.exists())
    }

    #[test]
    fn o_que_nao_e_fonte_e_recusado() {
        assert!(Font::load(b"nao sou uma fonte".to_vec(), "x".into()).is_none());
        assert!(Font::load(Vec::new(), "x".into()).is_none());
    }

    #[test]
    fn a_fonte_do_pacote_mede_e_desenha() {
        let Some(fonte) = tectoy() else {
            // Sem o jogo no cache não há o que testar: o teste existe para quem tem o material.
            return;
        };
        // Uma linha de 16 pixels tem altura de linha plausível, não zero e não absurda.
        let (ascent, descent) = (fonte.ascent(16.0), fonte.descent(16.0));
        assert!(
            (8..40).contains(&ascent),
            "ascent fora do razoável: {ascent}"
        );
        assert!(descent < 20, "descent fora do razoável: {descent}");

        // Texto mais longo ocupa mais espaço, e o espaço em branco conta.
        assert!(fonte.width("Jogar", 16.0) > fonte.width("J", 16.0));
        assert!(fonte.width("a a", 16.0) > fonte.width("aa", 16.0));

        // "Jogar" tem cinco letras e todas desenham alguma coisa.
        let glifos = fonte.layout("Jogar", 16.0);
        assert_eq!(glifos.len(), 5);
        assert!(glifos.iter().all(|g| g.coverage.iter().any(|&v| v > 0)));

        // O espaço não vira glifo, mas empurra o que vem depois.
        let com_espaco = fonte.layout("a b", 16.0);
        assert_eq!(com_espaco.len(), 2);
        assert!(com_espaco[1].x > com_espaco[0].x);
    }
}
