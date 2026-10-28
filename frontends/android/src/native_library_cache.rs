//! Índice descartável da biblioteca. A leitura quente só abre o JSON, nunca a árvore de ROMs.
use super::metadata;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, atomic::{AtomicBool, AtomicU64, Ordering}};
use std::time::UNIX_EPOCH;

// O esquema 5 reconstrói arte com alfa/GIF e exclui extensões comprovadas.
const SCHEMA: u64 = 5;
const MAX_ENTRIES: usize = 500_000;
const MAX_MIF: u64 = 1024 * 1024;
const MAX_INDEX: u64 = 32 * 1024 * 1024;
static SCANS: Mutex<()> = Mutex::new(());
static EPOCH: AtomicU64 = AtomicU64::new(0);
static RUNNING: AtomicBool = AtomicBool::new(false);
static GAMEPLAY_ACTIVE: AtomicBool = AtomicBool::new(false);
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Running;
impl Drop for Running { fn drop(&mut self) { RUNNING.store(false, Ordering::Release); } }

pub fn scan_in_progress() -> bool { RUNNING.load(Ordering::Acquire) }

/// A barreira retorna somente depois de a descoberta em curso largar o disco.
/// A Activity também bloqueia novos pedidos de refresh enquanto a partida existe.
pub fn cancel_scan() {
    EPOCH.fetch_add(1, Ordering::AcqRel);
    let _scan = SCANS.lock().unwrap_or_else(|poisoned|poisoned.into_inner());
}

/// Fechar a entrada antes da barreira cobre o worker Java que passou pela geração
/// antiga, mas só chega ao JNI depois de o cancelamento ter encontrado SCANS ocioso.
pub fn begin_gameplay() {
    GAMEPLAY_ACTIVE.store(true, Ordering::Release);
    cancel_scan();
}

/// A sessão e o emulador já devem estar destruídos quando a descoberta for liberada.
pub fn end_gameplay() { GAMEPLAY_ACTIVE.store(false, Ordering::Release); }

fn check(epoch: u64) -> Result<(), String> {
    if EPOCH.load(Ordering::Acquire) == epoch { Ok(()) } else { Err("Library scan cancelled".into()) }
}

fn normalized(path: &Path) -> Result<PathBuf, String> {
    let path = std::fs::canonicalize(path).or_else(|_|std::path::absolute(path)).map_err(|e|e.to_string())?;
    // A forma verbatim do Windows depende de o diretório ainda estar montado.
    // Remover só o prefixo de drive conserva a mesma chave quando ele fica indisponível.
    #[cfg(windows)]
    if let Some(text) = path.to_str().and_then(|s|s.strip_prefix("\\\\?\\")) {
        if !text.starts_with("UNC\\") { return Ok(PathBuf::from(text)); }
    }
    Ok(path)
}

fn digest(value: impl Hash) -> String {
    let mut hash = std::collections::hash_map::DefaultHasher::new(); value.hash(&mut hash); format!("{:016x}",hash.finish())
}
fn index_path(root: &Path, art: &Path) -> PathBuf { art.join(format!("library-v{SCHEMA}-{}.json",digest(root))) }

fn read_index(root: &Path, art: &Path) -> Result<Option<Value>, String> {
    let file = match std::fs::File::open(index_path(root,art)) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Cannot read library cache: {e}")),
    };
    let mut bytes = Vec::new(); file.take(MAX_INDEX + 1).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
    if bytes.len() as u64 > MAX_INDEX { return Ok(None); }
    let Some(index) = serde_json::from_slice::<Value>(&bytes).ok() else { return Ok(None); };
    if index["schema"] != SCHEMA || index["root"] != json!(root) { return Ok(None); }
    let Some(entries) = index["entries"].as_object() else { return Ok(None); };
    for (path,entry) in entries {
        let row = &entry["row"];
        if row.is_null() && entry["ignored"] == true && entry["fingerprint"].is_string() { continue; }
        if !Path::new(path).starts_with(root) || row["path"].as_str() != Some(path)
            || !row["title"].is_string() || !row["game_folder"].is_string()
            || row["content_id"].as_str().and_then(zeebx::storage::ContentId::parse).is_none()
            || !entry["fingerprint"].is_string() { return Ok(None); }
    }
    Ok(Some(index))
}

