//! DSP FM isolado: a fronteira C não lê arquivos nem decide quais sequências aceitar.
use std::{ffi::c_void, ptr::NonNull};

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(super) struct Channel {
    pub gain: f32,
    pub left: f32,
    pub right: f32,
}

impl Default for Channel {
    fn default() -> Self {
        Self {
            gain: 1.0,
            left: std::f32::consts::FRAC_1_SQRT_2,
            right: std::f32::consts::FRAC_1_SQRT_2,
        }
    }
}

unsafe extern "C" {
    fn brew_smaf_fm_new(rate: u32) -> *mut c_void;
    fn brew_smaf_fm_delete(engine: *mut c_void);
    fn brew_smaf_fm_patch(engine: *mut c_void, payload: *const u8, size: usize) -> u32;
    fn brew_smaf_fm_on(
        engine: *mut c_void,
        token: u32,
        channel: u32,
        bank: u8,
        program: u8,
        rhythm: u8,
        note: u8,
    ) -> u32;
    fn brew_smaf_fm_off(engine: *mut c_void, token: u32);
    fn brew_smaf_fm_mute(engine: *mut c_void, first: u32, count: u32);
    fn brew_smaf_fm_render(
        engine: *mut c_void,
        frames: u32,
        channels: *const Channel,
        output: *mut f32,
    ) -> u32;
}

pub(super) struct Engine(NonNull<c_void>);

impl Engine {
    pub fn new(rate: u32) -> Result<Self, String> {
        // O ponteiro é exclusivo desta instância; toda liberação passa pelo Drop abaixo.
        NonNull::new(unsafe { brew_smaf_fm_new(rate) })
            .map(Self)
            .ok_or_else(|| super::error("falha ao criar DSP FM"))
    }
    pub fn set_patch(&mut self, payload: &[u8]) -> Result<(), String> {
        let accepted =
            unsafe { brew_smaf_fm_patch(self.0.as_ptr(), payload.as_ptr(), payload.len()) };
        if accepted == 1 {
            Ok(())
        } else {
            Err(super::error("instrumento VMA inválido ou limite excedido"))
        }
    }
    pub fn note_on(
        &mut self,
        token: u32,
        channel: u32,
        bank: u8,
        program: u8,
        rhythm: bool,
        note: u8,
    ) -> Result<(), String> {
        let accepted = unsafe {
            brew_smaf_fm_on(
                self.0.as_ptr(),
                token,
                channel,
                bank,
                program,
                u8::from(rhythm),
                note,
            )
        };
        if accepted == 1 {
            Ok(())
        } else {
            Err(super::error("nota ou banco FM não suportado"))
        }
    }
    pub fn note_off(&mut self, token: u32) {
        unsafe { brew_smaf_fm_off(self.0.as_ptr(), token) }
    }
    pub fn mute_channels(&mut self, first: u32, count: u32) {
        unsafe { brew_smaf_fm_mute(self.0.as_ptr(), first, count) }
    }
    pub fn render(
        &mut self,
        frames: usize,
        channels: &[Channel; 32],
        output: &mut [f32],
    ) -> Result<(), String> {
        if output.len()
            != frames
                .checked_mul(2)
                .ok_or_else(|| super::error("buffer FM excedido"))?
        {
            return Err(super::error("buffer FM contradiz a quantidade de quadros"));
        }
        let frames = u32::try_from(frames).map_err(|_| super::error("buffer FM excedido"))?;
        if frames == 0 {
            return Ok(());
        }
        let accepted = unsafe {
            brew_smaf_fm_render(
                self.0.as_ptr(),
                frames,
                channels.as_ptr(),
                output.as_mut_ptr(),
            )
        };
        if accepted == 1 {
            Ok(())
        } else {
            Err(super::error("falha ao renderizar FM"))
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        unsafe { brew_smaf_fm_delete(self.0.as_ptr()) }
    }
}
