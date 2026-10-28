//! Roteamento físico compartilhado com ButtonBindings.java; não consulta disco durante o jogo.
use zeebx::handset::input::Key;
use std::collections::{BTreeMap,BTreeSet};
use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum Input {
    Key { code: i32, scan: Option<i32> },
    Axis { axis: i32, positive: bool },
}
impl Input {
    fn parse(token: &str) -> Option<Self> {
        let parts: Vec<_> = token.split(':').collect();
        match parts.as_slice() {
            ["key", code] => {
                let code = code.parse::<i32>().ok()?;
                ((1..=1023).contains(&code) && ![3,4,26,164,187,223,224].contains(&code))
                    .then_some(Self::Key { code, scan: None })
            }
            ["key", "4", "scan", scan] => {
                let scan = scan.parse::<i32>().ok()?;
                (scan >= 0).then_some(Self::Key { code: 4, scan: Some(scan) })
            }
            ["axis", axis, sign] => {
                let axis = axis.parse::<i32>().ok()?;
                ((AXES.contains(&axis) || TRIGGER_ALIASES.contains(&axis)) && ["+","-"].contains(sign))
                    .then_some(Self::Axis { axis, positive: *sign == "+" })
            }
            _ => None,
        }
    }
    fn default_target(self) -> Option<Key> {
        match self {
            Self::Key { code: 7..=16, .. } => {
                let Self::Key { code, .. } = self else { unreachable!() };
                Some(Key::ALL[(code - 7) as usize])
            }
            Self::Key { code, .. } => Some(match code {
                17 => Key::Star, 18 => Key::Pound,
                19 => Key::Up, 20 => Key::Down, 21 => Key::Left, 22 => Key::Right,
                23 | 66 | 96 => Key::Select,
                1 | 102 | 82 => Key::Soft1, 2 | 103 | 4 => Key::Soft2,
                28 | 67 => Key::Clear,
                _ => return None,
            }),
            Self::Axis { axis: 15, positive } => Some(if positive {Key::Right} else {Key::Left}),
            Self::Axis { axis: 16, positive } => Some(if positive {Key::Down} else {Key::Up}),
            _ => None,
        }
    }
}

pub const AXES: [i32; 10] = [0,1,11,12,13,14,15,16,17,18];
// Gas/Brake são aliases de gatilhos em controles Android mais antigos.
pub const TRIGGER_ALIASES: [i32; 2] = [22,23];

