//! Catálogo público e vínculos privados, resolvidos antes de a sessão começar.
#[path="profile_values.rs"]
pub mod values;
use serde_json::{Value,json};
use std::io::{Read,Write};
use std::path::{Path,PathBuf};
use std::sync::{Mutex,atomic::{AtomicU64,Ordering}};
use zeebx::storage::ContentId;
static WRITE:Mutex<()>=Mutex::new(());
static NEXT:AtomicU64=AtomicU64::new(0);
const MAX_BYTES:u64=4*1024*1024;

fn missing_parent_is_directory(path:&Path)->Result<(),String>{
    // Windows também devolve NotFound quando um componente intermediário é um arquivo.
    let mut parent=path.parent();
    while let Some(directory)=parent{
        match std::fs::metadata(directory){
            Ok(metadata)=>return if metadata.is_dir(){Ok(())}else{Err(format!("Not a profile directory: {}",directory.display()))},
            Err(e)if e.kind()==std::io::ErrorKind::NotFound=>parent=directory.parent(),
            Err(e)=>return Err(format!("Cannot access {}: {e}",directory.display())),
        }
    }
    Ok(())
}
fn read(path:&Path)->Result<Option<Value>,String> {
    let file=match std::fs::File::open(path){Ok(f)=>f,Err(e)if e.kind()==std::io::ErrorKind::NotFound=>{missing_parent_is_directory(path)?;return Ok(None);},Err(e)=>return Err(format!("Cannot read {}: {e}",path.display()))};
    let mut bytes=Vec::new();file.take(MAX_BYTES+1).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
    if bytes.len() as u64>MAX_BYTES{return Err(format!("File is too large: {}",path.display()));}
    serde_json::from_slice(&bytes).map(Some).map_err(|e|format!("Invalid JSON in {}: {e}",path.display()))
}

