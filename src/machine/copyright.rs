//! ISHELL_ShowCopyright é a forma Prompt(NULL), não uma resposta automática ao applet.

use super::*;
use serde::{Deserialize, Serialize};

pub(super) const COPYRIGHT_END: u32 = 0x303;

pub(super) enum CopyrightInput {
    Pass,
    Consume,
    End,
}

#[derive(Serialize, Deserialize)]
pub(super) struct CopyrightModal {
    previous_screen: Vec<u16>,
}

#[derive(Serialize, Deserialize)]
struct SavedCopyright {
    version: u32,
    modal: Option<CopyrightModal>,
    held: BTreeSet<u16>,
}

impl<C: CpuBackend> Machine<C> {
    /// O launcher já escolheu este MIF; não procurar outro arquivo pela coleção.
    pub fn set_applet_mif(&mut self, bytes: &[u8], class: u32) -> bool {
        self.copyright_text.clear();
        self.applet_manifest = None;
        if bytes.len() < 32 || bytes.len() > 2 * 1024 * 1024 {
            return false;
        }
        let word = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        if word(20) > 4096 || u16::from_le_bytes([bytes[6], bytes[7]]) > 4096 {
            return false;
        }
        let Ok(resources) = crate::loader::resfile::ResFile::parse(bytes.to_vec()) else {
            return false;
        };
        let Ok(mif) = crate::loader::miffile::MifFile::parse(bytes) else {
            return false;
        };
        if !mif.applets.contains(&class) {
            return false;
        }
        self.applet_manifest = Some((bytes.to_vec(), 0));
        let base = mif.sections.iter().find_map(|&(offset, length)| {
            let at = offset as usize;
            (length == 20 && bytes.get(at..at + 4) == Some(&class.to_le_bytes()))
                .then(|| u16::from_le_bytes([bytes[at + 8], bytes[at + 9]]))
        });
        // AEEShell.h: nome em wIDBase+0; IDS_MIF_COMPANY/COPYRIGHT/VERSION são 6/7/8.
        for id in base.into_iter().chain([6, 7, 8]) {
            if let Some(text) = resources.string(id) {
                let text =
                    String::from_utf16_lossy(&text.into_iter().take(128).collect::<Vec<_>>());
                if !text.is_empty() && !self.copyright_text.contains(&text) {
                    self.copyright_text.push(text);
                }
            }
        }
        true
    }

    /// O nome vem do par escolhido pelo launcher; não procurar outro MIF na coleção.
    pub fn set_applet_mif_named(&mut self, bytes: &[u8], class: u32, name: &str) -> bool {
        if !self.set_applet_mif(bytes, class) {
            return false;
        }
        if name.len() > 4096 || name.contains('\0') {
            return false;
        }
        let Ok(pointer) = self.malloc(name.len() as u32 + 1) else {
            return false;
        };
        if pointer == 0
            || self
                .cpu
                .write_mem(pointer, format!("{name}\0").as_bytes())
                .is_err()
        {
            return false;
        }
        if let Some((_, stored)) = &mut self.applet_manifest {
            *stored = pointer;
        }
        true
    }

    pub(super) fn shell_copyright(&mut self) -> Result<u32, CpuError> {
        if self.cpu.read_reg(Reg::R1) != 0 || self.copyright_modal.is_some() {
            return Ok(FALSE);
        }
        let device = self.device_bitmap()?;
        let previous_screen = self.screen().pixels().to_vec();
        let (old_target, old_clip, old_color) =
            (self.display_target, self.clip, self.colors[CLR_USER_TEXT]);
        self.display_target = 0;
        self.clip = None;
        self.colors[CLR_USER_TEXT] = Rgb::BLACK;
        if self.font.is_none() {
            self.font = egui::FontDefinitions::default()
                .font_data
                .get("Ubuntu-Light")
                .and_then(|data| {
                    crate::video::font::Font::load(data.font.to_vec(), "Ubuntu-Light".into())
                });
        }
        let result = (|| {
            let (width, height) = (self.screen().width(), self.screen().height());
            self.bitmaps.get_mut(&device).unwrap().fill_rect(
                Rect {
                    x: 0,
                    y: 0,
                    width: width as i16,
                    height: height as i16,
                },
                Rgb::WHITE,
            );
            self.draw_text("Copyright", 8, 6)?;
            let columns = ((width.saturating_sub(16)) / 8).max(1) as usize;
            let footer_y = height.saturating_sub(FONT_SIZE as u32 + 10) as i32;
            let mut y = FONT_SIZE as i32 + 14;
            for text in self.copyright_text.clone() {
                let characters = text.chars().collect::<Vec<_>>();
                for line in characters.chunks(columns) {
                    if y + FONT_SIZE as i32 >= footer_y {
                        break;
                    }
                    self.draw_text(&line.iter().collect::<String>(), 8, y)?;
                    y += FONT_SIZE as i32 + 3;
                }
            }
            self.draw_text("C / Clear", 8, footer_y)?;
            self.guarda_quadro_do_update()?;
            Ok(())
        })();
        self.display_target = old_target;
        self.clip = old_clip;
        self.colors[CLR_USER_TEXT] = old_color;
        if let Err(error) = result {
            self.bitmaps
                .get_mut(&device)
                .unwrap()
                .load_rgb565_words(&previous_screen);
            return Err(error);
        }
        self.copyright_modal = Some(CopyrightModal { previous_screen });
        Ok(TRUE)
    }

