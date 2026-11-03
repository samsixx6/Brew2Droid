use zeebx::audio::{smaf, wav::Sound};
#[path = "common/smaf.rs"]
mod fixtures;
use fixtures::{chunk, seal};

fn selected(sequence: &[u8], start: usize, stop: usize, base: u8) -> Vec<u8> {
    let phrase = [
        b"st:".as_slice(),
        &(start as u32).to_be_bytes(),
        b",sp:",
        &(stop as u32).to_be_bytes(),
        b",",
    ]
    .concat();
    let mut body = vec![0, 0, 0x11, 0, base, base];
    body.extend(chunk(*b"AspI", &phrase));
    body.extend(chunk(*b"Atsq", sequence));
    body.extend(chunk([b'A', b'w', b'a', 1], &[0x77; 128]));
    seal(
        [
            chunk(*b"CNTI", &[0, 0, 0, 0, 0]),
            chunk([b'A', b'T', b'R', 0], &body),
        ]
        .concat(),
    )
}
fn plain(sequence: &[u8], base: u8) -> Sound {
    let bytes = seal(
        [
            chunk(*b"CNTI", &[0, 0, 0, 0, 0]),
            fixtures::track(sequence, &[0x77; 128], 0x11, base),
        ]
        .concat(),
    );
    smaf::decode(&bytes).unwrap()
}
fn rejects(bytes: Vec<u8>) {
    assert!(smaf::validate(&bytes).is_err());
    assert!(smaf::decode(&bytes).is_err());
}

#[test]
fn selected_start_retains_its_own_zero_or_nonzero_duration_and_exact_pcm() {
    for (first_delta, frames, onset) in [(0, 128, 0), (1, 160, 32)] {
        // Range-only oracle is built independently: only the selected original
        // commands, with their Duration/gate bytes unchanged. Base2 means4ms.
        let range = [first_delta, 1, 3, 4, 0xff, 0, 0, 0, 0, 0];
        let reference = plain(&range, 2);
        assert_eq!((reference.rate, reference.frames()), (8000, frames));
        let full = [vec![1, 0, 0x37, 127, 1, 0xff, 0], range.to_vec()].concat();
        let bytes = selected(&full, 7, 13, 2);
        smaf::validate(&bytes)
            .expect("neutral prefix at an exact Duration boundary must be selectable");
        let sound = smaf::decode(&bytes).unwrap();
        assert_eq!(
            sound, reference,
            "a first Duration must not disappear during rebasing"
        );
        assert!(sound.samples[..onset * 2].iter().all(|v| *v == 0.0));
        assert!(
            sound.samples[onset * 2..onset * 2 + 16]
                .iter()
                .any(|v| *v != 0.0)
        );
        assert!(sound.samples[(onset + 96) * 2..].iter().all(|v| *v == 0.0));
    }
}

#[test]
fn all_neutral_controllers_and_nops_can_precede_a_selected_range() {
    let mut prefix = Vec::new();
    for channel in 0..4 {
        for (control, value) in [(0x37, 127), (0x3b, 127), (0x3a, 64)] {
            prefix.extend([1, 0, channel << 6 | control, value]);
        }
        prefix.extend([1, 0, channel << 6 | 0x0e]); // Short expression127.
        prefix.extend([1, 0xff, 0]);
    }
    let range = [0, 0xc1, 3, 4, 0xff, 0, 0, 0, 0, 0];
    let start = prefix.len();
    prefix.extend(range);
    let sound = smaf::decode(&selected(&prefix, start, start + 6, 0)).unwrap();
    assert_eq!(sound, plain(&range, 0));
    assert_eq!(sound.frames(), 32);
}

#[test]
fn selected_range_preserves_eos_cutoff_without_an_extra_gate_tail() {
    let range = [1, 1, 100, 2, 0xff, 0, 0, 0, 0, 0];
    let mut full = vec![1, 0, 0x37, 127, 1, 0xff, 0];
    full.extend(range);
    let sound = smaf::decode(&selected(&full, 7, 13, 0)).unwrap();
    assert_eq!(sound, plain(&range, 0));
    assert_eq!(sound.frames(), 24);
    assert!(sound.samples[..16].iter().all(|v| *v == 0.0));
    assert!(sound.samples[16..].iter().any(|v| *v != 0.0));
}

