//! Partituras Handy Phone: eventos limitados e agenda absoluta, sem parser externo.
//!
//! SMAF 3.05 §4.4.1: A da oitava Mid High é 440 Hz, e o segundo byte
//! de duração soma 128 ao valor de 14 bits. Mtsu pertence ao setup do arquivo:
//! uma faixa pode usar um instrumento declarado por outra antes do primeiro som.

use super::fm::{Channel, Engine};
use super::{MAX_EVENTS, MAX_TRACKS, MAX_US, byte, chunks, error, timebase, variable};
use std::collections::BTreeSet;

pub(super) const RATE: u32 = 22_050;
const MAX_PATCHES: usize = 256;

#[derive(Debug)]
enum Kind {
    Note { pitch: u8, gate: u64 },
    Program(u8),
    Bank(u8),
    Octave(i16),
    Volume(u8),
    Expression(u8),
    Pan(u8),
    Patch(Vec<u8>),
    Ma2Sequence91,
    Nop,
}

#[derive(Debug)]
struct Event {
    at: u64,
    channel: usize,
    kind: Kind,
}

pub(super) struct Track {
    setup: Vec<Vec<u8>>,
    events: Vec<Event>,
    end: u64,
    rhythm: [bool; 4],
}

impl Track {
    pub(super) fn duration_us(&self) -> u64 {
        self.end
    }
}

fn count_event(count: &mut usize) -> Result<(), String> {
    *count = count
        .checked_add(1)
        .filter(|&n| n <= MAX_EVENTS)
        .ok_or_else(|| error("limite de eventos"))?;
    Ok(())
}

fn patch_key(payload: &[u8]) -> Result<(u8, u8), String> {
    // SMAF3.05 p38 identifica Maker/Format; o restante pertence ao dispositivo.
    // No VMA, go-smaf voice/vmapc.go lê Enigma/Bank/PC e o DSP usa Bank/PC
    // nos bytes3/4. O byte2 é opaco, não uma versão: o original contém0..5.
    if payload.len() < 7
        || payload[..2] != [0x43, 0x03]
        || payload[4] > 127
        || payload[6] != 1
    {
        return Err(error("instrumento VMA não suportado"));
    }
    let operators = if payload[5] & 7 < 2 { 2 } else { 4 };
    if payload.len() != 7 + operators * 5 {
        return Err(error("instrumento VMA truncado ou com dados excedentes"));
    }
    Ok((payload[3], payload[4]))
}

// O comprimento inclui F7; o DSP recebe somente o payload anterior ao terminador.
fn exclusive(b: &[u8], p: &mut usize) -> Result<Vec<u8>, String> {
    let size = usize::try_from(variable(b, p)?).map_err(|_| error("sysex excedido"))?;
    let end = p
        .checked_add(size)
        .filter(|&end| size != 0 && end <= b.len())
        .ok_or_else(|| error("sysex truncado"))?;
    let data = &b[*p..end];
    if data.last() != Some(&0xf7) {
        return Err(error("terminador sysex ausente"));
    }
    let payload = &data[..data.len() - 1];
    *p = end;
    Ok(payload.to_vec())
}

fn setup(b: &[u8], event_count: &mut usize) -> Result<Vec<Vec<u8>>, String> {
    if b.is_empty() {
        return Err(error("Mtsu vazio"));
    }
    let mut p = 0;
    let mut patches = Vec::new();
    while p < b.len() {
        count_event(event_count)?;
        if byte(b, &mut p)? != 0xff || byte(b, &mut p)? != 0xf0 {
            return Err(error("evento de setup não suportado"));
        }
        let payload = exclusive(b, &mut p)?;
        patch_key(&payload)?;
        patches.push(payload);
    }
    Ok(patches)
}

