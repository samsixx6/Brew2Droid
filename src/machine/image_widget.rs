//! IImageWidget/IDecorator e IInterfaceModel do Brew MP, com objetos separados.
//! Os campos privados vivem no guest, junto com referências e listeners, para
//! preservar o mesmo contrato durante restauração sem um segundo estado oculto.
use super::*;

pub(super) const CLASS_ID: u32 = 0x0105_4a8f;
const IID_IMAGE: u32 = 0x0101_3110;
const IID_MODEL: u32 = 0x0101_593a;
const IID_INTERFACE_MODEL: u32 = 0x0101_593c;
const BAD_PARAMETER: u32 = 14;
const CLASS_NOT_SUPPORTED: u32 = 3;
// Layout privado desta implementação, sem pretensão de reproduzir offsets OEM.
const MODEL: u32 = 4;
const WIDTH: u32 = 8;
const HEIGHT: u32 = 12;
const PARENT: u32 = 16;
const HANDLER: u32 = 20;
const CONTEXT: u32 = 24;
const FREE_HANDLER: u32 = 28;
const CHILD: u32 = 32;
const FLAGS: u32 = 36;
const FRAME: u32 = 40;
const DATA: u32 = 4;
const TYPE: u32 = 8;
const LISTENERS: u32 = 12;

impl<C: CpuBackend> Machine<C> {
    pub(super) fn create_image_widget(&mut self, out: u32) -> Result<u32, CpuError> {
        if out == 0 { return Ok(BAD_PARAMETER); }
        self.cpu.write_u32(out, 0)?;
        let model = self.new_object(Interface::InterfaceModel)?;
        if model == 0 { return Ok(2); }
        self.cpu.write_mem(model + 4, &[0; 60])?;
        let widget = self.new_object(Interface::ImageWidget)?;
        if widget == 0 { self.objects.release(model); return Ok(2); }
        self.cpu.write_mem(widget + 4, &[0; 60])?;
        self.cpu.write_u32(widget + MODEL, model)?;
        self.cpu.write_u32(out, widget)?;
        Ok(SUCCESS)
    }

    fn image_widget_base_call(&mut self, object: u32, slot: u32) -> Result<u32, CpuError> {
        if object == 0 { return Ok(0); }
        let vt = self.cpu.read_u32(object)?;
        let entry = self.cpu.read_u32(vt + slot * 4)?;
        match self.call_guest_aninhado(entry, [object, 0, 0, 0], QSORT_BUDGET)? {
            Outcome::Returned { code } => Ok(code),
            other => Err(CpuError(format!("referência de imagem/modelo interrompida: {other:?}"))),
        }
    }

    fn cancel_interface_model_listener(&mut self, listener: u32) -> Result<(), CpuError> {
        let model = self.cpu.read_u32(listener + 20)?;
        if self.objects.kind_of(model) != Some(Interface::InterfaceModel) { return Ok(()); }
        let prev = self.cpu.read_u32(listener + 4)?;
        let next = self.cpu.read_u32(listener)?;
        if prev == 0 {
            if self.cpu.read_u32(model + LISTENERS)? == listener {
                self.cpu.write_u32(model + LISTENERS, next)?;
            }
        } else { self.cpu.write_u32(prev, next)?; }
        if next != 0 { self.cpu.write_u32(next + 4, prev)?; }
        for at in [0, 4, 16, 20] { self.cpu.write_u32(listener + at, 0)?; }
        Ok(())
    }

    fn notify_interface_model(&mut self, model: u32, event: u32) -> Result<(), CpuError> {
        let mut listener = self.cpu.read_u32(model + LISTENERS)?;
        let mut listeners = Vec::new();
        while listener != 0 && listeners.len() < 1024 && !listeners.contains(&listener) {
            listeners.push(listener);
            listener = self.cpu.read_u32(listener)?;
        }
        // O callback pode cancelar outro listener. Conferimos o registro novamente
        // antes de cada entrega; o snapshot evita seguir links já modificados.
        for listener in listeners {
            if self.cpu.read_u32(listener + 20)? != model || self.cpu.read_u32(listener + 16)? == 0 { continue; }
            let function = self.cpu.read_u32(listener + 8)?;
            let context = self.cpu.read_u32(listener + 12)?;
            if function != 0 {
                self.call_guest_aninhado(function, [context, event, 0, 0], QSORT_BUDGET)?;
            }
        }
        Ok(())
    }

