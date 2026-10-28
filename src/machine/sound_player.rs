//! ISoundPlayer móvel usa o mesmo cache/mixer do IMedia, com ABI e notificações legadas.
use super::*;

// AEEError.h: códigos de contexto retornados por GetInfo(NULL), não estados IMedia.
const EIDLE: u32 = 31;
const EITEMBUSY: u32 = 32;

impl<C: CpuBackend> Machine<C> {
    pub(super) fn sound_player_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let Some(name) = Interface::SoundPlayer.method(slot) else {
            return Ok(None);
        };
        let this = self.arg(0);
        let a1 = self.arg(1);
        let a2 = self.arg(2);
        let result = match name {
            "AddRef" => self.objects.add_ref(this),
            "Release" => {
                if self.objects.contagem(this) == 1 {
                    self.avisos_de_midia.retain(|notice| notice.0 != this);
                }
                self.media_call_inner(Interface::Media, "Release", this)?
                    .unwrap_or(EFAILED)
            }
            "RegisterNotify" => {
                self.avisos_de_midia.retain(|notice| notice.0 != this);
                self.media.entry(this).or_default().notify = Callback {
                    function: a1,
                    context: a2,
                };
                SUCCESS
            }
            "SetStream" => self.sound_player_stream(this, a1)?,
            "Set" => self.sound_player_source(this, a1, a2, 0)?,
            "SetInfo" if a1 != 0 => {
                // No SDK ARM, AEESoundPlayerInput é int8; os bytes 1..3 são padding.
                // Asphalt 2 deixa lixo ali, que não pode virar parte do tipo de fonte.
                let mut input = [0u8; 1];
                self.cpu.read_mem(a1, &mut input)?;
                let input = u32::from(input[0]);
                let data = self.cpu.read_u32(a1 + 4)?;
                let size = self.cpu.read_u32(a1 + 8)?;
                self.anota_midia(
                    this,
                    format!("ISoundPlayer::SetInfo input={input} data={data:#x} bytes={size}"),
                );
                self.sound_player_source(this, input, data, size)?
            }
            "Play" => self.media_play(this)?,
            "Stop" => {
                self.stop_sound_player(this);
                SUCCESS
            }
            "Pause" | "Resume" => self
                .media_call_inner(Interface::Media, name, this)?
                .unwrap_or(EFAILED),
            "SetVolume" => {
                let state = self.media.entry(this).or_default();
                state.volume = a1.min(MAX_VOLUME);
                if let Some(mixer) = &self.audio {
                    mixer.set_volume(this, state.gain());
                }
                SUCCESS
            }
            "GetVolume" => {
                let state = self.media.entry(this).or_default();
                // BIA 0x8b188 cobra cmd4, status1 e uint16 no ponteiro R3.
                // R1 não é saída: o chamador deixa ali o endereço de um trampolim.
                if state.notify.function != 0 {
                    self.avisos_de_midia.push((
                        this,
                        MEDIA_LEGACY_COMMAND | MEDIA_LEGACY_VALUE | 4,
                        state.volume,
                        state.notify,
                    ));
                }
                SUCCESS
            }
            "GetInfo" => {
                let state = self.media.entry(this).or_default();
                if a1 == 0 {
                    // Asphalt 2 128x146 só entrega o áudio quando esta consulta devolve EIDLE.
                    // NULL é consulta de estado documentada desde BREW 1.1.
                    if matches!(state.state, MM_STATE_PLAY | MM_STATE_PLAY_PAUSE) {
                        EITEMBUSY
                    } else {
                        EIDLE
                    }
                } else if state.buffer.1 == 0 {
                    EUNSUPPORTED
                } else {
                    let (data, size) = state.buffer;
                    self.cpu.write_mem(a1, &[2])?;
                    for (offset, value) in [(4, data), (8, size)] {
                        self.cpu.write_u32(a1 + offset, value)?;
                    }
                    SUCCESS
                }
            }
            _ => EUNSUPPORTED,
        };
        self.anota_midia(
            this,
            format!("ISoundPlayer::{name}({a1:#x},{a2:#x}) -> {result:#x}"),
        );
        Ok(Some(result))
    }

    /// AEESoundPlayer.h: consumir a fonte e avisar STATUS_CB depois do retorno.
    /// Fast & Furious 4/6 passam ISource/IPeek criados por ISourceUtil, não SetInfo.
    fn sound_player_stream(&mut self, this: u32, source: u32) -> Result<u32, CpuError> {
        const LIMIT: usize = 4 * 1024 * 1024;
        let busy = self
            .media
            .get(&this)
            .is_some_and(|s| matches!(s.state, MM_STATE_PLAY | MM_STATE_PLAY_PAUSE));
        let bytes = if busy {
            None
        } else if let Some(bytes) = self.sources.get_mut(&source) {
            (bytes.len() <= LIMIT).then(|| std::mem::take(bytes))
        } else if let Some(peek) = self.peeks.get_mut(&source) {
            let count = peek.bytes.len().saturating_sub(peek.posicao);
            if count > LIMIT {
                None
            } else {
                let bytes = peek.bytes[peek.posicao..].to_vec();
                peek.posicao = peek.bytes.len();
                Some(bytes)
            }
        } else if let Some(stream) = self.streams.get(&source).copied() {
            let count = stream.size.saturating_sub(stream.position);
            if count as usize > LIMIT {
                None
            } else {
                let bytes = self.read_bytes(stream.buffer + stream.position, count)?;
                self.streams.get_mut(&source).unwrap().position = stream.size;
                Some(bytes)
            }
        } else if let Some(open) = self.open_files.get_mut(&source) {
            use std::io::Read;
            let mut bytes = Vec::new();
            match open
                .file
                .by_ref()
                .take(LIMIT as u64 + 1)
                .read_to_end(&mut bytes)
            {
                Ok(_) => (bytes.len() <= LIMIT).then_some(bytes),
                Err(_) => None,
            }
        } else {
            None
        };
        let success = bytes.is_some();
        if let Some(bytes) = bytes {
            self.stop_sound_player(this);
            let carga = self.guarda_som(bytes);
            let state = self.media.entry(this).or_default();
            state.carga = carga;
            state.pendente = (0, 0);
            state.buffer = (0, 0);
        }
        if let Some(state) = self.media.get(&this)
            && state.notify.function != 0
        {
            self.avisos_de_midia.push((
                this,
                MEDIA_LEGACY_COMMAND,
                if success { 1 } else { 15 },
                state.notify,
            ));
        }
        Ok(if success { SUCCESS } else { EFAILED })
    }

    fn stop_sound_player(&mut self, this: u32) {
        let active = self
            .media
            .get(&this)
            .is_some_and(|state| matches!(state.state, MM_STATE_PLAY | MM_STATE_PLAY_PAUSE));
        if let Some(mixer) = &self.audio {
            mixer.stop(this);
        }
        if let Some(state) = self.media.get_mut(&this) {
            state.state = MM_STATE_READY;
            state.ends_us = 0;
            state.tocar_ao_ler = false;
            if active && state.notify.function != 0 {
                self.avisos_de_midia
                    .push((this, MEDIA_LEGACY_COMMAND | 1, 12, state.notify));
            }
        }
    }

    fn sound_player_source(
        &mut self,
        this: u32,
        input: u32,
        data: u32,
        mut size: u32,
    ) -> Result<u32, CpuError> {
        if input == 0 {
            self.stop_sound_player(this);
            self.media.entry(this).or_default().carga = 0;
            return Ok(SUCCESS);
        }
        if data == 0 || !matches!(input, 1 | 2) {
            return Ok(EBADPARM);
        }
        self.stop_sound_player(this);
        if input == 1 {
            let name = self.cpu.read_cstring(data, MAX_STRING);
            let Some(bytes) = self
                .vfs
                .resolve(&name)
                .and_then(|path| std::fs::read(path).ok())
            else {
                self.anota_ponto_ruim(format!("ISoundPlayer arquivo ausente: {name}"));
                return Ok(EFAILED);
            };
            let key = self.guarda_som(bytes);
            let state = self.media.entry(this).or_default();
            state.carga = key;
            state.pendente = (0, 0);
            state.buffer = (0, 0);
        } else {
            if size == 0 {
                let header = self.read_bytes(data, 12)?;
                size = if header.starts_with(b"RIFF") {
                    u32::from_le_bytes(header[4..8].try_into().unwrap())
                        .checked_add(8)
                        .unwrap_or(0)
                } else if header.starts_with(b"cmid") {
                    u32::from_be_bytes(header[4..8].try_into().unwrap())
                        .checked_add(8)
                        .unwrap_or(0)
                } else if header.starts_with(b"MThd") {
                    let count = u16::from_be_bytes(header[10..12].try_into().unwrap());
                    let mut at =
                        8u32.saturating_add(u32::from_be_bytes(header[4..8].try_into().unwrap()));
                    if count > 32 || at > 4 * 1024 * 1024 {
                        return Ok(EBADPARM);
                    }
                    for _ in 0..count {
                        let Some(address) = data.checked_add(at) else {
                            return Ok(EBADPARM);
                        };
                        let track = self.read_bytes(address, 8)?;
                        if !track.starts_with(b"MTrk") {
                            return Ok(EBADPARM);
                        }
                        at = at
                            .saturating_add(8)
                            .saturating_add(u32::from_be_bytes(track[4..8].try_into().unwrap()));
                        if at > 4 * 1024 * 1024 {
                            return Ok(EBADPARM);
                        }
                    }
                    at
                } else {
                    return Ok(EUNSUPPORTED);
                };
            }
            if size == 0 || size > 4 * 1024 * 1024 {
                return Ok(EBADPARM);
            }
            let state = self.media.entry(this).or_default();
            state.carga = 0;
            state.pendente = (data, size);
            state.buffer = (data, size);
        }
        Ok(SUCCESS)
    }
}
