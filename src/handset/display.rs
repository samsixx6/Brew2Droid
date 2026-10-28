//! Seleção de tela por jogo; executada ao descobrir/abrir, nunca por quadro.
use super::types::DisplayGeometry;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::UNIX_EPOCH;
static EDIT: Mutex<()> = Mutex::new(());
static NEXT: AtomicU64 = AtomicU64::new(0);
const MAX_MODULE: u64 = 16 * 1024 * 1024;
// Este stub só descompacta z.bar. As três edições de Asphalt 3 têm o mesmo .mod;
// os hashes abaixo são do código empacotado, conferido em execução a 240x320/176x220.
const PACKED_LOADER: &str = "94e7329d73313e7af041dd302b7b7edfdd6d5ea9b54c6881f6bbbeddbb1456e1";
const PACKED_SCREENS: [(&str,u16,u16);3] = [
    ("c0acf91ae952f8be6d938ec1204a4ca50a430966d3f273a01e13c21b783bb0db",240,320),
    ("685a2b45fc709eec565350926e95677bd8a3bfb9404e37d03eb2d89a8c2b41bc",176,220),
    ("6a17ee50bbc11d0edcbf794a0d926b51140f152e8c613df81feef493f2c24f7f",176,220),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Manual,
    Known,
    Name,
    Fallback,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Automatic {
    pub geometry: DisplayGeometry,
    pub source: Source,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    pub geometry: DisplayGeometry,
    pub source: Source,
    pub automatic: Automatic,
    pub module_sha256: String,
}

pub fn automatic(hash: &str, module: &Path) -> Automatic {
    let payload = packed_payload(hash,module).and_then(|path| {
        let mut bytes=Vec::new();
        std::fs::File::open(path).ok()?.take(MAX_MODULE+1).read_to_end(&mut bytes).ok()?;
        (bytes.len() as u64<=MAX_MODULE).then(||digest(&bytes))
    });
    automatic_identity(payload.as_deref().unwrap_or(hash),module)
}

fn packed_payload(hash: &str,module: &Path) -> Option<std::path::PathBuf> {
    (hash==PACKED_LOADER).then(||module.with_file_name("z.bar"))
}

fn automatic_identity(hash: &str,module: &Path) -> Automatic {
    if super::platform::recognize(hash,module) {
        return Automatic {geometry:DisplayGeometry{width:640,height:480,depth:16},source:Source::Known};
    }
    if let Some(&(_,width,height))=PACKED_SCREENS.iter().find(|&&(known,_,_)|known==hash) {
        return Automatic { geometry:DisplayGeometry{width,height,depth:16},source:Source::Known };
    }
    if let Some((id, _, _)) = super::profile::IDENTITIES
        .iter()
        .find(|(_, known, _)| *known == hash)
    {
        let (width, height) = match id {
            super::types::GameId::Bowling3d => (176, 220),
            _ => (240, 320),
        };
        return Automatic {
            geometry: DisplayGeometry {
                width,
                height,
                depth: 16,
            },
            source: Source::Known,
        };
    }
    let mut sizes = std::collections::BTreeSet::new();
    if let Some(stem) = module.file_stem() {
        size_labels(&stem.to_string_lossy(), &mut sizes);
    }
    if let Some(dir) = module.parent() {
        // Coleções BREW usam Variante-176x220/<id>/jogo.mod: o id esconde a pista
        // se só olharmos a pasta imediata. Não herdar dimensões de fora da coleção.
        let collection = dir.ancestors().take(9).any(|ancestor| {
            ancestor.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.eq_ignore_ascii_case("mod") || name.eq_ignore_ascii_case("games")
            })
        });
        // Sem uma raiz reconhecível, conservar a pista imediata evita herdar o nome
        // de um diretório externo à pasta escolhida (ex.: 800x480/MinhaBiblioteca).
        for (depth, ancestor) in dir
            .ancestors()
            .take(if collection { 9 } else { 1 })
            .enumerate()
        {
            let Some(name) = ancestor.file_name() else {
                break;
            };
            let name = name.to_string_lossy();
            if name.eq_ignore_ascii_case("games") {
                break;
            }
            if name.eq_ignore_ascii_case("mod") {
                // O título pode guardar Mod/jogo.mod ou Mod/<id>/jogo.mod. Ler
                // apenas o pai do Mod evita herdar a resolução de outra coleção.
                if depth <= 1 {
                    if let Some(name) = ancestor.parent().and_then(Path::file_name) {
                        size_labels(&name.to_string_lossy(), &mut sizes);
                    }
                }
                break;
            }
            size_labels(&name, &mut sizes);
        }
    }
    if sizes.len() == 1 {
        let (width, height) = sizes.into_iter().next().unwrap();
        Automatic {
            geometry: DisplayGeometry {
                width,
                height,
                depth: 16,
            },
            source: Source::Name,
        }
    } else {
        Automatic {
            geometry: DisplayGeometry {
                width: 240,
                height: 320,
                depth: 16,
            },
            source: Source::Fallback,
        }
    }
}

