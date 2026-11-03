//! SMAF ATR e MTR Handy Phone (Yamaha SMAF 3.05, seções4.4.1/4.5).

//! Codecs, controles e seleção de frases não implementados continuam sendo erros.

//! O CRC segue o apêndice 9.2; a aritmética ADPCM foi conferida com FFmpeg adpcm.c.

use super::wav::Sound;

use std::collections::BTreeMap;

mod fm;
mod score;

const MAX_INPUT: usize = 4 * 1024 * 1024;

const MAX_US: u64 = 120_000_000;

const MAX_EVENTS: usize = 16_384;

const MAX_CHUNKS: usize = 4096;

const MAX_TRACKS: usize = 8;

fn error(s: &str) -> String {
    format!("SMAF: {s}")
}

fn be32(b: &[u8]) -> Result<usize, String> {
    Ok(u32::from_be_bytes(
        b.get(..4)
            .ok_or_else(|| error("tamanho truncado"))?
            .try_into()
            .unwrap(),
    ) as usize)
}

fn crc(b: &[u8]) -> u16 {
    let mut r = 0xffffu16;
    for &v in b {
        r ^= u16::from(v) << 8;
        for _ in 0..8 {
            r = if r & 0x8000 != 0 {
                (r << 1) ^ 0x1021
            } else {
                r << 1
            };
        }
    }
    !r
}

fn chunks<'a>(mut b: &'a [u8], count: &mut usize) -> Result<Vec<([u8; 4], &'a [u8])>, String> {
    let mut out = Vec::new();
    while !b.is_empty() {
        *count += 1;
        if *count > MAX_CHUNKS {
            return Err(error("limite de chunks"));
        }

        let id = b
            .get(..4)
            .ok_or_else(|| error("chunk truncado"))?
            .try_into()
            .unwrap();

        let n = be32(b.get(4..).ok_or_else(|| error("chunk truncado"))?)?;

        let end = 8usize
            .checked_add(n)
            .filter(|&e| e <= b.len())
            .ok_or_else(|| error("chunk excede o pai"))?;

        out.push((id, &b[8..end]));
        b = &b[end..];
    }
    Ok(out)
}

fn timebase(code: u8) -> Result<u64, String> {
    match code {
        0 => Ok(1000),
        1 => Ok(2000),
        2 => Ok(4000),
        3 => Ok(5000),
        0x10 => Ok(10000),
        0x11 => Ok(20000),
        0x12 => Ok(40000),
        0x13 => Ok(50000),
        _ => Err(error("timebase reservado")),
    }
}

fn variable(b: &[u8], p: &mut usize) -> Result<u64, String> {
    let a = byte(b, p)?;
    if a & 128 == 0 {
        Ok(u64::from(a))
    } else {
        let v = byte(b, p)?;
        if v & 128 != 0 {
            return Err(error("duração inválida"));
        }
        Ok((u64::from(a & 127) << 7) + u64::from(v) + 128)
    }
}

fn byte(b: &[u8], p: &mut usize) -> Result<u8, String> {
    let v = *b.get(*p).ok_or_else(|| error("evento truncado"))?;
    *p += 1;
    Ok(v)
}

#[derive(Clone, Copy)]
enum Kind {
    Wave(u8, u64),
    Volume(u8),
    Expression(u8),
    Pan(u8),
    Nop,
}

struct Event {
    offset: usize,
    at: u64,
    channel: usize,
    kind: Kind,
}

struct Sequence {
    events: Vec<Event>,
    end: u64,
    eos_offset: usize,
}