fn response(index: Option<&Value>, reused: usize, processed: usize, cached: bool) -> Value {
    let mut games: Vec<Value> = index.and_then(|i|i["entries"].as_object()).into_iter().flatten()
        .filter_map(|(_,entry)|entry["row"].as_object().map(|_|entry["row"].clone())).collect();
    // Uma cópia sem MIF também pode ser a mesma extensão conhecida. Um applet
    // explícito tem precedência: metadados conflitantes não provam exclusividade.
    let ids_for = |role: &str| -> BTreeSet<String> {
        games.iter().filter(|g| g["module_role"].as_str() == Some(role))
            .filter_map(|g| g["content_id"].as_str().map(str::to_owned)).collect()
    };
    let extensions = ids_for("extension");
    let applets = ids_for("applet");
    games.retain(|game| {
        let role = game["module_role"].as_str();
        role != Some("extension") && (role == Some("applet")
            || game["content_id"].as_str().is_none_or(|id| !extensions.contains(id) || applets.contains(id)))
    });
    // O empréstimo existe só na resposta: remover/alterar o doador invalida a
    // associação sem deixar arte antiga gravada na linha do receptor. Nomes nunca
    // provam identidade; somente o ContentId completo do binário serve de chave.
    let mut artwork: BTreeMap<String, Option<String>> = BTreeMap::new();
    for game in &games {
        if let (Some(id), Some(art)) = (game["content_id"].as_str(), game["art"].as_str()) {
            artwork.entry(id.to_owned()).and_modify(|previous| {
                if previous.as_deref() != Some(art) { *previous = None; }
            }).or_insert_with(|| Some(art.to_owned()));
        }
    }
    for game in &mut games {
        if game["art"].is_null() {
            if let Some(Some(art)) = game["content_id"].as_str().and_then(|id| artwork.get(id)) {
                game["art"] = json!(art);
            }
        }
    }
    games.sort_by(|a,b|a["title"].as_str().unwrap_or_default().to_lowercase().cmp(&b["title"].as_str().unwrap_or_default().to_lowercase())
        .then_with(||a["path"].as_str().cmp(&b["path"].as_str())));
    json!({"games":games,"cached":cached,"reused":reused,"processed":processed})
}

pub fn library_cached(root: &Path, art_cache: &Path) -> Result<Value,String> {
    let root = normalized(root)?;
    let index = read_index(&root,art_cache)?;
    let count = index.as_ref().and_then(|i|i["entries"].as_object()).map_or(0,|e|e.len());
    Ok(response(index.as_ref(),count,0,index.is_some()))
}

fn stamp(meta: &std::fs::Metadata, kind: &str) -> Value {
    let time = |time: std::io::Result<std::time::SystemTime>|time.ok().and_then(|t|t.duration_since(UNIX_EPOCH).ok())
        .map(|t|json!([t.as_secs(),t.subsec_nanos()]));
    // ctime/change-time fecha a lacuna de substituições de mesmo tamanho e mtime.
    #[cfg(unix)]
    let changed = { use std::os::unix::fs::MetadataExt; json!([meta.ctime(),meta.ctime_nsec()]) };
    #[cfg(not(unix))]
    let changed = json!(time(meta.created()));
    json!({"kind":kind,"length":meta.len(),"modified":time(meta.modified()),"changed":changed})
}

struct Inventory {
    files: BTreeMap<PathBuf,Value>,
    modules: Vec<PathBuf>,
    manifests: BTreeMap<String,Vec<PathBuf>>,
}
fn extension(path: &Path, ext: &str) -> bool { path.extension().is_some_and(|e|e.to_string_lossy().eq_ignore_ascii_case(ext)) }
fn key(name: &std::ffi::OsStr) -> String { name.to_string_lossy().to_lowercase() }

fn discover(root: &Path, art: &Path, profiles: &Path, epoch: u64) -> Result<Inventory,String> {
    let mut inventory = Inventory { files:BTreeMap::new(), modules:Vec::new(), manifests:BTreeMap::new() };
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        check(epoch)?;
        let entries = std::fs::read_dir(&folder).map_err(|e|format!("Cannot open game folder {}: {e}",folder.display()))?;
        for entry in entries {
            check(epoch)?;
            if inventory.files.len() >= MAX_ENTRIES { return Err("Library entry limit reached; choose a smaller collection".into()); }
            let entry = entry.map_err(|e|e.to_string())?; let path = entry.path();
            if path == art || path == profiles { continue; }
            let meta = std::fs::symlink_metadata(&path).map_err(|e|e.to_string())?;
            let kind = if meta.file_type().is_symlink() { "link" } else if meta.is_dir() { "dir" } else if meta.is_file() { "file" } else { "other" };
            inventory.files.insert(path.clone(),stamp(&meta,kind));
            if kind == "dir" { folders.push(path); }
            else if kind == "file" {
                if extension(&path,"mod") || zeebx::loader::archive::embalado(&path) { inventory.modules.push(path.clone()); }
                if extension(&path,"mif") { if let Some(stem) = path.file_stem() { inventory.manifests.entry(key(stem)).or_default().push(path); } }
            }
        }
    }
    inventory.modules.sort(); for paths in inventory.manifests.values_mut() { paths.sort(); }
    Ok(inventory)
}