fn size_labels(text: &str, sizes: &mut std::collections::BTreeSet<(u16, u16)>) {
    let bytes = text.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if !bytes[at].is_ascii_digit() {
            at += 1;
            continue;
        }
        let start = at;
        while at < bytes.len() && bytes[at].is_ascii_digit() {
            at += 1;
        }
        if !matches!(bytes.get(at), Some(b'x' | b'X')) {
            continue;
        }
        let middle = at;
        at += 1;
        let height_start = at;
        while at < bytes.len() && bytes[at].is_ascii_digit() {
            at += 1;
        }
        if height_start == at {
            continue;
        }
        if let (Ok(w), Ok(h)) = (
            text[start..middle].parse::<u16>(),
            text[height_start..at].parse::<u16>(),
        ) {
            // "4x4" costuma fazer parte do título; dimensões de ícones/texturas não são prova de tela.
            if (64..=1024).contains(&w) && (64..=1024).contains(&h) {
                sizes.insert((w, h));
            }
        }
    }
}
fn valid(g: DisplayGeometry) -> bool {
    (1..=1024).contains(&g.width) && (1..=1024).contains(&g.height) && g.depth == 16
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Stamp {
    length: u64,
    seconds: u64,
    nanos: u32,
}
#[derive(Serialize, Deserialize)]
struct Cache {
    schema: u8,
    stamp: Stamp,
    sha256: String,
}
fn stamp(path: &Path) -> Result<Option<Stamp>, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() > MAX_MODULE {
        return Err("Module exceeds display inspection limits".into());
    }
    Ok(meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|t| Stamp {
            length: meta.len(),
            seconds: t.as_secs(),
            nanos: t.subsec_nanos(),
        }))
}
fn read_small(path: &Path) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(16 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 16 * 1024 {
        return Err("Display preference exceeds size limit".into());
    }
    Ok(bytes)
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let parent = path.parent().ok_or("Display preference has no parent")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temp = parent.join(format!(
        ".display-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> std::io::Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&temp, path)
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_file(temp);
        return Err(error.to_string());
    }
    Ok(())
}
fn hash_index(module: &Path, profiles: &Path) -> std::path::PathBuf {
    profiles.join("index").join(format!(
        "{}.json",
        digest(module.as_os_str().as_encoded_bytes())
    ))
}

/// O Rescan explícito do frontend precisa reler o código mesmo se a data foi
/// preservada na cópia. Só assinaturas descartáveis saem; overrides continuam.
pub fn invalidate_cache(module: &Path, profiles: &Path) -> Result<(), String> {
    for caminho in [module.to_path_buf(), module.with_file_name("z.bar")] {
        match std::fs::remove_file(hash_index(&caminho, profiles)) {
            Ok(()) => {},
            Err(erro) if erro.kind() == std::io::ErrorKind::NotFound => {},
            Err(erro) => return Err(format!("Cannot invalidate display cache: {erro}")),
        }
    }
    Ok(())
}

fn hash_for(module: &Path, profiles: &Path) -> Result<String, String> {
    let before = stamp(module)?;
    let index = hash_index(module, profiles);
    if let Some(stamp) = &before {
        if let Ok(cache) = read_small(&index)
            .and_then(|b| serde_json::from_slice::<Cache>(&b).map_err(|e| e.to_string()))
        {
            if cache.schema == 1 && cache.stamp == *stamp && valid_hash(&cache.sha256) {
                return Ok(cache.sha256);
            }
        }
    }
    let mut bytes = Vec::new();
    std::fs::File::open(module)
        .map_err(|e| e.to_string())?
        .take(MAX_MODULE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_MODULE {
        return Err("Module grew beyond display inspection limit".into());
    }
    let after = stamp(module)?;
    if before != after {
        return Err("Game changed during display inspection; scan again".into());
    }
    let hash = digest(&bytes);
    if let Some(stamp) = after {
        let cache = Cache {
            schema: 1,
            stamp,
            sha256: hash.clone(),
        };
        // O índice é descartável; falhar ao gravá-lo só exige recalcular o hash na próxima vez.
        let _ = atomic_write(
            &index,
            &serde_json::to_vec(&cache).map_err(|e| e.to_string())?,
        );
    }
    Ok(hash)
}
fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// A abertura já calculou SHA-256 ao validar o pacote; não relê o módulo para escolher a tela.
pub fn resolve_hash(module: &Path, profiles: &Path, hash: &str) -> Result<Selection, String> {
    if !valid_hash(hash) {
        return Err("Invalid display module identity".into());
    }
    let identity = identity_for(module,profiles,hash)?;
    let automatic = automatic_identity(&identity, module);
    let path = profiles.join("overrides").join(format!("{identity}.json"));
    let manual = match std::fs::metadata(&path) {
        Ok(_) => {
            let g: DisplayGeometry = serde_json::from_slice(&read_small(&path)?)
                .map_err(|e| format!("Invalid saved game resolution: {e}"))?;
            if !valid(g) {
                return Err("Saved game resolution must be 1–1024 pixels per side, RGB565".into());
            }
            Some(g)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.to_string()),
    };
    Ok(Selection {
        geometry: manual.unwrap_or(automatic.geometry),
        source: if manual.is_some() {
            Source::Manual
        } else {
            automatic.source
        },
        automatic,
        module_sha256: hash.into(),
    })
}
pub fn resolve(module: &Path, profiles: &Path) -> Result<Selection, String> {
    let hash = hash_for(module, profiles)?;
    resolve_hash(module, profiles, &hash)
}

fn identity_for(module: &Path,profiles: &Path,hash: &str) -> Result<String,String> {
    match packed_payload(hash,module) {
        // Reaproveitar o cache por caminho/tamanho/data evita reler o arquivo na biblioteca.
        // Preferências do stub eram ambíguas entre jogos; cada payload ganha sua própria chave.
        Some(payload)=>hash_for(&payload,profiles),
        None=>Ok(hash.into()),
    }
}

/// O índice da biblioteca observa o override do payload, que pode ter identidade
/// diferente do stub compartilhado. Chamar só ao processar uma linha alterada.
pub fn override_identity(module: &Path, profiles: &Path, hash: &str) -> Result<String, String> {
    if !valid_hash(hash) { return Err("Invalid display module identity".into()); }
    identity_for(module, profiles, hash)
}
pub fn set_override(
    module: &Path,
    profiles: &Path,
    geometry: Option<DisplayGeometry>,
) -> Result<Selection, String> {
    if geometry.is_some_and(|g| !valid(g)) {
        return Err("Resolution must be 1–1024 pixels per side, RGB565".into());
    }
    let _edit = EDIT.lock().map_err(|_| "Display preferences unavailable")?;
    let hash = hash_for(module, profiles)?;
    let identity = identity_for(module,profiles,&hash)?;
    let path = profiles.join("overrides").join(format!("{identity}.json"));
    if let Some(g) = geometry {
        atomic_write(&path, &serde_json::to_vec(&g).map_err(|e| e.to_string())?)?;
    } else {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    resolve_hash(module, profiles, &hash)
}