fn sequence(
    b: &[u8],
    duration: u64,
    gate: u64,
    event_count: &mut usize,
) -> Result<Sequence, String> {
    let (mut p, mut now, mut tail) = (0usize, 0u64, 0u64);
    let mut events = Vec::new();
    let mut eos_offset = b.len();
    let mut eos = false;

    while p < b.len() {
        if b.get(p..p + 4) == Some(&[0, 0, 0, 0]) {
            eos_offset = p;
            p += 4;
            if b[p..].iter().any(|&v| v != 0) {
                return Err(error("dados após EOS"));
            }
            eos = true;
            break;
        }

        let offset = p;
        *event_count += 1;
        if *event_count > MAX_EVENTS {
            return Err(error("limite de eventos"));
        }

        let delta = variable(b, &mut p)?
            .checked_mul(duration)
            .ok_or_else(|| error("duração excedida"))?;

        now = now
            .checked_add(delta)
            .filter(|&v| v <= MAX_US)
            .ok_or_else(|| error("sequência excede 120 segundos"))?;

        let op = byte(b, &mut p)?;
        let mut channel = 0usize;

        let kind = match op {
            0 => {
                let control = byte(b, &mut p)?;
                channel = usize::from(control >> 6);
                match control & 63 {
                    0x37 | 0x3a | 0x3b => {
                        let v = byte(b, &mut p)?;
                        if v > 127 {
                            return Err(error("controle fora da faixa"));
                        }
                        match control & 63 {
                            0x37 => Kind::Volume(v),
                            0x3a => Kind::Pan(v),
                            _ => Kind::Expression(v),
                        }
                    }

                    1..=14 => {
                        let v = [0, 0, 31, 39, 47, 55, 63, 71, 79, 87, 95, 103, 111, 119, 127]
                            [usize::from(control & 63)];
                        Kind::Expression(v)
                    }

                    _ => return Err(error("controle ATR não suportado")),
                }
            }

            0xff => {
                if byte(b, &mut p)? != 0 {
                    return Err(error("evento exclusivo ATR não suportado"));
                }
                Kind::Nop
            }

            _ => {
                channel = usize::from(op >> 6);
                let id = op & 63;
                if !(1..=62).contains(&id) {
                    return Err(error("wave ID reservado"));
                }

                let length = variable(b, &mut p)?
                    .checked_mul(gate)
                    .filter(|&v| v != 0)
                    .ok_or_else(|| error("gate inválido"))?;

                let end = now
                    .checked_add(length)
                    .filter(|&v| v <= MAX_US)
                    .ok_or_else(|| error("gate excede 120 segundos"))?;
                tail = tail.max(end);
                Kind::Wave(id, length)
            }
        };

        events.push(Event {
            offset,
            at: now,
            channel,
            kind,
        });
    }

    if events.is_empty() {
        return Err(error("sequência vazia"));
    }

    Ok(Sequence {
        events,
        end: if eos { now } else { now.max(tail) },
        eos_offset,
    })
}

fn yamaha(b: &[u8]) -> Vec<i16> {
    const SCALE: [i32; 8] = [230, 230, 230, 230, 307, 409, 512, 614];
    let (mut predictor, mut step) = (0i32, 127i32);
    let mut out = Vec::with_capacity(b.len() * 2);

    for &v in b {
        for nibble in [v & 15, v >> 4] {
            let magnitude = i32::from(nibble & 7) * 2 + 1;
            let delta = step * magnitude / 8;
            predictor =
                (predictor + if nibble & 8 != 0 { -delta } else { delta }).clamp(-32768, 32767);
            out.push(predictor as i16);
            step = (step * SCALE[usize::from(nibble & 7)] / 256).clamp(127, 24576);
        }
    }
    out
}

struct Track<'a> {
    rate: u32,
    sequence: Sequence,
    waves: BTreeMap<u8, &'a [u8]>,
}

