//! CTextModel: AECHAR e seleção reais; IRichTextModel tem uma tabela distinta.
use super::*;
pub(super) const CLASS_ID: u32 = 0x0105_51e5;
const BAD: u32 = 14;
const OOM: u32 = 2;
const UNSUPPORTED: u32 = 20;
const BYTES: u32 = 128;
const TEXT: u32 = 0;
const LENGTH: u32 = 4;
const CAP: u32 = 8;
const START: u32 = 12;
const END: u32 = 16;
const LISTENERS: u32 = 20;
const TAGS: u32 = 24;
const TAG_COUNT: u32 = 28;
const DESTROY: u32 = 32;
const MAX_TEXT: usize = 65_536;
const MAX_TAGS: usize = 64;
#[derive(Clone)]
struct TextTag {
    start: usize,
    length: usize,
    name: Vec<u16>,
    pointer: u32,
}
impl<C: CpuBackend> Machine<C> {
    fn text_model_state(&self, iface: Interface, this: u32) -> Option<(u32, u32)> {
        if self.objects.kind_of(this) != Some(iface) {
            return None;
        }
        let owner = if iface == Interface::TextModel {
            this
        } else {
            self.cpu.read_u32(this + 4).ok()?
        };
        if self.objects.kind_of(owner) != Some(Interface::TextModel) {
            return None;
        }
        if iface == Interface::RichTextModel && self.cpu.read_u32(owner + 8).ok() != Some(this) {
            return None;
        }
        let state = self.cpu.read_u32(owner + 4).ok()?;
        if self.heap.size_of(state).is_none_or(|n| n < BYTES) {
            return None;
        }
        Some((owner, state))
    }
    pub(super) fn create_text_model(&mut self, out: u32) -> Result<u32, CpuError> {
        if !self.modern_font_write(out, &0u32.to_le_bytes()) {
            return Ok(BAD);
        }
        let owner = self.new_object(Interface::TextModel)?;
        if owner == 0 {
            return Ok(OOM);
        }
        let rich = self.new_object(Interface::RichTextModel)?;
        if rich == 0 {
            self.objects.release(owner);
            return Ok(OOM);
        }
        let state = match self.malloc(BYTES) {
            Ok(p) => p,
            Err(e) => {
                self.objects.release(rich);
                self.objects.release(owner);
                return Err(e);
            }
        };
        if state == 0 {
            self.objects.release(rich);
            self.objects.release(owner);
            return Ok(OOM);
        }
        self.cpu.write_mem(owner + 4, &[0; 60])?;
        self.cpu.write_mem(rich + 4, &[0; 60])?;
        self.cpu.write_mem(state, &[0; BYTES as usize])?;
        self.cpu.write_u32(owner + 4, state)?;
        self.cpu.write_u32(owner + 8, rich)?;
        self.cpu.write_u32(rich + 4, owner)?;
        self.cpu.write_u32(state + TEXT, state + 64)?;
        self.cpu.write_u32(out, owner)?;
        Ok(SUCCESS)
    }
    fn text_model_units(&self, p: u32, count: i32) -> Result<Vec<u16>, u32> {
        if count == 0 {
            return Ok(Vec::new());
        }
        if p == 0 {
            return Err(BAD);
        }
        let n = if count < 0 { MAX_TEXT } else { count as usize };
        if n > MAX_TEXT {
            return Err(UNSUPPORTED);
        }
        let mut units = Vec::new();
        for i in 0..n {
            let at = p.checked_add((i * 2) as u32).ok_or(BAD)?;
            let mut b = [0; 2];
            self.cpu.read_mem(at, &mut b).map_err(|_| BAD)?;
            let v = u16::from_le_bytes(b);
            if count < 0 && v == 0 {
                return Ok(units);
            }
            units.push(v);
        }
        if count < 0 {
            Err(UNSUPPORTED)
        } else {
            Ok(units)
        }
    }
    fn text_model_text(&self, state: u32) -> Result<Vec<u16>, CpuError> {
        let p = self.cpu.read_u32(state + TEXT)?;
        let n = self.cpu.read_u32(state + LENGTH)?;
        self.text_model_units(p, n as i32)
            .map_err(|_| CpuError("texto privado inválido".into()))
    }
    fn text_model_tags(&self, state: u32) -> Result<Vec<TextTag>, CpuError> {
        let count = self.cpu.read_u32(state + TAG_COUNT)? as usize;
        if count > MAX_TAGS {
            return Err(CpuError("tags acima do limite".into()));
        }
        let mut at = self.cpu.read_u32(state + TAGS)?;
        let mut tags = Vec::new();
        for _ in 0..count {
            let start = self.cpu.read_u32(at)? as usize;
            let length = self.cpu.read_u32(at + 4)? as usize;
            let n = self.cpu.read_u32(at + 8)? as usize;
            if n == 0 || n > MAX_TEXT {
                return Err(CpuError("nome privado de tag inválido".into()));
            }
            let pointer = at + 12;
            let name = self
                .text_model_units(pointer, n as i32)
                .map_err(|_| CpuError("nome privado de tag inválido".into()))?;
            tags.push(TextTag {
                start,
                length,
                name,
                pointer,
            });
            at = at
                .checked_add(((12 + (n + 1) * 2 + 3) & !3) as u32)
                .ok_or_else(|| CpuError("tags fora da memória".into()))?;
        }
        Ok(tags)
    }
    fn text_model_cancel(&mut self, l: u32) -> Result<(), CpuError> {
        if !self.modern_font_valid(l, 24) {
            return Ok(());
        }
        let owner = self.cpu.read_u32(l + 20)?;
        if self.objects.kind_of(owner) != Some(Interface::TextModel) {
            return Ok(());
        }
        let state = self.cpu.read_u32(owner + 4)?;
        let prev = self.cpu.read_u32(l + 4)?;
        let next = self.cpu.read_u32(l)?;
        if prev == 0 {
            if self.cpu.read_u32(state + LISTENERS)? == l {
                self.cpu.write_u32(state + LISTENERS, next)?;
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
    fn text_model_notify(&mut self, owner: u32, state: u32, event: u32) -> Result<(), CpuError> {
        if !self.modern_font_valid(event, 12) {
            return Err(CpuError("evento de texto inválido".into()));
        }
        let mut entries = Vec::new();
        let mut l = self.cpu.read_u32(state + LISTENERS)?;
        while l != 0 && entries.len() < 4096 && !entries.contains(&l) {
            if !self.modern_font_valid(l, 24) {
                break;
            }
            entries.push(l);
            l = self.cpu.read_u32(l)?;
        }
        for l in entries {
            if self.cpu.read_u32(l + 20)? != owner || self.cpu.read_u32(l + 16)? == 0 {
                continue;
            }
            let f = self.cpu.read_u32(l + 8)?;
            let context = self.cpu.read_u32(l + 12)?;
            if f != 0 {
                self.modern_ui_call(f, [context, event, 0, 0], &[])?;
            }
        }
        Ok(())
    }
    fn text_model_emit(
        &mut self,
        owner: u32,
        state: u32,
        code: u32,
        param: u32,
    ) -> Result<(), CpuError> {
        if self.cpu.read_u32(state + LISTENERS)? == 0 {
            return Ok(());
        }
        let original = self.cpu.read_reg(Reg::Sp);
        let sp = original
            .checked_sub(16)
            .ok_or_else(|| CpuError("pilha de evento de texto inválida".into()))?
            & !7;
        self.cpu.write_mem(
            sp,
            &[code, owner, param]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )?;
        self.cpu.write_reg(Reg::Sp, sp);
        let result = self.text_model_notify(owner, state, sp);
        self.cpu.write_reg(Reg::Sp, original);
        result
    }
    fn text_model_release(&mut self, owner: u32, state: u32) -> Result<u32, CpuError> {
        if self.cpu.read_u32(state + DESTROY)? != 0 {
            return Ok(0);
        }
        if self.objects.contagem(owner) != 1 {
            return Ok(self.objects.release(owner));
        }
        self.cpu.write_u32(state + DESTROY, 1)?;
        let mut l = self.cpu.read_u32(state + LISTENERS)?;
        self.cpu.write_u32(state + LISTENERS, 0)?;
        let mut visited = Vec::new();
        while l != 0 && visited.len() < 4096 && !visited.contains(&l) {
            if !self.modern_font_valid(l, 24) {
                break;
            }
            visited.push(l);
            let next = self.cpu.read_u32(l)?;
            if self.cpu.read_u32(l + 20)? == owner {
                for at in [0, 4, 16, 20] {
                    self.cpu.write_u32(l + at, 0)?;
                }
            }
            l = next;
        }
        if self.cpu.read_u32(state + CAP)? > 0 {
            self.heap.free(self.cpu.read_u32(state + TEXT)?);
        }
        let tags = self.cpu.read_u32(state + TAGS)?;
        if tags != 0 {
            self.heap.free(tags);
        }
        let rich = self.cpu.read_u32(owner + 8)?;
        self.objects.release(rich);
        self.objects.release(owner);
        self.heap.free(state);
        Ok(0)
    }
    fn text_model_commit(
        &mut self,
        owner: u32,
        state: u32,
        units: &[u16],
        tags: &[TextTag],
        selection: (u32, u32),
        param: u32,
    ) -> Result<u32, CpuError> {
        if units.len() > MAX_TEXT || tags.len() > MAX_TAGS {
            return Ok(UNSUPPORTED);
        }
        let old = self.cpu.read_u32(state + TEXT)?;
        let cap = self.cpu.read_u32(state + CAP)? as usize;
        let old_tags = self.cpu.read_u32(state + TAGS)?;
        let mut buffer = old;
        let mut new_cap = cap;
        if units.len() > cap {
            buffer = self.malloc(((units.len() + 1) * 2) as u32)?;
            if buffer == 0 {
                return Ok(OOM);
            }
            new_cap = (self
                .heap
                .size_of(buffer)
                .unwrap_or(((units.len() + 1) * 2) as u32) as usize
                / 2)
            .saturating_sub(1)
            .min(MAX_TEXT);
        }
        let bytes = tags
            .iter()
            .map(|t| (12 + (t.name.len() + 1) * 2 + 3) & !3)
            .sum::<usize>();
        let block = if bytes == 0 {
            0
        } else {
            match self.malloc(bytes as u32) {
                Ok(0) => {
                    if buffer != old {
                        self.heap.free(buffer);
                    }
                    return Ok(OOM);
                }
                Ok(p) => p,
                Err(e) => {
                    if buffer != old {
                        self.heap.free(buffer);
                    }
                    return Err(e);
                }
            }
        };
        let mut at = block;
        for tag in tags {
            let header = [tag.start as u32, tag.length as u32, tag.name.len() as u32];
            self.cpu.write_mem(
                at,
                &header
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )?;
            self.cpu.write_mem(
                at + 12,
                &tag.name
                    .iter()
                    .copied()
                    .chain([0])
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>(),
            )?;
            at += ((12 + (tag.name.len() + 1) * 2 + 3) & !3) as u32;
        }
        self.cpu.write_mem(
            buffer,
            &units
                .iter()
                .copied()
                .chain([0])
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        )?;
        for (field, value) in [
            (TEXT, buffer),
            (LENGTH, units.len() as u32),
            (CAP, new_cap as u32),
            (TAGS, block),
            (TAG_COUNT, tags.len() as u32),
            (START, selection.0),
            (END, selection.1),
        ] {
            self.cpu.write_u32(state + field, value)?;
        }
        if buffer != old && cap > 0 {
            self.heap.free(old);
        }
        if old_tags != 0 {
            self.heap.free(old_tags);
        }
        self.text_model_emit(owner, state, 0x1002, param)?;
        Ok(SUCCESS)
    }
    fn text_model_markup(&self, text: &[u16], tags: &[TextTag]) -> Vec<u16> {
        let mut out = Vec::new();
        let mut i = 0;
        let mut t = 0;
        while i < text.len() {
            if t < tags.len() && tags[t].start == i {
                let tag = &tags[t];
                out.push(91);
                out.extend(&tag.name);
                out.push(93);
                for &c in &text[i..i + tag.length] {
                    out.push(c);
                    if c == 91 {
                        out.push(91);
                    }
                }
                out.extend([91, 93]);
                i += tag.length;
                t += 1;
            } else {
                let c = text[i];
                out.push(c);
                if c == 91 {
                    out.push(91);
                }
                i += 1;
            }
        }
        out
    }
    fn text_model_parse_markup(&self, input: &[u16]) -> Result<(Vec<u16>, Vec<TextTag>), u32> {
        let mut text = Vec::new();
        let mut tags = Vec::new();
        let mut active: Option<(usize, Vec<u16>)> = None;
        let mut i = 0;
        while i < input.len() {
            if input[i] != 91 {
                text.push(input[i]);
                i += 1;
                continue;
            }
            if input.get(i + 1) == Some(&91) {
                text.push(91);
                i += 2;
                continue;
            }
            let end = input[i + 1..]
                .iter()
                .position(|c| *c == 93)
                .map(|n| i + 1 + n)
                .ok_or(1u32)?;
            let token = &input[i + 1..end];
            if token.is_empty() || token[0] == 47 {
                let (start, name) = active.take().ok_or(1u32)?;
                if !token.is_empty() && token[1..] != name {
                    return Err(1);
                }
                if text.len() > start {
                    tags.push(TextTag {
                        start,
                        length: text.len() - start,
                        name,
                        pointer: 0,
                    });
                }
            } else {
                if active.is_some() {
                    return Err(UNSUPPORTED);
                }
                if token.contains(&91) {
                    return Err(1);
                }
                active = Some((text.len(), token.to_vec()));
            }
            i = end + 1;
            if tags.len() > MAX_TAGS {
                return Err(UNSUPPORTED);
            }
        }
        if active.is_some() {
            return Err(1);
        }
        Ok((text, tags))
    }
    pub(super) fn text_model_call(
        &mut self,
        iface: Interface,
        slot: u32,
    ) -> Result<Option<u32>, CpuError> {
        let this = self.arg(0);
        if iface == Interface::TextModel && slot == 9 {
            self.text_model_cancel(this)?;
            return Ok(Some(this));
        }
        let Some((owner, state)) = self.text_model_state(iface, this) else {
            return Ok(Some(BAD));
        };
        if slot == 0 {
            return Ok(Some(self.objects.add_ref(owner)));
        }
        if slot == 1 {
            return Ok(Some(self.text_model_release(owner, state)?));
        }
        self.objects.add_ref(owner);
        let result = self.text_model_inner(iface, slot, owner, state, this);
        let cleanup = self.text_model_release(owner, state);
        match result {
            Err(e) => {
                let _ = cleanup;
                Err(e)
            }
            Ok(v) => {
                cleanup?;
                Ok(v)
            }
        }
    }
    fn text_model_inner(
        &mut self,
        iface: Interface,
        slot: u32,
        owner: u32,
        state: u32,
        this: u32,
    ) -> Result<Option<u32>, CpuError> {
        let (a1, a2) = (self.arg(1), self.arg(2));
        if slot == 2 {
            if !self.modern_font_write(a2, &0u32.to_le_bytes()) {
                return Ok(Some(BAD));
            }
            let target = match a1 {
                0x01000001 | 0x0101593a | 0x01015946 => owner,
                0x0103969b => self.cpu.read_u32(owner + 8)?,
                _ => return Ok(Some(3)),
            };
            self.cpu.write_u32(a2, target)?;
            self.objects.add_ref(owner);
            return Ok(Some(SUCCESS));
        }
        if slot == 3 {
            if !self.modern_font_valid(a1, 24) || self.cpu.read_u32(a1 + 8)? == 0 {
                return Ok(Some(BAD));
            }
            let cancel = self.cpu.read_u32(a1 + 16)?;
            if cancel != 0 {
                self.modern_ui_call(cancel, [a1, 0, 0, 0], &[])?;
            }
            let head = self.cpu.read_u32(state + LISTENERS)?;
            self.cpu.write_u32(a1, head)?;
            self.cpu.write_u32(a1 + 4, 0)?;
            if head != 0 {
                self.cpu.write_u32(head + 4, a1)?;
            }
            self.cpu
                .write_u32(a1 + 16, aee::encode(Interface::TextModel, 9))?;
            self.cpu.write_u32(a1 + 20, owner)?;
            self.cpu.write_u32(state + LISTENERS, a1)?;
            return Ok(Some(SUCCESS));
        }
        if slot == 4 {
            self.text_model_notify(owner, state, a1)?;
            return Ok(Some(this));
        }
        let text = self.text_model_text(state)?;
        let length = text.len() as u32;
        let result = if iface == Interface::TextModel {
            match slot {
                5 => {
                    let insert = match self.text_model_units(a1, a2 as i32) {
                        Ok(v) => v,
                        Err(e) => return Ok(Some(e)),
                    };
                    let start = self.cpu.read_u32(state + START)?.min(length);
                    let end = self.cpu.read_u32(state + END)?.min(length);
                    let lo = start.min(end) as usize;
                    let hi = start.max(end) as usize;
                    let tags = self.text_model_tags(state)?;
                    if !tags.is_empty() && (lo != 0 || hi != text.len()) {
                        return Ok(Some(UNSUPPORTED));
                    }
                    let total = text.len() - hi + lo + insert.len();
                    if total > MAX_TEXT {
                        return Ok(Some(UNSUPPORTED));
                    }
                    let mut value = text[..lo].to_vec();
                    value.extend(&insert);
                    value.extend(&text[hi..]);
                    let caret = (lo + insert.len()) as u32;
                    self.text_model_commit(owner, state, &value, &[], (caret, caret), lo as u32)?
                }
                6 => {
                    let start = self.cpu.read_u32(state + START)?.min(length);
                    let end = self.cpu.read_u32(state + END)?.min(length);
                    let lo = start.min(end) as usize;
                    let hi = start.max(end) as usize;
                    if (a2 as i32) > 0 {
                        let count = (hi - lo).min(a2 as usize - 1);
                        let bytes = text[lo..lo + count]
                            .iter()
                            .copied()
                            .chain([0])
                            .flat_map(u16::to_le_bytes)
                            .collect::<Vec<_>>();
                        if !self.modern_font_write(a1, &bytes) {
                            return Ok(Some(BAD));
                        }
                    }
                    (hi - lo) as u32
                }
                7 => {
                    let data = [
                        self.cpu.read_u32(state + TEXT)?,
                        length,
                        self.cpu.read_u32(state + START)?,
                        self.cpu.read_u32(state + END)?,
                    ];
                    if !self.modern_font_write(
                        a1,
                        &data
                            .into_iter()
                            .flat_map(u32::to_le_bytes)
                            .collect::<Vec<_>>(),
                    ) {
                        return Ok(Some(BAD));
                    }
                    this
                }
                8 => {
                    let clamp = |n: u32| {
                        if (n as i32) < 0 {
                            length
                        } else {
                            n.min(length)
                        }
                    };
                    let start = clamp(a1);
                    let end = clamp(a2);
                    self.cpu.write_u32(state + START, start)?;
                    self.cpu.write_u32(state + END, end)?;
                    self.text_model_emit(owner, state, 0x1003, start.min(end))?;
                    this
                }
                _ => return Ok(None),
            }
        } else {
            let mut tags = self.text_model_tags(state)?;
            match slot {
                5 => {
                    if !self.modern_font_valid(a2, 4) {
                        return Ok(Some(BAD));
                    }
                    let capacity = self.cpu.read_u32(a2)?;
                    if (capacity as i32) < 0 {
                        return Ok(Some(BAD));
                    }
                    let markup = self.text_model_markup(&text, &tags);
                    let required = ((markup.len() + 1) * 2) as u32;
                    if a1 != 0 && capacity >= 2 {
                        let count = markup.len().min(capacity as usize / 2 - 1);
                        let bytes = markup[..count]
                            .iter()
                            .copied()
                            .chain([0])
                            .flat_map(u16::to_le_bytes)
                            .collect::<Vec<_>>();
                        if !self.modern_font_write(a1, &bytes) {
                            return Ok(Some(BAD));
                        }
                    }
                    self.cpu.write_u32(a2, required)?;
                    SUCCESS
                }
                6 => {
                    let input = match self.text_model_units(a1, a2 as i32) {
                        Ok(v) => v,
                        Err(e) => return Ok(Some(e)),
                    };
                    let (units, tags) = match self.text_model_parse_markup(&input) {
                        Ok(v) => v,
                        Err(e) => return Ok(Some(e)),
                    };
                    let caret = units.len() as u32;
                    self.text_model_commit(owner, state, &units, &tags, (caret, caret), 0)?
                }
                7 => {
                    if !self.modern_font_valid(a1, 16) {
                        return Ok(Some(BAD));
                    }
                    let start = self.cpu.read_u32(a1)?;
                    let len = self.cpu.read_u32(a1 + 4)?;
                    let n = self.cpu.read_u32(a1 + 8)?;
                    let p = self.cpu.read_u32(a1 + 12)?;
                    if (n as i32) <= 0
                        || (start as i32) < 0
                        || len == 0
                        || (len as i32) < 0
                        || start.checked_add(len).is_none_or(|v| v > length)
                    {
                        return Ok(Some(BAD));
                    }
                    let mut name = match self.text_model_units(p, n as i32) {
                        Ok(v) => v,
                        Err(e) => return Ok(Some(e)),
                    };
                    while name.last() == Some(&0) {
                        name.pop();
                    }
                    if name.is_empty() || name.iter().any(|c| matches!(*c, 47 | 91 | 93)) {
                        return Ok(Some(BAD));
                    }
                    if tags.len() >= MAX_TAGS {
                        return Ok(Some(UNSUPPORTED));
                    }
                    if tags.iter().any(|t| {
                        (start as usize) < t.start + t.length && t.start < (start + len) as usize
                    }) {
                        return Ok(Some(BAD));
                    }
                    tags.push(TextTag {
                        start: start as usize,
                        length: len as usize,
                        name,
                        pointer: 0,
                    });
                    tags.sort_by_key(|t| t.start);
                    self.text_model_commit(
                        owner,
                        state,
                        &text,
                        &tags,
                        (
                            self.cpu.read_u32(state + START)?,
                            self.cpu.read_u32(state + END)?,
                        ),
                        start,
                    )?
                }
                8 => tags.len() as u32,
                9 => {
                    let Some(tag) = tags.get(a1 as usize) else {
                        return Ok(Some(BAD));
                    };
                    let data = [
                        tag.start as u32,
                        tag.length as u32,
                        tag.name.len() as u32,
                        tag.pointer,
                    ];
                    if !self.modern_font_write(
                        a2,
                        &data
                            .into_iter()
                            .flat_map(u32::to_le_bytes)
                            .collect::<Vec<_>>(),
                    ) {
                        return Ok(Some(BAD));
                    }
                    SUCCESS
                }
                10 => {
                    if a1 as usize >= tags.len() {
                        return Ok(Some(BAD));
                    }
                    tags.remove(a1 as usize);
                    self.text_model_commit(
                        owner,
                        state,
                        &text,
                        &tags,
                        (
                            self.cpu.read_u32(state + START)?,
                            self.cpu.read_u32(state + END)?,
                        ),
                        0,
                    )?
                }
                11 => {
                    let block = self.cpu.read_u32(state + TAGS)?;
                    self.cpu.write_u32(state + TAGS, 0)?;
                    self.cpu.write_u32(state + TAG_COUNT, 0)?;
                    if block != 0 {
                        self.heap.free(block);
                    }
                    self.text_model_emit(owner, state, 0x1002, 0)?;
                    this
                }
                _ => return Ok(None),
            }
        };
        Ok(Some(result))
    }
}
