//! IO da biblioteca nativa. Nada deste módulo roda por quadro ou guarda uma sessão.
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use serde_json::{json, Value};
use zeebx::ui::settings::Settings;
#[path = "native_library_metadata.rs"]
mod metadata;
#[path = "native_library_cache.rs"]
mod library_cache;
pub use library_cache::{library_cached, library_refresh, library_invalidate, cancel_scan, scan_in_progress, begin_gameplay, end_gameplay};
static TEMP_ID: AtomicU64 = AtomicU64::new(0);
static SETTINGS_WRITE: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Debug)]
pub struct KeypadSettings { pub transparency: u8, pub vibration: bool }
impl Default for KeypadSettings {fn default()->Self {Self{transparency:50,vibration:true}}}

pub fn keypad_settings(path: &Path) -> Result<KeypadSettings,String> {
    let raw=read(path)?;
    Ok(KeypadSettings {
        transparency:raw.pointer("/handset_keypad/transparency").and_then(Value::as_u64).filter(|n|*n<=100).unwrap_or(50) as u8,
        vibration:raw.pointer("/handset_keypad/vibration").and_then(Value::as_bool).unwrap_or(true),
    })
}

fn read(path: &Path) -> Result<Value, String> {
    match std::fs::read(path) {
        Ok(bytes) => {
            let value: Value = serde_json::from_slice(&bytes).map_err(|e| format!("Invalid settings: {e}"))?;
            if !value.is_object() { return Err("Settings must be an object".into()); }
            serde_json::from_value::<Settings>(value.clone()).map_err(|e| format!("Invalid settings: {e}"))?;
            Ok(value)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(format!("Cannot read settings: {e}")),
    }
}

fn merge(target: &mut Value, source: &Value) {
    if let (Some(dst), Some(src)) = (target.as_object_mut(), source.as_object()) {
        for (key, value) in src { merge(dst.entry(key).or_insert(Value::Null), value); }
    } else { *target = source.clone(); }
}

fn view(raw: &Value, default_root: &Path) -> Result<Value, String> {
    let mut settings = serde_json::to_value(Settings::default()).map_err(|e| e.to_string())?;
    // O Android nasce com GPU; o merge seguinte conserva uma escolha CPU já salva.
    settings["graphics"]["gpu_rasterizer"] = json!(true);
    settings["audio"]["enabled"] = json!(true);
    settings["audio"]["volume"] = json!(100);
    settings["handset_keypad"] = json!({"transparency":50,"vibration":true});
    merge(&mut settings, raw);
    if settings["roms_dir"].is_null() { settings["roms_dir"] = json!(default_root); }
    let mut catalog = zeebx::ui::i18n::Catalog::default();
    if let Some(code) = settings["language"].as_str() { catalog.select(code); }
    // As chaves embutidas são comuns aos quatro idiomas; Catalog aplica o fallback real.
    let keys: Value = serde_json::from_str(include_str!("../../../assets/lang/en.json")).map_err(|e| e.to_string())?;
    let strings: serde_json::Map<String, Value> = keys.as_object().unwrap().keys()
        .map(|key| (key.clone(), json!(catalog.get(key)))).collect();
    let languages: Vec<_> = catalog.languages().iter().map(|l| json!({"code": l.code, "name": l.name})).collect();
    Ok(json!({"settings":settings,"strings":strings,"languages":languages,"language":catalog.current(),"version":env!("CARGO_PKG_VERSION")}))
}

pub fn snapshot(path: &Path, default_root: &Path) -> Result<Value, String> { view(&read(path)?, default_root) }

/// A sessão e a Activity compartilham o padrão Android sem trocar os defaults desktop.
pub fn settings_for_android(path: &Path) -> Result<Settings, String> {
    let raw = read(path)?;
    // load_from também aplica as migrações históricas de controles e atalhos.
    let mut settings = Settings::load_from(path);
    if raw.pointer("/graphics/gpu_rasterizer").is_none() { settings.graphics.gpu_rasterizer = true; }
    // Defaults do app só preenchem campos ausentes; mute/volume0 salvos continuam válidos.
    if raw.pointer("/audio/enabled").is_none() { settings.audio.enabled = true; }
    if raw.pointer("/audio/volume").is_none() { settings.audio.volume = 100; }
    Ok(settings)
}

fn allowed(key: &str, value: &Value) -> bool {
    match key {
        "language" => value.is_null() || value.as_str().is_some_and(|s| ["en","es","es-MX","pt-BR"].contains(&s)),
        "roms_dir" | "audio.soundfont" => value.is_null() || value.as_str().is_some_and(|s| !s.is_empty()),
        "graphics.scaling" => value.as_str().is_some_and(|s| ["fit","integer","stretch"].contains(&s)),
        "controles_na_tela.modo" => value.as_str().is_some_and(|s| ["automatico","sempre","nunca"].contains(&s)),
        "audio.volume" | "handset_keypad.transparency" => value.as_u64().is_some_and(|n| n <= 100),
        "handset_keypad.vibration" => value.is_boolean(),
        "graphics.keep_aspect" | "graphics.smooth" | "graphics.gpu_rasterizer" |
        "audio.enabled" | "audio.midi_effects" | "debug.fps_counter" | "debug.overlay" |
        "debug.speed" | "debug.clock" | "debug.memory" | "debug.timeline" => value.is_boolean(),
        _ => false,
    }
}

fn validate_patch(patch: &Value, prefix: &str) -> Result<(), String> {
    let object = patch.as_object().ok_or("Settings patch must be an object")?;
    for (key, value) in object {
        let field = if prefix.is_empty() { key.clone() } else { format!("{prefix}.{key}") };
        if prefix.is_empty() && ["graphics","audio","debug","controles_na_tela","handset_keypad"].contains(&key.as_str()) {
            validate_patch(value, &field)?;
        } else if !allowed(&field, value) { return Err(format!("Unsupported setting or value: {field}")); }
    }
    Ok(())
}

pub fn update(path: &Path, default_root: &Path, patch: &Value) -> Result<Value, String> {
    // Activities distintas podem terminar IO quase juntas; serializar só a transação
    // de preferências evita perder um patch entre a leitura e o rename atômico.
    let _writer = SETTINGS_WRITE.lock().map_err(|_| "Settings writer unavailable")?;
    validate_patch(patch, "")?;
    let mut raw = read(path)?;
    merge(&mut raw, patch);
    let result = view(&raw, default_root)?;
    // O arquivo original nunca é truncado. Um erro de escrita não perde a configuração.
    let parent = path.parent().ok_or("Settings have no parent directory")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temp = parent.join(format!(".brew-settings-{}-{}.tmp", std::process::id(), TEMP_ID.fetch_add(1, Ordering::Relaxed)));
    let write = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;
        file.write_all(&serde_json::to_vec_pretty(&raw)?)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)
    })();
    if let Err(e) = write { let _ = std::fs::remove_file(&temp); return Err(format!("Cannot save settings: {e}")); }
    Ok(result)
}