impl Inventory {
    fn candidates(&self, root: &Path, module: &Path) -> Vec<PathBuf> {
        let Some(dir) = module.parent() else { return Vec::new(); };
        let mut names = Vec::new();
        for name in [module.file_stem(),dir.file_name()].into_iter().flatten() { let name=key(name); if !names.contains(&name) { names.push(name); } }
        // O grupo de maior precedência é inteiro: cópias conflitantes invalidam a
        // unicidade, mas um MIF global não invalida uma pareação local já demonstrada.
        let shared = dir.ancestors().take_while(|p|p.starts_with(root))
            .find(|p|p.file_name().is_some_and(|n|n.to_string_lossy().eq_ignore_ascii_case("mod")))
            .and_then(Path::parent).unwrap_or(root);
        for name in &names {
            let matching: Vec<_> = self.manifests.get(name).into_iter().flatten().filter(|p|p.parent()==Some(dir)).cloned().collect();
            if !matching.is_empty() { return matching; }
        }
        if let Some(name)=dir.file_name() {
            let matching: Vec<_> = self.manifests.get(&key(name)).into_iter().flatten().filter(|p|p.parent()==dir.parent()).cloned().collect();
            if !matching.is_empty() { return matching; }
        }
        for scope in [shared,root] {
            for name in &names {
                let matching: Vec<_> = self.manifests.get(name).into_iter().flatten().filter(|p| {
                    p.strip_prefix(scope).is_ok_and(|p|p.components().next().is_some_and(|c|c.as_os_str().to_string_lossy().eq_ignore_ascii_case("mif")))
                }).cloned().collect();
                if !matching.is_empty() { return matching; }
            }
        }
        Vec::new()
    }

    fn fingerprint(&self, root: &Path, module: &Path, folder: &Path, candidates: &[PathBuf], profiles: &Path, previous: Option<&Value>) -> String {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        module.hash(&mut hash);
        for (path, stamp) in self.files.range(folder.to_path_buf()..) {
            if !path.starts_with(folder) { break; }
            // Um módulo solto não é dono da coleção inteira. Capas homônimas e
            // manifestos pareados são as únicas dependências externas demonstráveis.
            if folder == root && previous.is_none_or(|r|r["folder_size_bytes"].is_null()) && path != module
                && !["png","jpg","jpeg","bmp"].iter().any(|ext|*path == module.with_extension(ext)) { continue; }
            path.hash(&mut hash);
            // As datas das pastas também mudam ao adicionar arquivos; os caminhos e
            // os arquivos presentes já detectam isso sem invalidar por uma data isolada.
            if stamp["kind"] != "dir" { stamp.to_string().hash(&mut hash); }
        }
        for candidate in candidates { candidate.hash(&mut hash); self.files.get(candidate).map(Value::to_string).hash(&mut hash); }
        if let Some(identity) = previous.and_then(|r|r["resolution_override_identity"].as_str()) {
            let path=profiles.join("overrides").join(format!("{identity}.json"));
            std::fs::metadata(path).ok().map(|m|stamp(&m,"file").to_string()).hash(&mut hash);
        } else { Option::<String>::None.hash(&mut hash); }
        format!("{:016x}",hash.finish())
    }
}

fn read_mif(path: &Path, epoch: u64) -> Result<Vec<u8>,String> {
    check(epoch)?;
    let mut bytes = Vec::new(); std::fs::File::open(path).map_err(|e|e.to_string())?
        .take(MAX_MIF+1).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
    check(epoch)?;
    if bytes.len() as u64 > MAX_MIF { return Err("MIF exceeds inspection limit".into()); }
    Ok(bytes)
}
fn unique(paths: &[PathBuf], epoch: u64) -> Result<Option<PathBuf>,String> {
    let Some(first) = paths.first() else { return Ok(None); };
    if paths.len() > 1 {
        let original = read_mif(first,epoch)?;
        for path in &paths[1..] { if read_mif(path,epoch)? != original { return Ok(None); } }
    }
    Ok(Some(first.clone()))
}

