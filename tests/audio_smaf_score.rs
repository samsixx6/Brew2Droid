use zeebx::audio::{smaf, wav::Sound};
#[path = "common/smaf.rs"]
mod fixtures;
use fixtures::{chunk, seal};

// Sem bytes de jogo: dois operadores seno de razão 1:1, algoritmo aditivo e sem LFO.
// Decay zero/SL15 mantém a nota; RR15 torna o release curto para testar o gate.
fn sine(bank: u8, program: u8, multiple: u8) -> Vec<u8> {
    let mut p = vec![0x43, 0x03, 0x00, bank, program, 0x01, 0x01];
    for level in [63, 8] {
        p.extend([multiple << 4, 0xf0, 0xff, level << 2, 0x00]);
    }
    p
}
fn exclusive(p: &[u8]) -> Vec<u8> {
    assert!(p.len() < 127);
    [vec![0xff, 0xf0, p.len() as u8 + 1], p.to_vec(), vec![0xf7]].concat()
}
fn v(n: u16) -> Vec<u8> {
    assert!(n <= 16511);
    if n < 128 {
        vec![n as u8]
    } else {
        let n = n - 128;
        vec![0x80 | (n >> 7) as u8, (n & 127) as u8]
    }
}
fn event(seq: &mut Vec<u8>, delta: u16, bytes: &[u8]) {
    seq.extend(v(delta));
    seq.extend(bytes);
}
fn note(seq: &mut Vec<u8>, delta: u16, channel: u8, code: u8, gate: u16) {
    let mut data = vec![channel << 6 | code];
    data.extend(v(gate));
    event(seq, delta, &data);
}
fn control(seq: &mut Vec<u8>, delta: u16, channel: u8, control: u8, value: u8) {
    event(seq, delta, &[0, channel << 6 | control, value]);
}
fn end(seq: &mut Vec<u8>, delta: u16) {
    event(seq, delta, &[0xff, 0]);
    seq.extend([0, 0, 0, 0]);
}
fn score(id: u8, seq: &[u8], setup: &[u8], base: u8, phrase: Option<&[u8]>) -> Vec<u8> {
    let mut body = vec![0, 0, base, base, 0, 0];
    if let Some(phrase) = phrase {
        body.extend(chunk(*b"MspI", phrase));
    }
    if !setup.is_empty() {
        body.extend(chunk(*b"Mtsu", setup));
    }
    body.extend(chunk(*b"Mtsq", seq));
    chunk([b'M', b'T', b'R', id], &body)
}
fn file(tracks: &[Vec<u8>]) -> Vec<u8> {
    let mut b = chunk(*b"CNTI", &[0, 0, 0, 0, 0]);
    for track in tracks {
        b.extend(track);
    }
    seal(b)
}
fn single(seq: &[u8], patches: &[Vec<u8>], base: u8) -> Vec<u8> {
    let setup = patches
        .iter()
        .flat_map(|p| exclusive(p))
        .collect::<Vec<_>>();
    file(&[score(0, seq, &setup, base, None)])
}
fn tone(pan: u8, volume: u8, expression: u8) -> Vec<u8> {
    let mut seq = Vec::new();
    control(&mut seq, 0, 0, 0x3a, pan);
    control(&mut seq, 0, 0, 0x37, volume);
    control(&mut seq, 0, 0, 0x3b, expression);
    note(&mut seq, 0, 0, 0x29, 1000);
    end(&mut seq, 1000);
    single(&seq, &[sine(0, 0, 1)], 0)
}
fn energy(s: &Sound, from: usize, to: usize, channel: usize) -> f64 {
    s.samples[from * 2..to * 2]
        .chunks_exact(2)
        .map(|p| f64::from(p[channel]).powi(2))
        .sum()
}
fn frequency(s: &Sound, channel: usize, from: usize, to: usize) -> f64 {
    let samples = &s.samples[from * 2..to * 2];
    let mut crossings = Vec::new();
    for i in 1..samples.len() / 2 {
        let a = samples[(i - 1) * 2 + channel];
        let b = samples[i * 2 + channel];
        if a <= 0.0 && b > 0.0 {
            crossings.push((i - 1) as f64 + f64::from(-a) / f64::from(b - a));
        }
    }
    assert!(
        crossings.len() > 20,
        "PCM precisa conter ciclos reais, não um marcador"
    );
    (crossings.len() - 1) as f64 * s.rate as f64 / (crossings.last().unwrap() - crossings[0])
}
fn reject(bytes: Vec<u8>) {
    assert!(
        smaf::validate(&bytes).is_err(),
        "validate aceitou entrada não suportada"
    );
    assert!(
        smaf::decode(&bytes).is_err(),
        "decode aceitou entrada não suportada"
    );
}

