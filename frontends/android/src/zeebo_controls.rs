//! Geometria e captura multitouch independentes do Android e do renderer.
use std::collections::BTreeMap;
use zeebx::input::Pad;
#[derive(Debug, Clone, Copy, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl Rect {
    pub fn center(self) -> [f32; 2] {
        [self.x + self.w / 2., self.y + self.h / 2.]
    }
    pub fn radius(self) -> f32 {
        self.w.min(self.h) / 2.
    }
    pub fn contains(self, p: [f32; 2]) -> bool {
        p[0] >= self.x && p[1] >= self.y && p[0] <= self.x + self.w && p[1] <= self.y + self.h
    }
    pub fn circle(self, p: [f32; 2]) -> bool {
        let c = self.center();
        (p[0] - c[0]).hypot(p[1] - c[1]) <= self.radius()
    }
}
#[derive(Default)]
pub struct Layout {
    pub buttons: Vec<(&'static str, Rect)>,
    pub sticks: [Rect; 2],
    pub inner_sizes: [f32; 2],
    pub dpad: Rect,
}
impl Layout {
    pub fn new(w: f32, h: f32) -> Self {
        if !w.is_finite() || !h.is_finite() || w <= 0. || h <= 0. {
            return Self::default();
        }
        let landscape = w > h;
        let scale = if landscape {
            (w / 1679.).min(h / 938.)
        } else {
            (w / 591.).min(h / 1280.)
        };
        let rect = |x: f32, y: f32, bw: f32, bh: f32| Rect {
            x: x * scale,
            y: y * scale,
            w: bw * scale,
            h: bh * scale,
        };
        let mut l = Self::default();
        if landscape {
            let left = |x: f32, y: f32, bw: f32, bh: f32| {
                let mut r = rect(x, y, bw, bh);
                r.y = h - (938. - y) * scale;
                r
            };
            let right = |x: f32, y: f32, bw: f32, bh: f32| {
                let mut r = left(x, y, bw, bh);
                r.x = w - (1679. - x) * scale;
                r
            };
            l.buttons = vec![
                ("zl", rect(42., 37., 260., 110.)),
                (
                    "home",
                    Rect {
                        x: (w - 226. * scale) / 2.,
                        ..rect(0., 37., 226., 78.)
                    },
                ),
                (
                    "zr",
                    Rect {
                        x: w - 309. * scale,
                        ..rect(0., 37., 260., 110.)
                    },
                ),
                ("b3", right(1393., 420., 142., 142.)),
                ("b2", right(1289., 528., 142., 142.)),
                ("b4", right(1497., 528., 142., 142.)),
                ("b1", right(1393., 633., 142., 142.)),
            ];
            l.dpad = left(39., 419., 330., 328.);
            l.sticks = [left(387., 619., 266., 262.), right(1019., 619., 266., 262.)];
            l.inner_sizes = [132. * scale; 2];
        } else {
            let pos = |x: f32, y: f32, bw: f32, bh: f32| {
                let mut r = rect(x, y, bw, bh);
                r.x += (w - 591. * scale) / 2.;
                r.y += h - 1280. * scale;
                r
            };
            l.buttons = vec![
                ("zl", pos(18., 789., 170., 68.)),
                ("zr", pos(402., 789., 170., 68.)),
                ("home", pos(227., 1056., 136., 48.)),
                ("b3", pos(419., 882., 91., 91.)),
                ("b2", pos(352., 951., 91., 91.)),
                ("b4", pos(485., 951., 91., 91.)),
                ("b1", pos(419., 1019., 91., 91.)),
            ];
            l.dpad = pos(20., 891., 215., 212.);
            l.sticks = [pos(138., 1106., 140., 140.), pos(312., 1106., 140., 140.)];
            l.inner_sizes = [132. / 262. * 140. * scale; 2];
        }
        l
    }
    pub fn button(&self, name: &str) -> Option<Rect> {
        self.buttons
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, r)| *r)
    }
    pub fn direction(&self, name: &str) -> (Rect, u8) {
        let (x, y, w, h, q) = match name {
            "right" => (193., 97., 136., 135., 1),
            "down" => (98., 192., 135., 136., 2),
            "left" => (0., 97., 136., 135., 3),
            _ => (98., 0., 135., 136., 0),
        };
        (
            Rect {
                x: self.dpad.x + x / 330. * self.dpad.w,
                y: self.dpad.y + y / 328. * self.dpad.h,
                w: w / 330. * self.dpad.w,
                h: h / 328. * self.dpad.h,
            },
            q,
        )
    }
}
#[derive(Clone, Copy)]
enum Target {
    Button(&'static str),
    Dpad,
    Stick(usize),
    Blocked,
}
#[derive(Clone, Copy)]
struct Touch {
    target: Target,
    position: [f32; 2],
}
pub struct Controls {
    pub layout: Layout,
    size: [f32; 2],
    origin: [f32; 2],
    enabled: bool,
    touches: BTreeMap<i32, Touch>,
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            layout: Layout::default(),
            size: [0.; 2],
            origin: [0.; 2],
            enabled: true,
            touches: BTreeMap::new(),
        }
    }
}
pub fn button_index(name: &str) -> Option<usize> {
    Pad::button_by_name(if name == "home" { "back" } else { name })
}
fn finite(p: [f32; 2]) -> bool {
    p.into_iter().all(f32::is_finite)
}
impl Controls {
    pub fn resize(&mut self, w: f32, h: f32) -> bool {
        if self.size == [w, h] {
            return false;
        }
        self.clear();
        self.size = [w, h];
        self.layout = Layout::new(w, h);
        true
    }
    pub fn viewport(&mut self, w: f32, h: f32, origin: [f32; 2]) -> bool {
        let moved = self.origin != origin;
        if moved {
            self.clear();
            self.origin = origin;
        }
        self.resize(w, h) || moved
    }
    pub fn local(&self, p: [f32; 2]) -> [f32; 2] {
        [p[0] - self.origin[0], p[1] - self.origin[1]]
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn set_enabled(&mut self, on: bool) {
        if !on {
            self.clear();
        }
        self.enabled = on;
    }
    pub fn clear(&mut self) {
        self.touches.clear();
    }
    pub fn begin(&mut self, id: i32, p: [f32; 2]) -> bool {
        if !self.enabled || self.size[0] <= 0. || self.size[1] <= 0. || !finite(p) {
            return false;
        }
        let target = self
            .layout
            .buttons
            .iter()
            .find(|(name, r)| {
                if name.starts_with('b') {
                    r.circle(p)
                } else {
                    r.contains(p)
                }
            })
            .map(|(name, _)| Target::Button(*name))
            .or_else(|| self.layout.dpad.circle(p).then_some(Target::Dpad))
            .or_else(|| {
                self.layout
                    .sticks
                    .iter()
                    .position(|r| r.circle(p))
                    .map(Target::Stick)
            });
        let Some(mut target) = target else {
            return false;
        };
        if let Target::Stick(i) = target {
            if self
                .touches
                .values()
                .any(|t| matches!(t.target,Target::Stick(j)if i==j))
            {
                target = Target::Blocked;
            }
        }
        self.touches.insert(
            id,
            Touch {
                target,
                position: p,
            },
        );
        true
    }
    pub fn move_to(&mut self, id: i32, p: [f32; 2]) {
        if finite(p) {
            if let Some(t) = self.touches.get_mut(&id) {
                t.position = p;
            }
        }
    }
    pub fn end(&mut self, id: i32) {
        self.touches.remove(&id);
    }
    pub fn owns(&self, id: i32) -> bool {
        self.touches.contains_key(&id)
    }
    pub fn stick_vector(&self, index: usize) -> [f32; 2] {
        let Some(t) = self
            .touches
            .values()
            .find(|t| matches!(t.target,Target::Stick(i)if i==index))
        else {
            return [0.; 2];
        };
        let base = self.layout.sticks[index];
        let center = base.center();
        let travel = (base.radius() - self.layout.inner_sizes[index] / 2.).max(0.001);
        let x = (t.position[0] - center[0]) / travel;
        let y = (t.position[1] - center[1]) / travel;
        let length = x.hypot(y).max(1.);
        [x / length, y / length]
    }
    pub fn inner_center(&self, index: usize) -> [f32; 2] {
        let center = self.layout.sticks[index].center();
        let travel =
            (self.layout.sticks[index].radius() - self.layout.inner_sizes[index] / 2.).max(0.);
        let v = self.stick_vector(index);
        [center[0] + v[0] * travel, center[1] + v[1] * travel]
    }
    pub fn pad(&self) -> Pad {
        let mut pad = Pad::default();
        let mut press = |name: &str| {
            if let Some(i) = button_index(name) {
                pad.press(i, true);
            }
        };
        for t in self.touches.values() {
            match t.target {
                Target::Button(name) => {
                    if self.layout.button(name).is_some_and(|r| {
                        if name.starts_with('b') {
                            r.circle(t.position)
                        } else {
                            r.contains(t.position)
                        }
                    }) {
                        press(name);
                    }
                }
                Target::Dpad => {
                    let c = self.layout.dpad.center();
                    let x = t.position[0] - c[0];
                    let y = t.position[1] - c[1];
                    let radius = self.layout.dpad.radius();
                    let len = x.hypot(y);
                    if len > radius * 0.23 {
                        let x = x / len;
                        let y = y / len;
                        if x > 0.35 {
                            press("right");
                        }
                        if x < -0.35 {
                            press("left");
                        }
                        if y > 0.35 {
                            press("down");
                        }
                        if y < -0.35 {
                            press("up");
                        }
                    }
                }
                _ => {}
            }
        }
        for index in 0..2 {
            let v = self.stick_vector(index);
            pad.axes[index * 2] = (v[0] * 128.).round() as i32;
            pad.axes[index * 2 + 1] = (v[1] * 128.).round() as i32;
        }
        pad
    }
}
pub fn merge(a: Pad, b: Pad) -> Pad {
    let mut p = a;
    p.buttons |= b.buttons;
    for (axis, touch) in p.axes.iter_mut().zip(b.axes) {
        if *axis == 0 {
            *axis = touch;
        }
    }
    p
}
/// Eventos de tecla complementam IHID para jogos como Rally; números não são dígitos.
pub const KEY_BUTTONS: [(&str, u32); 11] = [
    ("b1", 0xe064),
    ("b2", 0xe063),
    ("b3", 0xe066),
    ("b4", 0xe065),
    ("zl", 0xe067),
    ("zr", 0xe068),
    ("home", 0xe030),
    ("up", 0xe031),
    ("down", 0xe032),
    ("left", 0xe033),
    ("right", 0xe034),
];