fn save(path:&Path,value:&Value)->Result<(),String> {
    let parent=path.parent().ok_or("Profile file has no directory")?;
    std::fs::create_dir_all(parent).map_err(|e|format!("Cannot create {}: {e}",parent.display()))?;
    let bytes=serde_json::to_vec_pretty(value).map_err(|e|e.to_string())?;
    if bytes.len() as u64>MAX_BYTES{return Err("Profile catalog is too large".into());}
    let temp=parent.join(format!(".brew-profiles-{}-{}.tmp",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
    let result=(||->std::io::Result<()>{let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;file.write_all(&bytes)?;file.sync_all()?;drop(file);std::fs::rename(&temp,path)})();
    if let Err(e)=result{let _=std::fs::remove_file(&temp);return Err(format!("Cannot save {}: {e}",path.display()));}
    Ok(())
}

fn legacy_values(private:&Path,config:&Path)->Result<Value,String> {
    let mut result=values::defaults();
    let raw=read(config)?.unwrap_or(json!({}));
    if !raw.is_object(){return Err("Settings must be an object".into());}
    for (source,target) in [("/graphics/scaling","scaling"),("/graphics/smooth","smooth"),("/graphics/keep_aspect","keep_aspect")] {
        if let Some(value)=raw.pointer(source){result=values::patch(&result,&json!({target:value}))?;}
    }
    if let Some(mode)=raw.pointer("/controles_na_tela/modo").and_then(Value::as_str){result["controls_enabled"]=json!(mode!="nunca");}
    if let Some(n)=raw.pointer("/handset_keypad/transparency").and_then(Value::as_u64).filter(|n|*n<=100){result["transparency"]=json!(100-n);}
    if let Ok(Some(mapping))=read(&private.join("button-mapping.json")) {
        if mapping["version"]==1 {
            if let Some(bindings)=mapping.get("bindings") {
                if let Ok(next)=values::patch(&result,&json!({"bindings":bindings})){result=next;}
            }
        }
    }
    Ok(result)
}

fn name(raw:&Value)->Result<String,String> {
    let raw=raw.as_str().ok_or("Enter a profile name")?.trim();
    if raw.is_empty()||raw.chars().count()>80||raw.chars().any(char::is_control)||raw.eq_ignore_ascii_case("custom") {return Err("Use a profile name from 1 to 80 characters (Custom is reserved)".into());}
    Ok(raw.into())
}

pub fn migrate_catalog(legacy:&Path,public:&Path)->Result<(),String> {
    if legacy==public || std::fs::canonicalize(legacy).ok().zip(std::fs::canonicalize(public).ok()).is_some_and(|(a,b)|a==b){return Ok(());}
    let Some(source)=read(&legacy.join("profiles.json"))? else{return Ok(());};
    validate_catalog(source.clone())?;
    let target=public.join("profiles.json");
    let marker=public.join(".legacy-profiles-import.json");
    match read(&target)? {
        Some(current)=>{
            validate_catalog(current.clone())?;
            if current==source || read(&marker)?.as_ref()==Some(&source){
                if !marker.exists(){save(&marker,&source)?;}return Ok(());
            }
            return Err(format!("Conflicting profile catalogs: {} and {}. Both files were preserved; reconcile their profiles before continuing.",legacy.join("profiles.json").display(),target.display()));
        },
        None=>save(&target,&source)?,
    }
    save(&marker,&source)
}
fn migrate_public_root(public:&Path)->Result<(),String>{
    if public.file_name().is_some_and(|s|s=="profiles") {
        if let Some(base)=public.parent().filter(|p|p.file_name().is_some_and(|s|s=="brew2droid")).and_then(Path::parent){
            migrate_catalog(&base.join("Brew2Droid/profiles"),public)?;
        }
    }
    Ok(())
}
fn catalog(public:&Path,private:&Path,config:&Path)->Result<Value,String> {
    migrate_public_root(public)?;
    match std::fs::metadata(public){Ok(m)if !m.is_dir()=>return Err("Profiles root is not a directory".into()),Err(e)if e.kind()!=std::io::ErrorKind::NotFound=>return Err(e.to_string()),_=>{}}
    let path=public.join("profiles.json");
    let result=match read(&path)? {
        Some(raw)=>raw,
        None=>{
            let raw=json!({"version":1,"default_id":"default","profiles":{"default":{"name":"Default","values":legacy_values(private,config)?}}});
            save(&path,&raw)?;raw
        }
    };
    validate_catalog(result)
}
fn validate_catalog(mut result:Value)->Result<Value,String>{
    if result["version"]!=1{return Err("Unsupported profile catalog version".into());}
    let default=result["default_id"].as_str().ok_or("Profile catalog has no default")?.to_owned();
    let profiles=result["profiles"].as_object_mut().ok_or("Invalid profiles catalog")?;
    if !profiles.contains_key("default")||!profiles.contains_key(&default)||profiles.len()>256{return Err("Invalid default profile or too many profiles".into());}
    let mut used=std::collections::BTreeSet::new();
    for profile in profiles.values_mut(){
        let label=name(&profile["name"])?;
        if !used.insert(label.to_lowercase()){return Err("Duplicate profile name in catalog".into());}
        profile["name"]=json!(label);profile["values"]=values::normalize(&profile["values"])?;
    }
    Ok(result)
}

fn profile<'a>(catalog:&'a Value,id:&str)->Result<&'a Value,String> {
    catalog["profiles"].get(id).ok_or_else(||"Profile no longer exists".into())
}
fn default_id(catalog:&Value)->&str {catalog["default_id"].as_str().unwrap_or("default")}
fn profile_view(catalog:&Value,id:&str)->Result<Value,String> {
    let value=profile(catalog,id)?;
    Ok(json!({"id":id,"name":value["name"],"profile_id":id,"profile_name":value["name"],"mode":"profile","protected":id=="default","selected":default_id(catalog)==id,"values":value["values"]}))
}
fn list(catalog:&Value)->Value {
    let mut profiles:Vec<_>=catalog["profiles"].as_object().unwrap().iter().map(|(id,value)|json!({"id":id,"name":value["name"],"selected":id==default_id(catalog),"protected":id=="default"})).collect();
    profiles.sort_by_key(|p|(p["id"]!="default",p["name"].as_str().unwrap_or("").to_lowercase()));
    json!({"profiles":profiles,"default_id":default_id(catalog)})
}
fn game_path(private:&Path,content:&ContentId)->PathBuf {private.join("profile-games").join(format!("{}.json",content.as_str()))}

