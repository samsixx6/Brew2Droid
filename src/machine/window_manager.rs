//! WindowMgr e DisplayWidget públicos do Brew MP.
//! O estado privado, as filas e as referências usam memória/objetos já serializados.
use super::*;
pub(super) const MANAGER_CLASS: u32 = 0x0107_4e85;
pub(super) const DISPLAY_CLASS: u32 = 0x0103_f105;
const CAPABILITY: u32 = 0x0107_3d57;
const BAD: u32 = 14;
const OOM: u32 = 2;
const NO_INTERFACE: u32 = 3;
const UNSUPPORTED: u32 = 20;
const HEAD: u32 = 4;
const NEXT_ID: u32 = 8;
const FOCUS: u32 = 12;
// Window node: dados públicos copiados, sem empréstimo do propList do chamador.
const N_NEXT: u32 = 0;
const N_WIDGET: u32 = 4;
const N_IDENTITY: u32 = 8;
const N_ID: u32 = 12;
const N_LIVE: u32 = 16;
const N_OP: u32 = 20;
const N_CLASS: u32 = 24;
const N_X: u32 = 36;
const N_Y: u32 = 40;
const N_MIN: u32 = 44;
const N_PENDING: u32 = 48;
const NODE_BYTES: u32 = 80;
// DisplayWidget: IWidget, modelo, handler, display retido e bitmap realmente fora da tela.
const MODEL: u32 = 4;
const WIDTH: u32 = 8;
const HEIGHT: u32 = 12;
const PARENT: u32 = 16;
const HANDLER: u32 = 20;
const CONTEXT: u32 = 24;
const FREE: u32 = 28;
const DISPLAY: u32 = 32;
const BITMAP: u32 = 36;
const DESTINATION: u32 = 40;
const MANAGER: u32 = 44;
const FOCUSED: u32 = 48;
const DEFAULT_DISPLAY: u32 = 52;
const FLAGS: u32 = 56;
const CONTENT_MODEL: u32 = 60;
// ViewModel contém apenas listeners; não expõe SetIPtr/GetIPtr.
const LISTENERS: u32 = 12;
#[derive(Clone, Copy)]
struct WindowConfig {
    class: u32,
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    min: u32,
    activate: u32,
}
impl<C: CpuBackend> Machine<C> {
    fn window_invoke(&mut self, object: u32, slot: u32, args: [u32; 3]) -> Result<u32, CpuError> {
        if object == 0 {
            return Ok(0);
        }
        let entry = self.cpu.read_u32(self.cpu.read_u32(object)? + slot * 4)?;
        match self.call_guest_aninhado(entry, [object, args[0], args[1], args[2]], QSORT_BUDGET)? {
            Outcome::Returned { code } => Ok(code),
            other => Err(CpuError(format!(
                "operação de janela não retornou: {other:?}"
            ))),
        }
    }
    fn window_event(&mut self, widget: u32, event: u32, w: u32, dw: u32) -> Result<u32, CpuError> {
        self.window_invoke(widget, 3, [event, w, dw])
    }
    pub(super) fn create_window_manager(&mut self, out: u32) -> Result<u32, CpuError> {
        if out == 0 {
            return Ok(BAD);
        }
        self.cpu.write_u32(out, 0)?;
        // Uma região física tem um gerenciador; CreateInstance entrega outra referência.
        if let Some(manager) = self
            .objects
            .addresses(Interface::WindowMgr)
            .first()
            .copied()
        {
            self.objects.add_ref(manager);
            self.cpu.write_u32(out, manager)?;
            return Ok(SUCCESS);
        }
        let manager = self.new_object(Interface::WindowMgr)?;
        if manager == 0 {
            return Ok(OOM);
        }
        self.cpu.write_mem(manager + 4, &[0; 60])?;
        self.cpu.write_u32(out, manager)?;
        Ok(SUCCESS)
    }
    pub(super) fn create_display_widget(&mut self, out: u32) -> Result<u32, CpuError> {
        if out == 0 {
            return Ok(BAD);
        }
        self.cpu.write_u32(out, 0)?;
        let model = self.new_object(Interface::WindowViewModel)?;
        if model == 0 {
            return Ok(OOM);
        }
        self.cpu.write_mem(model + 4, &[0; 60])?;
        let content = self.new_object(Interface::InterfaceModel)?;
        if content == 0 {
            self.objects.release(model);
            return Ok(OOM);
        }
        self.cpu.write_mem(content + 4, &[0; 60])?;
        let widget = self.new_object(Interface::DisplayWidget)?;
        if widget == 0 {
            self.objects.release(model);
            self.objects.release(content);
            return Ok(OOM);
        }
        self.cpu.write_mem(widget + 4, &[0; 60])?;
        self.cpu.write_u32(widget + MODEL, model)?;
        self.cpu.write_u32(widget + CONTENT_MODEL, content)?;
        self.cpu
            .write_u32(widget + HANDLER, aee::encode(Interface::DisplayWidget, 14))?;
        self.cpu.write_u32(widget + CONTEXT, widget)?;
        self.cpu.write_u32(out, widget)?;
        Ok(SUCCESS)
    }
    fn window_nodes(&self, manager: u32) -> Result<Vec<u32>, CpuError> {
        let mut nodes = Vec::new();
        let mut node = self.cpu.read_u32(manager + HEAD)?;
        while node != 0 && nodes.len() < 256 && !nodes.contains(&node) {
            nodes.push(node);
            node = self.cpu.read_u32(node + N_NEXT)?;
        }
        Ok(nodes)
    }
    fn window_find(&self, manager: u32, identity: u32) -> Result<Option<u32>, CpuError> {
        Ok(self.window_nodes(manager)?.into_iter().find(|n| {
            self.cpu.read_u32(*n + N_IDENTITY).ok() == Some(identity)
                || self.cpu.read_u32(*n + N_WIDGET).ok() == Some(identity)
        }))
    }
    fn window_read_config(&self, node: u32, pending: bool) -> Result<WindowConfig, CpuError> {
        let base = node + if pending { N_PENDING } else { N_CLASS };
        Ok(WindowConfig {
            class: self.cpu.read_u32(base)?,
            width: self.cpu.read_u32(base + 4)?,
            height: self.cpu.read_u32(base + 8)?,
            x: self.cpu.read_u32(base + 12)? as i32,
            y: self.cpu.read_u32(base + 16)? as i32,
            min: self.cpu.read_u32(base + 20)?,
            activate: 0,
        })
    }
    fn window_write_config(
        &mut self,
        node: u32,
        pending: bool,
        c: WindowConfig,
    ) -> Result<(), CpuError> {
        let base = node + if pending { N_PENDING } else { N_CLASS };
        for (i, value) in [
            c.class, c.width, c.height, c.x as u32, c.y as u32, c.min, c.activate,
        ]
        .into_iter()
        .enumerate()
        {
            self.cpu.write_u32(base + i as u32 * 4, value)?;
        }
        Ok(())
    }
    fn window_properties(
        &self,
        list: u32,
        count: u32,
        mut c: WindowConfig,
    ) -> Result<Result<WindowConfig, u32>, CpuError> {
        if count > 64 || (count != 0 && list == 0) {
            return Ok(Err(BAD));
        }
        for i in 0..count {
            let p = list + i * 16;
            let id = self.cpu.read_u32(p)?;
            let data = self.cpu.read_u32(p + 4)?;
            let size = self.cpu.read_u32(p + 8)?;
            let word = |offset: u32| -> Result<Option<u32>, CpuError> {
                if data == 0 || size < offset + 4 {
                    Ok(None)
                } else {
                    Ok(Some(self.cpu.read_u32(data + offset)?))
                }
            };
            match id {
                0x0106_c5ab => {
                    let Some(v) = word(0)? else {
                        return Ok(Err(BAD));
                    };
                    if !matches!(v, 1 | 2) {
                        return Ok(Err(UNSUPPORTED));
                    }
                    c.class = v;
                }
                0x0107_3d56 => {
                    if size != 0 {
                        return Ok(Err(BAD));
                    }
                    c.min = 0;
                    c.activate = 1;
                }
                0x0106_c5a4 => {
                    if size != 0 {
                        return Ok(Err(BAD));
                    }
                    c.min = 1;
                }
                0x0107_3d5c => {
                    let (Some(flags), Some(w), Some(h)) = (word(0)?, word(4)?, word(8)?) else {
                        return Ok(Err(BAD));
                    };
                    if flags & !1 != 0 {
                        return Ok(Err(BAD));
                    }
                    let width = (w as i32).max(0) as u32;
                    let height = (h as i32).max(0) as u32;
                    c.width = if flags == 1 {
                        self.screen().width() * width.min(100) / 100
                    } else {
                        width.min(self.screen().width())
                    };
                    c.height = if flags == 1 {
                        self.screen().height() * height.min(100) / 100
                    } else {
                        height.min(self.screen().height())
                    };
                }
                0x0106_c5a8 => {
                    let (Some(x), Some(y), Some(xc), Some(yc)) =
                        (word(0)?, word(4)?, word(8)?, word(12)?)
                    else {
                        return Ok(Err(BAD));
                    };
                    if xc > 2 || yc > 2 {
                        return Ok(Err(UNSUPPORTED));
                    }
                    let sw = self.screen().width() as i32;
                    let sh = self.screen().height() as i32;
                    c.x = match xc {
                        1 => x as i32,
                        2 => sw - c.width as i32 + x as i32,
                        _ => (sw - c.width as i32) / 2 + x as i32,
                    };
                    c.y = match yc {
                        1 => y as i32,
                        2 => sh - c.height as i32 + y as i32,
                        _ => (sh - c.height as i32) / 2 + y as i32,
                    };
                }
                // O único display físico não pode prometer rotação/annunciadores inexistentes.
                0x0106_c5a6 | 0x0107_0e8d => {
                    if word(0)? != Some(0) {
                        return Ok(Err(UNSUPPORTED));
                    }
                }
                _ => return Ok(Err(UNSUPPORTED)),
            }
        }
        if c.class == 1 {
            c.width = self.screen().width();
            c.height = self.screen().height();
            c.x = 0;
            c.y = 0;
        }
        Ok(Ok(c))
    }
    fn window_post(&mut self, manager: u32, node: u32, op: u32) -> Result<(), CpuError> {
        self.cpu.write_u32(node + N_OP, op)?;
        self.objects.add_ref(manager);
        self.posted_calls.push(GuestCall {
            function: aee::encode(Interface::WindowMgr, 8),
            args: [manager, node, op, 0],
        });
        Ok(())
    }
    pub(super) fn window_manager_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let this = self.arg(0);
        let (a1, a2, a3) = (self.arg(1), self.arg(2), self.arg(3));
        if self.objects.kind_of(this) != Some(Interface::WindowMgr) {
            return Ok(Some(BAD));
        }
        let value = match slot {
            0 => self.objects.add_ref(this),
            1 => {
                if self.objects.contagem(this) == 1 {
                    self.window_destroy_manager(this)?;
                }
                self.objects.release(this)
            }
            2 => {
                if a2 == 0 {
                    return Ok(Some(BAD));
                }
                let yes = matches!(a1, 0x0100_0001 | 0x0107_4e84);
                self.cpu.write_u32(a2, if yes { this } else { 0 })?;
                if yes {
                    self.objects.add_ref(this);
                    SUCCESS
                } else {
                    NO_INTERFACE
                }
            }
            3 => {
                let out = self.stack_arg(0)?;
                if out == 0 {
                    return Ok(Some(BAD));
                }
                self.cpu.write_u32(out, 0)?;
                if a1 == 0 {
                    return Ok(Some(BAD));
                }
                if self.window_find(this, a1)?.is_some() {
                    return Ok(Some(1));
                }
                let initial = WindowConfig {
                    class: 1,
                    width: self.screen().width(),
                    height: self.screen().height(),
                    x: 0,
                    y: 0,
                    min: 0,
                    activate: 1,
                };
                let config = match self.window_properties(a2, a3, initial)? {
                    Ok(v) => v,
                    Err(e) => return Ok(Some(e)),
                };
                if self.window_nodes(this)?.len() >= 256 {
                    return Ok(Some(OOM));
                }
                let scratch = self.malloc(4)?;
                if scratch == 0 {
                    return Ok(Some(OOM));
                }
                self.cpu.write_u32(scratch, 0)?;
                let qi = self.window_invoke(a1, 2, [0x0101_5952, scratch, 0]);
                let widget = self.cpu.read_u32(scratch)?;
                self.heap.free(scratch);
                if qi? != SUCCESS || widget == 0 {
                    return Ok(Some(NO_INTERFACE));
                }
                let node = self.malloc(NODE_BYTES)?;
                if node == 0 {
                    self.window_invoke(widget, 1, [0, 0, 0])?;
                    return Ok(Some(OOM));
                }
                let id = self.cpu.read_u32(this + NEXT_ID)?.checked_add(1);
                let Some(id) = id else {
                    self.heap.free(node);
                    self.window_invoke(widget, 1, [0, 0, 0])?;
                    return Ok(Some(OOM));
                };
                self.cpu.write_mem(node, &[0; NODE_BYTES as usize])?;
                self.cpu
                    .write_u32(node + N_NEXT, self.cpu.read_u32(this + HEAD)?)?;
                for (field, value) in [(N_WIDGET, widget), (N_IDENTITY, a1), (N_ID, id)] {
                    self.cpu.write_u32(node + field, value)?;
                }
                self.window_write_config(node, true, config)?;
                self.cpu.write_u32(this + NEXT_ID, id)?;
                self.cpu.write_u32(this + HEAD, node)?;
                if self.objects.kind_of(widget) == Some(Interface::DisplayWidget) {
                    self.cpu.write_u32(widget + MANAGER, this)?;
                }
                self.cpu.write_u32(out, id)?;
                self.window_post(this, node, 1)?;
                SUCCESS
            }
            4 => {
                let Some(node) = self.window_find(this, a1)? else {
                    return Ok(Some(BAD));
                };
                if self.cpu.read_u32(node + N_LIVE)? == 0 || self.cpu.read_u32(node + N_OP)? != 0 {
                    return Ok(Some(1));
                }
                let initial = self.window_read_config(node, false)?;
                let config = match self.window_properties(a2, a3, initial)? {
                    Ok(v) => v,
                    Err(e) => return Ok(Some(e)),
                };
                self.window_write_config(node, true, config)?;
                self.window_post(this, node, 4)?;
                SUCCESS
            }
            5 => {
                if a3 == 0 || a2 > 256 || (a2 != 0 && a1 == 0) {
                    return Ok(Some(BAD));
                }
                let mut yes = false;
                for at in 0..a2 {
                    yes |= self.cpu.read_u32(a1 + at * 4)? == CAPABILITY;
                }
                self.cpu.write_mem(a3, &[u8::from(yes)])?;
                SUCCESS
            }
            6 => {
                if a3 == 0 || (a2 as i32) < 0 || (a2 != 0 && a1 == 0) {
                    return Ok(Some(BAD));
                }
                self.cpu.write_u32(a3, 1)?;
                if a2 != 0 {
                    self.cpu.write_u32(a1, CAPABILITY)?;
                }
                SUCCESS
            }
            7 => {
                let Some(node) = self.window_find(this, a1)? else {
                    return Ok(Some(BAD));
                };
                if self.cpu.read_u32(node + N_LIVE)? == 0 || self.cpu.read_u32(node + N_OP)? != 0 {
                    return Ok(Some(1));
                }
                self.window_post(this, node, 2)?;
                SUCCESS
            }
            8 => {
                self.window_complete(this, a1, a2)?;
                if self.objects.contagem(this) == 1 {
                    self.window_destroy_manager(this)?;
                }
                self.objects.release(this);
                SUCCESS
            }
            _ => return Ok(None),
        };
        Ok(Some(value))
    }
    fn window_set_extended(&mut self, widget: u32, id: u32, value: u32) -> Result<(), CpuError> {
        let p = self.malloc(16)?;
        if p == 0 {
            return Ok(());
        }
        self.cpu.write_u32(p, id)?;
        self.cpu.write_u32(p + 4, 4)?;
        self.cpu.write_u32(p + 8, p + 12)?;
        self.cpu.write_u32(p + 12, value)?;
        let result = self.window_event(widget, 0x801, 0x600, p);
        self.heap.free(p);
        result?;
        Ok(())
    }
    fn window_change_focus(&mut self, manager: u32) -> Result<(), CpuError> {
        let mut next = 0;
        for node in self.window_nodes(manager)? {
            if self.cpu.read_u32(node + N_LIVE)? == 0 || self.cpu.read_u32(node + N_MIN)? != 0 {
                continue;
            }
            let widget = self.cpu.read_u32(node + N_WIDGET)?;
            let p = self.malloc(4)?;
            if p == 0 {
                break;
            }
            self.cpu.write_u32(p, 0)?;
            self.window_event(widget, 0x702, 0, p)?;
            let mut yes = [0];
            self.cpu.read_mem(p, &mut yes)?;
            self.heap.free(p);
            if yes[0] != 0 {
                next = widget;
                break;
            }
        }
        let old = self.cpu.read_u32(manager + FOCUS)?;
        if next != old {
            self.cpu.write_u32(manager + FOCUS, next)?;
            if old != 0 {
                self.window_event(old, 0x700, 0, 0)?;
            }
            if next != 0 {
                self.window_event(next, 0x700, 1, 0)?;
            }
        }
        Ok(())
    }
    fn window_complete(&mut self, manager: u32, node: u32, op: u32) -> Result<(), CpuError> {
        if !self.window_nodes(manager)?.contains(&node) || self.cpu.read_u32(node + N_OP)? != op {
            return Ok(());
        }
        let widget = self.cpu.read_u32(node + N_WIDGET)?;
        self.cpu.write_u32(node + N_OP, 0)?;
        if op == 2 {
            self.cpu.write_u32(node + N_LIVE, 0)?;
            self.window_change_focus(manager)?;
            self.window_event(widget, 0x750, 2, 0)?;
            self.window_unlink(manager, node)?;
            self.window_compose(manager)?;
            return Ok(());
        }
        let config = self.window_read_config(node, true)?;
        let activate = self.cpu.read_u32(node + N_PENDING + 24)?;
        self.window_write_config(node, false, config)?;
        self.cpu.write_u32(node + N_LIVE, 1)?;
        if activate != 0 {
            self.window_raise(manager, node)?;
        }
        let extent = self.malloc(8)?;
        if extent == 0 {
            self.window_event(widget, 0x750, op, OOM)?;
            return Ok(());
        }
        self.cpu.write_u32(extent, config.width)?;
        self.cpu.write_u32(extent + 4, config.height)?;
        let result = self.window_invoke(widget, 7, [extent, 0, 0]);
        self.heap.free(extent);
        // IWidget::SetExtent retorna void: R0 residual não é AEEResult.
        result?;
        self.window_set_extended(widget, 0x0107_0e93, if config.min != 0 { 4 } else { 1 })?;
        self.window_set_extended(widget, 0x0107_0e8e, 0)?;
        self.window_change_focus(manager)?;
        self.window_event(widget, 0x750, op, SUCCESS)?;
        self.window_compose(manager)?;
        Ok(())
    }
    fn window_raise(&mut self, manager: u32, node: u32) -> Result<(), CpuError> {
        let nodes = self.window_nodes(manager)?;
        if nodes.first() == Some(&node) {
            return Ok(());
        }
        if let Some(prev) = nodes
            .into_iter()
            .find(|p| self.cpu.read_u32(*p + N_NEXT).ok() == Some(node))
        {
            self.cpu
                .write_u32(prev + N_NEXT, self.cpu.read_u32(node + N_NEXT)?)?;
            self.cpu
                .write_u32(node + N_NEXT, self.cpu.read_u32(manager + HEAD)?)?;
            self.cpu.write_u32(manager + HEAD, node)?;
        }
        Ok(())
    }
    fn window_unlink(&mut self, manager: u32, node: u32) -> Result<(), CpuError> {
        let next = self.cpu.read_u32(node + N_NEXT)?;
        if self.cpu.read_u32(manager + HEAD)? == node {
            self.cpu.write_u32(manager + HEAD, next)?;
        } else if let Some(prev) = self
            .window_nodes(manager)?
            .into_iter()
            .find(|p| self.cpu.read_u32(*p + N_NEXT).ok() == Some(node))
        {
            self.cpu.write_u32(prev + N_NEXT, next)?;
        }
        let widget = self.cpu.read_u32(node + N_WIDGET)?;
        if self.objects.kind_of(widget) == Some(Interface::DisplayWidget) {
            self.cpu.write_u32(widget + MANAGER, 0)?;
        }
        self.heap.free(node);
        self.window_invoke(widget, 1, [0, 0, 0])?;
        Ok(())
    }
    fn window_destroy_manager(&mut self, manager: u32) -> Result<(), CpuError> {
        self.cpu.write_u32(manager + FOCUS, 0)?;
        for node in self.window_nodes(manager)? {
            let widget = self.cpu.read_u32(node + N_WIDGET)?;
            self.window_event(widget, 0x700, 0, 0)?;
            self.window_event(widget, 0x750, 2, 0)?;
            self.window_unlink(manager, node)?;
        }
        Ok(())
    }
    fn window_compose(&mut self, manager: u32) -> Result<(), CpuError> {
        let nodes = self.window_nodes(manager)?;
        let mut visible = Vec::new();
        for node in nodes {
            if self.cpu.read_u32(node + N_LIVE)? != 0 && self.cpu.read_u32(node + N_MIN)? == 0 {
                visible.push(node);
                if self.cpu.read_u32(node + N_CLASS)? == 1 {
                    break;
                }
            }
        }
        let screen = self.device_bitmap()?;
        let canvas = self.new_object(Interface::Canvas)?;
        if canvas == 0 {
            return Ok(());
        }
        self.canvases.insert(canvas, screen);
        let old_target = self.display_target;
        let old_clip = self.clip;
        self.display_target = screen;
        self.clip = None;
        let result = (|| {
            if let Some(fb) = self.bitmaps.get_mut(&screen) {
                fb.fill_rect(
                    Rect {
                        x: 0,
                        y: 0,
                        width: fb.width() as i16,
                        height: fb.height() as i16,
                    },
                    Rgb::BLACK,
                );
            }
            for node in visible.into_iter().rev() {
                self.window_invoke(
                    self.cpu.read_u32(node + N_WIDGET)?,
                    10,
                    [
                        canvas,
                        self.cpu.read_u32(node + N_X)?,
                        self.cpu.read_u32(node + N_Y)?,
                    ],
                )?;
            }
            Ok(())
        })();
        self.display_target = old_target;
        self.clip = old_clip;
        self.canvases.remove(&canvas);
        self.objects.release(canvas);
        result
    }
    pub(super) fn window_input(
        &mut self,
        event: u32,
        w: u32,
        dw: u32,
    ) -> Result<Option<u32>, CpuError> {
        for manager in self.objects.addresses(Interface::WindowMgr) {
            let focused = self.cpu.read_u32(manager + FOCUS)?;
            if focused != 0 {
                let result = self.window_event(focused, event, w, dw)?;
                if result != 0 {
                    return Ok(Some(result));
                }
            }
        }
        Ok(None)
    }
    fn window_cancel_listener(&mut self, l: u32) -> Result<(), CpuError> {
        let model = self.cpu.read_u32(l + 20)?;
        if self.objects.kind_of(model) != Some(Interface::WindowViewModel) {
            return Ok(());
        }
        let prev = self.cpu.read_u32(l + 4)?;
        let next = self.cpu.read_u32(l)?;
        if prev == 0 {
            if self.cpu.read_u32(model + LISTENERS)? == l {
                self.cpu.write_u32(model + LISTENERS, next)?;
            }
        } else {
            self.cpu.write_u32(prev, next)?;
        }
        if next != 0 {
            self.cpu.write_u32(next + 4, prev)?;
        }
        for at in [0, 4, 16, 20] {
            self.cpu.write_u32(l + at, 0)?;
        }
        Ok(())
    }
    fn window_notify_model(&mut self, model: u32, event: u32) -> Result<(), CpuError> {
        let mut records = Vec::new();
        let mut l = self.cpu.read_u32(model + LISTENERS)?;
        while l != 0 && records.len() < 1024 && !records.contains(&l) {
            records.push(l);
            l = self.cpu.read_u32(l)?;
        }
        for l in records {
            if self.cpu.read_u32(l + 20)? != model || self.cpu.read_u32(l + 16)? == 0 {
                continue;
            }
            let f = self.cpu.read_u32(l + 8)?;
            if f != 0 {
                let ctx = self.cpu.read_u32(l + 12)?;
                match self.call_guest_aninhado(f, [ctx, event, 0, 0], QSORT_BUDGET)? {
                    Outcome::Returned { .. } => {}
                    x => return Err(CpuError(format!("listener de janela não retornou: {x:?}"))),
                }
            }
        }
        Ok(())
    }
    pub(super) fn window_view_model_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let this = self.arg(0);
        let (a1, a2) = (self.arg(1), self.arg(2));
        if slot == 5 {
            self.window_cancel_listener(this)?;
            return Ok(Some(SUCCESS));
        }
        if self.objects.kind_of(this) != Some(Interface::WindowViewModel) {
            return Ok(Some(BAD));
        }
        let value = match slot {
            0 => self.objects.add_ref(this),
            1 => {
                if self.objects.contagem(this) == 1 {
                    while self.cpu.read_u32(this + LISTENERS)? != 0 {
                        self.window_cancel_listener(self.cpu.read_u32(this + LISTENERS)?)?;
                    }
                }
                self.objects.release(this)
            }
            2 => {
                if a2 == 0 {
                    return Ok(Some(BAD));
                }
                let yes = matches!(a1, 0x0100_0001 | 0x0101_593a);
                self.cpu.write_u32(a2, if yes { this } else { 0 })?;
                if yes {
                    self.objects.add_ref(this);
                    SUCCESS
                } else {
                    NO_INTERFACE
                }
            }
            3 => {
                if a1 == 0 || self.cpu.read_u32(a1 + 8)? == 0 {
                    return Ok(Some(BAD));
                }
                let cancel = self.cpu.read_u32(a1 + 16)?;
                if cancel != 0 {
                    match self.call_guest_aninhado(cancel, [a1, 0, 0, 0], QSORT_BUDGET)? {
                        Outcome::Returned { .. } => {}
                        _ => return Ok(Some(1)),
                    }
                }
                let old = self.cpu.read_u32(this + LISTENERS)?;
                self.cpu.write_u32(a1, old)?;
                self.cpu.write_u32(a1 + 4, 0)?;
                if old != 0 {
                    self.cpu.write_u32(old + 4, a1)?;
                }
                self.cpu
                    .write_u32(a1 + 16, aee::encode(Interface::WindowViewModel, 5))?;
                self.cpu.write_u32(a1 + 20, this)?;
                self.cpu.write_u32(this + LISTENERS, a1)?;
                SUCCESS
            }
            4 => {
                if a1 == 0 {
                    return Ok(Some(BAD));
                }
                self.window_notify_model(this, a1)?;
                SUCCESS
            }
            _ => return Ok(None),
        };
        Ok(Some(value))
    }
    fn window_release_ref(&mut self, object: u32) -> Result<(), CpuError> {
        if object != 0 {
            self.window_invoke(object, 1, [0, 0, 0])?;
        }
        Ok(())
    }
    fn window_binding(&self, display: u32) -> Option<u32> {
        self.objects
            .addresses(Interface::DisplayWidget)
            .into_iter()
            .find(|w| self.cpu.read_u32(*w + DISPLAY).ok() == Some(display))
    }
    fn window_bind_display(&mut self, widget: u32, display: u32) -> Result<u32, CpuError> {
        if self.objects.kind_of(display) != Some(Interface::Display) {
            return Ok(FALSE);
        }
        if self.cpu.read_u32(widget + DISPLAY)? == display {
            return Ok(TRUE);
        }
        if self.window_binding(display).is_some() {
            return Ok(FALSE);
        }
        let screen = self.device_bitmap()?;
        let (width, height) = self
            .bitmaps
            .get(&screen)
            .map(|s| (s.width(), s.height()))
            .unwrap_or((0, 0));
        let bitmap = self.new_object(Interface::Bitmap)?;
        if bitmap == 0 {
            return Ok(FALSE);
        }
        self.bitmaps.insert(bitmap, Framebuffer::new(width, height));
        self.objects.add_ref(display);
        self.objects.add_ref(bitmap);
        let old_display = self.cpu.read_u32(widget + DISPLAY)?;
        let old_bitmap = self.cpu.read_u32(widget + BITMAP)?;
        let old_dest = self.cpu.read_u32(widget + DESTINATION)?;
        self.cpu.write_u32(widget + DISPLAY, display)?;
        self.cpu.write_u32(widget + BITMAP, bitmap)?;
        self.cpu.write_u32(widget + DESTINATION, bitmap)?;
        // O modelo de dados do BitmapWidget é distinto de seu ViewModel.
        let content = self.cpu.read_u32(widget + CONTENT_MODEL)?;
        self.window_invoke(content, 5, [bitmap, 0x0100_1045, 0])?;
        let is_default = self.applet_display == 0 || self.applet_display == display;
        self.cpu
            .write_u32(widget + DEFAULT_DISPLAY, u32::from(is_default))?;
        if is_default {
            self.display_target = bitmap;
        }
        self.window_release_ref(old_dest)?;
        self.window_release_ref(old_bitmap)?;
        self.window_release_ref(old_display)?;
        Ok(TRUE)
    }
    pub(super) fn window_bound_display_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let display = self.arg(0);
        let Some(widget) = self.window_binding(display) else {
            return self.display_call_unwrapped(slot);
        };
        let old = self.display_target;
        self.display_target = self.cpu.read_u32(widget + DESTINATION)?;
        // NULL volta ao bitmap do display envolvido, que deixou de ser a tela física.
        let reset_destination =
            Interface::Display.method(slot) == Some("SetDestination") && self.arg(1) == 0;
        if reset_destination {
            self.cpu
                .write_reg(Reg::R1, self.cpu.read_u32(widget + BITMAP)?);
        }
        let update = matches!(Interface::Display.method(slot), Some("Update" | "UpdateEx"));
        let manager = self.cpu.read_u32(widget + MANAGER)?;
        let result = (|| {
            if update && self.objects.kind_of(manager) == Some(Interface::WindowMgr) {
                self.window_compose(manager)?;
            }
            self.display_call_unwrapped(slot)
        })();
        if reset_destination {
            self.cpu.write_reg(Reg::R1, 0);
        }
        // SetDestination é por IDisplay; o destino padrão também alimenta IImage/IGraphics.
        self.cpu
            .write_u32(widget + DESTINATION, self.display_target)?;
        if self.cpu.read_u32(widget + DEFAULT_DISPLAY)? == 0 {
            self.display_target = old;
        }
        result
    }
    fn window_default_event(
        &mut self,
        widget: u32,
        event: u32,
        w: u32,
        dw: u32,
    ) -> Result<u32, CpuError> {
        match (event, w) {
            (0x702, _) => {
                if dw != 0 {
                    self.cpu.write_mem(dw, &[1])?;
                }
                Ok(TRUE)
            }
            (0x700, _) => {
                self.cpu.write_u32(widget + FOCUSED, u32::from(w != 0))?;
                Ok(TRUE)
            }
            (0x701, _) => {
                if dw != 0 {
                    self.cpu
                        .write_mem(dw, &[u8::from(self.cpu.read_u32(widget + FOCUSED)? != 0)])?;
                }
                Ok(TRUE)
            }
            (0x800, 0x161) => {
                if dw == 0 {
                    return Ok(FALSE);
                }
                let model = self.cpu.read_u32(widget + MODEL)?;
                self.objects.add_ref(model);
                self.cpu.write_u32(dw, model)?;
                Ok(TRUE)
            }
            (0x801, 0x600) => {
                if dw == 0 {
                    return Ok(FALSE);
                }
                let id = self.cpu.read_u32(dw)?;
                if id == 0x0103_f104 {
                    self.window_bind_display(widget, self.cpu.read_u32(dw + 8)?)
                } else {
                    Ok(FALSE)
                }
            }
            (0x800 | 0x801, 0x153) => {
                if event == 0x800 {
                    if dw == 0 {
                        return Ok(FALSE);
                    }
                    self.cpu.write_u32(dw, self.cpu.read_u32(widget + FLAGS)?)?;
                } else {
                    self.cpu.write_u32(widget + FLAGS, dw)?;
                }
                Ok(TRUE)
            }
            _ => Ok(FALSE),
        }
    }
    pub(super) fn display_widget_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let this = self.arg(0);
        let (a1, a2, a3) = (self.arg(1), self.arg(2), self.arg(3));
        if self.objects.kind_of(this) != Some(Interface::DisplayWidget) {
            return Ok(Some(BAD));
        }
        let value = match slot {
            0 => self.objects.add_ref(this),
            1 => {
                if self.objects.contagem(this) == 1 {
                    let dest = self.cpu.read_u32(this + DESTINATION)?;
                    if self.cpu.read_u32(this + DEFAULT_DISPLAY)? != 0
                        && self.display_target == dest
                    {
                        self.display_target = self.device_bitmap()?;
                    }
                    for field in [MODEL, CONTENT_MODEL, DESTINATION, BITMAP, DISPLAY] {
                        let o = self.cpu.read_u32(this + field)?;
                        self.cpu.write_u32(this + field, 0)?;
                        self.window_release_ref(o)?;
                    }
                    let free = self.cpu.read_u32(this + FREE)?;
                    if free != 0 {
                        let context = self.cpu.read_u32(this + CONTEXT)?;
                        self.call_guest_aninhado(free, [context, 0, 0, 0], QSORT_BUDGET)?;
                    }
                }
                self.objects.release(this)
            }
            2 => {
                if a2 == 0 {
                    return Ok(Some(BAD));
                }
                let yes = matches!(a1, 0x0100_0001 | 0x0101_5952 | 0x0101_5956);
                self.cpu.write_u32(a2, if yes { this } else { 0 })?;
                if yes {
                    self.objects.add_ref(this);
                    SUCCESS
                } else {
                    NO_INTERFACE
                }
            }
            3 => {
                let f = self.cpu.read_u32(this + HANDLER)?;
                let ctx = self.cpu.read_u32(this + CONTEXT)?;
                match self.call_guest_aninhado(f, [ctx, a1, a2, a3], QSORT_BUDGET)? {
                    Outcome::Returned { code } => code,
                    _ => FALSE,
                }
            }
            4 => {
                if a1 == 0 {
                    return Ok(Some(BAD));
                }
                let values = [
                    self.cpu.read_u32(a1)?,
                    self.cpu.read_u32(a1 + 4)?,
                    self.cpu.read_u32(a1 + 8)?,
                ];
                for (i, field) in [HANDLER, CONTEXT, FREE].into_iter().enumerate() {
                    let old = self.cpu.read_u32(this + field)?;
                    self.cpu.write_u32(this + field, values[i])?;
                    self.cpu.write_u32(a1 + i as u32 * 4, old)?;
                }
                SUCCESS
            }
            5 | 6 => {
                if a1 == 0 {
                    return Ok(Some(BAD));
                }
                let size = if slot == 5 {
                    (self.screen().width(), self.screen().height())
                } else {
                    (
                        self.cpu.read_u32(this + WIDTH)?,
                        self.cpu.read_u32(this + HEIGHT)?,
                    )
                };
                self.cpu.write_u32(a1, size.0)?;
                self.cpu.write_u32(a1 + 4, size.1)?;
                SUCCESS
            }
            7 => {
                if a1 == 0 {
                    return Ok(Some(BAD));
                }
                let width = self.cpu.read_u32(a1)?;
                let height = self.cpu.read_u32(a1 + 4)?;
                if width > i16::MAX as u32 || height > i16::MAX as u32 {
                    return Ok(Some(BAD));
                }
                if u64::from(width) * u64::from(height) > 4096 * 4096 {
                    return Ok(Some(OOM));
                }
                let changed = self.cpu.read_u32(this + WIDTH)? != width
                    || self.cpu.read_u32(this + HEIGHT)? != height;
                let event = if changed { self.malloc(20)? } else { 0 };
                if changed && event == 0 {
                    return Ok(Some(OOM));
                }
                self.cpu.write_u32(this + WIDTH, width)?;
                self.cpu.write_u32(this + HEIGHT, height)?;
                if changed {
                    let bitmap = self.cpu.read_u32(this + BITMAP)?;
                    let resize = self
                        .bitmaps
                        .get(&bitmap)
                        .is_some_and(|fb| fb.width() != width || fb.height() != height);
                    if resize {
                        self.solta_dib(bitmap);
                    }
                    if let Some(fb) = self.bitmaps.get_mut(&bitmap) {
                        if fb.width() != width || fb.height() != height {
                            *fb = Framebuffer::new(width, height);
                        }
                    }
                    let model = self.cpu.read_u32(this + MODEL)?;
                    let p = event;
                    for (at, v) in [
                        (0, 0x100c),
                        (4, model),
                        (8, p + 12),
                        (12, width),
                        (16, height),
                    ] {
                        self.cpu.write_u32(p + at, v)?;
                    }
                    let result = self.window_notify_model(model, p);
                    self.heap.free(p);
                    result?;
                }
                SUCCESS
            }
            8 => {
                if a1 == 0 {
                    return Ok(Some(BAD));
                }
                let p = self.cpu.read_u32(this + PARENT)?;
                if p != 0 {
                    self.window_invoke(p, 0, [0, 0, 0])?;
                }
                self.cpu.write_u32(a1, p)?;
                SUCCESS
            }
            9 => {
                self.cpu.write_u32(this + PARENT, a1)?;
                SUCCESS
            }
            10 => {
                let content = self.cpu.read_u32(this + CONTENT_MODEL)?;
                if self.cpu.read_u32(content + 8)? != 0x0100_1045 {
                    return Ok(Some(SUCCESS));
                }
                let source = self.cpu.read_u32(content + 4)?;
                if let Some(&target) = self.canvases.get(&a1) {
                    self.sync_from_guest(source)?;
                    let source_fb = self
                        .bitmaps
                        .get(&source)
                        .map(|fb| (fb.width(), fb.height(), fb.pixels().to_vec()));
                    if let (Some((width, height, pixels)), Some(dst)) =
                        (source_fb, self.bitmaps.get_mut(&target))
                    {
                        for y in 0..height {
                            for x in 0..width {
                                dst.set_pixel_native(
                                    (a2 as i32).saturating_add(x as i32),
                                    (a3 as i32).saturating_add(y as i32),
                                    pixels[(y * width + x) as usize],
                                );
                            }
                        }
                    }
                }
                SUCCESS
            }
            11 => FALSE,
            12 => {
                if a2 == 0 {
                    return Ok(Some(BAD));
                }
                let yes = matches!(a1, 0x0101_593a | 0x0101_593c);
                let model = self.cpu.read_u32(this + CONTENT_MODEL)?;
                self.cpu.write_u32(a2, if yes { model } else { 0 })?;
                if yes {
                    self.objects.add_ref(model);
                    SUCCESS
                } else {
                    NO_INTERFACE
                }
            }
            13 => UNSUPPORTED,
            14 => self.window_default_event(this, a1, a2, a3)?,
            _ => return Ok(None),
        };
        Ok(Some(value))
    }
}
