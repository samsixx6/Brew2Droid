use ab_glyph::{Font as _, FontVec, PxScale, ScaleFont as _};
use std::io::Read;
use zeebx::video::font::Font;

fn ubuntu() -> Vec<u8> {
    egui::FontDefinitions::default().font_data["Ubuntu-Light"]
        .font
        .to_vec()
}

fn inflated(bytes: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_end(&mut output)
        .unwrap();
    output
}

fn shape(font: &Font, text: &str) -> Vec<(u32, u32, Vec<u8>)> {
    font.layout(text, 16.0)
        .into_iter()
        .map(|glyph| (glyph.width, glyph.height, glyph.coverage))
        .collect()
}

#[test]
fn missing_primary_hangul_kana_and_kanji_use_real_outlines_instead_of_notdef() {
    let font = Font::load(ubuntu(), "primary-ubuntu".into()).unwrap();
    let missing = shape(&font, "\u{10ffff}");
    for character in ["한", "あ", "漢"] {
        let actual = shape(&font, character);
        assert!(
            actual
                .iter()
                .any(|glyph| glyph.2.iter().any(|coverage| *coverage != 0)),
            "{character} precisa de contorno verdadeiro"
        );
        assert_ne!(
            actual, missing,
            "{character} não pode continuar sendo a caixa .notdef"
        );
        let bytes: &[u8] = if character == "한" {
            include_bytes!("../assets/fonts/neodgm-v1.601.ttf.gz")
        } else {
            include_bytes!("../assets/fonts/misaki_gothic.ttf.gz")
        };
        let fallback = FontVec::try_from_vec(inflated(bytes)).unwrap();
        let id = fallback.glyph_id(character.chars().next().unwrap());
        let advance = fallback.as_scaled(PxScale::from(16.0)).h_advance(id);
        assert_eq!(
            font.width(character, 16.0),
            advance.ceil() as u32,
            "a quebra de linha precisa medir o mesmo glifo que foi desenhado"
        );
    }
}

#[test]
fn an_existing_primary_glyph_and_its_metrics_always_win_over_fallback_fonts() {
    let cases = [
        (ubuntu(), 'A'),
        (
            inflated(include_bytes!("../assets/fonts/misaki_gothic.ttf.gz")),
            'A',
        ),
        (
            inflated(include_bytes!("../assets/fonts/neodgm-v1.601.ttf.gz")),
            '한',
        ),
    ];
    for (bytes, character) in cases {
        // O esperado vem do contorno/advance da fonte fornecida, sem passar pelo rasterizador testado.
        let face = FontVec::try_from_vec(bytes.clone()).unwrap();
        let scaled = face.as_scaled(PxScale::from(16.0));
        let id = face.glyph_id(character);
        assert_ne!(id.0, 0);
        let outline = face
            .outline_glyph(id.with_scale_and_position(
                PxScale::from(16.0),
                ab_glyph::point(0.0, scaled.ascent()),
            ))
            .unwrap();
        let bounds = outline.px_bounds();
        let (width, height) = (bounds.width() as u32, bounds.height() as u32);
        let mut coverage = vec![0; (width * height) as usize];
        outline.draw(|x, y, value| {
            coverage[(y * width + x) as usize] = (value * 255.0).round().clamp(0.0, 255.0) as u8;
        });
        let font = Font::load(bytes, "user-selected-primary".into()).unwrap();
        assert_eq!(
            shape(&font, &character.to_string()),
            vec![(width, height, coverage)]
        );
        assert_eq!(
            font.width(&character.to_string(), 16.0),
            scaled.h_advance(id).ceil().max(0.0) as u32
        );
        assert_eq!(font.ascent(16.0), scaled.ascent().ceil() as u32);
        assert_eq!(
            font.descent(16.0),
            (-scaled.descent()).ceil().max(0.0) as u32
        );
    }
}
