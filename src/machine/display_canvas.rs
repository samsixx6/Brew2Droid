//! CDisplayCanvas: adapta o IDisplay retido ao contrato público de ICanvas.
use super::*;
pub(super) const CLASS_ID: u32 = 0x0105_51c8;
const MARKER: u32 = 0x4344_4356;
const DISPLAY: u32 = 8;
const BITMAP: u32 = 12;
const GENERATION: u32 = 16;
const BAD: u32 = 14;
const FAILED: u32 = 1;

impl<C: CpuBackend> Machine<C> {
    fn is_display_canvas(&self, canvas: u32) -> bool {
        self.objects.kind_of(canvas) == Some(Interface::Canvas)
            && self.cpu.read_u32(canvas + 4).ok() == Some(MARKER)
    }
    pub(super) fn create_display_canvas(&mut self, out: u32) -> Result<u32, CpuError> {
        if !self.modern_font_write(out, &0u32.to_le_bytes()) {
            return Ok(BAD);
        }
        let canvas = self.new_object(Interface::Canvas)?;
        if canvas == 0 {
            return Ok(2);
        }
        self.cpu.write_mem(canvas + 4, &[0; 60])?;
        self.cpu.write_u32(canvas + 4, MARKER)?;
        self.cpu.write_u32(out, canvas)?;
        Ok(SUCCESS)
    }
    fn display_canvas_release(&mut self, canvas: u32) -> Result<u32, CpuError> {
        if self.objects.contagem(canvas) != 1 {
            return Ok(self.objects.release(canvas));
        }
        let display = self.cpu.read_u32(canvas + DISPLAY)?;
        let bitmap = self.cpu.read_u32(canvas + BITMAP)?;
        // Limpar antes dos Releases evita deixar cache ou posse numa identidade reciclada.
        self.cpu.write_mem(canvas + 4, &[0; 60])?;
        self.canvases.remove(&canvas);
        self.objects.release(canvas);
        let a = self.modern_ui_invoke(bitmap, 1, [0; 3], &[]);
        let b = self.modern_ui_invoke(display, 1, [0; 3], &[]);
        a?;
        b?;
        Ok(0)
    }
    fn display_canvas_destination(&mut self, display: u32) -> Result<u32, CpuError> {
        let bitmap = self.modern_ui_invoke(display, 15, [0; 3], &[])?;
        if bitmap == 0 {
            return Ok(0);
        }
        if self.objects.kind_of(bitmap) != Some(Interface::Bitmap)
            || !self.bitmaps.contains_key(&bitmap)
        {
            // A superfície estrangeira conserva sua própria ABI; não é um framebuffer nativo.
            self.modern_ui_invoke(bitmap, 1, [0; 3], &[])?;
            return Ok(0);
        }
        Ok(bitmap)
    }
    fn display_canvas_refresh(&mut self, canvas: u32) -> Result<u32, CpuError> {
        let display = self.cpu.read_u32(canvas + DISPLAY)?;
        if display == 0 {
            return Ok(0);
        }
        let generation = self.cpu.read_u32(canvas + GENERATION)?;
        self.objects.add_ref(display);
        let result = self.display_canvas_destination(display);
        let released = self.modern_ui_invoke(display, 1, [0; 3], &[]);
        let bitmap = result?;
        released?;
        if bitmap == 0 {
            return Ok(0);
        }
        if self.cpu.read_u32(canvas + GENERATION)? != generation
            || self.cpu.read_u32(canvas + DISPLAY)? != display
        {
            self.modern_ui_invoke(bitmap, 1, [0; 3], &[])?;
            return Ok(0);
        }
        let old = self.cpu.read_u32(canvas + BITMAP)?;
        self.cpu.write_u32(canvas + BITMAP, bitmap)?;
        self.canvases.insert(canvas, bitmap);
        self.modern_ui_invoke(old, 1, [0; 3], &[])?;
        Ok(bitmap)
    }
    fn display_canvas_bind(&mut self, canvas: u32, display: u32) -> Result<u32, CpuError> {
        if display != 0 && self.objects.kind_of(display) != Some(Interface::Display) {
            return Ok(BAD);
        }
        let bitmap = if display == 0 {
            0
        } else {
            self.objects.add_ref(display);
            let bitmap = match self.display_canvas_destination(display) {
                Ok(bitmap) => bitmap,
                Err(error) => {
                    self.modern_ui_invoke(display, 1, [0; 3], &[])?;
                    return Err(error);
                }
            };
            if bitmap == 0 {
                self.modern_ui_invoke(display, 1, [0; 3], &[])?;
                return Ok(FAILED);
            }
            bitmap
        };
        let old_display = self.cpu.read_u32(canvas + DISPLAY)?;
        let old_bitmap = self.cpu.read_u32(canvas + BITMAP)?;
        let generation = self.cpu.read_u32(canvas + GENERATION)?.wrapping_add(1);
        self.cpu.write_u32(canvas + GENERATION, generation)?;
        self.cpu.write_u32(canvas + DISPLAY, display)?;
        self.cpu.write_u32(canvas + BITMAP, bitmap)?;
        if bitmap == 0 {
            self.canvases.remove(&canvas);
        } else {
            self.canvases.insert(canvas, bitmap);
        }
        let a = self.modern_ui_invoke(old_bitmap, 1, [0; 3], &[]);
        let b = self.modern_ui_invoke(old_display, 1, [0; 3], &[]);
        a?;
        b?;
        Ok(SUCCESS)
    }
    /// Some entrega uma referência adquirida; None conserva o canvas de bitmap antigo.
    pub(super) fn display_canvas_snapshot(
        &mut self,
        canvas: u32,
    ) -> Result<Option<(u32, Rect)>, CpuError> {
        if !self.is_display_canvas(canvas) {
            return Ok(None);
        }
        self.objects.add_ref(canvas);
        let result = (|| {
            let empty = Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            };
            let bitmap = self.display_canvas_refresh(canvas)?;
            if bitmap == 0 {
                return Ok(Some((0, empty)));
            }
            self.objects.add_ref(bitmap);
            let display = self.cpu.read_u32(canvas + DISPLAY)?;
            let clip = self.modern_ui_scratch(8, |m, at| {
                // IDisplay::GetClipRect é void; R0 residual não indica erro.
                m.modern_ui_invoke(display, 19, [at, 0, 0], &[])?;
                Ok(m.read_rect(at)?.unwrap_or(empty))
            });
            match clip {
                Ok(clip) => Ok(Some((bitmap, clip))),
                Err(error) => {
                    self.modern_ui_invoke(bitmap, 1, [0; 3], &[])?;
                    Err(error)
                }
            }
        })();
        let released = self.display_canvas_release(canvas);
        let result = result?;
        released?;
        Ok(result)
    }
    pub(super) fn display_canvas_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let canvas = self.cpu.read_reg(Reg::R0);
        if !self.is_display_canvas(canvas) {
            // WindowMgr compõe a tela com um canvas temporário de bitmap. O RootContainer
            // usa o mesmo prefixo público, mesmo sem haver IDisplay retido nesse canvas.
            if self.objects.kind_of(canvas) == Some(Interface::Canvas)
                && let Some(&bitmap) = self.canvases.get(&canvas)
                && matches!(slot, 3 | 5 | 6)
            {
                let a1 = self.cpu.read_reg(Reg::R1);
                if slot == 3 {
                    if !self.modern_font_write(a1, &0u32.to_le_bytes()) {
                        return Ok(Some(BAD));
                    }
                    if self.objects.kind_of(bitmap) != Some(Interface::Bitmap)
                        || !self.bitmaps.contains_key(&bitmap)
                    {
                        return Ok(Some(FAILED));
                    }
                    self.objects.add_ref(bitmap);
                    self.cpu.write_u32(a1, bitmap)?;
                } else if slot == 5 {
                    if !self.modern_font_valid(a1, 8) {
                        return Ok(Some(BAD));
                    }
                    let Some(surface) = self.bitmaps.get(&bitmap) else {
                        return Ok(Some(FAILED));
                    };
                    let clip = self.clip.unwrap_or(Rect {
                        x: 0,
                        y: 0,
                        width: surface.width().min(i16::MAX as u32) as i16,
                        height: surface.height().min(i16::MAX as u32) as i16,
                    });
                    self.cpu.write_mem(
                        a1,
                        &[clip.x, clip.y, clip.width, clip.height]
                            .into_iter()
                            .flat_map(i16::to_le_bytes)
                            .collect::<Vec<_>>(),
                    )?;
                } else {
                    if a1 != 0 && !self.modern_font_valid(a1, 8) {
                        return Ok(Some(BAD));
                    }
                    self.clip = self.read_rect(a1)?;
                }
                return Ok(Some(SUCCESS));
            }
            return self.canvas_call(slot);
        }
        let a1 = self.cpu.read_reg(Reg::R1);
        let a2 = self.cpu.read_reg(Reg::R2);
        if slot == 0 {
            return Ok(Some(self.objects.add_ref(canvas)));
        }
        if slot == 1 {
            return self.display_canvas_release(canvas).map(Some);
        }
        if slot == 2 {
            if !self.modern_font_write(a2, &0u32.to_le_bytes()) {
                return Ok(Some(BAD));
            }
            if !matches!(a1, 0x0100_0001 | 0x0101_e3f2 | 0x0101_e443) {
                return Ok(Some(3));
            }
            self.objects.add_ref(canvas);
            self.cpu.write_u32(a2, canvas)?;
            return Ok(Some(SUCCESS));
        }
        if !(3..=8).contains(&slot) {
            return Ok(None);
        }
        self.objects.add_ref(canvas);
        let result = (|| {
            if slot == 8 {
                return self.display_canvas_bind(canvas, a1);
            }
            let display = self.cpu.read_u32(canvas + DISPLAY)?;
            if display == 0 {
                return Ok(FAILED);
            }
            match slot {
                3 => {
                    if !self.modern_font_write(a1, &0u32.to_le_bytes()) {
                        return Ok(BAD);
                    }
                    let bitmap = self.display_canvas_refresh(canvas)?;
                    if bitmap == 0 {
                        return Ok(FAILED);
                    }
                    self.objects.add_ref(bitmap);
                    self.cpu.write_u32(a1, bitmap)?;
                    Ok(SUCCESS)
                }
                4 => {
                    if self.objects.kind_of(a1) != Some(Interface::Bitmap)
                        || !self.bitmaps.contains_key(&a1)
                    {
                        return Ok(BAD);
                    }
                    let code = self.modern_ui_invoke(display, 14, [a1, 0, 0], &[])?;
                    if code != SUCCESS {
                        return Ok(code);
                    }
                    Ok(if self.display_canvas_refresh(canvas)? == 0 {
                        FAILED
                    } else {
                        SUCCESS
                    })
                }
                5 | 6 => {
                    if (slot == 5 || a1 != 0) && !self.modern_font_valid(a1, 8) {
                        return Ok(BAD);
                    }
                    self.modern_ui_invoke(
                        display,
                        if slot == 5 { 19 } else { 18 },
                        [a1, 0, 0],
                        &[],
                    )?;
                    Ok(SUCCESS)
                }
                7 => {
                    if !self.modern_font_write(a1, &0u32.to_le_bytes()) {
                        return Ok(BAD);
                    }
                    self.objects.add_ref(display);
                    self.cpu.write_u32(a1, display)?;
                    Ok(SUCCESS)
                }
                _ => Ok(20),
            }
        })();
        let released = self.display_canvas_release(canvas);
        let code = result?;
        released?;
        Ok(Some(code))
    }
}