fn legacy_game(private:&Path,module:&Path,content:&ContentId,base:&Value,cached:Option<&Value>)->Result<Option<Value>,String> {
    let mut values=base.clone();let mut changed=false;
    if let Some(old)=read(&private.join("game-options").join(format!("{}.json",content.as_str())))? {
        if !old.is_object(){return Err("Invalid legacy game options".into());}
        for (key,default) in [("orientation","auto"),("gravity","center")] {
            if let Some(value)=old.get(key){
                values=values::patch(&values,&json!({key:value}))?;
                // Center era o padrão antigo; não descartá-lo quando o perfil agora herda Top.
                changed|=value!=default||(key=="gravity"&&value!=&base[key]);
            }
        }
        match old["controls"].as_str(){Some("on")=>{values["controls_enabled"]=json!(true);changed=true;},Some("off")=>{values["controls_enabled"]=json!(false);changed=true;},Some("system_default")|None=>{},_=>return Err("Invalid legacy on-screen controls".into())}
    }
    let displays=private.join("handset-displays");
    if displays.exists(){
        let selection=match cached{Some(value)=>value.clone(),None=>serde_json::to_value(zeebx::handset::display::resolve(module,&displays)?).map_err(|e|e.to_string())?};
        if selection["source"]=="manual"{values=values::patch(&values,&json!({"resolution":selection["geometry"]}))?;changed=true;}
    }
    Ok(changed.then(||json!({"version":1,"mode":"custom","values":values})))
}

fn game(catalog:&Value,private:&Path,module:&Path,content:&ContentId)->Result<Value,String> {
    let path=game_path(private,content);
    let stored=match read(&path)? {
        Some(raw)=>Some(raw),
        None=>{
            let migrated=legacy_game(private,module,content,&catalog["profiles"][default_id(catalog)]["values"],None)?;
            if let Some(value)=&migrated{save(&path,value)?;}migrated
        }
    };
    if let Some(raw)=&stored {
        if raw["version"]!=1{return Err("Unsupported game profile version".into());}
        if raw["mode"]=="custom" {
            return Ok(json!({"id":null,"mode":"custom","profile_id":null,"profile_name":"Custom","values":values::normalize(&raw["values"])?}));
        }
        if raw["mode"]!="default"&&raw["mode"]!="profile"{return Err("Invalid game profile mode".into());}
        if raw["mode"]=="profile"&&raw["profile_id"].as_str().is_none(){return Err("Invalid game profile link".into());}
    }
    let named=stored.as_ref().filter(|v|v["mode"]=="profile").and_then(|v|v["profile_id"].as_str()).filter(|id|catalog["profiles"].get(*id).is_some());
    let id=named.unwrap_or_else(||default_id(catalog));
    let mut result=profile_view(catalog,id)?;result["mode"]=json!(if named.is_some(){"profile"}else{"default"});Ok(result)
}

pub fn effective(public:&Path,private:&Path,config:&Path,module:&Path,content:&ContentId)->Result<Value,String> {
    let _lock=WRITE.lock().map_err(|_|"Profile writer unavailable")?;
    let catalog=catalog(public,private,config)?;decorate(game(&catalog,private,module,content)?,Some(module),private)
}

