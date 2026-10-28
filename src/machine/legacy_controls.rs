//! Controles dos SDKs móveis: IBase de dois slots, seguido de IControl e da API específica.
//! Não compartilham os stubs do controle de texto do Zenonia.

use super::*;
use serde::{Deserialize, Serialize};

const SDK_BAD_PARAM: u32 = 14;
const SDK_NO_MEMORY: u32 = 2;
const MAX_CONTROLS: usize = 256;
const MAX_ITEMS: usize = 1024;
const MAX_TEXT: usize = 4096;
const MAX_MENU_TEXT: usize = 128; // MAX_MENU_TEXT de AEEMenu.h.
const MP_WRAPSCROLL: u32 = 0x0001_0000;
const MP_ALLOW_DUPE_IDS: u32 = 0x0004_0000;
const MP_NO_REDRAW: u32 = 0x0008_0000;
const ST_CENTERTEXT: u32 = 0x0001_0000;
const ST_CENTERTITLE: u32 = 0x0002_0000;
const EVT_KEY_LEGACY: u32 = 0x100;
const EVT_COMMAND_LEGACY: u32 = 0x200; // inc/AEEEvent.h, não um código de tecla.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum LegacyControlKind {
    Menu,
    Softkey,
    Text100,
    Static,
}

impl LegacyControlKind {
    fn interface(self) -> Interface {
        match self {
            Self::Menu => Interface::MenuLegacy,
            Self::Softkey => Interface::SoftkeyLegacy,
            Self::Text100 => Interface::TextLegacy100,
            Self::Static => Interface::StaticLegacy,
        }
    }

