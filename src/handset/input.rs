//! Teclado móvel: códigos conferidos no EmulatorKeypad.java e ordem física de breInput.c.

use serde::Serialize;
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PenKind { Down, Move, Up }

impl PenKind {
    // Undercover U960 compara 0x504/505/506 e extrai x dos 16 bits altos de dwParam
    // no tratador em 0x35910. Não são os EVT_POINTER, cujo parâmetro seria um ponteiro.
    pub const fn code(self) -> u32 {
        match self { Self::Down => 0x504, Self::Move => 0x505, Self::Up => 0x506 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PenEvent { pub kind: PenKind, pub x: u16, pub y: u16 }

impl PenEvent {
    pub const fn packed(self) -> u32 { ((self.x as u32) << 16) | self.y as u32 }
}

#[derive(Default)]
pub struct PenInput {
    queue: VecDeque<PenEvent>,
    held: Option<PenEvent>,
    delivered: Option<PenEvent>,
}

impl PenInput {
    pub fn push(&mut self, event: PenEvent) -> Result<(), &'static str> {
        if event.kind == PenKind::Move && self.queue.back().is_some_and(|e| e.kind == PenKind::Move) {
            *self.queue.back_mut().unwrap() = event;
        } else {
            if self.queue.len() >= 256 { return Err("Handset pointer queue is full"); }
            self.queue.push_back(event);
        }
        self.held = (event.kind != PenKind::Up).then_some(event);
        Ok(())
    }
    pub fn pop(&mut self) -> Option<PenEvent> {
        let event = self.queue.pop_front()?;
        self.delivered = (event.kind != PenKind::Up).then_some(event);
        Some(event)
    }
    pub fn is_empty(&self) -> bool { self.queue.is_empty() }
    pub fn release_all(&mut self) {
        let release = self.held.or(self.delivered);
        self.held = None;
        self.queue.clear();
        if let Some(event) = release { self.queue.push_back(PenEvent { kind: PenKind::Up, ..event }); }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[repr(u8)]
pub enum Key {
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
    Star,
    Pound,
    Up,
    Down,
    Left,
    Right,
    Select,
    Soft1,
    Soft2,
    Clear,
}

impl Key {
    pub const ALL: [Self; 20] = [
        Self::Num0,
        Self::Num1,
        Self::Num2,
        Self::Num3,
        Self::Num4,
        Self::Num5,
        Self::Num6,
        Self::Num7,
        Self::Num8,
        Self::Num9,
        Self::Star,
        Self::Pound,
        Self::Up,
        Self::Down,
        Self::Left,
        Self::Right,
        Self::Select,
        Self::Soft1,
        Self::Soft2,
        Self::Clear,
    ];

    pub fn from_name(name: &str) -> Option<Self> {
        const NAMES: [&str; 20] = [
            "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "*", "#", "up", "down", "left",
            "right", "select", "soft1", "soft2", "clear",
        ];
        NAMES
            .iter()
            .position(|candidate| *candidate == name)
            .map(|index| Self::ALL[index])
    }

    pub const fn code(self) -> u16 {
        const CODES: [u16; 20] = [
            0xe021, 0xe022, 0xe023, 0xe024, 0xe025, 0xe026, 0xe027, 0xe028, 0xe029, 0xe02a, 0xe02b,
            0xe02c, 0xe031, 0xe032, 0xe033, 0xe034, 0xe035, 0xe036, 0xe037, 0xe030,
        ];
        CODES[self as usize]
    }

    const fn mask(self) -> u32 {
        1 << self as u8
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum KeyKind {
    Press,
    Key,
    Release,
}

impl KeyKind {
    pub const fn code(self) -> u32 {
        match self {
            Self::Press => 0x101,
            Self::Key => 0x100,
            Self::Release => 0x102,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct KeyEvent {
    pub key: Key,
    pub kind: KeyKind,
    pub timestamp_us: u64,
}

#[derive(Default)]
pub struct Keypad {
    held: u32,
    delivered: u32,
    queue: VecDeque<KeyEvent>,
}

impl Keypad {
    pub fn set(&mut self, key: Key, down: bool, timestamp_us: u64) -> Result<bool, &'static str> {
        let pressed = self.held & key.mask() != 0;
        if pressed == down {
            return Ok(false);
        }
        let kinds: &[KeyKind] = if down {
            &[KeyKind::Press, KeyKind::Key]
        } else {
            &[KeyKind::Release]
        };
        if self.queue.len() + kinds.len() > 256 {
            return Err("Handset input queue is full");
        }
        if down {
            self.held |= key.mask();
        } else {
            self.held &= !key.mask();
        }
        for kind in kinds {
            self.queue.push_back(KeyEvent {
                key,
                kind: *kind,
                timestamp_us,
            });
        }
        Ok(true)
    }

    pub fn pop(&mut self) -> Option<KeyEvent> {
        let event = self.queue.pop_front()?;
        match event.kind {
            KeyKind::Press | KeyKind::Key => self.delivered |= event.key.mask(),
            KeyKind::Release => self.delivered &= !event.key.mask(),
        }
        Some(event)
    }

    /// Cancelar toques pendentes não pode apagar uma soltura que o guest ainda precisa receber.
    pub fn release_all(&mut self, timestamp_us: u64) {
        let release = self.held | self.delivered;
        self.held = 0;
        self.queue.clear();
        for key in Key::ALL {
            if release & key.mask() != 0 {
                self.queue.push_back(KeyEvent {
                    key,
                    kind: KeyKind::Release,
                    timestamp_us,
                });
            }
        }
    }
}

/// Bordas AVK do console no laço móvel: entrega de topo preserva Budget e falhas.
#[derive(Default)]
pub struct AvkInput {
    held: std::collections::BTreeSet<u32>,
    delivered: std::collections::BTreeSet<u32>,
    queue: VecDeque<(u32,bool)>,
}
impl AvkInput {
    pub fn set(&mut self,code:u32,down:bool)->Result<bool,&'static str>{
        if self.held.contains(&code)==down{return Ok(false);}
        if self.queue.len()>=256{return Err("Console input queue is full");}
        if down{self.held.insert(code);}else{self.held.remove(&code);}
        self.queue.push_back((code,down));Ok(true)
    }
    pub fn pop(&mut self)->Option<(u32,bool)>{let (code,down)=self.queue.pop_front()?;if down{self.delivered.insert(code);}else{self.delivered.remove(&code);}Some((code,down))}
    pub fn is_empty(&self)->bool{self.queue.is_empty()}
    pub fn release_all(&mut self){
        self.queue.clear();for code in self.held.union(&self.delivered){self.queue.push_back((*code,false));}self.held.clear();
    }
}