#[test]
fn handy_phone_mid_high_a_is_440_hz_from_a_synthetic_vma_sine() {
    let b = tone(0, 127, 127);
    smaf::validate(&b).expect("partitura Handy Phone precisa ser reconhecida");
    let s = smaf::decode(&b).unwrap();
    assert_eq!((s.rate, s.channels, s.frames()), (22050, 2, 22050));
    assert!(s.samples.iter().all(|s| s.is_finite()));
    let hz = frequency(&s, 0, 2205, 19845);
    assert!((hz - 440.0).abs() < 2.0, "A4 deve ser 440Hz, medido {hz}");
    assert!(s.samples.iter().skip(1).step_by(2).all(|v| *v == 0.0));
}

#[test]
fn note_delay_and_eos_cut_are_sample_accurate_without_an_added_tail() {
    let mut seq = Vec::new();
    control(&mut seq, 0, 0, 0x3a, 0);
    note(&mut seq, 100, 0, 0x29, 1000);
    end(&mut seq, 200);
    let s = smaf::decode(&single(&seq, &[sine(0, 0, 1)], 0)).unwrap();
    assert_eq!(s.frames(), 6615);
    assert!(s.samples[..4410].iter().all(|v| *v == 0.0));
    assert!(energy(&s, 2400, 6400, 0) > 0.1);
    let mut other = Vec::new();
    event(&mut other, 400, &[0xff, 0]);
    end(&mut other, 0);
    let b = file(&[
        score(0, &seq, &exclusive(&sine(0, 0, 1)), 0, None),
        score(1, &other, &[], 0, None),
    ]);
    let s = smaf::decode(&b).unwrap();
    assert_eq!(s.frames(), 8820);
    assert!(
        s.samples[6615 * 2..].iter().all(|v| *v == 0.0),
        "EOS deve mutar a faixa"
    );
}

#[test]
fn volume_expression_and_constant_power_pan_control_the_real_pcm() {
    let full = smaf::decode(&tone(0, 127, 127)).unwrap();
    let quiet = smaf::decode(&tone(0, 64, 80)).unwrap();
    let right = smaf::decode(&tone(127, 127, 127)).unwrap();
    let center = smaf::decode(&tone(64, 127, 127)).unwrap();
    let expected_gain = (64.0f64 / 127.0).powi(2) * (80.0f64 / 127.0).powi(2);
    let measured_gain = (energy(&quiet, 2205, 19845, 0) / energy(&full, 2205, 19845, 0)).sqrt();
    assert!((measured_gain - expected_gain).abs() < 0.0001);
    assert!(right.samples.iter().step_by(2).all(|v| *v == 0.0));
    assert!((energy(&right, 2205, 19845, 1) / energy(&full, 2205, 19845, 0) - 1.0).abs() < 0.0001);
    let total = energy(&center, 2205, 19845, 0) + energy(&center, 2205, 19845, 1);
    assert!((total / energy(&full, 2205, 19845, 0) - 1.0).abs() < 0.0001);
    let muted = smaf::decode(&tone(64, 0, 127)).unwrap();
    assert!(muted.samples.iter().all(|v| *v == 0.0));
}

#[test]
fn setup_in_one_track_is_global_and_its_eos_does_not_mute_another_track() {
    let mut first = Vec::new();
    event(&mut first, 10, &[0xff, 0]);
    end(&mut first, 0);
    let mut second = Vec::new();
    control(&mut second, 0, 0, 0x31, 5);
    control(&mut second, 0, 0, 0x30, 44);
    control(&mut second, 0, 0, 0x3a, 0);
    note(&mut second, 0, 0, 0x29, 500);
    end(&mut second, 500);
    let setup = exclusive(&sine(5, 44, 1));
    let b = file(&[
        score(0, &first, &setup, 0, None),
        score(1, &second, &[], 0, None),
    ]);
    smaf::validate(&b).unwrap();
    let s = smaf::decode(&b).unwrap();
    assert_eq!(s.frames(), 11025);
    assert!(
        energy(&s, 2205, 10000, 0) > 0.1,
        "EOS da primeira faixa não pode encerrar a segunda"
    );
    assert!((frequency(&s, 0, 2205, 10000) - 440.0).abs() < 2.0);
    let reversed = file(&[
        score(1, &second, &[], 0, None),
        score(0, &first, &setup, 0, None),
    ]);
    assert_eq!(s.samples, smaf::decode(&reversed).unwrap().samples);
}

