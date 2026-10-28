//! Ponte de IO e de ciclo de vida. Não participa da execução de quadros.
use android_activity::AndroidApp;
use jni::{objects::{JClass, JObject, JString, JValue}, JNIEnv};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
pub static GAME_ACTIVE: AtomicBool = AtomicBool::new(false);

fn string(env: &mut JNIEnv<'_>, value: JString<'_>) -> Result<String,String> {
    env.get_string(&value).map(Into::into).map_err(|e| e.to_string())
}
fn response(env: &mut JNIEnv<'_>, result: Result<serde_json::Value,String>) -> jni::sys::jstring {
    let value = match result {Ok(data)=>serde_json::json!({"ok":true,"data":data}),Err(error)=>serde_json::json!({"ok":false,"error":error})};
    env.new_string(value.to_string()).map(JString::into_raw).unwrap_or(std::ptr::null_mut())
}
fn localize(env: &mut JNIEnv<'_>, mut data: serde_json::Value) -> serde_json::Value {
    if !data["settings"]["language"].is_null() { return data; }
    let preferred = (|| -> Result<String,jni::errors::Error> {
        let locale=env.call_static_method("java/util/Locale","getDefault","()Ljava/util/Locale;",&[])?.l()?;
        let tag=env.call_method(locale,"toLanguageTag","()Ljava/lang/String;",&[])?.l()?;
        Ok(env.get_string(&JString::from(tag))?.into())
    })().unwrap_or_else(|_| "en".into());
    let mut catalog=zeebx::ui::i18n::Catalog::default();catalog.select_best(&preferred);
    data["language"]=serde_json::json!(catalog.current());
    if let Some(strings)=data["strings"].as_object_mut() {
        for (key,value) in strings { *value=serde_json::json!(catalog.get(key)); }
    }
    data
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_brew_handset_NativeBridge_snapshot(mut env: JNIEnv<'_>, _: JClass<'_>, config:JString<'_>, root:JString<'_>)->jni::sys::jstring {
    let result=(|| {let config=string(&mut env,config)?;let root=string(&mut env,root)?;
        crate::native_ui_model::snapshot(Path::new(&config),Path::new(&root))})();
    let result=result.map(|v|localize(&mut env,v));response(&mut env,result)
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_brew_handset_NativeBridge_update(mut env: JNIEnv<'_>, _:JClass<'_>,config:JString<'_>,root:JString<'_>,patch:JString<'_>)->jni::sys::jstring {
    let result=(|| {
        if GAME_ACTIVE.load(Ordering::Acquire) {return Err("Close the game before changing settings".into());}
        let config=string(&mut env,config)?;let root=string(&mut env,root)?;let patch=string(&mut env,patch)?;
        let patch=serde_json::from_str(&patch).map_err(|e|format!("Invalid settings patch: {e}"))?;
        crate::native_ui_model::update(Path::new(&config),Path::new(&root),&patch)
    })();let result=result.map(|v|localize(&mut env,v));response(&mut env,result)
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_brew_handset_NativeBridge_scan(mut env:JNIEnv<'_>,_:JClass<'_>,root:JString<'_>,cache:JString<'_>,profiles:JString<'_>)->jni::sys::jstring {
    let result=(|| {let root=string(&mut env,root)?;let cache=string(&mut env,cache)?;let profiles=string(&mut env,profiles)?;
        crate::native_ui_model::library_refresh(Path::new(&root),Path::new(&cache),Path::new(&profiles),true)})();response(&mut env,result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_brew_handset_NativeBridge_library(mut env:JNIEnv<'_>,_:JClass<'_>,root:JString<'_>,cache:JString<'_>,profiles:JString<'_>,public:JString<'_>,private:JString<'_>,config:JString<'_>,refresh:jni::sys::jboolean,force:jni::sys::jboolean)->jni::sys::jstring {
    let result=(|| {let root=string(&mut env,root)?;let cache=string(&mut env,cache)?;let profiles=string(&mut env,profiles)?;
        let public=string(&mut env,public)?;let private=string(&mut env,private)?;let config=string(&mut env,config)?;
        let mut library=if refresh!=0&&!GAME_ACTIVE.load(Ordering::Acquire){crate::native_ui_model::library_refresh(Path::new(&root),Path::new(&cache),Path::new(&profiles),force!=0)}
        else{crate::native_ui_model::library_cached(Path::new(&root),Path::new(&cache))}?;
        crate::profile_store::project_library(Path::new(&public),Path::new(&private),Path::new(&config),&mut library)?;
        Ok(library)
    })();response(&mut env,result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_brew_handset_NativeBridge_cancelScan(_:JNIEnv<'_>,_:JClass<'_>){crate::native_ui_model::cancel_scan();}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_brew_handset_NativeBridge_profiles(mut env:JNIEnv<'_>,_:JClass<'_>,public:JString<'_>,private:JString<'_>,config:JString<'_>,module:JString<'_>,request:JString<'_>)->jni::sys::jstring {
    let result=(|| {
        if GAME_ACTIVE.load(Ordering::Acquire){return Err("Close the game before changing profiles".into());}
        let public=string(&mut env,public)?;let private=string(&mut env,private)?;let config=string(&mut env,config)?;let module=string(&mut env,module)?;let request=string(&mut env,request)?;
        let request=serde_json::from_str(&request).map_err(|e|format!("Invalid profile request: {e}"))?;
        crate::profile_store::request(Path::new(&public),Path::new(&private),Path::new(&config),if module.is_empty(){None}else{Some(Path::new(&module))},&request)
    })();response(&mut env,result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_brew_handset_NativeBridge_gameOptions(mut env:JNIEnv<'_>,_:JClass<'_>,root:JString<'_>,module:JString<'_>,patch:JString<'_>)->jni::sys::jstring {
    let result=(|| {if GAME_ACTIVE.load(Ordering::Acquire){return Err("Close the game before changing its settings".into());}
        let root=string(&mut env,root)?;let module=string(&mut env,module)?;let patch=string(&mut env,patch)?;
        crate::game_options::view(Path::new(&root),Path::new(&module),&patch)
    })();response(&mut env,result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_brew_handset_NativeBridge_gameDisplay(mut env:JNIEnv<'_>,_:JClass<'_>,profiles:JString<'_>,module:JString<'_>,patch:JString<'_>)->jni::sys::jstring {
    let result=(|| {
        if GAME_ACTIVE.load(Ordering::Acquire){return Err("Close the game before changing its resolution".into());}
        let profiles=string(&mut env,profiles)?;let module=string(&mut env,module)?;let patch=string(&mut env,patch)?;
        crate::native_ui_model::game_display(Path::new(&module),Path::new(&profiles),&patch)
    })();response(&mut env,result)
}

pub struct Launch { pub game: Option<PathBuf>, pub probe:bool }
pub fn launch(app:&AndroidApp)->Result<Launch,String> {
    let read=|| -> Result<Launch,jni::errors::Error> {
        let vm=unsafe{jni::JavaVM::from_raw(app.vm_as_ptr().cast())}?;let mut env=vm.attach_current_thread()?;
        let activity=unsafe{JObject::from_raw(app.activity_as_ptr().cast())};
        let intent=env.call_method(activity,"getIntent","()Landroid/content/Intent;",&[])?.l()?;
        let key=env.new_string("brew.game_path")?;
        let path=env.call_method(&intent,"getStringExtra","(Ljava/lang/String;)Ljava/lang/String;",&[(&key).into()])?.l()?;
        let game=if path.is_null(){None}else{Some(PathBuf::from(String::from(env.get_string(&JString::from(path))?)))};
        let key=env.new_string("brew.probe_only")?;
        let probe=env.call_method(intent,"getBooleanExtra","(Ljava/lang/String;Z)Z",&[(&key).into(),JValue::Bool(0)])?.z()?;
        Ok(Launch{game,probe})
    };
    let launch=read().map_err(|e|e.to_string())?;
    if launch.game.is_some()&&!storage_access(app)?{return Err("Allow Storage Permission in Brew2Droid before opening this game.".into());}
    Ok(launch)
}
fn storage_access(app:&AndroidApp)->Result<bool,String>{
    let read=||->Result<bool,jni::errors::Error>{
        let vm=unsafe{jni::JavaVM::from_raw(app.vm_as_ptr().cast())}?;let mut env=vm.attach_current_thread()?;
        let api=env.get_static_field("android/os/Build$VERSION","SDK_INT","I")?.i()?;
        if api>=30{return env.call_static_method("android/os/Environment","isExternalStorageManager","()Z",&[])?.z();}
        let activity=unsafe{JObject::from_raw(app.activity_as_ptr().cast())};
        for name in ["android.permission.READ_EXTERNAL_STORAGE","android.permission.WRITE_EXTERNAL_STORAGE"]{
            let permission=env.new_string(name)?;
            if env.call_method(&activity,"checkSelfPermission","(Ljava/lang/String;)I",&[(&permission).into()])?.i()?!=0{return Ok(false);}
        }
        Ok(true)
    };read().map_err(|e|format!("Could not check storage permission: {e}"))
}
pub fn finish(app:&AndroidApp,error:Option<&str>) {
    let error=error.map(str::to_owned);let app=app.clone();
    app.clone().run_on_java_main_thread(Box::new(move || {
        let result=(|| -> Result<(),jni::errors::Error>{
            let vm=unsafe{jni::JavaVM::from_raw(app.vm_as_ptr().cast())}?;let mut env=vm.attach_current_thread()?;
            let activity=unsafe{JObject::from_raw(app.activity_as_ptr().cast())};
            let intent=env.new_object("android/content/Intent","()V",&[])?;
            if let Some(error)=&error {
                let key=env.new_string("brew.error")?;let value=env.new_string(error)?;
                env.call_method(&intent,"putExtra","(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;",&[(&key).into(),(&value).into()])?;
            }
            env.call_method(&activity,"setResult","(ILandroid/content/Intent;)V",&[JValue::Int(if error.is_some(){0}else{-1}),(&intent).into()])?;
            env.call_method(activity,"finish","()V",&[])?;Ok(())
        })();if let Err(error)=result{log::error!("native activity finish: {error}");}
    }));
}
