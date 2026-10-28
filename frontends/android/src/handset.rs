//! Teclado móvel experimental, mantendo os dedos fora do ponteiro único do egui.

use android_activity::input::{Axis, InputEvent, KeyAction, MotionAction, ToolType};
pub use crate::button_mapping::InputMap;
use zeebx::handset::touch::TouchKeypad;
use zeebx::handset::types::DisplayGeometry;

#[derive(Default)]
pub struct TouchRoutes {keypad:std::collections::BTreeMap<i32,bool>}
impl TouchRoutes {pub fn clear(&mut self){self.keypad.clear();}}
pub fn receive_touch(keys:&mut TouchKeypad,touch:&mut zeebx::handset::touch::Touchscreen,routes:&mut TouchRoutes,event:&InputEvent,scale:f32)->Option<Option<zeebx::handset::input::PenEvent>>{
    let InputEvent::MotionEvent(motion)=event else{return None;};if controller(motion.source()){return None;}
    if motion.action()==MotionAction::Cancel||!scale.is_finite()||scale<=0.0{routes.clear();keys.pointers([]);return Some(touch.cancel());}
    if !matches!(motion.action(),MotionAction::Down|MotionAction::PointerDown|MotionAction::Move|MotionAction::Up|MotionAction::PointerUp){return Some(None);}
    let points:Vec<_>=motion.pointers().filter(|p|p.tool_type()!=ToolType::Palm).map(|p|(p.pointer_id(),[p.axis_value(Axis::X)/scale,p.axis_value(Axis::Y)/scale])).collect();
    if motion.action()==MotionAction::Down{routes.clear();}
    routes.keypad.retain(|id,_|points.iter().any(|(pointer,_)|id==pointer));
    // O destino pertence ao dedo desde o início do gesto, mesmo ao cruzar a imagem ou outra tecla.
    for (id,point) in &points{routes.keypad.entry(*id).or_insert_with(||keys.is_enabled()&&keys.regions.iter().any(|r|point[0]>=r.min[0]&&point[0]<r.max[0]&&point[1]>=r.min[1]&&point[1]<r.max[1]));}
    let up=matches!(motion.action(),MotionAction::Up|MotionAction::PointerUp).then(||motion.pointer_at_index(motion.pointer_index()).pointer_id());
    keys.pointers(points.iter().filter(|(id,_)|Some(*id)!=up&&routes.keypad.get(id)==Some(&true)).map(|(_,point)|*point));
    let pen=if let Some(id)=up{let point=points.iter().find(|(pointer,_)|*pointer==id).map(|(_,point)|*point).unwrap_or([0.0,0.0]);touch.end(id,point)}else{touch.update(points.iter().filter(|(id,_)|routes.keypad.get(id)!=Some(&true)).copied())};
    if let Some(id)=up{routes.keypad.remove(&id);}
    Some(pen)
}

pub fn vibration_press(keys:&TouchKeypad,event:&InputEvent,scale:f32,enabled:bool)->bool {
    if !enabled||!keys.is_enabled()||!scale.is_finite()||scale<=0.0{return false;}
    let InputEvent::MotionEvent(motion)=event else{return false;};
    if controller(motion.source())||!matches!(motion.action(),MotionAction::Down|MotionAction::PointerDown){return false;}
    let Some(pointer)=motion.pointers().find(|p|p.pointer_index()==motion.pointer_index()&&p.tool_type()!=ToolType::Palm) else{return false;};
    let point=[pointer.axis_value(Axis::X)/scale,pointer.axis_value(Axis::Y)/scale];
    keys.regions.iter().any(|r|point[0]>=r.min[0]&&point[0]<r.max[0]&&point[1]>=r.min[1]&&point[1]<r.max[1])
}

pub fn geometry(path: &std::path::Path, profiles: &std::path::Path) -> Result<DisplayGeometry, String> {
    let mif=zeebx::library::resolve_mif(path)?;
    let package = zeebx::handset::package::read_module_pair(path, &mif)
        .map_err(|error| error.to_string())?;
    Ok(zeebx::handset::display::resolve_hash(path,profiles,&package.report.module_sha256)?.geometry)
}

#[cfg(target_os="android")]
pub fn profile_geometry(path:&std::path::Path,resolution:Option<DisplayGeometry>)->Result<DisplayGeometry,String>{
    let mif=zeebx::library::resolve_mif(path)?;let package=zeebx::handset::package::read_module_pair(path,&mif).map_err(|e|e.to_string())?;
    Ok(crate::profile_runtime::geometry(path,&package.report.module_sha256,resolution))
}

