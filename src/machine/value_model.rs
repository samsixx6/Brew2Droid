//! IValueModel público: valor opaco, callbacks de adaptação e posse explícita.
use super::*;
pub(super) const CLASS_ID: u32 = 0x0105_51e9;
const BAD: u32 = 14;
const LISTENERS: u32 = 4;
const VALUE: u32 = 8;
const LENGTH: u32 = 12;
const FREE: u32 = 16;
const GET: u32 = 20;
const GET_CONTEXT: u32 = 24;
const SET: u32 = 28;
const SET_CONTEXT: u32 = 32;
impl<C: CpuBackend> Machine<C> {
    pub(super) fn modern_ui_call(
        &mut self,
        entry: u32,
        args: [u32; 4],
        extra: &[u32],
    ) -> Result<u32, CpuError> {
        let old = self.cpu.read_reg(Reg::Sp);
        if !extra.is_empty() {
            let sp = old
                .checked_sub(extra.len() as u32 * 4 + 8)
                .ok_or_else(|| CpuError("pilha de UI inválida".into()))?
                & !7;
            self.cpu.write_mem(
                sp,
                &extra
                    .iter()
                    .copied()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )?;
            self.cpu.write_reg(Reg::Sp, sp);
        }
        let result = self.call_guest_aninhado(entry, args, QSORT_BUDGET);
        self.cpu.write_reg(Reg::Sp, old);
        match result? {
            Outcome::Returned { code } => Ok(code),
            other => Err(CpuError(format!(
                "callback público de UI interrompido: {other:?}"
            ))),
        }
    }
    pub(super) fn modern_ui_scratch<T>(
        &mut self,
        size: u32,
        action: impl FnOnce(&mut Self, u32) -> Result<T, CpuError>,
    ) -> Result<T, CpuError> {
        let old = self.cpu.read_reg(Reg::Sp);
        let sp = old
            .checked_sub(
                size.checked_add(8)
                    .ok_or_else(|| CpuError("tamanho temporário de UI inválido".into()))?,
            )
            .ok_or_else(|| CpuError("pilha temporária de UI inválida".into()))?
            & !7;
        self.cpu.write_mem(sp, &vec![0; size as usize])?;
        self.cpu.write_reg(Reg::Sp, sp);
        let result = action(self, sp);
        self.cpu.write_reg(Reg::Sp, old);
        result
    }
    pub(super) fn modern_ui_invoke(
        &mut self,
        object: u32,
        slot: u32,
        args: [u32; 3],
        extra: &[u32],
    ) -> Result<u32, CpuError> {
        if object == 0 {
            return Ok(0);
        }
        let table = self.cpu.read_u32(object)?;
        let entry = self.cpu.read_u32(table + slot * 4)?;
        if entry == 0 {
            return Ok(20);
        }
        self.modern_ui_call(entry, [object, args[0], args[1], args[2]], extra)
    }
    pub(super) fn create_value_model(&mut self, out: u32) -> Result<u32, CpuError> {
        if !self.modern_font_write(out, &0u32.to_le_bytes()) {
            return Ok(BAD);
        }
        let model = self.new_object(Interface::ValueModel)?;
        if model == 0 {
            return Ok(2);
        }
        self.cpu.write_mem(model + 4, &[0; 60])?;
        self.cpu.write_u32(out, model)?;
        Ok(SUCCESS)
    }
    fn value_cancel(&mut self, listener: u32) -> Result<(), CpuError> {
        if !self.modern_font_valid(listener, 24) {
            return Ok(());
        }
        let model = self.cpu.read_u32(listener + 20)?;
        if self.objects.kind_of(model) != Some(Interface::ValueModel) {
            return Ok(());
        }
        let prev = self.cpu.read_u32(listener + 4)?;
        let next = self.cpu.read_u32(listener)?;
        if prev == 0 {
            if self.cpu.read_u32(model + LISTENERS)? == listener {
                self.cpu.write_u32(model + LISTENERS, next)?;
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
    fn value_notify(&mut self, model: u32, event: u32) -> Result<(), CpuError> {
        let mut entries = Vec::new();
        let mut l = self.cpu.read_u32(model + LISTENERS)?;
        while l != 0 && entries.len() < 4096 && !entries.contains(&l) {
            entries.push(l);
            l = self.cpu.read_u32(l)?;
        }
        self.objects.add_ref(model);
        let result = (|| {
            for l in entries {
                if self.cpu.read_u32(l + 20)? != model || self.cpu.read_u32(l + 16)? == 0 {
                    continue;
                }
                let f = self.cpu.read_u32(l + 8)?;
                let context = self.cpu.read_u32(l + 12)?;
                if f != 0 {
                    self.modern_ui_call(f, [context, event, 0, 0], &[])?;
                }
            }
            Ok(())
        })();
        self.modern_font_base(model, 1)?;
        result
    }
    pub(super) fn value_model_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let this = self.arg(0);
        if !(3..=8).contains(&slot) || self.objects.kind_of(this) != Some(Interface::ValueModel) {
            return self.value_model_call_inner(slot);
        }
        // Adaptadores e callbacks podem soltar a última referência do chamador.
        // O método em curso mantém sua própria referência até terminar.
        self.objects.add_ref(this);
        let result = self.value_model_call_inner(slot);
        self.modern_font_base(this, 1)?;
        result
    }
    fn value_model_call_inner(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let this = self.arg(0);
        let (a1, a2, a3) = (self.arg(1), self.arg(2), self.arg(3));
        if slot == 9 {
            self.value_cancel(this)?;
            return Ok(Some(SUCCESS));
        }
        if self.objects.kind_of(this) != Some(Interface::ValueModel) {
            return Ok(Some(BAD));
        }
        let result = match slot {
            0 => self.objects.add_ref(this),
            1 => {
                if self.objects.contagem(this) == 1 {
                    while self.cpu.read_u32(this + LISTENERS)? != 0 {
                        let l = self.cpu.read_u32(this + LISTENERS)?;
                        self.value_cancel(l)?;
                    }
                    let value = self.cpu.read_u32(this + VALUE)?;
                    let free = self.cpu.read_u32(this + FREE)?;
                    self.cpu.write_u32(this + FREE, 0)?;
                    self.cpu.write_u32(this + VALUE, 0)?;
                    if free != 0 {
                        self.modern_ui_call(free, [value, 0, 0, 0], &[])?;
                    }
                }
                self.objects.release(this)
            }
            2 => {
                if !self.modern_font_write(a2, &0u32.to_le_bytes()) {
                    return Ok(Some(BAD));
                }
                if matches!(a1, 0x01000001 | 0x0101593a | 0x0101593b) {
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
                    self.modern_ui_call(cancel, [a1, 0, 0, 0], &[])?;
                }
                let head = self.cpu.read_u32(this + LISTENERS)?;
                self.cpu.write_u32(a1, head)?;
                self.cpu.write_u32(a1 + 4, 0)?;
                if head != 0 {
                    self.cpu.write_u32(head + 4, a1)?;
                }
                self.cpu
                    .write_u32(a1 + 16, aee::encode(Interface::ValueModel, 9))?;
                self.cpu.write_u32(a1 + 20, this)?;
                self.cpu.write_u32(this + LISTENERS, a1)?;
                SUCCESS
            }
            4 => {
                if !self.modern_font_valid(a1, 12) {
                    return Ok(Some(BAD));
                }
                self.value_notify(this, a1)?;
                SUCCESS
            }
            5 => {
                if (a2 as i32) < 0 {
                    return Ok(Some(BAD));
                }
                let (mut value, mut length, mut free) = (a1, a2, a3);
                let adapt = self.cpu.read_u32(this + SET)?;
                if adapt != 0 {
                    let context = self.cpu.read_u32(this + SET_CONTEXT)?;
                    (value, length, free) = self.modern_ui_scratch(16, |m, out| {
                        m.cpu.write_u32(out, value)?;
                        m.cpu.write_u32(out + 4, length)?;
                        m.cpu.write_u32(out + 8, free)?;
                        m.modern_ui_call(
                            adapt,
                            [context, value, length, out],
                            &[out + 4, out + 8],
                        )?;
                        Ok((
                            m.cpu.read_u32(out)?,
                            m.cpu.read_u32(out + 4)?,
                            m.cpu.read_u32(out + 8)?,
                        ))
                    })?;
                    if (length as i32) < 0 {
                        return Ok(Some(BAD));
                    }
                }
                let old = self.cpu.read_u32(this + VALUE)?;
                let old_free = self.cpu.read_u32(this + FREE)?;
                self.cpu.write_u32(this + VALUE, value)?;
                self.cpu.write_u32(this + LENGTH, length)?;
                self.cpu.write_u32(this + FREE, free)?;
                if old_free != 0 && (old != value || old_free != free) {
                    self.modern_ui_call(old_free, [old, 0, 0, 0], &[])?;
                }
                if self.cpu.read_u32(this + LISTENERS)? != 0 {
                    // ModelEvent cabe no objeto já retido: o heap cheio não perde eventos.
                    let event = this + 40;
                    self.cpu.write_u32(event, 0x1000)?;
                    self.cpu.write_u32(event + 4, this)?;
                    self.cpu.write_u32(event + 8, 0)?;
                    self.value_notify(this, event)?;
                }
                SUCCESS
            }
            6 => {
                if a1 != 0 && !self.modern_font_valid(a1, 4) {
                    return Ok(Some(BAD));
                }
                let (mut value, mut length) = (
                    self.cpu.read_u32(this + VALUE)?,
                    self.cpu.read_u32(this + LENGTH)?,
                );
                let adapt = self.cpu.read_u32(this + GET)?;
                if adapt != 0 {
                    let context = self.cpu.read_u32(this + GET_CONTEXT)?;
                    (value, length) = self.modern_ui_scratch(8, |m, out| {
                        m.cpu.write_u32(out, value)?;
                        m.cpu.write_u32(out + 4, length)?;
                        m.modern_ui_call(adapt, [context, value, length, out], &[out + 4])?;
                        Ok((m.cpu.read_u32(out)?, m.cpu.read_u32(out + 4)?))
                    })?;
                }
                if a1 != 0 && !self.modern_font_write(a1, &length.to_le_bytes()) {
                    return Ok(Some(BAD));
                }
                value
            }
            7 | 8 => {
                self.cpu
                    .write_u32(this + if slot == 7 { GET } else { SET }, a1)?;
                self.cpu
                    .write_u32(this + if slot == 7 { GET_CONTEXT } else { SET_CONTEXT }, a2)?;
                SUCCESS
            }
            _ => return Ok(None),
        };
        Ok(Some(result))
    }
}
