//! O frontend conserva o caminho simples quando o pacote não declara um applet único verificável.
use std::ops::{Deref, DerefMut};
use std::time::Duration;
use zeebx::session::handset_runtime::HandsetRuntime;
use zeebx::session::{Session as CoreSession, Step};
use zeebx::velocidade::Ritmo;
pub enum Session {
    Single(CoreSession),
    Installed(HandsetRuntime),
}
impl Deref for Session {
    type Target = CoreSession;
    fn deref(&self) -> &CoreSession {
        match self {
            Self::Single(s) => s,
            Self::Installed(s) => s,
        }
    }
}
impl DerefMut for Session {
    fn deref_mut(&mut self) -> &mut CoreSession {
        match self {
            Self::Single(s) => s,
            Self::Installed(s) => s,
        }
    }
}
impl Session {
    pub fn installed(&self) -> Option<&HandsetRuntime> {
        match self {
            Self::Installed(s) => Some(s),
            _ => None,
        }
    }
    pub fn installed_mut(&mut self) -> Option<&mut HandsetRuntime> {
        match self {
            Self::Installed(s) => Some(s),
            _ => None,
        }
    }
    pub fn anda(&mut self, budget: Duration, rhythm: &Ritmo) -> Step {
        match self {
            Self::Single(s) => s.anda(budget, rhythm),
            Self::Installed(s) => s.anda(budget, rhythm),
        }
    }
    pub fn congela_o_som(&mut self, paused: bool) {
        match self {
            Self::Single(s) => s.congela_o_som(paused),
            Self::Installed(s) => s.congela_o_som(paused),
        }
    }
    pub fn next_guest_work_delay(&self) -> Option<Duration> {
        match self {
            Self::Single(s) => s.next_guest_work_delay(),
            Self::Installed(s) => s.next_guest_work_delay(),
        }
    }
    pub fn pode_salvar(&mut self) -> Result<(), String> {
        match self {
            Self::Single(s) => s.pode_salvar(),
            Self::Installed(s) => s.pode_salvar(),
        }
    }
    pub fn restaura_estado(&mut self, bytes: &[u8]) -> Result<(), zeebx::save_state::Erro> {
        match self {
            Self::Single(s) => s.restaura_estado(bytes),
            Self::Installed(s) => s.restaura_estado(bytes),
        }
    }
    pub fn stopped_reason(&self) -> Option<String> {
        match self {
            Self::Single(s) => s.stopped_reason(),
            Self::Installed(s) => s.stopped_reason(),
        }
    }
    pub fn saiu_normalmente(&self) -> bool {
        match self {
            Self::Single(s) => s.saiu_normalmente(),
            Self::Installed(s) => s.saiu_normalmente(),
        }
    }
    /// A solicitação do proprietário de background pode apenas notificar o app já visível.
    /// Esse caminho não toca em perfis, arquivos, controles, estados, FPS ou streams de áudio.
    pub fn notify_top_launch(&mut self) -> Result<bool, String> {
        let Some(runtime) = self.installed_mut() else {
            return Ok(false);
        };
        if runtime
            .pending_handoff()
            .is_some_and(|request| runtime.is_live_foreground(&request.target))
        {
            runtime.begin_handoff(None)?;
            return Ok(true);
        }
        Ok(false)
    }
    pub fn saiu_sozinho(&self) -> bool {
        match self {
            Self::Single(s) => s.saiu_sozinho(),
            Self::Installed(s) => s.saiu_sozinho(),
        }
    }
}
