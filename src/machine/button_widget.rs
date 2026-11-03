//! CButtonWidget público: decoração real e notificações IModel do SDK.
use super::*;
pub(super) const CLASS_ID: u32 = 0x0105_3c8b;
const BAD: u32 = 14;
const BYTES: u32 = 208;
const W: u32 = 0;
const H: u32 = 4;
const PARENT: u32 = 8;
const CHILD: u32 = 12;
const MODEL: u32 = 16;
const HANDLER: u32 = 20;
const CONTEXT: u32 = 24;
const FREE: u32 = 28;
const FOCUS: u32 = 32;
const SELECTED: u32 = 36;
const DISABLED: u32 = 40;
const PRESSED: u32 = 44;
const LAYOUT: u32 = 48;
const BW_A: u32 = 52;
const BW_I: u32 = 56;
const PAD_L: u32 = 60;
const PAD_R: u32 = 64;
const PAD_T: u32 = 68;
const PAD_B: u32 = 72;
const STYLE: u32 = 76;
const RADIUS: u32 = 80;
const BC_A: u32 = 84;
const BC_I: u32 = 88;
const BC_SA: u32 = 92;
const BC_SI: u32 = 96;
const BG_A: u32 = 100;
const BG_I: u32 = 104;
const BG_SA: u32 = 108;
const BG_SI: u32 = 112;
const SH_X: u32 = 116;
const SH_Y: u32 = 120;
const SH_SX: u32 = 124;
const SH_SY: u32 = 128;
const SH_COLOR: u32 = 132;
const SH_SCOLOR: u32 = 136;
const SH_SET: u32 = 140;
const USER: u32 = 144;
const BUSY: u32 = 148;
const DESTROYING: u32 = 152;
const FACET: u32 = 160;
const VT: u32 = 168;
const PRIVATE: u32 = 17;
const GRADIENT: u32 = 156;

