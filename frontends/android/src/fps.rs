//! Indicador leve: usa o pintor existente e reaproveita texto/layout até a taxa mudar.

use std::sync::Arc;
use std::time::Duration;

use zeebx::handset::fps::{FpsCounter, FrameStamp};

#[derive(Default)]
pub(crate) struct FpsOverlay {
    counter: FpsCounter,
    pending: Option<FrameStamp>,
    galley: Option<Arc<egui::Galley>>,
}

impl FpsOverlay {
    pub(crate) fn prepare(&mut self, frame: Option<FrameStamp>) {
        self.pending = frame;
    }

    pub(crate) fn presented(&mut self, now: Duration, success: bool) {
        let frame = self.pending.take();
        self.counter
            .presented(now, if success { frame } else { None });
    }

    pub(crate) fn reset(&mut self) {
        self.counter.reset();
        self.pending = None;
        self.galley = None;
    }

    pub(crate) fn paint(&mut self, ctx: &egui::Context) {
        // O egui memoiza o layout e o invalida quando recria o atlas, mesmo sem mudar
        // texto/escala. Guardar os UVs por conta própria deixaria glifos corrompidos.
        self.galley = Some(ctx.fonts_mut(|fonts| {
            fonts.layout_no_wrap(
                self.counter.label().to_owned(),
                egui::FontId::monospace(14.0),
                egui::Color32::WHITE,
            )
        }));
        let galley = self.galley.as_ref().expect("FPS text layout");
        let size = galley.size() + egui::vec2(12.0, 8.0);
        let right_top = ctx.content_rect().right_top() + egui::vec2(-12.0, 12.0);
        let rect = egui::Rect::from_min_size(right_top - egui::vec2(size.x, 0.0), size);
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("light-fps"),
        ));
        painter.rect_filled(rect, 3.0, egui::Color32::from_black_alpha(180));
        painter.galley(
            rect.min + egui::vec2(6.0, 4.0),
            galley.clone(),
            egui::Color32::WHITE,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(writes: u64) -> FrameStamp {
        FrameStamp {
            cpu_series: 1,
            cpu_writes: writes,
            gpu_frames: 0,
        }
    }

    #[test]
    fn failed_swap_and_extra_ui_pass_do_not_count_a_presentation() {
        let mut overlay = FpsOverlay::default();
        overlay.prepare(Some(frame(1)));
        overlay.presented(Duration::ZERO, true);
        overlay.prepare(Some(frame(2)));
        overlay.presented(Duration::from_millis(100), false);
        overlay.presented(Duration::from_millis(200), true);
        overlay.prepare(Some(frame(2)));
        overlay.presented(Duration::from_millis(500), true);
        overlay.presented(Duration::from_secs(1), true);
        assert_eq!(overlay.counter.value(), Some(1));
        overlay.reset();
        assert_eq!(overlay.counter.value(), None);
    }

    #[test]
    fn unchanged_fps_reuses_its_text_layout_across_repaints() {
        let ctx = egui::Context::default();
        let mut overlay = FpsOverlay::default();
        let _ = ctx.run(Default::default(), |ctx| overlay.paint(ctx));
        let first = overlay.galley.as_ref().unwrap().clone();
        let _ = ctx.run(Default::default(), |ctx| overlay.paint(ctx));
        assert!(Arc::ptr_eq(&first, overlay.galley.as_ref().unwrap()));
        overlay.presented(Duration::ZERO, true);
        overlay.presented(Duration::from_secs(1), true);
        let _ = ctx.run(Default::default(), |ctx| overlay.paint(ctx));
        let zero = overlay.galley.as_ref().unwrap().clone();
        assert!(!Arc::ptr_eq(&first, &zero));
        assert_eq!(zero.job.text, "0 FPS");
        let _ = ctx.run(Default::default(), |ctx| overlay.paint(ctx));
        assert!(Arc::ptr_eq(&zero, overlay.galley.as_ref().unwrap()));
    }

    #[test]
    fn atlas_recreation_replaces_layout_even_when_rate_and_scale_are_unchanged() {
        let ctx = egui::Context::default();
        let mut overlay = FpsOverlay::default();
        let input = |side| egui::RawInput {
            max_texture_side: Some(side),
            ..Default::default()
        };
        let _ = ctx.run(input(2048), |ctx| overlay.paint(ctx));
        let first = overlay.galley.as_ref().unwrap().clone();
        let _ = ctx.run(input(1024), |ctx| overlay.paint(ctx));
        let replaced = overlay.galley.as_ref().unwrap();
        assert!(!Arc::ptr_eq(&first, replaced), "atlas novo exige UVs novos");
        assert_eq!(replaced.job.text, "FPS --");
    }
}