#[test]
fn timed_instrument_and_octave_changes_affect_only_subsequent_notes() {
    let mut seq = Vec::new();
    control(&mut seq, 0, 0, 0x3a, 0);
    note(&mut seq, 0, 0, 0x29, 300);
    control(&mut seq, 300, 0, 0x30, 1);
    control(&mut seq, 0, 0, 0x32, 1);
    note(&mut seq, 0, 0, 0x29, 300);
    end(&mut seq, 300);
    let s = smaf::decode(&single(&seq, &[sine(0, 0, 1), sine(0, 1, 2)], 0)).unwrap();
    assert!((frequency(&s, 0, 1500, 5500) - 440.0).abs() < 2.0);
    // Novo program dobra o múltiplo do operador e shift+1 dobra a nota: 4×440.
    assert!((frequency(&s, 0, 8000, 12500) - 1760.0).abs() < 5.0);
}

#[test]
fn full_sequence_phrase_selection_is_supported_but_partial_seek_is_rejected() {
    let seq = [0, 0x29, 20, 20, 0xff, 0, 0, 0, 0, 0];
    let phrase = [
        b's', b't', b':', 0, 0, 0, 0, b',', b's', b'p', b':', 0, 0, 0, 6, b',',
    ];
    let b = file(&[score(0, &seq, &[], 0, Some(&phrase))]);
    smaf::validate(&b).unwrap();
    assert!(smaf::decode(&b).unwrap().samples.iter().any(|v| *v != 0.0));
    let mut partial = phrase;
    partial[14] = 3;
    reject(file(&[score(0, &seq, &[], 0, Some(&partial))]));
    let mut start = phrase;
    start[6] = 1;
    reject(file(&[score(0, &seq, &[], 0, Some(&start))]));
}

#[test]
fn neutral_pitch_modulation_and_short_expression_are_parsed_without_skipping() {
    let mut seq = Vec::new();
    control(&mut seq, 0, 0, 0x34, 0x40);
    control(&mut seq, 0, 0, 0x33, 0);
    event(&mut seq, 0, &[0, 0x18]);
    event(&mut seq, 0, &[0, 0x21]);
    control(&mut seq, 0, 0, 0x3a, 0);
    event(&mut seq, 0, &[0, 0x02]);
    note(&mut seq, 0, 0, 0x29, 500);
    end(&mut seq, 500);
    let a = smaf::decode(&single(&seq, &[sine(0, 0, 1)], 0)).unwrap();
    let mut standard = Vec::new();
    control(&mut standard, 0, 0, 0x3a, 0);
    control(&mut standard, 0, 0, 0x3b, 31);
    note(&mut standard, 0, 0, 0x29, 500);
    end(&mut standard, 500);
    assert_eq!(
        a.samples,
        smaf::decode(&single(&standard, &[sine(0, 0, 1)], 0))
            .unwrap()
            .samples
    );
}

#[test]
fn unknown_banks_and_unsupported_events_fail_before_audio_is_accepted() {
    for bytes in [
        vec![0, 0, 0x31, 5, 0, 0x29, 10],
        vec![0, 0, 0x34, 63, 0, 0x29, 10],
        vec![0, 0, 0x33, 1, 0, 0x29, 10],
        vec![0, 0, 0x35, 1],
        vec![0, 0, 0x12],
        vec![0, 0x20, 1],
        vec![0, 0x2d, 1],
        vec![0, 0x29, 0],
        vec![0, 0x29],
        vec![0x80, 0x80, 0x29, 1],
        vec![0, 0xff, 0x51, 0],
        vec![0, 0xff, 0xf0, 3, 1, 2, 0xf7],
    ] {
        reject(single(&bytes, &[], 0));
    }
    let mut unsupported_header = vec![2, 0, 0, 0, 0, 0];
    unsupported_header.extend(chunk(*b"Mtsq", &[0, 0x29, 1]));
    reject(file(&[chunk([b'M', b'T', b'R', 0], &unsupported_header)]));
    unsupported_header[0] = 0;
    unsupported_header[1] = 1;
    reject(file(&[chunk([b'M', b'T', b'R', 0], &unsupported_header)]));
}

