//! RootContainer: pai real, canvas tipado e ciclos de pintura pedidos por Invalidate.
use super::*;
pub(super) const CLASS_ID: u32 = 0x0105_5241;
const BAD: u32 = 14;
const BYTES: u32 = 160;
const INNER: u32 = 0;
const INNER_W: u32 = 4;
const CANVAS: u32 = 8;
const REGION: u32 = 12;
const PARENT: u32 = 20;
const HANDLER: u32 = 24;
const CONTEXT: u32 = 28;
const FREE: u32 = 32;
const DIRTY: u32 = 36;
const PENDING: u32 = 40;
const PAINTING: u32 = 44;
const BG_A: u32 = 48;
const BG_I: u32 = 52;
const BG_SA: u32 = 56;
const BG_SI: u32 = 60;
const FOCUSED: u32 = 64;
const SELECTED: u32 = 68;
const DAMAGE: u32 = 72;
const CALLBACK: u32 = 80;
const VIEW: u32 = 112;
const CAPTURE: u32 = 116;
const TOUCH: u32 = 120;
const LAYOUT: u32 = 124;
impl<C: CpuBackend> Machine<C> {
    fn root_state(&self, iface: Interface, p: u32) -> Option<(u32, u32)> {
        if self.objects.kind_of(p) != Some(iface) {
            return None;
        }
        let owner = if iface == Interface::RootContainer {
            p
        } else {
            self.cpu.read_u32(p + 4).ok()?
        };
        if self.objects.kind_of(owner) != Some(Interface::RootContainer) {
            return None;
        }
        let s = self.cpu.read_u32(owner + 4).ok()?;
        self.heap
            .size_of(s)
            .filter(|n| *n >= BYTES)
            .map(|_| (owner, s))
    }
    pub(super) fn modern_ui_canvas<T>(
        &mut self,
        canvas: u32,
        f: impl FnOnce(&mut Self) -> Result<T, CpuError>,
    ) -> Result<T, CpuError> {
        let pin = self.objects.kind_of(canvas) == Some(Interface::Canvas);
        if pin {
            self.objects.add_ref(canvas);
        }
        let snapshot = self.display_canvas_snapshot(canvas);
        let result = match snapshot {
            Err(e) => Err(e),
            Ok(None) => f(self),
            Ok(Some((bitmap, clip))) => {
                let old_target = if bitmap != 0 {
                    self.canvases.insert(canvas, bitmap)
                } else {
                    self.canvases.remove(&canvas)
                };
                let old_clip = self.clip;
                self.clip = Some(Self::root_intersection(old_clip.unwrap_or(clip), clip));
                let result = f(self);
                self.clip = old_clip;
                if let Some(old) = old_target {
                    self.canvases.insert(canvas, old);
                } else {
                    self.canvases.remove(&canvas);
                }
                self.modern_font_base(bitmap, 1)?;
                result
            }
        };
        if pin {
            self.modern_font_base(canvas, 1)?;
        }
        result
    }
    fn root_intersection(a: Rect, b: Rect) -> Rect {
        let l = i32::from(a.x).max(i32::from(b.x));
        let t = i32::from(a.y).max(i32::from(b.y));
        let r = (i32::from(a.x) + i32::from(a.width)).min(i32::from(b.x) + i32::from(b.width));
        let d = (i32::from(a.y) + i32::from(a.height)).min(i32::from(b.y) + i32::from(b.height));
        Rect {
            x: l.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
            y: t.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
            width: (r - l).clamp(0, i16::MAX as i32) as i16,
            height: (d - t).clamp(0, i16::MAX as i32) as i16,
        }
    }
    fn root_rect(&self, p: u32) -> Result<Rect, CpuError> {
        Ok(self.read_rect(p)?.unwrap())
    }
    fn root_write_rect(&mut self, p: u32, r: Rect) -> Result<(), CpuError> {
        self.cpu.write_mem(
            p,
            &[r.x, r.y, r.width, r.height]
                .into_iter()
                .flat_map(i16::to_le_bytes)
                .collect::<Vec<_>>(),
        )
    }
    fn root_notify(
        &mut self,
        s: u32,
        event: u32,
        rect: Option<Rect>,
        extent: bool,
    ) -> Result<(), CpuError> {
        let model = self.cpu.read_u32(s + VIEW)?;
        self.modern_font_base(model, 0)?;
        let result = self.modern_ui_scratch(24, |m, p| {
            m.cpu.write_u32(p, event)?;
            m.cpu.write_u32(p + 4, model)?;
            m.cpu
                .write_u32(p + 8, if rect.is_some() { p + 12 } else { 0 })?;
            if let Some(r) = rect {
                if extent {
                    m.cpu.write_u32(p + 12, r.width.max(0) as u32)?;
                    m.cpu.write_u32(p + 16, r.height.max(0) as u32)?;
                } else {
                    m.root_write_rect(p + 12, r)?;
                }
            }
            m.modern_ui_invoke(model, 4, [p, 0, 0], &[])?;
            Ok(())
        });
        self.modern_font_base(model, 1)?;
        result
    }
    pub(super) fn create_root_container(&mut self, out: u32) -> Result<u32, CpuError> {
        if !self.modern_font_write(out, &0u32.to_le_bytes()) {
            return Ok(BAD);
        }
        let owner = self.new_object(Interface::RootContainer)?;
        if owner == 0 {
            return Ok(2);
        }
        let facet = self.new_object(Interface::RootContainerWidget)?;
        if facet == 0 {
            self.objects.release(owner);
            return Ok(2);
        }
        let s = self.malloc(BYTES)?;
        if s == 0 {
            self.objects.release(facet);
            self.objects.release(owner);
            return Ok(2);
        }
        self.cpu.write_mem(owner + 4, &[0; 60])?;
        self.cpu.write_mem(facet + 4, &[0; 60])?;
        self.cpu.write_mem(s, &[0; BYTES as usize])?;
        self.cpu.write_u32(owner + 4, s)?;
        self.cpu.write_u32(owner + 8, facet)?;
        self.cpu.write_u32(facet + 4, owner)?;
        let rc = self.create_xy_container(s + INNER)?;
        if rc != 0 {
            self.heap.free(s);
            self.objects.release(facet);
            self.objects.release(owner);
            return Ok(rc);
        }
        let inner = self.cpu.read_u32(s + INNER)?;
        let rc = self.modern_ui_invoke(inner, 2, [0x01015952, s + INNER_W, 0], &[])?;
        if rc != 0 {
            self.modern_font_base(inner, 1)?;
            self.heap.free(s);
            self.objects.release(facet);
            self.objects.release(owner);
            return Ok(rc);
        }
        let model = self.new_object(Interface::WindowViewModel)?;
        if model == 0 {
            self.modern_font_base(self.cpu.read_u32(s + INNER_W)?, 1)?;
            self.modern_font_base(inner, 1)?;
            self.heap.free(s);
            self.objects.release(facet);
            self.objects.release(owner);
            return Ok(2);
        }
        self.cpu.write_mem(model + 4, &[0; 60])?;
        for (f, v) in [
            (VIEW, model),
            (BG_A, u32::MAX),
            (BG_I, u32::MAX),
            (BG_SA, u32::MAX),
            (BG_SI, u32::MAX),
            (HANDLER, aee::encode(Interface::RootContainerWidget, 14)),
            (CONTEXT, owner),
            (LAYOUT, 1),
            (CALLBACK + 16, aee::encode(Interface::RootContainer, 12)),
            (CALLBACK + 20, owner),
        ] {
            self.cpu.write_u32(s + f, v)?;
        }
        self.cpu.write_u32(out, owner)?;
        Ok(0)
    }
    fn root_children(&mut self, s: u32) -> Result<Vec<u32>, CpuError> {
        let inner = self.cpu.read_u32(s + INNER)?;
        let mut out = Vec::new();
        let mut p = self.modern_ui_invoke(inner, 7, [0, 1, 0], &[0])?;
        while p != 0 {
            if out.len() >= 1024 || out.contains(&p) {
                return Err(CpuError("filhos de RootContainer corrompidos".into()));
            }
            out.push(p);
            p = self.modern_ui_invoke(inner, 7, [p, 1, 0], &[0])?;
        }
        Ok(out)
    }
    fn root_extent(&self, s: u32) -> Result<Rect, CpuError> {
        let r = self.root_rect(s + REGION)?;
        Ok(Rect {
            x: 0,
            y: 0,
            width: r.width,
            height: r.height,
        })
    }
    fn root_mark(&mut self, owner: u32, s: u32, rect: Rect, flags: u32) -> Result<(), CpuError> {
        let rect = Self::root_intersection(rect, self.root_extent(s)?);
        if rect.width <= 0 || rect.height <= 0 {
            return Ok(());
        }
        let damage = if self.cpu.read_u32(s + DIRTY)? != 0 {
            let old = self.root_rect(s + DAMAGE)?;
            let l = old.x.min(rect.x);
            let t = old.y.min(rect.y);
            let r = (i32::from(old.x) + i32::from(old.width))
                .max(i32::from(rect.x) + i32::from(rect.width));
            let b = (i32::from(old.y) + i32::from(old.height))
                .max(i32::from(rect.y) + i32::from(rect.height));
            Rect {
                x: l,
                y: t,
                width: (r - i32::from(l)).min(i16::MAX as i32) as i16,
                height: (b - i32::from(t)).min(i16::MAX as i32) as i16,
            }
        } else {
            rect
        };
        self.root_write_rect(s + DAMAGE, damage)?;
        self.cpu.write_u32(s + DIRTY, 1)?;
        self.root_notify(s, 0x100e, Some(rect), false)?;
        let parent = self.cpu.read_u32(s + PARENT)?;
        if parent != 0 {
            let widget = self.cpu.read_u32(owner + 8)?;
            self.modern_ui_scratch(8, |m, p| {
                m.root_write_rect(p, rect)?;
                m.modern_ui_invoke(parent, 3, [widget, p, flags], &[])?;
                Ok(())
            })?;
            return Ok(());
        }
        if flags & 4 != 0 || self.cpu.read_u32(s + LAYOUT)? == 0 {
            return Ok(());
        }
        if flags & 8 != 0 && self.cpu.read_u32(s + PAINTING)? == 0 {
            return self.root_paint_pending(owner, s);
        }
        if self.cpu.read_u32(s + PENDING)? == 0 {
            self.cpu.write_u32(s + PENDING, 1)?;
            self.schedule_callback(s + CALLBACK, owner)?;
        }
        Ok(())
    }
    fn root_paint_pending(&mut self, owner: u32, s: u32) -> Result<(), CpuError> {
        self.cpu.write_u32(s + PENDING, 0)?;
        if self.cpu.read_u32(s + DIRTY)? == 0
            || self.cpu.read_u32(s + LAYOUT)? == 0
            || self.cpu.read_u32(s + PARENT)? != 0
        {
            return Ok(());
        }
        let canvas = self.cpu.read_u32(s + CANVAS)?;
        if canvas == 0 {
            return Ok(());
        }
        let rect = self.root_rect(s + REGION)?;
        let damage = self.root_rect(s + DAMAGE)?;
        self.cpu.write_u32(s + DIRTY, 0)?;
        self.cpu.write_u32(s + PAINTING, 1)?;
        let result = self.root_render(s, canvas, rect.x as i32, rect.y as i32, damage);
        self.cpu.write_u32(s + PAINTING, 0)?;
        let _ = owner;
        result.map(|_| ())
    }
    fn root_render(
        &mut self,
        s: u32,
        canvas: u32,
        x: i32,
        y: i32,
        damage: Rect,
    ) -> Result<u32, CpuError> {
        self.modern_font_base(canvas, 0)?;
        let result = self.modern_ui_scratch(24, |m, p| {
            let rc = m.modern_ui_invoke(canvas, 3, [p, 0, 0], &[])?;
            let bitmap = m.cpu.read_u32(p)?;
            if rc != 0 || bitmap == 0 {
                return Ok(if rc != 0 { rc } else { 1 });
            }
            let work = (|| {
                let rc = m.modern_ui_invoke(canvas, 5, [p + 8, 0, 0], &[])?;
                if rc != 0 {
                    return Ok(rc);
                }
                let old_canvas_clip = m.root_rect(p + 8)?;
                let original_clip = m.clip;
                let shifted = Rect {
                    x: x.saturating_add(damage.x as i32)
                        .clamp(i16::MIN as i32, i16::MAX as i32) as i16,
                    y: y.saturating_add(damage.y as i32)
                        .clamp(i16::MIN as i32, i16::MAX as i32) as i16,
                    width: damage.width,
                    height: damage.height,
                };
                let mut clip = Self::root_intersection(old_canvas_clip, shifted);
                if let Some(old) = original_clip {
                    clip = Self::root_intersection(clip, old)
                }
                if clip.width <= 0 || clip.height <= 0 {
                    return Ok(0);
                }
                m.root_write_rect(p + 16, clip)?;
                let rc = m.modern_ui_invoke(canvas, 6, [p + 16, 0, 0], &[])?;
                if rc != 0 {
                    return Ok(rc);
                }
                m.clip = Some(clip);
                let prior = m.canvases.insert(canvas, bitmap);
                let draw = (|| {
                    let active = m.cpu.read_u32(s + FOCUSED)? != 0;
                    let selected = m.cpu.read_u32(s + SELECTED)? != 0;
                    let bg = m.cpu.read_u32(
                        s + match (active, selected) {
                            (true, false) => BG_A,
                            (false, false) => BG_I,
                            (true, true) => BG_SA,
                            (false, true) => BG_SI,
                        },
                    )?;
                    m.modern_widget_fill(bitmap, clip, bg)?;
                    let inner = m.cpu.read_u32(s + INNER_W)?;
                    m.modern_ui_invoke(inner, 10, [canvas, x as u32, y as u32], &[])?;
                    Ok(0)
                })();
                m.clip = original_clip;
                if let Some(old) = prior {
                    m.canvases.insert(canvas, old);
                } else {
                    m.canvases.remove(&canvas);
                }
                let restore = m.modern_ui_invoke(canvas, 6, [p + 8, 0, 0], &[]);
                restore?;
                if draw.is_ok() {
                    m.root_notify(s, 0x1015, Some(shifted), false)?;
                }
                draw
            })();
            m.modern_font_base(bitmap, 1)?;
            work
        });
        self.modern_font_base(canvas, 1)?;
        result
    }
    fn root_set_extent(&mut self, owner: u32, s: u32, w: u32, h: u32) -> Result<u32, CpuError> {
        if w > i16::MAX as u32 || h > i16::MAX as u32 {
            return Ok(BAD);
        }
        let old = self.root_rect(s + REGION)?;
        if old.width == w as i16 && old.height == h as i16 {
            return Ok(0);
        }
        let next = Rect {
            width: w as i16,
            height: h as i16,
            ..old
        };
        self.root_write_rect(s + REGION, next)?;
        let inner = self.cpu.read_u32(s + INNER_W)?;
        self.modern_ui_scratch(8, |m, p| {
            m.cpu.write_u32(p, w)?;
            m.cpu.write_u32(p + 4, h)?;
            m.modern_ui_invoke(inner, 7, [p, 0, 0], &[])?;
            Ok(())
        })?;
        self.root_notify(s, 0x100c, Some(next), true)?;
        self.root_mark(owner, s, self.root_extent(s)?, 3)?;
        Ok(0)
    }
    fn root_can_insert(&mut self, owner: u32, s: u32, widget: u32) -> Result<bool, CpuError> {
        if !self.modern_font_valid(widget, 4) {
            return Ok(false);
        }
        let own_widget = self.cpu.read_u32(owner + 8)?;
        if widget == own_widget {
            return Ok(false);
        }
        let mut current = own_widget;
        let mut held = false;
        let mut visited = Vec::new();
        for _ in 0..64 {
            if current == widget || visited.contains(&current) {
                if held {
                    self.modern_font_base(current, 1)?;
                }
                return Ok(false);
            }
            visited.push(current);
            let next = self.modern_ui_scratch(8, |m, p| {
                m.modern_ui_invoke(current, 8, [p, 0, 0], &[])?;
                let parent = m.cpu.read_u32(p)?;
                if parent == 0 {
                    return Ok(0);
                }
                m.cpu.write_u32(p, 0)?;
                let rc = m.modern_ui_invoke(parent, 2, [0x01015952, p, 0], &[])?;
                let next = if rc == 0 { m.cpu.read_u32(p)? } else { 0 };
                m.modern_font_base(parent, 1)?;
                Ok(next)
            })?;
            if held {
                self.modern_font_base(current, 1)?;
            }
            current = next;
            held = current != 0;
            if current == 0 {
                return Ok(true);
            }
        }
        if held {
            self.modern_font_base(current, 1)?;
        }
        let _ = s;
        Ok(false)
    }
    fn root_invalidate(
        &mut self,
        owner: u32,
        s: u32,
        widget: u32,
        pRect: u32,
        flags: u32,
    ) -> Result<u32, CpuError> {
        if flags & !63 != 0 || (pRect != 0 && !self.modern_font_valid(pRect, 8)) {
            return Ok(BAD);
        }
        let inner = self.cpu.read_u32(s + INNER)?;
        let target = if matches!(widget, 1 | u32::MAX) {
            self.modern_ui_invoke(inner, 7, [0, (widget == u32::MAX) as u32, 0], &[0])?
        } else {
            widget
        };
        let mut rect = if pRect != 0 {
            self.root_rect(pRect)?
        } else if target == 0 {
            self.root_extent(s)?
        } else {
            self.modern_ui_scratch(8, |m, p| {
                m.modern_ui_invoke(target, 6, [p, 0, 0], &[])?;
                Ok(Rect {
                    x: 0,
                    y: 0,
                    width: m.cpu.read_u32(p)?.min(i16::MAX as u32) as i16,
                    height: m.cpu.read_u32(p + 4)?.min(i16::MAX as u32) as i16,
                })
            })?
        };
        if target != 0 {
            if !self.root_children(s)?.contains(&target) {
                return Ok(BAD);
            }
            let offset = self.modern_ui_scratch(12, |m, p| {
                let rc = m.modern_ui_invoke(inner, 9, [target, p, 0], &[])?;
                Ok((rc, m.cpu.read_u32(p)? as i32, m.cpu.read_u32(p + 4)? as i32))
            })?;
            if offset.0 != 0 {
                return Ok(offset.0);
            }
            let rx = i32::from(rect.x).saturating_add(offset.1);
            let ry = i32::from(rect.y).saturating_add(offset.2);
            if i16::try_from(rx).is_err() || i16::try_from(ry).is_err() {
                return Ok(BAD);
            }
            rect.x = rx as i16;
            rect.y = ry as i16;
        }
        self.root_mark(owner, s, rect, flags)?;
        Ok(0)
    }
    // PROPEX_SCREEN é absoluto mesmo quando este RootContainer é filho de outro.
    // Locate conserva a transformação pública dos containers intermediários.
    fn root_screen_origin(&mut self, owner: u32, state: u32) -> Result<Option<(i16, i16)>, CpuError> {
        let mut held = Vec::new();
        let result = (|| {
            let (mut current, mut state) = (owner, state);
            let (mut x, mut y) = (0i32, 0i32);
            let mut visited = Vec::new();
            for _ in 0..64 {
                if visited.contains(&current) { return Ok(None); }
                visited.push(current);
                let parent = self.cpu.read_u32(state + PARENT)?;
                if parent == 0 {
                    let origin = self.root_rect(state + REGION)?;
                    return Ok(i16::try_from(x + i32::from(origin.x)).ok()
                        .zip(i16::try_from(y + i32::from(origin.y)).ok()));
                }
                self.modern_font_base(parent, 0)?;
                held.push(parent);
                let widget = self.cpu.read_u32(current + 8)?;
                let (status, root, position) = self.modern_ui_scratch(16, |m, p| {
                    let status = m.modern_ui_invoke(parent, 4, [widget, p, p + 8], &[])?;
                    Ok((status, m.cpu.read_u32(p)?, m.root_rect(p + 8)?))
                })?;
                if status != 0 || root == 0 { return Ok(None); }
                held.push(root); // Locate adquire o container retornado.
                x += i32::from(position.x);
                y += i32::from(position.y);
                if let Some((next, next_state)) = self.root_state(Interface::RootContainer, root) {
                    current = next;
                    state = next_state;
                    continue;
                }
                // Um provedor estrangeiro pode devolver o root via sua própria ABI.
                let widget = self.modern_ui_scratch(4, |m, p| {
                    let status = m.modern_ui_invoke(root, 2, [0x01015952, p, 0], &[])?;
                    Ok(if status == 0 { m.cpu.read_u32(p)? } else { 0 })
                })?;
                if widget == 0 { return Ok(None); }
                held.push(widget);
                if let Some((native, _)) = self.root_state(Interface::RootContainerWidget, widget)
                    && visited.contains(&native) { return Ok(None); }
                let origin = self.modern_ui_scratch(24, |m, p| {
                    m.cpu.write_u32(p, 0x0103f55c)?;
                    m.cpu.write_u32(p + 4, 8)?;
                    m.cpu.write_u32(p + 8, p + 16)?;
                    let handled = m.modern_ui_invoke(widget, 3, [0x800, 0x600, p], &[])?;
                    Ok(if handled != 0 { Some(m.root_rect(p + 16)?) } else { None })
                })?;
                return Ok(origin.and_then(|r| i16::try_from(x + i32::from(r.x)).ok()
                    .zip(i16::try_from(y + i32::from(r.y)).ok())));
            }
            Ok(None)
        })();
        let mut cleanup = None;
        for object in held.into_iter().rev() {
            if let Err(error) = self.modern_font_base(object, 1) { cleanup.get_or_insert(error); }
        }
        match (result, cleanup) {
            (Err(error), _) | (Ok(_), Some(error)) => Err(error),
            (Ok(origin), None) => Ok(origin),
        }
    }
    fn root_event(
        &mut self,
        owner: u32,
        s: u32,
        event: u32,
        w: u32,
        dw: u32,
    ) -> Result<u32, CpuError> {
        let inner = self.cpu.read_u32(s + INNER_W)?;
        if event == 0x820 {
            self.cpu.write_u32(s + TOUCH, (dw != 0) as u32)?;
            for child in self.root_children(s)? {
                self.modern_font_base(child, 0)?;
                let call = self.modern_ui_invoke(child, 3, [event, w, dw], &[]);
                self.modern_font_base(child, 1)?;
                call?;
            }
            if dw == 0 {
                let capture = self.cpu.read_u32(s + CAPTURE)?;
                self.cpu.write_u32(s + CAPTURE, 0)?;
                self.modern_font_base(capture, 1)?;
            }
            return Ok(TRUE);
        }
        if matches!(event,0x504..=0x507|0x50b..=0x50e|0x5a5|0x5a6) {
            return self.root_pointer(s, event, w, dw);
        }
        if matches!(event, 0x800 | 0x801) {
            let set = event == 0x801;
            if w == 0x161 && !set {
                let model = self.cpu.read_u32(s + VIEW)?;
                if !self.modern_font_write(dw, &model.to_le_bytes()) {
                    return Ok(FALSE);
                }
                self.modern_font_base(model, 0)?;
                return Ok(TRUE);
            }
            if w == 0x600 && !set {
                if !self.modern_font_valid(dw, 12) {
                    return Ok(FALSE);
                }
                let id = self.cpu.read_u32(dw)?;
                let size = self.cpu.read_u32(dw + 4)?;
                let out = self.cpu.read_u32(dw + 8)?;
                if id == 0x0103f55c && size == 8 && self.modern_font_valid(out, 8) {
                    let Some((x, y)) = self.root_screen_origin(owner, s)? else {
                        return Ok(FALSE);
                    };
                    self.cpu.write_mem(
                        out,
                        &[x, y]
                            .into_iter()
                            .flat_map(i16::to_le_bytes)
                            .collect::<Vec<_>>(),
                    )?;
                    return Ok(TRUE);
                }
                return Ok(FALSE);
            }
            let field = match w {
                0x130 | 0x131 => Some(BG_A),
                0x132 => Some(BG_I),
                0x133 => Some(BG_SA),
                0x134 => Some(BG_SI),
                _ => None,
            };
            if let Some(f) = field {
                if set {
                    for field in if w == 0x130 {
                        vec![BG_A, BG_I, BG_SA, BG_SI]
                    } else {
                        vec![f]
                    } {
                        self.cpu.write_u32(s + field, dw)?;
                    }
                    self.root_mark(owner, s, self.root_extent(s)?, 2)?;
                    return Ok(TRUE);
                }
                return Ok(
                    self.modern_font_write(dw, &self.cpu.read_u32(s + f)?.to_le_bytes()) as u32,
                );
            }
        }
        if event == 0x802 {
            if w > 2 {
                return Ok(FALSE);
            }
            if w == 0 {
                self.cpu.write_u32(s + LAYOUT, 0)?;
                self.cancel_callbacks_of(owner)?;
                self.cpu.write_u32(s + PENDING, 0)?;
            } else {
                self.cpu.write_u32(s + LAYOUT, 1)?;
                self.root_mark(owner, s, self.root_extent(s)?, 3)?;
            }
        }
        if event == 0x700 {
            self.cpu.write_u32(s + FOCUSED, (w != 0) as u32)?;
        }
        if event == 0x801 && w == 0x151 {
            self.cpu.write_u32(s + SELECTED, dw)?;
        }
        let handled = self.modern_ui_invoke(inner, 3, [event, w, dw], &[])?;
        if handled != 0 && matches!(event, 0x700 | 0x801) {
            self.root_mark(owner, s, self.root_extent(s)?, 2)?;
        }
        Ok(handled)
    }
    fn root_pointer(&mut self, s: u32, event: u32, w: u32, dw: u32) -> Result<u32, CpuError> {
        if self.cpu.read_u32(s + TOUCH)? == 0 {
            return Ok(FALSE);
        }
        let legacy = (0x504..=0x507).contains(&event);
        let (rawx, rawy, original) = if legacy {
            (
                (dw >> 16) as u16 as i16 as i32,
                dw as u16 as i16 as i32,
                String::new(),
            )
        } else {
            if w == 0 || w > 1024 || !self.modern_font_valid(dw, w as usize) {
                return Ok(FALSE);
            }
            let mut bytes = vec![0; w as usize];
            self.cpu.read_mem(dw, &mut bytes)?;
            if bytes.last() != Some(&0) {
                return Ok(FALSE);
            }
            let text = String::from_utf8_lossy(&bytes[..bytes.len() - 1]).into_owned();
            let get = |key: &str| {
                text.split(',')
                    .find_map(|p| {
                        p.strip_prefix(key)
                            .and_then(|v| u32::from_str_radix(v, 16).ok())
                    })
                    .map(|v| v as i32)
            };
            let (Some(x), Some(y)) = (get("x="), get("y=")) else {
                return Ok(FALSE);
            };
            (x, y, text)
        };
        let region = self.root_rect(s + REGION)?;
        let x = rawx.saturating_sub(i32::from(region.x));
        let y = rawy.saturating_sub(i32::from(region.y));
        let oldcapture = self.cpu.read_u32(s + CAPTURE)?;
        let mut target = oldcapture;
        let mut ox = 0;
        let mut oy = 0;
        let inner = self.cpu.read_u32(s + INNER)?;
        if target == 0 || matches!(event, 0x504 | 0x50b | 0x5a5 | 0x5a6) {
            let mut children = self.root_children(s)?;
            children.reverse();
            target = 0;
            for child in children {
                let hit = self.modern_ui_scratch(24, |m, p| {
                    m.modern_ui_invoke(inner, 9, [child, p, 0], &[])?;
                    let (px, py) = (m.cpu.read_u32(p)? as i32, m.cpu.read_u32(p + 4)? as i32);
                    let visible = m.cpu.read_u32(p + 8)? & 255 != 0;
                    m.modern_ui_invoke(child, 6, [p, 0, 0], &[])?;
                    let (cw, ch) = (m.cpu.read_u32(p)? as i32, m.cpu.read_u32(p + 4)? as i32);
                    if !visible
                        || x < px
                        || y < py
                        || x >= px.saturating_add(cw)
                        || y >= py.saturating_add(ch)
                    {
                        return Ok(None);
                    }

                    Ok(Some((px, py)))
                })?;
                if let Some((px, py)) = hit {
                    let capable = self.modern_ui_scratch(12, |m, p| {
                        m.cpu.write_u32(p, (x - px) as u32)?;
                        m.cpu.write_u32(p + 4, (y - py) as u32)?;
                        m.cpu.write_u32(p + 8, 0)?;
                        m.modern_ui_invoke(child, 3, [0x901, 0, p], &[])?;
                        Ok(m.cpu.read_u32(p + 8)? & 255 != 0)
                    })?;
                    if capable {
                        target = child;
                        ox = px;
                        oy = py;
                        break;
                    }
                }
            }
        } else {
            let position = self.modern_ui_scratch(12, |m, p| {
                let rc = m.modern_ui_invoke(inner, 9, [target, p, 0], &[])?;
                Ok((rc, m.cpu.read_u32(p)? as i32, m.cpu.read_u32(p + 4)? as i32))
            })?;
            if position.0 != 0 {
                target = 0
            } else {
                ox = position.1;
                oy = position.2;
            }
        }
        if target == 0 {
            return Ok(FALSE);
        }
        self.modern_font_base(target, 0)?;
        if matches!(event, 0x504 | 0x50b) {
            self.modern_font_base(target, 0)?;
            self.cpu.write_u32(s + CAPTURE, target)?;
            self.modern_font_base(oldcapture, 1)?;
        }
        let result = if legacy {
            let packed = ((x - ox) as u16 as u32) << 16 | ((y - oy) as u16 as u32);
            self.modern_ui_invoke(target, 3, [event, w, packed], &[])
        } else {
            let tail = original
                .split(',')
                .filter(|p| !p.starts_with("x=") && !p.starts_with("y=") && !p.is_empty())
                .collect::<Vec<_>>()
                .join(",");
            let text = format!(
                "x={:08X},y={:08X},{}{}",
                (x - ox) as u32,
                (y - oy) as u32,
                tail,
                if tail.is_empty() { "" } else { "," }
            );
            self.modern_ui_scratch(text.len() as u32 + 1, |m, p| {
                m.cpu.write_mem(p, text.as_bytes())?;
                m.cpu.write_mem(p + text.len() as u32, &[0])?;
                m.modern_ui_invoke(target, 3, [event, text.len() as u32 + 1, p], &[])
            })
        };
        if matches!(event, 0x506 | 0x50d) {
            let held = self.cpu.read_u32(s + CAPTURE)?;
            self.cpu.write_u32(s + CAPTURE, 0)?;
            self.modern_font_base(held, 1)?;
        }
        self.modern_font_base(target, 1)?;
        result
    }
    pub(super) fn root_container_call(
        &mut self,
        iface: Interface,
        slot: u32,
    ) -> Result<Option<u32>, CpuError> {
        let this = self.arg(0);
        let resolved = if iface == Interface::RootContainerWidget && slot == 14 {
            self.root_state(Interface::RootContainer, this)
        } else {
            self.root_state(iface, this)
        };
        let Some((owner, s)) = resolved else {
            return Ok(Some(BAD));
        };
        if slot > 2 {
            self.objects.add_ref(owner);
        }
        let result = self.root_call_inner(owner, s, iface, slot);
        if slot > 2 {
            self.modern_font_base(owner, 1)?;
        }
        result
    }
    fn root_call_inner(
        &mut self,
        owner: u32,
        s: u32,
        iface: Interface,
        slot: u32,
    ) -> Result<Option<u32>, CpuError> {
        let (a, b, c) = (self.arg(1), self.arg(2), self.arg(3));
        let widget = self.cpu.read_u32(owner + 8)?;
        let inner = self.cpu.read_u32(s + INNER)?;
        let iw = self.cpu.read_u32(s + INNER_W)?;
        if iface == Interface::RootContainerWidget && slot == 14 {
            return Ok(Some(self.root_event(owner, s, a, b, c)?));
        }
        let rc = match slot {
            0 => self.objects.add_ref(owner),
            1 => {
                if self.objects.contagem(owner) == 1 {
                    self.cancel_callbacks_of(owner)?;
                    for f in [CAPTURE, CANVAS, VIEW] {
                        let p = self.cpu.read_u32(s + f)?;
                        self.cpu.write_u32(s + f, 0)?;
                        self.modern_font_base(p, 1)?;
                    }
                    self.modern_font_base(iw, 1)?;
                    self.modern_font_base(inner, 1)?;
                    let free = self.cpu.read_u32(s + FREE)?;
                    self.cpu.write_u32(s + FREE, 0)?;
                    if free != 0 {
                        self.modern_ui_call(free, [self.cpu.read_u32(s + CONTEXT)?, 0, 0, 0], &[])?;
                    }
                    self.heap.free(s);
                    self.objects.release(widget);
                }
                self.objects.release(owner)
            }
            2 => {
                if !self.modern_font_write(b, &0u32.to_le_bytes()) {
                    return Ok(Some(BAD));
                }
                let target = match a {
                    0x01000001 | 0x0102ec16 | 0x01015954 | 0x01015932 => owner,
                    0x01015952 | 0x01015956 => widget,
                    _ => 0,
                };
                if target == 0 {
                    3
                } else {
                    self.cpu.write_u32(b, target)?;
                    self.objects.add_ref(owner);
                    0
                }
            }
            _ if iface == Interface::RootContainer => match slot {
                3 => self.root_invalidate(owner, s, a, b, c)?,
                4 => {
                    if !self.modern_font_valid(c, 8) || (b != 0 && !self.modern_font_valid(b, 4)) {
                        return Ok(Some(BAD));
                    }
                    if !self.root_children(s)?.contains(&a) {
                        return Ok(Some(1));
                    }
                    self.modern_ui_scratch(12, |m, p| {
                        let rc = m.modern_ui_invoke(inner, 9, [a, p, 0], &[])?;
                        if rc != 0 {
                            return Ok(rc);
                        }
                        let mut r = m.root_rect(c)?;
                        let x = i32::from(r.x) + m.cpu.read_u32(p)? as i32;
                        let y = i32::from(r.y) + m.cpu.read_u32(p + 4)? as i32;
                        if i16::try_from(x).is_err() || i16::try_from(y).is_err() {
                            return Ok(BAD);
                        }
                        r.x = x as i16;
                        r.y = y as i16;
                        m.root_write_rect(c, r)?;
                        if b != 0 {
                            m.cpu.write_u32(b, owner)?;
                            m.objects.add_ref(owner);
                        }
                        Ok(0)
                    })?
                }
                5 => {
                    if !self.root_can_insert(owner, s, a)? {
                        return Ok(Some(BAD));
                    }
                    let rc = self.modern_ui_invoke(inner, 5, [a, b, c], &[])?;
                    if rc == 0 {
                        self.modern_ui_invoke(a, 9, [owner, 0, 0], &[])?;
                        if self.cpu.read_u32(s + TOUCH)? != 0 {
                            self.modern_ui_invoke(a, 3, [0x820, 0, 1], &[])?;
                        }
                        self.root_mark(owner, s, self.root_extent(s)?, 3)?;
                    }
                    rc
                }
                6 | 8 => {
                    let rc = self.modern_ui_invoke(inner, slot, [a, b, c], &[])?;
                    if rc == 0 {
                        self.root_mark(owner, s, self.root_extent(s)?, 3)?;
                    }
                    rc
                }
                7 => {
                    self.modern_ui_invoke(inner, 7, [a, b, c], &[self.stack_arg(0).unwrap_or(0)])?
                }
                9 => self.modern_ui_invoke(inner, 9, [a, b, c], &[])?,
                10 => {
                    if b == 0 || !self.modern_font_valid(b, 8) {
                        return Ok(Some(BAD));
                    }
                    let rect = self.root_rect(b)?;
                    if rect.width < 0 || rect.height < 0 {
                        return Ok(Some(BAD));
                    }
                    if a != 0
                        && self
                            .objects
                            .kind_of(a)
                            .is_some_and(|kind| kind != Interface::Canvas)
                    {
                        return Ok(Some(3));
                    }
                    let next = if a == 0 {
                        0
                    } else {
                        self.modern_ui_scratch(8, |m, p| {
                            let rc = m.modern_ui_invoke(a, 2, [0x0101e3f2, p, 0], &[])?;
                            Ok(if rc == 0 { m.cpu.read_u32(p)? } else { 0 })
                        })?
                    };
                    if a != 0 && next == 0 {
                        return Ok(Some(3));
                    }
                    let old = self.cpu.read_u32(s + CANVAS)?;
                    self.cpu.write_u32(s + CANVAS, next)?;
                    let original = self.root_rect(s + REGION)?;
                    self.cpu.write_mem(
                        s + REGION,
                        &[rect.x, rect.y, original.width, original.height]
                            .into_iter()
                            .flat_map(i16::to_le_bytes)
                            .collect::<Vec<_>>(),
                    )?;
                    self.modern_font_base(old, 1)?;
                    self.root_set_extent(owner, s, rect.width as u32, rect.height as u32)?;
                    self.root_mark(owner, s, self.root_extent(s)?, 3)?;
                    0
                }
                11 => {
                    if !self.modern_font_write(a, &0u32.to_le_bytes())
                        || !self.modern_font_valid(b, 8)
                    {
                        return Ok(Some(BAD));
                    }
                    let canvas = self.cpu.read_u32(s + CANVAS)?;
                    self.root_write_rect(b, self.root_rect(s + REGION)?)?;
                    if canvas == 0 {
                        1
                    } else {
                        self.cpu.write_u32(a, canvas)?;
                        self.modern_font_base(canvas, 0)?;
                        0
                    }
                }
                12 => {
                    self.root_paint_pending(owner, s)?;
                    0
                }
                _ => return Ok(None),
            },
            3 => {
                let handler = self.cpu.read_u32(s + HANDLER)?;
                if handler == 0 {
                    FALSE
                } else {
                    self.modern_ui_call(handler, [self.cpu.read_u32(s + CONTEXT)?, a, b, c], &[])?
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
                for (f, v) in [HANDLER, CONTEXT, FREE].into_iter().zip(next) {
                    self.cpu.write_u32(s + f, v)?;
                }
                0
            }
            5 | 6 => {
                self.modern_ui_invoke(iw, slot, [a, b, c], &[])?;
                0
            }
            7 => {
                if !self.modern_font_valid(a, 8) {
                    return Ok(Some(BAD));
                }
                self.root_set_extent(owner, s, self.cpu.read_u32(a)?, self.cpu.read_u32(a + 4)?)?
            }
            8 => {
                let parent = self.cpu.read_u32(s + PARENT)?;
                if !self.modern_font_write(a, &parent.to_le_bytes()) {
                    return Ok(Some(BAD));
                }
                self.modern_font_base(parent, 0)?;
                0
            }
            9 => {
                if a == owner {
                    return Ok(Some(BAD));
                }
                self.cpu.write_u32(s + PARENT, a)?;
                if a != 0 {
                    self.cancel_callbacks_of(owner)?;
                    self.cpu.write_u32(s + PENDING, 0)?;
                }
                0
            }
            10 => self.root_render(s, a, b as i32, c as i32, self.root_extent(s)?)?,
            11 => self.modern_ui_invoke(iw, 11, [a, b, c], &[])?,
            12 => {
                if self.modern_font_write(b, &0u32.to_le_bytes()) {
                    3
                } else {
                    BAD
                }
            }
            13 => 3,
            _ => return Ok(None),
        };
        Ok(Some(rc))
    }
}
