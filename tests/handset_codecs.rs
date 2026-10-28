use zeebx::audio::{cmf, qcp};

fn qcp_file(packets: &[u8]) -> Vec<u8> {
    let mut fmt = vec![1, 0];
    fmt.extend([
        0x41, 0x6d, 0x7f, 0x5e, 0x15, 0xb1, 0xd0, 0x11, 0xba, 0x91, 0, 0x80, 0x5f, 0xb4, 0xb9, 0x7e,
    ]);
    fmt.resize(150, 0);
    fmt[104..106].copy_from_slice(&160u16.to_le_bytes());
    fmt[106..108].copy_from_slice(&8000u16.to_le_bytes());
    let mut bytes = b"RIFF\0\0\0\0QLCMfmt ".to_vec();
    bytes.extend((fmt.len() as u32).to_le_bytes());
    bytes.extend(fmt);
    bytes.extend(b"data");
    bytes.extend((packets.len() as u32).to_le_bytes());
    bytes.extend(packets);
    let len = bytes.len() as u32 - 8;
    bytes[4..8].copy_from_slice(&len.to_le_bytes());
    bytes
}

#[test]
fn qcp_blank_packets_have_twenty_milliseconds_of_finite_pcm_each() {
    let sound = qcp::decode(&qcp_file(&[0; 7])).unwrap();
    assert_eq!(
        (sound.rate, sound.channels, sound.frames()),
        (8000, 1, 1120)
    );
    assert!(
        sound
            .samples
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 1.0)
    );
}

#[test]
fn qcp_rejects_unknown_codec_and_incomplete_packet_without_fake_audio() {
    let mut bytes = qcp_file(&[0]);
    bytes[22] ^= 1;
    assert!(qcp::decode(&bytes).is_err());
    assert!(qcp::decode(&qcp_file(&[4, 1, 2])).is_err());
    let mut bytes = qcp_file(&[0]);
    bytes.truncate(bytes.len() - 1);
    assert!(qcp::decode(&bytes).is_err());
}

#[test]
fn qcp_ignores_bytes_beyond_the_validated_riff_extent() {
    let mut bytes = qcp_file(&[0; 3]);
    bytes.extend([0xaa; 4]);
    assert_eq!(qcp::decode(&bytes).unwrap().frames(), 480);
}

fn cmf_file(events: &[u8]) -> Vec<u8> {
    let mut bytes = b"cmid\0\0\0\0\0\x03\x02\x02\x01trac".to_vec();
    bytes.extend((events.len() as u32).to_be_bytes());
    bytes.extend(events);
    let len = bytes.len() as u32 - 8;
    bytes[4..8].copy_from_slice(&len.to_be_bytes());
    bytes
}

fn cmf_song(tracks: &[&[u8]]) -> Vec<u8> {
    let mut bytes = b"cmid\0\0\0\0\0\x0b\x02\x01".to_vec();
    bytes.push(tracks.len() as u8);
    bytes.extend(b"note\0\x02\0\x01");
    for track in tracks {
        bytes.extend(b"trac");
        bytes.extend((track.len() as u32).to_be_bytes());
        bytes.extend(*track);
    }
    let len = bytes.len() as u32 - 8;
    bytes[4..8].copy_from_slice(&len.to_be_bytes());
    bytes
}