#[test]
fn malformed_setup_sysex_crc_and_truncation_are_not_silent_success() {
    let b = tone(0, 127, 127);
    for n in 0..b.len() {
        reject(b[..n].to_vec());
    }
    let mut bad_crc = b.clone();
    bad_crc[20] ^= 1;
    reject(bad_crc);
    let seq = [0, 0x29, 10];
    for setup in [
        vec![0xff, 0xf0, 18, 0x43],
        vec![0xff, 0xf0, 1, 0xf6],
        vec![0xff, 0x00],
        exclusive(&[0x43, 3, 0, 0, 0, 1, 1]),
    ] {
        reject(file(&[score(0, &seq, &setup, 0, None)]));
    }
    let mut malformed = sine(0, 0, 1);
    malformed[6] = 0;
    reject(single(&seq, &[malformed], 0));
    let mut malformed = sine(0, 0, 1);
    malformed[1] = 4;
    reject(single(&seq, &[malformed], 0));
    let mut malformed = sine(0, 0, 1);
    malformed[4] = 128;
    reject(single(&seq, &[malformed], 0));
    let mut body = vec![0, 0, 0, 0, 0, 0];
    body.extend(chunk(*b"Mtsq", &seq));
    body.extend(chunk(*b"Mtsq", &seq));
    reject(file(&[chunk([b'M', b'T', b'R', 0], &body)]));
    let mut unknown = vec![0, 0, 0, 0, 0, 0];
    unknown.extend(chunk(*b"Mtsq", &seq));
    unknown.extend(chunk(*b"JUNK", &[]));
    reject(file(&[chunk([b'M', b'T', b'R', 0], &unknown)]));
}

#[test]
fn handy_phone_duration_and_global_track_event_chunk_limits_are_bounded() {
    let seq = [0x80, 0, 0x29, 1];
    let s = smaf::decode(&single(&seq, &[], 0)).unwrap();
    assert_eq!(
        s.frames(),
        2844,
        "duração=128 mais gate1, diferente de MIDI VLQ"
    );
    let tracks = (0..9)
        .map(|i| score(i, &[0, 0x29, 1], &[], 0, None))
        .collect::<Vec<_>>();
    reject(file(&tracks));
    let mut seq = Vec::new();
    for _ in 0..16385 {
        event(&mut seq, 0, &[0xff, 0]);
    }
    note(&mut seq, 0, 0, 0x29, 1);
    reject(single(&seq, &[], 0));
    let mut long = Vec::new();
    for _ in 0..3 {
        event(&mut long, 16511, &[0xff, 0]);
    }
    note(&mut long, 0, 0, 0x29, 1);
    reject(single(&long, &[], 2));
    let mut chunks = chunk(*b"CNTI", &[0, 0, 0, 0, 0]);
    for _ in 0..4096 {
        chunks.extend(chunk(*b"OPDA", &[]));
    }
    chunks.extend(score(0, &[0, 0x29, 1], &[], 0, None));
    reject(seal(chunks));
}

#[test]
fn gate_enters_native_release_and_becomes_silent_before_the_later_eos() {
    let mut seq = Vec::new();
    control(&mut seq, 0, 0, 0x3a, 0);
    note(&mut seq, 0, 0, 0x29, 100);
    end(&mut seq, 400);
    let s = smaf::decode(&single(&seq, &[sine(0, 0, 1)], 0)).unwrap();
    assert_eq!(s.frames(), 8820);
    assert!(energy(&s, 200, 2000, 0) > 0.1);
    assert!(
        s.samples[3000 * 2..].iter().all(|v| *v == 0.0),
        "release rápido deve terminar depois do gate de100ms"
    );
}

#[test]
fn a_stolen_voices_old_gate_cannot_stop_the_replacement_note() {
    let mut seq = Vec::new();
    control(&mut seq, 0, 0, 0x3a, 0);
    note(&mut seq, 0, 0, 0x29, 100);
    for _ in 0..47 {
        note(&mut seq, 0, 0, 0x29, 105);
    }
    note(&mut seq, 50, 0, 0x29, 300);
    end(&mut seq, 350);
    let s = smaf::decode(&single(&seq, &[sine(0, 0, 1)], 0)).unwrap();
    assert_eq!(s.frames(), 8820);
    assert!(
        energy(&s, 3300, 6500, 0) > 0.1,
        "gate antigo de100ms não pode cortar a nova voz que roubou seu slot aos50ms"
    );
    assert!(s.samples[8000 * 2..].iter().all(|v| *v == 0.0));
}

