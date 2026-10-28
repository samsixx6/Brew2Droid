// A fila NDK só existe no Android. Este adaptador fornece seus eventos ao receptor real;
// TouchKeypad e a fila BREW continuam sendo os mesmos usados pelo aplicativo.
extern crate self as android_activity;

#[allow(dead_code)]
pub mod input {
    #[derive(Clone, Copy)]
    pub enum Axis {
        X,
        Y,
        Other(u32),
    }
    impl From<u32> for Axis {fn from(axis:u32)->Self {match axis {0=>Self::X,1=>Self::Y,other=>Self::Other(other)}}}
    #[derive(Clone, Copy)]
    pub enum Source { Touchscreen, Gamepad, Joystick }
    impl From<Source> for u32 {fn from(source:Source)->u32 {match source {Source::Touchscreen=>0x1002,Source::Gamepad=>0x401,Source::Joystick=>0x01000010}}}
    #[derive(Clone, Copy, PartialEq)]
    pub enum KeyAction {
        Down,
        Up,
        Multiple,
    }
    #[derive(Clone, Copy)]
    #[repr(u32)]
    pub enum Keycode {
        Keycode0=7,
        Keycode1=8,
        Keycode2=9,
        Keycode3=10,
        Keycode4=11,
        Keycode5=12,
        Keycode6=13,
        Keycode7=14,
        Keycode8=15,
        Keycode9=16,
        Star=17,
        Pound=18,
        DpadUp=19,
        DpadDown=20,
        DpadLeft=21,
        DpadRight=22,
        DpadCenter=23,
        Enter=66,
        ButtonA=96,
        SoftLeft=1,
        ButtonL1=102,
        SoftRight=2,
        ButtonR1=103,
        Clear=28,
        Del=67,
        Menu=82,
        Back=4,
        VolumeUp=24,
    }
    impl From<Keycode> for u32 {fn from(code:Keycode)->u32 {code as u32}}
    #[derive(Clone, Copy, PartialEq)]
    pub enum MotionAction {
        Down,
        PointerDown,
        Move,
        Cancel,
        Up,
        PointerUp,
        Other,
    }
    #[derive(Clone, Copy, PartialEq)]
    pub enum ToolType {
        Palm,
        Finger,
    }

    pub struct KeyEvent {
        pub code: Keycode,
        pub scan: i32,
        pub device: i32,
        pub action: KeyAction,
    }
    impl KeyEvent {
        pub fn key_code(&self) -> Keycode {
            self.code
        }
        pub fn scan_code(&self) -> i32 {
            self.scan
        }
        pub fn device_id(&self) -> i32 {
            self.device
        }
        pub fn action(&self) -> KeyAction {
            self.action
        }
    }
    pub struct MotionEvent { pub action: MotionAction, pub index:usize, pub points:Vec<Pointer>,pub source:Source,pub device:i32 }
    impl MotionEvent {
        pub fn source(&self)->Source {self.source}
        pub fn device_id(&self)->i32 {self.device}
        pub fn action(&self) -> MotionAction {
            self.action
        }
        pub fn pointer_index(&self) -> usize {
            self.index
        }
        pub fn pointers(&self) -> impl Iterator<Item=Pointer> + '_ {
            self.points.iter().cloned()
        }
        pub fn pointer_at_index(&self,index:usize) -> Pointer {self.points[index].clone()}
    }
    #[derive(Clone)]
    pub struct Pointer { pub id:i32, pub index:usize, pub point:[f32;2],pub tool:ToolType }
    impl Pointer {
        pub fn pointer_index(&self) -> usize {
            self.index
        }
        pub fn pointer_id(&self) -> i32 {self.id}
        pub fn tool_type(&self) -> ToolType {
            self.tool
        }
        pub fn axis_value(&self, axis: Axis) -> f32 {
            match axis {Axis::X=>self.point[0],Axis::Y=>self.point[1],Axis::Other(_)=>0.0}
        }
        pub fn history(&self)->impl Iterator<Item=Pointer> {std::iter::empty()}
    }
    pub enum InputEvent {
        KeyEvent(KeyEvent),
        MotionEvent(MotionEvent),
        Other,
    }
}