fn track<'a>(b: &'a [u8], count: &mut usize, event_count: &mut usize) -> Result<Track<'a>, String> {
    let h = b.get(..6).ok_or_else(|| error("cabeçalho ATR truncado"))?;

    if h[0] != 0 || h[1] != 0 || h[2] & 0xf0 != 0x10 || h[3] != 0 {
        return Err(error("perfil ATR não suportado"));
    }

    let rate = match h[2] & 15 {
        0 => 4000,
        1 => 8000,
        2 => 11025,
        3 => 22050,
        4 => 44100,
        _ => return Err(error("taxa reservada")),
    };

    let (duration, gate) = (timebase(h[4])?, timebase(h[5])?);
    let (mut seq, mut phrase) = (None, None);
    let mut waves = BTreeMap::new();

    for (id, body) in chunks(&b[6..], count)? {
        match &id {
            b"Atsq" => {
                if seq.is_some() {
                    return Err(error("Atsq duplicado"));
                }
                seq = Some(sequence(body, duration, gate, event_count)?);
            }

            b"AspI" => {
                if phrase.replace(body).is_some() {
                    return Err(error("AspI duplicado"));
                }
            }

            _ if &id[..3] == b"Awa" => {
                let n = id[3];
                if !(1..=62).contains(&n) || body.is_empty() || waves.contains_key(&n) {
                    return Err(error("wave inválida ou duplicada"));
                }

                if body.len() > 120 * rate as usize / 2 {
                    return Err(error("wave excede 120 segundos"));
                }
                waves.insert(n, body);
            }

            _ => return Err(error("chunk ATR não suportado")),
        }
    }

    let mut sequence = seq.ok_or_else(|| error("Atsq ausente"))?;
    if waves.is_empty() {
        return Err(error("wave ausente"));
    }

    for e in &sequence.events {
        if let Kind::Wave(id, _) = e.kind {
            if !waves.contains_key(&id) {
                return Err(error("wave referenciada ausente"));
            }
        }
    }

    if let Some(p) = phrase {
        if p.len() != 16
            || p.get(..3) != Some(b"st:")
            || p.get(7..11) != Some(b",sp:")
            || p[15] != b','
        {
            return Err(error("seleção de frases não suportada"));
        }

        let start = be32(&p[3..7])?;
        let stop = be32(&p[11..15])?;
        if stop != sequence.eos_offset {
            // eos_offset equals sequence length when EOS was omitted.

            return Err(error("stop parcial não suportado"));
        }
        if start >= stop {
            return Err(error("intervalo ATR vazio ou inválido"));
        }
        if start != 0 {
            let selected = sequence.events.iter()
                .position(|event| event.offset == start)
                .ok_or_else(|| error("início ATR fora de uma fronteira de Duration"))?;
            // Os nove efeitos originais pulam volume 127/NOP, sem estado a transportar.
            // Outros prefixos ainda exigem uma semântica de carry-in não implementada.
            if sequence.events[..selected].iter().any(|event| {
                !matches!(event.kind,
                    Kind::Nop | Kind::Volume(127) | Kind::Expression(127) | Kind::Pan(64))
            }) {
                return Err(error("estado anterior ao início ATR não suportado"));
            }
            // Reproduz os bytes selecionados: retira só o tempo dos eventos completos
            // do prefixo, preservando a primeira Duration selecionada e os gates. Não
            // identifica nuances históricas do chip. A sequência inteira já foi
            // validada/contada, e o EOS original continua limitando a saída.
            let elapsed = sequence.events[..selected].last().map_or(0, |event| event.at);
            sequence.events.drain(..selected);
            for event in &mut sequence.events {
                event.at -= elapsed;
            }
            sequence.end -= elapsed;
        }
    }

    Ok(Track {
        rate,
        sequence,
        waves,
    })
}

#[derive(Clone, Copy)]
struct Voice {
    id: u8,
    start: u64,
    gate: u64,
}

#[derive(Clone, Copy)]
struct Channel {
    voice: Option<Voice>,
    volume: u8,
    expression: u8,
    pan: u8,
}

impl Default for Channel {
    fn default() -> Self {
        Self {
            voice: None,
            volume: 127,
            expression: 127,
            pan: 64,
        }
    }
}

fn frame(us: u64, rate: u32) -> usize {
    (us * u64::from(rate) / 1_000_000) as usize
}

fn render(track: &Track<'_>, rate: u32, out: &mut [f32]) {
    let waves: BTreeMap<u8, Vec<i16>> = track
        .waves
        .iter()
        .map(|(&id, &bytes)| (id, yamaha(bytes)))
        .collect();

    let mut channels = [Channel::default(); 4];
    let mut position = 0usize;

    let mut span = |until: u64, channels: &[Channel; 4]| {
        let end = frame(until, rate).min(out.len() / 2);

        for ch in channels {
            let Some(v) = ch.voice else { continue };
            let wave = &waves[&v.id];

            let gain = (f32::from(ch.volume) / 127.0).powi(2)
                * (f32::from(ch.expression) / 127.0).powi(2)
                / 32768.0;

            let pan = f32::from(ch.pan) * std::f32::consts::FRAC_PI_2 / 127.0;

            let (left, right) = (
                if ch.pan == 127 { 0.0 } else { pan.cos() },
                if ch.pan == 0 { 0.0 } else { pan.sin() },
            );

            let start = frame(v.start, rate);
            let stop = frame(v.start + v.gate, rate).min(end);

            for i in position.max(start)..stop {
                let scaled = (i - start) as u64 * u64::from(track.rate);
                let n = (scaled / u64::from(rate)) as usize;
                if n >= wave.len() {
                    break;
                }

                let frac = (scaled % u64::from(rate)) as f32 / rate as f32;
                let a = f32::from(wave[n]);
                let b = f32::from(*wave.get(n + 1).unwrap_or(&wave[n]));
                let value = (a + (b - a) * frac) * gain;
                out[i * 2] += value * left;
                out[i * 2 + 1] += value * right;
            }
        }
        position = end;
    };

    for e in &track.sequence.events {
        if e.at > track.sequence.end {
            break;
        }
        span(e.at, &channels);
        let ch = &mut channels[e.channel];
        match e.kind {
            Kind::Wave(id, gate) => {
                ch.voice = Some(Voice {
                    id,
                    start: e.at,
                    gate,
                })
            }
            Kind::Volume(v) => ch.volume = v,
            Kind::Expression(v) => ch.expression = v,
            Kind::Pan(v) => ch.pan = v,
            Kind::Nop => {}
        }
    }

    span(track.sequence.end, &channels);
}