#[test]
fn selection_without_eos_preserves_the_original_gate_and_whole_stream_stays_equal() {
    let range = [1, 1, 3];
    let mut full = vec![1, 0, 0x37, 127, 1, 0xff, 0];
    full.extend(range);
    let sound = smaf::decode(&selected(&full, 7, 10, 0)).unwrap();
    assert_eq!(sound, plain(&range, 0));
    assert_eq!(sound.frames(), 32);
    assert_eq!(
        smaf::decode(&selected(&range, 0, 3, 0)).unwrap(),
        plain(&range, 0)
    );
}

#[test]
fn unsupported_boundaries_partial_stops_and_malformed_ranges_remain_errors() {
    let full = [1, 0, 0x37, 127, 1, 0xff, 0, 1, 1, 3, 4, 0xff, 0, 0, 0, 0, 0];
    for (start, stop) in [
        (2, 13),
        (8, 13),
        (13, 13),
        (14, 13),
        (18, 13),
        (7, 10),
        (0, 10),
        (7, 17),
        (7, 100),
    ] {
        rejects(selected(&full, start, stop, 0));
    }
    let medium = [
        1, 0, 0x37, 127, 1, 0xff, 0, 0x80, 0, 1, 1, 1, 0xff, 0, 0, 0, 0, 0,
    ];
    rejects(selected(&medium, 8, 14, 0)); // Second byte of selected Duration.
    let mut broken_crc = selected(&full, 7, 13, 0);
    broken_crc[20] ^= 1;
    rejects(broken_crc);
    let mut truncated = selected(&full, 7, 13, 0);
    truncated.pop();
    rejects(truncated);
}

#[test]
fn wave_or_nonneutral_carry_in_is_rejected_even_when_the_prefix_would_be_skipped() {
    let range = [0, 1, 3, 4, 0xff, 0, 0, 0, 0, 0];
    for mut prefix in [
        vec![1, 0, 0x37, 64, 1, 0xff, 0],
        vec![1, 0, 0x3b, 64, 1, 0xff, 0],
        vec![1, 0, 0x3a, 0, 1, 0xff, 0],
        vec![0, 1, 1, 2, 0xff, 0],
        vec![0, 0, 0x34, 0x40],
        vec![0, 0, 0x37, 0, 0, 0, 0x37, 127],
    ] {
        let start = prefix.len();
        prefix.extend(range);
        rejects(selected(&prefix, start, start + 6, 0));
    }
    let missing_wave = [0, 2, 3, 4, 0xff, 0, 0, 0, 0, 0];
    let mut full = vec![1, 0, 0x37, 127, 1, 0xff, 0];
    full.extend(missing_wave);
    rejects(selected(&full, 7, 13, 0));
}

#[test]
fn original_events_are_counted_once_and_selection_cannot_hide_a_global_limit() {
    let range = [0, 1, 1, 1, 0xff, 0, 0, 0, 0, 0];
    let mut prefix = Vec::new();
    for _ in 0..16382 {
        prefix.extend([0, 0xff, 0]);
    }
    let start = prefix.len();
    prefix.extend(range);
    let sound = smaf::decode(&selected(&prefix, start, start + 6, 0)).unwrap();
    assert_eq!(sound, plain(&range, 0));
    assert_eq!(sound.frames(), 8);
    let mut too_many = vec![0, 0xff, 0];
    too_many.extend(prefix);
    rejects(selected(&too_many, start + 3, start + 9, 0));
    let mut over_time = Vec::new();
    for _ in 0..8 {
        over_time.extend([0xff, 0x7f, 0xff, 0]);
    }
    let start = over_time.len();
    over_time.extend(range);
    rejects(selected(&over_time, start, start + 6, 0));
}