#[test]
fn keypad_vibration_only_follows_new_visible_touch_presses_when_enabled() {
    use input::{MotionAction,MotionEvent,Pointer,Source,ToolType};
    use zeebx::handset::touch::KeyRegion;
    let mut keys=TouchKeypad::default();keys.regions.push(KeyRegion{key:Key::Num1,min:[0.0,0.0],max:[10.0,10.0]});
    let motion=|action,point,tool,source|InputEvent::MotionEvent(MotionEvent{action,index:0,device:0,source,points:vec![Pointer{id:4,index:0,point,tool}]});
    let press=motion(MotionAction::Down,[10.0,10.0],ToolType::Finger,Source::Touchscreen);
    assert!(android_handset::vibration_press(&keys,&press,2.0,true),"Enabled virtual key press must request vibration");
    assert!(!android_handset::vibration_press(&keys,&press,2.0,false));
    keys.hardware(Key::Num1,true);assert!(android_handset::vibration_press(&keys,&press,2.0,true),"A held physical key must not hide a new touch");
    for action in [MotionAction::Move,MotionAction::Up,MotionAction::PointerUp,MotionAction::Cancel] {
        assert!(!android_handset::vibration_press(&keys,&motion(action,[5.0,5.0],ToolType::Finger,Source::Touchscreen),1.0,true));
    }
    assert!(android_handset::vibration_press(&keys,&motion(MotionAction::PointerDown,[5.0,5.0],ToolType::Finger,Source::Touchscreen),1.0,true));
    assert!(!android_handset::vibration_press(&keys,&motion(MotionAction::Down,[20.0,20.0],ToolType::Finger,Source::Touchscreen),1.0,true));
    assert!(!android_handset::vibration_press(&keys,&motion(MotionAction::Down,[5.0,5.0],ToolType::Palm,Source::Touchscreen),1.0,true));
    assert!(!android_handset::vibration_press(&keys,&motion(MotionAction::Down,[5.0,5.0],ToolType::Finger,Source::Joystick),1.0,true));
    assert!(!android_handset::vibration_press(&keys,&press,0.0,true));
    keys.set_enabled(false);assert!(!android_handset::vibration_press(&keys,&press,2.0,true));
}

#[allow(dead_code)]
#[path = "../frontends/android/src/button_mapping.rs"]
mod button_mapping;
#[allow(dead_code)]
#[path = "../frontends/android/src/handset.rs"]
mod android_handset;

use button_mapping::InputMap;
use input::{InputEvent, KeyAction, KeyEvent, Keycode};
use zeebx::handset::input::{Key, KeyKind, Keypad};
use zeebx::handset::touch::TouchKeypad;

#[test]
fn moved_keypad_buttons_capture_touch_without_emitting_pen_and_pen_fingers_do_not_press_keys(){
    use input::{MotionAction,MotionEvent,Pointer,ToolType};use zeebx::handset::touch::{Touchscreen,KeyRegion};use zeebx::handset::input::PenKind;
    let mut keys=TouchKeypad::default();keys.regions.push(KeyRegion{key:Key::Num1,min:[100.0,100.0],max:[150.0,150.0]});
    let mut touch=Touchscreen::default();touch.viewport=Some(([0.0,0.0],[360.0,500.0],[360,500]));let mut routes=android_handset::TouchRoutes::default();
    let event=|action,index,points:Vec<(i32,[f32;2])>|InputEvent::MotionEvent(MotionEvent{action,index,source:input::Source::Touchscreen,device:0,points:points.into_iter().enumerate().map(|(index,(id,point))|Pointer{id,index,point,tool:ToolType::Finger}).collect()});
    let mut route=|action,index,points|android_handset::receive_touch(&mut keys,&mut touch,&mut routes,&event(action,index,points),1.0).unwrap();
    assert_eq!(route(MotionAction::Down,0,vec![(1,[120.0,120.0])]),None,"Moved button must not also emit pen down");
    assert_eq!(route(MotionAction::Move,0,vec![(1,[180.0,180.0])]),None,"Keypad finger remains captured after leaving key");
    assert_eq!(route(MotionAction::PointerDown,1,vec![(1,[180.0,180.0]),(2,[220.0,220.0])]).unwrap().kind,PenKind::Down);
    assert_eq!(route(MotionAction::Move,0,vec![(1,[120.0,120.0]),(2,[120.0,120.0])]).unwrap().kind,PenKind::Move);
    assert_eq!(route(MotionAction::PointerUp,0,vec![(1,[120.0,120.0]),(2,[120.0,120.0])]),None);
    assert_eq!(route(MotionAction::Move,0,vec![(2,[125.0,125.0])]).unwrap().kind,PenKind::Move);
    assert_eq!(keys.held(),0,"Screen finger must not press a button it crosses");
    let cancel=android_handset::receive_touch(&mut keys,&mut touch,&mut routes,&event(MotionAction::Cancel,0,vec![]),1.0).unwrap();assert_eq!(cancel.unwrap().kind,PenKind::Up);
    assert_eq!(android_handset::receive_touch(&mut keys,&mut touch,&mut routes,&event(MotionAction::Down,0,vec![(2,[120.0,120.0])]),1.0),Some(None));
    assert_eq!(keys.held(),1<<Key::Num1 as u8,"Canceled pointer identity can be reused for a new button press");
}