fn decorate(mut state:Value,module:Option<&Path>,private:&Path)->Result<Value,String>{
 if !state["values"].is_object(){return Ok(state);}
 let detected=if let Some(module)=module{
  let display=zeebx::handset::display::resolve(module,&private.join("handset-displays"))?;
  state["automatic_resolution"]=json!(display.automatic.geometry);
  zeebx::handset::platform::recognize(&display.module_sha256,module)
 }else{false};
 let kind=values::button_type(&state["values"],detected);
 state["button_type"]=json!(kind);state["detected_platform"]=json!(if detected{"zeebo"}else{"cellphone"});
 if kind=="zeebo" && state["values"]["resolution"].is_null(){state["automatic_resolution"]=json!({"width":640,"height":480,"depth":16});}
 Ok(state)
}
pub fn request(public:&Path,private:&Path,config:&Path,module:Option<&Path>,request:&Value)->Result<Value,String>{
 decorate(request_inner(public,private,config,module,request)?,module,private)
}
fn request_inner(public:&Path,private:&Path,config:&Path,module:Option<&Path>,request:&Value)->Result<Value,String> {
    let operation=request["op"].as_str().ok_or("Profile operation is missing")?;
    if !["list","create","view","rename","delete","set_default","update","game","game_update","game_profile","game_reset"].contains(&operation){return Err("Unsupported profile operation".into());}
    let _lock=WRITE.lock().map_err(|_|"Profile writer unavailable")?;
    let mut catalog=catalog(public,private,config)?;
    if operation.starts_with("game") {
        let module=module.ok_or("Game module is missing")?;
        let content=ContentId::from_reader(std::fs::File::open(module).map_err(|e|format!("Cannot open game module: {e}"))?).map_err(|e|e.to_string())?;
        let current=game(&catalog,private,module,&content)?;
        let next=match operation {
            "game"=>return Ok(current),
            "game_reset"=>json!({"version":1,"mode":"default"}),
            "game_profile"=>{let id=request["id"].as_str().ok_or("Profile is missing")?;profile(&catalog,id)?;json!({"version":1,"mode":"profile","profile_id":id})},
            "game_update"=>{
                let patch=&request["patch"];
                let next=values::patch(&current["values"],patch)?;
                if patch.as_object().is_some_and(|p|p.is_empty()){return Ok(current);}
                json!({"version":1,"mode":"custom","values":next})
            },
            _=>return Err("Unsupported game profile operation".into()),
        };
        save(&game_path(private,&content),&next)?;
        return game(&catalog,private,module,&content);
    }
    if operation=="list"{return Ok(list(&catalog));}
    let id=if operation=="create" {
        if catalog["profiles"].as_object().unwrap().len()>=256{return Err("Too many profiles".into());}
        format!("p_{:x}_{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e|e.to_string())?.as_nanos(),NEXT.fetch_add(1,Ordering::Relaxed))
    }else{request["id"].as_str().ok_or("Profile is missing")?.to_owned()};
    if operation!="create"{profile(&catalog,&id)?;}
    if operation=="view"{return profile_view(&catalog,&id);}
    match operation {
        "create"|"rename"=>{
            let label=name(&request["name"])?;
            if catalog["profiles"].as_object().unwrap().iter().any(|(other,p)|other!=&id&&p["name"].as_str().is_some_and(|s|s.to_lowercase()==label.to_lowercase())) {return Err("A profile with this name already exists".into());}
            if operation=="create"{catalog["profiles"][&id]=json!({"name":label,"values":catalog["profiles"][default_id(&catalog)]["values"]});}
            else {catalog["profiles"][&id]["name"]=json!(label);}
        },
        "delete"=>{
            if id=="default"{return Err("The built-in Default profile cannot be deleted".into());}
            if default_id(&catalog)==id{catalog["default_id"]=json!("default");}
            catalog["profiles"].as_object_mut().unwrap().remove(&id);
        },
        "set_default"=>catalog["default_id"]=json!(id),
        "update"=>catalog["profiles"][&id]["values"]=values::patch(&catalog["profiles"][&id]["values"],&request["patch"])? ,
        _=>return Err("Unsupported profile operation".into()),
    }
    save(&public.join("profiles.json"),&catalog)?;
    if ["delete","set_default"].contains(&operation){Ok(list(&catalog))}else{profile_view(&catalog,&id)}
}

/// Uma leitura de catálogo e de vínculos; nenhuma abertura/hash de ROM ao voltar dos ajustes.
pub fn project_library(public:&Path,private:&Path,config:&Path,library:&mut Value)->Result<(),String>{
    let _lock=WRITE.lock().map_err(|_|"Profile writer unavailable")?;
    let catalog=catalog(public,private,config)?;
    let mut links=std::collections::BTreeMap::new();
    match std::fs::read_dir(private.join("profile-games")) {
        Ok(entries)=>for entry in entries {
            let path=entry.map_err(|e|e.to_string())?.path();
            if path.extension().is_some_and(|s|s=="json") {
                if let Some(id)=path.file_stem().and_then(|s|s.to_str()).and_then(|s|ContentId::parse(s.to_owned())) {
                    if let Some(raw)=read(&path)?{links.insert(id.as_str().to_owned(),raw);}
                }
            }
        },
        Err(e) if e.kind()==std::io::ErrorKind::NotFound=>missing_parent_is_directory(&private.join("profile-games/placeholder"))?,
        Err(e)=>return Err(format!("Cannot read game profile links: {e}")),
    }
    let legacy_displays=match std::fs::metadata(private.join("handset-displays")) {
        Ok(metadata) if metadata.is_dir()=>true,
        Ok(_)=>return Err("Legacy display storage is not a directory".into()),
        Err(e) if e.kind()==std::io::ErrorKind::NotFound=>{missing_parent_is_directory(&private.join("handset-displays/placeholder"))?;false},
        Err(e)=>return Err(format!("Cannot read legacy display storage: {e}")),
    };
    let mut legacy_options=std::collections::BTreeSet::new();
    match std::fs::read_dir(private.join("game-options")) {
        Ok(entries)=>for entry in entries {
            let path=entry.map_err(|e|format!("Cannot read legacy game options: {e}"))?.path();
            if path.extension().is_some_and(|s|s=="json") {
                if let Some(id)=path.file_stem().and_then(|s|s.to_str()){legacy_options.insert(id.to_owned());}
            }
        },
        Err(e) if e.kind()==std::io::ErrorKind::NotFound=>missing_parent_is_directory(&private.join("game-options/placeholder"))?,
        Err(e)=>return Err(format!("Cannot read legacy game options: {e}")),
    }
    if let Some(rows)=library["games"].as_array_mut(){for row in rows {
        let Some(content)=row["content_id"].as_str().and_then(|s|ContentId::parse(s.to_owned())) else{continue;};
        // A projeção anterior não pode parecer um override legado na próxima volta.
        let cached=row.get("cached_resolution").cloned().unwrap_or_else(||row["resolution"].clone());
        row["cached_resolution"]=cached.clone();
        if !links.contains_key(content.as_str()) && (legacy_options.contains(content.as_str()) || (legacy_displays&&cached["source"]=="manual")) {
            let module=Path::new(row["path"].as_str().unwrap_or_default());
            let migrated=legacy_game(private,module,&content,&catalog["profiles"][default_id(&catalog)]["values"],Some(&cached))?;
            if let Some(raw)=migrated{save(&game_path(private,&content),&raw)?;links.insert(content.as_str().to_owned(),raw);}
        }
        let stored=links.get(content.as_str());
        if let Some(raw)=stored {
            if raw["version"]!=1 || !["custom","default","profile"].iter().any(|m|raw["mode"]==*m){return Err("Invalid game profile link".into());}
        }
        let named=stored.filter(|v|v["mode"]=="profile").and_then(|v|v["profile_id"].as_str());
        if stored.is_some_and(|v|v["mode"]=="profile")&&named.is_none(){return Err("Invalid game profile link".into());}
        let id=named.filter(|id|catalog["profiles"].get(*id).is_some()).unwrap_or_else(||default_id(&catalog));
        let values=if let Some(raw)=stored.filter(|v|v["mode"]=="custom"){values::normalize(&raw["values"])?}else{catalog["profiles"][id]["values"].clone()};
        let mut automatic=row["resolution"]["automatic"].clone();
        let detected=row["detected_platform"]=="zeebo" || zeebx::handset::platform::known(row["resolution"]["module_sha256"].as_str().unwrap_or(""));
        let kind=values::button_type(&values,detected);row["button_type"]=json!(kind);
        if kind=="zeebo" && automatic.is_object(){automatic["geometry"]=json!({"width":640,"height":480,"depth":16});}
        if !automatic.is_object(){continue;}
        row["resolution"]["geometry"]=if values["resolution"].is_null(){automatic["geometry"].clone()}else{values["resolution"].clone()};
        row["resolution"]["source"]=if values["resolution"].is_null(){automatic["source"].clone()}else{json!("manual")};
        row["profile_name"]=if stored.is_some_and(|v|v["mode"]=="custom"){json!("Custom")}else{catalog["profiles"][id]["name"].clone()};
    }}
    Ok(())
}
