//! CBTFETextLayout virtual: cache UTF16 salvo, fontes retidas e glyphs verdadeiros.
use super::*;
pub(super) const CLASS_ID: u32 = 0x0103_bda6;
const BAD: u32 = 14;
const UNSUP: u32 = 20;
const STATE: u32 = 64;
const FONT: u32 = 0;
const MAP: u32 = 4;
const CACHE: u32 = 8;
const WIDTH: u32 = 12;
const HEIGHT: u32 = 16;
const FLAGS: u32 = 20;
const GAP: u32 = 24;
const HINT_W: u32 = 28;
const HINT_ROWS: u32 = 32;
const FG: u32 = 36;
const BG: u32 = 40;
const ELLIPSIS: u32 = 52;
const MODEL: u32 = 56;
const MULTI: u32 = 0x1000_0000;
const WORD: u32 = 0x2000_0000;
const ELLIPSIFY: u32 = 0x4000_0000;
const TRIM: u32 = 0x8000_0000;
#[derive(Debug)]
enum LayoutFailure {
    Status(u32),
    Cpu(CpuError),
}
impl From<CpuError> for LayoutFailure {
    fn from(e: CpuError) -> Self {
        Self::Cpu(e)
    }
}
type LResult<T> = Result<T, LayoutFailure>;
#[derive(Clone)]
struct LayoutLine {
    start: u32,
    end: u32,
    width: u32,
    height: u32,
    y: u32,
    first: u32,
    count: u32,
}
#[derive(Clone)]
struct LayoutRun {
    font: u32,
    start: u32,
    len: u32,
    x: u32,
    y: u32,
    width: u32,
}
struct LayoutCache {
    text: Vec<u16>,
    lines: Vec<LayoutLine>,
    runs: Vec<LayoutRun>,
    ellipsis: u16,
}
struct LayoutInput {
    text: Vec<u16>,
    fonts: Vec<u32>,
    held: Vec<u32>,
}
impl<C: CpuBackend> Machine<C> {
    pub(super) fn modern_text_model_pointer(&self, p: u32) -> bool {
        // QI de algumas interfaces legadas ainda e permissivo. Isso nao
        // transforma seus objetos nativos em modelos de texto modernos.
        self.objects.kind_of(p).is_none_or(|kind| {
            matches!(
                kind,
                Interface::ValueModel | Interface::TextModel | Interface::RichTextModel
            )
        })
    }