#[test]
fn android_screen_touches_keep_pointer_identity_and_release_at_the_actual_up_position() {
    use zeebx::handset::input::{PenEvent,PenKind};
    use zeebx::handset::touch::Touchscreen;
    use input::{MotionAction,MotionEvent,Pointer,ToolType};
    let mut touch=Touchscreen::default();
    touch.viewport=Some(([10.0,100.0],[410.0,340.0],[800,480]));
    let event=|action,index,points:Vec<(i32,[f32;2])>| InputEvent::MotionEvent(MotionEvent {
        action,index,source:input::Source::Touchscreen,device:0,points:points.into_iter().enumerate().map(|(index,(id,point))|
            Pointer{id,index,point,tool:ToolType::Finger}).collect(),
    });
    assert_eq!(android_handset::receive_pen(&mut touch,&event(MotionAction::Down,0,vec![(7,[420.0,440.0])]),2.0),
        Some(PenEvent{kind:PenKind::Down,x:400,y:240}));
    assert_eq!(android_handset::receive_pen(&mut touch,&event(MotionAction::PointerDown,1,vec![(7,[420.0,440.0]),(9,[400.0,800.0])]),2.0),None);
    assert_eq!(android_handset::receive_pen(&mut touch,&event(MotionAction::PointerUp,1,vec![(7,[420.0,440.0]),(9,[400.0,800.0])]),2.0),None);
    assert_eq!(android_handset::receive_pen(&mut touch,&event(MotionAction::Up,0,vec![(7,[620.0,540.0])]),2.0),
        Some(PenEvent{kind:PenKind::Up,x:600,y:340}));
    assert_eq!(android_handset::receive_pen(&mut touch,&event(MotionAction::Down,0,vec![(7,[420.0,440.0])]),2.0),
        Some(PenEvent{kind:PenKind::Down,x:400,y:240}));
    assert_eq!(android_handset::receive_pen(&mut touch,&event(MotionAction::Cancel,0,vec![]),2.0),
        Some(PenEvent{kind:PenKind::Up,x:400,y:240}));
}

fn receive(
    keys: &mut TouchKeypad,
    mapping: &mut InputMap,
    code: Keycode,
    scan: i32,
    device: i32,
    action: KeyAction,
) -> bool {
    android_handset::receive(
        keys,
        mapping,
        &InputEvent::KeyEvent(KeyEvent {
            code,
            scan,
            device,
            action,
        }),
        1.0,
    )
}