fn art(module: &Path, bytes: Option<&[u8]>, cache: &Path, epoch: u64) -> Result<Option<PathBuf>,String> {
    check(epoch)?;
    let image = zeebx::library::cover(module).or_else(|| zeebx::library::manifest_art(bytes?));
    check(epoch)?;
    let Some(image) = image else { return Ok(None); };
    let image=image.downscaled(192); let path=cache.join(format!("{}.rgba",digest((image.width,image.height,&image.rgba))));
    if !path.is_file() {
        let mut bytes = Vec::with_capacity(8+image.rgba.len()); bytes.extend_from_slice(&(image.width as u32).to_le_bytes());
        bytes.extend_from_slice(&(image.height as u32).to_le_bytes()); bytes.extend(image.rgba);
        check(epoch)?; std::fs::write(&path,bytes).map_err(|e|e.to_string())?;
    }
    Ok(Some(path))
}

fn row(root: &Path, module: &Path, candidates: &[PathBuf], inventory: &Inventory, art_cache: &Path, profiles: &Path, epoch: u64) -> Result<Option<Value>,String> {
    check(epoch)?;
    let packed=zeebx::loader::archive::embalado(module);
    let manifest=unique(candidates,epoch)?;
    let internal = if packed { zeebx::loader::archive::find_module(module) } else { None };
    check(epoch)?;
    if packed && internal.is_none() { return Ok(None); }
    let data = if packed { internal.as_ref().and_then(|inner|zeebx::loader::archive::find_manifest(module,inner)) }
        else { manifest.as_deref().map(|p|read_mif(p,epoch)).transpose()? };
    let parsed = data.as_deref().and_then(|b| zeebx::handset::package::parse_mif(b).ok());
    let role = if parsed.as_ref().is_some_and(|mif| !mif.extensao().is_empty()) { "extension" }
        else if parsed.as_ref().is_some_and(|mif| mif.main_applet().is_some()) { "applet" } else { "unknown" };
    if role == "extension" {
        let content=zeebx::storage::ContentId::from_reader(std::fs::File::open(module).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        return Ok(Some(json!({"title":zeebx::library::title_for(module),"path":module,"art":null,
            "game_folder":module.parent().unwrap_or(root),"content_id":content.as_str(),"module_role":role})));
    }
    let details = metadata::from_inventory(root,module,manifest.as_deref(),&inventory.files);
    let folder_title=details.title.is_some();
    let title=details.title.unwrap_or_else(||internal.as_ref().map(|inner|zeebx::loader::archive::title_of(module,inner)).unwrap_or_else(||zeebx::library::title_for(module)));
    let image = art(module,data.as_deref(),art_cache,epoch)?;
    let company=data.as_deref().and_then(metadata::company).unwrap_or_else(||"Unknown".into());
    let mut row=json!({"title":title,"path":module,"art":image,"game_folder":details.folder,"folder_size_bytes":details.size,
        "version":details.version,"company":company,"module_role":role,"title_source":if folder_title {"folder"} else {"metadata"}});
    check(epoch)?;
    // A assinatura do inventário inclui ctime/criação, enquanto o cache de tela
    // histórico só usa tamanho e mtime. Toda linha alterada precisa de um hash novo.
    zeebx::handset::display::invalidate_cache(module, profiles)?; check(epoch)?;
    match zeebx::handset::display::resolve(module,profiles) {
        Ok(display) => {
            if !folder_title { if let Some(title)=metadata::known_title(&display.module_sha256) { row["title"]=json!(title); } }
            row["resolution_override_identity"]=json!(zeebx::handset::display::override_identity(module,profiles,&display.module_sha256)?);
            row["detected_platform"]=json!(if zeebx::handset::platform::recognize(&display.module_sha256,module){"zeebo"}else{"cellphone"});
            row["resolution"]=json!(display);
        },
        Err(error) => row["resolution_error"]=json!(error),
    }
    let content=zeebx::storage::ContentId::from_reader(std::fs::File::open(module).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    row["content_id"]=json!(content.as_str());
    check(epoch)?; Ok(Some(row))
}

fn save(root: &Path, art: &Path, index: &Value, epoch: u64) -> Result<(),String> {
    check(epoch)?;
    std::fs::create_dir_all(art).map_err(|e|e.to_string())?;
    let target=index_path(root,art); let temp=art.join(format!(".library-{}-{}.tmp",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
    let result=(|| -> Result<(),String> {
        let bytes=serde_json::to_vec(index).map_err(|e|e.to_string())?;
        if bytes.len() as u64 > MAX_INDEX { return Err("Library index exceeds size limit".into()); }
        let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&temp).map_err(|e|e.to_string())?;
        for chunk in bytes.chunks(64*1024) { check(epoch)?; file.write_all(chunk).map_err(|e|e.to_string())?; }
        file.sync_all().map_err(|e|e.to_string())?; drop(file); check(epoch)?;
        std::fs::rename(&temp,&target).map_err(|e|e.to_string())
    })();
    if result.is_err() { let _=std::fs::remove_file(&temp); } result
}

pub fn library_refresh(root: &Path, art_cache: &Path, profiles: &Path, force: bool) -> Result<Value,String> {
    // Capturar antes do lock também cancela pedidos já enfileirados na Activity.
    let epoch=EPOCH.load(Ordering::Acquire);
    let _scan=SCANS.lock().map_err(|_|"Library scanner unavailable")?;
    // A geração sozinha só alcança pedidos já iniciados. Esta verificação sob o
    // mesmo mutex da barreira também rejeita chamadas atrasadas durante a partida.
    if GAMEPLAY_ACTIVE.load(Ordering::Acquire) { return Err("Library refresh blocked during gameplay".into()); }
    check(epoch)?;
    RUNNING.store(true,Ordering::Release); let _running=Running;
    let root=normalized(root)?; let art_cache=normalized(art_cache)?; let profiles=normalized(profiles)?;
    let previous=read_index(&root,&art_cache)?;
    let update=(|| -> Result<Value,String> {
        check(epoch)?;
        let inventory=discover(&root,&art_cache,&profiles,epoch)?;
        check(epoch)?; std::fs::create_dir_all(&art_cache).map_err(|e|e.to_string())?;
        let mut entries=serde_json::Map::new(); let (mut reused,mut processed)=(0,0);
        for module in &inventory.modules {
            check(epoch)?;
            let key=module.to_string_lossy().to_string();
            let old=previous.as_ref().and_then(|i|i["entries"].get(&key));
            let candidates=inventory.candidates(&root,module);
            let folder=old.and_then(|e|e["row"]["game_folder"].as_str()).map(PathBuf::from)
                .filter(|p|p.starts_with(&root)).unwrap_or_else(||module.parent().unwrap_or(&root).to_path_buf());
            let fingerprint=inventory.fingerprint(&root,module,&folder,&candidates,&profiles,old.map(|e|&e["row"]));
            let entry=if !force && old.is_some_and(|e|e["fingerprint"].as_str()==Some(&fingerprint)) {
                reused+=1; old.unwrap().clone()
            } else {
                let row=row(&root,module,&candidates,&inventory,&art_cache,&profiles,epoch)?;
                if old.is_some_and(|e|e["row"]["resolution"].is_object()) && row.as_ref().is_some_and(|r|r["resolution_error"].is_string()) {
                    return Err(row.as_ref().unwrap()["resolution_error"].as_str().unwrap().to_string());
                }
                let folder=row.as_ref().and_then(|r|r["game_folder"].as_str()).map(PathBuf::from).unwrap_or(folder);
                let fingerprint=inventory.fingerprint(&root,module,&folder,&candidates,&profiles,row.as_ref());
                processed+=1; json!({"fingerprint":fingerprint,"ignored":row.is_none(),"row":row})
            };
            entries.insert(key,entry);
        }
        check(epoch)?;
        let index=json!({"schema":SCHEMA,"root":root,"entries":entries});
        save(&root,&art_cache,&index,epoch)?;
        Ok(response(Some(&index),reused,processed,false))
    })();
    match update {
        Ok(result) => Ok(result),
        Err(error) => {
            check(epoch)?;
            if let Some(index)=previous {
                let mut result=response(Some(&index),index["entries"].as_object().map_or(0,|e|e.len()),0,true);
                result["refresh_error"]=json!(error); Ok(result)
            } else { Err(error) }
        },
    }
}

/// Preferências alteradas fora da descoberta invalidam só a linha correspondente.
/// A linha continua visível enquanto o refresh seguinte calcula sua substituta.
pub fn library_invalidate(root: &Path, art_cache: &Path, module: &Path) -> Result<(),String> {
    let _scan=SCANS.lock().map_err(|_|"Library scanner unavailable")?;
    let root=normalized(root)?; let module=normalized(module)?;
    if !module.starts_with(&root) { return Err("Game is outside the selected collection".into()); }
    let Some(mut index)=read_index(&root,art_cache)? else { return Ok(()); };
    if let Some(entry)=index["entries"].get_mut(module.to_string_lossy().as_ref()) { entry["fingerprint"]=json!(""); }
    save(&root,art_cache,&index,EPOCH.load(Ordering::Acquire))
}