    pub(super) fn image_widget_call(&mut self, iface: Interface, slot: u32) -> Result<Option<u32>, CpuError> {
        let this = self.arg(0);
        let (a1, a2, a3) = (self.arg(1), self.arg(2), self.arg(3));
        // A entrada de cancelamento segue a assinatura PFNLISTENERCANCEL, cujo
        // primeiro argumento é o registro do listener e não o objeto do modelo.
        if iface == Interface::InterfaceModel && slot == 7 {
            self.cancel_interface_model_listener(this)?;
            return Ok(Some(SUCCESS));
        }
        if self.objects.kind_of(this) != Some(iface) { return Ok(Some(BAD_PARAMETER)); }
        let result = if iface == Interface::InterfaceModel {
            match slot {
                0 => self.objects.add_ref(this),
                1 => {
                    // Os cancelamentos ainda precisam reconhecer o modelo enquanto vivo.
                    if self.objects.contagem(this) == 1 {
                        while self.cpu.read_u32(this + LISTENERS)? != 0 {
                            let listener = self.cpu.read_u32(this + LISTENERS)?;
                            self.cancel_interface_model_listener(listener)?;
                        }
                        let data = self.cpu.read_u32(this + DATA)?;
                        self.cpu.write_u32(this + DATA, 0)?;
                        self.image_widget_base_call(data, 1)?;
                    }
                    self.objects.release(this)
                }
                2 => {
                    if a2 == 0 { return Ok(Some(BAD_PARAMETER)); }
                    let supported = matches!(a1, 0x0100_0001 | IID_MODEL | IID_INTERFACE_MODEL);
                    self.cpu.write_u32(a2, if supported { this } else { 0 })?;
                    if supported { self.objects.add_ref(this); SUCCESS } else { CLASS_NOT_SUPPORTED }
                }
                3 => {
                    if a1 == 0 || self.cpu.read_u32(a1 + 8).unwrap_or(0) == 0 { return Ok(Some(BAD_PARAMETER)); }
                    let old_cancel = self.cpu.read_u32(a1 + 16)?;
                    if old_cancel != 0 {
                        self.call_guest_aninhado(old_cancel, [a1, 0, 0, 0], QSORT_BUDGET)?;
                    }
                    let head = self.cpu.read_u32(this + LISTENERS)?;
                    self.cpu.write_u32(a1, head)?;
                    self.cpu.write_u32(a1 + 4, 0)?;
                    if head != 0 { self.cpu.write_u32(head + 4, a1)?; }
                    self.cpu.write_u32(a1 + 16, aee::encode(Interface::InterfaceModel, 7))?;
                    self.cpu.write_u32(a1 + 20, this)?;
                    self.cpu.write_u32(this + LISTENERS, a1)?;
                    SUCCESS
                }
                4 => { if a1 != 0 { self.notify_interface_model(this, a1)?; } SUCCESS }
                5 => {
                    self.image_widget_base_call(a1, 0)?;
                    let old = self.cpu.read_u32(this + DATA)?;
                    self.cpu.write_u32(this + DATA, a1)?;
                    self.cpu.write_u32(this + TYPE, a2)?;
                    self.image_widget_base_call(old, 1)?;
                    if self.cpu.read_u32(this + LISTENERS)? != 0 {
                        let event = self.malloc(12)?;
                        if event != 0 {
                            self.cpu.write_u32(event, 0x1000)?; // EVT_MDL_VALUE
                            self.cpu.write_u32(event + 4, this)?;
                            self.cpu.write_u32(event + 8, 0)?;
                            let result = self.notify_interface_model(this, event);
                            self.heap.free(event);
                            result?;
                        }
                    }
                    SUCCESS
                }
                6 => {
                    if a2 == 0 { return Ok(Some(BAD_PARAMETER)); }
                    let data = self.cpu.read_u32(this + DATA)?;
                    let supported = data != 0 && a1 == self.cpu.read_u32(this + TYPE)?;
                    self.cpu.write_u32(a2, if supported { data } else { 0 })?;
                    if supported { self.image_widget_base_call(data, 0)?; SUCCESS } else { CLASS_NOT_SUPPORTED }
                }
                _ => return Ok(None),
            }
        } else {
            match slot {
                0 => self.objects.add_ref(this),
                1 => {
                    if self.objects.contagem(this) == 1 {
                        for field in [MODEL, CHILD] {
                            let object = self.cpu.read_u32(this + field)?;
                            self.cpu.write_u32(this + field, 0)?;
                            self.image_widget_base_call(object, 1)?;
                        }
                        let free = self.cpu.read_u32(this + FREE_HANDLER)?;
                        if free != 0 {
                            let context = self.cpu.read_u32(this + CONTEXT)?;
                            self.call_guest_aninhado(free, [context, 0, 0, 0], QSORT_BUDGET)?;
                        }
                    }
                    self.objects.release(this)
                }
                2 => {
                    if a2 == 0 { return Ok(Some(BAD_PARAMETER)); }
                    let supported = matches!(a1, 0x0100_0001 | 0x0101_5934 | 0x0101_5952 | 0x0101_5956);
                    self.cpu.write_u32(a2, if supported { this } else { 0 })?;
                    if supported { self.objects.add_ref(this); SUCCESS } else { CLASS_NOT_SUPPORTED }
                }
                3 => {
                    match (a1, a2) {
                        (0x800 | 0x801, 0x153 | 0x346) => {
                            let field = if a2 == 0x153 { FLAGS } else { FRAME };
                            if a1 == 0x800 {
                                if a3 == 0 { return Ok(Some(FALSE)); }
                                self.cpu.write_u32(a3, self.cpu.read_u32(this + field)?)?;
                            } else { self.cpu.write_u32(this + field, a3)?; }
                            TRUE
                        }
                        (0x801, 0x215) if a3 != 0 => {
                            let model = self.cpu.read_u32(this + MODEL)?;
                            let image = self.cpu.read_u32(model + DATA)?;
                            if self.cpu.read_u32(model + TYPE)? != IID_IMAGE || !self.images.contains_key(&image) { FALSE }
                            else {
                                let parm = self.cpu.read_u32(a3)?;
                                let p1 = self.cpu.read_u32(a3 + 4)?;
                                let p2 = self.cpu.read_u32(a3 + 8)?;
                                self.image_set_parm(image, parm, p1, p2)?;
                                TRUE
                            }
                        }
                        _ => {
                            let function = self.cpu.read_u32(this + HANDLER)?;
                            if function == 0 { FALSE } else {
                                let context = self.cpu.read_u32(this + CONTEXT)?;
                                match self.call_guest_aninhado(function, [context, a1, a2, a3], QSORT_BUDGET)? {
                                    Outcome::Returned { code } => code,
                                    _ => FALSE,
                                }
                            }
                        }
                    }
                }
                4 => {
                    if a1 == 0 { return Ok(Some(BAD_PARAMETER)); }
                    let next = [self.cpu.read_u32(a1)?, self.cpu.read_u32(a1 + 4)?, self.cpu.read_u32(a1 + 8)?];
                    for (index, field) in [HANDLER, CONTEXT, FREE_HANDLER].into_iter().enumerate() {
                        let old = self.cpu.read_u32(this + field)?;
                        self.cpu.write_u32(this + field, next[index])?;
                        self.cpu.write_u32(a1 + index as u32 * 4, old)?;
                    }
                    SUCCESS
                }
                5 | 6 => {
                    if a1 == 0 { return Ok(Some(BAD_PARAMETER)); }
                    let mut size = (self.cpu.read_u32(this + WIDTH)?, self.cpu.read_u32(this + HEIGHT)?);
                    if slot == 5 {
                        let model = self.cpu.read_u32(this + MODEL)?;
                        let image = self.cpu.read_u32(model + DATA)?;
                        size = self.images.get(&image).map_or((0, 0), |i| (i.frame_width as u32, i.height));
                    }
                    self.cpu.write_u32(a1, size.0)?;
                    self.cpu.write_u32(a1 + 4, size.1)?;
                    SUCCESS
                }
                7 => {
                    if a1 == 0 { return Ok(Some(BAD_PARAMETER)); }
                    let width = self.cpu.read_u32(a1)?;
                    let height = self.cpu.read_u32(a1 + 4)?;
                    self.cpu.write_u32(this + WIDTH, width)?;
                    self.cpu.write_u32(this + HEIGHT, height)?;
                    SUCCESS
                }
                8 | 15 => {
                    if a1 == 0 { return Ok(Some(BAD_PARAMETER)); }
                    let object = self.cpu.read_u32(this + if slot == 8 { PARENT } else { CHILD })?;
                    self.cpu.write_u32(a1, object)?;
                    self.image_widget_base_call(object, 0)?;
                    SUCCESS
                }
                9 => { self.cpu.write_u32(this + PARENT, a1)?; SUCCESS }
                10 => { self.draw_image_widget(this, a1, a2 as i32, a3 as i32)?; SUCCESS }
                11 => FALSE, // A transparência depende do conteúdo; nunca prometemos opacidade.
                12 => {
                    if a2 == 0 { return Ok(Some(BAD_PARAMETER)); }
                    let model = self.cpu.read_u32(this + MODEL)?;
                    let supported = matches!(a1, IID_MODEL | IID_INTERFACE_MODEL);
                    self.cpu.write_u32(a2, if supported { model } else { 0 })?;
                    if supported { self.image_widget_base_call(model, 0)?; SUCCESS } else { CLASS_NOT_SUPPORTED }
                }
                13 | 14 => {
                    let field = if slot == 13 { MODEL } else { CHILD };
                    if slot == 13 && self.objects.kind_of(a1) != Some(Interface::InterfaceModel) { return Ok(Some(CLASS_NOT_SUPPORTED)); }
                    self.image_widget_base_call(a1, 0)?;
                    let old = self.cpu.read_u32(this + field)?;
                    self.cpu.write_u32(this + field, a1)?;
                    self.image_widget_base_call(old, 1)?;
                    SUCCESS
                }
                _ => return Ok(None),
            }
        };
        Ok(Some(result))
    }

