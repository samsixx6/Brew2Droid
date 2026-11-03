//! CFontMapModel: nomes copiados, fontes retidas e listeners IModel canceláveis.
use super::*;
pub(super) const CLASS_ID: u32 = 0x0105_51cb;
const BAD: u32 = 14;
const HEAD: u32 = 4;
const LISTENERS: u32 = 8;
// Nó privado: next, nome era não-NULL, IFont*, AECHAR nome[...].
// O StyleDesc público começa em +8 e permanece emprestado pelo mapa.
#[derive(Clone)]
struct Style {
    node: u32,
    font: u32,
    name: Option<Vec<u16>>,
}
impl<C: CpuBackend> Machine<C> {
    fn font_map_name(&self, p: u32) -> Result<Option<Vec<u16>>, u32> {
        if p == 0 {
            return Ok(None);
        }
        let mut units = Vec::new();
        for index in 0..4096u32 {
            let at = p.checked_add(index * 2).ok_or(BAD)?;
            let mut bytes = [0; 2];
            self.cpu.read_mem(at, &mut bytes).map_err(|_| BAD)?;
            let c = u16::from_le_bytes(bytes);
            if c == 0 {
                if String::from_utf16(&units).is_err() {
                    return Err(BAD);
                }
                return Ok(Some(units));
            }
            units.push(c);
        }
        Err(BAD)
    }
    fn font_map_styles(&self, map: u32) -> Result<Vec<Style>, CpuError> {
        let mut out = Vec::new();
        let mut node = self.cpu.read_u32(map + HEAD)?;
        while node != 0 {
            if out.len() >= 4096
                || out.iter().any(|s: &Style| s.node == node)
                || self.heap.size_of(node).is_none_or(|n| n < 16)
            {
                return Err(CpuError("lista privada de fontes corrompida".into()));
            }
            let present = self.cpu.read_u32(node + 4)? != 0;
            let name = if present {
                self.font_map_name(node + 12)
                    .map_err(|_| CpuError("nome privado de fonte inválido".into()))?
            } else {
                None
            };
            out.push(Style {
                node,
                font: self.cpu.read_u32(node + 8)?,
                name,
            });
            node = self.cpu.read_u32(node)?;
        }
        Ok(out)
    }
    pub(super) fn create_font_map(&mut self, out: u32) -> Result<u32, CpuError> {
        if !self.modern_font_write(out, &0u32.to_le_bytes()) {
            return Ok(BAD);
        }
        let map = self.new_object(Interface::FontMapModel)?;
        if map == 0 {
            return Ok(2);
        }
        self.cpu.write_mem(map + 4, &[0; 60])?;
        self.cpu.write_u32(out, map)?;
        Ok(SUCCESS)
    }
    fn font_map_cancel(&mut self, listener: u32) -> Result<(), CpuError> {
        if !self.modern_font_valid(listener, 24) {
            return Ok(());
        }
        let map = self.cpu.read_u32(listener + 20)?;
        if self.objects.kind_of(map) != Some(Interface::FontMapModel) {
            return Ok(());
        }
        let next = self.cpu.read_u32(listener)?;
        let prev = self.cpu.read_u32(listener + 4)?;
        if prev == 0 {
            if self.cpu.read_u32(map + LISTENERS)? == listener {
                self.cpu.write_u32(map + LISTENERS, next)?;
            }
        } else {
            self.cpu.write_u32(prev, next)?;
        }
        if next != 0 {
            self.cpu.write_u32(next + 4, prev)?;
        }
        for at in [0, 4, 16, 20] {
            self.cpu.write_u32(listener + at, 0)?;
        }
        Ok(())
    }
    fn font_map_notify(&mut self, map: u32, event: u32) -> Result<(), CpuError> {
        if !self.modern_font_valid(event, 12) {
            return Ok(());
        }
        let mut listener = self.cpu.read_u32(map + LISTENERS)?;
        let mut entries = Vec::new();
        while listener != 0 && entries.len() < 4096 && !entries.contains(&listener) {
            entries.push(listener);
            listener = self.cpu.read_u32(listener)?;
        }
        // O listener pode cancelar a si mesmo ou outro listener durante o callback.
        self.objects.add_ref(map);
        let result = (|| {
            for l in entries {
                if self.cpu.read_u32(l + 20)? != map || self.cpu.read_u32(l + 16)? == 0 {
                    continue;
                }
                let f = self.cpu.read_u32(l + 8)?;
                let context = self.cpu.read_u32(l + 12)?;
                if f != 0 {
                    self.call_guest_aninhado(f, [context, event, 0, 0], QSORT_BUDGET)?;
                }
            }
            Ok(())
        })();
        self.modern_font_base(map, 1)?;
        result
    }
    fn font_map_changed(&mut self, map: u32, pos: u32, old: u32, new: u32) -> Result<(), CpuError> {
        if self.cpu.read_u32(map + LISTENERS)? == 0 {
            return Ok(());
        }
        let event = map + 16;
        let mut previous = [0u8; 24];
        self.cpu.read_mem(event, &mut previous)?;
        let data = [0x1001, map, 0, pos, old, new]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        self.cpu.write_mem(event, &data)?;
        let result = self.font_map_notify(map, event);
        // Um listener pode alterar o mesmo mapa; restaurar preserva o evento externo.
        self.cpu.write_mem(event, &previous)?;
        result
    }