    fn tl_state(&self, p: u32) -> Option<u32> {
        if self.objects.kind_of(p) != Some(Interface::TextLayout) {
            return None;
        }
        let s = self.cpu.read_u32(p + 4).ok()?;
        self.heap.size_of(s).filter(|n| *n >= STATE).map(|_| s)
    }
    pub(super) fn create_text_layout(&mut self, out: u32) -> Result<u32, CpuError> {
        if !self.modern_font_write(out, &0u32.to_le_bytes()) {
            return Ok(BAD);
        }
        let p = self.new_object(Interface::TextLayout)?;
        if p == 0 {
            return Ok(2);
        }
        let s = self.malloc(STATE)?;
        if s == 0 {
            self.objects.release(p);
            return Ok(2);
        }
        self.cpu.write_mem(s, &[0; STATE as usize])?;
        self.cpu.write_mem(p + 4, &[0; 60])?;
        self.cpu.write_u32(p + 4, s)?;
        let status = self.create_modern_hfont(s + FONT, 0, 16, 0, 0, false)?;
        if status != 0 {
            self.heap.free(s);
            self.objects.release(p);
            return Ok(status);
        }
        self.cpu.write_u32(s + FG, 0xff)?;
        self.cpu.write_u32(s + ELLIPSIS, 0x2026)?;
        self.cpu.write_u32(out, p)?;
        Ok(0)
    }
    fn tl_acquire(&mut self, p: u32, id: u32) -> LResult<u32> {
        self.modern_ui_scratch(8, |m, out| {
            let rc = m.modern_ui_invoke(p, 2, [id, out, 0], &[])?;
            Ok(if rc == 0 { m.cpu.read_u32(out)? } else { 0 })
        })
        .map_err(Into::into)
    }
    fn tl_units(&self, p: u32, n: u32) -> LResult<Vec<u16>> {
        if n > 4096 || (n != 0 && !self.modern_font_valid(p, n as usize * 2)) {
            return Err(LayoutFailure::Status(BAD));
        }
        let mut bytes = vec![0; n as usize * 2];
        if n != 0 {
            self.cpu.read_mem(p, &mut bytes)?;
        }
        let text = bytes
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect::<Vec<_>>();
        if String::from_utf16(&text).is_err() {
            return Err(LayoutFailure::Status(BAD));
        }
        Ok(text)
    }
    fn tl_lookup_font(&mut self, map: u32, name: Option<&[u16]>, fallback: u32) -> LResult<u32> {
        if map == 0 {
            self.modern_font_base(fallback, 0)?;
            return Ok(fallback);
        }
        let bytes = name.map_or(0, |n| (n.len() + 1) as u32 * 2);
        let result = self.modern_ui_scratch(8 + bytes, |m, p| {
            let namep = if let Some(name) = name {
                m.cpu.write_mem(
                    p + 8,
                    &name
                        .iter()
                        .copied()
                        .chain([0])
                        .flat_map(u16::to_le_bytes)
                        .collect::<Vec<_>>(),
                )?;
                p + 8
            } else {
                0
            };
            let rc = m.modern_ui_invoke(map, 7, [namep, p, 0], &[])?;
            Ok(if rc == 0 { m.cpu.read_u32(p)? } else { 0 })
        })?;
        if result == 0 {
            self.modern_font_base(fallback, 0)?;
            Ok(fallback)
        } else if self.objects.kind_of(result) == Some(Interface::ModernHFont) {
            Ok(result)
        } else {
            self.modern_font_base(result, 1)?;
            Err(LayoutFailure::Status(UNSUP))
        }
    }
    fn tl_input(&mut self, s: u32, model: u32) -> LResult<LayoutInput> {
        if !self.modern_font_valid(model, 4) {
            return Err(LayoutFailure::Status(BAD));
        }
        if !self.modern_text_model_pointer(model) {
            return Err(LayoutFailure::Status(3));
        }
        let mut held = Vec::new();
        let result = (|| {
            let textmodel = self.tl_acquire(model, 0x01015946)?;
            let text = if textmodel != 0 {
                held.push(textmodel);
                let (p, n) = self.modern_ui_scratch(16, |m, out| {
                    m.modern_ui_invoke(textmodel, 7, [out, 0, 0], &[])?;
                    Ok((m.cpu.read_u32(out)?, m.cpu.read_u32(out + 4)?))
                })?;
                self.tl_units(p, n)?
            } else {
                let value = self.tl_acquire(model, 0x0101593b)?;
                if value == 0 {
                    return Err(LayoutFailure::Status(3));
                }
                held.push(value);
                let (p, n) = self.modern_ui_scratch(8, |m, out| {
                    let p = m.modern_ui_invoke(value, 6, [out, 0, 0], &[])?;
                    Ok((p, m.cpu.read_u32(out)?))
                })?;
                self.tl_units(p, n)?
            };
            let map = self.cpu.read_u32(s + MAP)?;
            let fallback = self.cpu.read_u32(s + FONT)?;
            // QI e tags podem substituir o mapa. A fonte base resolvida abaixo já tem
            // sua própria referência e também preserva o fallback usado pela operação.
            if map != 0 {
                self.modern_font_base(map, 0)?;
                held.push(map);
            }
            let base = self.tl_lookup_font(map, None, fallback)?;
            held.push(base);
            let mut fonts = vec![base; text.len()];
            let rich = self.tl_acquire(model, 0x0103969b)?;
            if rich != 0 {
                held.push(rich);
                let n = self.modern_ui_invoke(rich, 8, [0; 3], &[])?;
                if n > 1024 {
                    return Err(LayoutFailure::Status(BAD));
                }
                let mut end = 0;
                for index in 0..n {
                    let (start, len, name) = self.modern_ui_scratch(16, |m, p| {
                        let rc = m.modern_ui_invoke(rich, 9, [index, p, 0], &[])?;
                        if rc != 0 {
                            return Ok((u32::MAX, 0, Vec::new()));
                        }
                        let start = m.cpu.read_u32(p)?;
                        let len = m.cpu.read_u32(p + 4)?;
                        let nn = m.cpu.read_u32(p + 8)?;
                        let np = m.cpu.read_u32(p + 12)?;
                        if nn > 128 || !m.modern_font_valid(np, nn as usize * 2) {
                            return Ok((u32::MAX, 0, Vec::new()));
                        }
                        let mut bytes = vec![0; nn as usize * 2];
                        m.cpu.read_mem(np, &mut bytes)?;
                        Ok((
                            start,
                            len,
                            bytes
                                .chunks_exact(2)
                                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                                .collect::<Vec<_>>(),
                        ))
                    })?;
                    let Some(next) = start.checked_add(len) else {
                        return Err(LayoutFailure::Status(BAD));
                    };
                    if start < end
                        || next > text.len() as u32
                        || String::from_utf16(&text[..start as usize]).is_err()
                        || String::from_utf16(&text[..next as usize]).is_err()
                    {
                        return Err(LayoutFailure::Status(BAD));
                    }
                    let font = self.tl_lookup_font(map, Some(&name), base)?;
                    held.push(font);
                    fonts[start as usize..next as usize].fill(font);
                    end = next;
                }
            }
            Ok((text, fonts))
        })();
        match result {
            Ok((text, fonts)) => Ok(LayoutInput { text, fonts, held }),
            Err(e) => {
                for p in held {
                    self.modern_font_base(p, 1)?;
                }
                Err(e)
            }
        }
    }
    fn tl_measure(&mut self, font: u32, text: &[u16]) -> LResult<u32> {
        if text.is_empty() {
            return Ok(0);
        }
        let (rc, pixels) = self.modern_ui_scratch(16 + (text.len() as u32 + 1) * 2, |m, p| {
            m.cpu.write_mem(
                p + 16,
                &text
                    .iter()
                    .copied()
                    .chain([0])
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>(),
            )?;
            let rc = m.modern_ui_invoke(
                font,
                4,
                [p + 16, text.len() as u32, i32::MAX as u32],
                &[p, p + 4],
            )?;
            Ok((rc, m.cpu.read_u32(p + 4)?))
        })?;
        if rc != 0 {
            Err(LayoutFailure::Status(rc))
        } else {
            Ok(pixels)
        }
    }
    fn tl_metrics(&mut self, font: u32) -> LResult<(u32, u32)> {
        let (rc, info) = self.modern_ui_scratch(8, |m, p| {
            let rc = m.modern_ui_invoke(font, 5, [p, 4, 0], &[])?;
            Ok((rc, m.cpu.read_u32(p)?))
        })?;
        if rc != 0 {
            return Err(LayoutFailure::Status(rc));
        }
        let (a, d) = (info as u16 as i16, (info >> 16) as u16 as i16);
        if a < 0 || d < 0 {
            return Err(LayoutFailure::Status(BAD));
        }
        Ok((a as u32, d as u32))
    }
    fn tl_span(
        &mut self,
        input: &LayoutInput,
        start: usize,
        end: usize,
    ) -> LResult<(u32, u32, u32, Vec<LayoutRun>)> {
        let mut runs = Vec::new();
        let (mut x, mut ascent, mut descent) = (0u32, 0u32, 0u32);
        let mut p = start;
        while p < end {
            let font = input.fonts[p];
            let mut z = p + 1;
            while z < end && input.fonts[z] == font {
                z += 1
            }
            let width = self.tl_measure(font, &input.text[p..z])?;
            let (a, d) = self.tl_metrics(font)?;
            ascent = ascent.max(a);
            descent = descent.max(d);
            runs.push(LayoutRun {
                font,
                start: p as u32,
                len: (z - p) as u32,
                x,
                y: a,
                width,
            });
            x = x.saturating_add(width);
            p = z;
        }
        if start == end {
            let font = input
                .held
                .iter()
                .copied()
                .find(|p| self.objects.kind_of(*p) == Some(Interface::ModernHFont))
                .ok_or(LayoutFailure::Status(BAD))?;
            let (a, d) = self.tl_metrics(font)?;
            ascent = a;
            descent = d;
        }
        for run in &mut runs {
            run.y = ascent - run.y;
        }
        Ok((x, ascent, descent, runs))
    }
    fn tl_compose(
        &mut self,
        s: u32,
        input: &LayoutInput,
        width: u32,
        flags: u32,
    ) -> LResult<LayoutCache> {
        if flags & !(MULTI | WORD | ELLIPSIFY | TRIM) != 0 {
            return Err(LayoutFailure::Status(UNSUP));
        }
        let ellipsis = self.cpu.read_u32(s + ELLIPSIS)?;
        if ellipsis > u16::MAX as u32 || char::from_u32(ellipsis).is_none() {
            return Err(LayoutFailure::Status(BAD));
        }
        let mut lines = Vec::new();
        let mut runs = Vec::new();
        let mut y = 0u32;
        let mut start = 0usize;
        let n = input.text.len();
        loop {
            let end = (start..n).find(|i| input.text[*i] == 10).unwrap_or(n);
            let mut a = start;
            loop {
                let mut b = end;
                if flags & MULTI != 0 && width > 0 && a < end {
                    let mut scan = a;
                    let mut fit = a;
                    while scan < end {
                        let step = if (0xd800..=0xdbff).contains(&input.text[scan]) {
                            2
                        } else {
                            1
                        };
                        scan += step;
                        if self.tl_span(input, a, scan)?.0 > width {
                            break;
                        }
                        fit = scan;
                    }
                    if fit < end {
                        if fit == a {
                            fit = a + if (0xd800..=0xdbff).contains(&input.text[a]) {
                                2
                            } else {
                                1
                            };
                        }
                        b = fit;
                        if flags & WORD != 0 {
                            if let Some(space) = (a..fit).rev().find(|i| input.text[*i] == 32) {
                                if space > a {
                                    b = space + 1;
                                }
                            }
                        }
                    }
                }
                let mut drawn_end = b;
                if flags & TRIM != 0 {
                    while drawn_end > a && matches!(input.text[drawn_end - 1], 9 | 13 | 32) {
                        drawn_end -= 1
                    }
                }
                let (w, asc, desc, mut part) = self.tl_span(input, a, drawn_end)?;
                let height = asc.saturating_add(desc);
                let first = runs.len() as u32;
                for r in &mut part {
                    r.y = r.y.saturating_add(y);
                }
                let count = part.len() as u32;
                runs.extend(part);
                lines.push(LayoutLine {
                    start: a as u32,
                    end: b as u32,
                    width: w,
                    height,
                    y,
                    first,
                    count,
                });
                if lines.len() > 4096 || runs.len() > 8192 {
                    return Err(LayoutFailure::Status(BAD));
                }
                y = y
                    .saturating_add(height)
                    .saturating_add(self.cpu.read_u32(s + GAP)?);
                if b >= end {
                    break;
                }
                a = b;
            }
            if end >= n {
                break;
            }
            start = end + 1;
        }
        Ok(LayoutCache {
            text: input.text.clone(),
            lines,
            runs,
            ellipsis: ellipsis as u16,
        })
    }
    fn tl_ellipsify(
        &mut self,
        input: &LayoutInput,
        cache: &mut LayoutCache,
        width: Option<u32>,
        height: Option<u32>,
        flags: u32,
    ) -> LResult<()> {
        let mut keep = cache.lines.len();
        if let Some(height) = height {
            keep = cache
                .lines
                .iter()
                .take_while(|line| line.y.saturating_add(line.height) <= height)
                .count();
        }
        if flags & MULTI == 0 && (width.is_some() || height.is_some()) {
            keep = keep.min(1);
        }
        let hidden = keep < cache.lines.len();
        let mut lines = Vec::new();
        let mut runs = Vec::new();
        for (index, original) in cache.lines.iter().take(keep).enumerate() {
            let mut line = original.clone();
            let first = runs.len() as u32;
            let force = hidden && index + 1 == keep;
            if !force && width.is_none_or(|limit| line.width <= limit) {
                runs.extend_from_slice(
                    &cache.runs[line.first as usize..(line.first + line.count) as usize],
                );
            } else {
                let limit = width.unwrap_or(u32::MAX);
                let start = line.start as usize;
                let mut cut = cache.runs[line.first as usize..(line.first + line.count) as usize]
                    .last()
                    .map_or(start, |run| (run.start + run.len) as usize);
                loop {
                    let font = if cut > start {
                        input.fonts[cut - 1]
                    } else {
                        cache.runs[line.first as usize..(line.first + line.count) as usize]
                            .first()
                            .map(|run| run.font)
                            .or_else(|| {
                                input.held.iter().copied().find(|p| {
                                    self.objects.kind_of(*p) == Some(Interface::ModernHFont)
                                })
                            })
                            .ok_or(LayoutFailure::Status(BAD))?
                    };
                    let marker_width = self.tl_measure(font, &[cache.ellipsis])?;
                    // Uma elipse usa a fonte da linha capturada. Um prefixo vazio
                    // não pode acrescentar métricas da fonte base a uma linha rica.
                    let (prefix_width, ascent, descent, mut prefix) = if cut == start {
                        (0, 0, 0, Vec::new())
                    } else {
                        self.tl_span(input, start, cut)?
                    };
                    if marker_width <= limit && prefix_width.saturating_add(marker_width) <= limit {
                        let (ma, md) = self.tl_metrics(font)?;
                        let baseline = ascent.max(ma);
                        for run in &mut prefix {
                            run.y = run
                                .y
                                .saturating_add(line.y)
                                .saturating_add(baseline - ascent);
                        }
                        runs.extend(prefix);
                        // len0 identifica apenas o glifo de decoração. O texto original
                        // e seus índices UTF16 continuam intactos para consultas/caret.
                        runs.push(LayoutRun {
                            font,
                            start: cut as u32,
                            len: 0,
                            x: prefix_width,
                            y: line.y.saturating_add(baseline - ma),
                            width: marker_width,
                        });
                        line.width = prefix_width + marker_width;
                        line.height = baseline.saturating_add(descent.max(md));
                        break;
                    }
                    if cut == start {
                        line.width = 0;
                        break;
                    }
                    cut -= 1;
                    if cut > start && (0xdc00..=0xdfff).contains(&input.text[cut]) {
                        cut -= 1;
                    }
                }
            }
            line.first = first;
            line.count = runs.len() as u32 - first;
            lines.push(line);
        }
        cache.lines = lines;
        cache.runs = runs;
        Ok(())
    }
    fn tl_drop_cache(&mut self, s: u32) -> Result<(), CpuError> {
        let p = self.cpu.read_u32(s + CACHE)?;
        self.cpu.write_u32(s + CACHE, 0)?;
        if p != 0 {
            if let Ok(cache) = self.tl_read_cache(p) {
                for run in cache.runs {
                    self.modern_font_base(run.font, 1)?;
                }
            }
            self.heap.free(p);
        }
        let model = self.cpu.read_u32(s + MODEL)?;
        self.cpu.write_u32(s + MODEL, 0)?;
        self.modern_font_base(model, 1)?;
        Ok(())
    }
    fn tl_store_cache(
        &mut self,
        s: u32,
        cache: &LayoutCache,
        model: u32,
        width: u32,
        height: u32,
        flags: u32,
    ) -> LResult<()> {
        let bytes = 16 + cache.lines.len() * 28 + cache.runs.len() * 24 + cache.text.len() * 2;
        let p = self.malloc(bytes as u32)?;
        if p == 0 {
            return Err(LayoutFailure::Status(2));
        }
        let mut out = Vec::with_capacity(bytes);
        for v in [
            cache.lines.len() as u32,
            cache.runs.len() as u32,
            cache.text.len() as u32,
            cache.ellipsis as u32,
        ] {
            out.extend(v.to_le_bytes());
        }
        for l in &cache.lines {
            for v in [l.start, l.end, l.width, l.height, l.y, l.first, l.count] {
                out.extend(v.to_le_bytes());
            }
        }
        for r in &cache.runs {
            for v in [r.font, r.start, r.len, r.x, r.y, r.width] {
                out.extend(v.to_le_bytes());
            }
        }
        out.extend(cache.text.iter().copied().flat_map(u16::to_le_bytes));
        self.cpu.write_mem(p, &out)?;
        for r in &cache.runs {
            self.modern_font_base(r.font, 0)?;
        }
        self.modern_font_base(model, 0)?;
        self.tl_drop_cache(s)?;
        self.cpu.write_u32(s + CACHE, p)?;
        self.cpu.write_u32(s + MODEL, model)?;
        for (f, v) in [(WIDTH, width), (HEIGHT, height), (FLAGS, flags)] {
            self.cpu.write_u32(s + f, v)?;
        }
        Ok(())
    }
    fn tl_read_cache(&self, p: u32) -> LResult<LayoutCache> {
        if p == 0 {
            return Err(LayoutFailure::Status(1));
        }
        let nl = self.cpu.read_u32(p)? as usize;
        let nr = self.cpu.read_u32(p + 4)? as usize;
        let nt = self.cpu.read_u32(p + 8)? as usize;
        let ellipsis = self.cpu.read_u32(p + 12)?;
        if nl > 4096
            || nr > 8192
            || nt > 4096
            || ellipsis > u16::MAX as u32
            || char::from_u32(ellipsis).is_none()
            || self
                .heap
                .size_of(p)
                .is_none_or(|size| (size as usize) < 16 + nl * 28 + nr * 24 + nt * 2)
        {
            return Err(LayoutFailure::Status(BAD));
        }
        let mut lines = Vec::new();
        for i in 0..nl {
            let a = p + 16 + i as u32 * 28;
            let v = (0..7)
                .map(|j| self.cpu.read_u32(a + j * 4))
                .collect::<Result<Vec<_>, _>>()?;
            if v[0] > v[1]
                || v[1] > nt as u32
                || v[5].checked_add(v[6]).is_none_or(|end| end > nr as u32)
            {
                return Err(LayoutFailure::Status(BAD));
            }
            lines.push(LayoutLine {
                start: v[0],
                end: v[1],
                width: v[2],
                height: v[3],
                y: v[4],
                first: v[5],
                count: v[6],
            });
        }
        let mut runs = Vec::new();
        for i in 0..nr {
            let a = p + 16 + nl as u32 * 28 + i as u32 * 24;
            let v = (0..6)
                .map(|j| self.cpu.read_u32(a + j * 4))
                .collect::<Result<Vec<_>, _>>()?;
            if v[1].checked_add(v[2]).is_none_or(|z| z > nt as u32) {
                return Err(LayoutFailure::Status(BAD));
            }
            runs.push(LayoutRun {
                font: v[0],
                start: v[1],
                len: v[2],
                x: v[3],
                y: v[4],
                width: v[5],
            });
        }
        let text = self.tl_units(p + 16 + nl as u32 * 28 + nr as u32 * 24, nt as u32)?;
        Ok(LayoutCache {
            text,
            lines,
            runs,
            ellipsis: ellipsis as u16,
        })
    }
    fn tl_geometry(cache: &LayoutCache) -> (u32, u32) {
        (
            cache.lines.iter().map(|l| l.width).max().unwrap_or(0),
            cache
                .lines
                .last()
                .map_or(0, |l| l.y.saturating_add(l.height)),
        )
    }
    fn tl_layout_model(
        &mut self,
        s: u32,
        model: u32,
        width: u32,
        height: u32,
        flags: u32,
        store: bool,
    ) -> LResult<(u32, u32)> {
        let input = self.tl_input(s, model)?;
        let result = (|| {
            let mut cache = self.tl_compose(s, &input, width, flags)?;
            if flags & ELLIPSIFY != 0 {
                // HintWidth0 não limita a medida preferida; a extensão concreta
                // de Layout, inclusive largura0, continua sendo uma restrição.
                self.tl_ellipsify(
                    &input,
                    &mut cache,
                    if store || width != 0 {
                        Some(width)
                    } else {
                        None
                    },
                    if store { Some(height) } else { None },
                    flags,
                )?;
            }
            let extent = Self::tl_geometry(&cache);
            if store {
                self.tl_store_cache(s, &cache, model, width, height, flags)?;
            }
            Ok(extent)
        })();
        for p in input.held {
            self.modern_font_base(p, 1)?;
        }
        result
    }
    fn tl_draw(&mut self, s: u32, canvas: u32, x: i32, y: i32) -> LResult<u32> {
        let Some(&target) = self.canvases.get(&canvas) else {
            return Err(LayoutFailure::Status(UNSUP));
        };
        let cache = self.tl_read_cache(self.cpu.read_u32(s + CACHE)?)?;
        let surface = self
            .bitmaps
            .get(&target)
            .ok_or(LayoutFailure::Status(UNSUP))?;
        let w = self.cpu.read_u32(s + WIDTH)? as i32;
        let h = self.cpu.read_u32(s + HEIGHT)? as i32;
        let (mut x0, mut y0, mut x1, mut y1) = (
            x.max(0),
            y.max(0),
            x.saturating_add(w).min(surface.width() as i32),
            y.saturating_add(h).min(surface.height() as i32),
        );
        if let Some(c) = self.clip {
            x0 = x0.max(c.x as i32);
            y0 = y0.max(c.y as i32);
            x1 = x1.min(c.x as i32 + c.width as i32);
            y1 = y1.min(c.y as i32 + c.height as i32);
        }
        if x1 <= x0 || y1 <= y0 {
            return Ok(0);
        }
        let rect = Rect {
            x: x0 as i16,
            y: y0 as i16,
            width: (x1 - x0).min(i16::MAX as i32) as i16,
            height: (y1 - y0).min(i16::MAX as i32) as i16,
        };
        self.modern_widget_fill(target, rect, self.cpu.read_u32(s + BG)?)?;
        let color = self.cpu.read_u32(s + FG)?;
        let alpha = color & 255;
        if alpha == 0 {
            return Ok(0);
        }
        let before = if alpha < 255 {
            let fb = self.bitmaps.get(&target).unwrap();
            Some(
                (y0..y1)
                    .flat_map(|yy| (x0..x1).map(move |xx| (xx, yy, fb.get_pixel(xx, yy))))
                    .collect::<Vec<_>>(),
            )
        } else {
            None
        };
        for run in &cache.runs {
            let end = run.start as usize + run.len as usize;
            let marker = [cache.ellipsis];
            let text = if run.len == 0 {
                &marker[..]
            } else {
                &cache.text[run.start as usize..end]
            };
            let rc = self.modern_ui_scratch(16 + (text.len() as u32 + 1) * 2, |m, p| {
                m.cpu.write_mem(
                    p,
                    &[rect.x, rect.y, rect.width, rect.height]
                        .into_iter()
                        .flat_map(i16::to_le_bytes)
                        .collect::<Vec<_>>(),
                )?;
                m.cpu.write_mem(
                    p + 16,
                    &text
                        .iter()
                        .copied()
                        .chain([0])
                        .flat_map(u16::to_le_bytes)
                        .collect::<Vec<_>>(),
                )?;
                m.modern_ui_invoke(
                    run.font,
                    3,
                    [
                        target,
                        x.saturating_add(run.x as i32) as u32,
                        y.saturating_add(run.y as i32) as u32,
                    ],
                    &[
                        p + 16,
                        text.len() as u32,
                        Rgb::from_rgbval(color).to_rgb565() as u32,
                        0,
                        p,
                        0x8000,
                    ],
                )
            })?;
            if rc != 0 {
                return Err(LayoutFailure::Status(rc));
            }
        }
        if let Some(before) = before {
            let fb = self.bitmaps.get_mut(&target).unwrap();
            for (xx, yy, p) in before {
                let a = Rgb::from_rgb565(fb.get_pixel(xx, yy));
                let b = Rgb::from_rgb565(p);
                let mix = |a: u8, b: u8| {
                    ((u32::from(a) * alpha + u32::from(b) * (255 - alpha) + 127) / 255) as u8
                };
                fb.set_pixel(
                    xx,
                    yy,
                    Rgb {
                        r: mix(a.r, b.r),
                        g: mix(a.g, b.g),
                        b: mix(a.b, b.b),
                    },
                );
            }
        }
        Ok(0)
    }
    fn tl_caret(&mut self, cache: &LayoutCache, index: u32) -> LResult<Rect> {
        if index > cache.text.len() as u32
            || String::from_utf16(&cache.text[..index as usize]).is_err()
        {
            return Err(LayoutFailure::Status(BAD));
        }
        let line = cache
            .lines
            .iter()
            .rev()
            .find(|l| index >= l.start)
            .ok_or(LayoutFailure::Status(BAD))?;
        let mut x = 0;
        for run in &cache.runs[line.first as usize..(line.first + line.count) as usize] {
            if index <= run.start {
                break;
            }
            let n = (index - run.start).min(run.len);
            x = run.x.saturating_add(self.tl_measure(
                run.font,
                &cache.text[run.start as usize..(run.start + n) as usize],
            )?);
            if n < run.len {
                break;
            }
        }
        if x > i16::MAX as u32 || line.y > i16::MAX as u32 || line.height > i16::MAX as u32 {
            return Err(LayoutFailure::Status(BAD));
        }
        Ok(Rect {
            x: x as i16,
            y: line.y as i16,
            width: 1,
            height: line.height as i16,
        })
    }
    pub(super) fn text_layout_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let p = self.arg(0);
        let Some(s) = self.tl_state(p) else {
            return Ok(Some(BAD));
        };
        if slot > 2 {
            self.objects.add_ref(p);
        }
        let result = self.tl_call_inner(p, s, slot);
        if slot > 2 {
            self.modern_font_base(p, 1)?;
        }
        match result {
            Ok(v) => Ok(Some(v)),
            Err(LayoutFailure::Status(code)) => Ok(Some(code)),
            Err(LayoutFailure::Cpu(error)) => Err(error),
        }
    }
    fn tl_call_inner(&mut self, p: u32, s: u32, slot: u32) -> LResult<u32> {
        let (a, b, c) = (self.arg(1), self.arg(2), self.arg(3));
        Ok(match slot {
            0 => self.objects.add_ref(p),
            1 => {
                if self.objects.contagem(p) == 1 {
                    self.tl_drop_cache(s)?;
                    for f in [FONT, MAP] {
                        self.modern_font_base(self.cpu.read_u32(s + f)?, 1)?;
                    }
                    self.heap.free(s);
                }
                self.objects.release(p)
            }
            2 => {
                if !self.modern_font_write(b, &0u32.to_le_bytes()) {
                    return Err(LayoutFailure::Status(BAD));
                }
                if matches!(a, 0x01000001 | 0x0103bd09) {
                    self.cpu.write_u32(b, p)?;
                    self.objects.add_ref(p);
                    0
                } else {
                    3
                }
            }
            3 => {
                if !matches!(a, 0x800 | 0x801) {
                    return Ok(FALSE);
                }
                let set = a == 0x801;
                let field = match b {
                    0x152 => FONT,
                    0x418 => MAP,
                    0x303 => GAP,
                    0x173 => HINT_W,
                    0x171 => HINT_ROWS,
                    0x140 => FG,
                    0x130 => BG,
                    0x349 => ELLIPSIS,
                    _ => return Ok(FALSE),
                };
                if !set {
                    let value = self.cpu.read_u32(s + field)?;
                    if field == ELLIPSIS {
                        return Ok(self.modern_font_write(c, &(value as u16).to_le_bytes()) as u32);
                    }
                    if !self.modern_font_write(c, &value.to_le_bytes()) {
                        return Ok(FALSE);
                    }
                    if matches!(field, FONT | MAP) {
                        self.modern_font_base(value, 0)?;
                    }
                    TRUE
                } else {
                    if field == FONT && self.objects.kind_of(c) != Some(Interface::ModernHFont) {
                        return Ok(FALSE);
                    }
                    if field == MAP
                        && c != 0
                        && self.objects.kind_of(c) != Some(Interface::FontMapModel)
                    {
                        return Ok(FALSE);
                    }
                    if matches!(field, GAP | HINT_W | HINT_ROWS) && (c as i32) < 0 {
                        return Ok(FALSE);
                    }
                    if field == ELLIPSIS && (c > u16::MAX as u32 || char::from_u32(c).is_none()) {
                        return Ok(FALSE);
                    }
                    if matches!(field, FONT | MAP) {
                        self.modern_font_base(c, 0)?;
                        let old = self.cpu.read_u32(s + field)?;
                        self.cpu.write_u32(s + field, c)?;
                        self.modern_font_base(old, 1)?;
                    } else {
                        self.cpu.write_u32(s + field, c)?;
                    }
                    TRUE
                }
            }
            4 => {
                if !self.modern_font_valid(c, 8) {
                    return Err(LayoutFailure::Status(BAD));
                }
                let width = self.cpu.read_u32(s + HINT_W)?;
                let (w, mut h) = self.tl_layout_model(s, a, width, 0, b, false)?;
                let rows = self.cpu.read_u32(s + HINT_ROWS)?;
                if rows > 0 {
                    let font = self.cpu.read_u32(s + FONT)?;
                    let (asc, desc) = self.tl_metrics(font)?;
                    h = (asc + desc)
                        .saturating_add(self.cpu.read_u32(s + GAP)?)
                        .saturating_mul(rows)
                        .saturating_sub(self.cpu.read_u32(s + GAP)?);
                }
                self.cpu.write_u32(c, w)?;
                self.cpu.write_u32(c + 4, h)?;
                0
            }
            5 => {
                if !self.modern_font_valid(b, 8) {
                    return Err(LayoutFailure::Status(BAD));
                }
                let (w, h) = (self.cpu.read_u32(b)?, self.cpu.read_u32(b + 4)?);
                if (w as i32) < 0 || (h as i32) < 0 {
                    return Err(LayoutFailure::Status(BAD));
                }
                self.tl_layout_model(s, a, w, h, c, true)?;
                0
            }
            6 => self.tl_draw(s, a, b as i32, c as i32)?,
            7 => {
                if !self.modern_font_valid(a, 8) || (b != 0 && !self.modern_font_valid(b, 4)) {
                    return Err(LayoutFailure::Status(BAD));
                }
                let cache = self.tl_read_cache(self.cpu.read_u32(s + CACHE)?)?;
                let (w, h) = Self::tl_geometry(&cache);
                self.cpu.write_u32(a, w)?;
                self.cpu.write_u32(a + 4, h)?;
                if b != 0 {
                    self.cpu.write_u32(b, cache.lines.len() as u32)?;
                }
                0
            }
            8 | 9 => {
                if !self.modern_font_valid(b, 4) {
                    return Err(LayoutFailure::Status(BAD));
                }
                let cache = self.tl_read_cache(self.cpu.read_u32(s + CACHE)?)?;
                let value = if slot == 8 {
                    if a > cache.text.len() as u32 {
                        return Err(LayoutFailure::Status(BAD));
                    }
                    cache
                        .lines
                        .iter()
                        .rposition(|l| a >= l.start)
                        .ok_or(LayoutFailure::Status(BAD))? as u32
                } else {
                    cache
                        .lines
                        .get(a as usize)
                        .ok_or(LayoutFailure::Status(BAD))?
                        .start
                };
                self.cpu.write_u32(b, value)?;
                0
            }
            10 => {
                if !self.modern_font_valid(b, 8) {
                    return Err(LayoutFailure::Status(BAD));
                }
                let cache = self.tl_read_cache(self.cpu.read_u32(s + CACHE)?)?;
                let r = self.tl_caret(&cache, a)?;
                self.cpu.write_mem(
                    b,
                    &[r.x, r.y, r.width, r.height]
                        .into_iter()
                        .flat_map(i16::to_le_bytes)
                        .collect::<Vec<_>>(),
                )?;
                0
            }
            11 => {
                if !self.modern_font_valid(c, 4) {
                    return Err(LayoutFailure::Status(BAD));
                }
                return Err(LayoutFailure::Status(UNSUP));
            }
            12 => {
                if (a as i32) < 0 || (b as i32) < 0 {
                    return Err(LayoutFailure::Status(BAD));
                }
                if a <= b {
                    0
                } else {
                    let limit = (a - b).min(i32::MAX as u32) as i32;
                    (c as i32).clamp(-limit, 0) as u32
                }
            }
            _ => return Err(LayoutFailure::Status(UNSUP)),
        })
    }
}
