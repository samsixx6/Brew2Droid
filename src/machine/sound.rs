//! ISound e ISoundPlayer: a saída de áudio de baixo nível.

use super::*;

impl<C: CpuBackend> Machine<C> {
    /// Liga a saída de som. Sem ela o emulador roda igual, mudo.
    pub fn set_audio(&mut self, mixer: Option<crate::audio::Mixer>) {
        self.audio = mixer;
    }

    pub(super) fn sound_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let Some(name) = Interface::Sound.method(slot) else {
            return Ok(None);
        };
        let this = self.cpu.read_reg(Reg::R0);
        let (a1, a2) = (self.cpu.read_reg(Reg::R1), self.cpu.read_reg(Reg::R2));
        let result = match name {
            "AddRef" => self.objects.add_ref(this),
            "Release" => {
                let remaining = self.objects.release(this);
                if remaining == 0 {
                    self.sounds.remove(&this);
                    self.avisos_de_midia
                        .retain(|(owner, _, _, _)| *owner != this);
                }
                remaining
            }
            // void RegisterNotify(ISound *, PFNSOUNDSTATUS pfn, const void *pUser)
            "RegisterNotify" => {
                self.avisos_de_midia
                    .retain(|(owner, _, _, _)| *owner != this);
                self.sounds.entry(this).or_default().notify = Callback {
                    function: a1,
                    context: a2,
                };
                SUCCESS
            }
            // int Set(ISound *, const AEESoundInfo *) — cinco `int8`: eDevice, eMethod, eAPath,
            // eEarMuteCtl, eMicMuteCtl.
            "Set" => {
                if a1 != 0 {
                    let mut info = [0u8; 5];
                    self.cpu.read_mem(a1, &mut info)?;
                    self.sounds.entry(this).or_default().info = info;
                }
                SUCCESS
            }
            "Get" => {
                if a1 != 0 {
                    let info = self.sounds.entry(this).or_default().info;
                    self.cpu.write_mem(a1, &info)?;
                }
                SUCCESS
            }
            // ROTK espera a conclusão assíncrona para continuar a inicialização.
            // Confirmar o dispositivo virtual exige STATUS_CB/SUCCESS, além do retorno.
            "SetDevice" => {
                self.complete_sound_command(this);
                SUCCESS
            }
            // Tocar é instantâneo e mudo, mas o jogo costuma esperar o callback de conclusão
            // antes de liberar o recurso de áudio.
            "PlayTone" | "PlayToneList" | "PlayFreqTone" => {
                self.finish_sound(this, AEE_SOUND_STATUS_CB, AEE_SOUND_PLAY_DONE);
                SUCCESS
            }
            "Vibrate" => {
                self.finish_sound(this, AEE_SOUND_STATUS_CB, AEE_SOUND_PLAY_DONE);
                SUCCESS
            }
            "StopTone" | "StopVibrate" => SUCCESS,
            "SetVolume" => {
                self.sounds.entry(this).or_default().volume = (a1 as u16).min(AEE_MAX_VOLUME);
                self.complete_sound_command(this);
                SUCCESS
            }
            // `GetVolume` devolve `void`: o valor chega ao jogo pelo callback, com
            // `AEE_SOUND_VOLUME_CB` e o volume em `dwParam`.
            "GetVolume" => {
                let volume = self.sounds.entry(this).or_default().volume;
                if self.objects.kind_of(this) == Some(Interface::SoundLegacy) {
                    let notify = self.sounds[&this].notify;
                    if notify.function != 0 {
                        // O callback 0x8b168 do BIA lê uint16 em R3, como o tocador legado.
                        self.avisos_de_midia.push((
                            this,
                            MEDIA_LEGACY_COMMAND | MEDIA_LEGACY_VALUE | 1,
                            u32::from(volume),
                            notify,
                        ));
                    }
                } else {
                    self.finish_sound_with(
                        this,
                        AEE_SOUND_VOLUME_CB,
                        AEE_SOUND_SUCCESS,
                        volume as u32,
                    );
                }
                SUCCESS
            }
            // Não há disputa por recurso de áudio num emulador de um jogo só.
            "GetResourceCtl" => {
                if a1 != 0 {
                    self.cpu.write_u32(a1, 0)?;
                }
                ECLASSNOTSUPPORT
            }
            _ => return Ok(None),
        };
        Ok(Some(result))
    }

    /// A fila de mídia só entrega depois que o tratador devolve o controle; pending_calls
    /// roda entre APIs e avisaria antes de ROTK gravar o estado que aguarda a conclusão.
    fn complete_sound_command(&mut self, sound: u32) {
        if let Some(state) = self.sounds.get(&sound) {
            if state.notify.function != 0 {
                self.avisos_de_midia.push((
                    sound,
                    MEDIA_LEGACY_COMMAND | AEE_SOUND_STATUS_CB,
                    AEE_SOUND_SUCCESS,
                    state.notify,
                ));
            }
        }
    }

    /// Enfileira o callback de `ISound` sem parâmetro extra.
    pub(super) fn finish_sound(&mut self, sound: u32, kind: u32, status: u32) {
        self.finish_sound_with(sound, kind, status, 0);
    }

    /// Enfileira o `PFNSOUNDSTATUS(pUser, eCBType, eSPStatus, dwParam)` do som.
    pub(super) fn finish_sound_with(&mut self, sound: u32, kind: u32, status: u32, param: u32) {
        let Some(state) = self.sounds.get(&sound) else {
            return;
        };
        if state.notify.function == 0 {
            return;
        }
        let notify = state.notify;
        // O aviso conserva o dono para Release/RegisterNotify cancelarem a entrega.
        // Undercover carrega imagens neste callback, que precisa sobreviver a Budget.
        self.avisos_de_midia.push((sound, MEDIA_NATIVE_SOUND_COMMAND | kind,
            if kind == AEE_SOUND_VOLUME_CB { param } else { status }, notify));
    }
}
