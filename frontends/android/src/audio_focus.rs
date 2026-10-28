//! A política de foco fica no Android; o listener só publica um booleano ao loop nativo.
use android_activity::AndroidApp;
use std::sync::atomic::{AtomicBool, Ordering};
static GRANTED: AtomicBool = AtomicBool::new(true);

#[derive(Default)]
pub struct AudioFocus {
    wanted: bool,
}
impl AudioFocus {
    pub fn update(&mut self, app: &AndroidApp, wanted: bool) -> bool {
        if wanted != self.wanted {
            self.wanted = wanted;
            GRANTED.store(!wanted, Ordering::Release);
            if let Err(error) = request(app, wanted) {
                log::warn!("audio focus: {error}; usando foco da janela");
                GRANTED.store(true, Ordering::Release);
            }
        }
        wanted && !GRANTED.load(Ordering::Acquire)
    }
}

fn request(app: &AndroidApp, wanted: bool) -> Result<(), jni::errors::Error> {
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }?;
    let mut env = vm.attach_current_thread()?;
    let result = env.with_local_frame(16, |env| -> Result<(), jni::errors::Error> {
        let activity = unsafe { jni::objects::JObject::from_raw(app.activity_as_ptr().cast()) };
        let loader = env
            .call_method(
                &activity,
                "getClassLoader",
                "()Ljava/lang/ClassLoader;",
                &[],
            )?
            .l()?;
        let name = env.new_string("io.github.brew.handset.AudioFocus")?;
        let class = env
            .call_method(
                &loader,
                "loadClass",
                "(Ljava/lang/String;)Ljava/lang/Class;",
                &[(&name).into()],
            )?
            .l()?;
        env.call_static_method(
            jni::objects::JClass::from(class),
            "setWanted",
            "(Landroid/app/Activity;Z)V",
            &[
                (&activity).into(),
                jni::objects::JValue::Bool(u8::from(wanted)),
            ],
        )?;
        Ok(())
    });
    if result.is_err() && env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    result
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_brew_handset_AudioFocus_changed(
    _: jni::JNIEnv<'_>,
    _: jni::objects::JClass<'_>,
    granted: jni::sys::jboolean,
) {
    GRANTED.store(granted != 0, Ordering::Release);
}
