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
const MP_NO_ARROWS: u32 = 0x0002_0000;
const MP_ALLOW_DUPE_IDS: u32 = 0x0004_0000;
const MP_NO_REDRAW: u32 = 0x0008_0000;
const MP_MAXSOFTKEYITEMS: u32 = 0x0010_0000;
const MP_ICON_TEXT_TOP: u32 = 0x0080_0000;
const MP_ICON_NO_CENTERSEL: u32 = 0x0100_0000;
const MP_UNDERLINE_TITLE: u32 = 0x0400_0000;
const TP_NODRAW: u32 = 0x0010_0000;
const ST_CENTERTEXT: u32 = 0x0001_0000;
const ST_CENTERTITLE: u32 = 0x0002_0000;
const EVT_KEY_LEGACY: u32 = 0x100;
const EVT_COMMAND_LEGACY: u32 = 0x200; // inc/AEEEvent.h, não um código de tecla.
const TM_LETTERS: u32 = 3;
const TM_NUMBERS: u32 = 5;
const TEXT_MODES: [u32; 2] = [TM_LETTERS, TM_NUMBERS];
const MULTITAP_MS: u32 = 1000;

fn default_input_mode() -> u32 {
    TM_LETTERS
}

fn legacy_letters(key: u16) -> Option<&'static [u8]> {
    // O mapa do teclado é local ao aparelho; os modos preditivos exigem um dicionário.
    Some(match key {
        0xe021 => b" 0",
        0xe022 => b".,?!1",
        0xe023 => b"abc2",
        0xe024 => b"def3",
        0xe025 => b"ghi4",
        0xe026 => b"jkl5",
        0xe027 => b"mno6",
        0xe028 => b"pqrs7",
        0xe029 => b"tuv8",
        0xe02a => b"wxyz9",
        _ => return None,
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct LegacyMultitap {
    key: u16,
    cycle: usize,
    position: usize,
    character: u16,
    last_ms: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum LegacyControlKind {
    Menu,
    List,
    Softkey,
    IconView,
    Text100,
    Static,
}

impl LegacyControlKind {
    fn interface(self) -> Interface {
        match self {
            Self::Menu | Self::List | Self::IconView => Interface::MenuLegacy,
            Self::Softkey => Interface::SoftkeyLegacy,
            Self::Text100 => Interface::TextLegacy100,
            Self::Static => Interface::StaticLegacy,
        }
    }

    fn is_menu(self) -> bool {
        matches!(
            self,
            Self::Menu | Self::List | Self::Softkey | Self::IconView
        )
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
            1 | 5 => Some(1),     // EMPTY / BOX
            2 | 3 | 4 => Some(2), // 3D_EMPTY / RAISED / SUNKEN
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
    #[serde(default = "default_input_mode")]
    input_mode: u32,
    #[serde(default)]
    mode_enum_index: usize,
    #[serde(default)]
    multitap: Option<LegacyMultitap>,
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
            properties: if matches!(kind, LegacyControlKind::Softkey | LegacyControlKind::List) {
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
            input_mode: TM_LETTERS,
            mode_enum_index: 0,
            multitap: None,
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

    fn valid_multitap(&self) -> bool {
        self.multitap.as_ref().is_none_or(|tap| {
            self.kind == LegacyControlKind::Text100
                && self.input_mode == TM_LETTERS
                && tap.position.checked_add(1) == Some(self.cursor)
                && self.text.get(tap.position) == Some(&tap.character)
                && legacy_letters(tap.key)
                    .and_then(|letters| letters.get(tap.cycle))
                    .is_some_and(|letter| u16::from(*letter) == tap.character)
        })
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
        state.multitap = None;
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
                if a1 == 0 {
                    self.legacy_controls.get_mut(&this).unwrap().multitap = None;
                }
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
                if state.kind == LegacyControlKind::IconView
                    && a1
                        & !(MP_WRAPSCROLL
                            | MP_ALLOW_DUPE_IDS
                            | MP_NO_REDRAW
                            | MP_NO_ARROWS
                            | MP_MAXSOFTKEYITEMS
                            | MP_ICON_TEXT_TOP
                            | MP_ICON_NO_CENTERSEL)
                        != 0
                {
                    // SINGLE_FRAME, animação e estados múltiplos precisam de vistas próprias.
                    return Ok(None);
                }
                // AEEMenu.h limita MAXSOFTKEYITEMS à vista softkey. Preservar esse
                // bit documentado numa IconView não muda suas células nem a transforma
                // em SINGLE_FRAME; NO_ARROWS também só atua nesta última modalidade.
                self.legacy_controls.get_mut(&this).unwrap().properties = a1
                    | if matches!(
                        state.kind,
                        LegacyControlKind::Softkey | LegacyControlKind::List
                    ) {
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
                target.multitap = None;
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
                    self.install_decoded_image(image, decoded)?;
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
                        // As abas e os outros efeitos continuam sem anunciar desenho inexistente.
                        if value.frame_width().is_none() || ![0, 2, 7].contains(&value.image_rop) {
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
                let target = self.legacy_controls.get_mut(&this).unwrap();
                target.cursor = cursor;
                target.multitap = None;
                SUCCESS
            }
            "SetInputMode" => {
                // AEEText.h devolve o modo realmente selecionado, não um código SUCCESS.
                // KANJI/RAPID e modos OEM não podem anunciar editores que não existem.
                let mode = match a1 {
                    0 => TM_LETTERS,
                    mode if TEXT_MODES.contains(&mode) => mode,
                    _ => state.input_mode,
                };
                let target = self.legacy_controls.get_mut(&this).unwrap();
                if mode != target.input_mode {
                    target.multitap = None;
                }
                target.input_mode = mode;
                mode
            }
            "GetInputMode" => {
                self.legacy_mode_info(a1, state.input_mode)?;
                state.input_mode
            }
            "EnumModeInit" => {
                self.legacy_controls.get_mut(&this).unwrap().mode_enum_index = 0;
                SUCCESS
            }
            "EnumNextMode" => {
                let mode = TEXT_MODES.get(state.mode_enum_index).copied().unwrap_or(0);
                self.legacy_mode_info(a1, mode)?;
                self.legacy_controls.get_mut(&this).unwrap().mode_enum_index =
                    (state.mode_enum_index + 1).min(TEXT_MODES.len());
                mode
            }
            _ => return Ok(None),
        };
        Ok(Some(result))
    }

    fn legacy_mode_info(&mut self, pointer: u32, mode: u32) -> Result<(), CpuError> {
        if pointer != 0 {
            // int32 + AECHAR[15]; os dois bytes finais de alinhamento não são campos.
            let mut bytes = [0; 34];
            bytes[..4].copy_from_slice(&mode.to_le_bytes());
            let name: &[u8] = match mode {
                TM_LETTERS => b"abc",
                TM_NUMBERS => b"123",
                _ => b"",
            };
            for (at, character) in name.iter().enumerate() {
                bytes[4 + at * 2..6 + at * 2].copy_from_slice(&u16::from(*character).to_le_bytes());
            }
            self.cpu.write_mem(pointer, &bytes)?;
        }
        Ok(())
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
        if state.kind == LegacyControlKind::Text100 {
            return self.legacy_text_event(this, state, event, key);
        }
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
            (LegacyControlKind::IconView, 0xe031..=0xe034) => {
                let columns = self.legacy_icon_cell(&state).0;
                let column = selected % columns;
                // IconView sempre dá a volta, mesmo sem publicar MP_WRAPSCROLL.
                // Uma última linha incompleta não muda a coluna ao navegar verticalmente.
                selected = match key {
                    0xe031 if selected >= columns => selected - columns,
                    0xe031 => column + (state.items.len() - 1 - column) / columns * columns,
                    0xe032 if selected + columns < state.items.len() => selected + columns,
                    0xe032 => column,
                    0xe033 if selected > 0 => selected - 1,
                    0xe033 => state.items.len() - 1,
                    _ => (selected + 1) % state.items.len(),
                };
                0
            }
            (LegacyControlKind::Menu | LegacyControlKind::List, 0xe031)
            | (LegacyControlKind::Softkey, 0xe033) => -1,
            (LegacyControlKind::Menu | LegacyControlKind::List, 0xe032)
            | (LegacyControlKind::Softkey, 0xe034) => 1,
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

    fn legacy_text_event(
        &mut self,
        this: u32,
        state: LegacyControl,
        event: u32,
        key: u16,
    ) -> Result<u32, CpuError> {
        if !state.active || event != EVT_KEY_LEGACY {
            return Ok(FALSE);
        }
        let mut text = self.legacy_current_text(&state)?;
        let mut cursor = state.cursor.min(text.len());
        let now = self.now_ms();
        let mut multitap = None;
        match key {
            0xe033 | 0xe034 => {
                // Não deixar o cursor no meio de um par UTF-16 que veio de SetText.
                if key == 0xe033 && cursor > 0 {
                    cursor -= 1;
                    if cursor > 0
                        && (0xdc00..=0xdfff).contains(&text[cursor])
                        && (0xd800..=0xdbff).contains(&text[cursor - 1])
                    {
                        cursor -= 1;
                    }
                } else if key == 0xe034 && cursor < text.len() {
                    cursor += 1;
                    if cursor < text.len()
                        && (0xd800..=0xdbff).contains(&text[cursor - 1])
                        && (0xdc00..=0xdfff).contains(&text[cursor])
                    {
                        cursor += 1;
                    }
                }
            }
            0xe030 => {
                if cursor == 0 {
                    self.legacy_controls.get_mut(&this).unwrap().multitap = None;
                    return Ok(FALSE);
                }
                let end = cursor;
                cursor -= 1;
                if cursor > 0
                    && (0xdc00..=0xdfff).contains(&text[cursor])
                    && (0xd800..=0xdbff).contains(&text[cursor - 1])
                {
                    cursor -= 1;
                }
                text.drain(cursor..end);
            }
            0xe021..=0xe02a => {
                let character = if state.input_mode == TM_NUMBERS {
                    u16::from(b'0') + key - 0xe021
                } else {
                    let letters = legacy_letters(key).expect("dígito do teclado");
                    // GetTextPtr é gravável. Só substituir o caractere ainda pertencente
                    // à composição; uma edição do guest já confirmou o anterior.
                    let previous = state.multitap.as_ref().filter(|tap| {
                        tap.key == key
                            && tap.position.checked_add(1) == Some(cursor)
                            && text.get(tap.position) == Some(&tap.character)
                            && now.wrapping_sub(tap.last_ms) < MULTITAP_MS
                    });
                    let cycle = previous.map_or(0, |tap| (tap.cycle + 1) % letters.len());
                    let character = u16::from(letters[cycle]);
                    if let Some(tap) = previous {
                        text[tap.position] = character;
                        multitap = Some(LegacyMultitap {
                            key,
                            cycle,
                            position: tap.position,
                            character,
                            last_ms: now,
                        });
                    } else if text.len() < state.max_size {
                        text.insert(cursor, character);
                        multitap = Some(LegacyMultitap {
                            key,
                            cycle,
                            position: cursor,
                            character,
                            last_ms: now,
                        });
                        cursor += 1;
                    } else {
                        return Ok(TRUE);
                    }
                    character
                };
                if state.input_mode == TM_NUMBERS {
                    if text.len() >= state.max_size {
                        return Ok(TRUE);
                    }
                    text.insert(cursor, character);
                    cursor += 1;
                }
            }
            _ => return Ok(FALSE),
        }
        if !self.legacy_set_text(this, text, 0)? {
            return Ok(FALSE);
        }
        let target = self.legacy_controls.get_mut(&this).unwrap();
        target.cursor = cursor;
        target.multitap = multitap;
        if state.properties & TP_NODRAW == 0 {
            self.legacy_redraw(this)?;
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

    fn legacy_menu_frame(
        &mut self,
        target: u32,
        rect: Rect,
        style: LegacyItemStyle,
        frame_color: u32,
    ) {
        if !matches!(style.frame, 3 | 4 | 5) {
            return;
        }
        for layer in 0..style.frame_width().unwrap_or(0) as i16 {
            let inner = Rect {
                x: rect.x.saturating_add(layer),
                y: rect.y.saturating_add(layer),
                width: rect.width.saturating_sub(layer * 2),
                height: rect.height.saturating_sub(layer * 2),
            };
            if inner.width <= 0 || inner.height <= 0 {
                break;
            }
            let (mut light, mut dark) = if matches!(style.frame, 3 | 4) {
                // RAISED é um relevo de dois pixels. A paleta neutra pertence ao tema
                // do aparelho; cFrame continua sendo a cor do quadro BOX.
                if layer == 0 {
                    (Rgb::WHITE, Rgb::BLACK)
                } else {
                    (
                        Rgb {
                            r: 192,
                            g: 192,
                            b: 192,
                        },
                        Rgb {
                            r: 128,
                            g: 128,
                            b: 128,
                        },
                    )
                }
            } else {
                let color = Rgb::from_rgbval(frame_color);
                (color, color)
            };
            if style.frame == 4 {
                std::mem::swap(&mut light, &mut dark);
            }
            let edges = [
                (Rect { height: 1, ..inner }, light),
                (Rect { width: 1, ..inner }, light),
                (
                    Rect {
                        y: inner.y.saturating_add(inner.height - 1),
                        height: 1,
                        ..inner
                    },
                    dark,
                ),
                (
                    Rect {
                        x: inner.x.saturating_add(inner.width - 1),
                        width: 1,
                        ..inner
                    },
                    dark,
                ),
            ];
            // Recortar cada aresta preserva o relevo quando o item atravessa o clip.
            for (edge, color) in edges {
                if let (Some(area), Some(fb)) =
                    (self.clip_rect(edge), self.bitmaps.get_mut(&target))
                {
                    fb.fill_rect(area, color);
                }
            }
        }
    }

    fn legacy_or_image(&mut self, image: u32, x: i32, y: i32) -> Result<(), CpuError> {
        let target = self.target()?;
        let Some(info) = self.images.get(&image) else {
            return Ok(());
        };
        let crop = self
            .recortes_de_imagem
            .get(&image)
            .copied()
            .unwrap_or_default();
        let frame_width = if info.frame_width > 0 {
            i32::from(info.frame_width)
        } else {
            info.width as i32
        };
        let (source_x, source_y) = (crop.x.max(0), crop.y.max(0));
        let (width, height) = crop.tamanho.unwrap_or((i32::MAX, i32::MAX));
        let (mut first_x, mut last_x) = (0, width.min(frame_width - source_x));
        let (mut first_y, mut last_y) = (0, height.min(info.height as i32 - source_y));
        if let Some(clip) = self.clip {
            first_x = first_x.max(i32::from(clip.x) - x);
            last_x = last_x.min(i32::from(clip.x) + i32::from(clip.width) - x);
            first_y = first_y.max(i32::from(clip.y) - y);
            last_y = last_y.min(i32::from(clip.y) + i32::from(clip.height) - y);
        }
        if first_x >= last_x || first_y >= last_y {
            return Ok(());
        }
        let Some(surface) = self.bitmaps.get_mut(&target) else {
            // Um IBitmap do guest recebe o ROP verdadeiro no callback, como DrawFrame.
            self.pending_blits.push(PendingBlit {
                image,
                target,
                x: x + first_x,
                y: y + first_y,
                src_x: source_x + first_x,
                src_y: source_y + first_y,
                width: (last_x - first_x) as u32,
                height: (last_y - first_y) as u32,
                rop: 0,
                frame: Some(0),
            });
            return Ok(());
        };
        first_x = first_x.max(-x).max(0);
        first_y = first_y.max(-y).max(0);
        last_x = last_x.min(surface.width() as i32 - x);
        last_y = last_y.min(surface.height() as i32 - y);
        for row in first_y..last_y {
            for column in first_x..last_x {
                let source =
                    ((row + source_y) as u32 * info.width + (column + source_x) as u32) as usize;
                let Some(&pixel) = info.pixels.get(source) else {
                    continue;
                };
                let background = surface.get_pixel(x + column, y + row);
                let result = background | pixel;
                match info.alfa.get(source).copied() {
                    Some(0) => {}
                    Some(alpha) => surface.set_pixel_native(
                        x + column,
                        y + row,
                        mistura_rgb565(background, result, alpha),
                    ),
                    None if info.opaque.get(source).copied().unwrap_or(true) => {
                        surface.set_pixel_native(x + column, y + row, result);
                    }
                    None => {}
                }
            }
        }
        Ok(())
    }

    fn legacy_icon_cell(&self, state: &LegacyControl) -> (usize, i32, i32) {
        let mut images = state
            .items
            .iter()
            .filter_map(|item| self.images.get(&item.image));
        let first = images.next();
        let (mut width, mut height) = first.map_or((FONT_SIZE as i32, FONT_SIZE as i32), |image| {
            (
                if image.frame_width > 0 {
                    i32::from(image.frame_width)
                } else {
                    image.width.min(i16::MAX as u32) as i32
                },
                image.height.min(i16::MAX as u32) as i32,
            )
        });
        for image in images {
            width = width.max(if image.frame_width > 0 {
                i32::from(image.frame_width)
            } else {
                image.width.min(i16::MAX as u32) as i32
            });
            height = height.max(image.height.min(i16::MAX as u32) as i32);
        }
        let padding_x = [state.normal_style, state.selected_style]
            .into_iter()
            .map(|style| style.frame_width().unwrap_or(0) + i32::from(style.x_offset))
            .max()
            .unwrap_or(0);
        let padding_y = [state.normal_style, state.selected_style]
            .into_iter()
            .map(|style| style.frame_width().unwrap_or(0) + i32::from(style.y_offset))
            .max()
            .unwrap_or(0);
        let cell_width = (width + 2 * padding_x).max(1);
        let cell_height = (height + 2 * padding_y).max(1);
        let columns = (i32::from(state.rect[2]).max(0) / cell_width).max(1) as usize;
        (columns, cell_width, cell_height)
    }

    fn legacy_icon_redraw(
        &mut self,
        state: &LegacyControl,
        target: u32,
        top: i32,
    ) -> Result<(), CpuError> {
        let (columns, cell_width, cell_height) = self.legacy_icon_cell(state);
        let rect = state.rect();
        let label_height = FONT_SIZE as i32 + 4;
        let label_at_top = state.properties & MP_ICON_TEXT_TOP != 0;
        let label_y = if label_at_top {
            top
        } else {
            (i32::from(rect.y) + i32::from(rect.height) - label_height).max(top)
        };
        let grid_top = top + if label_at_top { label_height } else { 0 };
        let grid_height = (i32::from(rect.y) + i32::from(rect.height)
            - grid_top
            - if label_at_top { 0 } else { label_height })
        .max(0);
        let visible_rows = (grid_height / cell_height).max(1) as usize;
        let total_rows = state.items.len().div_ceil(columns);
        let selected_row = state.selected.unwrap_or(0) / columns;
        let offset = if state.properties & MP_ICON_NO_CENTERSEL == 0 {
            visible_rows / 2
        } else {
            visible_rows - 1
        };
        let first_row = selected_row
            .saturating_sub(offset)
            .min(total_rows.saturating_sub(visible_rows));
        let origin_x =
            i32::from(rect.x) + (i32::from(rect.width) - columns as i32 * cell_width).max(0) / 2;
        let grid = Rect {
            x: rect.x,
            y: grid_top.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
            width: rect.width,
            height: grid_height.min(i32::from(i16::MAX)) as i16,
        };
        let previous_clip = self.clip;
        if let Some(clip) = self.clip_rect(grid) {
            self.clip = Some(clip);
            let result: Result<(), CpuError> = (|| {
                for (index, item) in state.items.iter().enumerate().skip(first_row * columns) {
                    let row = index / columns - first_row;
                    if row >= visible_rows {
                        break;
                    }
                    let x = origin_x + (index % columns) as i32 * cell_width;
                    let y = grid_top + row as i32 * cell_height;
                    let selected = state.active && state.selected == Some(index);
                    let style = if selected {
                        state.selected_style
                    } else {
                        state.normal_style
                    };
                    let cell = Rect {
                        x: x.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
                        y: y.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
                        width: cell_width.min(i32::from(i16::MAX)) as i16,
                        height: cell_height.min(i32::from(i16::MAX)) as i16,
                    };
                    if selected {
                        if let (Some(area), Some(fb)) =
                            (self.clip_rect(cell), self.bitmaps.get_mut(&target))
                        {
                            fb.fill_rect(area, Rgb::from_rgbval(state.colors[2]));
                        }
                    }
                    self.legacy_menu_frame(target, cell, style, state.colors[4]);
                    if item.image != 0 {
                        let x = x + style.frame_width().unwrap_or(0) + i32::from(style.x_offset);
                        let y = y + style.frame_width().unwrap_or(0) + i32::from(style.y_offset);
                        let old_crop = self.recortes_de_imagem.get(&item.image).copied();
                        self.recortes_de_imagem
                            .entry(item.image)
                            .or_default()
                            .transparente = u32::from(style.image_rop) == AEE_RO_TRANSPARENT;
                        let drawn = if style.image_rop == 0 {
                            self.legacy_or_image(item.image, x, y)
                        } else {
                            self.draw_image(item.image, x, y, Some(0))
                        };
                        match old_crop {
                            Some(crop) => {
                                self.recortes_de_imagem.insert(item.image, crop);
                            }
                            None => {
                                self.recortes_de_imagem.remove(&item.image);
                            }
                        }
                        drawn?;
                    }
                }
                Ok(())
            })();
            self.clip = previous_clip;
            result?;
        }
        if let Some(item) = state.selected.and_then(|index| state.items.get(index)) {
            let label = Rect {
                x: rect.x,
                y: label_y.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
                width: rect.width,
                height: label_height as i16,
            };
            if let Some(clip) = self.clip_rect(label) {
                self.clip = Some(clip);
                if let Some(fb) = self.bitmaps.get_mut(&target) {
                    fb.fill_rect(
                        clip,
                        Rgb::from_rgbval(state.colors[if state.active { 2 } else { 0 }]),
                    );
                }
                let text_width = self.font.as_ref().map_or(0, |font| {
                    font.width(&String::from_utf16_lossy(&item.text), FONT_SIZE) as i32
                });
                let x = i32::from(rect.x) + (i32::from(rect.width) - text_width).max(0) / 2;
                let result = self.legacy_draw_text(
                    &item.text,
                    x,
                    label_y + 2,
                    state.colors[if state.active { 3 } else { 1 }],
                );
                self.clip = previous_clip;
                result?;
            }
        }
        Ok(())
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
            let row_height = if state.kind == LegacyControlKind::IconView {
                FONT_SIZE as i32 + 4
            } else if state.kind == LegacyControlKind::List {
                state
                    .normal_style
                    .row_height()
                    .max(state.selected_style.row_height())
            } else if state.kind.is_menu() {
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
                let underlined = state.kind == LegacyControlKind::Menu
                    && state.properties & MP_UNDERLINE_TITLE != 0;
                if underlined {
                    // MP_UNDERLINE_TITLE é uma decoração do título do menu comum.
                    // A cor e a largura acompanham o texto; clip_rect conserva tanto
                    // o retângulo do controle quanto um clip menor imposto pelo guest.
                    let width = self.font.as_ref().map_or(0, |font| {
                        font.width(&String::from_utf16_lossy(&state.title), FONT_SIZE)
                            .min(i16::MAX as u32) as i32
                    });
                    let rule = Rect {
                        x: x.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
                        y: (y + FONT_SIZE.ceil() as i32 + 1)
                            .clamp(i32::from(i16::MIN), i32::from(i16::MAX))
                            as i16,
                        width: width.clamp(0, i32::from(i16::MAX)) as i16,
                        height: 1,
                    };
                    if let (Some(area), Some(fb)) =
                        (self.clip_rect(rule), self.bitmaps.get_mut(&target))
                    {
                        fb.fill_rect(area, Rgb::from_rgbval(state.colors[8]));
                    }
                }
                y += if underlined {
                    row_height.max(FONT_SIZE.ceil() as i32 + 2)
                } else {
                    row_height
                };
            }
            if state.kind == LegacyControlKind::IconView {
                self.legacy_icon_redraw(&state, target, y)?;
            } else if state.kind.is_menu() {
                // AEEMenu: LISTCTL exibe somente o texto do item selecionado,
                // mesmo quando seu retângulo comporta várias linhas de menu.
                let visible = if state.kind == LegacyControlKind::List {
                    1
                } else {
                    ((i32::from(rect.height) - (y - i32::from(rect.y))) / row_height).max(1)
                        as usize
                };
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
                    self.legacy_menu_frame(target, item_rect, style, state.colors[4]);
                    let content_x = x + border + i32::from(style.x_offset);
                    let content_y = item_y + border + i32::from(style.y_offset);
                    let mut text_x = content_x;
                    if item.image != 0 && state.kind != LegacyControlKind::List {
                        let old_crop = self.recortes_de_imagem.get(&item.image).copied();
                        self.recortes_de_imagem
                            .entry(item.image)
                            .or_default()
                            .transparente = u32::from(style.image_rop) == AEE_RO_TRANSPARENT;
                        let drawn = if style.image_rop == 0 {
                            self.legacy_or_image(item.image, content_x, content_y)
                        } else {
                            self.draw_image(item.image, content_x, content_y, Some(0))
                        };
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
                if !state.valid_multitap() {
                    state.multitap = None;
                }
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
                || !TEXT_MODES.contains(&state.input_mode)
                || state.mode_enum_index > TEXT_MODES.len()
                || !state.valid_multitap()
                || state.normal_style.frame_width().is_none()
                || state.selected_style.frame_width().is_none()
                || ![0, 2, 7].contains(&state.normal_style.image_rop)
                || ![0, 2, 7].contains(&state.selected_style.image_rop)
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