fn assert_game_key(code: Keycode, scan: i32, device: i32, key: Key, brew_code: u16) {
    let mut keys = TouchKeypad::default();
    let mut mapping=InputMap::default();
    keys.set_enabled(false);
    let mut guest = Keypad::default();
    // A repetição de Down não pode duplicar Press/Key nem impedir a soltura.
    for action in [KeyAction::Down, KeyAction::Down, KeyAction::Up] {
        assert!(
            receive(&mut keys, &mut mapping, code, scan, device, action),
            "key must be consumed by the game"
        );
        let held = if action == KeyAction::Up {
            0
        } else {
            1 << key as u8
        };
        assert_eq!(keys.held(), held);
        for candidate in Key::ALL {
            guest
                .set(candidate, keys.held() & (1 << candidate as u8) != 0, 0)
                .unwrap();
        }
    }
    let events: Vec<_> = std::iter::from_fn(|| guest.pop())
        .map(|event| (event.key.code(), event.kind))
        .collect();
    assert_eq!(
        events,
        vec![
            (brew_code, KeyKind::Press),
            (brew_code, KeyKind::Key),
            (brew_code, KeyKind::Release)
        ]
    );
}

#[test]
fn doov_flat_left_menu_reaches_brew_soft1_with_controls_hidden() {
    assert_game_key(Keycode::Menu, 139, 3, Key::Soft1, 0xe036);
}

#[test]
fn doov_flat_right_hardware_back_reaches_brew_soft2_with_controls_hidden() {
    assert_game_key(Keycode::Back, 158, 3, Key::Soft2, 0xe037);
}

#[test]
fn android_virtual_back_falls_through_to_the_emulator_navigation() {
    let mut keys = TouchKeypad::default();
    let mut mapping=InputMap::default();
    for (scan, device) in [(0, -1), (0, 3), (158, -1)] {
        for action in [KeyAction::Down, KeyAction::Up] {
            assert!(!receive(&mut keys, &mut mapping, Keycode::Back, scan, device, action));
            assert_eq!(keys.held(), 0);
        }
    }
}

#[test]
fn standard_softkeys_center_and_gamepad_keep_their_brew_mapping() {
    for (code, key, brew_code) in [
        (Keycode::SoftLeft, Key::Soft1, 0xe036),
        (Keycode::ButtonL1, Key::Soft1, 0xe036),
        (Keycode::SoftRight, Key::Soft2, 0xe037),
        (Keycode::ButtonR1, Key::Soft2, 0xe037),
        (Keycode::Enter, Key::Select, 0xe035),
        (Keycode::DpadCenter, Key::Select, 0xe035),
        (Keycode::ButtonA, Key::Select, 0xe035),
    ] {
        assert_game_key(code, 28, 3, key, brew_code);
    }
}

#[test]
fn controller_axes_never_become_pen_events_or_replace_held_touch() {
    use input::{MotionAction,MotionEvent,Pointer,Source,ToolType};
    use zeebx::handset::touch::{KeyRegion,Touchscreen};
    let mut mapping=InputMap::parse(r#"{"version":1,"bindings":{"left":"axis:0:-"}}"#);
    let mut keys=TouchKeypad::default();
    keys.regions.push(KeyRegion {key:Key::Num1,min:[0.0,0.0],max:[10.0,10.0]});keys.pointers([[5.0,5.0]]);
    let mut touch=Touchscreen::default();touch.viewport=Some(([-2.0,-1.0],[2.0,1.0],[400,200]));
    let motion=|x|InputEvent::MotionEvent(MotionEvent {action:MotionAction::Move,index:0,device:8,source:Source::Joystick,
        points:vec![Pointer {id:0,index:0,point:[x,0.0],tool:ToolType::Finger}]});
    assert_eq!(android_handset::receive_pen(&mut touch,&motion(-1.0),1.0),None);
    assert!(android_handset::receive(&mut keys,&mut mapping,&motion(-1.0),1.0));
    assert_eq!(keys.held(),(1<<Key::Left as u8)|(1<<Key::Num1 as u8));
    receive(&mut keys,&mut mapping,Keycode::Enter,28,8,KeyAction::Down);
    assert!(android_handset::receive(&mut keys,&mut mapping,&motion(0.0),1.0));
    assert_eq!(keys.held(),(1<<Key::Select as u8)|(1<<Key::Num1 as u8));
    receive(&mut keys,&mut mapping,Keycode::Enter,28,8,KeyAction::Up);
    assert_eq!(keys.held(),1<<Key::Num1 as u8);
}
