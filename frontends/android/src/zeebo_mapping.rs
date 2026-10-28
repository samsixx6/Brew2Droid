//! Bank física exclusiva do Zeebo. Nenhuma entrada desta rota vira dígito Cellphone.
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use zeebx::input::Pad;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Input {
    Key(i32, Option<i32>),
    Axis(i32, bool),
}
impl Input {
    fn parse(s: &str) -> Option<Self> {
        let a: Vec<_> = s.split(':').collect();
        match a.as_slice() {
            ["key", c] => {
                let c = c.parse().ok()?;
                ((1..=1023).contains(&c) && ![3, 4, 26, 164, 187, 223, 224].contains(&c))
                    .then_some(Self::Key(c, None))
            }
            ["key", "4", "scan", s] => {
                let s = s.parse().ok()?;
                (s >= 0).then_some(Self::Key(4, Some(s)))
            }
            ["axis", a, s] => {
                let a = a.parse().ok()?;
                ([0, 1, 11, 12, 13, 14, 15, 16, 17, 18, 22, 23].contains(&a)
                    && ["-", "+"].contains(s))
                .then_some(Self::Axis(a, *s == "+"))
            }
            _ => None,
        }
    }
}
#[derive(Clone, Copy)]
enum Target {
    Button(usize),
    Axis(usize, bool),
}
fn target(s: &str) -> Option<Target> {
    if let Some(i) = super::zeebo_controls::button_index(s) {
        if [
            "b1", "b2", "b3", "b4", "zl", "zr", "home", "up", "down", "left", "right",
        ]
        .contains(&s)
        {
            return Some(Target::Button(i));
        }
    }
    let (i, positive) = match s {
        "lx-" => (0, false),
        "lx+" => (0, true),
        "ly-" => (1, false),
        "ly+" => (1, true),
        "rx-" => (2, false),
        "rx+" => (2, true),
        "ry-" => (3, false),
        "ry+" => (3, true),
        _ => return None,
    };
    Some(Target::Axis(i, positive))
}
fn default_target(input: Input) -> Option<Target> {
    match input {
        Input::Key(c, _) => target(match c {
            96 | 66 | 62 | 54 => "b1",
            99 | 52 => "b2",
            100 | 29 => "b3",
            97 | 47 => "b4",
            102 => "zl",
            103 => "zr",
            4 | 108 | 109 | 110 | 111 => "home",
            19 => "up",
            20 => "down",
            21 => "left",
            22 => "right",
            _ => return None,
        }),
        Input::Axis(a, p) => match a {
            0 | 1 | 11 | 14 => Some(Target::Axis(
                match a {
                    0 => 0,
                    1 => 1,
                    11 => 2,
                    _ => 3,
                },
                p,
            )),
            15 => target(if p { "right" } else { "left" }),
            16 => target(if p { "down" } else { "up" }),
            17 | 23 => target("zl"),
            18 | 22 => target("zr"),
            _ => None,
        },
    }
}
#[derive(Default)]
pub struct Map {
    custom: BTreeMap<Input, Target>,
    held: BTreeMap<(i32, Input), f32>,
    ranges: BTreeMap<(i32, i32), [f32; 3]>,
    known: BTreeSet<i32>,
    volume: bool,
}
impl Map {
    pub fn parse(bindings: &Value, volume: bool) -> Self {
        let mut map = Self {
            volume,
            ..Default::default()
        };
        if let Some(bindings) = bindings.as_object() {
            for (name, value) in bindings {
                if let (Some(t), Some(i)) = (target(name), value.as_str().and_then(Input::parse)) {
                    map.custom.entry(i).or_insert(t);
                }
            }
        }
        map
    }
    fn resolve(&self, input: Input) -> Option<Target> {
        if matches!(input, Input::Key(24 | 25, _)) && !self.volume {
            return None;
        }
        self.custom
            .get(&input)
            .copied()
            .or_else(|| default_target(input))
    }
    fn report(&mut self, device: i32, input: Input, magnitude: f32) -> bool {
        let Some(target) = self.resolve(input) else {
            return false;
        };
        let old = self.held.contains_key(&(device, input));
        let value = match (target, input) {
            (Target::Button(_), Input::Axis(a, _)) => {
                if magnitude
                    > if [15, 16].contains(&a) {
                        0.
                    } else if old {
                        0.35
                    } else {
                        0.65
                    }
                {
                    1.
                } else {
                    0.
                }
            }
            _ => magnitude,
        };
        if value > 0. {
            self.held.insert((device, input), value.clamp(0., 1.));
        } else {
            self.held.remove(&(device, input));
        }
        true
    }
    pub fn key(&mut self, device: i32, code: i32, scan: i32, down: bool) -> bool {
        if code == 4 && (scan <= 0 || device < 0) {
            return false;
        }
        if !(1..=1023).contains(&code) || [3, 26, 164, 187, 223, 224].contains(&code) {
            return false;
        }
        self.report(
            device,
            Input::Key(code, (code == 4).then_some(scan)),
            if down { 1. } else { 0. },
        )
    }
    pub fn physical_back(&mut self, device: i32, scan: i32, down: bool) -> bool {
        if device < 0 || scan < 0 {
            return false;
        }
        self.report(
            device,
            Input::Key(4, Some(scan)),
            if down { 1. } else { 0. },
        )
    }
    pub fn axis(&mut self, device: i32, axis: i32, raw: f32) -> bool {
        if !raw.is_finite() || ![0, 1, 11, 12, 13, 14, 15, 16, 17, 18, 22, 23].contains(&axis) {
            return false;
        }
        let trigger = [17, 18, 22, 23].contains(&axis);
        let [min, max, flat] = self.ranges.get(&(device, axis)).copied().unwrap_or([
            if trigger { 0. } else { -1. },
            1.,
            0.,
        ]);
        let center = if trigger {
            min
        } else if min < 0. && max > 0. {
            0.
        } else {
            (min + max) / 2.
        };
        let delta = raw.clamp(min, max) - center;
        let usable = if delta >= 0. {
            max - center
        } else {
            center - min
        };
        let dead = flat
            .max(if [15, 16].contains(&axis) {
                0.
            } else {
                usable * 0.12
            })
            .clamp(0., usable);
        let v = if usable > dead {
            ((delta.abs() - dead).max(0.) / (usable - dead)).copysign(delta)
        } else {
            0.
        };
        let mut result = false;
        for p in [false, true] {
            result |= self.report(
                device,
                Input::Axis(axis, p),
                if p { v.max(0.) } else { (-v).max(0.) },
            );
        }
        result
    }
    pub fn pad(&self) -> Pad {
        let mut p = Pad::default();
        for ((_, i), m) in &self.held {
            match self.resolve(*i) {
                Some(Target::Button(b)) => p.press(b, true),
                Some(Target::Axis(a, positive)) => {
                    let v = (m * 128.).round() as i32 * if positive { 1 } else { -1 };
                    if v.abs() > p.axes[a].abs() {
                        p.axes[a] = v;
                    }
                }
                _ => {}
            }
        }
        p
    }
    pub fn release_all(&mut self) {
        self.held.clear();
    }
    pub fn release_device(&mut self, device: i32) {
        self.held.retain(|(d, _), _| *d != device);
        self.ranges.retain(|(d, _), _| *d != device);
        self.known.remove(&device);
    }
    pub fn set_volume_keys_mappable(&mut self, on: bool) {
        self.volume = on;
        if !on {
            self.held
                .retain(|(_, i), _| !matches!(i, Input::Key(24 | 25, _)));
        }
    }
    pub fn volume_keys_mappable(&self) -> bool {
        self.volume
    }
    pub fn set_axis_range(&mut self, d: i32, a: i32, min: f32, max: f32, flat: f32) {
        if [min, max, flat].iter().all(|v| v.is_finite()) && max > min {
            self.ranges.insert((d, a), [min, max, flat]);
        }
    }
    pub fn ranges_loaded(&self, d: i32) -> bool {
        self.known.contains(&d)
    }
    pub fn mark_ranges_loaded(&mut self, d: i32) {
        self.known.insert(d);
    }
}