#[test]
fn inline_vma_is_timed_and_cannot_satisfy_notes_before_its_definition() {
    let mut seq = Vec::new();
    control(&mut seq, 0, 0, 0x31, 5);
    control(&mut seq, 0, 0, 0x30, 44);
    control(&mut seq, 0, 0, 0x3a, 0);
    event(&mut seq, 100, &exclusive(&sine(5, 44, 1)));
    note(&mut seq, 0, 0, 0x29, 200);
    end(&mut seq, 200);
    let s = smaf::decode(&single(&seq, &[], 0)).unwrap();
    assert!(s.samples[..4410].iter().all(|v| *v == 0.0));
    assert!(energy(&s, 2400, 6000, 0) > 0.1);
    let mut premature = Vec::new();
    control(&mut premature, 0, 0, 0x31, 5);
    control(&mut premature, 0, 0, 0x30, 44);
    note(&mut premature, 0, 0, 0x29, 100);
    event(&mut premature, 100, &exclusive(&sine(5, 44, 1)));
    end(&mut premature, 100);
    reject(single(&premature, &[], 0));
}

#[test]
fn qualcomm_ma2_sequence_91_preserves_pcm_and_absolute_event_time() {
    // Qualcomm audfmt.c, parse_smaf_hp_cmd, lê seis bytes incluindo F7;
    // apenas 43/03/90 escreve pitch. 43/03/91 termina com sucesso sem DSP.
    // A equivalência cobre o som e o tempo após o comando, não só validate.
    let mut reference = Vec::new();
    note(&mut reference, 0, 0, 0x29, 400);
    event(&mut reference, 100, &[0xff, 0]);
    end(&mut reference, 300);
    let reference = smaf::decode(&single(&reference, &[sine(0, 0, 1)], 0)).unwrap();
    assert!(energy(&reference, 1000, 7000, 0) > 0.1);
    for parameters in [[0x19, 0x32], [0x1e, 0x98], [0, 0], [255, 255]] {
        let mut seq = Vec::new();
        note(&mut seq, 0, 0, 0x29, 400);
        event(&mut seq, 100, &exclusive(&[0x43, 3, 0x91, parameters[0], parameters[1]]));
        end(&mut seq, 300);
        let b = single(&seq, &[sine(0, 0, 1)], 0);
        smaf::validate(&b).expect("comando 91 enquadrado deve seguir o parser Qualcomm MA2");
        let sound = smaf::decode(&b).unwrap();
        assert_eq!(sound.frames(), 8820);
        assert_eq!(sound.samples, reference.samples);
    }
}

#[test]
fn qualcomm_sequence_91_does_not_relax_setup_profile_or_other_exclusives() {
    let payload = [0x43, 3, 0x91, 0x19, 0x32];
    let mut seq = Vec::new();
    event(&mut seq, 0, &exclusive(&payload));
    note(&mut seq, 0, 0, 0x29, 10);
    let mut allowed = single(&seq, &[], 0);
    allowed[17] = 0x2f;
    let n = allowed.len();
    let checksum = fixtures::crc(&allowed[..n - 2]);
    allowed[n - 2..].copy_from_slice(&checksum.to_be_bytes());
    smaf::validate(&allowed).unwrap();
    allowed[17] = 0x30;
    let checksum = fixtures::crc(&allowed[..n - 2]);
    allowed[n - 2..].copy_from_slice(&checksum.to_be_bytes());
    reject(allowed);
    reject(single(&[0, 0x29, 10], &[payload.to_vec()], 0));
    for payload in [
        vec![0x43, 3, 0x90, 0x19, 0x32],
        vec![0x43, 3, 0x92, 0x19, 0x32],
        vec![0x44, 3, 0x91, 0x19, 0x32],
        vec![0x43, 4, 0x91, 0x19, 0x32],
        vec![0x43, 3, 0x91, 0x19],
        vec![0x43, 3, 0x91, 0x19, 0x32, 0],
    ] {
        let mut seq = vec![0];
        seq.extend(exclusive(&payload));
        reject(single(&seq, &[], 0));
    }
    for sysex in [
        vec![0xff, 0xf0, 0],
        vec![0xff, 0xf0, 6, 0x43, 3, 0x91, 0x19, 0x32, 0xf6],
        vec![0xff, 0xf0, 7, 0x43, 3, 0x91, 0x19, 0x32, 0xf7],
    ] {
        let mut seq = vec![0];
        seq.extend(sysex);
        reject(single(&seq, &[], 0));
    }
}

