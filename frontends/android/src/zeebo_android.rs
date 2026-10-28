//! Captura Zeebo exclusiva: multitouch por ID, gamepad e teclado na bank ativa.
use super::{zeebo_controls::Controls, zeebo_mapping::Map};
use android_activity::input::{Axis, InputEvent, KeyAction, MotionAction, ToolType};
fn controller(source: android_activity::input::Source) -> bool {
    let s = u32::from(source);
    s & 0x01000010 == 0x01000010 || s & 0x00000401 == 0x00000401
}
pub fn receive(
    controls: &mut Controls,
    mapping: &mut Map,
    event: &InputEvent,
    scale: f32,
    app: &android_activity::AndroidApp,
    vibration: bool,
) -> bool {
    if let InputEvent::MotionEvent(motion) = event
        && controller(motion.source())
        && !mapping.ranges_loaded(motion.device_id())
    {
        let device = motion.device_id();
        let source = u32::from(motion.source()) as i32;
        let result = (|| -> Result<(), jni::errors::Error> {
            let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }?;
            let mut env = vm.attach_current_thread()?;
            let result = env.with_local_frame(64, |env| -> Result<(), jni::errors::Error> {
                let input = env
                    .call_static_method(
                        "android/view/InputDevice",
                        "getDevice",
                        "(I)Landroid/view/InputDevice;",
                        &[jni::objects::JValue::Int(device)],
                    )?
                    .l()?;
                if input.is_null() {
                    return Ok(());
                }
                for axis in crate::button_mapping::AXES
                    .into_iter()
                    .chain(crate::button_mapping::TRIGGER_ALIASES)
                {
                    let range = env
                        .call_method(
                            &input,
                            "getMotionRange",
                            "(II)Landroid/view/InputDevice$MotionRange;",
                            &[
                                jni::objects::JValue::Int(axis),
                                jni::objects::JValue::Int(source),
                            ],
                        )?
                        .l()?;
                    if range.is_null() {
                        continue;
                    }
                    let min = env.call_method(&range, "getMin", "()F", &[])?.f()?;
                    let max = env.call_method(&range, "getMax", "()F", &[])?.f()?;
                    let flat = env.call_method(&range, "getFlat", "()F", &[])?.f()?;
                    mapping.set_axis_range(device, axis, min, max, flat);
                }
                Ok(())
            });
            if result.is_err() && env.exception_check().unwrap_or(false) {
                let _ = env.exception_clear();
            }
            result
        })();
        if let Err(error) = result {
            log::warn!("faixas do controle {device}: {error}");
        }
        mapping.mark_ranges_loaded(device);
    }

    if !scale.is_finite() || scale <= 0. {
        controls.clear();
        return true;
    }
    match event {
        InputEvent::MotionEvent(motion) if controller(motion.source()) => {
            if motion.action() == MotionAction::Cancel {
                mapping.release_device(motion.device_id());
                return true;
            }
            if let Some(p) = motion.pointers().next() {
                for history in p.history() {
                    for a in crate::button_mapping::AXES
                        .into_iter()
                        .chain(crate::button_mapping::TRIGGER_ALIASES)
                    {
                        mapping.axis(
                            motion.device_id(),
                            a,
                            history.axis_value(Axis::from(a as u32)),
                        );
                    }
                }
                for a in crate::button_mapping::AXES
                    .into_iter()
                    .chain(crate::button_mapping::TRIGGER_ALIASES)
                {
                    mapping.axis(motion.device_id(), a, p.axis_value(Axis::from(a as u32)));
                }
            }
            true
        }
        InputEvent::MotionEvent(motion) => {
            match motion.action() {
                MotionAction::Cancel => controls.clear(),
                MotionAction::Up | MotionAction::PointerUp => {
                    let p = motion.pointer_at_index(motion.pointer_index());
                    controls.end(p.pointer_id());
                }
                MotionAction::Down | MotionAction::PointerDown => {
                    let p = motion.pointer_at_index(motion.pointer_index());
                    if p.tool_type() != ToolType::Palm {
                        if controls.begin(
                            p.pointer_id(),
                            controls.local([
                                p.axis_value(Axis::X) / scale,
                                p.axis_value(Axis::Y) / scale,
                            ]),
                        ) && vibration
                        {
                            crate::sistema::vibra_teclado(app);
                        }
                    }
                }
                MotionAction::Move => {
                    for p in motion
                        .pointers()
                        .filter(|p| p.tool_type() != ToolType::Palm)
                    {
                        controls.move_to(
                            p.pointer_id(),
                            controls.local([
                                p.axis_value(Axis::X) / scale,
                                p.axis_value(Axis::Y) / scale,
                            ]),
                        );
                    }
                }
                _ => {}
            }
            true
        }
        InputEvent::KeyEvent(key) => {
            if !matches!(key.action(), KeyAction::Down | KeyAction::Up) {
                return true;
            }
            let code = u32::from(key.key_code()) as i32;
            let changed = mapping.key(
                key.device_id(),
                code,
                key.scan_code(),
                key.action() == KeyAction::Down,
            );
            changed || !matches!(code, 3 | 4 | 24 | 25 | 26 | 164 | 187 | 223 | 224)
        }
        _ => false,
    }
}
