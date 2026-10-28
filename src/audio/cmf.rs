//! Áudio CMF/CMX: partitura compacta e wave QCELP/IMA.
//! Sintaxe pública: draft-atarius-cmf-00; limites impedem loops e alocações sem teto.

mod song;

use super::{
    qcp,
    wav::{Sound, decode_ima_adpcm},
};
use std::collections::HashMap;

const RATE: u32 = 8000;
const MAX_FRAMES: usize = 120 * RATE as usize;
struct Cached {
    samples: Vec<f32>,
    rate: u32,
    codec: u8,
}
struct Segment {
    at: usize,
    channel: usize,
    sound: Sound,
}
struct Control {
    at: usize,
    command: u8,
    value: u8,
}

fn be16(bytes: &[u8], at: usize) -> Result<usize, String> {
    Ok(u16::from_be_bytes(
        bytes
            .get(at..at + 2)
            .ok_or("CMF truncado")?
            .try_into()
            .unwrap(),
    ) as usize)
}
fn be32(bytes: &[u8], at: usize) -> Result<usize, String> {
    Ok(u32::from_be_bytes(
        bytes
            .get(at..at + 4)
            .ok_or("CMF truncado")?
            .try_into()
            .unwrap(),
    ) as usize)
}

pub fn decode(bytes: &[u8]) -> Result<Sound, String> {
    decode_with(bytes, super::midi::decode)
}

/// A partitura passa pelo mesmo banco do SMF; efeitos wave continuam no decoder próprio.
pub(crate) fn decode_with(
    bytes: &[u8],
    mut synthesize: impl FnMut(&[u8]) -> Option<Sound>,
) -> Result<Sound, String> {
    if contains_music(bytes) {
        return synthesize(&song::to_midi(bytes)?)
            .ok_or_else(|| "partitura CMF não sintetizada".into());
    }
    decode_wave(bytes)
}

pub(crate) fn contains_music(bytes: &[u8]) -> bool {
    bytes.starts_with(b"cmid")
        && (bytes.get(10) == Some(&1)
            || (bytes.get(10) == Some(&2) && bytes.get(11).is_some_and(|v| v & 1 != 0)))
}