    fn font_map_delete(&mut self, map: u32, index: usize) -> Result<u32, CpuError> {
        let styles = self.font_map_styles(map)?;
        let Some(style) = styles.get(index) else {
            return Ok(EFAILED);
        };
        let next = self.cpu.read_u32(style.node)?;
        if index == 0 {
            self.cpu.write_u32(map + HEAD, next)?;
        } else {
            self.cpu.write_u32(styles[index - 1].node, next)?;
        }
        self.modern_font_base(style.font, 1)?;
        self.heap.free(style.node);
        self.font_map_changed(
            map,
            index as u32,
            styles.len() as u32,
            styles.len() as u32 - 1,
        )?;
        Ok(SUCCESS)
    }
    pub(super) fn font_map_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let this = self.arg(0);
        if !(3..=10).contains(&slot) || self.objects.kind_of(this) != Some(Interface::FontMapModel)
        {
            return self.font_map_call_inner(slot);
        }
        self.objects.add_ref(this);
        let result = self.font_map_call_inner(slot);
        self.modern_font_base(this, 1)?;
        result
    }
    fn font_map_call_inner(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let this = self.arg(0);
        let (a1, a2) = (self.arg(1), self.arg(2));
        if slot == 11 {
            self.font_map_cancel(this)?;
            return Ok(Some(SUCCESS));
        }
        if self.objects.kind_of(this) != Some(Interface::FontMapModel) {
            return Ok(Some(BAD));
        }
        let result = match slot {
            0 => self.objects.add_ref(this),
            1 => {
                if self.objects.contagem(this) == 1 {
                    while self.cpu.read_u32(this + LISTENERS)? != 0 {
                        let l = self.cpu.read_u32(this + LISTENERS)?;
                        self.font_map_cancel(l)?;
                    }
                    while self.cpu.read_u32(this + HEAD)? != 0 {
                        self.font_map_delete(this, 0)?;
                    }
                }
                self.objects.release(this)
            }
            2 => {
                if !self.modern_font_write(a2, &0u32.to_le_bytes()) {
                    return Ok(Some(BAD));
                }
                if matches!(a1, 0x01000001 | 0x0101593a | 0x01015936 | 0x0103a3e6) {
                    self.cpu.write_u32(a2, this)?;
                    self.objects.add_ref(this);
                    SUCCESS
                } else {
                    3
                }
            }
            3 => {
                if !self.modern_font_valid(a1, 24) || self.cpu.read_u32(a1 + 8).unwrap_or(0) == 0 {
                    return Ok(Some(BAD));
                }
                let cancel = self.cpu.read_u32(a1 + 16)?;
                if cancel != 0 {
                    self.call_guest_aninhado(cancel, [a1, 0, 0, 0], QSORT_BUDGET)?;
                }
                let head = self.cpu.read_u32(this + LISTENERS)?;
                self.cpu.write_u32(a1, head)?;
                self.cpu.write_u32(a1 + 4, 0)?;
                if head != 0 {
                    self.cpu.write_u32(head + 4, a1)?;
                }
                self.cpu
                    .write_u32(a1 + 16, aee::encode(Interface::FontMapModel, 11))?;
                self.cpu.write_u32(a1 + 20, this)?;
                self.cpu.write_u32(this + LISTENERS, a1)?;
                SUCCESS
            }
            4 => {
                if !self.modern_font_valid(a1, 12) {
                    return Ok(Some(BAD));
                }
                self.font_map_notify(this, a1)?;
                SUCCESS
            }
            5 => self.font_map_styles(this)?.len() as u32,
            6 => {
                if !self.modern_font_write(a2, &0u32.to_le_bytes()) {
                    return Ok(Some(BAD));
                }
                let styles = self.font_map_styles(this)?;
                if let Some(style) = styles.get(a1 as usize) {
                    self.cpu.write_u32(a2, style.node + 8)?;
                    SUCCESS
                } else {
                    BAD
                }
            }
            7 => {
                if !self.modern_font_write(a2, &0u32.to_le_bytes()) {
                    return Ok(Some(BAD));
                }
                let name = match self.font_map_name(a1) {
                    Ok(n) => n,
                    Err(e) => return Ok(Some(e)),
                };
                if let Some(style) = self
                    .font_map_styles(this)?
                    .into_iter()
                    .find(|s| s.name == name)
                {
                    self.cpu.write_u32(a2, style.font)?;
                    self.modern_font_base(style.font, 0)?;
                    SUCCESS
                } else {
                    EFAILED
                }
            }
            8 => {
                let name = match self.font_map_name(a1) {
                    Ok(n) => n,
                    Err(e) => return Ok(Some(e)),
                };
                if a2 == 0 || !self.modern_font_valid(a2, 4) {
                    return Ok(Some(BAD));
                }
                // O IFont legado do console tem outro ABI; não o anunciar como moderno.
                if self.objects.kind_of(a2) == Some(Interface::Font) {
                    return Ok(Some(20));
                }
                let styles = self.font_map_styles(this)?;
                if styles.len() >= 4096 {
                    return Ok(Some(2));
                }
                let vt = self.cpu.read_u32(a2)?;
                if !self.modern_font_valid(vt, 24) {
                    return Ok(Some(BAD));
                }
                let query = self.cpu.read_u32(vt + 8)?;
                let out = self.malloc(8)?;
                if out == 0 {
                    return Ok(Some(2));
                }
                self.cpu.write_u32(out, 0)?;
                let queried =
                    self.call_guest_aninhado(query, [a2, 0x01001022, out, 0], QSORT_BUDGET);
                let font = self.cpu.read_u32(out)?;
                self.heap.free(out);
                if !matches!(queried?, Outcome::Returned { code: 0 }) || font == 0 {
                    return Ok(Some(3));
                }
                let units = name.as_deref().unwrap_or(&[]);
                let size = 12 + (units.len() as u32 + 1) * 2;
                let node = self.malloc(size)?;
                if node == 0 {
                    self.modern_font_base(font, 1)?;
                    return Ok(Some(2));
                }
                self.cpu.write_u32(node, self.cpu.read_u32(this + HEAD)?)?;
                self.cpu.write_u32(node + 4, name.is_some() as u32)?;
                self.cpu.write_u32(node + 8, font)?;
                self.cpu.write_mem(
                    node + 12,
                    &units
                        .iter()
                        .copied()
                        .chain([0])
                        .flat_map(u16::to_le_bytes)
                        .collect::<Vec<_>>(),
                )?;
                self.cpu.write_u32(this + HEAD, node)?;
                self.font_map_changed(this, 0, styles.len() as u32, styles.len() as u32 + 1)?;
                SUCCESS
            }
            9 => {
                let name = match self.font_map_name(a1) {
                    Ok(n) => n,
                    Err(e) => return Ok(Some(e)),
                };
                let styles = self.font_map_styles(this)?;
                if let Some(index) = styles.iter().position(|s| s.name == name) {
                    self.font_map_delete(this, index)?
                } else {
                    EFAILED
                }
            }
            10 => {
                while self.cpu.read_u32(this + HEAD)? != 0 {
                    self.font_map_delete(this, 0)?;
                }
                SUCCESS
            }
            _ => return Ok(None),
        };
        Ok(Some(result))
    }
}
