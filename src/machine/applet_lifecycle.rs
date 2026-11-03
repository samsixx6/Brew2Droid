//! Parâmetros de ciclo de vida BREW guardados até o retorno real do guest.
use super::*;

pub(crate) struct LifecycleCall {
    parameter: u32,
    event: u32,
}
impl<C: CpuBackend> Machine<C> {
    /// AEEAppStart empresta o display padrão pertencente ao proprietário, sem uma
    /// referência nova a cada RESUME. Referências adicionais do guest continuam explícitas.
    pub(crate) fn default_applet_display(&mut self) -> Result<u32, CpuError> {
        if self.objects.kind_of(self.applet_display) == Some(Interface::Display) {
            return Ok(self.applet_display);
        }
        let display = self
            .objects
            .create(Interface::Display)
            .ok_or_else(|| CpuError("no default applet display memory".into()))?;
        self.cpu
            .write_u32(display, loader::vtable_addr(Interface::Display))?;
        self.applet_display = display;
        Ok(display)
    }

    pub(crate) fn promote_applet_for_resume(&mut self) {
        self.applet_em_background = false;
    }

    pub(crate) fn notify_applet_would_start(&mut self, budget: u64) -> Result<Outcome, CpuError> {
        if !self.handset_event_ready() {
            return Err(CpuError("WOULD_START awaits guest return".into()));
        }
        let app = self.current_applet;
        let handler = self.cpu.read_u32(self.cpu.read_u32(app)? + 8)?;
        self.call_guest(handler, [app, 0x12, 0, 0], budget)
    }
    pub(crate) fn begin_applet_lifecycle(
        &mut self,
        event: u32,
        budget: u64,
    ) -> Result<(Outcome, LifecycleCall), CpuError> {
        if !self.handset_event_ready() {
            return Err(CpuError("lifecycle awaits guest return".into()));
        }
        let applet = self.current_applet;
        if applet == 0 {
            return Err(CpuError("lifecycle has no live applet".into()));
        }
        let handler = self.cpu.read_u32(self.cpu.read_u32(applet)? + 8)?;
        // AEE.h: SUSPEND -> AEESuspendInfo{int nErr;boolean bCloseDialogs}.
        // AEEShell.h: RESUME -> AEEAppStart{error,cls,pDisplay,rc,pszArgs}.
        let size = if event == 2 {
            8
        } else if event == 3 {
            24
        } else {
            return Err(CpuError("unsupported lifecycle event".into()));
        };
        let parameter = self
            .heap
            .alloc(size)
            .ok_or_else(|| CpuError("no lifecycle parameter memory".into()))?;
        self.cpu.write_mem(parameter, &vec![0; size as usize])?;
        if event == 2 {
            self.cpu.write_mem(parameter + 4, &[1])?;
        } else {
            let display = self.default_applet_display()?;
            self.cpu.write_u32(parameter + 4, self.applet_class)?;
            self.cpu.write_u32(parameter + 8, display)?;
            for (i, v) in [
                0i16,
                0,
                self.screen().width() as i16,
                self.screen().height() as i16,
            ]
            .into_iter()
            .enumerate()
            {
                self.cpu
                    .write_mem(parameter + 12 + i as u32 * 2, &v.to_le_bytes())?;
            }
            let screen = if let Some(screen) = self.bitmaps.get_mut(&self.device_bitmap) {
                screen
            } else {
                &mut self.screen
            };
            screen.fill_rect(
                Rect {
                    x: 0,
                    y: 0,
                    width: screen.width() as i16,
                    height: screen.height() as i16,
                },
                Rgb::BLACK,
            );
        }
        let outcome = self.call_guest(handler, [applet, event, 0, parameter], budget)?;
        Ok((outcome, LifecycleCall { parameter, event }))
    }
    pub(crate) fn finish_applet_lifecycle(&mut self, call: LifecycleCall) -> Result<(), CpuError> {
        if !self.handset_event_ready() {
            return Err(CpuError("lifecycle still owns guest continuation".into()));
        }
        let error = self.cpu.read_u32(call.parameter)?;
        self.heap.free(call.parameter);
        if error != 0 {
            return Err(CpuError(format!("applet lifecycle error {error}")));
        }
        if call.event == 3 {
            self.applet_em_background = false;
        }
        Ok(())
    }
}