#[test]
fn cmf_song_decodes_three_tracks_with_shared_tempo_and_real_percussion() {
    // Tick de 5 ms (base 96, 125 BPM); a terceira faixa usa o banco de bateria.
    // O baixo em C3 começa aos 100 ms, junto com C4 e o bumbo GM 36.
    let bytes = cmf_song(&[
        &[0, 255, 196, 125, 0, 255, 225, 2, 0, 255, 224, 32,
          20, 15, 40, 255, 40, 255, 223, 0],
        &[0, 255, 225, 2, 20, 15, 40, 252, 40, 255, 223, 0],
        &[0, 255, 225, 63, 20, 3, 40, 255, 40, 255, 223, 0],
    ]);
    let sound = cmf::decode(&bytes).expect("música CMX compacta deve tocar");
    assert_eq!((sound.rate, sound.channels), (22_050, 1));
    let start = 2205;
    assert!(sound.samples[..start].iter().all(|v| *v == 0.0));
    assert!(sound.samples[start..start + 2205].iter().any(|v| v.abs() > 0.01));
    assert!(sound.samples.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
    let duration_us = sound.frames() as i64 * 1_000_000 / i64::from(sound.rate);
    assert!((duration_us - 800_000).abs() < 100);
}

#[test]
fn cmf_song_worker_produces_pcm_instead_of_a_codec_error() {
    use zeebx::audio::decode::{DecodeSettings, DecodeWorker};
    let bytes = cmf_song(&[&[0, 24, 20, 252, 20, 255, 223, 0]]);
    let mut worker = DecodeWorker::new(DecodeSettings::default()).unwrap();
    worker.submit(72, bytes).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let result = loop {
        if let Some(result) = worker.try_recv() { break result; }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    assert!(result.midi, "o diagnóstico deve identificar a síntese CMX");
    assert!(result.sound.unwrap().samples.iter().any(|v| v.abs() > 0.01));
}

#[test]
fn cmf_song_drum_program_change_does_not_replace_the_melodic_program() {
    let expected = cmf_song(&[&[0, 255, 224, 4, 0, 255, 225, 63,
        0, 255, 225, 2, 0, 24, 20, 252, 20, 255, 223, 0]]);
    let ignored_program = cmf_song(&[&[0, 255, 224, 4, 0, 255, 225, 63,
        0, 255, 224, 23, 0, 255, 225, 2, 0, 24, 20, 252, 20, 255, 223, 0]]);
    assert!(cmf::decode(&ignored_program).unwrap().samples ==
        cmf::decode(&expected).unwrap().samples,
        "programa na bateria não pode mudar o instrumento melódico");
}

#[test]
fn cmf_song_rejects_truncation_invalid_notes_and_unbounded_timing() {
    for track in [
        vec![0, 24],
        vec![0, 63, 20, 252, 20, 255, 223, 0],
        vec![0, 255, 192, 19, 0, 24, 20, 252, 20, 255, 223, 0],
        vec![0, 255, 225, 17, 0, 24, 20, 252, 20, 255, 223, 0],
    ] {
        assert!(cmf::decode(&cmf_song(&[&track])).is_err());
    }
    let mut excessive = Vec::new();
    for _ in 0..16_385 { excessive.extend([0, 255, 222, 0]); }
    excessive.extend([0, 255, 223, 0]);
    assert!(cmf::decode(&cmf_song(&[&excessive])).is_err());
    let mut long = Vec::new();
    for _ in 0..140 { long.extend([255, 255, 222, 0]); }
    long.extend([0, 24, 20, 252, 20, 255, 223, 0]);
    assert!(cmf::decode(&cmf_song(&[&long])).is_err());
    let mut truncated = cmf_song(&[&[0, 24, 20, 252, 20, 255, 223, 0]]);
    truncated.pop();
    assert!(cmf::decode(&truncated).is_err());
    let mut bad_extent = cmf_song(&[&[0, 24, 20, 252, 20, 255, 223, 0]]);
    bad_extent[25..29].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(cmf::decode(&bad_extent).is_err());
}

#[cfg(feature = "soundfont")]
#[test]
#[ignore = "requer banco externo em BREW_CMX_SOUNDFONT"]
fn cmf_song_uses_the_selected_legacy_bank_in_the_background_worker() {
    use zeebx::audio::{decode::{DecodeSettings, DecodeWorker}, soundfont};
    let path = std::path::PathBuf::from(std::env::var_os("BREW_CMX_SOUNDFONT").unwrap());
    let bank = soundfont::abre(&path).expect("banco externo válido");
    let bytes = match std::env::var_os("BREW_CMX_MUSIC") {
        Some(path) => std::fs::read(path).unwrap(),
        None => cmf_song(&[&[0, 255, 225, 2, 0, 255, 224, 33,
            0, 7, 40, 255, 40, 255, 223, 0]]),
    };
    let mut worker = DecodeWorker::new(DecodeSettings { bank: Some(bank) }).unwrap();
    worker.submit(99, bytes).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let result = loop {
        if let Some(result) = worker.try_recv() { break result; }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    assert!(result.midi && result.soundfont);
    let elapsed_ms = result.elapsed_ms;
    let sound = result.sound.unwrap();
    assert_eq!(sound.rate, soundfont::TAXA_BANCO);
    assert!(sound.samples.iter().any(|v| v.abs() > 0.001));
    assert!(sound.samples.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
    eprintln!("CMX + banco: {} quadros a {}Hz; {:.2}ms de síntese", sound.frames(), sound.rate, elapsed_ms);
}

#[test]
fn cmf_qcelp_starts_at_delta_time_and_keeps_the_wave_tail_after_end_track() {
    // Um quadro QCELP em 10ms, seguido imediatamente por fim da partitura.
    let bytes = cmf_file(&[
        1, 0xff, 0xf1, 0, 8, 0, 0x44, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xdf, 0,
    ]);
    let sound = cmf::decode(&bytes).unwrap();
    assert_eq!((sound.rate, sound.channels, sound.frames()), (8000, 2, 240));
    assert!(
        sound
            .samples
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 1.0)
    );
}

#[test]
fn cmf_refuses_truncated_payload_and_unsupported_events() {
    assert!(cmf::decode(&cmf_file(&[0, 0xff, 0xf1, 0, 40, 0])).is_err());
    assert!(cmf::decode(&cmf_file(&[0, 0xff, 0xaa, 0])).is_err());
}

#[test]
fn cmf_recycle_events_cannot_accumulate_unbounded_pcm_or_render_work() {
    let mut events = vec![0, 0xff, 0xf1, 0, 8, 0, 0x44, 0, 0, 0, 0, 0, 0];
    for _ in 0..25_000 {
        events.extend([0, 0xff, 0xf1, 0, 7, 0, 0x84, 0, 0, 0, 0, 0]);
    }
    events.extend([0, 0xff, 0xdf, 0]);
    assert!(cmf::decode(&cmf_file(&events)).is_err());
}

#[test]
#[ignore = "requer os vetores públicos FATE em BREW_QCELP_FATE"]
fn qcelp_matches_the_independent_ffmpeg_fate_reference() {
    let root =
        std::path::PathBuf::from(std::env::var_os("BREW_QCELP_FATE").expect("BREW_QCELP_FATE"));
    let input = std::fs::read(root.join("0036580847.QCP")).unwrap();
    let reference = std::fs::read(root.join("0036580847.pcm")).unwrap();
    let sound = qcp::decode(&input).unwrap();
    let expected: Vec<i16> = reference
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect();
    assert_eq!(sound.samples.len(), expected.len());
    let maximum = sound
        .samples
        .iter()
        .zip(expected)
        .map(|(a, b)| ((a * 32768.0).round().clamp(-32768.0, 32767.0) as i32 - i32::from(b)).abs())
        .max()
        .unwrap();
    assert!(maximum <= 1, "maior diferença PCM: {maximum}");
}


#[test]
fn cmf_song_over_two_minutes_stays_within_the_existing_midi_duration_budget() {
    // A trilha CMF real do FF6 tem 146,130s; não copiar o recurso proprietário.
    let mut track=Vec::new();
    for _ in 0..57 {track.extend([255,255,222,0]);}
    track.extend([78,24,20,252,20,255,223,0]);
    let sound=cmf::decode(&cmf_song(&[&track])).expect("146 segundos cabem no sintetizador de 300s");
    assert_eq!(sound.rate,22_050);
    assert!(sound.frames()>146*sound.rate as usize);
    assert!(sound.samples.iter().rev().take(sound.rate as usize).any(|n|n.abs()>0.01));
    assert!(sound.samples.iter().all(|n|n.is_finite()&&n.abs()<=1.0));
}