#[derive(Default)]
pub struct InputMap {
    volume_keys_mappable: bool,
    custom: BTreeMap<Input, Key>,
    held: BTreeMap<(i32, Input), Key>,
    ranges: BTreeMap<(i32,i32),[f32;3]>,
    known_devices: BTreeSet<i32>,
}
impl InputMap {
    pub fn apply(&self, keys: &mut zeebx::handset::touch::TouchKeypad) {
        let held=self.held();
        for key in Key::ALL {keys.hardware(key,held & (1 << key as u8) != 0);}
    }
    pub fn set_axis_range(&mut self, device: i32, axis: i32, min: f32, max: f32, flat: f32) {
        if [min,max,flat].iter().all(|v| v.is_finite()) && max > min {
            self.ranges.insert((device,axis),[min,max,flat]);
        }
    }
    pub fn ranges_loaded(&self, device: i32) -> bool { self.known_devices.contains(&device) }
    pub fn mark_ranges_loaded(&mut self, device: i32) { self.known_devices.insert(device); }
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path).ok().filter(|s| s.len() <= 65536)
            .map(|s| Self::parse(&s)).unwrap_or_default()
    }
    pub fn parse(text: &str) -> Self {
        let mut result = Self::default();
        let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {return result};
        if value["version"].as_u64() != Some(1) {return result;}
        result.volume_keys_mappable = value["volume_keys_mappable"].as_bool().unwrap_or(false);
        let Some(bindings) = value["bindings"].as_object() else {return result};
        // A ordem fixa também define qual atribuição vence em arquivo editado/corrompido.
        for (index,name) in TARGETS.iter().enumerate() {
            if let Some(input) = bindings.get(*name).and_then(|v| v.as_str()).and_then(Input::parse) {
                result.custom.entry(input).or_insert(Key::ALL[index]);
            }
        }
        result
    }
    pub fn volume_keys_mappable(&self) -> bool { self.volume_keys_mappable }
    pub fn set_volume_keys_mappable(&mut self, enabled: bool) {
        self.volume_keys_mappable=enabled;
        if !enabled {self.held.retain(|(_,input),_|!matches!(input,Input::Key{code:24|25,..}));}
    }
    fn resolve(&self, input: Input) -> Option<Key> {
        if matches!(input,Input::Key{code:24|25,..}) && !self.volume_keys_mappable {return None;}
        self.custom.get(&input).copied().or_else(|| input.default_target())
    }
    fn report(&mut self, device: i32, input: Input, down: bool) -> bool {
        let source = (device,input);
        if !down && self.held.remove(&source).is_some() { return true; }
        let Some(target) = self.resolve(input) else {return false};
        if down { self.held.insert(source,target); }
        true
    }
    pub fn key(&mut self, device: i32, code: i32, scan: i32, down: bool) -> bool {
        if code == 4 && (scan <= 0 || device < 0) {return false;}
        if !(1..=1023).contains(&code) || [3,26,164,187,223,224].contains(&code) {return false;}
        self.report(device,Input::Key {code,scan:(code == 4).then_some(scan)},down)
    }
    // O host confirmou a origem física; scan zero também existe em teclados OEM.
    pub fn physical_back(&mut self,device:i32,scan:i32,down:bool)->bool {
        if device<0 || scan<0 {return false;}
        self.report(device,Input::Key{code:4,scan:Some(scan)},down)
    }
    pub fn axis(&mut self, device: i32, axis: i32, value: f32) -> bool {
        if !AXES.contains(&axis) && !TRIGGER_ALIASES.contains(&axis) || !value.is_finite() {return false;}
        let trigger = [17,18,22,23].contains(&axis);
        let [min,max,flat] = self.ranges.get(&(device,axis)).copied().unwrap_or([if trigger {0.0} else {-1.0},1.0,0.0]);
        let center = if trigger {min} else if min < 0.0 && max > 0.0 {0.0} else {(min+max)/2.0};
        let delta = value.clamp(min,max)-center;
        let usable = if delta >= 0.0 {max-center} else {center-min};
        let dead = flat.clamp(0.0,usable);
        let magnitude = if usable > dead {(delta.abs()-dead).max(0.0)/(usable-dead)} else {0.0};
        let value = magnitude.copysign(delta);
        let mut handled = false;
        for positive in [false,true] {
            let input = Input::Axis {axis,positive};
            let magnitude = if positive {value} else {-value};
            let threshold = if [15,16].contains(&axis) {0.0}
                else if self.held.contains_key(&(device,input)) {0.35} else {0.65};
            let down = if [15,16].contains(&axis) {magnitude > 0.0} else {magnitude >= threshold};
            handled |= self.report(device,input,down);
        }
        handled
    }
    pub fn held(&self) -> u32 { self.held.values().fold(0,|bits,key| bits | (1 << *key as u8)) }
    pub fn release_all(&mut self) { self.held.clear(); }
    pub fn release_device(&mut self, device: i32) {
        self.held.retain(|(id,_),_| *id != device);
        self.ranges.retain(|(id,_),_| *id != device);self.known_devices.remove(&device);
    }
    pub fn reset(&mut self) { *self = Self::default(); }
    pub fn target(&self, token: &str) -> Option<Key> { self.resolve(Input::parse(token)?) }
}

const TARGETS: [&str; 20] = ["0","1","2","3","4","5","6","7","8","9","*","#",
    "up","down","left","right","select","soft1","soft2","clear"];

static REMOVED_DEVICES: std::sync::Mutex<Vec<i32>> = std::sync::Mutex::new(Vec::new());

pub fn queue_removed_device(device: i32) {
    let mut removed=REMOVED_DEVICES.lock().unwrap_or_else(|poison|poison.into_inner());
    if !removed.contains(&device) {removed.push(device);}
}
pub fn take_removed_devices() -> Vec<i32> {
    let mut removed=REMOVED_DEVICES.lock().unwrap_or_else(|poison|poison.into_inner());
    std::mem::take(&mut *removed)
}

#[cfg(target_os="android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_brew_handset_GameActivity_nativeInputDeviceRemoved(
    _env: jni::JNIEnv<'_>,_class: jni::objects::JClass<'_>,device: jni::sys::jint,
) {
    crate::input_dispatch::discard_physical_back_device(device);
    queue_removed_device(device);
}
