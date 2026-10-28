//! Estado de toque separado do teclado físico: levantar um dedo não solta outra origem.

use super::input::Key;
use super::input::{PenEvent, PenKind};

/// Coordenadas lógicas da imagem apresentada, sem barras pretas nem teclado virtual.
#[derive(Default)]
pub struct Touchscreen {
    pub viewport: Option<([f32; 2], [f32; 2], [u16; 2])>,
    active: Option<(i32, PenEvent)>,
}

impl Touchscreen {
    fn position(&self, point: [f32; 2], inside: bool) -> Option<[u16; 2]> {
        let (min, max, size) = self.viewport?;
        if size.contains(&0) || (0..2).any(|i| max[i] <= min[i] || !point[i].is_finite()) { return None; }
        if inside && (0..2).any(|i| point[i] < min[i] || point[i] >= max[i]) { return None; }
        Some(std::array::from_fn(|i| (((point[i]-min[i])/(max[i]-min[i])*size[i] as f32)
            .floor().clamp(0.0,(size[i]-1) as f32)) as u16))
    }
    pub fn update(&mut self, points: impl IntoIterator<Item=(i32,[f32; 2])>) -> Option<PenEvent> {
        if let Some((id, previous)) = self.active {
            for (pointer, point) in points {
                if pointer == id {
                    let [x,y] = self.position(point,false)?;
                    if [x,y] == [previous.x,previous.y] { return None; }
                    let event = PenEvent { kind:PenKind::Move,x,y };
                    self.active = Some((id,event));
                    return Some(event);
                }
            }
            return self.cancel();
        }
        for (id, point) in points {
            if let Some([x,y]) = self.position(point,true) {
                let event = PenEvent { kind:PenKind::Down,x,y };
                self.active = Some((id,event));
                return Some(event);
            }
        }
        None
    }
    pub fn end(&mut self, id: i32, point: [f32; 2]) -> Option<PenEvent> {
        let (active, previous) = self.active?;
        if active != id { return None; }
        let [x,y] = self.position(point,false).unwrap_or([previous.x,previous.y]);
        self.active = None;
        Some(PenEvent { kind:PenKind::Up,x,y })
    }
    pub fn cancel(&mut self) -> Option<PenEvent> {
        self.active.take().map(|(_,event)| PenEvent { kind:PenKind::Up,..event })
    }
}

pub struct KeyRegion {
    pub key: Key,
    pub min: [f32; 2],
    pub max: [f32; 2],
}

#[derive(Default)]
pub struct TouchKeypad {
    pub regions: Vec<KeyRegion>,
    touch: u32,
    hardware: u32,
    disabled: bool,
}

impl TouchKeypad {
    pub fn is_enabled(&self) -> bool { !self.disabled }
    pub fn set_enabled(&mut self, enabled: bool) {
        self.disabled=!enabled;
        if !enabled {self.touch=0;self.regions.clear();}
    }
    pub fn pointers(&mut self, points: impl IntoIterator<Item = [f32; 2]>) {
        self.touch = 0;
        if self.disabled {return;}
        for point in points {
            for region in &self.regions {
                if point[0] >= region.min[0]
                    && point[0] < region.max[0]
                    && point[1] >= region.min[1]
                    && point[1] < region.max[1]
                {
                    self.touch |= 1 << region.key as u8;
                }
            }
        }
    }

    pub fn hardware(&mut self, key: Key, down: bool) {
        if down {
            self.hardware |= 1 << key as u8;
        } else {
            self.hardware &= !(1 << key as u8);
        }
    }

    pub fn held(&self) -> u32 {
        self.touch | self.hardware
    }

    pub fn clear(&mut self) {
        self.touch = 0;
        self.hardware = 0;
    }
}