#[test]
fn four_operator_vma_uses_the_declared_operator_count_and_pitch() {
    let mut patch = sine(0, 0, 1);
    patch[5] = 2;
    patch.extend([0x10, 0xf0, 0xff, 0xfc, 0, 0x10, 0xf0, 0xff, 0xfc, 0]);
    let mut seq = Vec::new();
    control(&mut seq, 0, 0, 0x3a, 0);
    note(&mut seq, 0, 0, 0x29, 500);
    end(&mut seq, 500);
    let s = smaf::decode(&single(&seq, &[patch.clone()], 0)).unwrap();
    assert!((frequency(&s, 0, 2205, 10000) - 440.0).abs() < 2.0);
    patch.truncate(17);
    reject(single(&seq, &[patch], 0));
}

#[test]
fn mixing_score_with_atr_preserves_both_tracks_and_the_longer_duration() {
    let score_file = tone(0, 127, 127);
    let score_sound = smaf::decode(&score_file).unwrap();
    let mut score_seq = Vec::new();
    control(&mut score_seq, 0, 0, 0x3a, 0);
    note(&mut score_seq, 0, 0, 0x29, 1000);
    end(&mut score_seq, 1000);
    let atr_seq = [0, 0, 0x3a, 127, 0, 1, 100, 100, 0xff, 0, 0, 0, 0, 0];
    let b = file(&[
        score(0, &score_seq, &exclusive(&sine(0, 0, 1)), 0, None),
        fixtures::track(&atr_seq, &[0x77; 2000], 0x13, 0),
    ]);
    let mixed = smaf::decode(&b).unwrap();
    assert_eq!((mixed.rate, mixed.frames()), (22050, 22050));
    assert!(
        mixed
            .samples
            .iter()
            .step_by(2)
            .zip(score_sound.samples.iter().step_by(2))
            .all(|(a, b)| a == b)
    );
    assert!(
        energy(&mixed, 20, 2000, 1) > 0.1,
        "FM não pode sobrescrever PCM ATR já misturado"
    );
    assert!(
        mixed.samples[4410..]
            .iter()
            .skip(1)
            .step_by(2)
            .all(|v| *v == 0.0)
    );
}

#[test]
fn vma_header_one_binds_the_declared_bank_program_without_fallback() {
    // O byte privado não modifica o endereço Bank/PC nem os operadores.
    // Banco5 obriga a usar este VMA e impede sucesso por um preset aproximado.
    let mut seq = Vec::new();
    control(&mut seq, 0, 0, 0x31, 5);
    control(&mut seq, 0, 0, 0x30, 44);
    control(&mut seq, 0, 0, 0x3a, 0);
    note(&mut seq, 0, 0, 0x29, 500);
    end(&mut seq, 500);
    let zero = sine(5, 44, 1);
    let mut one = zero.clone();
    one[2] = 1;
    let baseline = smaf::decode(&single(&seq, &[zero], 0)).unwrap();
    let b = single(&seq, &[one], 0);
    smaf::validate(&b).expect("header1 observado no original deve manter Bank/PC e corpo VMA");
    let s = smaf::decode(&b).unwrap();
    assert_eq!(s.samples, baseline.samples);
    assert!((frequency(&s, 0, 2205, 10000) - 440.0).abs() < 2.0);
}

#[test]
fn vma_private_header_preserves_every_patch_bank_program_and_body() {
    // Banco5 não tem preset: o corpo de cada instrumento precisa chegar ao DSP.
    // Os valores2..5 aparecem nos quatro últimos VMA do MTR original de893bytes.
    for private in 2..=5 {
        let mut seq = Vec::new();
        control(&mut seq, 0, 0, 0x31, 5);
        control(&mut seq, 0, 0, 0x30, 44);
        control(&mut seq, 0, 0, 0x3a, 0);
        note(&mut seq, 0, 0, 0x29, 500);
        end(&mut seq, 500);
        let zero = sine(5, 44, 1);
        let mut patch = zero.clone();
        patch[2] = private;
        let expected = smaf::decode(&single(&seq, &[zero], 0)).unwrap();
        let bytes = single(&seq, &[patch], 0);
        smaf::validate(&bytes).expect("byte privado não é uma versão do formato VMA");
        let sound = smaf::decode(&bytes).unwrap();
        assert_eq!(sound.samples, expected.samples);
        assert!((frequency(&sound, 0, 2205, 10000) - 440.0).abs() < 2.0);
    }
}