fn sequence(
    b: &[u8],
    duration: u64,
    gate: u64,
    count: &mut usize,
    ma2_profile: bool,
) -> Result<(Vec<Event>, u64, usize), String> {
    if b.is_empty() {
        return Err(error("Mtsq vazio"));
    }
    let (mut p, mut now, mut tail) = (0usize, 0u64, 0u64);
    let mut events = Vec::new();
    while p < b.len() {
        if b.get(p..p + 4) == Some(&[0, 0, 0, 0]) {
            if b[p + 4..].iter().any(|&v| v != 0) {
                return Err(error("dados após EOS"));
            }
            return Ok((events, now, p));
        }
        count_event(count)?;
        let delta = variable(b, &mut p)?
            .checked_mul(duration)
            .ok_or_else(|| error("duração excedida"))?;
        now = now
            .checked_add(delta)
            .filter(|&n| n <= MAX_US)
            .ok_or_else(|| error("sequência excede 120 segundos"))?;
        let op = byte(b, &mut p)?;
        let mut channel = usize::from(op >> 6);
        let kind = match op {
            0 => {
                let control = byte(b, &mut p)?;
                channel = usize::from(control >> 6);
                match control & 63 {
                    1..=14 => Kind::Expression(
                        [0, 0, 31, 39, 47, 55, 63, 71, 79, 87, 95, 103, 111, 119, 127]
                            [usize::from(control & 63)],
                    ),
                    0x18 | 0x21 => Kind::Nop, // Pitch central e modulação zero, tipos curtos.
                    0x30 => Kind::Program(seven_bit(b, &mut p)?),
                    0x31 => Kind::Bank(byte(b, &mut p)?),
                    0x32 => {
                        let value = byte(b, &mut p)?;
                        let shift = match value {
                            0..=4 => i16::from(value) * 12,
                            0x81..=0x84 => -i16::from(value - 0x80) * 12,
                            _ => return Err(error("octave shift reservado")),
                        };
                        Kind::Octave(shift)
                    }
                    0x33 => {
                        if byte(b, &mut p)? != 0 {
                            return Err(error("modulação não neutra não suportada"));
                        }
                        Kind::Nop
                    }
                    0x34 => {
                        if byte(b, &mut p)? != 0x40 {
                            return Err(error("pitch bend não neutro não suportado"));
                        }
                        Kind::Nop
                    }
                    0x37 => Kind::Volume(seven_bit(b, &mut p)?),
                    0x3a => Kind::Pan(seven_bit(b, &mut p)?),
                    0x3b => Kind::Expression(seven_bit(b, &mut p)?),
                    _ => return Err(error("controle MTR não suportado")),
                }
            }
            0xff => {
                channel = 0;
                match byte(b, &mut p)? {
                    0 => Kind::Nop,
                    0xf0 => {
                        let payload = exclusive(b, &mut p)?;
                        // Qualcomm audfmt.c, parse_smaf_hp_cmd, consome os seis
                        // bytes de 43/03/91 incluindo F7 sem enviar comando ao DSP.
                        // É compatibilidade MA2 do BREW; não atribui semântica
                        // de registrador Yamaha nem estende a regra ao Mtsu.
                        if ma2_profile && payload.len() == 5 && payload[..3] == [0x43, 3, 0x91] {
                            Kind::Ma2Sequence91
                        } else {
                            patch_key(&payload)?;
                            Kind::Patch(payload)
                        }
                    }
                    _ => return Err(error("evento exclusivo MTR não suportado")),
                }
            }
            _ => {
                let note = op & 15;
                if !(1..=12).contains(&note) {
                    return Err(error("nota reservada"));
                }
                let length = variable(b, &mut p)?
                    .checked_mul(gate)
                    .filter(|&n| n != 0)
                    .ok_or_else(|| error("gate inválido"))?;
                let stop = now
                    .checked_add(length)
                    .filter(|&n| n <= MAX_US)
                    .ok_or_else(|| error("gate excede 120 segundos"))?;
                tail = tail.max(stop);
                Kind::Note {
                    pitch: 36 + ((op >> 4) & 3) * 12 + note,
                    gate: length,
                }
            }
        };
        events.push(Event {
            at: now,
            channel,
            kind,
        });
    }
    Ok((events, now.max(tail), b.len()))
}

fn seven_bit(b: &[u8], p: &mut usize) -> Result<u8, String> {
    let value = byte(b, p)?;
    if value > 127 {
        return Err(error("controle fora da faixa"));
    }
    Ok(value)
}

