//! Applets móveis instalados: cada proprietário conserva CPU, temporizadores, som e VFS.
use super::*;
use crate::handset::installed::{InstalledApplet, InstalledCatalog};
use crate::machine::app_history::{History, SharedHistory};
use crate::machine::applet_lifecycle::LifecycleCall;
use std::ops::{Deref, DerefMut};

const MAX_OWNERS: usize = 8;
pub struct HandoffRequest {
    pub source: u32,
    pub target: InstalledApplet,
    pub resume: bool,
}
struct Retained {
    session: Session,
    entry: InstalledApplet,
    background: bool,
}
enum Phase {
    Suspend,
    Resume,
}
struct Transition {
    prepared: Option<Session>,
    resume: Option<usize>,
    call: Option<LifecycleCall>,
    phase: Phase,
}
pub struct HandsetRuntime {
    foreground: Session,
    entry: InstalledApplet,
    retained: Vec<Retained>,
    catalog: InstalledCatalog,
    request: Option<HandoffRequest>,
    transition: Option<Transition>,
    history: History,
    error: Option<String>,
    failed: bool,
    launch_failure: Option<String>,
    paused: bool,
}
impl Deref for HandsetRuntime {
    type Target = Session;
    fn deref(&self) -> &Session {
        &self.foreground
    }
}
impl DerefMut for HandsetRuntime {
    fn deref_mut(&mut self) -> &mut Session {
        &mut self.foreground
    }
}
impl HandsetRuntime {
    pub fn new(
        mut foreground: Session,
        entry: InstalledApplet,
        catalog: InstalledCatalog,
    ) -> Result<Self, String> {
        Self::matches(&foreground, &entry)?;
        foreground.set_installed_applets(catalog.shell_entries());
        let history = std::sync::Arc::new(std::sync::Mutex::new(SharedHistory {
            classes: vec![entry.class],
        }));
        foreground.machine.set_app_history(history.clone());
        Ok(Self {
            foreground,
            entry,
            retained: Vec::new(),
            catalog,
            request: None,
            transition: None,
            history,
            error: None,
            failed: false,
            launch_failure: None,
            paused: false,
        })
    }
    fn matches(session: &Session, entry: &InstalledApplet) -> Result<(), String> {
        if !session.is_handset()
            || session.classe() != entry.class
            || session.handset_identity.as_ref()
                != Some(&(entry.module_sha256.clone(), entry.mif_sha256.clone()))
        {
            return Err(
                "Prepared applet does not match the installed module/manifest identity".into(),
            );
        }
        Ok(())
    }
    fn sync_history(&mut self) {
        let mut classes = Vec::new();
        if self.foreground.stopped.is_none() && !self.foreground.machine.applet_em_background() {
            classes.push(self.entry.class);
        }
        classes.extend(
            self.retained
                .iter()
                .rev()
                .filter(|r| !r.background)
                .map(|r| r.entry.class),
        );
        self.history
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .classes = classes;
    }
    fn fail(&mut self, error: impl ToString) {
        self.failed = true;
        self.error = Some(error.to_string());
    }
    pub fn anda(&mut self, budget: Duration, ritmo: &Ritmo) -> Step {
        if self.paused {
            return Step::Ahead;
        }
        if self.failed {
            return Step::Stopped;
        }
        if self.transition.is_some() {
            return self.poll_transition();
        }
        let mut foreground_step = if self.request.is_some() {
            Step::Ahead
        } else {
            self.foreground.anda(budget, ritmo)
        };
        if self.foreground.machine.takes_return_to_idle()
            && !(self.retained.is_empty()
                && foreground_step == Step::Stopped
                && self.foreground.saiu_normalmente())
        {
            self.fail("Return-to-idle with live installed owners is not supported");
            return Step::Stopped;
        }
        self.sync_history();
        // Somente os proprietários que escolheram STOP FALSE continuam executando.
        let mut index = 0;
        while index < self.retained.len() {
            if !self.retained[index].background {
                index += 1;
                continue;
            }
            let step = self.retained[index].session.anda(Duration::ZERO, ritmo);
            if self.retained[index].session.machine.takes_return_to_idle() {
                self.fail("Background applet requested return-to-idle cleanup");
                return Step::Stopped;
            }
            if step == Step::Stopped {
                if !self.retained[index].session.saiu_normalmente() {
                    self.fail(format!(
                        "Background applet {:08x} failed: {}",
                        self.retained[index].entry.class,
                        self.retained[index]
                            .session
                            .stopped_reason()
                            .unwrap_or_default()
                    ));
                    return Step::Stopped;
                }
                self.retained.remove(index);
                continue;
            }
            index += 1;
        }
        self.sync_history();
        if self.request.is_none() {
            let launch = if self.foreground.stopped.is_none()
                && (!self.foreground.machine.handset_event_ready()
                    || self.foreground.machine.pediu_para_fechar())
            {
                None
            } else {
                self.foreground
                    .take_launch_request()
                    .map(|class| (self.entry.class, class))
                    .or_else(|| {
                        self.retained.iter_mut().find_map(|r| {
                            r.session
                                .take_launch_request()
                                .map(|class| (r.entry.class, class))
                        })
                    })
            };
            if let Some((source, class)) = launch {
                let resolved = if class == self.entry.class && self.foreground.stopped.is_none() {
                    Ok(self.entry.clone())
                } else {
                    self.catalog.resolve(class)
                };
                match resolved {
                    Ok(target) => {
                        let resume = (class == self.entry.class
                            && self.foreground.stopped.is_none()
                            && self.foreground.machine.applet_em_background())
                            || self.retained.iter().any(|r| r.entry.class == class);
                        self.request = Some(HandoffRequest {
                            source,
                            target,
                            resume,
                        });
                    }
                    Err(error) => {
                        self.launch_failure = Some(error);
                    }
                }
            }
        }
        if self.error.is_some() {
            return Step::Stopped;
        }
        if self.request.is_none()
            && (foreground_step == Step::Stopped || self.foreground.machine.applet_em_background())
        {
            if foreground_step == Step::Stopped && !self.foreground.saiu_normalmente() {
                return Step::Stopped;
            }
            if let Some(index) = self.retained.iter().rposition(|r| !r.background) {
                let owner = self.retained.remove(index);
                let mut previous = std::mem::replace(&mut self.foreground, owner.session);
                let entry = std::mem::replace(&mut self.entry, owner.entry);
                if previous.stopped.is_none() {
                    previous.handset_input = Some(Default::default());
                    previous.handset_pen = Default::default();
                    previous.handset_avk = Default::default();
                    self.retained.push(Retained {
                        background: previous.machine.applet_em_background(),
                        session: previous,
                        entry,
                    });
                }
                self.transition = Some(Transition {
                    prepared: None,
                    resume: None,
                    call: None,
                    phase: Phase::Resume,
                });
                foreground_step = self.poll_transition();
            } else if !self.retained.is_empty() {
                foreground_step = Step::Ahead;
            }
        }
        if self.request.is_some() {
            Step::Ahead
        } else {
            foreground_step
        }
    }
    /// Uma preparação recusada não suspende nem destrói a fonte. O frontend apresenta
    /// a falha; não há uma notificação de sucesso inventada para NMASK_SHELL_START_STATUS.
    pub fn reject_handoff(&mut self, error: String) -> Result<(), String> {
        if self.transition.is_some() || self.failed {
            return Err("A running lifecycle cannot be cancelled as an unstarted launch".into());
        }
        let request = self
            .request
            .take()
            .ok_or("No launch preparation is pending")?;
        self.launch_failure = Some(format!(
            "Applet {:08x} could not start {:08x}: {error}",
            request.source, request.target.class
        ));
        Ok(())
    }
    pub fn take_launch_failure(&mut self) -> Option<String> {
        self.launch_failure.take()
    }
    pub fn pending_handoff(&self) -> Option<&HandoffRequest> {
        if self.transition.is_some() {
            None
        } else {
            self.request.as_ref()
        }
    }
    pub fn begin_handoff(&mut self, mut prepared: Option<Session>) -> Result<(), String> {
        if self.transition.is_some() {
            return Err("An applet lifecycle transition is still pending".into());
        }
        let request = self
            .request
            .as_ref()
            .ok_or("No accepted applet launch is pending")?;
        if self.is_live_foreground(&request.target) {
            if prepared.is_some() {
                return Err("Top-visible applet must not be reloaded".into());
            }
            if self.paused {
                return Err("Top-visible notification awaits resume".into());
            }
            Self::matches(&self.foreground, &request.target)?;
            // AEEEvent.h: EVT_APP_WOULD_START, not another START or RESUME.
            let outcome = self
                .foreground
                .machine
                .notify_applet_would_start(500_000)
                .map_err(|e| e.to_string())?;
            if !matches!(outcome, Outcome::Returned { .. } | Outcome::Budget) {
                return Err(format!("WOULD_START failed: {outcome:?}"));
            }
            self.request = None;
            return Ok(());
        }
        if self.matches_current_owner(&request.target)
            && self.foreground.machine.applet_em_background()
        {
            if prepared.is_some() {
                return Err("Background current owner must resume without a fresh session".into());
            }
            Self::matches(&self.foreground, &request.target)?;
            self.transition = Some(Transition {
                prepared: None,
                resume: None,
                call: None,
                phase: Phase::Resume,
            });
            if !self.paused {
                self.poll_transition();
            }
            return self.error.clone().map_or(Ok(()), Err);
        }
        let verified = request.target.verify()?;
        let resume = self
            .retained
            .iter()
            .position(|r| r.entry.class == verified.class);
        if let Some(index) = resume {
            if self.retained[index].entry.module_sha256 != verified.module_sha256
                || self.retained[index].entry.mif_sha256 != verified.mif_sha256
            {
                return Err("Retained installed applet identity changed".into());
            }
            if prepared.is_some() {
                return Err("Retained applet must be resumed without reloading".into());
            }
        } else {
            if self.retained.len() + 1 >= MAX_OWNERS {
                return Err(format!(
                    "Installed applet owner limit ({MAX_OWNERS}) reached"
                ));
            }
            let target = prepared
                .as_mut()
                .ok_or("Launch requires a prepared target session")?;
            Self::matches(target, &verified)?;
            target.set_installed_applets(self.catalog.shell_entries());
            target.machine.set_app_history(self.history.clone());
            target.congela_o_som(self.paused);
        }
        self.transition = Some(Transition {
            prepared,
            resume,
            call: None,
            phase: Phase::Suspend,
        });
        if !self.paused {
            self.poll_transition();
        }
        if let Some(error) = self.error.clone() {
            Err(error)
        } else {
            Ok(())
        }
    }
    fn poll_transition(&mut self) -> Step {
        let Some(mut transition) = self.transition.take() else {
            return Step::Ahead;
        };
        if matches!(transition.phase, Phase::Resume) {
            if transition.call.is_none() {
                self.foreground.intermediario = None;
            }
            self.foreground.machine.promote_applet_for_resume();
            self.sync_history();
        }
        let event = match transition.phase {
            Phase::Suspend => 2,
            Phase::Resume => 3,
        };
        let skip = matches!(transition.phase, Phase::Suspend)
            && (self.foreground.stopped.is_some()
                || self.foreground.machine.applet_em_background());
        if !skip {
            let result = (|| -> Result<bool, String> {
                if transition.call.is_none() {
                    let (outcome, call) = self
                        .foreground
                        .machine
                        .begin_applet_lifecycle(event, 500_000)
                        .map_err(|e| e.to_string())?;
                    transition.call = Some(call);
                    if !matches!(outcome, Outcome::Returned { .. } | Outcome::Budget) {
                        return Err(format!("Lifecycle event {event} failed: {outcome:?}"));
                    }
                } else if !self.foreground.machine.handset_event_ready() {
                    let outcomes = self
                        .foreground
                        .machine
                        .advance(500_000)
                        .map_err(|e| e.to_string())?;
                    if let Some(bad) = outcomes
                        .iter()
                        .find(|o| !matches!(o, Outcome::Returned { .. } | Outcome::Budget))
                    {
                        return Err(format!("Lifecycle event {event} failed: {bad:?}"));
                    }
                }
                if !self.foreground.machine.handset_event_ready() {
                    return Ok(false);
                }
                self.foreground
                    .machine
                    .finish_applet_lifecycle(transition.call.take().unwrap())
                    .map_err(|e| e.to_string())?;
                Ok(true)
            })();
            match result {
                Ok(false) => {
                    self.transition = Some(transition);
                    return Step::Running;
                }
                Err(error) => {
                    self.fail(error);
                    return Step::Stopped;
                }
                Ok(true) => {}
            }
        }
        if matches!(transition.phase, Phase::Resume) {
            self.request = None;
            self.sync_history();
            return Step::Running;
        }
        let target = if let Some(index) = transition.resume {
            let owner = self.retained.remove(index);
            (owner.session, owner.entry, true)
        } else {
            let Some(request) = self.request.as_ref() else {
                self.fail("Launch identity disappeared during suspension");
                return Step::Stopped;
            };
            (
                transition.prepared.take().unwrap(),
                request.target.clone(),
                false,
            )
        };
        let old = std::mem::replace(&mut self.foreground, target.0);
        let entry = std::mem::replace(&mut self.entry, target.1);
        if old.stopped.is_none() {
            let mut old = old;
            old.handset_input = Some(Default::default());
            old.handset_pen = Default::default();
            old.handset_avk = Default::default();
            let background = old.machine.applet_em_background();
            self.retained.push(Retained {
                session: old,
                entry,
                background,
            });
        }
        self.request = None;
        self.sync_history();
        if target.2 {
            self.transition = Some(Transition {
                prepared: None,
                resume: None,
                call: None,
                phase: Phase::Resume,
            });
            return self.poll_transition();
        }
        Step::Running
    }
    pub fn catalog(&self) -> &InstalledCatalog {
        &self.catalog
    }
    pub fn is_transitioning(&self) -> bool {
        self.transition.is_some()
    }
    pub fn stopped_reason(&self) -> Option<String> {
        self.error
            .clone()
            .or_else(|| self.foreground.stopped_reason())
    }
    pub fn saiu_normalmente(&self) -> bool {
        !self.failed && self.foreground.saiu_normalmente()
    }
    pub fn saiu_sozinho(&self) -> bool {
        !self.failed
            && self.retained.is_empty()
            && self.request.is_none()
            && self.transition.is_none()
            && self.foreground.saiu_sozinho()
    }
    /// Consulta somente a identidade canonica ja carregada; nao reabre MOD/MIF.
    /// Um proprietario fechado ou em background exige outro caminho de ativacao.
    pub fn is_live_foreground(&self, target: &InstalledApplet) -> bool {
        self.matches_current_owner(target) && !self.foreground.machine.applet_em_background()
    }
    fn matches_current_owner(&self, target: &InstalledApplet) -> bool {
        self.foreground.stopped.is_none()
            && target.class == self.entry.class
            && target.module == self.entry.module
            && target.mif == self.entry.mif
            && target
                .module_sha256
                .eq_ignore_ascii_case(&self.entry.module_sha256)
            && target
                .mif_sha256
                .eq_ignore_ascii_case(&self.entry.mif_sha256)
    }
    pub fn foreground_entry(&self) -> &InstalledApplet {
        &self.entry
    }
    pub fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }
    pub fn retained_count(&self) -> usize {
        self.retained.len()
    }
    pub fn congela_o_som(&mut self, paused: bool) {
        self.paused = paused;
        self.foreground.congela_o_som(paused);
        for owner in &mut self.retained {
            owner.session.congela_o_som(paused);
        }
        if let Some(prepared) = self
            .transition
            .as_mut()
            .and_then(|transition| transition.prepared.as_mut())
        {
            prepared.congela_o_som(paused);
        }
    }
    pub fn next_guest_work_delay(&self) -> Option<Duration> {
        if self.request.is_some() || self.transition.is_some() {
            return Some(Duration::ZERO);
        }
        std::iter::once(self.foreground.next_guest_work_delay())
            .chain(
                self.retained
                    .iter()
                    .filter(|r| r.background)
                    .map(|r| r.session.next_guest_work_delay()),
            )
            .flatten()
            .min()
    }
    pub fn pode_salvar(&mut self) -> Result<(), String> {
        if !self.retained.is_empty()
            || self.request.is_some()
            || self.transition.is_some()
            || self.foreground.machine.history_in_use()
        {
            return Err("Save states do not yet support installed-app ownership/history".into());
        }
        self.foreground.pode_salvar()
    }
    pub fn restaura_estado(&mut self, bytes: &[u8]) -> Result<(), crate::save_state::Erro> {
        self.pode_salvar()
            .map_err(|motivo| crate::save_state::Erro::Secao {
                nome: "runtime.applet_owners".into(),
                motivo,
            })?;
        self.foreground.restaura_estado(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brew::aee::{self, Interface};
    use crate::cpu::CpuBackend;
    const APP: u32 = loader::HEAP_BASE + 0x10000;
    const TABLE: u32 = APP + 0x100;
    const HANDLER: u32 = loader::MODULE_BASE + 0x100;
    const CALLBACK: u32 = loader::MODULE_BASE + 0x200;
    fn shell(name: &str) -> u32 {
        aee::encode(
            Interface::Shell,
            (0..256)
                .find(|s| Interface::Shell.method(*s) == Some(name))
                .unwrap(),
        )
    }
    fn fixture(
        class: u32,
        background: bool,
    ) -> (Session, InstalledApplet, crate::scratch::TempDir) {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = crate::scratch::TempDir::new(&format!(
            "handoff-runtime-{}-{}-{class}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            u32::from(background)
        ));
        let module = root.join("game.mod");
        let mif = root.join("game.mif");
        std::fs::write(
            &module,
            [0xe3a00000u32, 0xe12fff1e]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let mut bytes = vec![0u8; 60];
        bytes[..4].copy_from_slice(&[17, 0, 1, 0]);
        for (at, v) in [
            (16, 32u32),
            (20, 1),
            (32, 40),
            (36, 60),
            (40, class),
            (48, 1),
        ] {
            bytes[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        std::fs::write(&mif, bytes).unwrap();
        let entry = InstalledApplet::inspect(&module, &mif).unwrap();
        let image =
            crate::loader::modfile::ModImage::parse(std::fs::read(&module).unwrap()).unwrap();
        let storage = StoragePaths::from_root(root.join("runtime"));
        let mut m = Machine::new_with_storage(
            CpuDaSessao::new().unwrap(),
            loader::load_with(&image, &[]).unwrap(),
            root.path(),
            &storage,
            None,
        );
        m.enable_handset_clock();
        assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
        // Conta START/SUSPEND/RESUME separadamente; STOP escreve FALSE somente no fixture background.
        let mut code = vec![
            0xe280c020u32,
            0xe79c2101,
            0xe2822001,
            0xe78c2101,
            0xe3510001,
        ];
        if background {
            code.extend([0x03a02000, 0x05c32000]);
        }
        code.extend([0xe3a00001, 0xe12fff1e]);
        m.cpu_mut()
            .write_mem(
                HANDLER,
                &code
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        m.cpu_mut().write_mem(APP, &[0; 128]).unwrap();
        m.cpu_mut().write_u32(APP, TABLE).unwrap();
        m.cpu_mut().write_u32(TABLE + 8, HANDLER).unwrap();
        m.cpu_mut()
            .write_mem(
                CALLBACK,
                &[0xe5901010u32, 0xe2811001, 0xe5801010, 0xe12fff1e]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        let mut s = super::super::tests::sessao_minima_para_save_state();
        s.machine = m;
        s.handset_identity = Some((entry.module_sha256.clone(), entry.mif_sha256.clone()));
        s.classe = class;
        s.handset_input = Some(Default::default());
        s.partida = Some((APP, class));
        s.started = Instant::now();
        (s, entry, root)
    }
    #[test]
    fn installed_handoff_suspends_then_starts_target_and_resumes_same_source_memory() {
        let (source, a, _ra) = fixture(0x01001234, false);
        let (target, b, _rb) = fixture(0x01005678, false);
        let catalog = InstalledCatalog::new([a.clone(), b.clone()]);
        let mut runtime = HandsetRuntime::new(source, a, catalog).unwrap();
        let rhythm = Ritmo::CONSOLE;
        runtime.anda(Duration::ZERO, &rhythm);
        runtime.foreground.intermediario = Some(Framebuffer::new(
            runtime.screen().width(),
            runtime.screen().height(),
        ));
        assert_eq!(
            runtime
                .machine
                .call_guest(
                    shell("StartApplet"),
                    [loader::OBJECT_BASE, b.class, 0, 0],
                    1000
                )
                .unwrap(),
            Outcome::Returned { code: 0 }
        );
        runtime.anda(Duration::ZERO, &rhythm);
        assert_eq!(
            runtime.pending_handoff().map(|r| r.target.class),
            Some(b.class)
        );
        runtime.begin_handoff(Some(target)).unwrap();
        runtime.anda(Duration::ZERO, &rhythm);
        assert_eq!(runtime.classe(), b.class);
        assert_eq!(runtime.retained_count(), 1);
        assert_eq!(
            runtime.retained[0]
                .session
                .machine
                .cpu()
                .read_u32(APP + 40)
                .unwrap(),
            1,
            "SUSPEND before target START"
        );
        runtime.retained[0]
            .session
            .machine
            .cpu_mut()
            .write_u32(APP + 20, 0x12345678)
            .unwrap();
        runtime
            .machine
            .call_guest(shell("CloseApplet"), [loader::OBJECT_BASE, 0, 0, 0], 1000)
            .unwrap();
        for _ in 0..4 {
            runtime.anda(Duration::ZERO, &rhythm);
        }
        assert_eq!(runtime.classe(), 0x01001234);
        assert!(
            runtime.foreground.intermediario.is_none(),
            "RESUME cannot retain the suspended framebuffer snapshot over the SDK display clear"
        );
        assert_eq!(
            runtime.machine.cpu().read_u32(APP + 20).unwrap(),
            0x12345678
        );
        assert_eq!(
            runtime.machine.cpu().read_u32(APP + 44).unwrap(),
            1,
            "RESUME must retain source machine"
        );
    }
    #[test]
    fn installed_handoff_keeps_background_timer_and_pause_clock_owner() {
        let (source, a, _ra) = fixture(0x01001235, true);
        let (target, b, _rb) = fixture(0x01005679, false);
        let mut runtime =
            HandsetRuntime::new(source, a.clone(), InstalledCatalog::new([a, b.clone()])).unwrap();
        let rhythm = Ritmo::CONSOLE;
        runtime.anda(Duration::ZERO, &rhythm);
        runtime
            .machine
            .call_guest(
                shell("SetTimer"),
                [loader::OBJECT_BASE, 0, CALLBACK, APP],
                1000,
            )
            .unwrap();
        runtime
            .machine
            .call_guest(shell("CloseApplet"), [loader::OBJECT_BASE, 0, 0, 0], 1000)
            .unwrap();
        runtime
            .machine
            .call_guest(
                shell("StartApplet"),
                [loader::OBJECT_BASE, b.class, 0, 0],
                1000,
            )
            .unwrap();
        runtime.anda(Duration::ZERO, &rhythm);
        assert!(runtime.pending_handoff().is_some());
        runtime.begin_handoff(Some(target)).unwrap();
        for _ in 0..3 {
            runtime.anda(Duration::ZERO, &rhythm);
        }
        assert_eq!(runtime.retained_count(), 1);
        assert_eq!(
            runtime.retained[0]
                .session
                .machine
                .cpu()
                .read_u32(APP + 16)
                .unwrap(),
            1,
            "background timer survives handoff"
        );
        assert_eq!(
            runtime.retained[0]
                .session
                .machine
                .cpu()
                .read_u32(APP + 40)
                .unwrap(),
            0,
            "STOP FALSE is already background"
        );
    }
    fn request(runtime: &mut HandsetRuntime, target: u32) {
        assert_eq!(
            runtime
                .machine
                .call_guest(
                    shell("StartApplet"),
                    [loader::OBJECT_BASE, target, 0, 0],
                    1000
                )
                .unwrap(),
            Outcome::Returned { code: 0 }
        );
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        assert!(runtime.pending_handoff().is_some());
    }
    fn background_pair() -> (
        HandsetRuntime,
        crate::scratch::TempDir,
        crate::scratch::TempDir,
    ) {
        let (source, a, ra) = fixture(0x01002201, true);
        let (target, b, rb) = fixture(0x01002202, false);
        let mut runtime =
            HandsetRuntime::new(source, a.clone(), InstalledCatalog::new([a, b.clone()])).unwrap();
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        runtime
            .machine
            .call_guest(shell("CloseApplet"), [loader::OBJECT_BASE, 0, 0, 0], 1000)
            .unwrap();
        request(&mut runtime, b.class);
        runtime.begin_handoff(Some(target)).unwrap();
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        (runtime, ra, rb)
    }
    #[test]
    fn installed_handoff_ticks_background_after_switch_and_freezes_every_clock() {
        let (mut runtime, _ra, _rb) = background_pair();
        runtime.retained[0]
            .session
            .machine
            .call_guest(
                shell("SetTimer"),
                [loader::OBJECT_BASE, 1, CALLBACK, APP],
                1000,
            )
            .unwrap();
        runtime.congela_o_som(true);
        let before = runtime.retained[0].session.machine.clock_us_for_input();
        std::thread::sleep(Duration::from_millis(10));
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        assert_eq!(
            runtime.retained[0].session.machine.clock_us_for_input(),
            before
        );
        assert_eq!(
            runtime.retained[0]
                .session
                .machine
                .cpu()
                .read_u32(APP + 16)
                .unwrap(),
            0
        );
        runtime.congela_o_som(false);
        std::thread::sleep(Duration::from_millis(2));
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        assert_eq!(
            runtime.retained[0]
                .session
                .machine
                .cpu()
                .read_u32(APP + 16)
                .unwrap(),
            1
        );
    }
    #[test]
    fn installed_handoff_waits_for_real_suspend_return_across_budgets() {
        let (source, a, _ra) = fixture(0x01003301, false);
        let (target, b, _rb) = fixture(0x01003302, false);
        let mut runtime = HandsetRuntime::new(
            source,
            a.clone(),
            InstalledCatalog::new([a.clone(), b.clone()]),
        )
        .unwrap();
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        let code = [
            0xe3510002u32,
            0x1a000002,
            0xe59f200c,
            0xe2522001,
            0x1afffffd,
            0xe3a00001,
            0xe12fff1e,
            600000,
        ];
        runtime
            .machine
            .cpu_mut()
            .write_mem(
                HANDLER,
                &code
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        request(&mut runtime, b.class);
        runtime.begin_handoff(Some(target)).unwrap();
        assert_eq!(runtime.classe(), a.class);
        assert!(runtime.is_transitioning());
        assert!(
            runtime.pending_handoff().is_none(),
            "host must not prepare a budgeted transition twice"
        );
        assert_eq!(runtime.retained_count(), 0);
        for _ in 0..10 {
            runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
            if runtime.classe() == b.class {
                break;
            }
        }
        assert_eq!(runtime.classe(), b.class);
        assert_eq!(runtime.retained_count(), 1);
        assert!(!runtime.is_transitioning());
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        assert_eq!(runtime.machine.cpu().read_u32(APP + 32).unwrap(), 1);
    }
    #[test]
    fn installed_handoff_suspend_error_keeps_original_owner_and_reports_failure() {
        let (source, a, _ra) = fixture(0x01004401, false);
        let (target, b, _rb) = fixture(0x01004402, false);
        let mut runtime = HandsetRuntime::new(
            source,
            a.clone(),
            InstalledCatalog::new([a.clone(), b.clone()]),
        )
        .unwrap();
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        let code = [
            0xe3510002u32,
            0x03a0200e,
            0x05832000,
            0xe3a00001,
            0xe12fff1e,
        ];
        runtime
            .machine
            .cpu_mut()
            .write_mem(
                HANDLER,
                &code
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        request(&mut runtime, b.class);
        assert!(runtime.begin_handoff(Some(target)).is_err());
        assert_eq!(runtime.classe(), a.class);
        assert_eq!(runtime.retained_count(), 0);
        assert!(runtime.take_error().unwrap().contains("14"));
        assert_eq!(runtime.anda(Duration::ZERO, &Ritmo::CONSOLE), Step::Stopped);
    }
    #[test]
    fn installed_handoff_preserves_sdk_suspend_and_resume_payloads() {
        let (source, a, _ra) = fixture(0x01005501, false);
        let (target, b, _rb) = fixture(0x01005502, false);
        let mut runtime = HandsetRuntime::new(
            source,
            a.clone(),
            InstalledCatalog::new([a.clone(), b.clone()]),
        )
        .unwrap();
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        let code = [
            0xe3510002u32,
            0x05932004,
            0x05802040,
            0x05932000,
            0x05802054,
            0xe3510003,
            0x05932004,
            0x05802044,
            0x05932008,
            0x05802048,
            0x05932010,
            0x0580204c,
            0x05932014,
            0x05802050,
            0xe3a00001,
            0xe12fff1e,
        ];
        runtime
            .machine
            .cpu_mut()
            .write_mem(
                HANDLER,
                &code
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        let (w, h) = (runtime.screen().width(), runtime.screen().height());
        request(&mut runtime, b.class);
        runtime.begin_handoff(Some(target)).unwrap();
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        runtime
            .machine
            .call_guest(shell("CloseApplet"), [loader::OBJECT_BASE, 0, 0, 0], 1000)
            .unwrap();
        for _ in 0..4 {
            runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        }
        assert_eq!(runtime.classe(), a.class);
        let cpu = runtime.machine.cpu();
        assert_eq!(cpu.read_u32(APP + 64).unwrap(), 1);
        assert_eq!(cpu.read_u32(APP + 84).unwrap(), 0);
        assert_eq!(cpu.read_u32(APP + 68).unwrap(), a.class);
        assert_ne!(cpu.read_u32(APP + 72).unwrap(), 0);
        assert_eq!(cpu.read_u32(APP + 76).unwrap(), (h << 16) | w);
        assert_eq!(cpu.read_u32(APP + 80).unwrap(), 0);
    }
    #[test]
    fn installed_handoff_same_top_notification_does_not_reload_owner_or_read_package() {
        let (mut runtime, _ra, _rb) = background_pair();
        let class = runtime.classe();
        let app = runtime.entry.clone();
        let before = runtime.machine.cpu().read_u32(APP + 32).unwrap();
        runtime
            .machine
            .cpu_mut()
            .write_u32(APP + 20, 0x23456789)
            .unwrap();
        std::fs::remove_file(&app.module).unwrap();
        std::fs::remove_file(&app.mif).unwrap();
        runtime.retained[0]
            .session
            .machine
            .call_guest(
                shell("StartApplet"),
                [loader::OBJECT_BASE, class, 0, 0],
                1000,
            )
            .unwrap();
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        assert_eq!(
            runtime.pending_handoff().map(|r| r.target.class),
            Some(class)
        );
        runtime.begin_handoff(None).unwrap();
        assert_eq!(runtime.retained_count(), 1);
        assert_eq!(
            runtime.machine.cpu().read_u32(APP + 20).unwrap(),
            0x23456789
        );
        assert_eq!(runtime.machine.cpu().read_u32(APP + 32).unwrap(), before);
        assert_eq!(
            runtime.machine.cpu().read_u32(APP + 32 + 0x12 * 4).unwrap(),
            1
        );
    }
    #[test]
    fn installed_handoff_target_close_waits_for_background_owner_real_stop() {
        let (mut runtime, _ra, _rb) = background_pair();
        runtime
            .machine
            .call_guest(shell("CloseApplet"), [loader::OBJECT_BASE, 0, 0, 0], 1000)
            .unwrap();
        assert_ne!(runtime.anda(Duration::ZERO, &Ritmo::CONSOLE), Step::Stopped);
        assert_eq!(runtime.retained_count(), 1);
        assert!(!runtime.saiu_sozinho());
        runtime.retained[0]
            .session
            .machine
            .cpu_mut()
            .write_mem(
                HANDLER,
                &[0xe3a00001u32, 0xe12fff1e]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        runtime.retained[0]
            .session
            .machine
            .call_guest(shell("CloseApplet"), [loader::OBJECT_BASE, 0, 0, 0], 1000)
            .unwrap();
        let mut step = Step::Ahead;
        for _ in 0..4 {
            step = runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
            if step == Step::Stopped {
                break;
            }
        }
        assert_eq!(runtime.retained_count(), 0);
        assert_eq!(step, Step::Stopped);
    }

    #[test]
    fn installed_handoff_rejected_preparation_keeps_source_running() {
        let (source, a, _ra) = fixture(0x01006601, false);
        let (_target, b, _rb) = fixture(0x01006602, false);
        let mut runtime = HandsetRuntime::new(
            source,
            a.clone(),
            InstalledCatalog::new([a.clone(), b.clone()]),
        )
        .unwrap();
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        request(&mut runtime, b.class);
        runtime
            .reject_handoff("Target profile/storage preparation failed".into())
            .unwrap();
        assert!(runtime.pending_handoff().is_none());
        assert_eq!(runtime.classe(), a.class);
        assert_eq!(runtime.retained_count(), 0);
        runtime
            .machine
            .call_guest(
                shell("SetTimer"),
                [loader::OBJECT_BASE, 0, CALLBACK, APP],
                1000,
            )
            .unwrap();
        assert_ne!(runtime.anda(Duration::ZERO, &Ritmo::CONSOLE), Step::Stopped);
        assert_eq!(runtime.machine.cpu().read_u32(APP + 16).unwrap(), 1);
    }
    #[test]
    fn installed_handoff_prepared_owner_and_suspend_wait_for_global_resume() {
        let (source, a, _ra) = fixture(0x01007701, false);
        let (target, b, _rb) = fixture(0x01007702, false);
        let mut runtime = HandsetRuntime::new(
            source,
            a.clone(),
            InstalledCatalog::new([a.clone(), b.clone()]),
        )
        .unwrap();
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        request(&mut runtime, b.class);
        runtime.congela_o_som(true);
        runtime.begin_handoff(Some(target)).unwrap();
        assert_eq!(runtime.classe(), a.class);
        assert_eq!(runtime.machine.cpu().read_u32(APP + 40).unwrap(), 0);
        assert!(runtime.pending_handoff().is_none());
        let before = runtime
            .transition
            .as_ref()
            .unwrap()
            .prepared
            .as_ref()
            .unwrap()
            .machine
            .clock_us_for_input();
        std::thread::sleep(Duration::from_millis(5));
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        assert_eq!(
            runtime
                .transition
                .as_ref()
                .unwrap()
                .prepared
                .as_ref()
                .unwrap()
                .machine
                .clock_us_for_input(),
            before
        );
        assert_eq!(runtime.classe(), a.class);
        runtime.congela_o_som(false);
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        assert_eq!(runtime.classe(), b.class);
        assert_eq!(runtime.retained_count(), 1);
    }

    #[test]
    fn installed_handoff_background_close_resumes_parent_and_keeps_child_timer() {
        let (source, a, _ra) = fixture(0x01008801, false);
        let (target, b, _rb) = fixture(0x01008802, true);
        let mut runtime = HandsetRuntime::new(
            source,
            a.clone(),
            InstalledCatalog::new([a.clone(), b.clone()]),
        )
        .unwrap();
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        request(&mut runtime, b.class);
        runtime.begin_handoff(Some(target)).unwrap();
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        runtime
            .machine
            .call_guest(shell("CloseApplet"), [loader::OBJECT_BASE, 0, 0, 0], 1000)
            .unwrap();
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        assert_eq!(runtime.classe(), a.class);
        assert_eq!(runtime.retained_count(), 1);
        assert_eq!(runtime.retained[0].entry.class, b.class);
        assert!(runtime.retained[0].background);
        assert_eq!(runtime.machine.cpu().read_u32(APP + 44).unwrap(), 1);
        runtime.retained[0]
            .session
            .machine
            .call_guest(
                shell("SetTimer"),
                [loader::OBJECT_BASE, 0, CALLBACK, APP],
                1000,
            )
            .unwrap();
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        assert_eq!(
            runtime.retained[0]
                .session
                .machine
                .cpu()
                .read_u32(APP + 16)
                .unwrap(),
            1
        );
    }
    #[test]
    fn installed_handoff_resumed_background_owner_is_active_during_resume() {
        let (mut runtime, _ra, _rb) = background_pair();
        let class = runtime.retained[0].entry.class;
        // O RESUME consulta ActiveApplet por código guest e guarda a classe no próprio heap.
        let active = shell("ActiveApplet");
        let code = [
            0xe92d4010u32,
            0xe1a04000,
            0xe3510003,
            0x1a000003,
            0xe59f000c,
            0xe59fc00c,
            0xe12fff3c,
            0xe5840018,
            0xe8bd8010,
            loader::OBJECT_BASE,
            active,
        ];
        runtime.retained[0]
            .session
            .machine
            .cpu_mut()
            .write_mem(
                HANDLER,
                &code
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        request(&mut runtime, class);
        runtime.begin_handoff(None).unwrap();
        for _ in 0..3 {
            runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        }
        assert_eq!(runtime.classe(), class);
        assert_eq!(runtime.machine.cpu().read_u32(APP + 24).unwrap(), class);
    }
    #[test]
    fn installed_handoff_repeated_resume_reuses_owner_display_and_reference() {
        let (mut source, _a, _root) = fixture(0x01009901, false);
        source.anda(Duration::ZERO, &Ritmo::CONSOLE);
        let before = source.memory().1;
        let code = [
            0xe3510003u32,
            0x05932008,
            0x05802048,
            0xe3a00001,
            0xe12fff1e,
        ];
        source
            .machine
            .cpu_mut()
            .write_mem(
                HANDLER,
                &code
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        let mut display = 0;
        for _ in 0..12 {
            let (outcome, call) = source.machine.begin_applet_lifecycle(2, 1000).unwrap();
            assert!(matches!(outcome, Outcome::Returned { .. }));
            source.machine.finish_applet_lifecycle(call).unwrap();
            let (outcome, call) = source.machine.begin_applet_lifecycle(3, 1000).unwrap();
            assert!(matches!(outcome, Outcome::Returned { .. }));
            source.machine.finish_applet_lifecycle(call).unwrap();
            let current = source.machine.cpu().read_u32(APP + 72).unwrap();
            if display == 0 {
                display = current;
            }
            assert_eq!(current, display);
            assert_eq!(source.memory().1, before);
        }
        assert_eq!(
            source
                .machine
                .call_guest(aee::encode(Interface::Display, 0), [display, 0, 0, 0], 1000)
                .unwrap(),
            Outcome::Returned { code: 2 }
        );
        assert_eq!(
            source
                .machine
                .call_guest(aee::encode(Interface::Display, 1), [display, 0, 0, 0], 1000)
                .unwrap(),
            Outcome::Returned { code: 1 }
        );
    }
    #[test]
    fn installed_handoff_background_launch_after_foreground_closed_is_not_lost() {
        let (mut runtime, _ra, _rb) = background_pair();
        let source = runtime.retained[0].entry.class;
        runtime
            .machine
            .call_guest(shell("CloseApplet"), [loader::OBJECT_BASE, 0, 0, 0], 1000)
            .unwrap();
        assert_ne!(runtime.anda(Duration::ZERO, &Ritmo::CONSOLE), Step::Stopped);
        assert_eq!(runtime.retained_count(), 1);
        assert_eq!(
            runtime.retained[0]
                .session
                .machine
                .call_guest(
                    shell("StartApplet"),
                    [loader::OBJECT_BASE, source, 0, 0],
                    1000
                )
                .unwrap(),
            Outcome::Returned { code: 0 }
        );
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        assert_eq!(
            runtime.pending_handoff().map(|r| r.target.class),
            Some(source),
            "a closed foreground cannot block an accepted request from a live background owner"
        );
        runtime.begin_handoff(None).unwrap();
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        assert_eq!(runtime.classe(), source);
        assert_eq!(runtime.retained_count(), 0);
        assert_eq!(runtime.machine.cpu().read_u32(APP + 44).unwrap(), 1);
    }
    #[test]
    fn installed_handoff_live_predicate_checks_owner_state_and_exact_loaded_identity() {
        let (source, app, _root) = fixture(0x0100aa01, false);
        let mut runtime =
            HandsetRuntime::new(source, app.clone(), InstalledCatalog::new([app.clone()])).unwrap();
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        assert!(runtime.is_live_foreground(&app));
        let mut other = app.clone();
        other.class ^= 1;
        assert!(!runtime.is_live_foreground(&other));
        let mut other = app.clone();
        other.module = other.module.with_file_name("different.mod");
        assert!(!runtime.is_live_foreground(&other));
        let mut other = app.clone();
        other.mif = other.mif.with_file_name("different.mif");
        assert!(!runtime.is_live_foreground(&other));
        let mut other = app.clone();
        other.module_sha256.replace_range(
            0..1,
            if app.module_sha256.starts_with('0') {
                "1"
            } else {
                "0"
            },
        );
        assert!(!runtime.is_live_foreground(&other));
        let mut other = app.clone();
        other.mif_sha256.replace_range(
            0..1,
            if app.mif_sha256.starts_with('0') {
                "1"
            } else {
                "0"
            },
        );
        assert!(!runtime.is_live_foreground(&other));
        runtime
            .machine
            .call_guest(shell("CloseApplet"), [loader::OBJECT_BASE, 0, 0, 0], 1000)
            .unwrap();
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        assert!(
            !runtime.is_live_foreground(&app),
            "same class/path after STOP is not a live top owner"
        );
        let (source, app, _background_root) = fixture(0x0100aa02, true);
        let mut runtime =
            HandsetRuntime::new(source, app.clone(), InstalledCatalog::new([app.clone()])).unwrap();
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        runtime
            .machine
            .call_guest(shell("CloseApplet"), [loader::OBJECT_BASE, 0, 0, 0], 1000)
            .unwrap();
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        assert!(
            !runtime.is_live_foreground(&app),
            "STOP FALSE keeps memory alive without making it top-visible"
        );
    }
    fn background_self_request(
        class: u32,
    ) -> (HandsetRuntime, InstalledApplet, crate::scratch::TempDir) {
        let (source, app, root) = fixture(class, true);
        let mut runtime =
            HandsetRuntime::new(source, app.clone(), InstalledCatalog::new([app.clone()])).unwrap();
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        runtime
            .machine
            .cpu_mut()
            .write_u32(APP + 20, 0x76543210)
            .unwrap();
        runtime
            .machine
            .call_guest(shell("CloseApplet"), [loader::OBJECT_BASE, 0, 0, 0], 1000)
            .unwrap();
        request(&mut runtime, class);
        assert!(runtime.pending_handoff().unwrap().resume);
        (runtime, app, root)
    }
    fn assert_same_self_owner(runtime: &HandsetRuntime, app: &InstalledApplet) {
        assert_eq!(runtime.classe(), app.class);
        assert_eq!(runtime.retained_count(), 0);
        assert!(runtime.is_live_foreground(app));
        assert!(runtime.request.is_none());
        assert_eq!(
            runtime.machine.cpu().read_u32(APP + 20).unwrap(),
            0x76543210
        );
        assert_eq!(
            runtime.machine.cpu().read_u32(APP + 32).unwrap(),
            1,
            "START once"
        );
        assert_eq!(
            runtime.machine.cpu().read_u32(APP + 44).unwrap(),
            1,
            "RESUME once"
        );
    }
    #[test]
    fn installed_handoff_background_self_resume_keeps_heap_timer_and_private_file() {
        let (mut runtime, app, root) = background_self_request(0x0100bb01);
        let content = runtime.content_root().to_path_buf();
        std::fs::write(root.join("private-owner.dat"), b"owner").unwrap();
        runtime
            .machine
            .cpu_mut()
            .write_mem(APP + 0x400, b"private-owner.dat\0")
            .unwrap();
        runtime
            .machine
            .call_guest(
                shell("CreateInstance"),
                [loader::OBJECT_BASE, 0x01001003, APP + 0x300, 0],
                1000,
            )
            .unwrap();
        let manager = runtime.machine.cpu().read_u32(APP + 0x300).unwrap();
        let open = (0..128)
            .find(|&slot| Interface::FileMgr.method(slot) == Some("OpenFile"))
            .unwrap();
        let file = match runtime
            .machine
            .call_guest(
                aee::encode(Interface::FileMgr, open),
                [manager, APP + 0x400, 1, 0],
                1000,
            )
            .unwrap()
        {
            Outcome::Returned { code } if code != 0 => code,
            other => panic!("{other:?}"),
        };
        let read = (0..128)
            .find(|&slot| Interface::File.method(slot) == Some("Read"))
            .unwrap();
        assert_eq!(
            runtime
                .machine
                .call_guest(
                    aee::encode(Interface::File, read),
                    [file, APP + 0x500, 1, 0],
                    1000
                )
                .unwrap(),
            Outcome::Returned { code: 1 }
        );
        assert_eq!(
            runtime.machine.cpu().read_u32(APP + 0x500).unwrap() & 255,
            u32::from(b'o')
        );
        runtime
            .machine
            .call_guest(
                shell("SetTimer"),
                [loader::OBJECT_BASE, 0, CALLBACK, APP],
                1000,
            )
            .unwrap();
        runtime.begin_handoff(None).unwrap();
        assert_same_self_owner(&runtime, &app);
        assert_eq!(runtime.content_root(), content);
        assert_eq!(
            runtime
                .machine
                .call_guest(
                    aee::encode(Interface::File, read),
                    [file, APP + 0x500, 1, 0],
                    1000
                )
                .unwrap(),
            Outcome::Returned { code: 1 }
        );
        assert_eq!(
            runtime.machine.cpu().read_u32(APP + 0x500).unwrap() & 255,
            u32::from(b'w'),
            "open private file position preserved"
        );
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        assert_eq!(runtime.machine.cpu().read_u32(APP + 16).unwrap(), 1);
    }
    #[test]
    fn installed_handoff_background_self_resume_waits_for_global_unpause() {
        let (mut runtime, app, _root) = background_self_request(0x0100bb02);
        runtime
            .machine
            .call_guest(
                shell("SetTimer"),
                [loader::OBJECT_BASE, 0, CALLBACK, APP],
                1000,
            )
            .unwrap();
        runtime.congela_o_som(true);
        let clock = runtime.machine.clock_us_for_input();
        runtime.begin_handoff(None).unwrap();
        assert!(runtime.request.is_some());
        assert!(runtime.pending_handoff().is_none());
        assert!(runtime.machine.applet_em_background());
        std::thread::sleep(Duration::from_millis(5));
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        assert_eq!(runtime.machine.clock_us_for_input(), clock);
        assert_eq!(runtime.machine.cpu().read_u32(APP + 44).unwrap(), 0);
        assert_eq!(runtime.machine.cpu().read_u32(APP + 16).unwrap(), 0);
        runtime.congela_o_som(false);
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        assert_same_self_owner(&runtime, &app);
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        assert_eq!(runtime.machine.cpu().read_u32(APP + 16).unwrap(), 1);
    }
    #[test]
    fn installed_handoff_background_self_resume_keeps_request_until_guest_returns() {
        let (mut runtime, app, _root) = background_self_request(0x0100bb03);
        let code = [
            0xe280c020u32,
            0xe79c2101,
            0xe2822001,
            0xe78c2101,
            0xe3510003,
            0x1a000002,
            0xe59f200c,
            0xe2522001,
            0x1afffffd,
            0xe3a00001,
            0xe12fff1e,
            600000,
        ];
        runtime
            .machine
            .cpu_mut()
            .write_mem(
                HANDLER,
                &code
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        runtime
            .machine
            .call_guest(
                shell("SetTimer"),
                [loader::OBJECT_BASE, 0, CALLBACK, APP],
                1000,
            )
            .unwrap();
        runtime.begin_handoff(None).unwrap();
        assert!(runtime.is_transitioning());
        assert!(runtime.request.is_some());
        assert!(runtime.pending_handoff().is_none());
        assert_eq!(runtime.retained_count(), 0);
        assert_eq!(runtime.machine.cpu().read_u32(APP + 16).unwrap(), 0);
        for _ in 0..10 {
            runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
            if !runtime.is_transitioning() {
                break;
            }
        }
        assert_same_self_owner(&runtime, &app);
        runtime.anda(Duration::ZERO, &Ritmo::CONSOLE);
        assert_eq!(runtime.machine.cpu().read_u32(APP + 16).unwrap(), 1);
    }
}
