//! CStaticWidget público e seu IValueModel, sem confundir texto com ITextModel.
use super::*;
pub(super) const CLASS_ID: u32 = 0x0105_51e0;
const BAD: u32 = 14;
const MODEL: u32 = 0;
const FONT: u32 = 4;
const MAP: u32 = 8;
const WIDTH: u32 = 12;
const HEIGHT: u32 = 16;
const PARENT: u32 = 20;
const FLAGS: u32 = 24;
const HANDLER: u32 = 28;
const CONTEXT: u32 = 32;
const FREE: u32 = 36;
const FOCUSED: u32 = 40;
const SELECTED: u32 = 44;
// Os slots reservados preservam o estado base dos saves anteriores (160 bytes).
const PAD_L: u32 = 48;
const PAD_R: u32 = 100;
const PAD_T: u32 = 104;
const PAD_B: u32 = 108;
// Fundos públicos usam RGBAVAL, com alpha no byte baixo.
const BG_A: u32 = 52;
const BG_I: u32 = 56;
const BG_SA: u32 = 60;
const BG_SI: u32 = 64;
const FG_A: u32 = 68;
const FG_I: u32 = 72;
const FG_SA: u32 = 76;
const FG_SI: u32 = 80;
const GAP: u32 = 84;
const HINT_W: u32 = 88;
const HINT_ROWS: u32 = 92;
const ELLIPSIS: u32 = 96;
const LISTENER: u32 = 112;
const STATE_BYTES: u32 = 160;
const DRAW_FACET: u32 = 136;
const DRAW_FN: u32 = 140;
const DRAW_CONTEXT: u32 = 144;
const DRAW_FREE: u32 = 148;
const LAYOUT: u32 = 152;
const GRADIENT: u32 = 156;
const WRAP: u32 = 0x0010_0000;
const NO_SHORT: u32 = 0x0020_0000;
const NO_BREAK: u32 = 0x0040_0000;
impl<C: CpuBackend> Machine<C> {
    fn static_content(&self, state: u32, x: i32, y: i32) -> Result<(i32, i32, u32, u32), CpuError> {
        let width = self.cpu.read_u32(state + WIDTH)?;
        let height = self.cpu.read_u32(state + HEIGHT)?;
        let left = self.cpu.read_u32(state + PAD_L)?.min(width);
        let right = self.cpu.read_u32(state + PAD_R)?.min(width - left);
        let top = self.cpu.read_u32(state + PAD_T)?.min(height);
        let bottom = self.cpu.read_u32(state + PAD_B)?.min(height - top);
        Ok((
            x.saturating_add(left as i32),
            y.saturating_add(top as i32),
            width - left - right,
            height - top - bottom,
        ))
    }
    fn static_content_clip(outer: Rect, x: i32, y: i32, w: u32, h: u32) -> Option<Rect> {
        let left = i32::from(outer.x).max(x);
        let top = i32::from(outer.y).max(y);
        let right = (i32::from(outer.x) + i32::from(outer.width)).min(x.saturating_add(w as i32));
        let bottom = (i32::from(outer.y) + i32::from(outer.height)).min(y.saturating_add(h as i32));
        (right > left && bottom > top).then_some(Rect {
            x: left as i16,
            y: top as i16,
            width: (right - left).min(i16::MAX as i32) as i16,
            height: (bottom - top).min(i16::MAX as i32) as i16,
        })
    }
    fn static_padded_preferred(&self, state: u32, w: u32, h: u32) -> Result<(u32, u32), CpuError> {
        Ok((
            w.saturating_add(self.cpu.read_u32(state + PAD_L)?)
                .saturating_add(self.cpu.read_u32(state + PAD_R)?)
                .min(i32::MAX as u32),
            h.saturating_add(self.cpu.read_u32(state + PAD_T)?)
                .saturating_add(self.cpu.read_u32(state + PAD_B)?)
                .min(i32::MAX as u32),
        ))
    }
    /// Saves anteriores têm o mesmo estado base, com este slot reservado zero.
    /// A extensão só existe quando o consumidor configura um gradiente.
    pub(super) fn modern_widget_gradient(
        &self,
        slot: u32,
        active: bool,
        selected: bool,
    ) -> Result<(u32, u32), CpuError> {
        let extension = self.cpu.read_u32(slot)?;
        if extension == 0 || self.heap.size_of(extension).is_none_or(|n| n < 20) {
            return Ok((4, 0));
        }
        let color = match (active, selected) {
            (true, false) => 4,
            (false, false) => 8,
            (true, true) => 12,
            (false, true) => 16,
        };
        Ok((
            self.cpu.read_u32(extension)?,
            self.cpu.read_u32(extension + color)?,
        ))
    }
    pub(super) fn modern_widget_gradient_property(
        &mut self,
        slot: u32,
        set: bool,
        property: u32,
        data: u32,
    ) -> Result<Option<u32>, CpuError> {
        let fields: &[u32] = match property {
            0x210 => &[0],
            0x238 if set => &[4, 8, 12, 16],
            0x239 if set => &[12, 16],
            0x240 => &[4],
            0x241 => &[8],
            0x242 => &[12],
            0x243 => &[16],
            _ => return Ok(None),
        };
        let mut extension = self.cpu.read_u32(slot)?;
        if !set {
            let value = if extension == 0 {
                if property == 0x210 { 4 } else { 0 }
            } else if self.heap.size_of(extension).is_some_and(|n| n >= 20) {
                self.cpu.read_u32(extension + fields[0])?
            } else {
                return Ok(Some(FALSE));
            };
            return Ok(Some(if property == 0x210 {
                self.modern_font_write(data, &(value as u16).to_le_bytes()) as u32
            } else {
                self.modern_font_write(data, &value.to_le_bytes()) as u32
            }));
        }
        if property == 0x210 && data > 4 {
            return Ok(Some(FALSE));
        }
        if extension == 0 {
            extension = self.malloc(20)?;
            if extension == 0 {
                return Ok(Some(FALSE));
            }
            self.cpu.write_u32(extension, 4)?;
            self.cpu.write_u32(slot, extension)?;
        } else if self.heap.size_of(extension).is_none_or(|n| n < 20) {
            return Ok(Some(FALSE));
        }
        for field in fields {
            self.cpu.write_u32(extension + *field, data)?;
        }
        Ok(Some(TRUE))
    }
    pub(super) fn modern_widget_gradient_release(&mut self, slot: u32) -> Result<(), CpuError> {
        let extension = self.cpu.read_u32(slot)?;
        self.cpu.write_u32(slot, 0)?;
        if extension != 0 && self.heap.size_of(extension).is_some_and(|n| n >= 20) {
            self.heap.free(extension);
        }
        Ok(())
    }
    /// O recorte não muda a origem da progressão: redesenhar só uma faixa deve
    /// produzir os mesmos pixels que desenhar o fundo inteiro.
    pub(super) fn modern_widget_background(
        &mut self,
        target: u32,
        bounds: Rect,
        area: Rect,
        start: u32,
        end: u32,
        style: u32,
    ) -> Result<(), CpuError> {
        let Some(surface) = self.bitmaps.get(&target) else {
            return Ok(());
        };
        let mut l = i32::from(area.x).max(i32::from(bounds.x)).max(0);
        let mut t = i32::from(area.y).max(i32::from(bounds.y)).max(0);
        let mut r = (i32::from(area.x) + i32::from(area.width))
            .min(i32::from(bounds.x) + i32::from(bounds.width))
            .min(surface.width() as i32);
        let mut b = (i32::from(area.y) + i32::from(area.height))
            .min(i32::from(bounds.y) + i32::from(bounds.height))
            .min(surface.height() as i32);
        if let Some(clip) = self.clip {
            l = l.max(i32::from(clip.x));
            t = t.max(i32::from(clip.y));
            r = r.min(i32::from(clip.x) + i32::from(clip.width));
            b = b.min(i32::from(clip.y) + i32::from(clip.height));
        }
        if r <= l || b <= t {
            return Ok(());
        }
        let rect = Rect {
            x: l as i16,
            y: t as i16,
            width: (r - l) as i16,
            height: (b - t) as i16,
        };
        if style >= 4 || start == end {
            return self.modern_widget_fill(target, rect, start);
        }
        let horizontal = style & 1 != 0;
        let origin = i32::from(if horizontal { bounds.x } else { bounds.y });
        let length = i32::from(if horizontal {
            bounds.width
        } else {
            bounds.height
        });
        let denominator = (length - 1).max(1) as u32;
        let (first, last) = if horizontal { (l, r) } else { (t, b) };
        for at in first..last {
            let offset = (at - origin) as u32;
            let numerator = if style >= 2 {
                offset.min((length - 1) as u32 - offset).saturating_mul(2)
            } else {
                offset
            };
            let mut color = 0;
            for shift in [0, 8, 16, 24] {
                let a = (start >> shift) & 255;
                let z = (end >> shift) & 255;
                color |= ((a * (denominator - numerator) + z * numerator + denominator / 2)
                    / denominator)
                    << shift;
            }
            let strip = if horizontal {
                Rect {
                    x: at as i16,
                    width: 1,
                    ..rect
                }
            } else {
                Rect {
                    y: at as i16,
                    height: 1,
                    ..rect
                }
            };
            self.modern_widget_fill(target, strip, color)?;
        }
        Ok(())
    }
    /// AEEWidgetProperties distingue FG RGBVAL de BG RGBAVAL. Alpha zero
    /// preserva o destino; ffffffff é branco opaco, não RGB_NONE.
    pub(super) fn modern_widget_fill(
        &mut self,
        target: u32,
        rect: Rect,
        color: u32,
    ) -> Result<(), CpuError> {
        let alpha = color & 255;
        if alpha == 0 || rect.width <= 0 || rect.height <= 0 {
            return Ok(());
        }
        let Some(surface) = self.bitmaps.get_mut(&target) else {
            return Ok(());
        };
        if alpha == 255 {
            surface.fill_rect_native(rect, Rgb::from_rgbval(color).to_rgb565());
            return Ok(());
        }
        let fg = Rgb::from_rgbval(color);
        for y in i32::from(rect.y).max(0)
            ..(i32::from(rect.y) + i32::from(rect.height)).min(surface.height() as i32)
        {
            for x in i32::from(rect.x).max(0)
                ..(i32::from(rect.x) + i32::from(rect.width)).min(surface.width() as i32)
            {
                let bg = Rgb::from_rgb565(surface.get_pixel(x, y));
                let mix = |a: u8, b: u8| {
                    ((u32::from(a) * alpha + u32::from(b) * (255 - alpha) + 127) / 255) as u8
                };
                surface.set_pixel(
                    x,
                    y,
                    Rgb {
                        r: mix(fg.r, bg.r),
                        g: mix(fg.g, bg.g),
                        b: mix(fg.b, bg.b),
                    },
                );
            }
        }
        Ok(())
    }