    fn is_menu(self) -> bool {
        matches!(self, Self::Menu | Self::Softkey)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct LegacyItemResource {
    file: Vec<u8>,
    id: u16,
    pointer: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct LegacyMenuItem {
    id: u16,
    text: Vec<u16>,
    data: u32,
    font: u16,
    #[serde(default)]
    image: u32,
    #[serde(default)]
    text_pointer: u32,
    #[serde(default)]
    text_resource: LegacyItemResource,
    #[serde(default)]
    image_resource: LegacyItemResource,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct LegacyItemStyle {
    frame: u8,
    x_offset: u16,
    y_offset: u16,
    image_rop: u8,
}

impl Default for LegacyItemStyle {
    fn default() -> Self {
        // Mantém o padding do rasterizador anterior em saves sem estilos explícitos.
        Self {
            frame: 0,
            x_offset: 2,
            y_offset: 2,
            image_rop: 2,
        }
    }
}

impl LegacyItemStyle {
    fn from_arm(bytes: [u8; 8]) -> Self {
        // AEEShell.h: enums int8 no ARM; os bytes 1 e 7 são padding, não campos.
        Self {
            frame: bytes[0],
            x_offset: u16::from_le_bytes([bytes[2], bytes[3]]),
            y_offset: u16::from_le_bytes([bytes[4], bytes[5]]),
            image_rop: bytes[6],
        }
    }

    fn arm_bytes(self) -> [u8; 8] {
        let [x0, x1] = self.x_offset.to_le_bytes();
        let [y0, y1] = self.y_offset.to_le_bytes();
        [self.frame, 0, x0, x1, y0, y1, self.image_rop, 0]
    }

    fn frame_width(self) -> Option<i32> {
        match self.frame {
            0 => Some(0),
            1 | 5 => Some(1), // EMPTY / BOX
            2 => Some(2),     // 3D_EMPTY, como IDisplay::DrawFrame
            _ => None,
        }
    }

    fn row_height(self) -> i32 {
        FONT_SIZE as i32 + 2 * (self.frame_width().unwrap_or(0) + i32::from(self.y_offset))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct LegacyControl {
    kind: LegacyControlKind,
    rect: [i16; 4],
    active: bool,
    properties: u32,
    title: Vec<u16>,
    text: Vec<u16>,
    items: Vec<LegacyMenuItem>,
    selected: Option<usize>,
    command_enabled: bool,
    command_id: u16,
    text_pointer: u32,
    text_capacity: usize,
    max_size: usize,
    cursor: usize,
    soft_menu: u32,
    font_title: u16,
    font_text: u16,
    colors: [u32; 9],
    #[serde(default)]
    normal_style: LegacyItemStyle,
    #[serde(default)]
    selected_style: LegacyItemStyle,
}

impl LegacyControl {
    fn new(kind: LegacyControlKind, width: u32, height: u32) -> Self {
        let rect = if kind == LegacyControlKind::Softkey {
            [0, height.saturating_sub(24) as i16, width as i16, 24]
        } else {
            [0, 0, width as i16, height as i16]
        };
        Self {
            kind,
            rect,
            active: false,
            properties: if kind == LegacyControlKind::Softkey {
                MP_WRAPSCROLL
            } else {
                0
            },
            title: Vec::new(),
            text: Vec::new(),
            items: Vec::new(),
            selected: None,
            command_enabled: true,
            command_id: 0,
            text_pointer: 0,
            text_capacity: 0,
            max_size: MAX_TEXT,
            cursor: 0,
            soft_menu: 0,
            font_title: 0x8001,
            font_text: 0x8000,
            normal_style: LegacyItemStyle::default(),
            selected_style: LegacyItemStyle::default(),
            // Cores padrão do aparelho; SetColors substitui apenas os campos de sua máscara.
            colors: [
                0xffff_ff00,
                0,
                0x8000_0000,
                0xffff_ff00,
                0,
                0,
                0,
                0xffff_ff00,
                0,
            ],
        }
    }

    fn rect(&self) -> Rect {
        Rect {
            x: self.rect[0],
            y: self.rect[1],
            width: self.rect[2],
            height: self.rect[3],
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SavedControls {
    version: u32,
    controls: Vec<(u32, LegacyControl)>,
}

impl<C: CpuBackend> Machine<C> {
    pub(super) fn cria_controle_legado(
        &mut self,
        kind: LegacyControlKind,
        out: u32,
    ) -> Result<u32, CpuError> {
        if out == 0 {
            return Ok(SDK_BAD_PARAM);
        }
        self.cpu.write_u32(out, 0)?;
        if self.legacy_controls.len() >= MAX_CONTROLS {
            return Ok(SDK_NO_MEMORY);
        }
        let object = self.new_object(kind.interface())?;
        if object == 0 {
            return Ok(SDK_NO_MEMORY);
        }
        let state = LegacyControl::new(kind, self.screen().width(), self.screen().height());
        self.legacy_controls.insert(object, state);
        self.cpu.write_u32(out, object)?;
        Ok(SUCCESS)
    }

    /// A leitura tem limite próprio: MAX_STRING do runtime permite um MiB, muito mais que um controle.
    fn legacy_read_text(&self, pointer: u32, limit: usize) -> Result<Option<Vec<u16>>, CpuError> {
        self.legacy_read_units(pointer, limit, true)
    }

    fn legacy_read_units(
        &self,
        pointer: u32,
        limit: usize,
        require_nul: bool,
    ) -> Result<Option<Vec<u16>>, CpuError> {
        if pointer == 0 {
            return Ok(Some(Vec::new()));
        }
        let mut units = Vec::new();
        for index in 0..=limit {
            if index == limit && !require_nul {
                return Ok(Some(units));
            }
            let Some(address) = pointer.checked_add(index as u32 * 2) else {
                return Ok(None);
            };
            let mut bytes = [0u8; 2];
            if self.cpu.read_mem(address, &mut bytes).is_err() {
                return Ok(None);
            }
            let unit = u16::from_le_bytes(bytes);
            if unit == 0 {
                return Ok(Some(units));
            }
            if units.len() == limit {
                return Ok(None);
            }
            units.push(unit);
        }
        Ok(None)
    }

    fn legacy_label(
        &mut self,
        file: u32,
        id: u16,
        text: u32,
    ) -> Result<Option<Vec<u16>>, CpuError> {
        if text != 0 {
            return self.legacy_read_text(text, MAX_MENU_TEXT);
        }
        if file == 0 && id == 0 {
            return Ok(Some(Vec::new()));
        }
        Ok(self
            .open_res_file(file)
            .and_then(|res| res.string(id))
            .filter(|units| units.len() <= MAX_MENU_TEXT))
    }

    fn legacy_item_resource(&self, file: u32, id: u16, used: bool) -> LegacyItemResource {
        if !used || id == 0 {
            return LegacyItemResource::default();
        }
        LegacyItemResource {
            file: if file == 0 {
                Vec::new()
            } else {
                self.cpu
                    .read_cstring(file, AEE_MAX_FILE_NAME as usize)
                    .into_bytes()
            },
            id,
            pointer: 0,
        }
    }

    fn legacy_free_item(&mut self, item: LegacyMenuItem) {
        if item.image != 0 {
            self.release_image(item.image);
        }
        for pointer in [
            item.text_pointer,
            item.text_resource.pointer,
            item.image_resource.pointer,
        ] {
            if pointer != 0 {
                self.heap.free(pointer);
            }
        }
    }

    fn legacy_export_item(&mut self, object: u32, index: usize) -> Result<bool, CpuError> {
        let item = self.legacy_controls[&object].items[index].clone();
        if item.text_pointer == 0 {
            let text = item
                .text
                .iter()
                .copied()
                .chain([0])
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>();
            let pointer = self.malloc(text.len() as u32)?;
            if pointer == 0 {
                return Ok(false);
            }
            self.cpu.write_mem(pointer, &text)?;
            self.legacy_controls.get_mut(&object).unwrap().items[index].text_pointer = pointer;
        }
        for (image, resource) in [(false, item.text_resource), (true, item.image_resource)] {
            if resource.pointer == 0 && !resource.file.is_empty() {
                let pointer = self.malloc(resource.file.len() as u32 + 1)?;
                if pointer == 0 {
                    return Ok(false);
                }
                let mut bytes = resource.file;
                bytes.push(0);
                self.cpu.write_mem(pointer, &bytes)?;
                let item = &mut self.legacy_controls.get_mut(&object).unwrap().items[index];
                if image {
                    item.image_resource.pointer = pointer;
                } else {
                    item.text_resource.pointer = pointer;
                }
            }
        }
        Ok(true)
    }

    fn legacy_release(&mut self, object: u32) -> u32 {
        let remaining = self.objects.release(object);
        if remaining == 0 {
            if let Some(state) = self.legacy_controls.remove(&object) {
                if state.text_pointer != 0 {
                    self.heap.free(state.text_pointer);
                }
                for item in state.items {
                    self.legacy_free_item(item);
                }
                if state.soft_menu != 0 {
                    self.legacy_release(state.soft_menu);
                }
            }
        }
        remaining
    }

    /// Só troca o ponteiro após a nova alocação ter dado certo; GetTextPtr continua estável
    /// quando o conteúdo cabe no buffer já existente.
    fn legacy_set_text(
        &mut self,
        object: u32,
        text: Vec<u16>,
        capacity: usize,
    ) -> Result<bool, CpuError> {
        let Some(state) = self.legacy_controls.get(&object) else {
            return Ok(false);
        };
        if text.len() > state.max_size || capacity > MAX_TEXT + 1 {
            return Ok(false);
        }
        let needed = capacity.max(text.len() + 1).max(1);
        let (old_pointer, old_capacity) = (state.text_pointer, state.text_capacity);
        let pointer = if old_capacity >= needed {
            old_pointer
        } else {
            self.malloc((needed * 2) as u32)?
        };
        if pointer == 0 {
            return Ok(false);
        }
        let bytes = text
            .iter()
            .copied()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        self.cpu.write_mem(pointer, &bytes)?;
        if pointer != old_pointer && old_pointer != 0 {
            self.heap.free(old_pointer);
        }
        let state = self
            .legacy_controls
            .get_mut(&object)
            .expect("controle vivo");
        state.text_pointer = pointer;
        state.text_capacity = old_capacity.max(needed);
        state.cursor = text.len();
        state.text = text;
        Ok(true)
    }

    fn legacy_current_text(&self, state: &LegacyControl) -> Result<Vec<u16>, CpuError> {
        if state.text_pointer == 0 {
            return Ok(state.text.clone());
        }
        // O ponteiro é público. Uma escrita do guest deve aparecer em GetText e no redraw.
        Ok(self
            .legacy_read_text(state.text_pointer, state.max_size)?
            .unwrap_or_else(|| state.text.clone()))
    }

    pub(super) fn legacy_control_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let this = self.cpu.read_reg(Reg::R0);
        let Some(state) = self.legacy_controls.get(&this).cloned() else {
            return Ok(Some(SDK_BAD_PARAM));
        };
        if self.objects.kind_of(this) != Some(state.kind.interface()) {
            return Ok(Some(SDK_BAD_PARAM));
        }
        let Some(name) = state.kind.interface().method(slot) else {
            return Ok(None);
        };
        let (a1, a2, a3) = (
            self.cpu.read_reg(Reg::R1),
            self.cpu.read_reg(Reg::R2),
            self.cpu.read_reg(Reg::R3),
        );
        let result = match name {
            "AddRef" => self.objects.add_ref(this),
            "Release" => self.legacy_release(this),
            "HandleEvent" => return self.legacy_handle_event(this, a1, a2 as u16, a3).map(Some),
            "Redraw" => {
                self.legacy_redraw(this)?;
                TRUE
            }
            "SetActive" => {
                self.legacy_controls.get_mut(&this).unwrap().active = a1 != 0;
                if state.active != (a1 != 0) && state.properties & MP_NO_REDRAW == 0 {
                    self.legacy_redraw(this)?;
                }
                SUCCESS
            }
            "IsActive" => u32::from(state.active),
            "SetRect" => {
                let Some(rect) = self.read_rect(a1)? else {
                    return Ok(Some(SDK_BAD_PARAM));
                };
                self.legacy_controls.get_mut(&this).unwrap().rect =
                    [rect.x, rect.y, rect.width, rect.height];
                SUCCESS
            }
            "GetRect" => {
                if a1 == 0 {
                    return Ok(Some(SDK_BAD_PARAM));
                }
                self.cpu.write_mem(
                    a1,
                    &state
                        .rect
                        .into_iter()
                        .flat_map(i16::to_le_bytes)
                        .collect::<Vec<_>>(),
                )?;
                SUCCESS
            }
            "SetProperties" => {
                self.legacy_controls.get_mut(&this).unwrap().properties = a1
                    | if state.kind == LegacyControlKind::Softkey {
                        MP_WRAPSCROLL
                    } else {
                        0
                    };
                SUCCESS
            }
            "GetProperties" => state.properties,
            "Reset" => {
                for item in state.items {
                    self.legacy_free_item(item);
                }
                let target = self.legacy_controls.get_mut(&this).unwrap();
                target.active = false;
                target.title.clear();
                target.text.clear();
                target.items.clear();
                target.selected = None;
                target.cursor = 0;
                if state.text_pointer != 0 {
                    self.cpu.write_mem(state.text_pointer, &[0, 0])?;
                }
                SUCCESS
            }
            _ if state.kind.is_menu() => return self.legacy_menu_call(this, name, [a1, a2, a3]),
            _ if state.kind == LegacyControlKind::Text100 => {
                return self.legacy_text_call(this, name, [a1, a2, a3]);
            }
            _ if state.kind == LegacyControlKind::Static => {
                return self.legacy_static_call(this, name, [a1, a2, a3]);
            }
            _ => return Ok(None),
        };
        Ok(Some(result))
    }

    fn legacy_menu_call(
        &mut self,
        this: u32,
        name: &str,
        [a1, a2, a3]: [u32; 3],
    ) -> Result<Option<u32>, CpuError> {
        let result = match name {
            "SetTitle" => {
                let Some(title) = self.legacy_label(a1, a2 as u16, a3)? else {
                    return Ok(Some(FALSE));
                };
                self.legacy_controls.get_mut(&this).unwrap().title = title;
                TRUE
            }
            "AddItem" | "AddItemEx" => {
                let (file, resource, id, text, data, font, supplied_image, image_file, image_id) =
                    if name == "AddItem" {
                        (
                            a1,
                            a2 as u16,
                            a3 as u16,
                            self.stack_arg(0)?,
                            self.stack_arg(1)?,
                            0x8000,
                            0,
                            0,
                            0,
                        )
                    } else {
                        if a1 == 0 {
                            return Ok(Some(FALSE));
                        }
                        // CtlAddItem: quatro ponteiros, quatro uint16 e dwData (28 bytes no ARM).
                        let mut fields = [0u8; 28];
                        self.cpu.read_mem(a1, &mut fields)?;
                        let word = |at| u32::from_le_bytes(fields[at..at + 4].try_into().unwrap());
                        let half = |at| u16::from_le_bytes(fields[at..at + 2].try_into().unwrap());
                        (
                            word(12),
                            half(16),
                            half(22),
                            word(0),
                            word(24),
                            half(18),
                            word(4),
                            word(8),
                            half(20),
                        )
                    };
                if self
                    .legacy_controls
                    .values()
                    .map(|c| c.items.len())
                    .sum::<usize>()
                    >= MAX_ITEMS
                {
                    return Ok(Some(FALSE));
                }
                let text_resource = self.legacy_item_resource(file, resource, text == 0);
                let image_resource =
                    self.legacy_item_resource(image_file, image_id, supplied_image == 0);
                let Some(text) = self.legacy_label(file, resource, text)? else {
                    return Ok(Some(FALSE));
                };
                let state = &self.legacy_controls[&this];
                if state.properties & MP_ALLOW_DUPE_IDS == 0
                    && state.items.iter().any(|item| item.id == id)
                {
                    return Ok(Some(FALSE));
                }
                // Só adquirir a referência após validar texto, limite e ID, evitando vazamentos
                // quando AddItemEx falha. O chamador pode Release imediatamente depois do sucesso.
                let image = if supplied_image != 0 {
                    if self.objects.kind_of(supplied_image) != Some(Interface::Image)
                        || !self.images.contains_key(&supplied_image)
                    {
                        return Ok(Some(FALSE));
                    }
                    self.objects.add_ref(supplied_image);
                    supplied_image
                } else if image_id != 0 {
                    let raw = self
                        .open_res_file(image_file)
                        .and_then(|res| res.get(crate::loader::resfile::RESTYPE_IMAGE, image_id))
                        .map(<[u8]>::to_vec);
                    let Some(raw) = raw else {
                        return Ok(Some(FALSE));
                    };
                    let raw = crate::loader::resfile::blob_data(&raw).unwrap_or(&raw);
                    let Some(decoded) = self.decode_resource_image(raw) else {
                        return Ok(Some(FALSE));
                    };
                    let image = self.new_object(Interface::Image)?;
                    if image == 0 {
                        return Ok(Some(FALSE));
                    }
                    self.images.insert(image, std::rc::Rc::new(decoded));
                    self.recursos_lidos.insert(image_id);
                    image
                } else {
                    0
                };
                let state = self.legacy_controls.get_mut(&this).unwrap();
                state.items.push(LegacyMenuItem {
                    id,
                    text,
                    data,
                    font,
                    image,
                    text_pointer: 0,
                    text_resource,
                    image_resource,
                });
                if state.selected.is_none() {
                    state.selected = Some(0);
                }
                TRUE
            }
            "GetItemData" => {
                let Some(data) = self.legacy_controls[&this]
                    .items
                    .iter()
                    .find(|item| item.id == a1 as u16)
                    .map(|item| item.data)
                else {
                    return Ok(Some(FALSE));
                };
                if a2 == 0 {
                    return Ok(Some(FALSE));
                }
                self.cpu.write_u32(a2, data)?;
                TRUE
            }
            "DeleteItem" => {
                let state = self.legacy_controls.get_mut(&this).unwrap();
                let Some(index) = state.items.iter().position(|item| item.id == a1 as u16) else {
                    return Ok(Some(FALSE));
                };
                let removed = state.items.remove(index);
                state.selected = match (state.selected, state.items.is_empty()) {
                    (_, true) => None,
                    (Some(selected), false) => Some(
                        selected
                            .saturating_sub(usize::from(index < selected))
                            .min(state.items.len() - 1),
                    ),
                    (None, false) => Some(0),
                };
                self.legacy_free_item(removed);
                TRUE
            }
            "DeleteAll" => {
                let state = self.legacy_controls.get_mut(&this).unwrap();
                let items = std::mem::take(&mut state.items);
                state.selected = None;
                for item in items {
                    self.legacy_free_item(item);
                }
                TRUE
            }
            "SetSel" => {
                let state = self.legacy_controls.get_mut(&this).unwrap();
                if let Some(index) = state.items.iter().position(|item| item.id == a1 as u16) {
                    state.selected = Some(index);
                    if state.properties & MP_NO_REDRAW == 0 {
                        self.legacy_redraw(this)?;
                    }
                }
                SUCCESS
            }
            "GetSel" => self.legacy_controls[&this]
                .selected
                .and_then(|index| self.legacy_controls[&this].items.get(index))
                .map_or(0, |item| u32::from(item.id)),
            "EnableCommand" => {
                self.legacy_controls.get_mut(&this).unwrap().command_enabled = a1 != 0;
                SUCCESS
            }
            "SetItemText" => {
                let source = self.stack_arg(0)?;
                let resource = self.legacy_item_resource(a2, a3 as u16, source == 0);
                let Some(text) = self.legacy_label(a2, a3 as u16, source)? else {
                    return Ok(Some(FALSE));
                };
                if let Some(item) = self
                    .legacy_controls
                    .get_mut(&this)
                    .unwrap()
                    .items
                    .iter_mut()
                    .find(|item| item.id == a1 as u16)
                {
                    for pointer in [item.text_pointer, item.text_resource.pointer] {
                        if pointer != 0 {
                            self.heap.free(pointer);
                        }
                    }
                    item.text_pointer = 0;
                    item.text_resource = resource;
                    item.text = text;
                }
                SUCCESS
            }
            "GetItem" => {
                if a2 == 0 {
                    return Ok(Some(FALSE));
                }
                let Some(index) = self.legacy_controls[&this]
                    .items
                    .iter()
                    .position(|item| item.id == a1 as u16)
                else {
                    return Ok(Some(FALSE));
                };
                if !self.legacy_export_item(this, index)? {
                    return Ok(Some(FALSE));
                }
                let item = &self.legacy_controls[&this].items[index];
                // AEEMenu.h: todos os ponteiros são emprestados, inclusive pImage; GetItem
                // não pode incrementar referências nem devolver a origem temporária do chamador.
                let mut bytes = [0; 28];
                for (at, word) in [
                    (0, item.text_pointer),
                    (4, item.image),
                    (8, item.image_resource.pointer),
                    (12, item.text_resource.pointer),
                    (24, item.data),
                ] {
                    bytes[at..at + 4].copy_from_slice(&word.to_le_bytes());
                }
                for (at, half) in [
                    (16, item.text_resource.id),
                    (18, item.font),
                    (20, item.image_resource.id),
                    (22, item.id),
                ] {
                    bytes[at..at + 2].copy_from_slice(&half.to_le_bytes());
                }
                self.cpu.write_mem(a2, &bytes)?;
                TRUE
            }
            "GetItemCount" => self.legacy_controls[&this].items.len() as u32,
            "SetItem" if a2 as u16 == 0x0008 => {
                // Chess usa MSIF_FONT: só wFont (+18) é válido; os demais campos podem ser lixo.
                let Some(index) = self.legacy_controls[&this]
                    .items
                    .iter()
                    .position(|item| item.id == a1 as u16)
                else {
                    return Ok(Some(FALSE));
                };
                let Some(address) = a3.checked_add(18).filter(|_| a3 != 0) else {
                    return Ok(Some(FALSE));
                };
                let mut font = [0; 2];
                self.cpu.read_mem(address, &mut font)?;
                self.legacy_controls.get_mut(&this).unwrap().items[index].font =
                    u16::from_le_bytes(font);
                // AEEMenu.h exige Redraw separado depois de SetItem.
                TRUE
            }
            "GetItemID" => self.legacy_controls[&this]
                .items
                .get(a1 as usize)
                .map_or(0, |item| u32::from(item.id)),
            "SetStyle" => {
                let mut styles = [None, None];
                for (style, pointer) in styles.iter_mut().zip([a1, a2]) {
                    if pointer != 0 {
                        let mut bytes = [0; 8];
                        self.cpu.read_mem(pointer, &mut bytes)?;
                        let value = LegacyItemStyle::from_arm(bytes);
                        // Não aceitar efeitos 3D/abas como um sucesso sem desenho.
                        if value.frame_width().is_none() || ![2, 7].contains(&value.image_rop) {
                            return Ok(None);
                        }
                        *style = Some(value);
                    }
                }
                let state = self.legacy_controls.get_mut(&this).unwrap();
                if let Some(style) = styles[0] {
                    state.normal_style = style;
                }
                if let Some(style) = styles[1] {
                    state.selected_style = style;
                }
                SUCCESS
            }
            "GetStyle" => {
                let state = &self.legacy_controls[&this];
                let styles = [state.normal_style, state.selected_style];
                for (style, pointer) in styles.into_iter().zip([a1, a2]) {
                    if pointer != 0 {
                        self.cpu.write_mem(pointer, &style.arm_bytes())?;
                    }
                }
                SUCCESS
            }
            "SetColors" => {
                if a1 == 0 {
                    return Ok(Some(SDK_BAD_PARAM));
                }
                let mut mask = [0; 2];
                self.cpu.read_mem(a1, &mut mask)?;
                let mask = u16::from_le_bytes(mask);
                for (index, bit) in [1u16, 2, 4, 8, 0x10, 0x40, 0x80, 0x100, 0x200]
                    .into_iter()
                    .enumerate()
                {
                    if mask & bit != 0 {
                        let color = self.cpu.read_u32(a1 + 4 + index as u32 * 4)?;
                        self.legacy_controls.get_mut(&this).unwrap().colors[index] = color;
                    }
                }
                SUCCESS
            }
            _ => return Ok(None),
        };
        Ok(Some(result))
    }

    fn legacy_text_call(
        &mut self,
        this: u32,
        name: &str,
        [a1, a2, a3]: [u32; 3],
    ) -> Result<Option<u32>, CpuError> {
        let state = self.legacy_controls[&this].clone();
        let result = match name {
            "SetTitle" => {
                let Some(title) = self.legacy_label(a1, a2 as u16, a3)? else {
                    return Ok(Some(FALSE));
                };
                self.legacy_controls.get_mut(&this).unwrap().title = title;
                TRUE
            }
            "SetText" => {
                // cch positivo delimita a origem: não exigir NUL depois do prefixo pedido.
                let text = if a2 as i32 >= 0 {
                    self.legacy_read_units(a1, (a2 as usize).min(state.max_size + 1), false)?
                } else {
                    self.legacy_read_text(a1, MAX_TEXT)?
                };
                let Some(text) = text else {
                    return Ok(Some(FALSE));
                };
                u32::from(self.legacy_set_text(this, text, 0)?)
            }
            "GetText" => {
                if a1 == 0 || a2 == 0 {
                    return Ok(Some(FALSE));
                }
                let text = self.legacy_current_text(&state)?;
                let count = text.len().min((a2 as usize).saturating_sub(1));
                self.cpu.write_mem(
                    a1,
                    &text[..count]
                        .iter()
                        .copied()
                        .chain([0])
                        .flat_map(u16::to_le_bytes)
                        .collect::<Vec<_>>(),
                )?;
                TRUE
            }
            "GetTextPtr" => {
                if state.text_pointer == 0 && !self.legacy_set_text(this, state.text, 1)? {
                    return Ok(Some(0));
                }
                self.legacy_controls[&this].text_pointer
            }
            "EnableCommand" => {
                let state = self.legacy_controls.get_mut(&this).unwrap();
                state.command_enabled = a1 != 0;
                state.command_id = a2 as u16;
                SUCCESS
            }
            "SetMaxSize" => {
                let size = a1 as u16 as usize;
                if size == 0 {
                    return Ok(Some(SUCCESS));
                }
                if size > MAX_TEXT {
                    return Ok(None);
                }
                let mut text = self.legacy_current_text(&state)?;
                text.truncate(size);
                // Aumentar o limite pode alocar; não perder o limite anterior numa falha.
                self.legacy_controls.get_mut(&this).unwrap().max_size = size;
                if !self.legacy_set_text(this, text, size + 1)? {
                    self.legacy_controls.get_mut(&this).unwrap().max_size = state.max_size;
                    return Ok(Some(SDK_NO_MEMORY));
                }
                SUCCESS
            }
            "SetSoftKeyMenu" => {
                if a1 != 0
                    && !self
                        .legacy_controls
                        .get(&a1)
                        .is_some_and(|control| control.kind.is_menu())
                {
                    return Ok(Some(SDK_BAD_PARAM));
                }
                if a1 != 0 {
                    self.objects.add_ref(a1);
                }
                self.legacy_controls.get_mut(&this).unwrap().soft_menu = a1;
                if state.soft_menu != 0 {
                    self.legacy_release(state.soft_menu);
                }
                SUCCESS
            }
            "GetCursorPos" => state.cursor as u32,
            "SetCursorPos" => {
                let cursor =
                    ((a1 as i32).max(0) as usize).min(self.legacy_current_text(&state)?.len());
                self.legacy_controls.get_mut(&this).unwrap().cursor = cursor;
                SUCCESS
            }
            _ => return Ok(None),
        };
        Ok(Some(result))
    }

    fn legacy_static_call(
        &mut self,
        this: u32,
        name: &str,
        [a1, a2, a3]: [u32; 3],
    ) -> Result<Option<u32>, CpuError> {
        match name {
            "SetText" => {
                let Some(title) = self.legacy_read_text(a1, MAX_MENU_TEXT)? else {
                    return Ok(Some(FALSE));
                };
                let Some(text) = self.legacy_read_text(a2, MAX_TEXT)? else {
                    return Ok(Some(FALSE));
                };
                let font_text = self.stack_arg(0)? as u16;
                let state = self.legacy_controls.get_mut(&this).unwrap();
                state.title = title;
                state.text = text;
                state.font_title = a3 as u16;
                state.font_text = font_text;
                Ok(Some(TRUE))
            }
            "SetFont" => {
                let state = self.legacy_controls.get_mut(&this).unwrap();
                state.font_text = a1 as u16;
                state.font_title = a2 as u16;
                Ok(Some(SUCCESS))
            }
            _ => Ok(None),
        }
    }

    fn legacy_handle_event(
        &mut self,
        this: u32,
        event: u32,
        key: u16,
        _data: u32,
    ) -> Result<u32, CpuError> {
        let state = self.legacy_controls[&this].clone();
        if !state.active
            || !state.kind.is_menu()
            || event != EVT_KEY_LEGACY
            || state.items.is_empty()
        {
            return Ok(FALSE);
        }
        let mut selected = state.selected.unwrap_or(0);
        let mut activate = false;
        let direction = match (state.kind, key) {
            (LegacyControlKind::Menu, 0xe031) | (LegacyControlKind::Softkey, 0xe033) => -1,
            (LegacyControlKind::Menu, 0xe032) | (LegacyControlKind::Softkey, 0xe034) => 1,
            (_, 0xe035) => {
                activate = true;
                0
            }
            (LegacyControlKind::Softkey, 0xe036..=0xe038) => {
                selected = usize::from(key - 0xe036);
                if selected >= state.items.len() {
                    return Ok(FALSE);
                }
                activate = true;
                0
            }
            _ => return Ok(FALSE),
        };
        if direction < 0 {
            selected = if selected > 0 {
                selected - 1
            } else if state.properties & MP_WRAPSCROLL != 0 {
                state.items.len() - 1
            } else {
                0
            };
        } else if direction > 0 {
            selected = if selected + 1 < state.items.len() {
                selected + 1
            } else if state.properties & MP_WRAPSCROLL != 0 {
                0
            } else {
                selected
            };
        }
        self.legacy_controls.get_mut(&this).unwrap().selected = Some(selected);
        if state.properties & MP_NO_REDRAW == 0 {
            self.legacy_redraw(this)?;
        }
        if activate && state.command_enabled {
            // Nenhum empréstimo do mapa atravessa o guest: o comando pode Reset/Release o controle.
            let item = &state.items[selected];
            self.send_applet_event(self.applet_class, EVT_COMMAND_LEGACY, item.id, item.data)?;
        }
        Ok(TRUE)
    }

    fn legacy_static_text_x(&self, state: &LegacyControl, text: &[u16], property: u32) -> i32 {
        if state.kind == LegacyControlKind::Static && state.properties & property != 0 {
            // AEEShell.h: título e corpo possuem alinhamento independente. Medir com a
            // mesma fonte usada no desenho evita centralizar por uma largura de célula fictícia.
            let width = self.font.as_ref().map_or(0, |font| {
                font.width(&String::from_utf16_lossy(text), FONT_SIZE) as i32
            });
            i32::from(state.rect[0]) + (i32::from(state.rect[2]) - width).max(0) / 2
        } else {
            i32::from(state.rect[0]) + 2
        }
    }

    fn legacy_draw_text(
        &mut self,
        units: &[u16],
        x: i32,
        y: i32,
        color: u32,
    ) -> Result<(), CpuError> {
        let previous = self.colors[CLR_USER_TEXT];
        self.colors[CLR_USER_TEXT] = Rgb::from_rgbval(color);
        let result = self.draw_text(&String::from_utf16_lossy(units), x, y);
        self.colors[CLR_USER_TEXT] = previous;
        result.map(|_| ())
    }

    fn legacy_redraw(&mut self, this: u32) -> Result<(), CpuError> {
        let Some(mut state) = self.legacy_controls.get(&this).cloned() else {
            return Ok(());
        };
        if self.font.is_none() {
            self.font = egui::FontDefinitions::default()
                .font_data
                .get("Ubuntu-Light")
                .and_then(|data| {
                    crate::video::font::Font::load(data.font.to_vec(), "Ubuntu-Light".into())
                });
        }
        state.text = self.legacy_current_text(&state)?;
        let rect = state.rect();
        let Some(clip) = self.clip_rect(rect) else {
            return Ok(());
        };
        let previous_clip = self.clip.replace(clip);
        let result = (|| {
            let target = self.target()?;
            if let Some(fb) = self.bitmaps.get_mut(&target) {
                fb.fill_rect(clip, Rgb::from_rgbval(state.colors[0]));
            }
            let row_height = if state.kind.is_menu() {
                let image_height = state
                    .items
                    .iter()
                    .filter_map(|item| self.images.get(&item.image))
                    .map(|image| image.height.min(i16::MAX as u32) as i32)
                    .max()
                    .unwrap_or(0);
                state
                    .normal_style
                    .row_height()
                    .max(state.selected_style.row_height())
                    + (image_height - FONT_SIZE as i32).max(0)
            } else {
                FONT_SIZE as i32 + 4
            };
            let mut y = i32::from(rect.y) + if state.kind.is_menu() { 0 } else { 2 };
            if !state.title.is_empty() {
                let x = self.legacy_static_text_x(&state, &state.title, ST_CENTERTITLE);
                self.legacy_draw_text(&state.title, x, y, state.colors[8])?;
                y += row_height;
            }
            if state.kind.is_menu() {
                let visible = ((i32::from(rect.height) - (y - i32::from(rect.y))) / row_height)
                    .max(1) as usize;
                let first = if state.kind == LegacyControlKind::Softkey {
                    0
                } else {
                    state.selected.unwrap_or(0).saturating_sub(visible - 1)
                };
                for (index, item) in state.items.iter().enumerate().skip(first) {
                    if state.kind != LegacyControlKind::Softkey && index - first >= visible {
                        break;
                    }
                    let (x, width, item_y) = if state.kind == LegacyControlKind::Softkey {
                        let width = i32::from(rect.width).max(0) / state.items.len().max(1) as i32;
                        (
                            i32::from(rect.x) + index as i32 * width,
                            width,
                            i32::from(rect.y),
                        )
                    } else {
                        (
                            i32::from(rect.x),
                            i32::from(rect.width),
                            y + (index - first) as i32 * row_height,
                        )
                    };
                    let selected = state.active && state.selected == Some(index);
                    let style = if selected {
                        state.selected_style
                    } else {
                        state.normal_style
                    };
                    let border = style.frame_width().unwrap_or(0);
                    let item_rect = Rect {
                        x: x as i16,
                        y: item_y as i16,
                        width: width as i16,
                        height: row_height.min(i32::from(i16::MAX)) as i16,
                    };
                    if selected {
                        let highlight = Rect {
                            x: x as i16,
                            y: item_y as i16,
                            width: width as i16,
                            height: row_height as i16,
                        };
                        if let Some(visible) = self.clip_rect(highlight) {
                            if let Some(fb) = self.bitmaps.get_mut(&target) {
                                fb.fill_rect(visible, Rgb::from_rgbval(state.colors[2]));
                            }
                        }
                    }
                    if style.frame == 5 && item_rect.width > 0 && item_rect.height > 0 {
                        // Recortar cada aresta separadamente não inventa uma moldura no clip.
                        let edges = [
                            Rect {
                                height: 1,
                                ..item_rect
                            },
                            Rect {
                                y: item_rect.y.saturating_add(item_rect.height - 1),
                                height: 1,
                                ..item_rect
                            },
                            Rect {
                                width: 1,
                                ..item_rect
                            },
                            Rect {
                                x: item_rect.x.saturating_add(item_rect.width - 1),
                                width: 1,
                                ..item_rect
                            },
                        ];
                        for edge in edges {
                            if let (Some(area), Some(fb)) =
                                (self.clip_rect(edge), self.bitmaps.get_mut(&target))
                            {
                                fb.fill_rect(area, Rgb::from_rgbval(state.colors[4]));
                            }
                        }
                    }
                    let content_x = x + border + i32::from(style.x_offset);
                    let content_y = item_y + border + i32::from(style.y_offset);
                    let mut text_x = content_x;
                    if item.image != 0 {
                        let old_crop = self.recortes_de_imagem.get(&item.image).copied();
                        self.recortes_de_imagem
                            .entry(item.image)
                            .or_default()
                            .transparente = u32::from(style.image_rop) == AEE_RO_TRANSPARENT;
                        let drawn = self.draw_image(item.image, content_x, content_y, Some(0));
                        match old_crop {
                            Some(crop) => {
                                self.recortes_de_imagem.insert(item.image, crop);
                            }
                            None => {
                                self.recortes_de_imagem.remove(&item.image);
                            }
                        }
                        drawn?;
                        if let Some(image) = self.images.get(&item.image) {
                            text_x += if image.frame_width > 0 {
                                i32::from(image.frame_width)
                            } else {
                                image.width.min(i16::MAX as u32) as i32
                            } + 2;
                        }
                    }
                    self.legacy_draw_text(
                        &item.text,
                        text_x,
                        item_y + border + i32::from(style.y_offset),
                        state.colors[if selected { 3 } else { 1 }],
                    )?;
                }
            } else {
                for line in String::from_utf16_lossy(&state.text).split('\n') {
                    let units = line.encode_utf16().collect::<Vec<_>>();
                    let x = self.legacy_static_text_x(&state, &units, ST_CENTERTEXT);
                    self.legacy_draw_text(&units, x, y, state.colors[1])?;
                    y += row_height;
                }
            }
            self.guarda_quadro_do_update()?;
            Ok(())
        })();
        self.clip = previous_clip;
        result
    }

    pub(super) fn grava_controles_legados(&self, sections: &mut crate::save_state::Secoes) {
        let mut controls = self
            .legacy_controls
            .iter()
            .map(|(&object, state)| (object, state.clone()))
            .collect::<Vec<_>>();
        controls.sort_by_key(|(object, _)| *object);
        for (_, state) in &mut controls {
            if let Ok(text) = self.legacy_current_text(state) {
                state.text = text;
            }
        }
        sections.poe(
            "tab.legacy_controls",
            serde_json::to_vec(&SavedControls {
                version: 1,
                controls,
            })
            .expect("estado de controles contém apenas valores serializáveis"),
        );
    }

    pub(super) fn restaura_controles_legados(
        &mut self,
        reader: &crate::save_state::Leitor<'_>,
    ) -> Result<(), crate::save_state::Erro> {
        use crate::save_state::Erro;
        let invalid = |reason: &str| Erro::Secao {
            nome: "tab.legacy_controls".into(),
            motivo: reason.into(),
        };
        let Some(bytes) = reader.secao("tab.legacy_controls") else {
            self.legacy_controls.clear();
            return Ok(());
        };
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(invalid("controles acima do limite"));
        }
        let saved: SavedControls = serde_json::from_slice(bytes)
            .map_err(|_| invalid("estrutura de controles inválida"))?;
        if saved.version != 1
            || saved.controls.len() > MAX_CONTROLS
            || saved
                .controls
                .iter()
                .map(|(_, control)| control.items.len())
                .sum::<usize>()
                > MAX_ITEMS
        {
            return Err(invalid("versão ou quantidade de controles inválida"));
        }
        let mut restored = std::collections::HashMap::new();
        let mut owned_text = std::collections::HashSet::new();
        let mut image_owners = std::collections::HashMap::<u32, u32>::new();
        for (object, state) in saved.controls {
            if state.title.len() > MAX_MENU_TEXT
                || state.text.len() > MAX_TEXT
                || state.max_size > MAX_TEXT
                || state.max_size == 0
                || state.text.len() > state.max_size
                || state.text_capacity > MAX_TEXT + 1
                || state.cursor > state.max_size
                || state.normal_style.frame_width().is_none()
                || state.selected_style.frame_width().is_none()
                || ![2, 7].contains(&state.normal_style.image_rop)
                || ![2, 7].contains(&state.selected_style.image_rop)
                || state.items.iter().any(|item| {
                    item.text.len() > MAX_MENU_TEXT
                        || (item.image != 0
                            && (self.objects.kind_of(item.image) != Some(Interface::Image)
                                || !self.images.contains_key(&item.image)))
                })
                || state
                    .selected
                    .is_some_and(|index| index >= state.items.len())
                || self.objects.kind_of(object) != Some(state.kind.interface())
            {
                return Err(invalid("metadados de controle inválidos"));
            }
            if (state.text_pointer == 0) != (state.text_capacity == 0)
                || (state.text_pointer != 0
                    && (!owned_text.insert(state.text_pointer)
                        || state.text_capacity <= state.text.len()
                        || self
                            .heap
                            .size_of(state.text_pointer)
                            .is_none_or(|bytes| bytes < state.text_capacity as u32 * 2)))
            {
                return Err(invalid("buffer de texto não pertence ao heap restaurado"));
            }
            for item in &state.items {
                if item.image != 0 {
                    *image_owners.entry(item.image).or_default() += 1;
                }
                for (pointer, needed) in [
                    (item.text_pointer, (item.text.len() + 1) * 2),
                    (
                        item.text_resource.pointer,
                        item.text_resource.file.len() + 1,
                    ),
                    (
                        item.image_resource.pointer,
                        item.image_resource.file.len() + 1,
                    ),
                ] {
                    if pointer != 0
                        && (!owned_text.insert(pointer)
                            || self
                                .heap
                                .size_of(pointer)
                                .is_none_or(|size| size < needed as u32))
                    {
                        return Err(invalid("buffer de item não pertence ao heap restaurado"));
                    }
                }
                if [
                    item.text_resource.file.len(),
                    item.image_resource.file.len(),
                ]
                .into_iter()
                .any(|length| length > AEE_MAX_FILE_NAME as usize)
                {
                    return Err(invalid("nome de recurso de item acima do limite"));
                }
            }
            if restored.insert(object, state).is_some() {
                return Err(invalid("controle duplicado"));
            }
        }
        if restored.values().any(|state| {
            state.soft_menu != 0
                && !restored
                    .get(&state.soft_menu)
                    .is_some_and(|menu| menu.kind.is_menu())
        }) {
            return Err(invalid("referência de softkey inválida"));
        }
        // Uma mesma imagem pode servir a vários itens; cada item retém sua referência.
        if image_owners
            .iter()
            .any(|(&image, &owners)| self.objects.contagem(image) < owners)
        {
            return Err(invalid("referências retidas da imagem insuficientes"));
        }
        self.legacy_controls = restored;
        Ok(())
    }
}
