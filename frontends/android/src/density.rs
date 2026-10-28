//! Pontos de telefone sem prender a escala ao tamanho fictício de uma janela ainda ausente.

pub fn scale(density: f32, window_width: Option<f32>) -> f32 {
    let width_limit = window_width
        .filter(|width| *width > 0.0)
        .map(|width| width / 360.0)
        .unwrap_or(density);
    density.min(width_limit).clamp(1.0, 4.0)
}
