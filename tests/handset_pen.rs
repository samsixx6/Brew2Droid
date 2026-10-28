use zeebx::handset::input::{PenEvent, PenKind, PenInput};
use zeebx::handset::touch::Touchscreen;

#[test]
fn pen_coordinates_follow_the_game_image_and_ignore_letterboxing_and_keypad() {
    let mut touch = Touchscreen::default();
    touch.viewport = Some(([10.0, 100.0], [410.0, 340.0], [800, 480]));
    assert_eq!(touch.update([(1, [200.0, 90.0])]), None);
    assert_eq!(touch.update([(1, [200.0, 400.0])]), None);
    assert_eq!(touch.update([(2, [210.0, 220.0])]),
        Some(PenEvent { kind: PenKind::Down, x: 400, y: 240 }));
    assert_eq!(touch.update([(3, [110.0, 160.0]), (2, [409.9, 339.9])]),
        Some(PenEvent { kind: PenKind::Move, x: 799, y: 479 }));
    assert_eq!(touch.update([(3, [110.0, 160.0])]),
        Some(PenEvent { kind: PenKind::Up, x: 799, y: 479 }));
    assert_eq!(PenEvent { kind: PenKind::Down, x: 799, y: 479 }.packed(), 0x031f01df);
    assert_eq!([PenKind::Down.code(), PenKind::Move.code(), PenKind::Up.code()], [0x504,0x505,0x506]);
}

#[test]
fn cancelling_a_touch_releases_the_delivered_pen_even_with_an_undelivered_up() {
    let mut input = PenInput::default();
    let down = PenEvent { kind: PenKind::Down, x: 400, y: 240 };
    input.push(down).unwrap();
    assert_eq!(input.pop(), Some(down));
    input.push(PenEvent { kind: PenKind::Up, ..down }).unwrap();
    input.release_all();
    assert_eq!(input.pop(), Some(PenEvent { kind: PenKind::Up, ..down }));
    assert_eq!(input.pop(), None);
    input.release_all();
    assert_eq!(input.pop(), None);
}

#[test]
fn pointer_moves_coalesce_without_losing_press_and_release_edges() {
    let mut input = PenInput::default();
    input.push(PenEvent { kind: PenKind::Down, x: 0, y: 0 }).unwrap();
    for x in 1..2000 { input.push(PenEvent { kind: PenKind::Move, x, y: 10 }).unwrap(); }
    input.push(PenEvent { kind: PenKind::Up, x: 1999, y: 10 }).unwrap();
    let events: Vec<_> = std::iter::from_fn(|| input.pop()).collect();
    assert_eq!(events.iter().map(|e|e.kind).collect::<Vec<_>>(), [PenKind::Down,PenKind::Move,PenKind::Up]);
    assert_eq!(events[1].x,1999);
}