pub(super) fn parse(
    b: &[u8],
    count: &mut usize,
    event_count: &mut usize,
    ma2_profile: bool,
) -> Result<Track, String> {
    let h = b.get(..6).ok_or_else(|| error("cabeçalho MTR truncado"))?;
    if h[0] != 0 || h[1] != 0 {
        return Err(error("perfil MTR não suportado"));
    }
    let (duration, gate) = (timebase(h[2])?, timebase(h[3])?);
    let (mut seq, mut patches, mut phrase) = (None, None, None);
    for (id, body) in chunks(&b[6..], count)? {
        match &id {
            b"Mtsq" => {
                if seq.is_some() {
                    return Err(error("Mtsq duplicado"));
                }
                seq = Some(sequence(body, duration, gate, event_count, ma2_profile)?);
            }
            b"Mtsu" => {
                if patches.is_some() {
                    return Err(error("Mtsu duplicado"));
                }
                patches = Some(setup(body, event_count)?);
            }
            b"MspI" => {
                if phrase.replace(body).is_some() {
                    return Err(error("MspI duplicado"));
                }
            }
            _ => return Err(error("chunk MTR não suportado")),
        }
    }
    let (events, end, eos_offset) = seq.ok_or_else(|| error("Mtsq ausente"))?;
    if let Some(p) = phrase {
        if p.len() != 16
            || &p[..3] != b"st:"
            || &p[7..11] != b",sp:"
            || p[15] != b','
            || p[3..7] != [0, 0, 0, 0]
            || u32::from_be_bytes(p[11..15].try_into().unwrap()) as usize != eos_offset
        {
            return Err(error("seleção parcial de frases não suportada"));
        }
    }
    let rhythm = std::array::from_fn(|ch| ((h[4 + ch / 2] >> (4 * (1 - ch % 2))) & 3) == 3);
    Ok(Track {
        setup: patches.unwrap_or_default(),
        events,
        end,
        rhythm,
    })
}

#[derive(Clone, Copy)]
struct State {
    bank: u8,
    program: u8,
    octave: i16,
    volume: u8,
    expression: u8,
    pan: u8,
}
impl Default for State {
    fn default() -> Self {
        Self {
            bank: 0,
            program: 0,
            octave: 0,
            volume: 127,
            expression: 127,
            pan: 64,
        }
    }
}
impl State {
    fn channel(self) -> Channel {
        let angle = f32::from(self.pan) * std::f32::consts::FRAC_PI_2 / 127.0;
        Channel {
            gain: (f32::from(self.volume) / 127.0).powi(2)
                * (f32::from(self.expression) / 127.0).powi(2),
            left: if self.pan == 127 { 0.0 } else { angle.cos() },
            right: if self.pan == 0 { 0.0 } else { angle.sin() },
        }
    }
    fn apply(&mut self, kind: &Kind) {
        match *kind {
            Kind::Program(v) => self.program = v,
            Kind::Bank(v) => self.bank = v,
            Kind::Octave(v) => self.octave = v,
            Kind::Volume(v) => self.volume = v,
            Kind::Expression(v) => self.expression = v,
            Kind::Pan(v) => self.pan = v,
            _ => {}
        }
    }
    fn note(self, pitch: u8) -> Result<u8, String> {
        let pitch = i16::from(pitch) + self.octave;
        u8::try_from(pitch)
            .ok()
            .filter(|&pitch| pitch <= 127)
            .ok_or_else(|| error("nota após octave shift fora da faixa"))
    }
}