fn decode_wave(bytes: &[u8]) -> Result<Sound, String> {
    if bytes.get(..4) != Some(b"cmid") {
        return Err("não é CMF".into());
    }
    let end = be32(bytes, 4)?
        .checked_add(8)
        .filter(|end| *end <= bytes.len())
        .ok_or("CMF incompleto")?;
    let bytes = &bytes[..end];
    // Os arquivos 0300 reais contam a própria palavra header-length: 8+0x5d+2=103.
    let header_end = be16(bytes, 8)?
        .checked_add(10)
        .filter(|end| *end >= 13 && *end <= bytes.len())
        .ok_or("cabeçalho CMF inválido")?;
    if bytes[10] != 2 || bytes[11] & !2 != 0 || bytes[12] != 1 {
        return Err("CMF além do perfil wave de uma faixa".into());
    }
    let mut at = 13;
    let mut wave = None;
    while at < header_end {
        let length = be16(bytes, at + 4)?;
        let next = at
            .checked_add(6 + length)
            .filter(|next| *next <= header_end)
            .ok_or("metadado CMF truncado")?;
        if bytes.get(at..at + 4) == Some(b"wave") && length == 1 {
            wave = bytes.get(at + 6).copied();
        }
        at = next;
    }
    if bytes.get(at..at + 4) != Some(b"trac") {
        return Err("CMF sem trac no limite do cabeçalho".into());
    }
    let length = be32(bytes, at + 4)?;
    let track_end = at
        .checked_add(8)
        .and_then(|start| start.checked_add(length))
        .ok_or("overflow da faixa CMF")?;
    let track = bytes.get(at + 8..track_end).ok_or("faixa CMF truncada")?;
    let mut at = 0;
    let mut now_us = 0.0;
    let mut tick_us = 10_000.0;
    let mut decoders = HashMap::<u8, qcp::Decoder>::new();
    let mut cache = HashMap::<u8, Cached>::new();
    let mut segments = Vec::new();
    let mut controls = Vec::new();
    let mut links = Vec::new();
    let mut boundaries = Vec::new();
    let mut ended = false;
    let mut cache_frames = 0usize;
    let mut segment_samples = 0usize;
    while at < track.len() {
        let start = at;
        if boundaries.len() >= 16_384 {
            return Err("CMF excede 16384 eventos".into());
        }
        boundaries.push(start);
        now_us += f64::from(track[at]) * tick_us;
        let timestamp = (now_us * f64::from(RATE) / 1_000_000.0).round() as usize;
        if timestamp > MAX_FRAMES {
            return Err("CMF excede 120 segundos".into());
        }
        at += 1;
        if track.get(at) != Some(&0xff) {
            return Err("CMF contém notas ainda não suportadas".into());
        }
        let command = *track.get(at + 1).ok_or("evento CMF truncado")?;
        at += 2;
        if command == 0xf1 {
            let size = be16(track, at)?;
            at += 2;
            let packet = track
                .get(at..at + size)
                .filter(|p| p.len() >= 7)
                .ok_or("wave CMF truncada")?;
            at += size;
            let key = packet[0];
            let channel = usize::from(key >> 6);
            let mode = packet[1] >> 6;
            let codec = packet[1] & 63;
            let continued = packet[6] & 1 != 0;
            let offset = be32(packet, 2)?;
            if offset != 0 {
                links.push(start.checked_add(offset).ok_or("offset CMF inválido")?);
            }
            if !matches!(codec, 4 | 5) || mode > 2 {
                return Err("codec/modo wave CMF desconhecido".into());
            }
            if wave.is_some_and(|format| format != u8::from(codec == 4)) {
                return Err("wave CMF contradiz o cabeçalho".into());
            }
            let sound = if mode == 2 {
                let saved = cache
                    .get(&key)
                    .filter(|s| s.codec == codec)
                    .ok_or("recycle CMF sem wave armazenada")?;
                if segment_samples.saturating_add(saved.samples.len()) > MAX_FRAMES * 4 {
                    return Err("CMF excede orçamento de PCM e mistura".into());
                }
                Sound {
                    rate: saved.rate,
                    channels: 1,
                    samples: saved.samples.clone(),
                }
            } else {
                let sound = if codec == 4 {
                    if !continued {
                        decoders.insert(key, qcp::Decoder::new()?);
                    }
                    let decoder = decoders
                        .get_mut(&key)
                        .ok_or("continuação QCELP sem início")?;
                    Sound {
                        rate: RATE,
                        channels: 1,
                        samples: decoder.decode(&packet[7..])?,
                    }
                } else {
                    let info = be16(packet, 7)?;
                    let rate = [8000, 16000, 32000]
                        .get(info >> 14)
                        .copied()
                        .ok_or("taxa IMA CMF reservada")?;
                    let block = info & 0xfff;
                    if block < 4 || (packet.len() - 9) % block != 0 {
                        return Err("bloco IMA CMF truncado".into());
                    }
                    Sound {
                        rate,
                        channels: 1,
                        samples: decode_ima_adpcm(&packet[9..], 1, block),
                    }
                };
                let old = cache.get(&key).map_or(0, |s| s.samples.len());
                cache_frames = cache_frames.saturating_sub(old);
                let saved = if continued {
                    let saved = cache
                        .get_mut(&key)
                        .filter(|s| s.rate == sound.rate && s.codec == codec)
                        .ok_or("continuação wave incompatível")?;
                    if cache_frames
                        .saturating_add(saved.samples.len())
                        .saturating_add(sound.samples.len())
                        > MAX_FRAMES * 4
                    {
                        return Err("cache wave CMF excede limite".into());
                    }
                    saved.samples.extend_from_slice(&sound.samples);
                    saved.samples.len()
                } else {
                    cache.insert(
                        key,
                        Cached {
                            samples: sound.samples.clone(),
                            rate: sound.rate,
                            codec,
                        },
                    );
                    sound.samples.len()
                };
                cache_frames += saved;
                if cache_frames > MAX_FRAMES * 4 {
                    return Err("cache wave CMF excede limite".into());
                }
                sound
            };
            if mode != 0 {
                segment_samples = segment_samples
                    .checked_add(sound.samples.len())
                    .filter(|sum| *sum <= MAX_FRAMES * 4)
                    .ok_or("CMF excede orçamento de PCM e mistura")?;
                segments.push(Segment {
                    at: timestamp,
                    channel,
                    sound,
                });
            }
        } else {
            let value = *track.get(at).ok_or("comando CMF truncado")?;
            at += 1;
            match command {
                0xc0..=0xcf => {
                    let base = [
                        6, 12, 24, 48, 96, 192, 384, 0, 15, 30, 60, 120, 240, 480, 960, 0,
                    ][usize::from(command & 15)];
                    if base == 0 || value < 20 {
                        return Err("tempo CMF inválido".into());
                    }
                    tick_us = 60_000_000.0 / (f64::from(base) * f64::from(value));
                }
                0xde | 0xd0 if value <= 1 => {}
                0xdf if value == 0 => {
                    ended = true;
                    break;
                }
                0xe8 | 0xe9 | 0xb0 => controls.push(Control {
                    at: timestamp,
                    command,
                    value,
                }),
                _ => return Err(format!("evento CMF não suportado: {command:02x}")),
            }
        }
    }
    if !ended
        || links
            .iter()
            .any(|link| boundaries.binary_search(link).is_err())
    {
        return Err("faixa/encadeamento CMF incompleto".into());
    }
    let mut channels = [Vec::<f32>::new(), Vec::new(), Vec::new(), Vec::new()];
    let mut frames = (now_us * f64::from(RATE) / 1_000_000.0).round() as usize;
    for segment in segments {
        let count = segment.sound.samples.len() * RATE as usize / segment.sound.rate as usize;
        let end = segment
            .at
            .checked_add(count)
            .filter(|end| *end <= MAX_FRAMES)
            .ok_or("wave CMF excede limite")?;
        let output = &mut channels[segment.channel];
        output.resize(output.len().max(end), 0.0);
        for i in 0..count {
            let pos = i as f64 * f64::from(segment.sound.rate) / f64::from(RATE);
            let index = pos as usize;
            let left = segment.sound.samples[index];
            let right = segment
                .sound
                .samples
                .get(index + 1)
                .copied()
                .unwrap_or(left);
            output[segment.at + i] += left + (right - left) * (pos - index as f64) as f32;
        }
        frames = frames.max(end);
    }
    let mut samples = vec![0.0; frames * 2];
    let mut volume = [1.0f32; 4];
    let mut pan = [32u8; 4];
    let mut master = 1.0;
    let mut control = 0;
    for i in 0..frames {
        while let Some(event) = controls.get(control).filter(|e| e.at <= i) {
            let channel = usize::from(event.value >> 6);
            match event.command {
                0xe8 => volume[channel] = f32::from(event.value & 63) / 63.0,
                0xe9 => pan[channel] = event.value & 63,
                0xb0 => master = f32::from(event.value.min(127)) / 100.0,
                _ => {}
            }
            control += 1;
        }
        for channel in 0..4 {
            let value = channels[channel].get(i).copied().unwrap_or(0.0) * volume[channel] * master;
            samples[i * 2] += value * (f32::from(63 - pan[channel]) / 31.0).min(1.0);
            samples[i * 2 + 1] += value * (f32::from(pan[channel]) / 32.0).min(1.0);
        }
    }
    for sample in &mut samples {
        *sample = sample.clamp(-1.0, 1.0);
    }
    Ok(Sound {
        rate: RATE,
        channels: 2,
        samples,
    })
}