struct ButtonGeometry {
    border: Rect,
    client: Rect,
    shadow_x: i32,
    shadow_y: i32,
    width: u32,
}
fn button_rect(x: i32, y: i32, w: u32, h: u32) -> Rect {
    Rect {
        x: x.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
        y: y.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
        width: w.min(i16::MAX as u32) as i16,
        height: h.min(i16::MAX as u32) as i16,
    }
}
impl<C: CpuBackend> Machine<C> {
    fn button_state(&self, w: u32) -> Option<u32> {
        if self.objects.kind_of(w) != Some(Interface::ButtonWidget) {
            return None;
        }
        let s = self.cpu.read_u32(w + 4).ok()?;
        self.heap.size_of(s).filter(|n| *n >= BYTES).map(|_| s)
    }
    pub(super) fn cria_button_widget(&mut self, out: u32) -> Result<u32, CpuError> {
        if !self.modern_font_write(out, &0u32.to_le_bytes()) {
            return Ok(BAD);
        }
        let w = self.new_object(Interface::ButtonWidget)?;
        if w == 0 {
            return Ok(2);
        }
        let s = self.malloc(BYTES)?;
        if s == 0 {
            self.objects.release(w);
            return Ok(2);
        }
        let model = self.new_object(Interface::WindowViewModel)?;
        if model == 0 {
            self.heap.free(s);
            self.objects.release(w);
            return Ok(2);
        }
        self.cpu.write_mem(w + 4, &[0; 60])?;
        self.cpu.write_mem(model + 4, &[0; 60])?;
        self.cpu.write_mem(s, &[0; BYTES as usize])?;
        self.cpu.write_u32(w + 4, s)?;
        for (field, value) in [
            (MODEL, model),
            (LAYOUT, 1),
            (HANDLER, aee::encode(Interface::ButtonWidget, 16)),
            (CONTEXT, w),
            (SH_COLOR, 0xff),
            (SH_SCOLOR, 0xff),
            (FACET, s + VT),
            (FACET + 4, w),
        ] {
            self.cpu.write_u32(s + field, value)?;
        }
        for slot in 0..8 {
            self.cpu.write_u32(
                s + VT + slot * 4,
                aee::encode(Interface::ButtonWidget, PRIVATE + slot),
            )?;
        }
        self.cpu.write_u32(out, w)?;
        Ok(SUCCESS)
    }
    fn button_release(&mut self, w: u32, s: u32) -> Result<u32, CpuError> {
        if self.objects.contagem(w) == 1 {
            self.cpu.write_u32(s + DESTROYING, 1)?;
            self.modern_widget_gradient_release(s + GRADIENT)?;
            let child = self.cpu.read_u32(s + CHILD)?;
            let model = self.cpu.read_u32(s + MODEL)?;
            let free = self.cpu.read_u32(s + FREE)?;
            for field in [CHILD, MODEL, FREE] {
                self.cpu.write_u32(s + field, 0)?;
            }
            if child != 0 {
                self.modern_ui_invoke(child, 9, [0; 3], &[])?;
                self.modern_font_base(child, 1)?;
            }
            self.modern_font_base(model, 1)?;
            if free != 0 {
                self.modern_ui_call(free, [self.cpu.read_u32(s + CONTEXT)?, 0, 0, 0], &[])?;
            }
            self.heap.free(s);
        }
        Ok(self.objects.release(w))
    }
    fn button_invalidate(&mut self, w: u32, s: u32) -> Result<(), CpuError> {
        let parent = self.cpu.read_u32(s + PARENT)?;
        if parent != 0 {
            self.modern_ui_invoke(parent, 3, [w, 0, 3], &[])?;
        }
        Ok(())
    }
    fn button_forward(&mut self, s: u32, slot: u32, args: [u32; 3]) -> Result<u32, CpuError> {
        let child = self.cpu.read_u32(s + CHILD)?;
        if child == 0 {
            return Ok(FALSE);
        }
        self.modern_font_base(child, 0)?;
        let result = self.modern_ui_invoke(child, slot, args, &[]);
        self.modern_font_base(child, 1)?;
        result
    }
    fn button_shadow_offsets(&self, s: u32) -> Result<(i32, i32), CpuError> {
        let selected = self.cpu.read_u32(s + SELECTED)? != 0;
        let set = self.cpu.read_u32(s + SH_SET)?;
        let sx = self.cpu.read_u32(
            s + if selected && set & 1 != 0 {
                SH_SX
            } else {
                SH_X
            },
        )? as i32;
        let sy = self.cpu.read_u32(
            s + if selected && set & 2 != 0 {
                SH_SY
            } else {
                SH_Y
            },
        )? as i32;
        Ok((sx, sy))
    }
    fn button_geometry(&self, s: u32) -> Result<ButtonGeometry, CpuError> {
        let (w, h) = (self.cpu.read_u32(s + W)?, self.cpu.read_u32(s + H)?);
        let (sx, sy) = self.button_shadow_offsets(s)?;
        let dx = sx.unsigned_abs().min(w);
        let dy = sy.unsigned_abs().min(h);
        let bw = self.cpu.read_u32(
            s + if self.cpu.read_u32(s + FOCUS)? != 0 {
                BW_A
            } else {
                BW_I
            },
        )?;
        let border = button_rect(
            if sx < 0 { dx as i32 } else { 0 },
            if sy < 0 { dy as i32 } else { 0 },
            w - dx,
            h - dy,
        );
        let bx = (w - dx).min(bw);
        let by = (h - dy).min(bw);
        let iw = (w - dx).saturating_sub(bw.saturating_mul(2));
        let ih = (h - dy).saturating_sub(bw.saturating_mul(2));
        let l = self.cpu.read_u32(s + PAD_L)?.min(iw);
        let r = self.cpu.read_u32(s + PAD_R)?.min(iw - l);
        let t = self.cpu.read_u32(s + PAD_T)?.min(ih);
        let b = self.cpu.read_u32(s + PAD_B)?.min(ih - t);
        Ok(ButtonGeometry {
            border,
            client: button_rect(
                i32::from(border.x) + (bx + l) as i32,
                i32::from(border.y) + (by + t) as i32,
                iw - l - r,
                ih - t - b,
            ),
            shadow_x: if sx < 0 { -(dx as i32) } else { dx as i32 },
            shadow_y: if sy < 0 { -(dy as i32) } else { dy as i32 },
            width: bw,
        })
    }
    fn button_layout(&mut self, s: u32) -> Result<(), CpuError> {
        if self.cpu.read_u32(s + BUSY)? != 0 || self.cpu.read_u32(s + LAYOUT)? == 0 {
            return Ok(());
        }
        let child = self.cpu.read_u32(s + CHILD)?;
        if child == 0 {
            return Ok(());
        }
        let rect = self.button_geometry(s)?.client;
        self.modern_font_base(child, 0)?;
        self.cpu.write_u32(s + BUSY, 1)?;
        let result = self.modern_ui_scratch(8, |m, p| {
            m.modern_ui_invoke(child, 6, [p, 0, 0], &[])?;
            if m.cpu.read_u32(p)? != rect.width as u32
                || m.cpu.read_u32(p + 4)? != rect.height as u32
            {
                m.cpu.write_u32(p, rect.width as u32)?;
                m.cpu.write_u32(p + 4, rect.height as u32)?;
                m.modern_ui_invoke(child, 7, [p, 0, 0], &[])?;
            }
            Ok(())
        });
        self.cpu.write_u32(s + BUSY, 0)?;
        self.modern_font_base(child, 1)?;
        result
    }
    fn button_cycle(&mut self, w: u32, candidate: u32) -> Result<bool, CpuError> {
        let mut current = w;
        let mut held = false;
        let mut seen = Vec::new();
        for _ in 0..64 {
            if current == candidate || seen.contains(&current) {
                if held {
                    self.modern_font_base(current, 1)?;
                }
                return Ok(true);
            }
            seen.push(current);
            let next = self.modern_ui_scratch(8, |m, p| {
                m.modern_ui_invoke(current, 8, [p, 0, 0], &[])?;
                let parent = m.cpu.read_u32(p)?;
                if parent == 0 {
                    return Ok(0);
                }
                m.cpu.write_u32(p, 0)?;
                let result = m.modern_ui_invoke(parent, 2, [0x01015952, p, 0], &[]);
                let next = m.cpu.read_u32(p)?;
                m.modern_font_base(parent, 1)?;
                Ok(if result? == 0 { next } else { 0 })
            });
            if held {
                self.modern_font_base(current, 1)?;
            }
            current = next?;
            held = current != 0;
            if current == 0 {
                return Ok(false);
            }
        }
        if held {
            self.modern_font_base(current, 1)?;
        }
        Ok(true)
    }
    fn button_set_child(&mut self, w: u32, s: u32, candidate: u32) -> Result<u32, CpuError> {
        let old = self.cpu.read_u32(s + CHILD)?;
        if candidate == old {
            return Ok(SUCCESS);
        }
        let next = if candidate == 0 {
            0
        } else {
            let child = self.modern_ui_scratch(8, |m, p| {
                if m.modern_ui_invoke(candidate, 2, [0x01015952, p, 0], &[])? != 0 {
                    return Ok(0);
                }
                let child = m.cpu.read_u32(p)?;
                if child == 0 {
                    return Ok(0);
                }
                if m.button_cycle(w, child)? {
                    m.modern_font_base(child, 1)?;
                    return Ok(0);
                }
                m.cpu.write_u32(p, 0)?;
                let result = m.modern_ui_invoke(child, 8, [p, 0, 0], &[]);
                let parent = m.cpu.read_u32(p)?;
                m.modern_font_base(parent, 1)?;
                if let Err(error) = result {
                    m.modern_font_base(child, 1)?;
                    return Err(error);
                }
                if parent != 0 {
                    m.modern_font_base(child, 1)?;
                    return Ok(0);
                }
                Ok(child)
            })?;
            if child == 0 {
                return Ok(BAD);
            }
            child
        };
        self.cpu.write_u32(s + CHILD, next)?;
        if next != 0 {
            // SetParent e eventos do filho podem trocar a própria composição.
            // A referência local sobrevive mesmo se o callback remover o filho.
            self.modern_font_base(next, 0)?;
            let initialized: Result<(), CpuError> = (|| {
                self.modern_ui_invoke(next, 9, [s + FACET, 0, 0], &[])?;
                if self.cpu.read_u32(s + CHILD)? == next {
                    self.modern_ui_invoke(next, 3, [0x700, self.cpu.read_u32(s + FOCUS)?, 0], &[])?;
                }
                if self.cpu.read_u32(s + CHILD)? == next {
                    self.modern_ui_invoke(
                        next,
                        3,
                        [0x801, 0x151, self.cpu.read_u32(s + SELECTED)?],
                        &[],
                    )?;
                }
                Ok(())
            })();
            self.modern_font_base(next, 1)?;
            initialized?;
        }
        if old != 0 {
            self.modern_ui_invoke(old, 9, [0; 3], &[])?;
            self.modern_font_base(old, 1)?;
        }
        self.button_layout(s)?;
        self.button_invalidate(w, s)?;
        Ok(SUCCESS)
    }
    fn button_notify(&mut self, s: u32, event: u32, detail: u32) -> Result<(), CpuError> {
        let model = self.cpu.read_u32(s + MODEL)?;
        self.modern_font_base(model, 0)?;
        let result = self.modern_ui_scratch(24, |m, p| {
            for (i, value) in [event, model, 0, 0, 0, detail].into_iter().enumerate() {
                m.cpu.write_u32(p + i as u32 * 4, value)?;
            }
            // Notify é void; o efeito está no callback realmente executado.
            m.modern_ui_invoke(model, 4, [p, 0, 0], &[])?;
            Ok(())
        });
        self.modern_font_base(model, 1)?;
        result
    }
    fn button_state_changed(&mut self, w: u32, s: u32) -> Result<(), CpuError> {
        let flags = (self.cpu.read_u32(s + FOCUS)? != 0) as u32
            | (((self.cpu.read_u32(s + SELECTED)? != 0) as u32) << 1)
            | (((self.cpu.read_u32(s + DISABLED)? != 0) as u32) << 2);
        self.button_layout(s)?;
        self.button_invalidate(w, s)?;
        self.button_notify(s, 0x1012, flags)
    }
    fn button_selected(&mut self, w: u32, s: u32, value: u32) -> Result<(), CpuError> {
        let value = (value != 0) as u32;
        if self.cpu.read_u32(s + SELECTED)? != value {
            self.cpu.write_u32(s + SELECTED, value)?;
            self.button_forward(s, 3, [0x801, 0x151, value])?;
            self.button_state_changed(w, s)?;
        }
        Ok(())
    }
    fn button_property(
        &mut self,
        w: u32,
        s: u32,
        set: bool,
        mut key: u32,
        mut data: u32,
    ) -> Result<u32, CpuError> {
        if key == 0x600 {
            if !self.modern_font_valid(data, 12) {
                return Ok(FALSE);
            }
            let ex = self.cpu.read_u32(data)?;
            let size = self.cpu.read_u32(data + 4)?;
            let p = self.cpu.read_u32(data + 8)?;
            if matches!(ex, 0x01070bd3 | 0x0107034f) {
                if size != 1 || !self.modern_font_valid(p, 1) {
                    return Ok(FALSE);
                }
                if !set {
                    if ex != 0x01070bd3 {
                        return Ok(FALSE);
                    }
                    return Ok(
                        self.modern_font_write(p, &[self.cpu.read_u32(s + DISABLED)? as u8]) as u32,
                    );
                }
                let mut value = [0];
                self.cpu.read_mem(p, &mut value)?;
                if ex == 0x0107034f {
                    if self.cpu.read_u32(s + DISABLED)? != 0 {
                        return Ok(FALSE);
                    }
                    self.cpu.write_u32(s + PRESSED, (value[0] != 0) as u32)?;
                    self.button_selected(w, s, value[0] as u32)?;
                } else {
                    let next = (value[0] != 0) as u32;
                    if self.cpu.read_u32(s + DISABLED)? != next {
                        self.cpu.write_u32(s + DISABLED, next)?;
                        self.cpu.write_u32(s + PRESSED, 0)?;
                        self.button_state_changed(w, s)?;
                    }
                }
                return Ok(TRUE);
            }
            if ex == 0x0106fb2c {
                // PROPEX_USER_DATA
                if size != 4 || !self.modern_font_valid(p, 4) {
                    return Ok(FALSE);
                }
                if set {
                    self.cpu.write_u32(s + USER, self.cpu.read_u32(p)?)?;
                    return Ok(TRUE);
                }
                return Ok(
                    self.modern_font_write(p, &self.cpu.read_u32(s + USER)?.to_le_bytes()) as u32,
                );
            }
            key = match ex {
                0x01077ada => 0x120,
                0x01077adb => 0x121,
                0x01077adc => 0x122,
                0x01077add => 0x123,
                0x01077ade => 0x124,
                0x01077adf => 0x100,
                0x01077ae0 => 0x101,
                0x01077ae1 => 0x102,
                _ => {
                    return self.button_forward(
                        s,
                        3,
                        [if set { 0x801 } else { 0x800 }, 0x600, data],
                    );
                }
            };
            if size != 4 || !self.modern_font_valid(p, 4) {
                return Ok(FALSE);
            }
            data = if set { self.cpu.read_u32(p)? } else { p };
        }
        if key == 0x161 {
            if set {
                return Ok(FALSE);
            }
            let model = self.cpu.read_u32(s + MODEL)?;
            if !self.modern_font_write(data, &model.to_le_bytes()) {
                return Ok(FALSE);
            }
            self.modern_font_base(model, 0)?;
            return Ok(TRUE);
        }
        if key == 0x320 && !set {
            let r = self.button_geometry(s)?.client;
            return Ok(self.modern_font_write(
                data,
                &[r.x, r.y, r.width, r.height]
                    .into_iter()
                    .flat_map(i16::to_le_bytes)
                    .collect::<Vec<_>>(),
            ) as u32);
        }
        if key == 0x151 {
            if !set {
                return Ok(self
                    .modern_font_write(data, &self.cpu.read_u32(s + SELECTED)?.to_le_bytes())
                    as u32);
            }
            self.button_selected(w, s, data)?;
            return Ok(TRUE);
        }
        if let Some(handled) = self.modern_widget_gradient_property(s + GRADIENT, set, key, data)? {
            if set && handled != 0 {
                self.button_invalidate(w, s)?;
            }
            return Ok(handled);
        }
        let field = match key {
            0x100 | 0x101 => Some(BW_A),
            0x102 => Some(BW_I),
            0x119 => Some(STYLE),
            0x103 => Some(RADIUS),
            0x110 | 0x111 => Some(BC_A),
            0x112 => Some(BC_I),
            0x113 | 0x115 => Some(BC_SA),
            0x114 => Some(BC_SI),
            0x121 => Some(PAD_L),
            0x122 => Some(PAD_R),
            0x123 => Some(PAD_T),
            0x124 => Some(PAD_B),
            0x130 | 0x131 => Some(BG_A),
            0x132 => Some(BG_I),
            0x133 | 0x135 => Some(BG_SA),
            0x134 => Some(BG_SI),
            0x204 => Some(SH_X),
            0x205 => Some(SH_Y),
            0x206 => Some(SH_COLOR),
            0x252 => Some(SH_SX),
            0x253 => Some(SH_SY),
            0x251 => Some(SH_SCOLOR),
            _ => None,
        };
        if let Some(field) = field {
            if !set {
                return Ok(
                    self.modern_font_write(data, &self.cpu.read_u32(s + field)?.to_le_bytes())
                        as u32,
                );
            }
            if [BW_A, BW_I, RADIUS, PAD_L, PAD_R, PAD_T, PAD_B].contains(&field)
                && (data as i32) < 0
            {
                return Ok(FALSE);
            }
            if field == STYLE && data > 2 {
                return Ok(FALSE);
            }
            let fields: &[u32] = match key {
                0x100 => &[BW_A, BW_I],
                0x110 => &[BC_A, BC_I, BC_SA, BC_SI],
                0x115 => &[BC_SA, BC_SI],
                0x130 => &[BG_A, BG_I, BG_SA, BG_SI],
                0x135 => &[BG_SA, BG_SI],
                _ => &[],
            };
            if fields.is_empty() {
                self.cpu.write_u32(s + field, data)?;
            } else {
                for f in fields {
                    self.cpu.write_u32(s + *f, data)?;
                }
            }
            let bit = match field {
                SH_SX => 1,
                SH_SY => 2,
                SH_SCOLOR => 4,
                _ => 0,
            };
            if bit != 0 {
                self.cpu
                    .write_u32(s + SH_SET, self.cpu.read_u32(s + SH_SET)? | bit)?;
            }
            self.button_layout(s)?;
            self.button_invalidate(w, s)?;
            return Ok(TRUE);
        }
        if set && matches!(key, 0x120 | 0x203 | 0x250) {
            let fields: &[u32] = match key {
                0x120 => &[PAD_L, PAD_R, PAD_T, PAD_B],
                0x203 => &[SH_X, SH_Y],
                _ => &[SH_SX, SH_SY],
            };
            if key == 0x120 && (data as i32) < 0 {
                return Ok(FALSE);
            }
            for field in fields {
                self.cpu.write_u32(s + *field, data)?;
            }
            if key == 0x250 {
                self.cpu
                    .write_u32(s + SH_SET, self.cpu.read_u32(s + SH_SET)? | 3)?;
            }
            self.button_layout(s)?;
            self.button_invalidate(w, s)?;
            return Ok(TRUE);
        }
        self.button_forward(s, 3, [if set { 0x801 } else { 0x800 }, key, data])
    }
    fn button_event(
        &mut self,
        w: u32,
        s: u32,
        event: u32,
        param: u32,
        data: u32,
    ) -> Result<u32, CpuError> {
        if matches!(event, 0x800 | 0x801) {
            return self.button_property(w, s, event == 0x801, param, data);
        }
        if matches!(event, 0x701 | 0x702) {
            let yes = event == 0x702 || self.cpu.read_u32(s + FOCUS)? != 0;
            return Ok(self.modern_font_write(data, &[yes as u8]) as u32);
        }
        if event == 0x700 {
            let value = (param != 0) as u32;
            if self.cpu.read_u32(s + FOCUS)? != value {
                self.cpu.write_u32(s + FOCUS, value)?;
                self.button_forward(s, 3, [event, value, 0])?;
                self.button_state_changed(w, s)?;
            }
            return Ok(TRUE);
        }
        if event == 0x802 {
            if param > 2 {
                return Ok(FALSE);
            }
            self.cpu.write_u32(s + LAYOUT, (param != 0) as u32)?;
            self.button_layout(s)?;
            return Ok(TRUE);
        }
        if matches!(event, 0x100 | 0x102) && param == 0xe035 {
            if self.cpu.read_u32(s + DISABLED)? != 0 {
                return Ok(FALSE);
            }
            if event == 0x100 {
                self.cpu.write_u32(s + PRESSED, 1)?;
                self.button_selected(w, s, 1)?;
                return Ok(TRUE);
            }
            if self.cpu.read_u32(s + PRESSED)? != 0 {
                self.cpu.write_u32(s + PRESSED, 0)?;
                self.button_selected(w, s, 0)?;
                self.button_notify(s, 0x1008, 0)?;
                self.button_notify(s, 0x1013, 1)?;
                return Ok(TRUE);
            }
            return Ok(FALSE);
        }
        if event == 0x711 {
            return Ok(TRUE);
        }
        self.button_forward(s, 3, [event, param, data])
    }
    fn button_shape(
        &mut self,
        target: u32,
        rect: Rect,
        style: u32,
        radius: u32,
        color: u32,
    ) -> Result<(), CpuError> {
        self.button_shape_background(target, rect, style, radius, color, color, 4)
    }
    fn button_shape_background(
        &mut self,
        target: u32,
        rect: Rect,
        style: u32,
        radius: u32,
        color: u32,
        gradient: u32,
        gradient_style: u32,
    ) -> Result<(), CpuError> {
        if rect.width <= 0 || rect.height <= 0 {
            return Ok(());
        }
        if style == 0 || radius == 0 {
            return self.modern_widget_background(
                target,
                rect,
                rect,
                color,
                gradient,
                gradient_style,
            );
        }
        let radius = radius
            .min(rect.width as u32 / 2)
            .min(rect.height as u32 / 2);
        for row in 0..rect.height as u32 {
            let edge = row.min(rect.height as u32 - row - 1);
            let cut = if edge >= radius {
                0
            } else if style == 1 {
                radius - edge
            } else {
                let r = radius as f64;
                let dy = r - edge as f64 - 0.5;
                (r - (r * r - dy * dy).max(0.0).sqrt()).ceil() as u32
            };
            self.modern_widget_background(
                target,
                rect,
                button_rect(
                    i32::from(rect.x) + cut as i32,
                    i32::from(rect.y) + row as i32,
                    (rect.width as u32).saturating_sub(cut * 2),
                    1,
                ),
                color,
                gradient,
                gradient_style,
            )?;
        }
        Ok(())
    }
    fn button_draw(&mut self, s: u32, canvas: u32, x: i32, y: i32) -> Result<u32, CpuError> {
        self.button_layout(s)?;
        let Some(&target) = self.canvases.get(&canvas) else {
            return Ok(20);
        };
        let Some(surface) = self.bitmaps.get(&target) else {
            return Ok(20);
        };
        let (mut l, mut t) = (x.max(0), y.max(0));
        let mut r = x
            .saturating_add(self.cpu.read_u32(s + W)? as i32)
            .min(surface.width() as i32);
        let mut b = y
            .saturating_add(self.cpu.read_u32(s + H)? as i32)
            .min(surface.height() as i32);
        let previous = self.clip;
        if let Some(c) = previous {
            l = l.max(c.x as i32);
            t = t.max(c.y as i32);
            r = r.min(c.x as i32 + c.width as i32);
            b = b.min(c.y as i32 + c.height as i32);
        }
        if r <= l || b <= t {
            return Ok(SUCCESS);
        }
        self.clip = Some(button_rect(l, t, (r - l) as u32, (b - t) as u32));
        let result: Result<(), CpuError> = (|| {
            let g = self.button_geometry(s)?;
            let active = self.cpu.read_u32(s + FOCUS)? != 0;
            let selected = self.cpu.read_u32(s + SELECTED)? != 0;
            let border = button_rect(
                x.saturating_add(g.border.x as i32),
                y.saturating_add(g.border.y as i32),
                g.border.width as u32,
                g.border.height as u32,
            );
            let style = self.cpu.read_u32(s + STYLE)?;
            let radius = self.cpu.read_u32(s + RADIUS)?;
            let shadow = self.cpu.read_u32(
                s + if selected && self.cpu.read_u32(s + SH_SET)? & 4 != 0 {
                    SH_SCOLOR
                } else {
                    SH_COLOR
                },
            )?;
            if g.shadow_x != 0 || g.shadow_y != 0 {
                self.button_shape(
                    target,
                    button_rect(
                        border.x as i32 + g.shadow_x,
                        border.y as i32 + g.shadow_y,
                        border.width as u32,
                        border.height as u32,
                    ),
                    style,
                    radius,
                    shadow,
                )?;
            }
            let (bc, bg) = match (active, selected) {
                (true, false) => (BC_A, BG_A),
                (false, false) => (BC_I, BG_I),
                (true, true) => (BC_SA, BG_SA),
                (false, true) => (BC_SI, BG_SI),
            };
            let bc = self.cpu.read_u32(s + bc)?;
            let bg = self.cpu.read_u32(s + bg)?;
            if g.width != 0 {
                self.button_shape(target, border, style, radius, bc)?;
            }
            let inner = button_rect(
                border.x as i32 + g.width as i32,
                border.y as i32 + g.width as i32,
                (border.width as u32).saturating_sub(g.width.saturating_mul(2)),
                (border.height as u32).saturating_sub(g.width.saturating_mul(2)),
            );
            let (gradient_style, gradient) =
                self.modern_widget_gradient(s + GRADIENT, active, selected)?;
            self.button_shape_background(
                target,
                inner,
                style,
                radius.saturating_sub(g.width),
                bg,
                gradient,
                gradient_style,
            )?;
            let child = self.cpu.read_u32(s + CHILD)?;
            if child != 0 && g.client.width > 0 && g.client.height > 0 {
                self.modern_font_base(child, 0)?;
                let result = self.modern_ui_invoke(
                    child,
                    10,
                    [
                        canvas,
                        x.saturating_add(g.client.x as i32) as u32,
                        y.saturating_add(g.client.y as i32) as u32,
                    ],
                    &[],
                );
                self.modern_font_base(child, 1)?;
                result?; // Draw é void; R0 residual não altera a composição.
            }
            Ok(())
        })();
        self.clip = previous;
        result?;
        Ok(SUCCESS)
    }
    pub(super) fn button_widget_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let pointer = self.arg(0);
        let private = slot >= PRIVATE;
        let owner = if private {
            self.cpu.read_u32(pointer + 4).unwrap_or(0)
        } else {
            pointer
        };
        let Some(s) = self.button_state(owner) else {
            return Ok(Some(BAD));
        };
        if private && pointer != s + FACET {
            return Ok(Some(BAD));
        }
        let actual = if private { slot - PRIVATE } else { slot };
        if self.cpu.read_u32(s + DESTROYING)? != 0 {
            return Ok(Some(if actual <= 1 { 0 } else { BAD }));
        }
        if actual == 0 {
            return Ok(Some(self.objects.add_ref(owner)));
        }
        if actual == 1 {
            return Ok(Some(self.button_release(owner, s)?));
        }
        self.objects.add_ref(owner);
        let result = self.button_inner(owner, s, private, actual);
        let released = self.button_release(owner, s);
        match result {
            Err(e) => {
                let _ = released;
                Err(e)
            }
            Ok(value) => {
                released?;
                Ok(value)
            }
        }
    }
    fn button_inner(
        &mut self,
        w: u32,
        s: u32,
        private: bool,
        slot: u32,
    ) -> Result<Option<u32>, CpuError> {
        let (a, b, c) = (self.arg(1), self.arg(2), self.arg(3));
        if slot == 2 {
            if !self.modern_font_write(b, &0u32.to_le_bytes()) {
                return Ok(Some(BAD));
            }
            let target = match a {
                0x01000001 if private => s + FACET,
                0x01000001 | 0x01015934 | 0x01015952 | 0x01015956 => w,
                0x01015932 => s + FACET,
                _ => 0,
            };
            if target == 0 {
                return Ok(Some(3));
            }
            self.cpu.write_u32(b, target)?;
            self.objects.add_ref(w);
            return Ok(Some(SUCCESS));
        }
        let rc = if private {
            match slot {
                3 => {
                    if a != self.cpu.read_u32(s + CHILD)?
                        || c & !63 != 0
                        || (b != 0 && !self.modern_font_valid(b, 8))
                    {
                        BAD
                    } else {
                        self.button_layout(s)?;
                        self.button_invalidate(w, s)?;
                        SUCCESS
                    }
                }
                4 => {
                    if a != self.cpu.read_u32(s + CHILD)?
                        || !self.modern_font_valid(c, 8)
                        || (b != 0 && !self.modern_font_valid(b, 4))
                    {
                        return Ok(Some(BAD));
                    }
                    let client = self.button_geometry(s)?.client;
                    let mut rect = self.read_rect(c)?.unwrap();
                    rect.x = (rect.x as i32 + client.x as i32)
                        .clamp(i16::MIN as i32, i16::MAX as i32)
                        as i16;
                    rect.y = (rect.y as i32 + client.y as i32)
                        .clamp(i16::MIN as i32, i16::MAX as i32)
                        as i16;
                    self.cpu.write_mem(
                        c,
                        &[rect.x, rect.y, rect.width, rect.height]
                            .into_iter()
                            .flat_map(i16::to_le_bytes)
                            .collect::<Vec<_>>(),
                    )?;
                    let parent = self.cpu.read_u32(s + PARENT)?;
                    if parent != 0 {
                        self.modern_ui_invoke(parent, 4, [w, b, c], &[])?
                    } else {
                        if b != 0 {
                            self.cpu.write_u32(b, s + FACET)?;
                            self.objects.add_ref(w);
                        }
                        SUCCESS
                    }
                }
                5 => {
                    if self.cpu.read_u32(s + CHILD)? != 0 {
                        1
                    } else {
                        self.button_set_child(w, s, a)?
                    }
                }
                6 => {
                    if a == 0 || a != self.cpu.read_u32(s + CHILD)? {
                        BAD
                    } else {
                        self.button_set_child(w, s, 0)?
                    }
                }
                7 => {
                    let child = self.cpu.read_u32(s + CHILD)?;
                    if a == 0 || c != 0 { child } else { 0 }
                }
                _ => return Ok(None),
            }
        } else {
            match slot {
                3 => {
                    let handler = self.cpu.read_u32(s + HANDLER)?;
                    if handler == 0 {
                        FALSE
                    } else {
                        self.modern_ui_call(
                            handler,
                            [self.cpu.read_u32(s + CONTEXT)?, a, b, c],
                            &[],
                        )?
                    }
                }
                4 => {
                    if !self.modern_font_valid(a, 12) {
                        return Ok(Some(BAD));
                    }
                    let next = [
                        self.cpu.read_u32(a)?,
                        self.cpu.read_u32(a + 4)?,
                        self.cpu.read_u32(a + 8)?,
                    ];
                    let old = [
                        self.cpu.read_u32(s + HANDLER)?,
                        self.cpu.read_u32(s + CONTEXT)?,
                        self.cpu.read_u32(s + FREE)?,
                    ];
                    self.cpu.write_mem(
                        a,
                        &old.into_iter()
                            .flat_map(u32::to_le_bytes)
                            .collect::<Vec<_>>(),
                    )?;
                    for (field, value) in [HANDLER, CONTEXT, FREE].into_iter().zip(next) {
                        self.cpu.write_u32(s + field, value)?;
                    }
                    SUCCESS
                }
                5 | 6 => {
                    if !self.modern_font_valid(a, 8) {
                        return Ok(Some(BAD));
                    }
                    let (mut width, mut height) =
                        (self.cpu.read_u32(s + W)?, self.cpu.read_u32(s + H)?);
                    if slot == 5 {
                        let size = self.modern_ui_scratch(8, |m, p| {
                            m.button_forward(s, 5, [p, 0, 0])?;
                            Ok((m.cpu.read_u32(p)?, m.cpu.read_u32(p + 4)?))
                        })?;
                        let g = self.button_geometry(s)?;
                        let (sx, sy) = self.button_shadow_offsets(s)?;
                        width = size
                            .0
                            .saturating_add(g.width.saturating_mul(2))
                            .saturating_add(self.cpu.read_u32(s + PAD_L)?)
                            .saturating_add(self.cpu.read_u32(s + PAD_R)?)
                            .saturating_add(sx.unsigned_abs());
                        height = size
                            .1
                            .saturating_add(g.width.saturating_mul(2))
                            .saturating_add(self.cpu.read_u32(s + PAD_T)?)
                            .saturating_add(self.cpu.read_u32(s + PAD_B)?)
                            .saturating_add(sy.unsigned_abs());
                    }
                    self.cpu.write_u32(a, width)?;
                    self.cpu.write_u32(a + 4, height)?;
                    SUCCESS
                }
                7 => {
                    if !self.modern_font_valid(a, 8) {
                        return Ok(Some(BAD));
                    }
                    let (width, height) = (self.cpu.read_u32(a)?, self.cpu.read_u32(a + 4)?);
                    if (width as i32) < 0 || (height as i32) < 0 {
                        return Ok(Some(BAD));
                    }
                    self.cpu.write_u32(s + W, width)?;
                    self.cpu.write_u32(s + H, height)?;
                    self.button_layout(s)?;
                    self.button_invalidate(w, s)?;
                    SUCCESS
                }
                8 => {
                    let parent = self.cpu.read_u32(s + PARENT)?;
                    if !self.modern_font_write(a, &parent.to_le_bytes()) {
                        BAD
                    } else {
                        self.modern_font_base(parent, 0)?;
                        SUCCESS
                    }
                }
                9 => {
                    self.cpu.write_u32(s + PARENT, a)?;
                    SUCCESS
                }
                10 => self.button_draw(s, a, b as i32, c as i32)?,
                11 => FALSE,
                12 | 13 => {
                    if self.cpu.read_u32(s + CHILD)? == 0 {
                        if slot == 12 && !self.modern_font_write(b, &0u32.to_le_bytes()) {
                            BAD
                        } else {
                            3
                        }
                    } else {
                        self.button_forward(s, slot, [a, b, c])?
                    }
                }
                14 => self.button_set_child(w, s, a)?,
                15 => {
                    let child = self.cpu.read_u32(s + CHILD)?;
                    if !self.modern_font_write(a, &child.to_le_bytes()) {
                        BAD
                    } else {
                        self.modern_font_base(child, 0)?;
                        SUCCESS
                    }
                }
                16 => self.button_event(w, s, a, b, c)?,
                _ => return Ok(None),
            }
        };
        Ok(Some(rc))
    }
}
