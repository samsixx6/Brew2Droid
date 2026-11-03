//! CScrollbarWidget público: decoração de um filho e ScrollEvent, sem alias de IWidget/IContainer.
use super::*;
pub(super) const CLASS_ID: u32 = 0x0105_51dc;
const BAD: u32 = 14;
const OOM: u32 = 2;
const BYTES: u32 = 256;
const W: u32 = 0;
const H: u32 = 4;
const PARENT: u32 = 8;
const CHILD: u32 = 12;
const MODEL: u32 = 16;
const FLAGS: u32 = 20;
const STYLE: u32 = 24;
const HANDLE: u32 = 28;
const PAD: u32 = 32;
const END: u32 = 36;
const MIN_V: u32 = 40;
const MIN_H: u32 = 44;
const ACTIVE: u32 = 48;
const HANDLER: u32 = 52;
const CONTEXT: u32 = 56;
const FREE: u32 = 60;
const VR: u32 = 64;
const VV: u32 = 68;
const VP: u32 = 72;
const HR: u32 = 76;
const HV: u32 = 80;
const HP: u32 = 84;
const FILL: u32 = 88;
const COLOR_A: u32 = 92;
const COLOR_I: u32 = 96;
const HANDLE_A: u32 = 100;
const HANDLE_I: u32 = 104;
const BORDER_A: u32 = 108;
const BORDER_I: u32 = 112;
const BCOLOR_A: u32 = 116;
const BCOLOR_I: u32 = 120;
const BG_A: u32 = 124;
const BG_I: u32 = 128;
const SCROLL_BORDER: u32 = 132;
const LISTENER: u32 = 144;
const BUSY: u32 = 168;
const DESTROYING: u32 = 172;
const SCRATCH: u32 = 176;
const DRIVE: u32 = 212;
const RGB_NONE: u32 = u32::MAX;
#[derive(Clone, Copy)]
struct ScrollGeometry {
    child: Rect,
    bar: Rect,
    fill: Rect,
    thumb: Rect,
    before: Rect,
    after: Rect,
}
fn scroll_rect(x: i32, y: i32, w: u32, h: u32) -> Rect {
    Rect {
        x: x.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
        y: y.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
        width: w.min(i16::MAX as u32) as i16,
        height: h.min(i16::MAX as u32) as i16,
    }
}
impl<C: CpuBackend> Machine<C> {
    fn scrollbar_state(&self, iface: Interface, this: u32) -> Option<(u32, u32)> {
        if self.objects.kind_of(this) != Some(iface) {
            return None;
        }
        let owner = if iface == Interface::Scrollbar {
            this
        } else {
            self.cpu.read_u32(this + 4).ok()?
        };
        if self.objects.kind_of(owner) != Some(Interface::Scrollbar) {
            return None;
        }
        if iface == Interface::ScrollbarContainer && self.cpu.read_u32(owner + 8).ok() != Some(this)
        {
            return None;
        }
        let state = self.cpu.read_u32(owner + 4).ok()?;
        if self.heap.size_of(state).is_none_or(|n| n < BYTES) {
            return None;
        }
        Some((owner, state))
    }
    pub(super) fn create_scrollbar(&mut self, out: u32) -> Result<u32, CpuError> {
        if !self.modern_font_write(out, &0u32.to_le_bytes()) {
            return Ok(BAD);
        }
        let owner = self.new_object(Interface::Scrollbar)?;
        if owner == 0 {
            return Ok(OOM);
        }
        let facet = self.new_object(Interface::ScrollbarContainer)?;
        if facet == 0 {
            self.objects.release(owner);
            return Ok(OOM);
        }
        let state = self.malloc(BYTES)?;
        if state == 0 {
            self.objects.release(facet);
            self.objects.release(owner);
            return Ok(OOM);
        }
        let model = self.new_object(Interface::WindowViewModel)?;
        if model == 0 {
            self.heap.free(state);
            self.objects.release(facet);
            self.objects.release(owner);
            return Ok(OOM);
        }
        self.cpu.write_mem(owner + 4, &[0; 60])?;
        self.cpu.write_mem(facet + 4, &[0; 60])?;
        self.cpu.write_mem(model + 4, &[0; 60])?;
        self.cpu.write_mem(state, &[0; BYTES as usize])?;
        self.cpu.write_u32(owner + 4, state)?;
        self.cpu.write_u32(owner + 8, facet)?;
        self.cpu.write_u32(facet + 4, owner)?;
        for (field, value) in [
            (MODEL, model),
            (HANDLE, 3),
            (PAD, 2),
            (MIN_V, 4),
            (MIN_H, 4),
            (SCROLL_BORDER, 1),
            (FILL, 0x80808000),
            (HANDLE_A, 0xffffff00),
            (HANDLE_I, 0xffffff00),
            (BG_A, 0),
            (BG_I, 0),
            (BCOLOR_A, 0),
            (BCOLOR_I, 0),
            (HANDLER, aee::encode(Interface::Scrollbar, 16)),
            (CONTEXT, owner),
        ] {
            self.cpu.write_u32(state + field, value)?;
        }
        let result = self.scrollbar_listen(owner, state, model);
        match result {
            Ok(SUCCESS) => {}
            Ok(code) => {
                self.scrollbar_release(owner, state)?;
                return Ok(code);
            }
            Err(e) => {
                let _ = self.scrollbar_release(owner, state);
                return Err(e);
            }
        }
        self.cpu.write_u32(out, owner)?;
        Ok(SUCCESS)
    }
    fn scrollbar_unlisten(&mut self, state: u32) -> Result<(), CpuError> {
        let listener = state + LISTENER;
        let cancel = self.cpu.read_u32(listener + 16)?;
        if cancel != 0 {
            self.modern_ui_call(cancel, [listener, 0, 0, 0], &[])?;
        }
        Ok(())
    }
    fn scrollbar_listen(&mut self, owner: u32, state: u32, model: u32) -> Result<u32, CpuError> {
        let l = state + LISTENER;
        self.cpu.write_mem(l, &[0; 24])?;
        self.cpu
            .write_u32(l + 8, aee::encode(Interface::Scrollbar, 17))?;
        self.cpu.write_u32(l + 12, owner)?;
        self.modern_ui_invoke(model, 3, [l, 0, 0], &[])
    }
    fn scrollbar_release(&mut self, owner: u32, state: u32) -> Result<u32, CpuError> {
        if self.cpu.read_u32(state + DESTROYING)? != 0 {
            return Ok(0);
        }
        if self.objects.contagem(owner) != 1 {
            return Ok(self.objects.release(owner));
        }
        self.cpu.write_u32(state + DESTROYING, 1)?;
        let facet = self.cpu.read_u32(owner + 8)?;
        let child = self.cpu.read_u32(state + CHILD)?;
        let model = self.cpu.read_u32(state + MODEL)?;
        let drive = self.cpu.read_u32(state + DRIVE)?;
        let free = self.cpu.read_u32(state + FREE)?;
        let context = self.cpu.read_u32(state + CONTEXT)?;
        let mut error = self.scrollbar_unlisten(state).err();
        self.objects.release(owner);
        self.objects.release(facet);
        if child != 0 {
            if let Err(e) = self.modern_ui_invoke(child, 9, [0, 0, 0], &[]) {
                error.get_or_insert(e);
            }
            if let Err(e) = self.modern_font_base(child, 1) {
                error.get_or_insert(e);
            }
        }
        if let Err(e) = self.modern_font_base(model, 1) {
            error.get_or_insert(e);
        }
        if let Err(e) = self.modern_font_base(drive, 1) {
            error.get_or_insert(e);
        }

        if free != 0 {
            if let Err(e) = self.modern_ui_call(free, [context, 0, 0, 0], &[]) {
                error.get_or_insert(e);
            }
        }
        self.heap.free(state);
        match error {
            Some(e) => Err(e),
            None => Ok(0),
        }
    }
    fn scrollbar_invalidate(&mut self, owner: u32, state: u32) -> Result<(), CpuError> {
        let parent = self.cpu.read_u32(state + PARENT)?;
        if parent != 0 {
            self.modern_ui_invoke(parent, 3, [owner, 0, 3], &[])?;
        }
        Ok(())
    }
    fn scrollbar_geometry(&self, state: u32, vertical: bool) -> Result<ScrollGeometry, CpuError> {
        let w = self.cpu.read_u32(state + W)?;
        let h = self.cpu.read_u32(state + H)?;
        let flags = self.cpu.read_u32(state + FLAGS)?;
        let style = self.cpu.read_u32(state + STYLE)?;
        let has_child = self.cpu.read_u32(state + CHILD)? != 0;
        let b = self
            .cpu
            .read_u32(
                state
                    + if self.cpu.read_u32(state + ACTIVE)? != 0 {
                        BORDER_A
                    } else {
                        BORDER_I
                    },
            )?
            .min(w / 2)
            .min(h / 2);
        let iw = w.saturating_sub(b * 2);
        let ih = h.saturating_sub(b * 2);
        let sb = self.cpu.read_u32(state + SCROLL_BORDER)?;
        let thickness = self
            .cpu
            .read_u32(state + HANDLE)?
            .saturating_add(sb.saturating_mul(2));
        let pad = if has_child {
            self.cpu.read_u32(state + PAD)?
        } else {
            0
        };
        let sv = style != 1
            && (flags & 1 != 0
                || self.cpu.read_u32(state + VR)? > self.cpu.read_u32(state + VV)?);
        let sh = style != 0
            && (flags & 1 != 0
                || self.cpu.read_u32(state + HR)? > self.cpu.read_u32(state + HV)?);
        let cw = iw.saturating_sub(if sv && has_child {
            thickness.saturating_add(pad)
        } else {
            0
        });
        let ch = ih.saturating_sub(if sh && has_child {
            thickness.saturating_add(pad)
        } else {
            0
        });
        let child = scroll_rect(b as i32, b as i32, cw, ch);
        let shown = if vertical { sv } else { sh };
        let (cross, axis) = if vertical { (iw, ih) } else { (ih, iw) };
        let thick = thickness.min(cross);
        let length = axis.saturating_sub(if (if vertical { sh } else { sv }) && has_child {
            thickness.saturating_add(pad)
        } else {
            0
        });
        let cross_at = if has_child {
            cross.saturating_sub(thick)
        } else {
            match flags & if vertical { 0x70 } else { 0x700 } {
                0x10 | 0x100 => 0,
                0x20 | 0x200 => (cross - thick) / 2,
                _ => cross - thick,
            }
        };
        let (x, y) = if vertical {
            (b + cross_at, b)
        } else {
            (b, b + cross_at)
        };
        let bar = if shown {
            scroll_rect(
                x as i32,
                y as i32,
                if vertical { thick } else { length },
                if vertical { length } else { thick },
            )
        } else {
            scroll_rect(x as i32, y as i32, 0, 0)
        };
        let inset = sb.min(thick / 2).min(length / 2);
        let end = self.cpu.read_u32(state + END)?.min(length / 2);
        let arrow = if flags & 2 != 0 {
            thick.min(length.saturating_sub(inset * 2).saturating_sub(end * 2) / 2)
        } else {
            0
        };
        let start = inset.saturating_add(end).saturating_add(arrow).min(length);
        let usable = length.saturating_sub(start.saturating_mul(2));
        let (range, visible, position, minimum) = if vertical {
            (VR, VV, VP, MIN_V)
        } else {
            (HR, HV, HP, MIN_H)
        };
        let range = self.cpu.read_u32(state + range)?;
        let visible = self.cpu.read_u32(state + visible)?.min(range);
        let position = self
            .cpu
            .read_u32(state + position)?
            .min(range.saturating_sub(visible));
        let size = if range == 0 {
            usable
        } else {
            ((u64::from(usable) * u64::from(visible)) / u64::from(range)) as u32
        }
        .max(self.cpu.read_u32(state + minimum)?)
        .min(usable);
        let offset = if range <= visible {
            0
        } else {
            ((u64::from(usable - size) * u64::from(position)) / u64::from(range - visible)) as u32
        };
        let cross_size = thick.saturating_sub(inset * 2);
        let make = |at: u32, n: u32| {
            if vertical {
                scroll_rect((x + inset) as i32, (y + at) as i32, cross_size, n)
            } else {
                scroll_rect((x + at) as i32, (y + inset) as i32, n, cross_size)
            }
        };
        let fill = if shown {
            make(start, usable)
        } else {
            scroll_rect(0, 0, 0, 0)
        };
        let thumb = if shown {
            make(start + offset, size)
        } else {
            fill
        };
        let before = if shown {
            make(inset + end, arrow)
        } else {
            fill
        };
        let after = if shown {
            make(length.saturating_sub(inset + end + arrow), arrow)
        } else {
            fill
        };
        Ok(ScrollGeometry {
            child,
            bar,
            fill,
            thumb,
            before,
            after,
        })
    }
    fn scrollbar_layout(
        &mut self,
        owner: u32,
        state: u32,
        invalidate: bool,
    ) -> Result<(), CpuError> {
        if self.cpu.read_u32(state + BUSY)? != 0 {
            return Ok(());
        }
        self.cpu.write_u32(state + BUSY, 1)?;
        let result = (|| {
            let child = self.cpu.read_u32(state + CHILD)?;
            if child != 0 {
                let r = self.scrollbar_geometry(state, true)?.child;
                self.modern_ui_invoke(child, 6, [state + SCRATCH, 0, 0], &[])?;
                let actual = (
                    self.cpu.read_u32(state + SCRATCH)?,
                    self.cpu.read_u32(state + SCRATCH + 4)?,
                );
                let desired = (r.width.max(0) as u32, r.height.max(0) as u32);
                // SetExtent identico dispara notificacoes do viewport e recriava
                // um ciclo de Invalidate a cada pintura do RootContainer.
                if actual != desired {
                    self.cpu.write_u32(state + SCRATCH, desired.0)?;
                    self.cpu.write_u32(state + SCRATCH + 4, desired.1)?;
                    self.modern_ui_invoke(child, 7, [state + SCRATCH, 0, 0], &[])?;
                }
            }
            if invalidate {
                self.scrollbar_invalidate(owner, state)
            } else {
                Ok(())
            }
        })();
        self.cpu.write_u32(state + BUSY, 0)?;
        result
    }
    fn scrollbar_driving_model(&self, state: u32) -> Result<u32, CpuError> {
        let driving = self.cpu.read_u32(state + DRIVE)?;
        if driving != 0 {
            Ok(driving)
        } else {
            self.cpu.read_u32(state + MODEL)
        }
    }
    fn scrollbar_change_model(
        &mut self,
        owner: u32,
        state: u32,
        next: u32,
        driving: bool,
    ) -> Result<u32, CpuError> {
        if next == 0 && !driving {
            return Ok(BAD);
        }
        let mut acquired = 0;
        if next != 0 {
            if !self.modern_font_valid(next, 4) {
                return Ok(BAD);
            }
            let vt = self.cpu.read_u32(next)?;
            if !self.modern_font_valid(vt, 12) {
                return Ok(BAD);
            }
            self.cpu.write_u32(state + SCRATCH, 0)?;
            let rc = self.modern_ui_invoke(next, 2, [0x0101593a, state + SCRATCH, 0], &[])?;
            acquired = self.cpu.read_u32(state + SCRATCH)?;
            if rc != SUCCESS || acquired == 0 {
                return Ok(3);
            }
        }
        let old_model = self.cpu.read_u32(state + MODEL)?;
        let old_drive = self.cpu.read_u32(state + DRIVE)?;
        let old_effective = if old_drive == 0 { old_model } else { old_drive };
        // O dummy pertence ao scrollbar; o view model automático pertence ao filho.
        // Separá-los permite soltar o listener do filho sem alocar no detach.
        if driving && acquired == old_model {
            self.modern_font_base(acquired, 1)?;
            acquired = 0;
        }
        let (new_model, new_drive) = if driving {
            (old_model, acquired)
        } else {
            (acquired, 0)
        };
        if (new_model, new_drive) == (old_model, old_drive) {
            self.modern_font_base(acquired, 1)?;
            return Ok(SUCCESS);
        }
        let effective = if new_drive == 0 { new_model } else { new_drive };
        if let Err(error) = self.scrollbar_unlisten(state) {
            self.modern_font_base(acquired, 1)?;
            return Err(error);
        }
        let registration = self.scrollbar_listen(owner, state, effective);
        if !matches!(registration, Ok(SUCCESS)) {
            self.modern_font_base(acquired, 1)?;
            self.scrollbar_listen(owner, state, old_effective)?;
            return registration;
        }
        self.cpu.write_u32(state + MODEL, new_model)?;
        self.cpu.write_u32(state + DRIVE, new_drive)?;
        if !driving {
            self.modern_font_base(old_model, 1)?;
        }
        self.modern_font_base(old_drive, 1)?;
        if !driving {
            for field in [VR, VV, VP, HR, HV, HP] {
                self.cpu.write_u32(state + field, 0)?;
            }
        }
        self.scrollbar_layout(owner, state, true)?;
        Ok(SUCCESS)
    }
    fn scrollbar_set_model(&mut self, owner: u32, state: u32, next: u32) -> Result<u32, CpuError> {
        self.scrollbar_change_model(owner, state, next, false)
    }
    fn scrollbar_set_child(&mut self, owner: u32, state: u32, next: u32) -> Result<u32, CpuError> {
        let old = self.cpu.read_u32(state + CHILD)?;
        if next == old {
            return Ok(SUCCESS);
        }
        let facet = self.cpu.read_u32(owner + 8)?;
        let acquired = if next == 0 {
            0
        } else {
            if next == owner || next == facet {
                return Ok(BAD);
            }
            if !self.modern_font_valid(next, 4) {
                return Ok(BAD);
            }
            let vt = self.cpu.read_u32(next)?;
            if !self.modern_font_valid(vt, 12) {
                return Ok(BAD);
            }
            self.cpu.write_u32(state + SCRATCH, 0)?;
            let rc = self.modern_ui_invoke(next, 2, [0x01015952, state + SCRATCH, 0], &[])?;
            let p = self.cpu.read_u32(state + SCRATCH)?;
            if rc != 0 || p == 0 {
                return Ok(3);
            }
            self.cpu.write_u32(state + SCRATCH, 0)?;
            let result = self.modern_ui_invoke(p, 8, [state + SCRATCH, 0, 0], &[]);
            let parent = self.cpu.read_u32(state + SCRATCH)?;
            if parent != 0 {
                self.modern_font_base(parent, 1)?;
            }
            if result.is_err() || parent != 0 {
                self.modern_font_base(p, 1)?;
                result?;
                return Ok(BAD);
            }
            p
        };
        self.cpu.write_u32(state + CHILD, acquired)?;
        self.scrollbar_change_model(owner, state, 0, true)?;
        if acquired != 0 {
            self.modern_ui_invoke(acquired, 9, [facet, 0, 0], &[])?;
        }
        if old != 0 {
            self.modern_ui_invoke(old, 9, [0, 0, 0], &[])?;
            self.modern_font_base(old, 1)?;
        }
        if acquired != 0 {
            self.cpu.write_u32(state + SCRATCH, 0)?;
            if self.modern_ui_invoke(acquired, 3, [0x800, 0x161, state + SCRATCH], &[])? != 0 {
                let model = self.cpu.read_u32(state + SCRATCH)?;
                if model != 0 {
                    let result = self.scrollbar_change_model(owner, state, model, true);
                    self.modern_font_base(model, 1)?;
                    result?;
                }
            }
        }
        self.scrollbar_layout(owner, state, true)?;
        if acquired != 0 {
            self.modern_ui_invoke(acquired, 3, [0x801, 0x183, 0], &[])?;
        }
        Ok(SUCCESS)
    }
    fn scrollbar_changed(&mut self, owner: u32, state: u32, event: u32) -> Result<u32, CpuError> {
        if !self.modern_font_valid(event, 12) {
            return Ok(BAD);
        }
        let code = self.cpu.read_u32(event)?;
        if code == 0x1064 {
            if !self.modern_font_valid(event, 20) {
                return Ok(BAD);
            }
            let mut b = [0; 20];
            self.cpu.read_mem(event, &mut b)?;
            if b[12] > 1 {
                return Ok(BAD);
            }
            let vals = [14, 16, 18].map(|i| u16::from_le_bytes([b[i], b[i + 1]]) as u32);
            let fields = if b[12] != 0 {
                [VR, VV, VP]
            } else {
                [HR, HV, HP]
            };
            for (field, value) in fields.into_iter().zip([
                vals[0],
                vals[1].min(vals[0]),
                vals[2].min(vals[0].saturating_sub(vals[1].min(vals[0]))),
            ]) {
                self.cpu.write_u32(state + field, value)?;
            }
        } else if code == 0x1065 {
            if !self.modern_font_valid(event, 24) {
                return Ok(BAD);
            }
            let mut b = [0; 24];
            self.cpu.read_mem(event, &mut b)?;
            for (i, fields) in [[HR, HV, HP], [VR, VV, VP]].into_iter().enumerate() {
                let at = 12 + i * 6;
                let range = u16::from_le_bytes([b[at], b[at + 1]]) as u32;
                let vis = (u16::from_le_bytes([b[at + 2], b[at + 3]]) as u32).min(range);
                let pos = (u16::from_le_bytes([b[at + 4], b[at + 5]]) as u32).min(range - vis);
                for (field, value) in fields.into_iter().zip([range, vis, pos]) {
                    self.cpu.write_u32(state + field, value)?;
                }
            }
        } else {
            return Ok(SUCCESS);
        }
        self.scrollbar_layout(owner, state, true)?;
        Ok(SUCCESS)
    }
    fn scrollbar_property(
        &mut self,
        owner: u32,
        state: u32,
        set: bool,
        id: u32,
        value: u32,
    ) -> Result<Option<u32>, CpuError> {
        if id == 0x600 {
            if !self.modern_font_valid(value, 12) {
                return Ok(Some(FALSE));
            }
            let key = self.cpu.read_u32(value)?;
            let size = self.cpu.read_u32(value + 4)?;
            let p = self.cpu.read_u32(value + 8)?;
            if size != 4 || !self.modern_font_valid(p, 4) {
                return Ok(Some(FALSE));
            }
            let field = match key {
                0x01070b23 => MIN_H,
                0x01070b25 => MIN_V,
                0x0107264c => SCROLL_BORDER,
                _ => return Ok(Some(FALSE)),
            };
            if set {
                let v = self.cpu.read_u32(p)?;
                if v > i16::MAX as u32 {
                    return Ok(Some(FALSE));
                }
                self.cpu.write_u32(state + field, v)?;
                self.scrollbar_layout(owner, state, true)?;
            } else {
                let v = self.cpu.read_u32(state + field)?;
                if !self.modern_font_write(p, &v.to_le_bytes()) {
                    return Ok(Some(FALSE));
                }
            }
            return Ok(Some(TRUE));
        }
        if id == 0x161 {
            if set {
                return Ok(Some(
                    if self.scrollbar_change_model(owner, state, value, true)? == SUCCESS {
                        TRUE
                    } else {
                        FALSE
                    },
                ));
            }
            let model = self.scrollbar_driving_model(state)?;
            if !self.modern_font_write(value, &model.to_le_bytes()) {
                return Ok(Some(FALSE));
            }
            self.modern_font_base(model, 0)?;
            return Ok(Some(TRUE));
        }
        if (0x191..=0x199).contains(&id) {
            if set {
                return Ok(Some(FALSE));
            }
            let style = self.cpu.read_u32(state + STYLE)?;
            let axis = (id - 0x191) % 3;
            let vertical = match axis {
                0 if style != 2 => style == 0,
                1 if style != 1 => true,
                2 if style != 0 => false,
                _ => return Ok(Some(FALSE)),
            };
            let g = self.scrollbar_geometry(state, vertical)?;
            let r = match (id - 0x191) / 3 {
                0 => g.bar,
                1 => g.thumb,
                _ => g.fill,
            };
            let bytes = [r.x, r.y, r.width, r.height]
                .into_iter()
                .flat_map(i16::to_le_bytes)
                .collect::<Vec<_>>();
            return Ok(Some(self.modern_font_write(value, &bytes) as u32));
        }
        let fields: &[u32] = match id {
            0x180 => &[HANDLE],
            0x181 => &[PAD],
            0x182 => &[STYLE],
            0x226 => &[END],
            0x153 => &[FLAGS],
            0x184 => &[COLOR_A],
            0x185 => &[COLOR_I],
            0x186 => &[COLOR_A, COLOR_I],
            0x187 => &[HANDLE_A],
            0x188 => &[HANDLE_I],
            0x189 => &[HANDLE_A, HANDLE_I],
            0x190 => &[FILL],
            0x100 => &[BORDER_A, BORDER_I],
            0x101 => &[BORDER_A],
            0x102 => &[BORDER_I],
            0x110 => &[BCOLOR_A, BCOLOR_I],
            0x111 => &[BCOLOR_A],
            0x112 => &[BCOLOR_I],
            0x130 => &[BG_A, BG_I],
            0x131 => &[BG_A],
            0x132 => &[BG_I],
            _ => return Ok(None),
        };
        if set {
            if matches!(id, 0x180 | 0x181 | 0x226 | 0x100 | 0x101 | 0x102)
                && value > i16::MAX as u32
            {
                return Ok(Some(FALSE));
            }
            if id == 0x180 && value == 0 {
                return Ok(Some(FALSE));
            }
            if id == 0x182 && (value > 2 || (value == 2 && self.cpu.read_u32(state + CHILD)? == 0))
            {
                return Ok(Some(FALSE));
            }
            // FOCUSTRACK e imagens/flash personalizados não são anunciados sem comportamento.
            if id == 0x153 && value & !(3 | 0x770) != 0 {
                return Ok(Some(FALSE));
            }
            for field in fields {
                self.cpu.write_u32(state + *field, value)?;
            }
            self.scrollbar_layout(owner, state, true)?;
        } else {
            let v = self.cpu.read_u32(state + fields[0])?;
            if !self.modern_font_write(value, &v.to_le_bytes()) {
                return Ok(Some(FALSE));
            }
        }
        Ok(Some(TRUE))
    }
    fn scrollbar_event(
        &mut self,
        owner: u32,
        state: u32,
        event: u32,
        w: u32,
        dw: u32,
    ) -> Result<u32, CpuError> {
        let child = self.cpu.read_u32(state + CHILD)?;
        match event {
            0x800 | 0x801 => {
                if let Some(result) =
                    self.scrollbar_property(owner, state, event == 0x801, w, dw)?
                {
                    return Ok(result);
                }
                return Ok(FALSE);
            }
            0x700 => {
                self.cpu.write_u32(state + ACTIVE, (w != 0) as u32)?;
                self.scrollbar_layout(owner, state, true)?;
                if child == 0 {
                    return Ok(FALSE);
                }
            }
            0x701 | 0x702 => {
                if child == 0 {
                    return Ok(self.modern_font_write(dw, &[0]) as u32);
                }
            }
            _ => {}
        }
        if child == 0 {
            Ok(FALSE)
        } else {
            self.modern_ui_invoke(child, 3, [event, w, dw], &[])
        }
    }
    fn scrollbar_paint(
        &mut self,
        target: u32,
        rect: Rect,
        color: u32,
        x: i32,
        y: i32,
        outer: Rect,
    ) -> Result<(), CpuError> {
        if color == RGB_NONE || rect.width <= 0 || rect.height <= 0 {
            return Ok(());
        }
        let Some(surface) = self.bitmaps.get(&target) else {
            return Ok(());
        };
        let mut x0 = x.saturating_add(rect.x as i32).max(0).max(outer.x as i32);
        let mut y0 = y.saturating_add(rect.y as i32).max(0).max(outer.y as i32);
        let mut x1 = x
            .saturating_add(rect.x as i32)
            .saturating_add(rect.width as i32)
            .min(surface.width() as i32)
            .min(outer.x as i32 + outer.width as i32);
        let mut y1 = y
            .saturating_add(rect.y as i32)
            .saturating_add(rect.height as i32)
            .min(surface.height() as i32)
            .min(outer.y as i32 + outer.height as i32);
        if let Some(clip) = self.clip {
            x0 = x0.max(clip.x as i32);
            y0 = y0.max(clip.y as i32);
            x1 = x1.min(clip.x as i32 + clip.width as i32);
            y1 = y1.min(clip.y as i32 + clip.height as i32);
        }
        if x0 < x1 && y0 < y1 {
            self.bitmaps.get_mut(&target).unwrap().fill_rect_native(
                scroll_rect(x0, y0, (x1 - x0) as u32, (y1 - y0) as u32),
                Rgb::from_rgbval(color).to_rgb565(),
            );
        }
        Ok(())
    }
    fn scrollbar_paint_rgba(
        &mut self,
        target: u32,
        rect: Rect,
        color: u32,
        x: i32,
        y: i32,
        outer: Rect,
    ) -> Result<(), CpuError> {
        let alpha = color & 255;
        if alpha == 0 || rect.width <= 0 || rect.height <= 0 {
            return Ok(());
        }
        let Some(surface) = self.bitmaps.get(&target) else {
            return Ok(());
        };
        let mut x0 = x.saturating_add(rect.x as i32).max(0).max(outer.x as i32);
        let mut y0 = y.saturating_add(rect.y as i32).max(0).max(outer.y as i32);
        let mut x1 = x
            .saturating_add(rect.x as i32)
            .saturating_add(rect.width as i32)
            .min(surface.width() as i32)
            .min(outer.x as i32 + outer.width as i32);
        let mut y1 = y
            .saturating_add(rect.y as i32)
            .saturating_add(rect.height as i32)
            .min(surface.height() as i32)
            .min(outer.y as i32 + outer.height as i32);
        if let Some(c) = self.clip {
            x0 = x0.max(c.x as i32);
            y0 = y0.max(c.y as i32);
            x1 = x1.min(c.x as i32 + c.width as i32);
            y1 = y1.min(c.y as i32 + c.height as i32)
        }
        if x0 >= x1 || y0 >= y1 {
            return Ok(());
        }
        let rgb = Rgb::from_rgbval(color);
        let surface = self.bitmaps.get_mut(&target).unwrap();
        for row in y0..y1 {
            for column in x0..x1 {
                let old = Rgb::from_rgb565(surface.get_pixel(column, row));
                let mix = |a: u8, b: u8| {
                    ((u32::from(a) * alpha + u32::from(b) * (255 - alpha) + 127) / 255) as u8
                };
                surface.set_pixel(
                    column,
                    row,
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
    fn scrollbar_draw(
        &mut self,
        owner: u32,
        state: u32,
        canvas: u32,
        x: i32,
        y: i32,
    ) -> Result<u32, CpuError> {
        let Some(&target) = self.canvases.get(&canvas) else {
            return Ok(20);
        };
        if !self.bitmaps.contains_key(&target) {
            return Ok(20);
        }
        self.scrollbar_layout(owner, state, false)?;
        let w = self.cpu.read_u32(state + W)?;
        let h = self.cpu.read_u32(state + H)?;
        // Clip em coordenadas de host antes de converter ao AEERect i16.
        let surface = self.bitmaps.get(&target).unwrap();
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = x.saturating_add(w as i32).min(surface.width() as i32);
        let y1 = y.saturating_add(h as i32).min(surface.height() as i32);
        if x1 <= x0 || y1 <= y0 {
            return Ok(SUCCESS);
        }
        let outer = scroll_rect(x0, y0, (x1 - x0) as u32, (y1 - y0) as u32);
        let active = self.cpu.read_u32(state + ACTIVE)? != 0;
        let color = self
            .cpu
            .read_u32(state + if active { COLOR_A } else { COLOR_I })?;
        let handle = self
            .cpu
            .read_u32(state + if active { HANDLE_A } else { HANDLE_I })?;
        let fill = self.cpu.read_u32(state + FILL)?;
        let background = self
            .cpu
            .read_u32(state + if active { BG_A } else { BG_I })?;
        self.scrollbar_paint_rgba(target, scroll_rect(0, 0, w, h), background, x, y, outer)?;
        let border = self
            .cpu
            .read_u32(state + if active { BORDER_A } else { BORDER_I })?
            .min(w / 2)
            .min(h / 2);
        let bc = self
            .cpu
            .read_u32(state + if active { BCOLOR_A } else { BCOLOR_I })?;
        for r in [
            scroll_rect(0, 0, w, border),
            scroll_rect(0, h.saturating_sub(border) as i32, w, border),
            scroll_rect(0, 0, border, h),
            scroll_rect(w.saturating_sub(border) as i32, 0, border, h),
        ] {
            self.scrollbar_paint_rgba(target, r, bc, x, y, outer)?;
        }
        let child = self.cpu.read_u32(state + CHILD)?;
        if child != 0 {
            let r = self.scrollbar_geometry(state, true)?.child;
            let previous = self.clip;
            let mut cx0 = (x + r.x as i32).max(outer.x as i32);
            let mut cy0 = (y + r.y as i32).max(outer.y as i32);
            let mut cx1 =
                (x + r.x as i32 + r.width as i32).min(outer.x as i32 + outer.width as i32);
            let mut cy1 =
                (y + r.y as i32 + r.height as i32).min(outer.y as i32 + outer.height as i32);
            if let Some(c) = previous {
                cx0 = cx0.max(c.x as i32);
                cy0 = cy0.max(c.y as i32);
                cx1 = cx1.min(c.x as i32 + c.width as i32);
                cy1 = cy1.min(c.y as i32 + c.height as i32)
            }
            if cx0 < cx1 && cy0 < cy1 {
                self.clip = Some(scroll_rect(
                    cx0,
                    cy0,
                    (cx1 - cx0) as u32,
                    (cy1 - cy0) as u32,
                ));
                let result = self.modern_ui_invoke(
                    child,
                    10,
                    [
                        canvas,
                        x.saturating_add(r.x as i32) as u32,
                        y.saturating_add(r.y as i32) as u32,
                    ],
                    &[],
                );
                self.clip = previous;
                result?;
            }
        }
        for vertical in [true, false] {
            let g = self.scrollbar_geometry(state, vertical)?;
            self.scrollbar_paint(target, g.bar, color, x, y, outer)?;
            self.scrollbar_paint(target, g.fill, fill, x, y, outer)?;
            self.scrollbar_paint(target, g.thumb, handle, x, y, outer)?;
            // Os indicadores padrão são triângulos nativos; recursos de setas externos retornam não suportado.
            for (r, reverse) in [(g.before, false), (g.after, true)] {
                let n = if vertical { r.height } else { r.width }.max(0) as u32;
                let cross = if vertical { r.width } else { r.height }.max(0) as u32;
                for i in 0..n {
                    let row = if reverse { n - 1 - i } else { i };
                    let span =
                        ((u64::from(i + 1) * u64::from(cross)) / u64::from(n.max(1))).max(1) as u32;
                    let offset = (cross - span) / 2;
                    let line = if vertical {
                        scroll_rect(r.x as i32 + offset as i32, r.y as i32 + row as i32, span, 1)
                    } else {
                        scroll_rect(r.x as i32 + row as i32, r.y as i32 + offset as i32, 1, span)
                    };
                    self.scrollbar_paint(target, line, color, x, y, outer)?;
                }
            }
        }
        Ok(SUCCESS)
    }
    pub(super) fn scrollbar_call(
        &mut self,
        iface: Interface,
        slot: u32,
    ) -> Result<Option<u32>, CpuError> {
        let this = self.arg(0);
        let Some((owner, state)) = self.scrollbar_state(iface, this) else {
            return Ok(Some(BAD));
        };
        if self.cpu.read_u32(state + DESTROYING)? != 0 {
            return Ok(Some(BAD));
        }
        if slot == 0 {
            return Ok(Some(self.objects.add_ref(owner)));
        }
        if slot == 1 {
            return Ok(Some(self.scrollbar_release(owner, state)?));
        }
        self.objects.add_ref(owner);
        let result = self.scrollbar_call_inner(iface, slot, owner, state);
        let released = self.scrollbar_release(owner, state);
        match result {
            Err(e) => {
                let _ = released;
                Err(e)
            }
            Ok(v) => {
                released?;
                Ok(v)
            }
        }
    }
    fn scrollbar_call_inner(
        &mut self,
        iface: Interface,
        slot: u32,
        owner: u32,
        state: u32,
    ) -> Result<Option<u32>, CpuError> {
        let (a1, a2, a3) = (self.arg(1), self.arg(2), self.arg(3));
        let facet = self.cpu.read_u32(owner + 8)?;
        if slot == 2 {
            if !self.modern_font_write(a2, &0u32.to_le_bytes()) {
                return Ok(Some(BAD));
            }
            let target = match a1 {
                0x01000001 | 0x01015934 | 0x01015952 | 0x01015956 => owner,
                0x01015932 => facet,
                _ => return Ok(Some(3)),
            };
            self.cpu.write_u32(a2, target)?;
            self.objects.add_ref(owner);
            return Ok(Some(SUCCESS));
        }
        if iface == Interface::ScrollbarContainer {
            return Ok(Some(match slot {
                3 => {
                    if a1 != 0 && a1 != self.cpu.read_u32(state + CHILD)? {
                        BAD
                    } else {
                        self.scrollbar_layout(owner, state, true)?;
                        SUCCESS
                    }
                }
                4 => {
                    let child = self.cpu.read_u32(state + CHILD)?;
                    if a1 != owner && a1 != 0 && a1 != child {
                        return Ok(Some(BAD));
                    }
                    if (a2 != 0 && !self.modern_font_valid(a2, 4))
                        || (a3 != 0 && !self.modern_font_valid(a3, 8))
                    {
                        return Ok(Some(BAD));
                    }
                    let mut r = if a1 == child && child != 0 {
                        self.scrollbar_geometry(state, true)?.child
                    } else {
                        scroll_rect(
                            0,
                            0,
                            self.cpu.read_u32(state + W)?,
                            self.cpu.read_u32(state + H)?,
                        )
                    };
                    let parent = self.cpu.read_u32(state + PARENT)?;
                    if parent != 0 {
                        let rc =
                            self.modern_ui_invoke(parent, 4, [owner, a2, state + SCRATCH], &[])?;
                        if rc != 0 {
                            return Ok(Some(rc));
                        }
                        let mut b = [0; 8];
                        self.cpu.read_mem(state + SCRATCH, &mut b)?;
                        let p = Rect::from_bytes(b);
                        r.x = (r.x as i32 + p.x as i32).clamp(i16::MIN as i32, i16::MAX as i32)
                            as i16;
                        r.y = (r.y as i32 + p.y as i32).clamp(i16::MIN as i32, i16::MAX as i32)
                            as i16;
                    } else if a2 != 0 {
                        self.cpu.write_u32(a2, facet)?;
                        self.objects.add_ref(owner);
                    }
                    if a3 != 0 {
                        self.cpu.write_mem(
                            a3,
                            &[r.x, r.y, r.width, r.height]
                                .into_iter()
                                .flat_map(i16::to_le_bytes)
                                .collect::<Vec<_>>(),
                        )?;
                    }
                    SUCCESS
                }
                5 => {
                    if self.cpu.read_u32(state + CHILD)? != 0 {
                        1
                    } else {
                        self.scrollbar_set_child(owner, state, a1)?
                    }
                }
                6 => {
                    if a1 == 0 || a1 != self.cpu.read_u32(state + CHILD)? {
                        BAD
                    } else {
                        self.scrollbar_set_child(owner, state, 0)?
                    }
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
            }));
        }
        let result = match slot {
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
                let next = [
                    self.cpu.read_u32(a1)?,
                    self.cpu.read_u32(a1 + 4)?,
                    self.cpu.read_u32(a1 + 8)?,
                ];
                for (i, field) in [HANDLER, CONTEXT, FREE].into_iter().enumerate() {
                    let old = self.cpu.read_u32(state + field)?;
                    self.cpu.write_u32(state + field, next[i])?;
                    self.cpu.write_u32(a1 + i as u32 * 4, old)?;
                }
                SUCCESS
            }
            5 | 6 => {
                if !self.modern_font_valid(a1, 8) {
                    return Ok(Some(BAD));
                }
                let (w, h) = if slot == 6 {
                    (self.cpu.read_u32(state + W)?, self.cpu.read_u32(state + H)?)
                } else {
                    let child = self.cpu.read_u32(state + CHILD)?;
                    let mut cw = 0;
                    let mut ch = 0;
                    if child != 0 {
                        self.cpu.write_mem(state + SCRATCH, &[0; 8])?;
                        self.modern_ui_invoke(child, 5, [state + SCRATCH, 0, 0], &[])?;
                        cw = self.cpu.read_u32(state + SCRATCH)?.min(i16::MAX as u32);
                        ch = self.cpu.read_u32(state + SCRATCH + 4)?.min(i16::MAX as u32);
                    }
                    let thick = self.cpu.read_u32(state + HANDLE)?.saturating_add(
                        self.cpu.read_u32(state + SCROLL_BORDER)?.saturating_mul(2),
                    );
                    let pad = if child == 0 {
                        0
                    } else {
                        self.cpu.read_u32(state + PAD)?
                    };
                    match self.cpu.read_u32(state + STYLE)? {
                        0 => (
                            cw.saturating_add(thick + pad),
                            ch.max(self.cpu.read_u32(state + MIN_V)?),
                        ),
                        1 => (
                            cw.max(self.cpu.read_u32(state + MIN_H)?),
                            ch.saturating_add(thick + pad),
                        ),
                        _ => (
                            cw.saturating_add(thick + pad),
                            ch.saturating_add(thick + pad),
                        ),
                    }
                };
                self.cpu.write_u32(a1, w.min(i16::MAX as u32))?;
                self.cpu.write_u32(a1 + 4, h.min(i16::MAX as u32))?;
                SUCCESS
            }
            7 => {
                if !self.modern_font_valid(a1, 8) {
                    return Ok(Some(BAD));
                }
                let w = self.cpu.read_u32(a1)?;
                let h = self.cpu.read_u32(a1 + 4)?;
                if w > i16::MAX as u32 || h > i16::MAX as u32 {
                    return Ok(Some(BAD));
                }
                self.cpu.write_u32(state + W, w)?;
                self.cpu.write_u32(state + H, h)?;
                self.scrollbar_layout(owner, state, true)?;
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
                if a1 == owner || a1 == facet {
                    return Ok(Some(BAD));
                }
                self.cpu.write_u32(state + PARENT, a1)?;
                SUCCESS
            }
            10 => self.scrollbar_draw(owner, state, a1, a2 as i32, a3 as i32)?,
            11 => FALSE,
            12 => {
                if !self.modern_font_write(a2, &0u32.to_le_bytes()) {
                    return Ok(Some(BAD));
                }
                if a1 != 0x0101593a {
                    return Ok(Some(3));
                }
                let model = self.cpu.read_u32(state + MODEL)?;
                self.modern_font_base(model, 0)?;
                self.cpu.write_u32(a2, model)?;
                SUCCESS
            }
            13 => self.scrollbar_set_model(owner, state, a1)?,
            14 => self.scrollbar_set_child(owner, state, a1)?,
            15 => {
                let child = self.cpu.read_u32(state + CHILD)?;
                if !self.modern_font_write(a1, &child.to_le_bytes()) {
                    return Ok(Some(BAD));
                }
                self.modern_font_base(child, 0)?;
                SUCCESS
            }
            16 => self.scrollbar_event(owner, state, a1, a2, a3)?,
            17 => self.scrollbar_changed(owner, state, a1)?,
            _ => return Ok(None),
        };
        Ok(Some(result))
    }
}
