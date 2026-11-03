//! CViewportWidget: IDecorator e IContainer têm tabelas distintas e um dono.
//! O conteúdo continua sendo um IWidget real; mover a janela só muda origem e clip.
use super::*;
pub(super) const CLASS_ID: u32 = 0x0105_51eb;
const BAD: u32 = 14;
const OOM: u32 = 2;
const STATE_BYTES: u32 = 176;
const WIDTH: u32 = 0;
const HEIGHT: u32 = 4;
const PARENT: u32 = 8;
const CHILD: u32 = 12;
const MODEL: u32 = 16;
const FLAGS: u32 = 20;
const LAYOUT: u32 = 24;
const INCREMENT: u32 = 28;
const OFFSET_X: u32 = 32;
const OFFSET_Y: u32 = 36;
const FOCUSED: u32 = 40;
const HANDLER: u32 = 44;
const CONTEXT: u32 = 48;
const FREE: u32 = 52;
const BORDER_A: u32 = 56;
const BORDER_I: u32 = 60;
const PAD_L: u32 = 64;
const PAD_R: u32 = 68;
const PAD_T: u32 = 72;
const PAD_B: u32 = 76;
const BG_A: u32 = 80;
const BG_I: u32 = 84;
const BG_SA: u32 = 88;
const BG_SI: u32 = 92;
const COLOR_A: u32 = 96;
const COLOR_I: u32 = 100;
const COLOR_SA: u32 = 104;
const COLOR_SI: u32 = 108;
const SELECTED: u32 = 112;
const BUSY: u32 = 116;
const CHILD_W: u32 = 120;
const CHILD_H: u32 = 124;
const VISIBLE_W: u32 = 128;
const VISIBLE_H: u32 = 132;
const DESTROYING: u32 = 136;
const LAST_X: u32 = 140;
const LAST_Y: u32 = 144;
const SCRATCH: u32 = 152;
impl<C: CpuBackend> Machine<C> {
    fn viewport_state(&self, iface: Interface, this: u32) -> Option<(u32, u32)> {
        if self.objects.kind_of(this) != Some(iface) {
            return None;
        }
        let owner = if iface == Interface::Viewport {
            this
        } else {
            self.cpu.read_u32(this + 4).ok()?
        };
        if self.objects.kind_of(owner) != Some(Interface::Viewport) {
            return None;
        }
        if iface == Interface::ViewportContainer && self.cpu.read_u32(owner + 8).ok() != Some(this)
        {
            return None;
        }
        let state = self.cpu.read_u32(owner + 4).ok()?;
        if self.heap.size_of(state).is_none_or(|n| n < STATE_BYTES) {
            return None;
        }
        Some((owner, state))
    }
    pub(super) fn create_viewport(&mut self, out: u32) -> Result<u32, CpuError> {
        if !self.modern_font_write(out, &0u32.to_le_bytes()) {
            return Ok(BAD);
        }
        let widget = self.new_object(Interface::Viewport)?;
        if widget == 0 {
            return Ok(OOM);
        }
        let container = self.new_object(Interface::ViewportContainer)?;
        if container == 0 {
            self.objects.release(widget);
            return Ok(OOM);
        }
        let state = self.malloc(STATE_BYTES)?;
        if state == 0 {
            self.objects.release(container);
            self.objects.release(widget);
            return Ok(OOM);
        }
        let model = self.new_object(Interface::WindowViewModel)?;
        if model == 0 {
            self.heap.free(state);
            self.objects.release(container);
            self.objects.release(widget);
            return Ok(OOM);
        }
        self.cpu.write_mem(widget + 4, &[0; 60])?;
        self.cpu.write_mem(container + 4, &[0; 60])?;
        self.cpu.write_mem(model + 4, &[0; 60])?;
        self.cpu.write_mem(state, &[0; STATE_BYTES as usize])?;
        self.cpu.write_u32(widget + 4, state)?;
        self.cpu.write_u32(widget + 8, container)?;
        self.cpu.write_u32(container + 4, widget)?;
        for (field, value) in [
            (MODEL, model),
            (LAYOUT, 3),
            (INCREMENT, 1),
            (HANDLER, aee::encode(Interface::Viewport, 16)),
            (CONTEXT, widget),
        ] {
            self.cpu.write_u32(state + field, value)?;
        }
        self.cpu.write_u32(out, widget)?;
        Ok(SUCCESS)
    }
    fn viewport_release(&mut self, owner: u32, state: u32) -> Result<u32, CpuError> {
        if self.objects.contagem(owner) != 1 {
            return Ok(self.objects.release(owner));
        }
        if self.cpu.read_u32(state + DESTROYING)? != 0 {
            return Ok(0);
        }
        self.cpu.write_u32(state + DESTROYING, 1)?;
        let facet = self.cpu.read_u32(owner + 8)?;
        let child = self.cpu.read_u32(state + CHILD)?;
        let model = self.cpu.read_u32(state + MODEL)?;
        let free = self.cpu.read_u32(state + FREE)?;
        let context = self.cpu.read_u32(state + CONTEXT)?;
        self.cpu.write_u32(state + CHILD, 0)?;
        self.cpu.write_u32(state + PARENT, 0)?;
        // Tornar ambos os ponteiros mortos antes dos callbacks evita ressuscitar estado já liberado.
        self.objects.release(owner);
        self.objects.release(facet);
        if child != 0 {
            self.modern_ui_invoke(child, 9, [0, 0, 0], &[])?;
            self.modern_font_base(child, 1)?;
        }
        self.modern_font_base(model, 1)?;
        if free != 0 {
            self.modern_ui_call(free, [context, 0, 0, 0], &[])?;
        }
        self.heap.free(state);
        Ok(0)
    }
    fn viewport_inner(&self, state: u32) -> Result<(i32, i32, u32, u32), CpuError> {
        let active = self.cpu.read_u32(state + FOCUSED)? != 0;
        let b = self
            .cpu
            .read_u32(state + if active { BORDER_A } else { BORDER_I })?;
        let l = self.cpu.read_u32(state + PAD_L)?.saturating_add(b);
        let r = self.cpu.read_u32(state + PAD_R)?.saturating_add(b);
        let t = self.cpu.read_u32(state + PAD_T)?.saturating_add(b);
        let d = self.cpu.read_u32(state + PAD_B)?.saturating_add(b);
        let w = self
            .cpu
            .read_u32(state + WIDTH)?
            .saturating_sub(l.saturating_add(r));
        let h = self
            .cpu
            .read_u32(state + HEIGHT)?
            .saturating_sub(t.saturating_add(d));
        Ok((l as i32, t as i32, w, h))
    }
    fn viewport_extent(
        &mut self,
        child: u32,
        state: u32,
        preferred: bool,
    ) -> Result<(u32, u32), CpuError> {
        if child == 0 {
            return Ok((0, 0));
        }
        let out = state + SCRATCH;
        self.cpu.write_u32(out, 0)?;
        self.cpu.write_u32(out + 4, 0)?;
        self.modern_ui_invoke(child, if preferred { 5 } else { 6 }, [out, 0, 0], &[])?;
        Ok((
            self.cpu.read_u32(out)?.min(i16::MAX as u32),
            self.cpu.read_u32(out + 4)?.min(i16::MAX as u32),
        ))
    }
    fn viewport_notify_scroll(&mut self, state: u32, xy: bool) -> Result<(), CpuError> {
        let model = self.cpu.read_u32(state + MODEL)?;
        if model == 0 {
            return Ok(());
        }
        let w = self.cpu.read_u32(state + CHILD_W)?.min(u16::MAX as u32);
        let h = self.cpu.read_u32(state + CHILD_H)?.min(u16::MAX as u32);
        let vw = self.cpu.read_u32(state + VISIBLE_W)?.min(w);
        let vh = self.cpu.read_u32(state + VISIBLE_H)?.min(h);
        let x = self.cpu.read_u32(state + OFFSET_X)?.min(u16::MAX as u32);
        let y = self.cpu.read_u32(state + OFFSET_Y)?.min(u16::MAX as u32);
        // Um listener pode trocar o modelo do viewport durante a primeira orientação.
        self.modern_font_base(model, 0)?;
        // ScrollEvent é síncrono e escalar; heap cheio não pode perder a notificação.
        let original_sp = self.cpu.read_reg(Reg::Sp);
        let event = match original_sp
            .checked_sub(32)
            .map(|sp| sp & !7)
            .filter(|sp| *sp >= loader::STACK_BASE + 16)
            .filter(|_| original_sp <= loader::STACK_BASE + loader::STACK_SIZE as u32)
        {
            Some(sp) => sp,
            None => {
                self.modern_font_base(model, 1)?;
                return Err(CpuError("pilha insuficiente para ScrollEvent".into()));
            }
        };
        self.cpu.write_reg(Reg::Sp, event);
        let result = (|| {
            if xy {
                let mut bytes = [0u8; 24];
                bytes[..4].copy_from_slice(&0x1065u32.to_le_bytes());
                bytes[4..8].copy_from_slice(&model.to_le_bytes());
                for (i, v) in [w, vw, x, h, vh, y].into_iter().enumerate() {
                    bytes[12 + i * 2..14 + i * 2].copy_from_slice(&(v as u16).to_le_bytes());
                }
                self.cpu.write_mem(event, &bytes)?;
                self.modern_ui_invoke(model, 4, [event, 0, 0], &[])?;
            } else {
                let layout = self.cpu.read_u32(state + LAYOUT)?;
                for (vertical, range, visible, position) in [(false, w, vw, x), (true, h, vh, y)] {
                    if layout & if vertical { 1 } else { 2 } == 0 {
                        continue;
                    }
                    let mut bytes = [0u8; 20];
                    bytes[..4].copy_from_slice(&0x1064u32.to_le_bytes());
                    bytes[4..8].copy_from_slice(&model.to_le_bytes());
                    bytes[12] = u8::from(vertical);
                    bytes[14..16].copy_from_slice(&(range as u16).to_le_bytes());
                    bytes[16..18].copy_from_slice(&(visible as u16).to_le_bytes());
                    bytes[18..20].copy_from_slice(&(position as u16).to_le_bytes());
                    self.cpu.write_mem(event, &bytes)?;
                    self.modern_ui_invoke(model, 4, [event, 0, 0], &[])?;
                }
            }
            Ok(())
        })();
        self.cpu.write_reg(Reg::Sp, original_sp);
        let cleanup = self.modern_font_base(model, 1);
        match result {
            Ok(()) => {
                cleanup?;
                Ok(())
            }
            Err(error) => {
                let _ = cleanup;
                Err(error)
            }
        }
    }
    fn viewport_invalidate(&mut self, owner: u32, state: u32, flags: u32) -> Result<(), CpuError> {
        let parent = self.cpu.read_u32(state + PARENT)?;
        if parent != 0 {
            self.modern_ui_invoke(parent, 3, [owner, 0, flags], &[])?;
        }
        Ok(())
    }
    fn viewport_layout(&mut self, owner: u32, state: u32, force: bool) -> Result<(), CpuError> {
        if self.cpu.read_u32(state + BUSY)? != 0 {
            return Ok(());
        }
        self.cpu.write_u32(state + BUSY, 1)?;
        let result = (|| {
            let child = self.cpu.read_u32(state + CHILD)?;
            let (_, _, vw, vh) = self.viewport_inner(state)?;
            let (mut w, mut h) = self.viewport_extent(child, state, false)?;
            if child != 0 && (w == 0 || h == 0) {
                let (pw, ph) = self.viewport_extent(child, state, true)?;
                if w == 0 {
                    w = pw;
                }
                if h == 0 {
                    h = ph;
                }
            }
            if child != 0 && self.cpu.read_u32(state + FLAGS)? & 1 != 0 {
                w = w.max(vw);
                h = h.max(vh);
            }
            if child != 0 {
                let actual = self.viewport_extent(child, state, false)?;
                if actual != (w, h) {
                    self.cpu.write_u32(state + SCRATCH, w)?;
                    self.cpu.write_u32(state + SCRATCH + 4, h)?;
                    self.modern_ui_invoke(child, 7, [state + SCRATCH, 0, 0], &[])?;
                }
            }
            let style = self.cpu.read_u32(state + LAYOUT)?;
            let oldx = self.cpu.read_u32(state + OFFSET_X)?;
            let oldy = self.cpu.read_u32(state + OFFSET_Y)?;
            let x = if style & 2 != 0 {
                (oldx as i32).max(0) as u32
            } else {
                0
            }
            .min(if style & 2 != 0 {
                w.saturating_sub(vw)
            } else {
                0
            });
            let y = if style & 1 != 0 {
                (oldy as i32).max(0) as u32
            } else {
                0
            }
            .min(if style & 1 != 0 {
                h.saturating_sub(vh)
            } else {
                0
            });
            let changed = force
                || x != self.cpu.read_u32(state + LAST_X)?
                || y != self.cpu.read_u32(state + LAST_Y)?
                || self.cpu.read_u32(state + CHILD_W)? != w
                || self.cpu.read_u32(state + CHILD_H)? != h
                || self.cpu.read_u32(state + VISIBLE_W)? != vw
                || self.cpu.read_u32(state + VISIBLE_H)? != vh;
            for (field, value) in [
                (OFFSET_X, x),
                (OFFSET_Y, y),
                (LAST_X, x),
                (LAST_Y, y),
                (CHILD_W, w),
                (CHILD_H, h),
                (VISIBLE_W, vw),
                (VISIBLE_H, vh),
            ] {
                self.cpu.write_u32(state + field, value)?;
            }
            Ok(changed)
        })();
        self.cpu.write_u32(state + BUSY, 0)?;
        if result? {
            self.viewport_notify_scroll(state, false)?;
            self.viewport_invalidate(owner, state, 2)?;
        }
        Ok(())
    }
    fn viewport_cycle(&mut self, owner: u32, candidate: u32, state: u32) -> Result<bool, CpuError> {
        if owner == candidate {
            return Ok(true);
        }
        let mut current = owner;
        let mut acquired = false;
        let mut seen = Vec::new();
        for _ in 0..64 {
            if seen.contains(&current) {
                if acquired {
                    self.modern_font_base(current, 1)?;
                }
                return Ok(true);
            }
            seen.push(current);
            self.cpu.write_u32(state + SCRATCH, 0)?;
            let call = self.modern_ui_invoke(current, 8, [state + SCRATCH, 0, 0], &[]);
            let parent = self.cpu.read_u32(state + SCRATCH)?;
            if acquired {
                self.modern_font_base(current, 1)?;
            }
            call?;
            if parent == 0 {
                return Ok(false);
            }
            self.cpu.write_u32(state + SCRATCH, 0)?;
            let qi = self.modern_ui_invoke(parent, 2, [0x0101_5952, state + SCRATCH, 0], &[]);
            let next = self.cpu.read_u32(state + SCRATCH)?;
            self.modern_font_base(parent, 1)?;
            if qi? != 0 || next == 0 {
                return Ok(false);
            }
            if next == candidate {
                self.modern_font_base(next, 1)?;
                return Ok(true);
            }
            current = next;
            acquired = true;
        }
        if acquired {
            self.modern_font_base(current, 1)?;
        }
        Ok(true)
    }
    fn viewport_set_child(&mut self, owner: u32, state: u32, child: u32) -> Result<u32, CpuError> {
        let old = self.cpu.read_u32(state + CHILD)?;
        if child == old {
            return Ok(SUCCESS);
        }
        let facet = self.cpu.read_u32(owner + 8)?;
        let next = if child == 0 {
            0
        } else {
            if self.viewport_cycle(owner, child, state)? {
                return Ok(BAD);
            }
            self.cpu.write_u32(state + SCRATCH, 0)?;
            let rc = self.modern_ui_invoke(child, 2, [0x0101_5952, state + SCRATCH, 0], &[])?;
            let next = self.cpu.read_u32(state + SCRATCH)?;
            if rc != 0 || next == 0 {
                return Ok(3);
            }
            self.cpu.write_u32(state + SCRATCH, 0)?;
            let parent_result = self.modern_ui_invoke(next, 8, [state + SCRATCH, 0, 0], &[]);
            let parent = self.cpu.read_u32(state + SCRATCH)?;
            if parent != 0 {
                self.modern_font_base(parent, 1)?;
            }
            if parent_result.is_err() || parent != 0 {
                self.modern_font_base(next, 1)?;
                parent_result?;
                return Ok(BAD);
            }
            next
        };
        self.cpu.write_u32(state + CHILD, next)?;
        if next != 0 {
            self.modern_ui_invoke(next, 9, [facet, 0, 0], &[])?;
        }
        if old != 0 {
            self.modern_ui_invoke(old, 9, [0, 0, 0], &[])?;
            self.modern_font_base(old, 1)?;
        }
        self.viewport_layout(owner, state, true)?;
        self.viewport_invalidate(owner, state, 3)?;
        Ok(SUCCESS)
    }
    fn viewport_property(
        &mut self,
        owner: u32,
        state: u32,
        event: u32,
        key: u32,
        value: u32,
    ) -> Result<Option<u32>, CpuError> {
        let set = event == 0x801;
        if key == 0x600 {
            if !self.modern_font_valid(value, 12) {
                return Ok(Some(FALSE));
            }
            let id = self.cpu.read_u32(value)?;
            let size = self.cpu.read_u32(value + 4)?;
            let p = self.cpu.read_u32(value + 8)?;
            if size < 4 || !self.modern_font_valid(p, 4) {
                return Ok(Some(FALSE));
            }
            let legacy = match id {
                0x0107_7ada => 0x120,
                0x0107_7adb => 0x121,
                0x0107_7adc => 0x122,
                0x0107_7add => 0x123,
                0x0107_7ade => 0x124,
                0x0107_7adf => 0x100,
                0x0107_7ae0 => 0x101,
                0x0107_7ae1 => 0x102,
                _ => return Ok(None),
            };
            return self.viewport_property(
                owner,
                state,
                event,
                legacy,
                if set { self.cpu.read_u32(p)? } else { p },
            );
        }
        if key == 0x161 {
            if set {
                return Ok(Some(FALSE));
            }
            let model = self.cpu.read_u32(state + MODEL)?;
            if !self.modern_font_write(value, &model.to_le_bytes()) {
                return Ok(Some(FALSE));
            }
            self.modern_font_base(model, 0)?;
            return Ok(Some(TRUE));
        }
        if key == 0x183 {
            if !set {
                return Ok(Some(FALSE));
            }
            self.viewport_layout(owner, state, false)?;
            if value != 0 && value != 0x1065 {
                return Ok(Some(FALSE));
            }
            self.viewport_notify_scroll(state, value == 0x1065)?;
            return Ok(Some(TRUE));
        }
        let fields: Vec<u32> = match key {
            0x100 => vec![BORDER_A, BORDER_I],
            0x101 => vec![BORDER_A],
            0x102 => vec![BORDER_I],
            0x110 => vec![COLOR_A, COLOR_I, COLOR_SA, COLOR_SI],
            0x111 => vec![COLOR_A],
            0x112 => vec![COLOR_I],
            0x113 => vec![COLOR_SA],
            0x114 => vec![COLOR_SI],
            0x115 => vec![COLOR_SA, COLOR_SI],
            0x120 => vec![PAD_L, PAD_R, PAD_T, PAD_B],
            0x121 => vec![PAD_L],
            0x122 => vec![PAD_R],
            0x123 => vec![PAD_T],
            0x124 => vec![PAD_B],
            0x130 => vec![BG_A, BG_I, BG_SA, BG_SI],
            0x131 => vec![BG_A],
            0x132 => vec![BG_I],
            0x133 => vec![BG_SA],
            0x134 => vec![BG_SI],
            0x135 => vec![BG_SA, BG_SI],
            0x151 => vec![SELECTED],
            0x153 => vec![FLAGS],
            0x154 => vec![OFFSET_X],
            0x155 => vec![OFFSET_Y],
            0x156 => vec![INCREMENT],
            0x207 => vec![LAYOUT],
            _ => return Ok(None),
        };
        if !set {
            self.viewport_layout(owner, state, false)?;
            if fields.len() != 1 {
                return Ok(Some(FALSE));
            }
            let v = self.cpu.read_u32(state + fields[0])?;
            return Ok(Some(u32::from(
                self.modern_font_write(value, &v.to_le_bytes()),
            )));
        }
        if matches!(key,0x100..=0x102|0x120..=0x124|0x156) && value > i16::MAX as u32 {
            return Ok(Some(FALSE));
        }
        // Consumir/panear toque exige um touch controller; não prometer esse caminho ausente.
        if key == 0x153 && value & !3 != 0 {
            return Ok(Some(FALSE));
        }
        if key == 0x207 && value > 3 {
            return Ok(Some(FALSE));
        }
        if key == 0x151 && value > 1 {
            return Ok(Some(FALSE));
        }
        let previous = if fields.len() == 1 {
            self.cpu.read_u32(state + fields[0])?
        } else {
            value
        };
        for field in fields {
            self.cpu.write_u32(state + field, value)?;
        }
        self.viewport_layout(owner, state, key == 0x207 && previous != value)?;
        self.viewport_invalidate(owner, state, 2)?;
        Ok(Some(TRUE))
    }
    fn viewport_default(
        &mut self,
        owner: u32,
        state: u32,
        event: u32,
        w: u32,
        dw: u32,
    ) -> Result<u32, CpuError> {
        if matches!(event, 0x800 | 0x801) {
            if let Some(rc) = self.viewport_property(owner, state, event, w, dw)? {
                return Ok(rc);
            }
        }
        let child = self.cpu.read_u32(state + CHILD)?;
        match event {
            0x700 => {
                self.cpu.write_u32(state + FOCUSED, u32::from(w != 0))?;
                if child != 0 {
                    self.modern_ui_invoke(child, 3, [event, w, dw], &[])?;
                }
                self.viewport_layout(owner, state, true)?;
                Ok(TRUE)
            }
            0x701 => Ok(u32::from(self.modern_font_write(
                dw,
                &[u8::from(self.cpu.read_u32(state + FOCUSED)? != 0)],
            ))),
            0x702 => Ok(u32::from(self.modern_font_write(dw, &[1]))),
            0x713 => {
                if !self.modern_font_write(dw, &child.to_le_bytes()) {
                    return Ok(FALSE);
                }
                self.modern_font_base(child, 0)?;
                Ok(TRUE)
            }
            0x100 | 0x900 => {
                self.viewport_layout(owner, state, false)?;
                if event == 0x100 && self.cpu.read_u32(state + FLAGS)? & 2 != 0 {
                    return if child == 0 {
                        Ok(FALSE)
                    } else {
                        self.modern_ui_invoke(child, 3, [event, w, dw], &[])
                    };
                }
                let inc = self.cpu.read_u32(state + INCREMENT)? as i32;
                let (_, _, vw, vh) = self.viewport_inner(state)?;
                let oldx = self.cpu.read_u32(state + OFFSET_X)?;
                let oldy = self.cpu.read_u32(state + OFFSET_Y)?;
                let (mut x, mut y) = (oldx as i32, oldy as i32);
                match (event, w) {
                    (0x100, 0xe031) | (0x900, 100) => y = y.saturating_sub(inc),
                    (0x100, 0xe032) | (0x900, 101) => y = y.saturating_add(inc),
                    (0x100, 0xe033) | (0x900, 110) => x = x.saturating_sub(inc),
                    (0x100, 0xe034) | (0x900, 111) => x = x.saturating_add(inc),
                    (0x900, 102) => y = y.saturating_sub(vh as i32),
                    (0x900, 103) => y = y.saturating_add(vh as i32),
                    (0x900, 112) => x = x.saturating_sub(vw as i32),
                    (0x900, 113) => x = x.saturating_add(vw as i32),
                    (0x900, 104..=106) => y = dw as i32,
                    (0x900, 114..=116) => x = dw as i32,
                    (0x900, 124) => {
                        if !self.modern_font_valid(dw, 8) {
                            return Ok(FALSE);
                        }
                        x = self.cpu.read_u32(dw)? as i32;
                        y = self.cpu.read_u32(dw + 4)? as i32;
                    }
                    _ => {
                        return if child == 0 {
                            Ok(FALSE)
                        } else {
                            self.modern_ui_invoke(child, 3, [event, w, dw], &[])
                        };
                    }
                }
                self.cpu.write_u32(state + OFFSET_X, x.max(0) as u32)?;
                self.cpu.write_u32(state + OFFSET_Y, y.max(0) as u32)?;
                self.viewport_layout(owner, state, false)?;
                Ok(u32::from(
                    self.cpu.read_u32(state + OFFSET_X)? != oldx
                        || self.cpu.read_u32(state + OFFSET_Y)? != oldy,
                ))
            }
            _ => {
                if child == 0 {
                    Ok(FALSE)
                } else {
                    self.modern_ui_invoke(child, 3, [event, w, dw], &[])
                }
            }
        }
    }
    fn viewport_paint(&mut self, target: u32, rect: Rect, color: u32) -> Result<(), CpuError> {
        let alpha = (color & 255) as u8;
        if alpha == 0 || rect.width <= 0 || rect.height <= 0 {
            return Ok(());
        }
        let Some(fb) = self.bitmaps.get(&target) else {
            return Ok(());
        };
        let mut x0 = (rect.x as i32).max(0);
        let mut y0 = (rect.y as i32).max(0);
        let mut x1 = (rect.x as i32 + rect.width as i32).min(fb.width() as i32);
        let mut y1 = (rect.y as i32 + rect.height as i32).min(fb.height() as i32);
        if let Some(c) = self.clip {
            x0 = x0.max(c.x as i32);
            y0 = y0.max(c.y as i32);
            x1 = x1.min(c.x as i32 + c.width as i32);
            y1 = y1.min(c.y as i32 + c.height as i32);
        }
        if x1 <= x0 || y1 <= y0 {
            return Ok(());
        }
        let rgb = Rgb::from_rgbval(color);
        let fb = self.bitmaps.get_mut(&target).unwrap();
        for y in y0..y1 {
            for x in x0..x1 {
                let old = Rgb::from_rgb565(fb.get_pixel(x, y));
                let mix = |a: u8, b: u8| {
                    ((u32::from(a) * u32::from(alpha)
                        + u32::from(b) * u32::from(255 - alpha)
                        + 127)
                        / 255) as u8
                };
                fb.set_pixel(
                    x,
                    y,
                    Rgb {
                        r: mix(rgb.r, old.r),
                        g: mix(rgb.g, old.g),
                        b: mix(rgb.b, old.b),
                    },
                );
            }
        }
        Ok(())
    }
    fn viewport_draw(
        &mut self,
        owner: u32,
        state: u32,
        canvas: u32,
        x: i32,
        y: i32,
    ) -> Result<(), CpuError> {
        self.viewport_layout(owner, state, false)?;
        let Some(&target) = self.canvases.get(&canvas) else {
            return Ok(());
        };
        let (l, t, vw, vh) = self.viewport_inner(state)?;
        let w = self.cpu.read_u32(state + WIDTH)? as i16;
        let h = self.cpu.read_u32(state + HEIGHT)? as i16;
        let surface = self.bitmaps.get(&target).map(|s| (s.width(), s.height()));
        if w <= 0
            || h <= 0
            || surface.is_none_or(|(sw, sh)| {
                i64::from(x) >= i64::from(sw)
                    || i64::from(y) >= i64::from(sh)
                    || i64::from(x) + i64::from(w) <= 0
                    || i64::from(y) + i64::from(h) <= 0
            })
        {
            return Ok(());
        }
        let active = self.cpu.read_u32(state + FOCUSED)? != 0;
        let selected = self.cpu.read_u32(state + SELECTED)? != 0;
        let bg = self.cpu.read_u32(
            state
                + match (active, selected) {
                    (true, false) => BG_A,
                    (false, false) => BG_I,
                    (true, true) => BG_SA,
                    (false, true) => BG_SI,
                },
        )?;
        let border = self
            .cpu
            .read_u32(state + if active { BORDER_A } else { BORDER_I })?
            .min(w.max(0) as u32)
            .min(h.max(0) as u32) as i16;
        let color = self.cpu.read_u32(
            state
                + match (active, selected) {
                    (true, false) => COLOR_A,
                    (false, false) => COLOR_I,
                    (true, true) => COLOR_SA,
                    (false, true) => COLOR_SI,
                },
        )?;
        self.viewport_paint(
            target,
            Rect {
                x: x as i16,
                y: y as i16,
                width: w,
                height: h,
            },
            bg,
        )?;
        if border > 0 {
            for r in [
                Rect {
                    x: x as i16,
                    y: y as i16,
                    width: w,
                    height: border,
                },
                Rect {
                    x: x as i16,
                    y: (y + h as i32 - border as i32) as i16,
                    width: w,
                    height: border,
                },
                Rect {
                    x: x as i16,
                    y: y as i16,
                    width: border,
                    height: h,
                },
                Rect {
                    x: (x + w as i32 - border as i32) as i16,
                    y: y as i16,
                    width: border,
                    height: h,
                },
            ] {
                self.viewport_paint(target, r, color)?;
            }
        }
        let child = self.cpu.read_u32(state + CHILD)?;
        if child == 0 || vw == 0 || vh == 0 {
            return Ok(());
        }
        let (mut x0, mut y0, mut x1, mut y1) = (
            x.saturating_add(l),
            y.saturating_add(t),
            x.saturating_add(l).saturating_add(vw as i32),
            y.saturating_add(t).saturating_add(vh as i32),
        );
        if let Some(c) = self.clip {
            x0 = x0.max(c.x as i32);
            y0 = y0.max(c.y as i32);
            x1 = x1.min(c.x as i32 + c.width as i32);
            y1 = y1.min(c.y as i32 + c.height as i32);
        }
        if x1 <= x0 || y1 <= y0 {
            return Ok(());
        }
        let old_target = self.display_target;
        let old_clip = self.clip;
        self.display_target = target;
        self.clip = Some(Rect {
            x: x0.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
            y: y0.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
            width: (x1 - x0).min(i16::MAX as i32) as i16,
            height: (y1 - y0).min(i16::MAX as i32) as i16,
        });
        let result = self.modern_ui_invoke(
            child,
            10,
            [
                canvas,
                x.saturating_add(l)
                    .saturating_sub(self.cpu.read_u32(state + OFFSET_X)? as i32)
                    as u32,
                y.saturating_add(t)
                    .saturating_sub(self.cpu.read_u32(state + OFFSET_Y)? as i32)
                    as u32,
            ],
            &[],
        );
        self.display_target = old_target;
        self.clip = old_clip;
        result?;
        Ok(())
    }
    pub(super) fn viewport_call(
        &mut self,
        iface: Interface,
        slot: u32,
    ) -> Result<Option<u32>, CpuError> {
        let this = self.arg(0);
        let (a1, a2, a3) = (self.arg(1), self.arg(2), self.arg(3));
        let Some((owner, state)) = self.viewport_state(iface, this) else {
            return Ok(Some(BAD));
        };
        if slot == 0 {
            return Ok(Some(self.objects.add_ref(owner)));
        }
        if slot == 1 {
            return Ok(Some(self.viewport_release(owner, state)?));
        }
        if slot == 2 {
            if !self.modern_font_write(a2, &0u32.to_le_bytes()) {
                return Ok(Some(BAD));
            }
            let ptr = match a1 {
                0x0100_0001 | 0x0101_5934 | 0x0101_5952 | 0x0101_5956 => owner,
                0x0101_5932 => self.cpu.read_u32(owner + 8)?,
                _ => return Ok(Some(3)),
            };
            self.objects.add_ref(owner);
            self.cpu.write_u32(a2, ptr)?;
            return Ok(Some(SUCCESS));
        }
        self.objects.add_ref(owner);
        let result = (|| {
            let value = if iface == Interface::ViewportContainer {
                match slot {
                    3 => {
                        if a1 != 0 && a1 != self.cpu.read_u32(state + CHILD)? {
                            return Ok(Some(BAD));
                        }
                        self.viewport_layout(owner, state, a3 & 1 != 0)?;
                        self.viewport_invalidate(owner, state, a3)?;
                        SUCCESS
                    }
                    4 => {
                        if (a2 != 0 && !self.modern_font_valid(a2, 4))
                            || (a3 != 0 && !self.modern_font_valid(a3, 8))
                        {
                            return Ok(Some(BAD));
                        }
                        let child = self.cpu.read_u32(state + CHILD)?;
                        if a1 != 0 && a1 != child && a1 != owner {
                            return Ok(Some(BAD));
                        }
                        let facet = self.cpu.read_u32(owner + 8)?;
                        let parent = self.cpu.read_u32(state + PARENT)?;
                        let (mut x, mut y, mut w, mut h) = (
                            0i32,
                            0i32,
                            self.cpu.read_u32(state + WIDTH)?,
                            self.cpu.read_u32(state + HEIGHT)?,
                        );
                        if a1 == child && child != 0 {
                            let (l, t, _, _) = self.viewport_inner(state)?;
                            x = l - self.cpu.read_u32(state + OFFSET_X)? as i32;
                            y = t - self.cpu.read_u32(state + OFFSET_Y)? as i32;
                            (w, h) = self.viewport_extent(child, state, false)?;
                        }
                        if parent == 0 {
                            if a2 != 0 {
                                if !self.modern_font_write(a2, &facet.to_le_bytes()) {
                                    return Ok(Some(BAD));
                                }
                                self.objects.add_ref(owner);
                            }
                        } else {
                            let rc = self.modern_ui_invoke(
                                parent,
                                4,
                                [owner, a2, state + SCRATCH],
                                &[],
                            )?;
                            if rc != 0 {
                                return Ok(Some(rc));
                            }
                            let mut bytes = [0; 8];
                            self.cpu.read_mem(state + SCRATCH, &mut bytes)?;
                            let r = Rect::from_bytes(bytes);
                            x = x.saturating_add(r.x as i32);
                            y = y.saturating_add(r.y as i32);
                        }
                        if a3 != 0 {
                            let r = Rect {
                                x: x.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
                                y: y.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
                                width: w.min(i16::MAX as u32) as i16,
                                height: h.min(i16::MAX as u32) as i16,
                            };
                            let bytes = [r.x, r.y, r.width, r.height]
                                .into_iter()
                                .flat_map(i16::to_le_bytes)
                                .collect::<Vec<_>>();
                            if !self.modern_font_write(a3, &bytes) {
                                return Ok(Some(BAD));
                            }
                        }
                        SUCCESS
                    }
                    5 => {
                        if self.cpu.read_u32(state + CHILD)? != 0 {
                            return Ok(Some(1));
                        }
                        self.viewport_set_child(owner, state, a1)?
                    }
                    6 => {
                        if a1 == 0 || a1 != self.cpu.read_u32(state + CHILD)? {
                            return Ok(Some(BAD));
                        }
                        self.viewport_set_child(owner, state, 0)?
                    }
                    7 => {
                        let child = self.cpu.read_u32(state + CHILD)?;
                        if child != 0 && (a1 == 0 || a1 == 1 || a1 == u32::MAX || a3 != 0) {
                            self.modern_font_base(child, 0)?;
                            child
                        } else {
                            0
                        }
                    }
                    _ => return Ok(None),
                }
            } else {
                match slot {
                    3 => {
                        let f = self.cpu.read_u32(state + HANDLER)?;
                        let c = self.cpu.read_u32(state + CONTEXT)?;
                        if f == 0 {
                            FALSE
                        } else {
                            self.modern_ui_call(f, [c, a1, a2, a3], &[])?
                        }
                    }
                    4 => {
                        if !self.modern_font_valid(a1, 12) {
                            return Ok(Some(BAD));
                        }
                        let values = [
                            self.cpu.read_u32(a1)?,
                            self.cpu.read_u32(a1 + 4)?,
                            self.cpu.read_u32(a1 + 8)?,
                        ];
                        for (i, field) in [HANDLER, CONTEXT, FREE].into_iter().enumerate() {
                            let old = self.cpu.read_u32(state + field)?;
                            self.cpu.write_u32(state + field, values[i])?;
                            self.cpu.write_u32(a1 + i as u32 * 4, old)?;
                        }
                        SUCCESS
                    }
                    5 | 6 => {
                        if !self.modern_font_valid(a1, 8) {
                            return Ok(Some(BAD));
                        }
                        let (w, h) = if slot == 5 {
                            let child = self.cpu.read_u32(state + CHILD)?;
                            let (cw, ch) = self.viewport_extent(child, state, true)?;
                            let active = self.cpu.read_u32(state + FOCUSED)? != 0;
                            let border = self
                                .cpu
                                .read_u32(state + if active { BORDER_A } else { BORDER_I })?;
                            let outerw = border * 2
                                + self.cpu.read_u32(state + PAD_L)?
                                + self.cpu.read_u32(state + PAD_R)?;
                            let outerh = border * 2
                                + self.cpu.read_u32(state + PAD_T)?
                                + self.cpu.read_u32(state + PAD_B)?;
                            (cw.saturating_add(outerw), ch.saturating_add(outerh))
                        } else {
                            (
                                self.cpu.read_u32(state + WIDTH)?,
                                self.cpu.read_u32(state + HEIGHT)?,
                            )
                        };
                        self.cpu.write_u32(a1, w)?;
                        self.cpu.write_u32(a1 + 4, h)?;
                        SUCCESS
                    }
                    7 => {
                        if !self.modern_font_valid(a1, 8) {
                            return Ok(Some(BAD));
                        }
                        let (w, h) = (self.cpu.read_u32(a1)?, self.cpu.read_u32(a1 + 4)?);
                        if w > i16::MAX as u32 || h > i16::MAX as u32 {
                            return Ok(Some(BAD));
                        }
                        self.cpu.write_u32(state + WIDTH, w)?;
                        self.cpu.write_u32(state + HEIGHT, h)?;
                        self.viewport_layout(owner, state, true)?;
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
                        if a1 == self.cpu.read_u32(owner + 8)? {
                            return Ok(Some(BAD));
                        }
                        self.cpu.write_u32(state + PARENT, a1)?;
                        SUCCESS
                    }
                    10 => {
                        self.viewport_draw(owner, state, a1, a2 as i32, a3 as i32)?;
                        SUCCESS
                    }
                    11 => FALSE,
                    12 => {
                        if !self.modern_font_write(a2, &0u32.to_le_bytes()) {
                            return Ok(Some(BAD));
                        }
                        if a1 != 0x0101_593a {
                            return Ok(Some(1));
                        }
                        let model = self.cpu.read_u32(state + MODEL)?;
                        self.modern_font_base(model, 0)?;
                        self.cpu.write_u32(a2, model)?;
                        SUCCESS
                    }
                    13 => {
                        if a1 == 0 {
                            return Ok(Some(BAD));
                        }
                        self.cpu.write_u32(state + SCRATCH, 0)?;
                        let rc =
                            self.modern_ui_invoke(a1, 2, [0x0101_593a, state + SCRATCH, 0], &[])?;
                        let model = self.cpu.read_u32(state + SCRATCH)?;
                        if rc != 0 || model == 0 {
                            return Ok(Some(3));
                        }
                        let old = self.cpu.read_u32(state + MODEL)?;
                        self.cpu.write_u32(state + MODEL, model)?;
                        self.modern_font_base(old, 1)?;
                        self.viewport_notify_scroll(state, false)?;
                        SUCCESS
                    }
                    14 => self.viewport_set_child(owner, state, a1)?,
                    15 => {
                        let child = self.cpu.read_u32(state + CHILD)?;
                        if !self.modern_font_write(a1, &child.to_le_bytes()) {
                            return Ok(Some(BAD));
                        }
                        self.modern_font_base(child, 0)?;
                        SUCCESS
                    }
                    16 => self.viewport_default(owner, state, a1, a2, a3)?,
                    _ => return Ok(None),
                }
            };
            Ok(Some(value))
        })();
        let cleanup = self.viewport_release(owner, state);
        match result {
            Ok(v) => {
                cleanup?;
                Ok(v)
            }
            Err(e) => {
                let _ = cleanup;
                Err(e)
            }
        }
    }
}
