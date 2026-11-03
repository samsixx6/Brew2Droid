//! ITypeface/IHFont do Brew MP, separados do IFont legado.
//! A fonte portátil é NeoDunggeunmo, nunca uma identificação de Helvetica/QCSans OEM.
use super::*;
use ab_glyph::{Font as _, FontVec, GlyphId, PxScale, ScaleFont as _};
use std::{
    io::{Read, Seek, SeekFrom},
    ops::Deref,
    sync::OnceLock,
};

pub(super) const TYPEFACE_CLASS: u32 = 0x0103_5156;
pub(super) const VIRTUAL_FONT_CLASS: u32 = 0x0103_fb4b;
const BAD: u32 = 14;
const UNSUPPORTED: u32 = 20;
const OOM: u32 = 2;
const MAX_FONT: usize = 8 * 1024 * 1024;
const MAX_TEXT: u32 = 4096;
const SIZE: u32 = 4;
const OWNER: u32 = 8;
const DATA: u32 = 12;
const LENGTH: u32 = 16;
const OWNED: u32 = 20;
const EMPHASIS: u32 = 24;
const COLOR: u32 = 28;
const COLOR_SET: u32 = 32;
const RENDER: u32 = 36;
const SPACING: u32 = 40;
const DESCENT: u32 = 44;
const UNDERLINE: u32 = 48;
enum Face {
    Portable(&'static FontVec),
    Loaded(FontVec),
}
impl Deref for Face {
    type Target = FontVec;
    fn deref(&self) -> &FontVec {
        match self {
            Self::Portable(f) => f,
            Self::Loaded(f) => f,
        }
    }
}
fn portable() -> Option<&'static FontVec> {
    static FONT: OnceLock<Option<FontVec>> = OnceLock::new();
    FONT.get_or_init(|| {
        use sha2::{Digest, Sha256};
        let mut bytes = Vec::new();
        flate2::read::GzDecoder::new(
            &include_bytes!("../../assets/fonts/neodgm-v1.601.ttf.gz")[..],
        )
        .take(651045)
        .read_to_end(&mut bytes)
        .ok()?;
        if bytes.len() != 651044
            || Sha256::digest(&bytes)
                .iter()
                .map(|value| format!("{value:02x}"))
                .collect::<String>()
                != "77305267996073aae07bad9313dad2e306a4128e55bfafbed4c41558fee57b4d"
        {
            return None;
        }
        FontVec::try_from_vec(bytes).ok()
    })
    .as_ref()
}
fn advance(face: &FontVec, text: &str, size: f32, emphasis: u32, spacing: i32) -> u32 {
    let scaled = face.as_scaled(PxScale::from(size));
    let mut pen = 0.0;
    let mut prev: Option<GlyphId> = None;
    for c in text.chars() {
        let id = face.glyph_id(c);
        if let Some(previous) = prev {
            pen += scaled.kern(previous, id) + spacing as f32 / 1000.0;
        }
        pen += scaled.h_advance(id) + (emphasis & 1) as f32;
        prev = Some(id);
    }
    pen.ceil().max(0.0) as u32
}
fn blend(fg: u16, bg: u16, coverage: f32) -> u16 {
    let a = coverage.clamp(0.0, 1.0);
    let f = Rgb::from_rgb565(fg);
    let b = Rgb::from_rgb565(bg);
    Rgb {
        r: (f.r as f32 * a + b.r as f32 * (1.0 - a)).round() as u8,
        g: (f.g as f32 * a + b.g as f32 * (1.0 - a)).round() as u8,
        b: (f.b as f32 * a + b.b as f32 * (1.0 - a)).round() as u8,
    }
    .to_rgb565()
}
impl<C: CpuBackend> Machine<C> {
    pub(super) fn modern_font_valid(&self, p: u32, n: usize) -> bool {
        p != 0 && p.checked_add(n as u32).is_some() && self.cpu.read_mem(p, &mut vec![0; n]).is_ok()
    }
    pub(super) fn modern_font_write(&mut self, p: u32, bytes: &[u8]) -> bool {
        self.modern_font_valid(p, bytes.len()) && self.cpu.write_mem(p, bytes).is_ok()
    }
    pub(super) fn modern_font_base(&mut self, object: u32, slot: u32) -> Result<u32, CpuError> {
        if object == 0 {
            return Ok(0);
        }
        let vt = self.cpu.read_u32(object)?;
        let entry = self.cpu.read_u32(vt + slot * 4)?;
        match self.call_guest_aninhado(entry, [object, 0, 0, 0], QSORT_BUDGET)? {
            Outcome::Returned { code } => Ok(code),
            o => Err(CpuError(format!(
                "referência tipográfica interrompida: {o:?}"
            ))),
        }
    }
    fn modern_face(&self, font: u32) -> Result<Face, u32> {
        let p = self.cpu.read_u32(font + DATA).map_err(|_| BAD)?;
        if p == 0 {
            return portable().map(Face::Portable).ok_or(UNSUPPORTED);
        }
        let n = self.cpu.read_u32(font + LENGTH).map_err(|_| BAD)? as usize;
        if n == 0 || n > MAX_FONT {
            return Err(BAD);
        }
        let mut bytes = vec![0; n];
        self.cpu.read_mem(p, &mut bytes).map_err(|_| BAD)?;
        FontVec::try_from_vec(bytes)
            .map(Face::Loaded)
            .map_err(|_| UNSUPPORTED)
    }
    fn modern_font_text(&self, p: u32, n: u32) -> Result<String, u32> {
        if (n as i32) < 0 {
            return Err(BAD);
        }
        if n > MAX_TEXT {
            return Err(UNSUPPORTED);
        }
        if !self.modern_font_valid(p, (n as usize * 2).max(2)) {
            return Err(BAD);
        }
        let mut bytes = vec![0; n as usize * 2];
        self.cpu.read_mem(p, &mut bytes).map_err(|_| BAD)?;
        String::from_utf16(
            &bytes
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect::<Vec<_>>(),
        )
        .map_err(|_| BAD)
    }
    fn modern_font_name(&self, p: u32) -> Result<String, u32> {
        if p == 0 {
            return Err(BAD);
        }
        let mut bytes = Vec::new();
        for i in 0..4096 {
            let at = p.checked_add(i).ok_or(BAD)?;
            let mut b = [0];
            self.cpu.read_mem(at, &mut b).map_err(|_| BAD)?;
            if b[0] == 0 {
                return String::from_utf8(bytes).map_err(|_| BAD);
            }
            bytes.push(b[0]);
        }
        Err(BAD)
    }
    // Os perfis pertencem ao dispositivo virtual, nao a uma fonte OEM ausente.
    // AEEFont.h define as classes, mas nao fixa dimensoes em pixels.
    pub(super) fn create_virtual_system_font(
        &mut self,
        class: u32,
        out: u32,
    ) -> Result<u32, CpuError> {
        let (size, emphasis) = match class {
            0x0101_2786 => (18, 0),
            0x0101_2788 => (18, 1),
            0x0101_2787 => (28, 0),
            _ => return Ok(UNSUPPORTED),
        };
        let status = self.create_modern_hfont(out, 0, size, 0, 0, false)?;
        if status == SUCCESS {
            let font = self.cpu.read_u32(out)?;
            self.cpu.write_u32(font + EMPHASIS, emphasis)?;
        }
        Ok(status)
    }
    pub(super) fn create_modern_typeface(&mut self, out: u32) -> Result<u32, CpuError> {
        if !self.modern_font_write(out, &0u32.to_le_bytes()) {
            return Ok(BAD);
        }
        let face = self.new_object(Interface::ModernTypeface)?;
        if face == 0 {
            return Ok(OOM);
        }
        self.cpu.write_mem(face + 4, &[0; 60])?;
        self.cpu.write_u32(out, face)?;
        Ok(SUCCESS)
    }
    pub(super) fn create_modern_hfont(
        &mut self,
        out: u32,
        owner: u32,
        size: u32,
        data: u32,
        length: u32,
        owned: bool,
    ) -> Result<u32, CpuError> {
        if !self.modern_font_write(out, &0u32.to_le_bytes()) {
            if owned {
                self.heap.free(data);
            }
            return Ok(BAD);
        }
        if size == 0 || (size as i32) < 0 {
            if owned {
                self.heap.free(data);
            }
            return Ok(BAD);
        }
        if size > 256 {
            if owned {
                self.heap.free(data);
            }
            return Ok(UNSUPPORTED);
        }
        if data == 0 && portable().is_none() {
            return Ok(UNSUPPORTED);
        }
        let font = self.new_object(Interface::ModernHFont)?;
        if font == 0 {
            if owned {
                self.heap.free(data);
            }
            return Ok(OOM);
        }
        self.cpu.write_mem(font + 4, &[0; 60])?;
        for (field, value) in [
            (SIZE, size),
            (OWNER, owner),
            (DATA, data),
            (LENGTH, length),
            (OWNED, owned as u32),
            (RENDER, 2),
            (DESCENT, 2),
        ] {
            self.cpu.write_u32(font + field, value)?;
        }
        self.modern_font_base(owner, 0)?;
        self.cpu.write_u32(out, font)?;
        if data == 0 {
            self.assumptions.insert("IHFont do dispositivo virtual usa NeoDunggeunmo 1.601 redistribuível; não é a fonte OEM Helvetica/QCSans");
        }
        Ok(SUCCESS)
    }
    pub(super) fn modern_font_call(
        &mut self,
        iface: Interface,
        slot: u32,
    ) -> Result<Option<u32>, CpuError> {
        let this = self.arg(0);
        let (a1, a2, a3) = (self.arg(1), self.arg(2), self.arg(3));
        if self.objects.kind_of(this) != Some(iface) {
            return Ok(Some(BAD));
        }
        let result = match slot {
            0 => self.objects.add_ref(this),
            1 => {
                if iface == Interface::ModernHFont && self.objects.contagem(this) == 1 {
                    let owner = self.cpu.read_u32(this + OWNER)?;
                    self.cpu.write_u32(this + OWNER, 0)?;
                    if self.cpu.read_u32(this + OWNED)? != 0 {
                        let data = self.cpu.read_u32(this + DATA)?;
                        self.heap.free(data);
                    }
                    self.modern_font_base(owner, 1)?;
                }
                self.objects.release(this)
            }
            2 => {
                if !self.modern_font_write(a2, &0u32.to_le_bytes()) {
                    return Ok(Some(BAD));
                }
                let supported = a1 == 0x01000001
                    || if iface == Interface::ModernTypeface {
                        a1 == 0x010340cb
                    } else {
                        matches!(a1, 0x01001022 | 0x0102ef8e)
                    };
                if supported {
                    self.cpu.write_u32(a2, this)?;
                    self.objects.add_ref(this);
                    SUCCESS
                } else {
                    3
                }
            }
            _ if iface == Interface::ModernTypeface => match slot {
                3 | 4 => {
                    let (out, size) = if slot == 3 {
                        (
                            self.stack_arg(1).unwrap_or(0),
                            self.stack_arg(0).unwrap_or(0),
                        )
                    } else {
                        (self.stack_arg(0).unwrap_or(0), a3)
                    };
                    if !self.modern_font_write(out, &0u32.to_le_bytes()) {
                        return Ok(Some(BAD));
                    }
                    if size == 0 || (size as i32) < 0 {
                        return Ok(Some(BAD));
                    }
                    if size > 256 {
                        return Ok(Some(UNSUPPORTED));
                    }
                    if slot == 3 {
                        if (a2 as i32) < 0 || (a3 as i32) < 0 {
                            return Ok(Some(BAD));
                        }
                        let name = match self.modern_font_name(a1) {
                            Ok(n) => n,
                            Err(e) => return Ok(Some(e)),
                        };
                        let Some(path) = self.vfs.resolve(&name) else {
                            self.missing_files.insert(name);
                            return Ok(Some(0x101));
                        };
                        let metadata = match std::fs::metadata(&path) {
                            Ok(m) => m,
                            Err(_) => return Ok(Some(0x101)),
                        };
                        if !metadata.is_file() || u64::from(a2) > metadata.len() {
                            return Ok(Some(BAD));
                        }
                        let count = if a3 == 0 {
                            metadata.len() - u64::from(a2)
                        } else {
                            u64::from(a3)
                        };
                        if count == 0 || count > MAX_FONT as u64 {
                            return Ok(Some(UNSUPPORTED));
                        }
                        if u64::from(a2) + count > metadata.len() {
                            return Ok(Some(BAD));
                        }
                        let mut file = match std::fs::File::open(path) {
                            Ok(f) => f,
                            Err(_) => return Ok(Some(EFAILED)),
                        };
                        if file.seek(SeekFrom::Start(u64::from(a2))).is_err() {
                            return Ok(Some(EFAILED));
                        }
                        let mut bytes = vec![0; count as usize];
                        if file.read_exact(&mut bytes).is_err() {
                            return Ok(Some(EFAILED));
                        }
                        let font_bytes = bytes.as_slice();
                        if font_bytes.is_empty()
                            || font_bytes.len() > MAX_FONT
                            || FontVec::try_from_vec(font_bytes.to_vec()).is_err()
                        {
                            return Ok(Some(UNSUPPORTED));
                        }
                        let p = self.malloc(font_bytes.len() as u32)?;
                        if p == 0 {
                            return Ok(Some(OOM));
                        }
                        self.cpu.write_mem(p, font_bytes)?;
                        self.create_modern_hfont(out, this, size, p, font_bytes.len() as u32, true)?
                    } else {
                        if a2 == 0 || a2 as usize > MAX_FONT {
                            return Ok(Some(BAD));
                        }
                        let mut bytes = vec![0; a2 as usize];
                        if self.cpu.read_mem(a1, &mut bytes).is_err() {
                            return Ok(Some(BAD));
                        }
                        if FontVec::try_from_vec(bytes).is_err() {
                            return Ok(Some(UNSUPPORTED));
                        }
                        self.create_modern_hfont(out, this, size, a1, a2, false)?
                    }
                }
                5 => {
                    if !self.modern_font_write(a3, &0u32.to_le_bytes()) {
                        return Ok(Some(BAD));
                    }
                    if a1 != VIRTUAL_FONT_CLASS {
                        return Ok(Some(UNSUPPORTED));
                    }
                    self.create_modern_hfont(a3, this, a2, 0, 0, false)?
                }
                6 => {
                    if self.modern_font_write(a3, &0u32.to_le_bytes()) {
                        UNSUPPORTED
                    } else {
                        BAD
                    }
                }
                _ => return Ok(None),
            },
            3 => self.modern_font_draw(this, a1, a2 as i32, a3 as i32)?,
            4 => {
                let (out_fit, out_width) = (
                    self.stack_arg(0).unwrap_or(0),
                    self.stack_arg(1).unwrap_or(0),
                );
                if (a3 as i32) < 0
                    || !self.modern_font_valid(out_fit, 4)
                    || !self.modern_font_valid(out_width, 4)
                {
                    return Ok(Some(BAD));
                }
                let text = match self.modern_font_text(a1, a2) {
                    Ok(t) => t,
                    Err(e) => return Ok(Some(e)),
                };
                let face = match self.modern_face(this) {
                    Ok(f) => f,
                    Err(e) => return Ok(Some(e)),
                };
                let size = self.cpu.read_u32(this + SIZE)? as f32;
                let emphasis = self.cpu.read_u32(this + EMPHASIS)?;
                let spacing = self.cpu.read_u32(this + SPACING)? as i32;
                let (mut fit, mut width) = (0u32, 0u32);
                for (at, c) in text.char_indices() {
                    let end = at + c.len_utf8();
                    let w = advance(&face, &text[..end], size, emphasis, spacing);
                    if w <= a3 {
                        fit = text[..end].encode_utf16().count() as u32;
                        width = w;
                    } else {
                        break;
                    }
                }
                // Validar ambas as saídas antes da primeira escrita.
                if !self.modern_font_write(out_fit, &fit.to_le_bytes())
                    || !self.modern_font_write(out_width, &width.to_le_bytes())
                {
                    BAD
                } else {
                    SUCCESS
                }
            }
            5 => {
                if a2 != 4 {
                    return Ok(Some(UNSUPPORTED));
                }
                let face = match self.modern_face(this) {
                    Ok(f) => f,
                    Err(e) => return Ok(Some(e)),
                };
                let size = self.cpu.read_u32(this + SIZE)? as f32;
                let scaled = face.as_scaled(PxScale::from(size));
                let ascent = scaled.ascent().ceil().clamp(0.0, i16::MAX as f32) as i16;
                let descent = ((-scaled.descent()).ceil() as u32)
                    .saturating_add(self.cpu.read_u32(this + DESCENT)?)
                    .min(i16::MAX as u32) as i16;
                let bytes = [ascent, descent]
                    .into_iter()
                    .flat_map(i16::to_le_bytes)
                    .collect::<Vec<_>>();
                if self.modern_font_write(a1, &bytes) {
                    SUCCESS
                } else {
                    BAD
                }
            }
            6 => {
                if !matches!(a1, 0x800 | 0x801) {
                    return Ok(Some(FALSE));
                }
                let set = a1 == 0x801;
                let field = match a2 {
                    0x1001 => Some((EMPHASIS, 3)),
                    0x1003 => Some((RENDER, 4)),
                    0x1004 => Some((COLOR, u32::MAX)),
                    0x1006 => Some((UNDERLINE, 1)),
                    0x1010 => Some((SPACING, u32::MAX)),
                    0x1011 => Some((DESCENT, 256)),
                    _ => None,
                };
                if let Some((field, max)) = field {
                    if set {
                        if a3 > max || (field == RENDER && a3 == 0) {
                            return Ok(Some(FALSE));
                        }
                        self.cpu.write_u32(this + field, a3)?;
                        if field == COLOR {
                            self.cpu.write_u32(this + COLOR_SET, (a3 != 0) as u32)?;
                        }
                        TRUE
                    } else {
                        let v = self.cpu.read_u32(this + field)?;
                        if self.modern_font_write(a3, &v.to_le_bytes()) {
                            TRUE
                        } else {
                            FALSE
                        }
                    }
                } else if a2 == 0x1002 {
                    if set {
                        (a3 == 0) as u32
                    } else {
                        self.modern_font_write(a3, &0u32.to_le_bytes()) as u32
                    }
                } else if a2 == 0x1065 && !set {
                    let size = self.cpu.read_u32(this + SIZE)?;
                    self.modern_font_write(a3, &size.to_le_bytes()) as u32
                } else if a2 == 0x1064 && !set {
                    let owner = self.cpu.read_u32(this + OWNER)?;
                    if !self.modern_font_write(a3, &owner.to_le_bytes()) {
                        FALSE
                    } else {
                        self.modern_font_base(owner, 0)?;
                        TRUE
                    }
                } else {
                    FALSE
                }
            }
            _ => return Ok(None),
        };
        Ok(Some(result))
    }
    fn modern_font_draw(
        &mut self,
        font: u32,
        target: u32,
        x: i32,
        y: i32,
    ) -> Result<u32, CpuError> {
        let words = (0..6)
            .map(|i| self.stack_arg(i))
            .collect::<Result<Vec<_>, _>>();
        let words = match words {
            Ok(v) => v,
            Err(_) => return Ok(BAD),
        };
        let text = match self.modern_font_text(words[0], words[1]) {
            Ok(t) => t,
            Err(e) => return Ok(e),
        };
        if !self.modern_font_valid(words[4], 8) {
            return Ok(BAD);
        }
        let clip = self.read_rect(words[4])?.unwrap();
        if !self.bitmaps.contains_key(&target) {
            return Ok(UNSUPPORTED);
        }
        let face = match self.modern_face(font) {
            Ok(f) => f,
            Err(e) => return Ok(e),
        };
        let size = self.cpu.read_u32(font + SIZE)? as f32;
        let scaled = face.as_scaled(PxScale::from(size));
        let emphasis = self.cpu.read_u32(font + EMPHASIS)?;
        let spacing = self.cpu.read_u32(font + SPACING)? as i32;
        let width = advance(&face, &text, size, emphasis, spacing);
        let ascent = scaled.ascent().ceil() as i32;
        let height = (scaled.ascent() - scaled.descent()).ceil() as i32;
        if u64::from(width) * height.max(0) as u64 > 16 * 1024 * 1024 {
            return Ok(OOM);
        }
        if clip.width <= 0 || clip.height <= 0 {
            return Ok(SUCCESS);
        }
        let fg = if self.cpu.read_u32(font + COLOR_SET)? != 0 {
            Rgb::from_rgbval(self.cpu.read_u32(font + COLOR)?).to_rgb565()
        } else {
            words[2] as u16
        };
        let bg = words[3] as u16;
        let render = self.cpu.read_u32(font + RENDER)?;
        let underline = self.cpu.read_u32(font + UNDERLINE)? != 0;
        let surface = self.bitmaps.get_mut(&target).unwrap();
        let (x0, y0, x1, y1) = (
            i32::from(clip.x).max(0),
            i32::from(clip.y).max(0),
            (i32::from(clip.x) + i32::from(clip.width)).min(surface.width() as i32),
            (i32::from(clip.y) + i32::from(clip.height)).min(surface.height() as i32),
        );
        if words[5] & 0x8000 == 0 {
            for py in y.max(y0)..y.saturating_add(height).min(y1) {
                for px in x.max(x0)..x.saturating_add(width as i32).min(x1) {
                    surface.set_pixel_native(px, py, bg);
                }
            }
        }
        let mut pen = 0.0;
        let mut previous = None;
        for c in text.chars() {
            let id = face.glyph_id(c);
            if let Some(old) = previous {
                pen += scaled.kern(old, id) + spacing as f32 / 1000.0;
            }
            let glyph = id.with_scale_and_position(
                PxScale::from(size),
                ab_glyph::point(pen, scaled.ascent()),
            );
            if let Some(outline) = face.outline_glyph(glyph) {
                let bounds = outline.px_bounds();
                outline.draw(|gx, gy, v| {
                    let py = y
                        .saturating_add(bounds.min.y as i32)
                        .saturating_add(gy as i32);
                    let skew = if emphasis & 2 != 0 {
                        (ascent - (bounds.min.y as i32 + gy as i32)).max(0) / 4
                    } else {
                        0
                    };
                    let px = x
                        .saturating_add(bounds.min.x as i32)
                        .saturating_add(gx as i32)
                        .saturating_add(skew);
                    let coverage = match render {
                        1 => {
                            if v >= 0.5 {
                                1.0
                            } else {
                                0.0
                            }
                        }
                        3 => (v * 3.0).round() / 3.0,
                        4 => v,
                        _ => (v * 15.0).round() / 15.0,
                    };
                    if coverage == 0.0 {
                        return;
                    }
                    for dx in 0..=if emphasis & 1 != 0 { 1 } else { 0 } {
                        let px = px.saturating_add(dx);
                        if px >= x0 && px < x1 && py >= y0 && py < y1 {
                            let old = surface.get_pixel(px, py);
                            surface.set_pixel_native(px, py, blend(fg, old, coverage));
                        }
                    }
                });
            }
            pen += scaled.h_advance(id) + (emphasis & 1) as f32;
            previous = Some(id);
        }
        if underline {
            let py = y.saturating_add(ascent + 1);
            if py >= y0 && py < y1 {
                for px in x.max(x0)..x.saturating_add(width as i32).min(x1) {
                    surface.set_pixel_native(px, py, fg);
                }
            }
        }
        Ok(SUCCESS)
    }
}