pub fn receive(keys: &mut TouchKeypad, mapping: &mut InputMap, event: &InputEvent, scale: f32) -> bool {
    let handled = match event {
        InputEvent::MotionEvent(motion) if controller(motion.source()) => {
            if motion.action() == MotionAction::Cancel {
                mapping.release_device(motion.device_id());
                true
            } else if let Some(pointer) = motion.pointers().next() {
                let mut handled = false;
                for historical in pointer.history() {
                    for axis in crate::button_mapping::AXES.into_iter().chain(crate::button_mapping::TRIGGER_ALIASES) {
                        handled |= mapping.axis(motion.device_id(),axis,historical.axis_value(Axis::from(axis as u32)));
                    }
                }
                for axis in crate::button_mapping::AXES.into_iter().chain(crate::button_mapping::TRIGGER_ALIASES) {
                    handled |= mapping.axis(motion.device_id(), axis, pointer.axis_value(Axis::from(axis as u32)));
                }
                handled
            } else { false }
        }
        InputEvent::MotionEvent(motion) => {
            let action = motion.action();
            if action == MotionAction::Cancel { keys.pointers([]); return true; }
            let up = matches!(action, MotionAction::Up | MotionAction::PointerUp).then_some(motion.pointer_index());
            keys.pointers(motion.pointers()
                .filter(|p| Some(p.pointer_index()) != up && p.tool_type() != ToolType::Palm)
                .map(|p| [p.axis_value(Axis::X) / scale, p.axis_value(Axis::Y) / scale]));
            true
        }
        InputEvent::KeyEvent(event) => match event.action() {
            KeyAction::Down | KeyAction::Up => mapping.key(event.device_id(),u32::from(event.key_code()) as i32,
                event.scan_code(),event.action() == KeyAction::Down),
            _ => false,
        },
        _ => false,
    };
    if handled { mapping.apply(keys); }
    handled
}

/// MotionRange pertence ao Android; cada controle só exige esta consulta na primeira entrada.
#[cfg(target_os="android")]
pub fn receive_with_app(keys: &mut TouchKeypad, mapping: &mut InputMap, event: &InputEvent, scale: f32,
    app: &android_activity::AndroidApp) -> bool {
    if let InputEvent::MotionEvent(motion) = event
        && controller(motion.source()) && !mapping.ranges_loaded(motion.device_id()) {
        let device = motion.device_id();
        let source = u32::from(motion.source()) as i32;
        let result = (|| -> Result<(),jni::errors::Error> {
            let vm = unsafe {jni::JavaVM::from_raw(app.vm_as_ptr().cast())}?;
            let mut env = vm.attach_current_thread()?;
            let result = env.with_local_frame(64,|env| -> Result<(),jni::errors::Error> {
                let input = env.call_static_method("android/view/InputDevice","getDevice","(I)Landroid/view/InputDevice;",
                    &[jni::objects::JValue::Int(device)])?.l()?;
                if input.is_null() {return Ok(());}
                for axis in crate::button_mapping::AXES.into_iter().chain(crate::button_mapping::TRIGGER_ALIASES) {
                    let range = env.call_method(&input,"getMotionRange","(II)Landroid/view/InputDevice$MotionRange;",
                        &[jni::objects::JValue::Int(axis),jni::objects::JValue::Int(source)])?.l()?;
                    if range.is_null() {continue;}
                    let min = env.call_method(&range,"getMin","()F",&[])?.f()?;
                    let max = env.call_method(&range,"getMax","()F",&[])?.f()?;
                    let flat = env.call_method(&range,"getFlat","()F",&[])?.f()?;
                    mapping.set_axis_range(device,axis,min,max,flat);
                }
                Ok(())
            });
            if result.is_err() && env.exception_check().unwrap_or(false) {let _=env.exception_clear();}
            result
        })();
        if let Err(error) = result {log::warn!("faixas do controle {device}: {error}");}
        mapping.mark_ranges_loaded(device);
    }
    receive(keys,mapping,event,scale)
}

fn controller(source: android_activity::input::Source) -> bool {
    let source = u32::from(source);
    source & 0x01000010 == 0x01000010 || source & 0x00000401 == 0x00000401
}
pub fn receive_pen(touch: &mut zeebx::handset::touch::Touchscreen, event: &InputEvent, scale: f32)
    -> Option<zeebx::handset::input::PenEvent> {
    let InputEvent::MotionEvent(motion) = event else { return None; };
    if controller(motion.source()) { return None; }
    match motion.action() {
        MotionAction::Cancel => touch.cancel(),
        MotionAction::Up | MotionAction::PointerUp => {
            let pointer = motion.pointer_at_index(motion.pointer_index());
            touch.end(pointer.pointer_id(),[pointer.axis_value(Axis::X)/scale,pointer.axis_value(Axis::Y)/scale])
        }
        MotionAction::Down | MotionAction::PointerDown | MotionAction::Move => {
            touch.update(motion.pointers().filter(|p| p.tool_type()!=ToolType::Palm)
                .map(|p| (p.pointer_id(),[p.axis_value(Axis::X)/scale,p.axis_value(Axis::Y)/scale])))
        }
        _ => None,
    }
}

#[cfg(target_os="android")]
#[path = "keypad_art.rs"]
mod keypad_art;
#[cfg(target_os="android")]
pub use keypad_art::{paint,paint_layout};