#[derive(Clone, Copy)]
enum Action<'a> {
    Event(&'a Event, u32),
    Gate(u32),
    End,
}
struct Scheduled<'a> {
    at: u64,
    track: usize,
    order: usize,
    action: Action<'a>,
}
fn schedule(tracks: &[Track]) -> Result<Vec<Scheduled<'_>>, String> {
    if tracks.len() > MAX_TRACKS {
        return Err(error("limite de faixas"));
    }
    let mut result = Vec::new();
    let (mut order, mut token) = (0usize, 0u32);
    for (track, t) in tracks.iter().enumerate() {
        for e in &t.events {
            token = token
                .checked_add(1)
                .ok_or_else(|| error("identidade de nota excedida"))?;
            result.push(Scheduled {
                at: e.at,
                track,
                order,
                action: Action::Event(e, token),
            });
            if let Kind::Note { gate, .. } = e.kind {
                let stop = e.at + gate;
                if stop <= t.end {
                    result.push(Scheduled {
                        at: stop,
                        track,
                        order,
                        action: Action::Gate(token),
                    });
                }
            }
            order += 1;
        }
        result.push(Scheduled {
            at: t.end,
            track,
            order,
            action: Action::End,
        });
        order += 1;
    }
    // Gate antes de novos eventos libera vozes; EOS vem depois dos últimos eventos
    // da própria faixa. O token permanece exclusivo mesmo quando o DSP rouba uma voz.
    result.sort_by_key(|e| {
        (
            e.at,
            match e.action {
                Action::Gate(_) => 0,
                Action::Event(..) => 1,
                Action::End => 2,
            },
            e.order,
        )
    });
    Ok(result)
}

fn register(patches: &mut BTreeSet<(u8, u8)>, payload: &[u8]) -> Result<(), String> {
    patches.insert(patch_key(payload)?);
    if patches.len() > MAX_PATCHES {
        return Err(error("limite de instrumentos"));
    }
    Ok(())
}

pub(super) fn validate(tracks: &[Track]) -> Result<(), String> {
    let agenda = schedule(tracks)?;
    let mut patches = BTreeSet::new();
    for t in tracks {
        for p in &t.setup {
            register(&mut patches, p)?;
        }
    }
    let mut state = [State::default(); 32];
    for entry in agenda {
        if let Action::Event(e, _) = entry.action {
            let ch = &mut state[entry.track * 4 + e.channel];
            ch.apply(&e.kind);
            match &e.kind {
                Kind::Patch(p) => register(&mut patches, p)?,
                Kind::Note { pitch, .. } => {
                    ch.note(*pitch)?;
                    if ch.bank != 0 && ch.bank != 128 && !patches.contains(&(ch.bank, ch.program)) {
                        return Err(error("banco/programa sem instrumento VMA"));
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn frame(us: u64, rate: u32) -> usize {
    (us * u64::from(rate) / 1_000_000) as usize
}

pub(super) fn render(tracks: &[Track], rate: u32, output: &mut [f32]) -> Result<(), String> {
    validate(tracks)?;
    if output.len() % 2 != 0 {
        return Err(error("buffer FM estéreo inválido"));
    }
    if tracks.is_empty() {
        return Ok(());
    }
    let mut engine = Engine::new(rate)?;
    for track in tracks {
        for patch in &track.setup {
            engine.set_patch(patch)?;
        }
    }
    let mut state = [State::default(); 32];
    let mut channels = [State::default().channel(); 32];
    let mut position = 0;
    for entry in schedule(tracks)? {
        let end = frame(entry.at, rate).min(output.len() / 2);
        if end > position {
            engine.render(
                end - position,
                &channels,
                &mut output[position * 2..end * 2],
            )?;
            position = end;
        }
        match entry.action {
            Action::Event(e, token) => {
                let channel = entry.track * 4 + e.channel;
                state[channel].apply(&e.kind);
                channels[channel] = state[channel].channel();
                match &e.kind {
                    Kind::Patch(payload) => engine.set_patch(payload)?,
                    Kind::Note { pitch, .. } => {
                        let s = state[channel];
                        engine.note_on(
                            token,
                            channel as u32,
                            s.bank,
                            s.program,
                            tracks[entry.track].rhythm[e.channel] || s.bank & 0x80 != 0,
                            s.note(*pitch)?,
                        )?;
                    }
                    _ => {}
                }
            }
            Action::Gate(token) => engine.note_off(token),
            Action::End => engine.mute_channels(entry.track as u32 * 4, 4),
        }
    }
    Ok(())
}