    /// O fim é devolvido ao ponto de entrada do host para conservar seu orçamento/continuação.
    pub(super) fn copyright_input(
        &mut self,
        event: u32,
        key: u16,
    ) -> Result<CopyrightInput, CpuError> {
        let active = self.copyright_modal.is_some();
        if matches!(event, 0x100..=0x102) {
            let held = self.copyright_keys.contains(&key);
            if event == 0x102 {
                self.copyright_keys.remove(&key);
                if active || held {
                    return Ok(CopyrightInput::Consume);
                }
            } else if active {
                self.copyright_keys.insert(key);
                // AEEShell.h: ShowCopyright só termina no CLR. Press/Release não duplicam o fim.
                if event == 0x100 && key == crate::handset::input::Key::Clear.code() {
                    let modal = self.copyright_modal.take().unwrap();
                    if let Some(screen) = self.bitmaps.get_mut(&self.device_bitmap) {
                        screen.load_rgb565_words(&modal.previous_screen);
                    } else {
                        self.screen.load_rgb565_words(&modal.previous_screen);
                    }
                    let old_target = self.display_target;
                    self.display_target = 0;
                    let published = self.guarda_quadro_do_update();
                    self.display_target = old_target;
                    published?;
                    return Ok(CopyrightInput::End);
                }
                return Ok(CopyrightInput::Consume);
            } else if held {
                return Ok(CopyrightInput::Consume);
            }
        }
        Ok(if active {
            CopyrightInput::Consume
        } else {
            CopyrightInput::Pass
        })
    }

    pub(super) fn grava_copyright(&self, sections: &mut crate::save_state::Secoes) {
        #[derive(Serialize)]
        struct Saved<'a> {
            version: u32,
            modal: &'a Option<CopyrightModal>,
            held: &'a BTreeSet<u16>,
        }
        sections.poe(
            "tab.copyright",
            serde_json::to_vec(&Saved {
                version: 1,
                modal: &self.copyright_modal,
                held: &self.copyright_keys,
            })
            .expect("modal contém apenas valores serializáveis"),
        );
    }

    pub(super) fn restaura_copyright(
        &mut self,
        reader: &crate::save_state::Leitor<'_>,
    ) -> Result<(), crate::save_state::Erro> {
        let invalid = || crate::save_state::Erro::Secao {
            nome: "tab.copyright".into(),
            motivo: "estado de copyright inválido".into(),
        };
        let Some(bytes) = reader.secao("tab.copyright") else {
            self.copyright_modal = None;
            self.copyright_keys.clear();
            return Ok(());
        };
        if bytes.len() > 32 * 1024 * 1024 {
            return Err(invalid());
        }
        let saved: SavedCopyright = serde_json::from_slice(bytes).map_err(|_| invalid())?;
        if saved.version != 1
            || saved.held.len() > 256
            || saved
                .modal
                .as_ref()
                .is_some_and(|modal| modal.previous_screen.len() != self.screen().pixels().len())
        {
            return Err(invalid());
        }
        self.copyright_modal = saved.modal;
        self.copyright_keys = saved.held;
        Ok(())
    }
}