pub fn scan(root: &Path, art_cache: &Path) -> Result<Value, String> {
    // library::scan deliberadamente trata erros como vazio; aqui a UI precisa distinguir acesso negado.
    std::fs::read_dir(root).map_err(|e| format!("Cannot open game folder: {e}"))?;
    std::fs::create_dir_all(art_cache).map_err(|e| e.to_string())?;
    let mut games: Vec<Value> = Vec::new();
    let mut metadata = metadata::Scan::new(root);
    zeebx::library::scan_each(root, |game| {
        let details = metadata.describe(&game.path);
        let title_source = if details.title.is_some() { "folder" } else { "metadata" };
        let company = zeebx::library::resolve_mif(&game.path).ok()
            .and_then(|p|std::fs::read(p).ok()).and_then(|b|metadata::company(&b));
        let art = game.art.and_then(|image| {
            let image = image.downscaled(192);
            use std::hash::{Hash, Hasher};
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            game.path.hash(&mut hash); image.rgba.hash(&mut hash);
            let target = art_cache.join(format!("{:016x}.rgba", hash.finish()));
            let mut bytes = Vec::with_capacity(8 + image.rgba.len());
            bytes.extend_from_slice(&(image.width as u32).to_le_bytes());
            bytes.extend_from_slice(&(image.height as u32).to_le_bytes()); bytes.extend(image.rgba);
            if !target.is_file() && std::fs::write(&target, bytes).is_err() { return None; }
            Some(target)
        });
        games.push(json!({"title": details.title.unwrap_or(game.title), "path": game.path, "art": art,
            "game_folder": details.folder, "folder_size_bytes": details.size, "version": details.version,
            "company": company.unwrap_or_else(||"Unknown".into()), "title_source": title_source}));
    });
    games.sort_by(|a,b| a["title"].as_str().unwrap_or_default().to_lowercase().cmp(&b["title"].as_str().unwrap_or_default().to_lowercase()));
    Ok(json!({"games": games}))
}

/// O cache evita reler módulos inalterados quando a Activity volta à biblioteca.
pub fn scan_with_displays(root:&Path,art_cache:&Path,profiles:&Path)->Result<Value,String> {
    let mut result=scan(root,art_cache)?;
    for game in result["games"].as_array_mut().unwrap() {
        let module=std::path::PathBuf::from(game["path"].as_str().unwrap_or_default());
        match zeebx::handset::display::resolve(&module,profiles) {
            Ok(display)=>{
                if game["title"].as_str() == Some(zeebx::library::title_for(&module).as_str()) {
                    if let Some(title) = metadata::known_title(&display.module_sha256) { game["title"] = json!(title); }
                }
                game["resolution"]=json!(display);
            },
            Err(error)=>game["resolution_error"]=json!(error),
        }
    }
    result["games"].as_array_mut().unwrap().sort_by(|a,b| a["title"].as_str().unwrap_or_default().to_lowercase().cmp(&b["title"].as_str().unwrap_or_default().to_lowercase()));
    Ok(result)
}

pub fn game_display(module:&Path,profiles:&Path,patch:&str)->Result<Value,String> {
    let selection=if patch.is_empty() {zeebx::handset::display::resolve(module,profiles)?}
    else {
        let geometry=serde_json::from_str(patch).map_err(|e|format!("Invalid game resolution: {e}"))?;
        zeebx::handset::display::set_override(module,profiles,geometry)?
    };
    Ok(json!(selection))
}
