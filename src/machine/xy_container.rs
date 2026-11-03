//! CXYContainer público: facets distintas e filhos posicionados em coordenadas absolutas.
//! O estado privado fica no heap guest; ObjectStore e heap já participam do save state.
use super::*;

pub(super) const CLASS_ID: u32 = 0x0105_51ed;
const BAD: u32 = 14;
const NOT_SUPPORTED: u32 = 3;
const UNSUPPORTED: u32 = 20;
const NO_MEMORY: u32 = 2;
const STATE_SIZE: u32 = 128;
// Dono: +4 facet IWidget, +8 estado; facet: +4 dono. Não são offsets OEM.
const HEAD: u32 = 0;
const TAIL: u32 = 4;
const PARENT: u32 = 8;
const WIDTH: u32 = 12;
const HEIGHT: u32 = 16;
const FOCUS: u32 = 20;
const FLAGS: u32 = 24;
const HANDLER: u32 = 28;
const CONTEXT: u32 = 32;
const FREE: u32 = 36;
const FOCUSED: u32 = 40;
const LAYOUT: u32 = 44;
const COUNT: u32 = 48;
const PENDING: u32 = 52;
const BG_ENABLED: u32 = 56;
const BG_ACTIVE: u32 = 60;
const BG_INACTIVE: u32 = 64;
const BG_SELECTED_ACTIVE: u32 = 68;
const BG_SELECTED_INACTIVE: u32 = 72;
const SELECTED: u32 = 76;
const BG_IMAGE: u32 = 80;
const BG_FRAME: u32 = 84;
const KEY_WIDGET: u32 = 88;
const KEY_CODE: u32 = 92;
// Nó: next, prev, IWidget*, x, y, boolean; 24 bytes privados.
#[derive(Clone, Copy)]
struct Child {
    node: u32,
    widget: u32,
    x: i32,
    y: i32,
    visible: bool,
}

