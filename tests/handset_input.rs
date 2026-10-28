use zeebx::handset::input::{Key, KeyKind, Keypad};

#[test]
fn phone_key_codes_match_the_melange_keypad_reference() {
    for (name, code) in [
        ("0", 0xe021),
        ("5", 0xe026),
        ("9", 0xe02a),
        ("*", 0xe02b),
        ("#", 0xe02c),
        ("up", 0xe031),
        ("down", 0xe032),
        ("left", 0xe033),
        ("right", 0xe034),
        ("select", 0xe035),
        ("soft1", 0xe036),
        ("soft2", 0xe037),
        ("clear", 0xe030),
    ] {
        assert_eq!(Key::from_name(name).unwrap().code(), code);
    }
    assert!(Key::from_name("unknown").is_none());
}

#[test]
fn overlapping_keys_and_duplicate_edges_preserve_press_key_release_order() {
    let mut input = Keypad::default();
    assert!(input.set(Key::Up, true, 10).unwrap());
    assert!(!input.set(Key::Up, true, 11).unwrap());
    input.set(Key::Num5, true, 12).unwrap();
    input.set(Key::Up, false, 13).unwrap();
    let events: Vec<_> = std::iter::from_fn(|| input.pop()).collect();
    assert_eq!(
        events.iter().map(|e| (e.key, e.kind)).collect::<Vec<_>>(),
        [
            (Key::Up, KeyKind::Press),
            (Key::Up, KeyKind::Key),
            (Key::Num5, KeyKind::Press),
            (Key::Num5, KeyKind::Key),
            (Key::Up, KeyKind::Release),
        ]
    );
    assert_eq!(events[4].timestamp_us, 13);
    assert!(!input.set(Key::Up, false, 14).unwrap());
}

#[test]
fn focus_loss_releases_delivered_keys_even_if_their_release_was_still_queued() {
    let mut input = Keypad::default();
    input.set(Key::Up, true, 1).unwrap();
    input.pop();
    input.pop();
    input.set(Key::Up, false, 2).unwrap();
    input.set(Key::Num5, true, 3).unwrap();
    input.release_all(4);
    let events: Vec<_> = std::iter::from_fn(|| input.pop()).collect();
    assert_eq!(events.len(), 2);
    assert!(
        events
            .iter()
            .all(|e| e.kind == KeyKind::Release && e.timestamp_us == 4)
    );
    assert!(events.iter().any(|e| e.key == Key::Up));
    assert!(events.iter().any(|e| e.key == Key::Num5));
    input.release_all(5);
    assert!(input.pop().is_none());
}

#[test]
fn queue_pressure_is_bounded_and_focus_loss_can_always_release_keys() {
    let mut input = Keypad::default();
    let mut rejected = false;
    for i in 0..1000 {
        if input.set(Key::Num5, i % 2 == 0, i).is_err() {
            rejected = true;
            break;
        }
    }
    assert!(rejected);
    input.release_all(1001);
    let releases: Vec<_> = std::iter::from_fn(|| input.pop()).collect();
    assert!(releases.len() <= 20);
    assert!(releases.iter().all(|e| e.kind == KeyKind::Release));
}

#[test]
fn console_edges_are_deduplicated_bounded_and_released_after_cancel(){
    use zeebx::handset::input::AvkInput;
    let mut q=AvkInput::default();assert!(q.set(0xe064,true).unwrap());assert!(!q.set(0xe064,true).unwrap());assert_eq!(q.pop(),Some((0xe064,true)));q.set(0xe064,false).unwrap();q.set(0xe063,true).unwrap();q.release_all();
    assert_eq!(std::iter::from_fn(||q.pop()).collect::<Vec<_>>(),vec![(0xe063,false),(0xe064,false)]);assert!(q.is_empty());q.release_all();assert!(q.is_empty());
    for i in 0..256{q.set(0xe064,i%2==0).unwrap();}assert!(q.set(0xe064,true).is_err());q.release_all();assert!(std::iter::from_fn(||q.pop()).count()<=1);
}
