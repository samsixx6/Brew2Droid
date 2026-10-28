//! A notação compacta vira SMF antes de sintetizar: tempo e instrumentos têm um só caminho.

use super::{be16, be32};
use std::collections::BTreeSet;

// Todos os timebases CMF dividem 1920; trocar base não perde frações de pulso.
const DIVISION: u64 = 1920;
const BASES: [u32; 16] = [
    6, 12, 24, 48, 96, 192, 384, 0, 15, 30, 60, 120, 240, 480, 960, 0,
];
const MAX_EVENTS: usize = 16_384;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Voice {
    source: u8,
    drums: bool,
}

enum Kind {
    Midi(Voice, Vec<u8>),
    Tempo(u32, u8),
    End,
}
struct Event {
    tick: u64,
    kind: Kind,
}

fn scale(value: u8) -> u8 {
    ((u16::from(value) * 127 + 31) / 63) as u8
}

pub(super) fn to_midi(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let end = be32(bytes, 4)?
        .checked_add(8)
        .filter(|n| *n <= bytes.len())
        .ok_or("CMF incompleto")?;
    let bytes = &bytes[..end];
    let header = be16(bytes, 8)?
        .checked_add(10)
        .filter(|n| *n >= 13 && *n <= end)
        .ok_or("cabeçalho CMF inválido")?;
    if !matches!((bytes[10], bytes[11]), (1, 1 | 2) | (2, 1)) {
        return Err("CMF misto ou perfil musical desconhecido".into());
    }
    let tracks = bytes[12];
    if !(1..=4).contains(&tracks) {
        return Err("quantidade de faixas CMF inválida".into());
    }
    let mut at = 13;
    let mut extended_notes = None;
    while at < header {
        let len = be16(bytes, at + 4)?;
        let next = at
            .checked_add(6 + len)
            .filter(|n| *n <= header)
            .ok_or("metadado CMF truncado")?;
        if &bytes[at..at + 4] == b"note" {
            let config = be16(bytes, at + 6)?;
            if len != 2 || config > 1 {
                return Err("configuração de notas CMF inválida".into());
            }
            extended_notes = Some(config == 1);
        }
        at = next;
    }
    let extended_notes = extended_notes.ok_or("CMF musical sem configuração de notas")?;
    let mut events = Vec::new();
    let mut event_count = 0;
    for track in 0..tracks {
        if bytes.get(at..at + 4) != Some(b"trac") {
            return Err("CMF sem faixa declarada".into());
        }
        let len = be32(bytes, at + 4)?;
        let next = at
            .checked_add(8)
            .and_then(|start| start.checked_add(len))
            .filter(|n| *n <= end)
            .ok_or("faixa CMF truncada")?;
        parse_track(
            &bytes[at + 8..next],
            track,
            extended_notes,
            &mut events,
            &mut event_count,
        )?;
        at = next;
    }
    if at != end {
        return Err("CMF contém faixas além das declaradas".into());
    }
    events.sort_by_key(|event| event.tick);
    encode(events)
}