impl<C: CpuBackend> Machine<C> {
    fn xy_valid(&self, p: u32, n: usize) -> bool {
        p != 0 && p.checked_add(n as u32).is_some() && self.cpu.read_mem(p, &mut vec![0; n]).is_ok()
    }
    fn xy_write(&mut self, p: u32, bytes: &[u8]) -> bool {
        self.xy_valid(p, bytes.len()) && self.cpu.write_mem(p, bytes).is_ok()
    }
    fn xy_owner(&self, iface: Interface, this: u32) -> Option<(u32, u32)> {
        if self.objects.kind_of(this) != Some(iface) {
            return None;
        }
        let owner = if iface == Interface::XYContainer {
            this
        } else {
            self.cpu.read_u32(this + 4).ok()?
        };
        if self.objects.kind_of(owner) != Some(Interface::XYContainer) {
            return None;
        }
        let state = self.cpu.read_u32(owner + 8).ok()?;
        if self.heap.size_of(state).is_none_or(|n| n < STATE_SIZE) {
            return None;
        }
        Some((owner, state))
    }
    fn xy_children(&self, state: u32) -> Result<Vec<Child>, CpuError> {
        let mut out = Vec::new();
        let mut node = self.cpu.read_u32(state + HEAD)?;
        while node != 0 {
            if out.len() >= 1024
                || self.heap.size_of(node).is_none_or(|n| n < 24)
                || out.iter().any(|c: &Child| c.node == node)
            {
                return Err(CpuError("lista privada de CXYContainer corrompida".into()));
            }
            out.push(Child {
                node,
                widget: self.cpu.read_u32(node + 8)?,
                x: self.cpu.read_u32(node + 12)? as i32,
                y: self.cpu.read_u32(node + 16)? as i32,
                visible: self.cpu.read_u32(node + 20)? != 0,
            });
            node = self.cpu.read_u32(node)?;
        }
        Ok(out)
    }
    fn xy_target(children: &[Child], widget: u32) -> Option<Child> {
        match widget {
            0 | 1 => children.last().copied(),
            u32::MAX => children.first().copied(),
            _ => children.iter().find(|c| c.widget == widget).copied(),
        }
    }
    fn xy_virtual(
        &mut self,
        object: u32,
        slot: u32,
        args: [u32; 3],
        extra: Option<u32>,
    ) -> Result<u32, CpuError> {
        if object == 0 {
            return Ok(0);
        }
        let vt = self.cpu.read_u32(object)?;
        let entry = self.cpu.read_u32(vt + slot * 4)?;
        if entry == 0 {
            return Ok(UNSUPPORTED);
        }
        let sp = self.cpu.read_reg(Reg::Sp);
        if let Some(value) = extra {
            let next = sp
                .checked_sub(8)
                .ok_or_else(|| CpuError("pilha de CXYContainer inválida".into()))?;
            self.cpu.write_u32(next, value)?;
            self.cpu.write_reg(Reg::Sp, next);
        }
        let result =
            self.call_guest_aninhado(entry, [object, args[0], args[1], args[2]], QSORT_BUDGET);
        self.cpu.write_reg(Reg::Sp, sp);
        match result? {
            Outcome::Returned { code } => Ok(code),
            other => Err(CpuError(format!(
                "chamada filha de CXYContainer interrompida: {other:?}"
            ))),
        }
    }
    fn xy_scratch<T>(
        &mut self,
        size: u32,
        f: impl FnOnce(&mut Self, u32) -> Result<T, CpuError>,
    ) -> Result<T, CpuError> {
        let p = self.malloc(size)?;
        if p == 0 {
            return Err(CpuError("sem memória temporária para CXYContainer".into()));
        }
        self.cpu.write_mem(p, &vec![0; size as usize])?;
        let result = f(self, p);
        self.heap.free(p);
        result
    }
    pub(super) fn create_xy_container(&mut self, out: u32) -> Result<u32, CpuError> {
        if !self.xy_write(out, &0u32.to_le_bytes()) {
            return Ok(BAD);
        }
        let owner = self.new_object(Interface::XYContainer)?;
        if owner == 0 {
            return Ok(NO_MEMORY);
        }
        let widget = self.new_object(Interface::XYContainerWidget)?;
        if widget == 0 {
            self.objects.release(owner);
            return Ok(NO_MEMORY);
        }
        let state = self.malloc(STATE_SIZE)?;
        if state == 0 {
            self.objects.release(widget);
            self.objects.release(owner);
            return Ok(NO_MEMORY);
        }
        self.cpu.write_mem(owner + 4, &[0; 60])?;
        self.cpu.write_mem(widget + 4, &[0; 60])?;
        self.cpu.write_mem(state, &[0; STATE_SIZE as usize])?;
        self.cpu.write_u32(owner + 4, widget)?;
        self.cpu.write_u32(owner + 8, state)?;
        self.cpu.write_u32(widget + 4, owner)?;
        self.cpu.write_u32(state + LAYOUT, 1)?;
        // A função anterior devolvida por SetHandler deve permitir encadeamento.
        self.cpu.write_u32(
            state + HANDLER,
            aee::encode(Interface::XYContainerWidget, 14),
        )?;
        self.cpu.write_u32(state + CONTEXT, owner)?;
        self.cpu.write_u32(out, owner)?;
        Ok(SUCCESS)
    }
    fn xy_relink(&mut self, state: u32, children: &[Child]) -> Result<(), CpuError> {
        self.cpu
            .write_u32(state + HEAD, children.first().map_or(0, |c| c.node))?;
        self.cpu
            .write_u32(state + TAIL, children.last().map_or(0, |c| c.node))?;
        self.cpu.write_u32(state + COUNT, children.len() as u32)?;
        for (at, c) in children.iter().enumerate() {
            self.cpu
                .write_u32(c.node, children.get(at + 1).map_or(0, |c| c.node))?;
            self.cpu
                .write_u32(c.node + 4, if at == 0 { 0 } else { children[at - 1].node })?;
        }
        Ok(())
    }
    fn xy_placement(children: &[Child], before: u32) -> Option<usize> {
        match before {
            0 | 1 => Some(children.len()),
            u32::MAX => Some(0),
            _ => children.iter().position(|c| c.widget == before),
        }
    }
    fn xy_focusable(&mut self, child: Child) -> Result<bool, CpuError> {
        if !child.visible {
            return Ok(false);
        }
        // O handler pode remover o próprio filho; a chamada precisa mantê-lo vivo.
        self.xy_virtual(child.widget, 0, [0; 3], None)?;
        let result = self.xy_scratch(8, |m, p| {
            let handled = m.xy_virtual(child.widget, 3, [0x702, 0, p], None)?;
            let mut b = [0];
            m.cpu.read_mem(p, &mut b)?;
            Ok(handled != 0 && b[0] != 0)
        });
        self.xy_virtual(child.widget, 1, [0; 3], None)?;
        result
    }
    fn xy_move_focus(&mut self, state: u32, target: u32) -> Result<bool, CpuError> {
        let children = self.xy_children(state)?;
        let old = self.cpu.read_u32(state + FOCUS)?;
        let mut candidates = children.clone();
        match target {
            0 => candidates.clear(),
            1 => {}
            2 => candidates.reverse(),
            3 | 4 => {
                let current = candidates.iter().position(|c| c.widget == old);
                if target == 3 {
                    if let Some(at) = current {
                        candidates = candidates[at + 1..].to_vec();
                    }
                } else {
                    if let Some(at) = current {
                        candidates.truncate(at);
                    }
                    candidates.reverse();
                }
            }
            _ => candidates.retain(|c| c.widget == target),
        }
        let mut next = 0;
        for child in candidates {
            if !self
                .xy_children(state)?
                .iter()
                .any(|c| c.node == child.node && c.widget == child.widget && c.visible)
            {
                continue;
            }
            if self.xy_focusable(child)?
                && self
                    .xy_children(state)?
                    .iter()
                    .any(|c| c.node == child.node && c.widget == child.widget && c.visible)
            {
                next = child.widget;
                self.xy_virtual(next, 0, [0; 3], None)?;
                break;
            }
        }
        if next == 0 && target != 0 {
            return Ok(false);
        }
        let transition = (|| {
            // A consulta anterior pode ter alterado foco e lista de filhos.
            let current = self.cpu.read_u32(state + FOCUS)?;
            if next != current {
                self.cpu.write_u32(state + FOCUS, 0)?;
                if current != 0 {
                    self.xy_virtual(current, 3, [0x700, 0, 0], None)?;
                }
                if next != 0
                    && !self
                        .xy_children(state)?
                        .iter()
                        .any(|c| c.widget == next && c.visible)
                {
                    return Ok(false);
                }
                self.cpu.write_u32(state + FOCUS, next)?;
                if next != 0 {
                    self.xy_virtual(next, 3, [0x700, 1, 0], None)?;
                }
            }
            Ok(self.cpu.read_u32(state + FOCUS)? == next)
        })();
        if next != 0 {
            self.xy_virtual(next, 1, [0; 3], None)?;
        }
        transition
    }
    fn xy_changed(&mut self, owner: u32, state: u32) -> Result<(), CpuError> {
        self.cpu.write_u32(state + PENDING, 1)?;
        if self.cpu.read_u32(state + LAYOUT)? == 0 {
            return Ok(());
        }
        let parent = self.cpu.read_u32(state + PARENT)?;
        if parent != 0 {
            let widget = self.cpu.read_u32(owner + 4)?;
            self.xy_virtual(parent, 3, [widget, 0, 3], None)?;
        }
        self.cpu.write_u32(state + PENDING, 0)?;
        Ok(())
    }
    fn xy_remove(&mut self, owner: u32, state: u32, target: u32) -> Result<u32, CpuError> {
        let mut children = self.xy_children(state)?;
        let Some(child) = Self::xy_target(&children, target) else {
            return Ok(BAD);
        };
        children.retain(|c| c.node != child.node);
        self.xy_relink(state, &children)?;
        if self.cpu.read_u32(state + FOCUS)? == child.widget {
            self.xy_move_focus(state, 0)?;
        }
        // Desligar a ligação emprestada antes de soltar a referência possuída.
        self.xy_virtual(child.widget, 9, [0; 3], None)?;
        self.xy_virtual(child.widget, 1, [0; 3], None)?;
        self.heap.free(child.node);
        if self.cpu.read_u32(state + FLAGS)? & 8 != 0 {
            self.xy_move_focus(state, 2)?;
        }
        self.xy_changed(owner, state)?;
        Ok(SUCCESS)
    }
    fn xy_can_insert(&mut self, owner: u32, state: u32, widget: u32) -> Result<bool, CpuError> {
        if widget <= 1 || widget == u32::MAX || !self.xy_valid(widget, 4) {
            return Ok(false);
        }
        let vt = self.cpu.read_u32(widget)?;
        if !self.xy_valid(vt, 14 * 4) {
            return Ok(false);
        }
        // Um widget não pode ter dois pais nem fechar um ciclo de contenção.
        let own_widget = self.cpu.read_u32(owner + 4)?;
        if widget == own_widget {
            return Ok(false);
        }
        let mut ancestor = self.cpu.read_u32(state + PARENT)?;
        let mut visited = Vec::new();
        let mut acquired = Vec::new();
        let acyclic: Result<bool, CpuError> = (|| {
            while ancestor != 0 {
                if visited.contains(&ancestor) || visited.len() >= 1024 {
                    return Ok(false);
                }
                visited.push(ancestor);
                let step = self.xy_scratch(8, |m, p| {
                    let status = m.xy_virtual(ancestor, 2, [0x01015952, p, 0], None)?;
                    let current = m.cpu.read_u32(p)?;
                    // Um IContainer sem IWidget não pode ser o widget a inserir.
                    if status != SUCCESS || current == 0 {
                        return Ok((false, 0));
                    }
                    if current == widget {
                        m.xy_virtual(current, 1, [0; 3], None)?;
                        return Ok((true, 0));
                    }
                    m.cpu.write_u32(p, 0)?;
                    let next = m.xy_virtual(current, 8, [p, 0, 0], None);
                    m.xy_virtual(current, 1, [0; 3], None)?;
                    next?;
                    Ok((false, m.cpu.read_u32(p)?))
                })?;
                if step.0 {
                    return Ok(false);
                }
                ancestor = step.1;
                if ancestor != 0 {
                    acquired.push(ancestor);
                }
            }
            Ok(true)
        })();
        for parent in acquired {
            self.xy_virtual(parent, 1, [0; 3], None)?;
        }
        if !acyclic? {
            return Ok(false);
        }
        self.xy_scratch(8, |m, p| {
            let status = m.xy_virtual(widget, 2, [0x01015952, p, 0], None)?;
            let queried = m.cpu.read_u32(p)?;
            if status != SUCCESS || queried == 0 {
                return Ok(false);
            }
            m.xy_virtual(queried, 1, [0; 3], None)?;
            if queried != widget {
                return Ok(false);
            }
            m.cpu.write_u32(p, 0)?;
            m.xy_virtual(widget, 8, [p, 0, 0], None)?;
            let parent = m.cpu.read_u32(p)?;
            if parent != 0 {
                m.xy_virtual(parent, 1, [0; 3], None)?;
            }
            Ok(parent == 0)
        })
    }
    fn xy_locate(
        &mut self,
        owner: u32,
        state: u32,
        widget: u32,
        out: u32,
        rect: u32,
    ) -> Result<u32, CpuError> {
        if !self.xy_valid(rect, 8) || (out != 0 && !self.xy_valid(out, 4)) {
            return Ok(BAD);
        }
        let Some(child) = self
            .xy_children(state)?
            .into_iter()
            .find(|child| child.widget == widget)
        else {
            return Ok(EFAILED);
        };
        let mut r = self.read_rect(rect)?.unwrap();
        let x = i32::from(r.x).checked_add(child.x);
        let y = i32::from(r.y).checked_add(child.y);
        let (Some(x), Some(y)) = (x, y) else {
            return Ok(BAD);
        };
        let (Ok(x), Ok(y)) = (i16::try_from(x), i16::try_from(y)) else {
            return Ok(BAD);
        };
        r.x = x;
        r.y = y;
        let bytes = [r.x, r.y, r.width, r.height]
            .into_iter()
            .flat_map(i16::to_le_bytes)
            .collect::<Vec<_>>();
        if !self.xy_write(rect, &bytes) {
            return Ok(BAD);
        }
        let parent = self.cpu.read_u32(state + PARENT)?;
        if parent != 0 {
            let own_widget = self.cpu.read_u32(owner + 4)?;
            return self.xy_virtual(parent, 4, [own_widget, out, rect], None);
        }
        if out != 0 {
            if !self.xy_write(out, &owner.to_le_bytes()) {
                return Ok(BAD);
            }
            self.objects.add_ref(owner);
        }
        Ok(SUCCESS)
    }
    pub(super) fn xy_container_call(
        &mut self,
        iface: Interface,
        slot: u32,
    ) -> Result<Option<u32>, CpuError> {
        let this = self.arg(0);
        let (a1, a2, a3) = (self.arg(1), self.arg(2), self.arg(3));
        // A entrada privada 14 é PFNHANDLER: seu contexto é o dono, não IWidget.
        if iface == Interface::XYContainerWidget && slot == 14 {
            let Some((owner, state)) = self.xy_owner(Interface::XYContainer, this) else {
                return Ok(Some(BAD));
            };
            return Ok(Some(self.xy_event(owner, state, a1, a2, a3)?));
        }
        let Some((owner, state)) = self.xy_owner(iface, this) else {
            return Ok(Some(BAD));
        };
        let widget = self.cpu.read_u32(owner + 4)?;
        let result = match slot {
            0 => self.objects.add_ref(owner),
            1 => {
                if self.objects.contagem(owner) > 1 {
                    self.objects.release(owner)
                } else {
                    while self.cpu.read_u32(state + HEAD)? != 0 {
                        self.xy_remove(owner, state, u32::MAX)?;
                    }
                    let image = self.cpu.read_u32(state + BG_IMAGE)?;
                    self.cpu.write_u32(state + BG_IMAGE, 0)?;
                    self.xy_virtual(image, 1, [0; 3], None)?;
                    let free = self.cpu.read_u32(state + FREE)?;
                    let context = self.cpu.read_u32(state + CONTEXT)?;
                    self.cpu.write_u32(state + FREE, 0)?;
                    if free != 0 {
                        self.call_guest_aninhado(free, [context, 0, 0, 0], QSORT_BUDGET)?;
                    }
                    self.heap.free(state);
                    self.objects.release(widget);
                    self.objects.release(owner)
                }
            }
            2 => {
                if !self.xy_write(a2, &0u32.to_le_bytes()) {
                    return Ok(Some(BAD));
                }
                let target = match a1 {
                    0x01000001 | 0x01015932 | 0x01015954 => owner,
                    0x01015952 | 0x01015956 => widget,
                    _ => 0,
                };
                if target == 0 {
                    NOT_SUPPORTED
                } else {
                    self.cpu.write_u32(a2, target)?;
                    self.objects.add_ref(owner);
                    SUCCESS
                }
            }
            _ if iface == Interface::XYContainer => match slot {
                3 => {
                    if a3 & !63 != 0 {
                        return Ok(Some(BAD));
                    }
                    if !self.xy_children(state)?.iter().any(|c| c.widget == a1) {
                        return Ok(Some(BAD));
                    }
                    if a2 != 0 && !self.xy_valid(a2, 8) {
                        return Ok(Some(BAD));
                    }
                    self.xy_changed(owner, state)?;
                    SUCCESS
                }
                4 => {
                    let rect = a3;
                    self.xy_locate(owner, state, a1, a2, rect)?
                }
                5 | 8 => {
                    let desc = a3;
                    let mut bytes = [0u8; 12];
                    if desc != 0 {
                        if self.cpu.read_mem(desc, &mut bytes).is_err() {
                            return Ok(Some(BAD));
                        }
                    } else if slot == 8 {
                        return Ok(Some(BAD));
                    } else {
                        bytes[8] = 1;
                    }
                    let x = i32::from_le_bytes(bytes[..4].try_into().unwrap());
                    let y = i32::from_le_bytes(bytes[4..8].try_into().unwrap());
                    let visible = bytes[8] != 0;
                    let mut children = self.xy_children(state)?;
                    let mut child = if slot == 5 {
                        if children.iter().any(|c| c.widget == a1)
                            || children.len() >= 1024
                            || Self::xy_placement(&children, a2).is_none()
                        {
                            return Ok(Some(BAD));
                        }
                        if !self.xy_can_insert(owner, state, a1)? {
                            return Ok(Some(BAD));
                        }
                        let node = self.malloc(24)?;
                        if node == 0 {
                            return Ok(Some(NO_MEMORY));
                        }
                        self.cpu.write_mem(node, &[0; 24])?;
                        self.cpu.write_u32(node + 8, a1)?;
                        self.xy_virtual(a1, 0, [0; 3], None)?;
                        self.xy_virtual(a1, 9, [owner, 0, 0], None)?;
                        Child {
                            node,
                            widget: a1,
                            x,
                            y,
                            visible,
                        }
                    } else {
                        let Some(child) = Self::xy_target(&children, a1) else {
                            return Ok(Some(BAD));
                        };
                        if a2 != 0
                            && a2 != child.widget
                            && Self::xy_placement(&children, a2).is_none()
                        {
                            return Ok(Some(BAD));
                        }
                        child
                    };
                    let old_at = children.iter().position(|c| c.node == child.node);
                    children.retain(|c| c.node != child.node);
                    let at = if slot == 8 && (a2 == 0 || a2 == child.widget) {
                        old_at.unwrap_or(children.len())
                    } else {
                        Self::xy_placement(&children, a2).unwrap()
                    };
                    child.x = x;
                    child.y = y;
                    child.visible = visible;
                    self.cpu.write_u32(child.node + 12, x as u32)?;
                    self.cpu.write_u32(child.node + 16, y as u32)?;
                    self.cpu.write_u32(child.node + 20, visible as u32)?;
                    children.insert(at, child);
                    self.xy_relink(state, &children)?;
                    if !visible && self.cpu.read_u32(state + FOCUS)? == child.widget {
                        self.xy_move_focus(state, 0)?;
                    }
                    if self.cpu.read_u32(state + FLAGS)? & 8 != 0 {
                        self.xy_move_focus(state, 2)?;
                    }
                    self.xy_changed(owner, state)?;
                    SUCCESS
                }
                6 => self.xy_remove(owner, state, a1)?,
                7 => {
                    let children = self.xy_children(state)?;
                    if a1 == 0 {
                        if a2 != 0 {
                            children.first().map_or(0, |c| c.widget)
                        } else {
                            children.last().map_or(0, |c| c.widget)
                        }
                    } else if let Some(child) = Self::xy_target(&children, a1) {
                        let at = children.iter().position(|c| c.node == child.node).unwrap();
                        let next = if a2 != 0 {
                            at.checked_add(1).filter(|&i| i < children.len())
                        } else {
                            at.checked_sub(1)
                        };
                        next.or_else(|| {
                            if a3 != 0 {
                                Some(if a2 != 0 { 0 } else { children.len() - 1 })
                            } else {
                                None
                            }
                        })
                        .map_or(0, |i| children[i].widget)
                    } else {
                        0
                    }
                }
                9 => {
                    let Some(child) = Self::xy_target(&self.xy_children(state)?, a1) else {
                        return Ok(Some(BAD));
                    };
                    let mut bytes = [0; 12];
                    bytes[..4].copy_from_slice(&child.x.to_le_bytes());
                    bytes[4..8].copy_from_slice(&child.y.to_le_bytes());
                    bytes[8] = child.visible as u8;
                    if self.xy_write(a2, &bytes) {
                        SUCCESS
                    } else {
                        BAD
                    }
                }
                _ => return Ok(None),
            },
            _ => match slot {
                3 => {
                    let function = self.cpu.read_u32(state + HANDLER)?;
                    let context = self.cpu.read_u32(state + CONTEXT)?;
                    if function == 0 {
                        FALSE
                    } else {
                        match self.call_guest_aninhado(
                            function,
                            [context, a1, a2, a3],
                            QSORT_BUDGET,
                        )? {
                            Outcome::Returned { code } => code,
                            _ => FALSE,
                        }
                    }
                }
                4 => {
                    if !self.xy_valid(a1, 12) {
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
                    let bytes = old
                        .into_iter()
                        .flat_map(u32::to_le_bytes)
                        .collect::<Vec<_>>();
                    if !self.xy_write(a1, &bytes) {
                        return Ok(Some(BAD));
                    }
                    for (field, value) in [HANDLER, CONTEXT, FREE].into_iter().zip(next) {
                        self.cpu.write_u32(state + field, value)?;
                    }
                    SUCCESS
                }
                5 | 6 => {
                    // XY não escolhe o tamanho dos filhos: o tamanho preferido é o atual.
                    let bytes = [
                        self.cpu.read_u32(state + WIDTH)?,
                        self.cpu.read_u32(state + HEIGHT)?,
                    ]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>();
                    if self.xy_write(a1, &bytes) {
                        SUCCESS
                    } else {
                        BAD
                    }
                }
                7 => {
                    if !self.xy_valid(a1, 8) {
                        return Ok(Some(BAD));
                    }
                    let width = self.cpu.read_u32(a1)? as i32;
                    let height = self.cpu.read_u32(a1 + 4)? as i32;
                    if width < 0 || height < 0 {
                        return Ok(Some(BAD));
                    }
                    self.cpu.write_u32(state + WIDTH, width as u32)?;
                    self.cpu.write_u32(state + HEIGHT, height as u32)?;
                    self.xy_changed(owner, state)?;
                    SUCCESS
                }
                8 => {
                    let parent = self.cpu.read_u32(state + PARENT)?;
                    if !self.xy_write(a1, &parent.to_le_bytes()) {
                        return Ok(Some(BAD));
                    }
                    self.xy_virtual(parent, 0, [0; 3], None)?;
                    SUCCESS
                }
                9 => {
                    self.cpu.write_u32(state + PARENT, a1)?;
                    SUCCESS
                }
                10 => self.xy_draw(owner, state, a1, a2 as i32, a3 as i32)?,
                11 => FALSE,
                12 => {
                    if self.xy_write(a2, &0u32.to_le_bytes()) {
                        EFAILED
                    } else {
                        BAD
                    }
                }
                13 => NOT_SUPPORTED,
                _ => return Ok(None),
            },
        };
        Ok(Some(result))
    }
    fn xy_event(
        &mut self,
        owner: u32,
        state: u32,
        event: u32,
        w: u32,
        dw: u32,
    ) -> Result<u32, CpuError> {
        match event {
            0x700 => {
                self.cpu.write_u32(state + FOCUSED, (w != 0) as u32)?;
                if w == 0 {
                    self.xy_move_focus(state, 0)?;
                } else if self.cpu.read_u32(state + FOCUS)? == 0 {
                    self.xy_move_focus(state, 1)?;
                }
                self.xy_changed(owner, state)?;
                return Ok(TRUE);
            }
            0x701 => {
                let b = (self.cpu.read_u32(state + FOCUSED)? != 0) as u8;
                return Ok(if self.xy_write(dw, &[b]) { TRUE } else { FALSE });
            }
            0x702 => {
                let mut can = false;
                for child in self.xy_children(state)? {
                    if self.xy_focusable(child)?
                        && self
                            .xy_children(state)?
                            .iter()
                            .any(|c| c.widget == child.widget && c.node == child.node && c.visible)
                    {
                        can = true;
                        break;
                    }
                }
                return Ok(if self.xy_write(dw, &[can as u8]) {
                    TRUE
                } else {
                    FALSE
                });
            }
            0x711 => return Ok(self.xy_move_focus(state, dw)? as u32),
            0x712 => {
                if self.cpu.read_u32(state + FOCUS)? == dw {
                    self.xy_move_focus(state, 0)?;
                }
                return Ok(TRUE);
            }
            0x713 => {
                let focused = self.cpu.read_u32(state + FOCUS)?;
                if dw != 0 && !self.xy_write(dw, &focused.to_le_bytes()) {
                    return Ok(FALSE);
                }
                if dw != 0 {
                    self.xy_virtual(focused, 0, [0; 3], None)?;
                }
                return Ok(TRUE);
            }
            0x714 => return self.xy_traverse(owner, state, dw),
            0x802 => {
                if w > 2 {
                    return Ok(FALSE);
                }
                if w != 2 {
                    self.cpu.write_u32(state + LAYOUT, w)?;
                }
                if w != 0 {
                    self.xy_changed(owner, state)?;
                }
                return Ok(TRUE);
            }
            0x800 | 0x801 => {
                let set = event == 0x801;
                let field = match w {
                    0x153 => Some(FLAGS),
                    0x151 => Some(SELECTED),
                    0x131 => Some(BG_ACTIVE),
                    0x132 => Some(BG_INACTIVE),
                    0x133 => Some(BG_SELECTED_ACTIVE),
                    0x134 => Some(BG_SELECTED_INACTIVE),
                    0x353 => Some(BG_FRAME),
                    _ => None,
                };
                if let Some(field) = field {
                    if set {
                        if field == FLAGS && dw & !15 != 0 {
                            return Ok(FALSE);
                        }
                        self.cpu.write_u32(state + field, dw)?;
                        if matches!(
                            field,
                            BG_ACTIVE | BG_INACTIVE | BG_SELECTED_ACTIVE | BG_SELECTED_INACTIVE
                        ) {
                            self.cpu.write_u32(state + BG_ENABLED, 1)?;
                        }
                        self.xy_changed(owner, state)?;
                        return Ok(TRUE);
                    }
                    let value = self.cpu.read_u32(state + field)?;
                    return Ok(if self.xy_write(dw, &value.to_le_bytes()) {
                        TRUE
                    } else {
                        FALSE
                    });
                }
                if w == 0x130 && set {
                    for field in [
                        BG_ACTIVE,
                        BG_INACTIVE,
                        BG_SELECTED_ACTIVE,
                        BG_SELECTED_INACTIVE,
                    ] {
                        self.cpu.write_u32(state + field, dw)?;
                    }
                    self.cpu.write_u32(state + BG_ENABLED, 1)?;
                    self.xy_changed(owner, state)?;
                    return Ok(TRUE);
                }
                if w == 0x350 {
                    if set {
                        if dw != 0 && !self.images.contains_key(&dw) {
                            return Ok(FALSE);
                        }
                        self.xy_virtual(dw, 0, [0; 3], None)?;
                        let old = self.cpu.read_u32(state + BG_IMAGE)?;
                        self.cpu.write_u32(state + BG_IMAGE, dw)?;
                        self.xy_virtual(old, 1, [0; 3], None)?;
                        self.xy_changed(owner, state)?;
                        return Ok(TRUE);
                    }
                    let image = self.cpu.read_u32(state + BG_IMAGE)?;
                    if !self.xy_write(dw, &image.to_le_bytes()) {
                        return Ok(FALSE);
                    }
                    self.xy_virtual(image, 0, [0; 3], None)?;
                    return Ok(TRUE);
                }
                if w == 0x341 && self.xy_valid(dw, 8) {
                    let target = self.cpu.read_u32(dw)?;
                    if let Some(child) = Self::xy_target(&self.xy_children(state)?, target) {
                        if set {
                            let mut b = [0];
                            self.cpu.read_mem(dw + 4, &mut b)?;
                            self.cpu.write_u32(child.node + 20, (b[0] != 0) as u32)?;
                            if b[0] == 0 && self.cpu.read_u32(state + FOCUS)? == child.widget {
                                self.xy_move_focus(state, 0)?;
                            }
                            self.xy_changed(owner, state)?;
                            return Ok(TRUE);
                        }
                        return Ok(if self.xy_write(dw + 4, &[child.visible as u8]) {
                            TRUE
                        } else {
                            FALSE
                        });
                    }
                }
                // Propriedades desconhecidas não são encaminhadas para um filho como sucesso.
                return Ok(FALSE);
            }
            _ => {}
        }
        let flags = self.cpu.read_u32(state + FLAGS)?;
        if flags & 1 != 0 {
            let mut children = self.xy_children(state)?;
            children.reverse();
            for child in children {
                if child.visible && self.xy_virtual(child.widget, 3, [event, w, dw], None)? != 0 {
                    return Ok(TRUE);
                }
            }
            return Ok(FALSE);
        }
        if flags & 4 != 0 {
            return Ok(FALSE);
        }
        let focus = self.cpu.read_u32(state + FOCUS)?;
        if focus == 0 {
            return Ok(FALSE);
        }
        if flags & 2 == 0 {
            if event == 0x101 {
                self.cpu.write_u32(state + KEY_WIDGET, focus)?;
                self.cpu.write_u32(state + KEY_CODE, w)?;
            }
            if matches!(event, 0x100 | 0x102)
                && (self.cpu.read_u32(state + KEY_WIDGET)? != focus
                    || self.cpu.read_u32(state + KEY_CODE)? != w)
            {
                return Ok(FALSE);
            }
            if event == 0x102 {
                self.cpu.write_u32(state + KEY_WIDGET, 0)?;
            }
        }
        self.xy_virtual(focus, 3, [event, w, dw], None)
    }
    fn xy_traverse(&mut self, owner: u32, state: u32, desc: u32) -> Result<u32, CpuError> {
        if !self.xy_valid(desc, 12) {
            return Ok(FALSE);
        }
        let visit = self.cpu.read_u32(desc)?;
        let context = self.cpu.read_u32(desc + 4)?;
        let mut depth_bytes = [0; 2];
        self.cpu.read_mem(desc + 8, &mut depth_bytes)?;
        let depth = u16::from_le_bytes(depth_bytes);
        if visit == 0 {
            return Ok(FALSE);
        }
        let widget = self.cpu.read_u32(owner + 4)?;
        if !matches!(self.call_guest_aninhado(visit,[context,widget,0,0],QSORT_BUDGET)?,Outcome::Returned{code} if code!=0)
        {
            return Ok(FALSE);
        }
        if depth == 1 {
            return Ok(TRUE);
        }
        let children = self.xy_children(state)?;
        for child in children {
            if self.objects.kind_of(child.widget) == Some(Interface::XYContainerWidget) {
                let other = self.cpu.read_u32(child.widget + 4)?;
                let state = self.cpu.read_u32(other + 8)?;
                let result = self.xy_scratch(12, |m, p| {
                    m.cpu.write_u32(p, visit)?;
                    m.cpu.write_u32(p + 4, context)?;
                    m.cpu
                        .write_u32(p + 8, if depth == 0 { 0 } else { u32::from(depth - 1) })?;
                    m.xy_traverse(other, state, p)
                })?;
                if result == FALSE {
                    return Ok(FALSE);
                }
            } else if !matches!(self.call_guest_aninhado(visit,[context,child.widget,0,0],QSORT_BUDGET)?,Outcome::Returned{code} if code!=0)
            {
                return Ok(FALSE);
            }
        }
        Ok(TRUE)
    }
    fn xy_draw(
        &mut self,
        _owner: u32,
        state: u32,
        canvas: u32,
        x: i32,
        y: i32,
    ) -> Result<u32, CpuError> {
        let Some(&target) = self.canvases.get(&canvas) else {
            return Ok(UNSUPPORTED);
        };
        let Some(surface) = self.bitmaps.get(&target) else {
            return Ok(UNSUPPORTED);
        };
        let width = self.cpu.read_u32(state + WIDTH)? as i32;
        let height = self.cpu.read_u32(state + HEIGHT)? as i32;
        let (mut x0, mut y0, mut x1, mut y1) = (
            x.max(0),
            y.max(0),
            x.saturating_add(width)
                .min(surface.width() as i32)
                .min(i16::MAX as i32),
            y.saturating_add(height)
                .min(surface.height() as i32)
                .min(i16::MAX as i32),
        );
        let (old_target, old_clip) = (self.display_target, self.clip);
        if let Some(clip) = old_clip {
            x0 = x0.max(i32::from(clip.x));
            y0 = y0.max(i32::from(clip.y));
            x1 = x1.min(i32::from(clip.x) + i32::from(clip.width));
            y1 = y1.min(i32::from(clip.y) + i32::from(clip.height));
        }
        if width <= 0 || height <= 0 || x1 <= x0 || y1 <= y0 {
            return Ok(SUCCESS);
        }
        let clip = Rect {
            x: x0 as i16,
            y: y0 as i16,
            width: (x1 - x0) as i16,
            height: (y1 - y0) as i16,
        };
        self.display_target = target;
        self.clip = Some(clip);
        let result = (|| {
            if self.cpu.read_u32(state + BG_ENABLED)? != 0 {
                let active = self.cpu.read_u32(state + FOCUSED)? != 0;
                let selected = self.cpu.read_u32(state + SELECTED)? != 0;
                let field = match (active, selected) {
                    (true, false) => BG_ACTIVE,
                    (false, false) => BG_INACTIVE,
                    (true, true) => BG_SELECTED_ACTIVE,
                    (false, true) => BG_SELECTED_INACTIVE,
                };
                let color = Rgb::from_rgbval(self.cpu.read_u32(state + field)?);
                self.bitmaps
                    .get_mut(&target)
                    .unwrap()
                    .fill_rect(clip, color);
            }
            let image = self.cpu.read_u32(state + BG_IMAGE)?;
            if image != 0 {
                let frame = self.cpu.read_u32(state + BG_FRAME)?;
                self.draw_image(image, x, y, Some(frame))?;
            }
            for child in self.xy_children(state)? {
                if !child.visible {
                    continue;
                }
                let Some(cx) = x.checked_add(child.x) else {
                    continue;
                };
                let Some(cy) = y.checked_add(child.y) else {
                    continue;
                };
                let extent = self.xy_scratch(8, |m, p| {
                    m.xy_virtual(child.widget, 6, [p, 0, 0], None)?;
                    Ok((m.cpu.read_u32(p)? as i32, m.cpu.read_u32(p + 4)? as i32))
                })?;
                if extent.0 <= 0
                    || extent.1 <= 0
                    || cx >= x1
                    || cy >= y1
                    || cx.saturating_add(extent.0) <= x0
                    || cy.saturating_add(extent.1) <= y0
                {
                    continue;
                }
                self.xy_virtual(child.widget, 10, [canvas, cx as u32, cy as u32], None)?;
            }
            Ok(SUCCESS)
        })();
        self.display_target = old_target;
        self.clip = old_clip;
        result
    }
}