    fn static_state(&self, widget: u32) -> Option<u32> {
        if self.objects.kind_of(widget) != Some(Interface::StaticWidget) {
            return None;
        }
        let state = self.cpu.read_u32(widget + 4).ok()?;
        if self.heap.size_of(state).is_none_or(|n| n < STATE_BYTES) {
            return None;
        }
        Some(state)
    }
    fn static_invalidate(&mut self, widget: u32, state: u32) -> Result<(), CpuError> {
        let parent = self.cpu.read_u32(state + PARENT)?;
        if parent != 0 {
            self.modern_ui_invoke(parent, 3, [widget, 0, 3], &[])?;
        }
        Ok(())
    }
    fn static_unlisten(&mut self, state: u32) -> Result<(), CpuError> {
        let listener = state + LISTENER;
        let cancel = self.cpu.read_u32(listener + 16)?;
        if cancel != 0 {
            self.modern_ui_call(cancel, [listener, 0, 0, 0], &[])?;
        }
        Ok(())
    }
    fn static_listen(&mut self, widget: u32, state: u32, model: u32) -> Result<(), CpuError> {
        let listener = state + LISTENER;
        self.cpu.write_mem(listener, &[0; 24])?;
        self.cpu
            .write_u32(listener + 8, aee::encode(Interface::StaticWidget, 15))?;
        self.cpu.write_u32(listener + 12, widget)?;
        self.modern_ui_invoke(model, 3, [listener, 0, 0], &[])?;
        Ok(())
    }
    pub(super) fn create_static_widget(&mut self, out: u32) -> Result<u32, CpuError> {
        if !self.modern_font_write(out, &0u32.to_le_bytes()) {
            return Ok(BAD);
        }
        let widget = self.new_object(Interface::StaticWidget)?;
        if widget == 0 {
            return Ok(2);
        }
        let state = self.malloc(STATE_BYTES)?;
        if state == 0 {
            self.objects.release(widget);
            return Ok(2);
        }
        self.cpu.write_mem(state, &[0; STATE_BYTES as usize])?;
        self.cpu.write_mem(widget + 4, &[0; 60])?;
        self.cpu.write_u32(widget + 4, state)?;
        let model_status = self.create_value_model(state + MODEL)?;
        if model_status != SUCCESS {
            self.heap.free(state);
            self.objects.release(widget);
            return Ok(model_status);
        }
        let font_status = self.create_modern_hfont(state + FONT, 0, 12, 0, 0, false)?;
        if font_status != SUCCESS {
            let model = self.cpu.read_u32(state + MODEL)?;
            self.objects.release(model);
            self.heap.free(state);
            self.objects.release(widget);
            return Ok(font_status);
        }
        let draw_facet = self.new_object(Interface::StaticDrawHandler)?;
        if draw_facet == 0 {
            let font = self.cpu.read_u32(state + FONT)?;
            self.modern_font_base(font, 1)?;
            let model = self.cpu.read_u32(state + MODEL)?;
            self.objects.release(model);
            self.heap.free(state);
            self.objects.release(widget);
            return Ok(2);
        }
        self.cpu.write_mem(draw_facet + 4, &[0; 60])?;
        self.cpu.write_u32(draw_facet + 4, widget)?;
        for (field, value) in [
            (DRAW_FACET, draw_facet),
            (DRAW_FN, aee::encode(Interface::StaticWidget, 16)),
            (DRAW_CONTEXT, widget),
            (WIDTH, 30),
            (HEIGHT, 12),
            (FLAGS, NO_SHORT | 0x210),
            (HANDLER, aee::encode(Interface::StaticWidget, 14)),
            (CONTEXT, widget),
            (ELLIPSIS, 0x2026),
            (BG_A, 0),
            (BG_I, 0),
            (BG_SA, 0),
            (BG_SI, 0),
        ] {
            self.cpu.write_u32(state + field, value)?;
        }
        let model = self.cpu.read_u32(state + MODEL)?;
        self.static_listen(widget, state, model)?;
        self.cpu.write_u32(out, widget)?;
        Ok(SUCCESS)
    }
    fn static_text(&mut self, state: u32) -> Result<Vec<u16>, CpuError> {
        let model = self.cpu.read_u32(state + MODEL)?;
        self.modern_font_base(model, 0)?;
        let result = self.modern_ui_scratch(16, |m, out| {
            let query = m.modern_ui_invoke(model, 2, [0x01015946, out, 0], &[])?;
            let text_model = m.cpu.read_u32(out)?;
            let (value, count) = if query == 0 && text_model != 0 {
                let call = m.modern_ui_invoke(text_model, 7, [out, 0, 0], &[]);
                let pair = (m.cpu.read_u32(out)?, m.cpu.read_u32(out + 4)?);
                m.modern_font_base(text_model, 1)?;
                call?;
                pair
            } else {
                m.cpu.write_u32(out, 0)?;
                let value = m.modern_ui_invoke(model, 6, [out, 0, 0], &[])?;
                (value, m.cpu.read_u32(out)?)
            };
            if count == 0 {
                return Ok(Vec::new());
            }
            if count > 4096 || value == 0 {
                return Err(CpuError("texto Static fora do limite virtual".into()));
            }
            let mut bytes = vec![0; count as usize * 2];
            m.cpu.read_mem(value, &mut bytes)?;
            let units = bytes
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect::<Vec<_>>();
            if String::from_utf16(&units).is_err() {
                return Err(CpuError("texto UTF16 invalido no Static".into()));
            }
            Ok(units)
        });
        self.modern_font_base(model, 1)?;
        result
    }
    fn static_layout_flags(&self, state: u32) -> Result<u32, CpuError> {
        let flags = self.cpu.read_u32(state + FLAGS)?;
        Ok(if flags & WRAP != 0 {
            0x1000_0000
                | if flags & NO_BREAK == 0 {
                    0x2000_0000
                } else {
                    0
                }
        } else if flags & NO_SHORT == 0 {
            0x4000_0000
        } else {
            0
        })
    }
    fn static_layout_prepare(&mut self, state: u32, layout: u32) -> Result<(), CpuError> {
        let focused = self.cpu.read_u32(state + FOCUSED)? != 0;
        let selected = self.cpu.read_u32(state + SELECTED)? != 0;
        let fg = self.cpu.read_u32(
            state
                + match (focused, selected) {
                    (true, false) => FG_A,
                    (false, false) => FG_I,
                    (true, true) => FG_SA,
                    (false, true) => FG_SI,
                },
        )?;
        for (prop, value) in [
            (0x152, self.cpu.read_u32(state + FONT)?),
            (0x418, self.cpu.read_u32(state + MAP)?),
            (0x303, self.cpu.read_u32(state + GAP)?),
            (0x173, self.cpu.read_u32(state + HINT_W)?),
            (0x171, self.cpu.read_u32(state + HINT_ROWS)?),
            (0x349, self.cpu.read_u32(state + ELLIPSIS)?),
            (0x140, (fg & !255) | 255),
            (0x130, 0),
        ] {
            self.modern_ui_invoke(layout, 3, [0x801, prop, value], &[])?;
        }
        Ok(())
    }
    fn static_layout_preferred(&mut self, state: u32, layout: u32) -> Result<(u32, u32), CpuError> {
        self.static_layout_prepare(state, layout)?;
        let flags = self.static_layout_flags(state)?;
        let model = self.cpu.read_u32(state + MODEL)?;
        let (width, height) = self.modern_ui_scratch(8, |m, out| {
            let rc = m.modern_ui_invoke(layout, 4, [model, flags, out], &[])?;
            if rc != 0 {
                return Err(CpuError(format!("layout recusou modelo: {rc}")));
            }
            Ok((m.cpu.read_u32(out)?, m.cpu.read_u32(out + 4)?))
        })?;
        self.static_padded_preferred(state, width, height)
    }
    fn static_layout_draw(
        &mut self,
        state: u32,
        layout: u32,
        canvas: u32,
        x: i32,
        y: i32,
    ) -> Result<u32, CpuError> {
        let Some(&target) = self.canvases.get(&canvas) else {
            return Ok(20);
        };
        let Some(surface) = self.bitmaps.get(&target) else {
            return Ok(20);
        };
        let (w, h) = (
            self.cpu.read_u32(state + WIDTH)? as i32,
            self.cpu.read_u32(state + HEIGHT)? as i32,
        );
        let (mut x0, mut y0, mut x1, mut y1) = (
            x.max(0),
            y.max(0),
            x.saturating_add(w).min(surface.width() as i32),
            y.saturating_add(h).min(surface.height() as i32),
        );
        let old = self.clip;
        if let Some(c) = old {
            x0 = x0.max(c.x as i32);
            y0 = y0.max(c.y as i32);
            x1 = x1.min(c.x as i32 + c.width as i32);
            y1 = y1.min(c.y as i32 + c.height as i32)
        }
        if x1 <= x0 || y1 <= y0 {
            return Ok(0);
        }
        let clip = Rect {
            x: x0 as i16,
            y: y0 as i16,
            width: (x1 - x0).min(i16::MAX as i32) as i16,
            height: (y1 - y0).min(i16::MAX as i32) as i16,
        };
        let focused = self.cpu.read_u32(state + FOCUSED)? != 0;
        let selected = self.cpu.read_u32(state + SELECTED)? != 0;
        let bg = self.cpu.read_u32(
            state
                + match (focused, selected) {
                    (true, false) => BG_A,
                    (false, false) => BG_I,
                    (true, true) => BG_SA,
                    (false, true) => BG_SI,
                },
        )?;
        let (gradient_style, gradient) =
            self.modern_widget_gradient(state + GRADIENT, focused, selected)?;
        self.modern_widget_background(
            target,
            Rect {
                x: x as i16,
                y: y as i16,
                width: w.min(i16::MAX as i32) as i16,
                height: h.min(i16::MAX as i32) as i16,
            },
            clip,
            bg,
            gradient,
            gradient_style,
        )?;
        let (content_x, content_y, width, height) = self.static_content(state, x, y)?;
        let Some(content_clip) =
            Self::static_content_clip(clip, content_x, content_y, width, height)
        else {
            return Ok(SUCCESS);
        };
        self.static_layout_prepare(state, layout)?;
        let model = self.cpu.read_u32(state + MODEL)?;
        let flags = self.static_layout_flags(state)?;
        self.clip = Some(content_clip);
        let result = self.modern_ui_scratch(16, |m, p| {
            m.cpu.write_u32(p, width)?;
            m.cpu.write_u32(p + 4, height)?;
            let rc = m.modern_ui_invoke(layout, 5, [model, p, flags], &[])?;
            if rc != 0 {
                return Ok(rc);
            }
            m.modern_ui_invoke(layout, 7, [p, p + 8, 0], &[])?;
            let (tw, th) = (m.cpu.read_u32(p)? as i32, m.cpu.read_u32(p + 4)? as i32);
            let style = m.cpu.read_u32(state + FLAGS)?;
            let dx = match style & 0x70 {
                0x20 => (width as i32 - tw) / 2,
                0x40 => width as i32 - tw,
                _ => 0,
            };
            let dy = match style & 0x700 {
                0x200 => (height as i32 - th) / 2,
                0x400 => height as i32 - th,
                _ => 0,
            };
            m.modern_ui_invoke(
                layout,
                6,
                [
                    canvas,
                    content_x.saturating_add(dx) as u32,
                    content_y.saturating_add(dy) as u32,
                ],
                &[],
            )
        });
        self.clip = old;
        result
    }
    fn static_font(&mut self, state: u32) -> Result<u32, CpuError> {
        let map = self.cpu.read_u32(state + MAP)?;
        if map != 0 {
            let out = self.malloc(8)?;
            if out == 0 {
                return Ok(0);
            }
            self.cpu.write_u32(out, 0)?;
            let result = self.modern_ui_invoke(map, 7, [0, out, 0], &[]);
            let font = self.cpu.read_u32(out)?;
            self.heap.free(out);
            if result? == SUCCESS && font != 0 {
                return Ok(font);
            }
        }
        let font = self.cpu.read_u32(state + FONT)?;
        self.modern_font_base(font, 0)?;
        Ok(font)
    }
    fn static_measure(
        &mut self,
        font: u32,
        text: &str,
        max: u32,
    ) -> Result<(usize, u32), CpuError> {
        let units = text.encode_utf16().collect::<Vec<_>>();
        let scratch = self.malloc(16 + (units.len() as u32 + 1) * 2)?;
        if scratch == 0 {
            return Err(CpuError("sem memória para medir texto static".into()));
        }
        self.cpu.write_mem(scratch, &[0; 16])?;
        self.cpu.write_mem(
            scratch + 16,
            &units
                .iter()
                .copied()
                .chain([0])
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        )?;
        let result = self.modern_ui_invoke(
            font,
            4,
            [scratch + 16, units.len() as u32, max],
            &[scratch, scratch + 4],
        );
        let fits = self.cpu.read_u32(scratch)? as usize;
        let pixels = self.cpu.read_u32(scratch + 4)?;
        self.heap.free(scratch);
        if result? != SUCCESS || fits > units.len() {
            return Err(CpuError("a fonte recusou medir texto static".into()));
        }
        Ok((fits, pixels))
    }
    fn static_font_height(&mut self, font: u32) -> Result<u32, CpuError> {
        let out = self.malloc(8)?;
        if out == 0 {
            return Err(CpuError("sem memória para métricas static".into()));
        }
        self.cpu.write_mem(out, &[0; 8])?;
        let result = self.modern_ui_invoke(font, 5, [out, 4, 0], &[]);
        let value = self.cpu.read_u32(out)?;
        self.heap.free(out);
        if result? != SUCCESS {
            return Err(CpuError("fonte sem FontInfo moderno".into()));
        }
        Ok(u32::from(value as u16) + u32::from((value >> 16) as u16))
    }
    fn static_lines(
        &mut self,
        font: u32,
        text: &str,
        width: u32,
        flags: u32,
        ellipsis: u32,
    ) -> Result<Vec<String>, CpuError> {
        let mut lines = Vec::new();
        for paragraph in text.split('\n') {
            if flags & WRAP == 0 {
                let mut line = paragraph.to_string();
                if flags & NO_SHORT == 0
                    && width > 0
                    && self.static_measure(font, &line, i32::MAX as u32)?.1 > width
                {
                    let dots = char::from_u32(ellipsis).unwrap_or('…').to_string();
                    let dw = self.static_measure(font, &dots, i32::MAX as u32)?.1;
                    let fit = self
                        .static_measure(font, &line, width.saturating_sub(dw))?
                        .0;
                    line = String::from_utf16_lossy(
                        &line.encode_utf16().take(fit).collect::<Vec<_>>(),
                    );
                    line.push_str(&dots);
                }
                lines.push(line);
                continue;
            }
            if width == 0 {
                lines.push(String::new());
                continue;
            }
            let mut remaining = paragraph.to_string();
            if remaining.is_empty() {
                lines.push(String::new());
            }
            while !remaining.is_empty() {
                let (fit, _) = self.static_measure(font, &remaining, width)?;
                let units = remaining.encode_utf16().collect::<Vec<_>>();
                if fit >= units.len() {
                    lines.push(remaining);
                    break;
                }
                let mut byte = 0;
                let mut count = 0;
                for (at, c) in remaining.char_indices() {
                    if count + c.len_utf16() > fit {
                        break;
                    }
                    count += c.len_utf16();
                    byte = at + c.len_utf8();
                }
                if byte == 0 {
                    byte = remaining.chars().next().unwrap().len_utf8();
                }
                if flags & NO_BREAK == 0 {
                    if let Some(space) = remaining[..byte].rfind(char::is_whitespace) {
                        if space > 0 {
                            byte = space;
                        }
                    }
                }
                lines.push(
                    remaining[..byte]
                        .trim_end_matches(char::is_whitespace)
                        .to_string(),
                );
                remaining = remaining[byte..]
                    .trim_start_matches(char::is_whitespace)
                    .to_string();
            }
        }
        Ok(lines)
    }
    fn static_preferred(&mut self, state: u32) -> Result<(u32, u32), CpuError> {
        let layout = self.cpu.read_u32(state + LAYOUT)?;
        if layout != 0 {
            return self.static_layout_preferred(state, layout);
        }
        let units = self.static_text(state)?;
        let text = String::from_utf16(&units).unwrap_or_default();
        let font = self.static_font(state)?;
        if font == 0 {
            return Ok((0, 0));
        }
        let result = (|| {
            let flags = self.cpu.read_u32(state + FLAGS)?;
            let hint = self.cpu.read_u32(state + HINT_W)?;
            let mut width = 0;
            for line in text.split('\n') {
                width = width.max(self.static_measure(font, line, i32::MAX as u32)?.1);
            }
            if flags & WRAP != 0 && hint != 0 {
                width = hint;
            }
            let lines = self.static_lines(
                font,
                &text,
                width,
                flags,
                self.cpu.read_u32(state + ELLIPSIS)?,
            )?;
            let rows = self.cpu.read_u32(state + HINT_ROWS)?;
            let rows = if rows == 0 { lines.len() as u32 } else { rows };
            let h = self.static_font_height(font)?;
            let gap = self.cpu.read_u32(state + GAP)?;
            Ok((
                width,
                h.saturating_mul(rows)
                    .saturating_add(gap.saturating_mul(rows.saturating_sub(1))),
            ))
        })();
        self.modern_font_base(font, 1)?;
        let (width, height) = result?;
        self.static_padded_preferred(state, width, height)
    }
    fn static_event(
        &mut self,
        widget: u32,
        state: u32,
        event: u32,
        mut prop: u32,
        mut data: u32,
    ) -> Result<u32, CpuError> {
        match event {
            0x700 => {
                self.cpu.write_u32(state + FOCUSED, (prop != 0) as u32)?;
                self.static_invalidate(widget, state)?;
                return Ok(TRUE);
            }
            0x701 | 0x702 => {
                let focus = event == 0x701 && self.cpu.read_u32(state + FOCUSED)? != 0;
                return Ok(self.modern_font_write(data, &[focus as u8]) as u32);
            }
            0x800 | 0x801 => {
                let set = event == 0x801;
                if prop == 0x600 {
                    if !self.modern_font_valid(data, 12) {
                        return Ok(FALSE);
                    }
                    let extended = self.cpu.read_u32(data)?;
                    let size = self.cpu.read_u32(data + 4)?;
                    let pointer = self.cpu.read_u32(data + 8)?;
                    if size != 4 || !self.modern_font_valid(pointer, 4) {
                        return Ok(FALSE);
                    }
                    prop = match extended {
                        0x01077ada => 0x120,
                        0x01077adb => 0x121,
                        0x01077adc => 0x122,
                        0x01077add => 0x123,
                        0x01077ade => 0x124,
                        _ => return Ok(FALSE),
                    };
                    data = if set {
                        self.cpu.read_u32(pointer)?
                    } else {
                        pointer
                    };
                }
                if let Some(handled) =
                    self.modern_widget_gradient_property(state + GRADIENT, set, prop, data)?
                {
                    if set && handled != 0 {
                        self.static_invalidate(widget, state)?;
                    }
                    return Ok(handled);
                }
                let field = match prop {
                    0x153 => Some(FLAGS),
                    0x151 => Some(SELECTED),
                    0x131 => Some(BG_A),
                    0x132 => Some(BG_I),
                    0x133 => Some(BG_SA),
                    0x134 => Some(BG_SI),
                    0x140 | 0x141 => Some(FG_A),
                    0x142 => Some(FG_I),
                    0x143 => Some(FG_SA),
                    0x144 => Some(FG_SI),
                    0x303 => Some(GAP),
                    0x173 => Some(HINT_W),
                    0x171 => Some(HINT_ROWS),
                    0x349 => Some(ELLIPSIS),
                    0x121 => Some(PAD_L),
                    0x122 => Some(PAD_R),
                    0x123 => Some(PAD_T),
                    0x124 => Some(PAD_B),
                    _ => None,
                };
                if let Some(field) = field {
                    if !set {
                        let v = self.cpu.read_u32(state + field)?;
                        if field == ELLIPSIS {
                            return Ok(
                                self.modern_font_write(data, &(v as u16).to_le_bytes()) as u32
                            );
                        }
                        return Ok(self.modern_font_write(data, &v.to_le_bytes()) as u32);
                    }
                    if field == FLAGS && (data & !(0x770 | WRAP | NO_SHORT | NO_BREAK) != 0) {
                        return Ok(FALSE);
                    }
                    if matches!(
                        field,
                        GAP | HINT_W | HINT_ROWS | PAD_L | PAD_R | PAD_T | PAD_B
                    ) && (data as i32) < 0
                    {
                        return Ok(FALSE);
                    }
                    if field == ELLIPSIS && char::from_u32(data).is_none() {
                        return Ok(FALSE);
                    }
                    if prop == 0x140 {
                        for f in [FG_A, FG_I, FG_SA, FG_SI] {
                            self.cpu.write_u32(state + f, data)?;
                        }
                    } else {
                        self.cpu.write_u32(state + field, data)?;
                    }
                    self.static_invalidate(widget, state)?;
                    return Ok(TRUE);
                }
                if set && matches!(prop, 0x120 | 0x130 | 0x135 | 0x145) {
                    if prop == 0x120 && (data as i32) < 0 {
                        return Ok(FALSE);
                    }
                    let fields: Vec<_> = match prop {
                        0x120 => vec![PAD_L, PAD_R, PAD_T, PAD_B],
                        0x130 => vec![BG_A, BG_I, BG_SA, BG_SI],
                        0x135 => vec![BG_SA, BG_SI],
                        _ => vec![FG_SA, FG_SI],
                    };
                    for f in fields {
                        self.cpu.write_u32(state + f, data)?;
                    }
                    self.static_invalidate(widget, state)?;
                    return Ok(TRUE);
                }
                if prop == 0x417 {
                    if !set {
                        let value = if self.cpu.read_u32(state + LAYOUT)? != 0 {
                            super::text_layout::CLASS_ID
                        } else {
                            0
                        };
                        return Ok(self.modern_font_write(data, &value.to_le_bytes()) as u32);
                    }
                    if data != super::text_layout::CLASS_ID {
                        return Ok(FALSE);
                    }
                    return self.modern_ui_scratch(8, |m, out| {
                        let rc = m.create_text_layout(out)?;
                        if rc != 0 {
                            return Ok(FALSE);
                        }
                        let layout = m.cpu.read_u32(out)?;
                        m.static_layout_prepare(state, layout)?;
                        let old = m.cpu.read_u32(state + LAYOUT)?;
                        m.cpu.write_u32(state + LAYOUT, layout)?;
                        m.modern_font_base(old, 1)?;
                        m.static_invalidate(widget, state)?;
                        Ok(TRUE)
                    });
                }
                if set && prop == 0x315 {
                    return self.modern_ui_scratch(8, |m, out| {
                        if m.create_virtual_system_font(data, out)? != SUCCESS {
                            return Ok(FALSE);
                        }
                        let font = m.cpu.read_u32(out)?;
                        let old = m.cpu.read_u32(state + FONT)?;
                        m.cpu.write_u32(state + FONT, font)?;
                        m.modern_font_base(old, 1)?;
                        m.static_invalidate(widget, state)?;
                        Ok(TRUE)
                    });
                }
                if matches!(prop, 0x152 | 0x418) {
                    let field = if prop == 0x152 { FONT } else { MAP };
                    if !set {
                        let p = self.cpu.read_u32(state + field)?;
                        if !self.modern_font_write(data, &p.to_le_bytes()) {
                            return Ok(FALSE);
                        }
                        self.modern_font_base(p, 0)?;
                        return Ok(TRUE);
                    }
                    if data == 0 && field == FONT {
                        return Ok(FALSE);
                    }
                    if data != 0 {
                        let expected = if field == FONT {
                            Interface::ModernHFont
                        } else {
                            Interface::FontMapModel
                        };
                        if self.objects.kind_of(data) != Some(expected) {
                            return Ok(FALSE);
                        }
                    }
                    self.modern_font_base(data, 0)?;
                    let old = self.cpu.read_u32(state + field)?;
                    self.cpu.write_u32(state + field, data)?;
                    self.modern_font_base(old, 1)?;
                    self.static_invalidate(widget, state)?;
                    return Ok(TRUE);
                }
                return Ok(FALSE);
            }
            _ => {}
        }
        Ok(FALSE)
    }
    pub(super) fn static_widget_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let this = self.arg(0);
        if slot <= 2 || self.static_state(this).is_none() {
            return self.static_widget_call_inner(slot);
        }
        // AdaptGet/handlers podem fechar a janela dona do widget durante Draw.
        self.objects.add_ref(this);
        let result = self.static_widget_call_inner(slot);
        self.modern_font_base(this, 1)?;
        result
    }
    fn static_widget_call_inner(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let this = self.arg(0);
        let (a1, a2, a3) = (self.arg(1), self.arg(2), self.arg(3));
        let Some(state) = self.static_state(this) else {
            return Ok(Some(BAD));
        };
        if slot == 14 {
            return Ok(Some(self.static_event(this, state, a1, a2, a3)?));
        }
        if slot == 15 {
            self.static_invalidate(this, state)?;
            return Ok(Some(SUCCESS));
        }
        if slot == 16 {
            return Ok(Some(self.static_draw(state, a1, a2 as i32, a3 as i32)?));
        }
        let result = match slot {
            0 => self.objects.add_ref(this),
            1 => {
                if self.objects.contagem(this) == 1 {
                    self.modern_widget_gradient_release(state + GRADIENT)?;
                    self.static_unlisten(state)?;
                    for field in [MODEL, FONT, MAP, LAYOUT] {
                        let p = self.cpu.read_u32(state + field)?;
                        self.cpu.write_u32(state + field, 0)?;
                        self.modern_font_base(p, 1)?;
                    }
                    for (free_field, context_field) in [(FREE, CONTEXT), (DRAW_FREE, DRAW_CONTEXT)]
                    {
                        let free = self.cpu.read_u32(state + free_field)?;
                        let context = self.cpu.read_u32(state + context_field)?;
                        self.cpu.write_u32(state + free_field, 0)?;
                        if free != 0 {
                            self.modern_ui_call(free, [context, 0, 0, 0], &[])?;
                        }
                    }
                    let facet = self.cpu.read_u32(state + DRAW_FACET)?;
                    self.objects.release(facet);
                    self.heap.free(state);
                }
                self.objects.release(this)
            }
            2 => {
                if !self.modern_font_write(a2, &0u32.to_le_bytes()) {
                    return Ok(Some(BAD));
                }
                let target = match a1 {
                    0x01000001 | 0x01015952 | 0x01015956 => this,
                    0x0103465d => self.cpu.read_u32(state + DRAW_FACET)?,
                    _ => 0,
                };
                if target != 0 {
                    self.cpu.write_u32(a2, target)?;
                    self.objects.add_ref(this);
                    SUCCESS
                } else {
                    3
                }
            }
            3 => {
                let handler = self.cpu.read_u32(state + HANDLER)?;
                let context = self.cpu.read_u32(state + CONTEXT)?;
                if handler == 0 {
                    FALSE
                } else {
                    self.modern_ui_call(handler, [context, a1, a2, a3], &[])?
                }
            }
            4 => {
                if !self.modern_font_valid(a1, 12) {
                    return Ok(Some(BAD));
                }
                let next = [
                    self.cpu.read_u32(a1)?,
                    self.cpu.read_u32(a1 + 4)?,
                    self.cpu.read_u32(a1 + 8)?,
                ];
                let old = [
                    self.cpu.read_u32(state + HANDLER)?,
                    self.cpu.read_u32(state + CONTEXT)?,
                    self.cpu.read_u32(state + FREE)?,
                ];
                if !self.modern_font_write(
                    a1,
                    &old.into_iter()
                        .flat_map(u32::to_le_bytes)
                        .collect::<Vec<_>>(),
                ) {
                    return Ok(Some(BAD));
                }
                for (f, v) in [HANDLER, CONTEXT, FREE].into_iter().zip(next) {
                    self.cpu.write_u32(state + f, v)?;
                }
                SUCCESS
            }
            5 | 6 => {
                let (w, h) = if slot == 5 {
                    self.static_preferred(state)?
                } else {
                    (
                        self.cpu.read_u32(state + WIDTH)?,
                        self.cpu.read_u32(state + HEIGHT)?,
                    )
                };
                if self.modern_font_write(
                    a1,
                    &[w, h]
                        .into_iter()
                        .flat_map(u32::to_le_bytes)
                        .collect::<Vec<_>>(),
                ) {
                    SUCCESS
                } else {
                    BAD
                }
            }
            7 => {
                if !self.modern_font_valid(a1, 8) {
                    return Ok(Some(BAD));
                }
                let (w, h) = (self.cpu.read_u32(a1)?, self.cpu.read_u32(a1 + 4)?);
                if (w as i32) < 0 || (h as i32) < 0 {
                    return Ok(Some(BAD));
                }
                self.cpu.write_u32(state + WIDTH, w)?;
                self.cpu.write_u32(state + HEIGHT, h)?;
                self.static_invalidate(this, state)?;
                SUCCESS
            }
            8 => {
                let parent = self.cpu.read_u32(state + PARENT)?;
                if !self.modern_font_write(a1, &parent.to_le_bytes()) {
                    return Ok(Some(BAD));
                }
                self.modern_font_base(parent, 0)?;
                SUCCESS
            }
            9 => {
                self.cpu.write_u32(state + PARENT, a1)?;
                SUCCESS
            }
            10 => {
                let function = self.cpu.read_u32(state + DRAW_FN)?;
                let context = self.cpu.read_u32(state + DRAW_CONTEXT)?;
                if function == 0 {
                    SUCCESS
                } else {
                    self.modern_ui_call(function, [context, a1, a2, a3], &[])?
                }
            }
            11 => FALSE,
            12 => {
                if !self.modern_font_write(a2, &0u32.to_le_bytes()) {
                    return Ok(Some(BAD));
                }
                let model = self.cpu.read_u32(state + MODEL)?;
                self.modern_ui_invoke(model, 2, [a1, a2, 0], &[])?
            }
            13 => {
                if !self.modern_font_valid(a1, 4) || !self.modern_text_model_pointer(a1) {
                    return Ok(Some(3));
                }
                let (status, next) = self.modern_ui_scratch(8, |m, p| {
                    let mut status = m.modern_ui_invoke(a1, 2, [0x0101593b, p, 0], &[])?;
                    if status != 0 {
                        m.cpu.write_u32(p, 0)?;
                        status = m.modern_ui_invoke(a1, 2, [0x01015946, p, 0], &[])?;
                    }
                    Ok((status, m.cpu.read_u32(p)?))
                })?;
                if status != 0 || next == 0 {
                    return Ok(Some(3));
                }
                self.static_unlisten(state)?;
                let old = self.cpu.read_u32(state + MODEL)?;
                self.cpu.write_u32(state + MODEL, next)?;
                self.static_listen(this, state, next)?;
                self.modern_font_base(old, 1)?;
                self.static_invalidate(this, state)?;
                SUCCESS
            }
            _ => return Ok(None),
        };
        Ok(Some(result))
    }
    pub(super) fn static_draw_handler_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let facet = self.arg(0);
        let (a1, a2) = (self.arg(1), self.arg(2));
        if self.objects.kind_of(facet) != Some(Interface::StaticDrawHandler) {
            return Ok(Some(BAD));
        }
        let owner = self.cpu.read_u32(facet + 4)?;
        let Some(state) = self.static_state(owner) else {
            return Ok(Some(BAD));
        };
        let result = match slot {
            0 => self.objects.add_ref(owner),
            1 => self.modern_font_base(owner, 1)?,
            2 => {
                if !self.modern_font_write(a2, &0u32.to_le_bytes()) {
                    return Ok(Some(BAD));
                }
                let target = match a1 {
                    0x01000001 | 0x01015952 | 0x01015956 => owner,
                    0x0103465d => facet,
                    _ => 0,
                };
                if target == 0 {
                    3
                } else {
                    self.cpu.write_u32(a2, target)?;
                    self.objects.add_ref(owner);
                    SUCCESS
                }
            }
            3 => {
                if !self.modern_font_valid(a1, 12) {
                    return Ok(Some(BAD));
                }
                let next = [
                    self.cpu.read_u32(a1)?,
                    self.cpu.read_u32(a1 + 4)?,
                    self.cpu.read_u32(a1 + 8)?,
                ];
                let old = [
                    self.cpu.read_u32(state + DRAW_FN)?,
                    self.cpu.read_u32(state + DRAW_CONTEXT)?,
                    self.cpu.read_u32(state + DRAW_FREE)?,
                ];
                if !self.modern_font_write(
                    a1,
                    &old.into_iter()
                        .flat_map(u32::to_le_bytes)
                        .collect::<Vec<_>>(),
                ) {
                    return Ok(Some(BAD));
                }
                for (field, value) in [DRAW_FN, DRAW_CONTEXT, DRAW_FREE].into_iter().zip(next) {
                    self.cpu.write_u32(state + field, value)?;
                }
                SUCCESS
            }
            _ => return Ok(None),
        };
        Ok(Some(result))
    }
    fn static_draw(&mut self, state: u32, canvas: u32, x: i32, y: i32) -> Result<u32, CpuError> {
        let layout = self.cpu.read_u32(state + LAYOUT)?;
        if layout != 0 {
            return self.static_layout_draw(state, layout, canvas, x, y);
        }
        let Some(&target) = self.canvases.get(&canvas) else {
            return Ok(20);
        };
        let Some(surface) = self.bitmaps.get(&target) else {
            return Ok(20);
        };
        let (w, h) = (
            self.cpu.read_u32(state + WIDTH)? as i32,
            self.cpu.read_u32(state + HEIGHT)? as i32,
        );
        let (mut x0, mut y0, mut x1, mut y1) = (
            x.max(0),
            y.max(0),
            x.saturating_add(w).min(surface.width() as i32),
            y.saturating_add(h).min(surface.height() as i32),
        );
        if let Some(clip) = self.clip {
            x0 = x0.max(i32::from(clip.x));
            y0 = y0.max(i32::from(clip.y));
            x1 = x1.min(i32::from(clip.x) + i32::from(clip.width));
            y1 = y1.min(i32::from(clip.y) + i32::from(clip.height));
        }
        if w <= 0 || h <= 0 || x1 <= x0 || y1 <= y0 {
            return Ok(SUCCESS);
        }
        let clip = Rect {
            x: x0 as i16,
            y: y0 as i16,
            width: (x1 - x0).min(i16::MAX as i32) as i16,
            height: (y1 - y0).min(i16::MAX as i32) as i16,
        };
        let units = self.static_text(state)?;
        let text = String::from_utf16(&units).unwrap_or_default();
        let font = self.static_font(state)?;
        if font == 0 {
            return Ok(20);
        }
        let (content_x, content_y, content_width, content_height) =
            self.static_content(state, x, y)?;
        let result = (|| {
            let flags = self.cpu.read_u32(state + FLAGS)?;
            let lines = self.static_lines(
                font,
                &text,
                content_width,
                flags,
                self.cpu.read_u32(state + ELLIPSIS)?,
            )?;
            let line_height = self.static_font_height(font)? as i32;
            let gap = self.cpu.read_u32(state + GAP)? as i32;
            let text_height = (line_height + gap)
                .saturating_mul(lines.len() as i32)
                .saturating_sub(gap);
            let active = self.cpu.read_u32(state + FOCUSED)? != 0;
            let selected = self.cpu.read_u32(state + SELECTED)? != 0;
            let (fg_field, bg_field) = match (active, selected) {
                (true, false) => (FG_A, BG_A),
                (false, false) => (FG_I, BG_I),
                (true, true) => (FG_SA, BG_SA),
                (false, true) => (FG_SI, BG_SI),
            };
            let fg = Rgb::from_rgbval(self.cpu.read_u32(state + fg_field)?).to_rgb565() as u32;
            let bg = Rgb::from_rgbval(self.cpu.read_u32(state + bg_field)?).to_rgb565() as u32;
            let (gradient_style, gradient) =
                self.modern_widget_gradient(state + GRADIENT, active, selected)?;
            self.modern_widget_background(
                target,
                Rect {
                    x: x as i16,
                    y: y as i16,
                    width: w.min(i16::MAX as i32) as i16,
                    height: h.min(i16::MAX as i32) as i16,
                },
                clip,
                self.cpu.read_u32(state + bg_field)?,
                gradient,
                gradient_style,
            )?;
            let Some(clip) = Self::static_content_clip(
                clip,
                content_x,
                content_y,
                content_width,
                content_height,
            ) else {
                return Ok(SUCCESS);
            };
            let (x, y, w, h) = (
                content_x,
                content_y,
                content_width as i32,
                content_height as i32,
            );
            let dy = match flags & 0x700 {
                0x200 => (h - text_height) / 2,
                0x400 => h - text_height,
                _ => 0,
            };
            for (index, line) in lines.iter().enumerate() {
                let width = self.static_measure(font, line, i32::MAX as u32)?.1 as i32;
                let dx = match flags & 0x70 {
                    0x20 => (w - width) / 2,
                    0x40 => w - width,
                    _ => 0,
                };
                let py = y
                    .saturating_add(dy)
                    .saturating_add((line_height + gap).saturating_mul(index as i32));
                if py >= y1 || py.saturating_add(line_height) <= y0 {
                    continue;
                }
                let chars = line.encode_utf16().collect::<Vec<_>>();
                let scratch = self.malloc(16 + (chars.len() as u32 + 1) * 2)?;
                if scratch == 0 {
                    return Ok(2);
                }
                self.cpu.write_mem(
                    scratch,
                    &[clip.x, clip.y, clip.width, clip.height]
                        .into_iter()
                        .flat_map(i16::to_le_bytes)
                        .collect::<Vec<_>>(),
                )?;
                self.cpu.write_mem(
                    scratch + 16,
                    &chars
                        .iter()
                        .copied()
                        .chain([0])
                        .flat_map(u16::to_le_bytes)
                        .collect::<Vec<_>>(),
                )?;
                let px = x.saturating_add(dx);
                let call = self.modern_ui_invoke(
                    font,
                    3,
                    [target, px as u32, py as u32],
                    &[scratch + 16, chars.len() as u32, fg, bg, scratch, 0x8000],
                );
                self.heap.free(scratch);
                let code = call?;
                if code != SUCCESS {
                    return Ok(code);
                }
                if line.chars().any(|c| !c.is_whitespace()) {
                    if self.textos_desenhados.len() >= MAX_TEXTOS_DESENHADOS {
                        self.textos_desenhados.pop_front();
                    }
                    self.textos_desenhados
                        .push_back((self.clock_ms(), px, py, line.clone()));
                }
            }
            Ok(SUCCESS)
        })();
        self.modern_font_base(font, 1)?;
        result
    }
}
