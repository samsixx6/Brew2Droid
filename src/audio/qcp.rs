//! QCP/QLCM com QCELP-13K: chunks limitados pelo RIFF e quadros de 160 amostras.
//! Framing: RFC 3625. O decoder numérico deriva do FFmpeg; ver third-party/qcelp.

use super::wav::Sound;
use std::{ffi::c_void, ptr::NonNull};

unsafe extern "C" {
    fn brew_qcelp_new() -> *mut c_void;
    fn brew_qcelp_free(state: *mut c_void);
    fn brew_qcelp_frame(state: *mut c_void, data: *const u8, size: usize, pcm: *mut f32) -> i32;
}

pub(crate) struct Decoder(NonNull<c_void>);
impl Decoder {
    pub(crate) fn new() -> Result<Self, String> {
        // O estado é privado a este decoder e só um worker o acessa.
        NonNull::new(unsafe { brew_qcelp_new() })
            .map(Self)
            .ok_or_else(|| "sem memória QCELP".into())
    }
    pub(crate) fn decode(&mut self, bytes: &[u8]) -> Result<Vec<f32>, String> {
        let mut result = Vec::new();
        let mut at = 0;
        while at < bytes.len() {
            let size = packet_size(bytes[at]).ok_or("taxa QCELP desconhecida")?;
            let packet = bytes.get(at..at + size).ok_or("quadro QCELP truncado")?;
            if result.len() >= 8000 * 120 {
                return Err("QCELP excede 120 segundos".into());
            }
            let mut frame = [0.0; 160];
            // O C recebe exatamente um pacote validado e um destino de 160 floats.
            let count = unsafe {
                brew_qcelp_frame(
                    self.0.as_ptr(),
                    packet.as_ptr(),
                    packet.len(),
                    frame.as_mut_ptr(),
                )
            };
            if count != 160 || frame.iter().any(|value| !value.is_finite()) {
                return Err("quadro QCELP inválido".into());
            }
            result.extend(frame);
            at += size;
        }
        Ok(result)
    }
}
impl Drop for Decoder {
    fn drop(&mut self) {
        unsafe { brew_qcelp_free(self.0.as_ptr()) };
    }
}

fn packet_size(mode: u8) -> Option<usize> {
    [1, 4, 8, 17, 35].get(usize::from(mode)).copied()
}
fn le32(bytes: &[u8], at: usize) -> Result<usize, String> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or("QCP truncado")?
            .try_into()
            .unwrap(),
    ) as usize)
}

pub fn decode(bytes: &[u8]) -> Result<Sound, String> {
    if bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"QLCM") {
        return Err("não é QCP/QLCM".into());
    }
    let end = le32(bytes, 4)?
        .checked_add(8)
        .filter(|end| *end <= bytes.len())
        .ok_or("RIFF QCP truncado")?;
    let mut at = 12usize;
    let mut format = None;
    let mut payload = None;
    let mut packet_count = None;
    while at < end {
        let id = bytes.get(at..at + 4).ok_or("chunk QCP truncado")?;
        let len = le32(bytes, at + 4)?;
        let start = at.checked_add(8).ok_or("overflow QCP")?;
        let next = start
            .checked_add(len)
            .filter(|next| *next <= end)
            .ok_or("chunk QCP fora do RIFF")?;
        let chunk = &bytes[start..next];
        match id {
            b"fmt " if format.is_none() => format = Some(chunk),
            b"data" if payload.is_none() => payload = Some(chunk),
            b"vrat" if chunk.len() >= 8 && le32(chunk, 0)? != 0 => {
                packet_count = Some(le32(chunk, 4)?)
            }
            _ => {}
        }
        at = next + (len & 1);
        // Alguns arquivos móveis omitem o padding do último chunk de tamanho ímpar.
        if next == end {
            break;
        }
    }
    let fmt = format.ok_or("QCP sem fmt")?;
    const GUID: [u8; 16] = [
        0x41, 0x6d, 0x7f, 0x5e, 0x15, 0xb1, 0xd0, 0x11, 0xba, 0x91, 0, 0x80, 0x5f, 0xb4, 0xb9, 0x7e,
    ];
    if fmt.len() < 110 || fmt[0] != 1 || fmt[2..18] != GUID {
        return Err("codec QCP diferente de QCELP-13K".into());
    }
    if u16::from_le_bytes([fmt[104], fmt[105]]) != 160
        || u16::from_le_bytes([fmt[106], fmt[107]]) != 8000
    {
        return Err("geometria QCELP diferente de 160/8000".into());
    }
    let samples = Decoder::new()?.decode(payload.ok_or("QCP sem data")?)?;
    if packet_count.is_some_and(|count| count != 0 && count != samples.len() / 160) {
        return Err("contagem QCP não corresponde aos pacotes".into());
    }
    Ok(Sound {
        rate: 8000,
        channels: 1,
        samples,
    })
}