    fn draw_image_widget(&mut self, widget: u32, canvas: u32, x: i32, y: i32) -> Result<(), CpuError> {
        let Some(&target) = self.canvases.get(&canvas) else { return Ok(()); };
        let model = self.cpu.read_u32(widget + MODEL)?;
        if self.cpu.read_u32(model + TYPE)? != IID_IMAGE { return Ok(()); }
        let image = self.cpu.read_u32(model + DATA)?;
        let Some(info) = self.images.get(&image) else { return Ok(()); };
        let mut width = self.cpu.read_u32(widget + WIDTH)? as i32;
        let mut height = self.cpu.read_u32(widget + HEIGHT)? as i32;
        let image_width = i32::from(info.frame_width);
        let image_height = info.height as i32;
        if width <= 0 { width = image_width; }
        if height <= 0 { height = image_height; }
        let flags = self.cpu.read_u32(widget + FLAGS)?;
        let dx = match flags & 0x70 { 0x20 => (width - image_width) / 2, 0x40 => width - image_width, _ => 0 };
        let dy = match flags & 0x700 { 0x200 => (height - image_height) / 2, 0x400 => height - image_height, _ => 0 };
        let (old_target, old_clip) = (self.display_target, self.clip);
        let (mut x0, mut y0, mut x1, mut y1) = (x, y, x.saturating_add(width), y.saturating_add(height));
        if let Some(clip) = old_clip {
            x0 = x0.max(i32::from(clip.x)); y0 = y0.max(i32::from(clip.y));
            x1 = x1.min(i32::from(clip.x) + i32::from(clip.width));
            y1 = y1.min(i32::from(clip.y) + i32::from(clip.height));
        }
        if x1 <= x0 || y1 <= y0 { return Ok(()); }
        self.display_target = target;
        self.clip = Some(Rect { x: x0 as i16, y: y0 as i16, width: (x1 - x0) as i16, height: (y1 - y0) as i16 });
        let frame = self.cpu.read_u32(widget + FRAME)?;
        let result = self.draw_image(image, x.saturating_add(dx), y.saturating_add(dy), Some(frame));
        self.display_target = old_target; self.clip = old_clip;
        result
    }
}

