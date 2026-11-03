use zeebx::audio::smaf;
#[path = "common/smaf.rs"]
mod fixtures;
use fixtures::*;

#[test]
fn atr_yamaha_vector_uses_low_nibble_first_and_independent_wave_state() {
    let seq = left(&[0, 1, 1, 1, 1, 1, 1, 0xff, 0, 0, 0, 0, 0]);
    let s = smaf::decode(&file(&seq, &[0x70])).unwrap();
    assert_eq!((s.rate, s.channels, s.frames()), (8000, 2, 16));
    assert_eq!([pcm(s.samples[0]), pcm(s.samples[2])], [15, 253]);
    assert_eq!(
        [pcm(s.samples[16]), pcm(s.samples[18])],
        [15, 253],
        "retrigger resets predictor"
    );
    assert!(s.samples.iter().skip(1).step_by(2).all(|x| *x == 0.0));
}
#[test]
fn atr_respects_start_delay_gate_and_eos() {
    let seq = left(&[2, 1, 1, 2, 0xff, 0, 0, 0, 0, 0]);
    let s = smaf::decode(&file(&seq, &[0x77; 64])).unwrap();
    assert_eq!(s.frames(), 32);
    assert!(s.samples[..32].iter().all(|v| *v == 0.0));
    assert!(s.samples[32..48].iter().any(|v| *v != 0.0));
    assert!(s.samples[48..].iter().all(|v| *v == 0.0));
    let cut = left(&[0, 1, 20, 1, 0xff, 0, 0, 0, 0, 0]);
    let s = smaf::decode(&file(&cut, &[0x77; 64])).unwrap();
    assert_eq!(s.frames(), 8);
}
#[test]
fn medium_duration_and_eos_are_not_midi_variable_length() {
    let seq = left(&[0x80, 0, 1, 1, 1, 0xff, 0, 0, 0, 0, 0]);
    let s = smaf::decode(&file(&seq, &[0x70])).unwrap();
    assert_eq!(s.frames(), 129 * 8);
    assert!(s.samples[..128 * 16].iter().all(|v| *v == 0.0));
    assert_eq!(pcm(s.samples[128 * 16]), 15);
}
#[test]
fn sequence_volume_changes_apply_during_wave_and_channels_overlap() {
    let seq = left(&[0, 1, 4, 0, 0x41, 4, 1, 0, 0x37, 0, 1, 0xff, 0, 0, 0, 0, 0]);
    let s = smaf::decode(&file(&seq, &[0x77; 64])).unwrap();
    assert!(s.samples[..16].iter().any(|v| *v != 0.0));
    assert!(
        s.samples[17..].iter().step_by(2).any(|v| *v != 0.0),
        "other channel remains audible"
    );
}
#[test]
fn corrupt_crc_truncated_chunks_and_trailing_bytes_are_rejected() {
    let b = file(&[0, 1, 1, 1, 0xff, 0, 0, 0, 0, 0], &[0x70]);
    for n in 0..b.len() {
        assert!(smaf::decode(&b[..n]).is_err(), "prefix {n}");
    }
    let mut corrupt = b.clone();
    corrupt[20] ^= 1;
    assert!(smaf::decode(&corrupt).is_err());
    let mut trailing = b.clone();
    trailing.push(0);
    assert!(smaf::decode(&trailing).is_err());
    let mut corrupt_chunk = chunk(*b"CNTI", &[0, 1, 2, 0, 0]);
    corrupt_chunk[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(smaf::decode(&seal(corrupt_chunk)).is_err());
}
#[test]
fn unsupported_track_format_controls_and_bad_wave_references_fail_explicitly() {
    for seq in [
        &[0, 2, 1][..],
        &[0, 1][..],
        &[0, 1, 0][..],
        &[0, 0, 0x34, 0x40][..],
        &[0, 0xff, 0xf0, 1, 0xf7][..],
        &[0x80, 0x80, 1, 1][..],
    ] {
        assert!(smaf::decode(&file(seq, &[0x70])).is_err(), "{seq:?}");
    }
    assert!(
        smaf::decode(&seal(
            [
                chunk(*b"CNTI", &[0, 1, 2, 0, 0]),
                track(&[0, 1, 1], &[0x70], 0x21, 0)
            ]
            .concat()
        ))
        .is_err()
    );
    let mut body = chunk(*b"CNTI", &[0, 1, 2, 0, 0]);
    body.extend(track(&[0, 1, 1], &[0x70], 0x11, 0));
    body.extend(chunk([b'M', b'T', b'R', 0], &[0; 6]));
    assert!(smaf::decode(&seal(body)).is_err());
}
#[test]
fn parser_rejects_duplicate_required_chunks_and_excessive_duration() {
    let mut body = vec![0, 0, 0x11, 0, 0, 0];
    body.extend(chunk(*b"Atsq", &[0, 1, 1]));
    body.extend(chunk(*b"Atsq", &[0, 1, 1]));
    body.extend(chunk([b'A', b'w', b'a', 1], &[0x70]));
    assert!(
        smaf::decode(&seal(
            [
                chunk(*b"CNTI", &[0, 1, 2, 0, 0]),
                chunk([b'A', b'T', b'R', 0], &body)
            ]
            .concat()
        ))
        .is_err()
    );
    let mut seq = Vec::new();
    for _ in 0..100 {
        seq.extend([0xff, 0x7f, 0xff, 0]);
    }
    seq.extend([0, 1, 1]);
    assert!(smaf::decode(&file(&seq, &[0x70])).is_err());
}

#[test]
fn signed_yamaha_vector_and_saturation_match_integer_reference() {
    let seq = left(&[0, 1, 10, 10, 0xff, 0, 0, 0, 0, 0]);
    let s = smaf::decode(&file(&seq, &[0x70, 0x88, 0x08])).unwrap();
    let actual: Vec<i32> = s
        .samples
        .iter()
        .step_by(2)
        .take(6)
        .map(|v| pcm(*v))
        .collect();
    assert_eq!(actual, [15, 253, 215, 181, 151, 178]);
    for (byte, expected) in [(0x77, 32767), (0xff, -32768)] {
        let s = smaf::decode(&file(&seq, &[byte; 128])).unwrap();
        assert_eq!(pcm(s.samples[100]), expected);
    }
}

#[test]
fn validation_and_decode_reject_zero_global_duration_together() {
    for sequence in [&[0, 0xff, 0, 0, 0, 0, 0][..], &[0, 1, 1, 0, 0, 0, 0][..]] {
        let bytes = file(sequence, &[0x70]);
        let decoded = smaf::decode(&bytes).unwrap_err();
        let validated =
            smaf::validate(&bytes).expect_err("zero-duration source must not be accepted");
        assert_eq!(validated, decoded);
    }
}