/// Decodifica somente perfis realmente implementados; nenhuma faixa é descartada em silêncio.

struct Parsed<'a> {
    tracks: Vec<Track<'a>>,
    scores: Vec<score::Track>,
    rate: u32,
    frames: usize,
}

fn parse(bytes: &[u8]) -> Result<Parsed<'_>, String> {
    if bytes.len() < 10 || bytes.len() > MAX_INPUT || bytes.get(..4) != Some(b"MMMD") {
        return Err(error("cabeçalho inválido"));
    }

    let length = be32(&bytes[4..8])?
        .checked_add(8)
        .ok_or_else(|| error("tamanho excedido"))?;

    if length != bytes.len() {
        return Err(error("tamanho do arquivo contradiz cabeçalho"));
    }

    let end = bytes.len() - 2;
    if crc(&bytes[..end]) != u16::from_be_bytes(bytes[end..].try_into().unwrap()) {
        return Err(error("CRC inválido"));
    }

    let (mut count, mut event_count, mut contents) = (0usize, 0usize, false);
    let mut ma2_profile = false;
    let mut tracks = Vec::new();
    let mut scores = Vec::new();

    for (id, body) in chunks(&bytes[8..end], &mut count)? {
        match &id {
            b"CNTI" if !contents && tracks.is_empty() && scores.is_empty() && body.len() >= 5 => {
                contents = true;
                // ContentsType do CNTI: mesmo limiar usado por parse_smaf_head da Qualcomm.
                ma2_profile = body[1] < 0x30;
            }

            b"OPDA" if contents => {}

            _ if &id[..3] == b"ATR" && contents => {
                if tracks.len() + scores.len() >= MAX_TRACKS {
                    return Err(error("limite de faixas"));
                }
                tracks.push(track(body, &mut count, &mut event_count)?);
            }

            _ if &id[..3] == b"MTR" && contents => {
                if tracks.len() + scores.len() >= MAX_TRACKS {
                    return Err(error("limite de faixas"));
                }
                scores.push(score::parse(body, &mut count, &mut event_count, ma2_profile)?);
            }

            _ => return Err(error("faixa ou metadado não suportado")),
        }
    }

    if !contents || (tracks.is_empty() && scores.is_empty()) {
        return Err(error("faixa de áudio ausente"));
    }
    score::validate(&scores)?;

    let rate = tracks
        .iter()
        .map(|t| t.rate)
        .chain((!scores.is_empty()).then_some(score::RATE))
        .max()
        .unwrap();
    let us = tracks
        .iter()
        .map(|t| t.sequence.end)
        .chain(scores.iter().map(score::Track::duration_us))
        .max()
        .unwrap();
    let frames = frame(us, rate);
    if frames == 0 || frames > 120 * rate as usize {
        return Err(error("duração inválida"));
    }
    Ok(Parsed {
        tracks,
        scores,
        rate,
        frames,
    })
}

/// Confere perfil e limites antes de aceitar uma fonte, sem decodificar PCM.

pub fn validate(bytes: &[u8]) -> Result<(), String> {
    parse(bytes).map(|_| ())
}

/// Mistura somente os perfis validados; presets FM usam timbres substitutos, sem ROM Yamaha.

pub fn decode(bytes: &[u8]) -> Result<Sound, String> {
    let Parsed {
        tracks,
        scores,
        rate,
        frames,
    } = parse(bytes)?;

    let mut samples = vec![0.0f32; frames * 2];
    for track in &tracks {
        render(track, rate, &mut samples);
    }
    if !scores.is_empty() {
        score::render(&scores, rate, &mut samples)?;
    }
    for v in &mut samples {
        *v = v.clamp(-1.0, 1.0);
    }

    Ok(Sound {
        rate,
        channels: 2,
        samples,
    })
}