fn parse_track(
    track: &[u8],
    index: u8,
    extended: bool,
    out: &mut Vec<Event>,
    count: &mut usize,
) -> Result<(), String> {
    let mut banks = [0u8; 4];
    let mut programs = [0u8; 4];
    let mut volumes = [50u8; 4];
    let mut pans = [32u8; 4];
    let mut tick = 0u64;
    let mut at = 0;
    let mut ended = false;
    while at < track.len() {
        *count += 1;
        if *count > MAX_EVENTS {
            return Err("CMF excede 16384 eventos".into());
        }
        tick += u64::from(track[at]);
        let status = *track.get(at + 1).ok_or("evento CMF truncado")?;
        at += 2;
        if status != 255 {
            let key = status & 63;
            if key == 63 {
                return Err("nota CMF reservada".into());
            }
            let gate = *track.get(at).ok_or("nota CMF truncada")?;
            at += 1;
            let dynamics = if extended {
                let value = *track.get(at).ok_or("velocidade CMF truncada")?;
                at += 1;
                value
            } else {
                252
            };
            let channel = usize::from(status >> 6);
            let voice = Voice {
                source: index * 4 + channel as u8,
                drums: banks[channel] == 63,
            };
            let key = i16::from(key) + 45 + [0, 12, -24, -12][usize::from(dynamics & 3)];
            if !(0..=127).contains(&key) {
                return Err("altura CMF fora do MIDI".into());
            }
            if gate != 0 && dynamics >> 2 != 0 {
                out.push(Event {
                    tick,
                    kind: Kind::Midi(voice, vec![0x90, key as u8, scale(dynamics >> 2)]),
                });
                out.push(Event {
                    tick: tick + u64::from(gate),
                    kind: Kind::Midi(voice, vec![0x80, key as u8, 0]),
                });
            }
            continue;
        }
        let command = *track.get(at).ok_or("comando CMF truncado")?;
        let value = *track.get(at + 1).ok_or("comando CMF truncado")?;
        at += 2;
        let channel = usize::from(value >> 6);
        let data = value & 63;
        let voice = Voice {
            source: index * 4 + channel as u8,
            drums: banks[channel] == 63,
        };
        let mut message = |voice, data| {
            out.push(Event {
                tick,
                kind: Kind::Midi(voice, data),
            })
        };
        match command {
            0xe0 => {
                if !voice.drums {
                    programs[channel] = data;
                    message(
                        voice,
                        vec![0xc0, data + if banks[channel] == 3 { 64 } else { 0 }],
                    );
                }
            }
            0xe1 => {
                if !matches!(data, 0 | 2 | 3 | 63) {
                    return Err("banco CMF desconhecido".into());
                }
                banks[channel] = data;
                let voice = Voice {
                    drums: data == 63,
                    ..voice
                };
                message(voice, vec![0xb0, 7, scale(volumes[channel])]);
                message(
                    voice,
                    vec![
                        0xb0,
                        10,
                        if pans[channel] == 32 {
                            64
                        } else {
                            scale(pans[channel])
                        },
                    ],
                );
                if !voice.drums {
                    message(
                        voice,
                        vec![0xc0, programs[channel] + if data == 3 { 64 } else { 0 }],
                    );
                }
            }
            0xe2 => {
                volumes[channel] = data;
                message(voice, vec![0xb0, 7, scale(data)]);
            }
            0xe3 => {
                pans[channel] = data;
                message(
                    voice,
                    vec![0xb0, 10, if data == 32 { 64 } else { scale(data) }],
                );
            }
            0xe4 => {
                let bend = u16::from(data) * 256;
                message(voice, vec![0xe0, (bend & 127) as u8, (bend >> 7) as u8]);
            }
            0xe7 if data <= 12 => {
                for (cc, v) in [(101, 0), (100, 0), (6, data), (101, 127), (100, 127)] {
                    message(voice, vec![0xb0, cc, v]);
                }
            }
            0x00..=0x7f => {
                let voice = Voice {
                    source: index * 4 + (command >> 5),
                    drums: banks[usize::from(command >> 5)] == 63,
                };
                let bend = ((u16::from(command & 31) << 8) | u16::from(value)) * 2;
                message(voice, vec![0xe0, (bend & 127) as u8, (bend >> 7) as u8]);
            }
            0xc0..=0xcf if index == 0 && value >= 20 => {
                let base = BASES[usize::from(command & 15)];
                if base == 0 {
                    return Err("timebase CMF reservado".into());
                }
                out.push(Event {
                    tick,
                    kind: Kind::Tempo(base, value),
                });
            }
            0xde if value == 0 => {}
            0xd0 if index == 0 && value <= 1 => {}
            0xdf if value == 0 => {
                out.push(Event {
                    tick,
                    kind: Kind::End,
                });
                ended = true;
                break;
            }
            _ => return Err(format!("comando musical CMF não suportado: {command:02x}")),
        }
    }
    if !ended || at != track.len() {
        return Err("fim de faixa CMF inválido".into());
    }
    Ok(())
}

