//! CSoftkeyWidget: composição real de rótulos, sem transformar teclas em comandos.
use super::*;
pub(super) const CLASS_ID: u32 = 0x0105_51df;
const BAD: u32 = 14;
const BYTES: u32 = 160;
const WIDTH: u32 = 0;
const HEIGHT: u32 = 4;
const PARENT: u32 = 8;
const FLAGS: u32 = 12;
// IDF de alinhamento e SWF de texto já implementados pelo CStaticWidget.
const LABEL_FLAGS: u32 = 0x0070_0770;
const HANDLER: u32 = 16;
const CONTEXT: u32 = 20;
const FREE: u32 = 24;
const FOCUSED: u32 = 28;
const SELECTED: u32 = 32;
const LEFT: u32 = 36;
const RIGHT: u32 = 40;
const TOP: u32 = 44;
const BOTTOM: u32 = 48;
const BG_A: u32 = 52;
const BG_I: u32 = 56;
const BG_SA: u32 = 60;
const BG_SI: u32 = 64;
const LABEL1: u32 = 68;
const LABEL2: u32 = 72;
const LABEL3: u32 = 76;
// O pai dos rótulos tem ABI IContainer, distinta do IWidget público. O facet e
// sua vtable vivem no heap salvo; QI do softkey não anuncia IContainer.
const FACET: u32 = 96;
const VT: u32 = 104;
#[derive(Clone, Copy)]
struct SoftCell {
    widget: u32,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
}
impl<C: CpuBackend> Machine<C> {
    fn softkey_state(&self, w: u32) -> Option<u32> {
        if self.objects.kind_of(w) != Some(Interface::SoftkeyWidget) {
            return None;
        }
        let s = self.cpu.read_u32(w + 4).ok()?;
        self.heap.size_of(s).filter(|n| *n >= BYTES).map(|_| s)
    }
    fn softkey_invalidate(&mut self, w: u32, s: u32) -> Result<(), CpuError> {
        let p = self.cpu.read_u32(s + PARENT)?;
        if p != 0 {
            self.modern_ui_invoke(p, 3, [w, 0, 3], &[])?;
        }
        Ok(())
    }
    fn softkey_labels(&self, s: u32) -> Result<Vec<u32>, CpuError> {
        Ok([LABEL1, LABEL2, LABEL3]
            .into_iter()
            .map(|f| self.cpu.read_u32(s + f))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|p| *p != 0)
            .collect())
    }
    fn softkey_cells(&self, s: u32) -> Result<Vec<SoftCell>, CpuError> {
        let w = self.cpu.read_u32(s + WIDTH)?;
        let h = self.cpu.read_u32(s + HEIGHT)?;
        let l = self.cpu.read_u32(s + LEFT)?.min(w);
        let r = self.cpu.read_u32(s + RIGHT)?.min(w - l);
        let t = self.cpu.read_u32(s + TOP)?.min(h);
        let b = self.cpu.read_u32(s + BOTTOM)?.min(h - t);
        let center = self.cpu.read_u32(s + FLAGS)? & 1 != 0;
        let n: u32 = if center { 3 } else { 2 };
        let inner = w - l - r;
        let fields = if center {
            vec![LABEL1, LABEL3, LABEL2]
        } else {
            vec![LABEL1, LABEL2]
        };
        let mut cells = Vec::new();
        for (i, f) in fields.into_iter().enumerate() {
            let a = (u64::from(inner) * i as u64 / u64::from(n)) as u32;
            let z = (u64::from(inner) * (i + 1) as u64 / u64::from(n)) as u32;
            let p = self.cpu.read_u32(s + f)?;
            if p != 0 {
                cells.push(SoftCell {
                    widget: p,
                    x: (l + a) as i32,
                    y: t as i32,
                    w: z - a,
                    h: h - t - b,
                });
            }
        }
        Ok(cells)
    }
    fn softkey_layout(&mut self, s: u32) -> Result<(), CpuError> {
        for c in self.softkey_cells(s)? {
            self.modern_font_base(c.widget, 0)?;
            let result = self.modern_ui_scratch(8, |m, p| {
                m.modern_ui_invoke(c.widget, 6, [p, 0, 0], &[])?;
                if m.cpu.read_u32(p)? != c.w || m.cpu.read_u32(p + 4)? != c.h {
                    m.cpu.write_u32(p, c.w)?;
                    m.cpu.write_u32(p + 4, c.h)?;
                    m.modern_ui_invoke(c.widget, 7, [p, 0, 0], &[])?;
                }
                Ok(())
            });
            self.modern_font_base(c.widget, 1)?;
            result?;
        }
        Ok(())
    }
    fn softkey_new_label(&mut self, s: u32, field: u32) -> Result<u32, CpuError> {
        self.modern_ui_scratch(8, |m, p| {
            let rc = m.create_static_widget(p)?;
            if rc != SUCCESS {
                return Ok(rc);
            }
            let child = m.cpu.read_u32(p)?;
            let align = match field {
                LABEL1 => 0x210,
                LABEL2 => 0x240,
                _ => 0x220,
            };
            m.modern_ui_invoke(child, 3, [0x801, 0x153, 0x20_0000 | align], &[])?;
            m.cpu.write_u32(s + field, child)?;
            m.modern_ui_invoke(child, 9, [s + FACET, 0, 0], &[])?;
            Ok(SUCCESS)
        })
    }
    pub(super) fn create_softkey_widget(&mut self, out: u32) -> Result<u32, CpuError> {
        if !self.modern_font_write(out, &0u32.to_le_bytes()) {
            return Ok(BAD);
        }
        let w = self.new_object(Interface::SoftkeyWidget)?;
        if w == 0 {
            return Ok(2);
        }
        let s = self.malloc(BYTES)?;
        if s == 0 {
            self.objects.release(w);
            return Ok(2);
        }
        self.cpu.write_mem(s, &[0; BYTES as usize])?;
        self.cpu.write_mem(w + 4, &[0; 60])?;
        self.cpu.write_u32(w + 4, s)?;
        for (f, v) in [
            (WIDTH, 50),
            (HEIGHT, 10),
            (LEFT, 4),
            (RIGHT, 4),
            (HANDLER, aee::encode(Interface::SoftkeyWidget, 14)),
            (CONTEXT, w),
            (BG_A, 0),
            (BG_I, 0),
            (BG_SA, 0),
            (BG_SI, 0),
            (FACET, s + VT),
            (FACET + 4, w),
        ] {
            self.cpu.write_u32(s + f, v)?;
        }
        for slot in 0..8 {
            self.cpu.write_u32(
                s + VT + slot * 4,
                aee::encode(Interface::SoftkeyWidget, 15 + slot),
            )?;
        }
        for field in [LABEL1, LABEL2] {
            let rc = self.softkey_new_label(s, field)?;
            if rc != SUCCESS {
                self.modern_font_base(w, 1)?;
                return Ok(rc);
            }
        }
        self.softkey_layout(s)?;
        self.cpu.write_u32(out, w)?;
        Ok(SUCCESS)
    }
    fn softkey_cycle(&mut self, w: u32, candidate: u32) -> Result<bool, CpuError> {
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
    fn softkey_replace(
        &mut self,
        w: u32,
        s: u32,
        field: u32,
        child: u32,
    ) -> Result<bool, CpuError> {
        if self.cpu.read_u32(s + field)? == child {
            return Ok(true);
        }
        if child != 0 && self.softkey_cycle(w, child)? {
            return Ok(false);
        }
        let acquired = if child == 0 {
            0
        } else {
            self.modern_ui_scratch(8, |m, p| {
                if m.modern_ui_invoke(child, 2, [0x01015952, p, 0], &[])? != 0 {
                    return Ok(0);
                }
                let next = m.cpu.read_u32(p)?;
                if next == 0 {
                    return Ok(0);
                }
                // QI pode adquirir outro facet: o teste de ciclo deve usar
                // tambem a identidade IWidget efetivamente inserida.
                if m.softkey_cycle(w, next)? {
                    m.modern_font_base(next, 1)?;
                    return Ok(0);
                }
                m.cpu.write_u32(p, 0)?;
                let result = m.modern_ui_invoke(next, 8, [p, 0, 0], &[]);
                let parent = m.cpu.read_u32(p)?;
                if parent != 0 {
                    m.modern_font_base(parent, 1)?;
                }
                // GetParent e void; somente o ponteiro escrito indica pai.
                if let Err(error) = result {
                    m.modern_font_base(next, 1)?;
                    return Err(error);
                }
                if parent != 0 {
                    m.modern_font_base(next, 1)?;
                    return Ok(0);
                }
                Ok(next)
            })?
        };
        if child != 0 && acquired == 0 {
            return Ok(false);
        }
        let old = self.cpu.read_u32(s + field)?;
        self.cpu.write_u32(s + field, acquired)?;
        if acquired != 0 {
            self.modern_ui_invoke(acquired, 9, [s + FACET, 0, 0], &[])?;
        }
        if old != 0 {
            self.modern_ui_invoke(old, 9, [0; 3], &[])?;
            self.modern_font_base(old, 1)?;
        }
        self.softkey_layout(s)?;
        self.softkey_invalidate(w, s)?;
        Ok(true)
    }
    fn softkey_property(
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
            if size != 4 || !self.modern_font_valid(p, 4) {
                return Ok(FALSE);
            }
            key = match ex {
                0x01077ada => 0x120,
                0x01077adb => 0x121,
                0x01077adc => 0x122,
                0x01077add => 0x123,
                0x01077ade => 0x124,
                _ => return Ok(FALSE),
            };
            data = if set { self.cpu.read_u32(p)? } else { p };
        }
        let label = match key {
            0x200 => Some(LABEL1),
            0x201 => Some(LABEL2),
            0x254 => Some(LABEL3),
            _ => None,
        };
        if let Some(f) = label {
            if set {
                return Ok(self.softkey_replace(w, s, f, data)? as u32);
            }
            let p = self.cpu.read_u32(s + f)?;
            if !self.modern_font_write(data, &p.to_le_bytes()) {
                return Ok(FALSE);
            }
            self.modern_font_base(p, 0)?;
            return Ok(TRUE);
        }
        let field = match key {
            0x153 => Some(FLAGS),
            0x151 => Some(SELECTED),
            0x121 => Some(LEFT),
            0x122 => Some(RIGHT),
            0x123 => Some(TOP),
            0x124 => Some(BOTTOM),
            0x131 => Some(BG_A),
            0x132 => Some(BG_I),
            0x133 => Some(BG_SA),
            0x134 => Some(BG_SI),
            _ => None,
        };
        if let Some(f) = field {
            if !set {
                return Ok(
                    self.modern_font_write(data, &self.cpu.read_u32(s + f)?.to_le_bytes()) as u32,
                );
            }
            if f == FLAGS {
                if data & !(1 | LABEL_FLAGS) != 0 {
                    return Ok(FALSE);
                }
                if data & 1 != 0 && self.cpu.read_u32(s + LABEL3)? == 0 {
                    let rc = self.softkey_new_label(s, LABEL3)?;
                    if rc != 0 {
                        return Ok(FALSE);
                    }
                }
                // O SDK encaminha eventos não tratados aos widgets embutidos.
                // Recusar IDF_CENTER|IDF_BOTTOM fazia o consumidor abortar antes
                // de inserir o softkey no pai. O bit de centro pertence apenas
                // ao softkey; uma troca só desse bit preserva os rótulos próprios.
                let label_flags = data & LABEL_FLAGS;
                if label_flags != 0 {
                    let labels = self.softkey_labels(s)?;
                    for child in &labels {
                        self.modern_font_base(*child, 0)?;
                    }
                    let result: Result<u32, CpuError> = (|| {
                        for child in &labels {
                            if self.modern_ui_invoke(*child, 3, [0x801, key, label_flags], &[])?
                                == FALSE
                            {
                                return Ok(FALSE);
                            }
                        }
                        Ok(TRUE)
                    })();
                    for child in labels {
                        self.modern_font_base(child, 1)?;
                    }
                    if result? == FALSE {
                        return Ok(FALSE);
                    }
                }
            }
            if [LEFT, RIGHT, TOP, BOTTOM].contains(&f) && (data as i32) < 0 {
                return Ok(FALSE);
            }
            self.cpu.write_u32(s + f, data)?;
            self.softkey_layout(s)?;
            self.softkey_invalidate(w, s)?;
            return Ok(TRUE);
        }
        if set && matches!(key, 0x120 | 0x130 | 0x135) {
            let fields = match key {
                0x120 => vec![LEFT, RIGHT, TOP, BOTTOM],
                0x130 => vec![BG_A, BG_I, BG_SA, BG_SI],
                _ => vec![BG_SA, BG_SI],
            };
            if key == 0x120 && (data as i32) < 0 {
                return Ok(FALSE);
            }
            for f in fields {
                self.cpu.write_u32(s + f, data)?;
            }
            self.softkey_layout(s)?;
            self.softkey_invalidate(w, s)?;
            return Ok(TRUE);
        }
        // Fonte e cores pertencem aos rótulos reais; consultar devolve a propriedade
        // adquirida do primeiro rótulo, sem criar um modelo global fictício.
        if matches!(key, 0x140..=0x145 | 0x152 | 0x315 | 0x418) {
            let mut handled = FALSE;
            for label in self.softkey_labels(s)? {
                self.modern_font_base(label, 0)?;
                let result = self.modern_ui_invoke(
                    label,
                    3,
                    [if set { 0x801 } else { 0x800 }, key, data],
                    &[],
                );
                self.modern_font_base(label, 1)?;
                handled = result?;
                if !set || handled == FALSE {
                    break;
                }
            }
            return Ok(handled);
        }
        Ok(FALSE)
    }
    fn softkey_event(
        &mut self,
        w: u32,
        s: u32,
        event: u32,
        param: u32,
        data: u32,
    ) -> Result<u32, CpuError> {
        if matches!(event, 0x800 | 0x801) {
            let handled = self.softkey_property(w, s, event == 0x801, param, data)?;
            if handled != 0 {
                return Ok(handled);
            }
            return Ok(FALSE);
        }
        if event == 0x700 {
            self.cpu.write_u32(s + FOCUSED, (param != 0) as u32)?;
        }
        if matches!(event, 0x701 | 0x702) {
            return Ok(self.modern_font_write(
                data,
                &[(event == 0x701 && self.cpu.read_u32(s + FOCUSED)? != 0) as u8],
            ) as u32);
        }
        let mut handled = FALSE;
        for child in self.softkey_labels(s)? {
            self.modern_font_base(child, 0)?;
            let result = self.modern_ui_invoke(child, 3, [event, param, data], &[]);
            self.modern_font_base(child, 1)?;
            if result? != 0 {
                handled = TRUE;
                break;
            }
        }
        if event == 0x700 {
            self.softkey_invalidate(w, s)?;
            return Ok(TRUE);
        }
        Ok(handled)
    }
    fn softkey_draw(&mut self, s: u32, canvas: u32, x: i32, y: i32) -> Result<u32, CpuError> {
        self.softkey_layout(s)?;
        let Some(&target) = self.canvases.get(&canvas) else {
            return Ok(20);
        };
        let Some(surface) = self.bitmaps.get(&target) else {
            return Ok(20);
        };
        let w = self.cpu.read_u32(s + WIDTH)? as i32;
        let h = self.cpu.read_u32(s + HEIGHT)? as i32;
        let mut x0 = x.max(0);
        let mut y0 = y.max(0);
        let mut x1 = x.saturating_add(w).min(surface.width() as i32);
        let mut y1 = y.saturating_add(h).min(surface.height() as i32);
        let previous = self.clip;
        if let Some(c) = previous {
            x0 = x0.max(c.x as i32);
            y0 = y0.max(c.y as i32);
            x1 = x1.min(c.x as i32 + c.width as i32);
            y1 = y1.min(c.y as i32 + c.height as i32);
        }
        if x1 <= x0 || y1 <= y0 {
            return Ok(SUCCESS);
        }
        let outer = Rect {
            x: x0 as i16,
            y: y0 as i16,
            width: (x1 - x0).min(i16::MAX as i32) as i16,
            height: (y1 - y0).min(i16::MAX as i32) as i16,
        };
        let active = self.cpu.read_u32(s + FOCUSED)? != 0;
        let selected = self.cpu.read_u32(s + SELECTED)? != 0;
        let color = self.cpu.read_u32(
            s + match (active, selected) {
                (true, false) => BG_A,
                (false, false) => BG_I,
                (true, true) => BG_SA,
                (false, true) => BG_SI,
            },
        )?;
        self.modern_widget_fill(target, outer, color)?;
        let cells = self.softkey_cells(s)?;
        for c in &cells {
            self.modern_font_base(c.widget, 0)?;
        }
        let result = (|| {
            for c in &cells {
                let cx = x.saturating_add(c.x);
                let cy = y.saturating_add(c.y);
                let l = x0.max(cx);
                let t = y0.max(cy);
                let r = x1.min(cx.saturating_add(c.w as i32));
                let b = y1.min(cy.saturating_add(c.h as i32));
                if r <= l || b <= t {
                    continue;
                }
                self.clip = Some(Rect {
                    x: l as i16,
                    y: t as i16,
                    width: (r - l).min(i16::MAX as i32) as i16,
                    height: (b - t).min(i16::MAX as i32) as i16,
                });
                // IWidget::Draw e void; R0 residual nao pode suprimir irmaos.
                self.modern_ui_invoke(c.widget, 10, [canvas, cx as u32, cy as u32], &[])?;
            }
            Ok(SUCCESS)
        })();
        self.clip = previous;
        for c in cells {
            self.modern_font_base(c.widget, 1)?;
        }
        result
    }
    pub(super) fn softkey_widget_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let pointer = self.arg(0);
        let private = slot >= 15;
        let owner = if private {
            match self.cpu.read_u32(pointer + 4) {
                Ok(w) => w,
                Err(_) => return Ok(Some(BAD)),
            }
        } else {
            pointer
        };
        let Some(s) = self.softkey_state(owner) else {
            return Ok(Some(BAD));
        };
        if private && pointer != s + FACET {
            return Ok(Some(BAD));
        }
        let actual = if private { slot - 15 } else { slot };
        let pin = actual > 2;
        if pin {
            self.objects.add_ref(owner);
        }
        let result = self.softkey_widget_inner(owner, s, private, actual);
        if pin {
            self.modern_font_base(owner, 1)?;
        }
        result
    }
    fn softkey_widget_inner(
        &mut self,
        w: u32,
        s: u32,
        private: bool,
        slot: u32,
    ) -> Result<Option<u32>, CpuError> {
        let (a, b, c) = (self.arg(1), self.arg(2), self.arg(3));
        let rc = match slot {
            0 => self.objects.add_ref(w),
            1 => {
                if self.objects.contagem(w) == 1 {
                    for f in [LABEL1, LABEL2, LABEL3] {
                        let p = self.cpu.read_u32(s + f)?;
                        self.cpu.write_u32(s + f, 0)?;
                        if p != 0 {
                            self.modern_ui_invoke(p, 9, [0; 3], &[])?;
                            self.modern_font_base(p, 1)?;
                        }
                    }
                    let free = self.cpu.read_u32(s + FREE)?;
                    self.cpu.write_u32(s + FREE, 0)?;
                    if free != 0 {
                        self.modern_ui_call(free, [self.cpu.read_u32(s + CONTEXT)?, 0, 0, 0], &[])?;
                    }
                    self.heap.free(s);
                }
                self.objects.release(w)
            }
            2 => {
                if !self.modern_font_write(b, &0u32.to_le_bytes()) {
                    return Ok(Some(BAD));
                }
                let target = match a {
                    0x01000001 => {
                        if private {
                            s + FACET
                        } else {
                            w
                        }
                    }
                    0x01015952 | 0x01015956 => w,
                    0x01015932 if private => s + FACET,
                    _ => 0,
                };
                if target == 0 {
                    3
                } else {
                    self.cpu.write_u32(b, target)?;
                    self.objects.add_ref(w);
                    SUCCESS
                }
            }
            _ if private => match slot {
                3 => {
                    if !self.softkey_labels(s)?.contains(&a)
                        || c & !63 != 0
                        || (b != 0 && !self.modern_font_valid(b, 8))
                    {
                        BAD
                    } else {
                        self.softkey_invalidate(w, s)?;
                        SUCCESS
                    }
                }
                4 => {
                    if !self.modern_font_valid(c, 8) || (b != 0 && !self.modern_font_valid(b, 4)) {
                        return Ok(Some(BAD));
                    }
                    let Some(cell) = self.softkey_cells(s)?.into_iter().find(|v| v.widget == a)
                    else {
                        return Ok(Some(1));
                    };
                    let mut r = self.read_rect(c)?.unwrap();
                    let rx = i32::from(r.x) + cell.x;
                    let ry = i32::from(r.y) + cell.y;
                    if i16::try_from(rx).is_err() || i16::try_from(ry).is_err() {
                        return Ok(Some(BAD));
                    }
                    r.x = rx as i16;
                    r.y = ry as i16;
                    self.cpu.write_mem(
                        c,
                        &[r.x, r.y, r.width, r.height]
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
                5 | 6 => 20,
                7 => {
                    let children = self
                        .softkey_cells(s)?
                        .into_iter()
                        .map(|v| v.widget)
                        .collect::<Vec<_>>();
                    if a == 0 {
                        if b != 0 {
                            children.first().copied().unwrap_or(0)
                        } else {
                            children.last().copied().unwrap_or(0)
                        }
                    } else if let Some(i) = children.iter().position(|p| *p == a) {
                        if b != 0 {
                            children.get(i + 1).copied().unwrap_or(0)
                        } else if i > 0 {
                            children[i - 1]
                        } else {
                            0
                        }
                    } else {
                        0
                    }
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
                if !self.modern_font_write(
                    a,
                    &old.into_iter()
                        .flat_map(u32::to_le_bytes)
                        .collect::<Vec<_>>(),
                ) {
                    return Ok(Some(BAD));
                }
                for (f, v) in [HANDLER, CONTEXT, FREE].into_iter().zip(next) {
                    self.cpu.write_u32(s + f, v)?;
                }
                SUCCESS
            }
            5 | 6 => {
                let (mut width, mut height) = (
                    self.cpu.read_u32(s + WIDTH)?,
                    self.cpu.read_u32(s + HEIGHT)?,
                );
                if slot == 5 {
                    width = 0;
                    height = 0;
                    for child in self.softkey_cells(s)? {
                        self.modern_font_base(child.widget, 0)?;
                        let size = self.modern_ui_scratch(8, |m, p| {
                            m.modern_ui_invoke(child.widget, 5, [p, 0, 0], &[])?;
                            Ok((m.cpu.read_u32(p)?, m.cpu.read_u32(p + 4)?))
                        });
                        self.modern_font_base(child.widget, 1)?;
                        let (cw, ch) = size?;
                        width = width.saturating_add(cw);
                        height = height.max(ch);
                    }
                    width = width
                        .saturating_add(self.cpu.read_u32(s + LEFT)?)
                        .saturating_add(self.cpu.read_u32(s + RIGHT)?);
                    height = height
                        .saturating_add(self.cpu.read_u32(s + TOP)?)
                        .saturating_add(self.cpu.read_u32(s + BOTTOM)?);
                }
                if self.modern_font_write(
                    a,
                    &[width, height]
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
                if !self.modern_font_valid(a, 8) {
                    return Ok(Some(BAD));
                }
                let (width, height) = (self.cpu.read_u32(a)?, self.cpu.read_u32(a + 4)?);
                if (width as i32) < 0 || (height as i32) < 0 {
                    return Ok(Some(BAD));
                }
                self.cpu.write_u32(s + WIDTH, width)?;
                self.cpu.write_u32(s + HEIGHT, height)?;
                self.softkey_layout(s)?;
                self.softkey_invalidate(w, s)?;
                SUCCESS
            }
            8 => {
                let p = self.cpu.read_u32(s + PARENT)?;
                if !self.modern_font_write(a, &p.to_le_bytes()) {
                    return Ok(Some(BAD));
                }
                self.modern_font_base(p, 0)?;
                SUCCESS
            }
            9 => {
                self.cpu.write_u32(s + PARENT, a)?;
                SUCCESS
            }
            10 => self.softkey_draw(s, a, b as i32, c as i32)?,
            11 => FALSE,
            12 => {
                if self.modern_font_write(b, &0u32.to_le_bytes()) {
                    3
                } else {
                    BAD
                }
            }
            13 => 3,
            14 => self.softkey_event(w, s, a, b, c)?,
            _ => return Ok(None),
        };
        Ok(Some(rc))
    }
}