fn vlq(mut value: u64, out: &mut Vec<u8>) {
    let mut bytes = [0u8; 4];
    let mut at = 3;
    bytes[at] = (value & 127) as u8;
    while {
        value >>= 7;
        value != 0
    } {
        at -= 1;
        bytes[at] = (value & 127) as u8 | 128;
    }
    out.extend_from_slice(&bytes[at..]);
}

fn encode(events: Vec<Event>) -> Result<Vec<u8>, String> {
    let voices: BTreeSet<Voice> = events
        .iter()
        .filter_map(|event| match &event.kind {
            Kind::Midi(voice, data) if data[0] == 0x90 => Some(*voice),
            _ => None,
        })
        .collect();
    if voices.iter().filter(|v| v.drums).count() > 1 {
        return Err("CMF com múltiplos canais de bateria independentes".into());
    }
    let mut mapping = std::collections::BTreeMap::new();
    let mut channels: Vec<u8> = (0..16).filter(|n| *n != 9).collect();
    for voice in &voices {
        let channel = if voice.drums {
            9
        } else {
            if channels.is_empty() {
                return Err("CMF excede canais melódicos MIDI".into());
            }
            channels.remove(0)
        };
        mapping.insert(*voice, channel);
    }
    let mut body = vec![0, 255, 81, 3, 7, 83, 0]; // 125 BPM, padrão CMF.
    for channel in mapping.values() {
        body.extend([0, 0xb0 | channel, 7, scale(50)]);
    }
    let (mut previous, mut pulse, mut emitted) = (0u64, 0u64, 0u64);
    let (mut base, mut bpm, mut seconds) = (48u32, 125u8, 0.0f64);
    for event in events {
        let elapsed = event.tick - previous;
        seconds += elapsed as f64 * 60.0 / (f64::from(base) * f64::from(bpm));
        // FF6 usa uma partitura válida de 146,130s. CMF e SMF compartilham
        // o mesmo sintetizador; o teto temporal deve ser o mesmo nos dois caminhos.
        if seconds > super::super::midi::MAX_SEGUNDOS {
            return Err(format!("CMF excede {} segundos", super::super::midi::MAX_SEGUNDOS));
        }
        pulse += elapsed * DIVISION / u64::from(base);
        previous = event.tick;
        match event.kind {
            Kind::Tempo(next_base, next_bpm) => {
                base = next_base;
                bpm = next_bpm;
                let tempo = 60_000_000u32 / u32::from(bpm);
                vlq(pulse - emitted, &mut body);
                body.extend([
                    255,
                    81,
                    3,
                    (tempo >> 16) as u8,
                    (tempo >> 8) as u8,
                    tempo as u8,
                ]);
                emitted = pulse;
            }
            Kind::Midi(voice, mut data) => {
                if let Some(channel) = mapping.get(&voice) {
                    vlq(pulse - emitted, &mut body);
                    data[0] |= channel;
                    body.extend(data);
                    emitted = pulse;
                }
            }
            Kind::End => {}
        }
    }
    // O EOT pode vir depois da última nota: preservar esse silêncio também no synth interno.
    for channel in mapping.values() {
        vlq(pulse - emitted, &mut body);
        body.extend([0xb0 | channel, 123, 0]);
        emitted = pulse;
    }
    vlq(pulse - emitted, &mut body);
    body.extend([255, 47, 0]);
    let mut smf = b"MThd\0\0\0\x06\0\0\0\x01\x07\x80MTrk".to_vec();
    smf.extend((body.len() as u32).to_be_bytes());
    smf.extend(body);
    Ok(smf)
}
