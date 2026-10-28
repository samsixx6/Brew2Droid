//! Metadados calculados apenas na varredura; nunca fazem IO durante o desenho da lista.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const MAX_DEPTH: usize = 32;
const MAX_ENTRIES: usize = 20_000;
const MAX_SCAN_ENTRIES: usize = 100_000;

pub struct Metadata {
    pub folder: PathBuf,
    pub size: Option<u64>,
    pub title: Option<String>,
    pub version: Option<String>,
}

/// A descoberta já visitou todos os arquivos. Recalcular o total com o inventário
/// evita uma segunda travessia e permite interromper a única travessia em andamento.
pub fn from_inventory(
    root: &Path,
    module: &Path,
    manifest: Option<&Path>,
    files: &BTreeMap<PathBuf, serde_json::Value>,
) -> Metadata {
    let dir = module.parent().unwrap_or(root);
    let scope = dir.parent().filter(|p| {
        p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("mod"))
    }).and_then(Path::parent).filter(|p| {
        p.starts_with(root) && !collection_name(p) && (*p != root || organized_name(p).is_some())
    });
    let package = scope.filter(|p| organized_name(p).is_some() || manifest.is_some_and(|m|m.starts_with(p)));
    let paired = manifest.and_then(Path::parent).filter(|p| {
        Some(*p) == dir.parent() && *p != root && p.starts_with(root) && !collection_name(p)
    });
    let organized = dir.ancestors().take(2).take_while(|p|p.starts_with(root))
        .find_map(|p|organized_name(p).map(|(t,v)|(p.to_path_buf(),t,v)))
        .or_else(||package.and_then(|p| {
            let (title, version) = organized_name(p).or_else(||Some((p.file_name()?.to_str()?.into(),None)))?;
            Some((p.to_path_buf(),title,version))
        }))
        .or_else(||paired.and_then(|p|Some((p.to_path_buf(),p.file_name()?.to_str()?.into(),None))));
    let (folder, title, version, recognized) = match organized {
        Some((folder,title,version)) => (folder,Some(title),version,true),
        None => (dir.to_path_buf(),None,None,dir != root && !collection_name(dir) && scope.is_none()),
    };
    let size = recognized.then(|| {
        let mut total = 0u64;
        for (path, stamp) in files.range(folder.clone()..) {
            if !path.starts_with(&folder) { break; }
            if stamp["kind"] == "link" || stamp["kind"] == "other" { return None; }
            if stamp["kind"] == "file" { total = total.checked_add(stamp["length"].as_u64()?)?; }
        }
        if let Some(manifest) = manifest.filter(|p|!p.starts_with(&folder)) {
            total = total.checked_add(files.get(manifest)?["length"].as_u64()?)?;
        }
        Some(total)
    }).flatten();
    Metadata { folder, size, title, version }
}

/// Só seções textuais de um MIF válido participam; imagens e strings do MOD
/// não são evidência de editora. Mais de uma empresa reconhecida continua Unknown.
pub fn company(data: &[u8]) -> Option<String> {
    let mif = zeebx::loader::miffile::MifFile::parse(data).ok()?;
    let mut text = String::new();
    for (offset, length) in mif.sections {
        let bytes = data.get(offset as usize..offset.checked_add(length)? as usize)?;
        if bytes.get(2..).is_some_and(|s|s.starts_with(b"image/")) { continue; }
        text.push(' ');
        text.extend(bytes.iter().map(|b|if b.is_ascii_graphic() || *b == b' ' { *b as char } else { ' ' }));
        for shift in 0..2 {
            let words: Vec<u16> = bytes.get(shift..).unwrap_or_default().chunks_exact(2)
                .map(|b|u16::from_le_bytes([b[0],b[1]])).collect();
            text.push(' '); text.push_str(&String::from_utf16_lossy(&words));
        }
    }
    let normalized: String = text.to_lowercase().chars().map(|c|if c.is_alphanumeric() { c } else { ' ' }).collect();
    let normalized = format!(" {} ", normalized.split_whitespace().collect::<Vec<_>>().join(" "));
    let companies: &[(&str,&[&str])] = &[
        ("Gameloft", &["gameloft"]), ("Electronic Arts", &["electronic arts","ea mobile"]),
        ("Glu", &["glu mobile","glu games"]), ("JAMDAT", &["jamdat"]),
        ("Namco", &["namco","bandai namco"]), ("Digital Chocolate", &["digital chocolate"]),
        ("THQ", &["thq"]), ("Konami", &["konami"]), ("Capcom", &["capcom"]),
        ("SEGA", &["sega"]), ("Ubisoft", &["ubisoft"]), ("Disney", &["disney"]),
        ("Taito", &["taito"]), ("Hudson", &["hudson soft"]), ("Square Enix", &["square enix"]),
        ("PopCap", &["popcap"]), ("HandyGames", &["handygames","handy games"]),
        ("Sony Pictures", &["sony pictures"]), ("I-play", &["i play"]),
    ];
    let mut matches = companies.iter().filter(|(_,aliases)|aliases.iter().any(|alias|normalized.contains(&format!(" {alias} "))));
    let first = matches.next()?;
    matches.next().is_none().then(||first.0.to_string())
}

pub struct Scan<'a> {
    root: &'a Path,
    manifests: zeebx::handset::mif_lookup::Index,
    nested_manifests: BTreeMap<PathBuf, zeebx::handset::mif_lookup::Index>,
    sizes: BTreeMap<PathBuf, Option<u64>>,
    remaining: usize,
}

impl<'a> Scan<'a> {
    pub fn new(root: &'a Path) -> Self {
        Self {
            root,
            manifests: zeebx::handset::mif_lookup::Index::for_root(root),
            nested_manifests: BTreeMap::new(),
            sizes: BTreeMap::new(),
            remaining: MAX_SCAN_ENTRIES,
        }
    }

    pub fn describe(&mut self, module: &Path) -> Metadata {
        let dir = module.parent().unwrap_or(self.root);
        let mut manifest = self.manifests.find(module);
        let nested_scope = dir
            .parent()
            .filter(|parent| {
                parent
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("mod"))
            })
            .and_then(Path::parent)
            .filter(|scope| {
                scope.starts_with(self.root)
                    && !collection_name(scope)
                    && (*scope != self.root || organized_name(scope).is_some())
            });
        if let Some(scope) = nested_scope {
            // Um Mif dentro do título tem precedência sobre o Mif da coleção. Reusar
            // o índice por pacote também cobre módulos de extensão sem reler seus MIFs.
            let nested = self
                .nested_manifests
                .entry(scope.to_path_buf())
                .or_insert_with(|| zeebx::handset::mif_lookup::Index::for_root(dir));
            if nested.shared {
                let selected = nested.find(module);
                if !matches!(selected, Ok(None)) {
                    manifest = selected;
                }
            }
        }
        let nested_package = nested_scope.filter(|scope| {
            organized_name(scope).is_some()
                || manifest
                    .as_ref()
                    .ok()
                    .and_then(|path| path.as_ref())
                    .is_some_and(|path| path.starts_with(scope))
        });
        let paired_parent = manifest
            .as_ref()
            .ok()
            .and_then(|path| path.as_ref())
            .and_then(|path| path.parent())
            .filter(|parent| {
                Some(*parent) == dir.parent()
                    && *parent != self.root
                    && parent.starts_with(self.root)
                    && !collection_name(parent)
            });
        let organized = dir
            .ancestors()
            .take(2)
            .take_while(|p| p.starts_with(self.root))
            .find_map(|folder| {
                organized_name(folder)
                    .map(|(title, version)| (folder.to_path_buf(), title, version))
            })
            .or_else(|| {
                nested_package.and_then(|folder| {
                    let (title, version) = organized_name(folder)
                        .or_else(|| Some((folder.file_name()?.to_str()?.to_string(), None)))?;
                    Some((folder.to_path_buf(), title, version))
                })
            })
            .or_else(|| {
                paired_parent.and_then(|folder| {
                    let title = folder.file_name()?.to_str()?.to_string();
                    Some((folder.to_path_buf(), title, None))
                })
            });
        let (folder, title, version, recognized) = match organized {
            Some((folder, title, version)) => (folder, Some(title), version, true),
            None => (
                dir.to_path_buf(),
                None,
                None,
                dir != self.root && !collection_name(dir) && nested_scope.is_none(),
            ),
        };
        let mut size = if recognized {
            self.folder_size(&folder)
        } else {
            None
        };
        if size.is_some() {
            match manifest {
                Ok(Some(manifest)) if !manifest.starts_with(&folder) => {
                    size = size.and_then(|total| {
                        regular_size(&manifest).and_then(|mif| total.checked_add(mif))
                    });
                }
                Err(_) => size = None,
                _ => {}
            }
        }
        Metadata {
            folder,
            size,
            title,
            version,
        }
    }

    fn folder_size(&mut self, folder: &Path) -> Option<u64> {
        if let Some(size) = self.sizes.get(folder) {
            return *size;
        }
        let mut count = 0;
        let size = total(folder, 0, &mut count, &mut self.remaining);
        self.sizes.insert(folder.to_path_buf(), size);
        size
    }
}

fn collection_name(path: &Path) -> bool {
    path.file_name().is_some_and(|name| {
        ["games", "mod", "mif", "roms"]
            .iter()
            .any(|n| name.to_string_lossy().eq_ignore_ascii_case(n))
    })
}

fn regular_size(path: &Path) -> Option<u64> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    (!meta.file_type().is_symlink() && meta.is_file()).then_some(meta.len())
}

fn total(folder: &Path, depth: usize, count: &mut usize, remaining: &mut usize) -> Option<u64> {
    if depth > MAX_DEPTH
        || std::fs::symlink_metadata(folder)
            .ok()?
            .file_type()
            .is_symlink()
    {
        return None;
    }
    let mut size = 0u64;
    for entry in std::fs::read_dir(folder).ok()? {
        if *count >= MAX_ENTRIES || *remaining == 0 {
            return None;
        }
        *count += 1;
        *remaining -= 1;
        let entry = entry.ok()?;
        let kind = entry.file_type().ok()?;
        // Seguir links pode sair do pacote ou criar ciclos; um total parcial seria enganoso.
        let bytes = if kind.is_symlink() {
            return None;
        } else if kind.is_dir() {
            total(&entry.path(), depth + 1, count, remaining)?
        } else if kind.is_file() {
            entry.metadata().ok()?.len()
        } else {
            return None;
        };
        size = size.checked_add(bytes)?;
    }
    Some(size)
}

fn organized_name(folder: &Path) -> Option<(String, Option<String>)> {
    let name = folder.file_name()?.to_str()?;
    let (start, end) = name.match_indices('(').find_map(|(start, _)| {
        let close = name[start..].find(')')? + start;
        let label = &name[start + 1..close];
        if label.eq_ignore_ascii_case("unknown resolution") {
            return Some((start, close + 1));
        }
        let (width, height) = label.split_once(['x', 'X', '×'])?;
        let dimension = |s: &str| {
            !s.is_empty()
                && s.bytes().all(|b| b.is_ascii_digit())
                && s.parse::<u16>().is_ok_and(|n| n > 0 && n <= 8192)
        };
        (dimension(width) && dimension(height)).then_some((start, close + 1))
    })?;
    if name[..start].trim().is_empty() {
        return None;
    }
    let suffix = name[end..].trim();
    let (tags, candidate) = suffix
        .rsplit_once(char::is_whitespace)
        .unwrap_or(("", suffix));
    let number = candidate.strip_prefix(['v', 'V']).unwrap_or(candidate);
    let initial = number.split(['_', '-']).next().unwrap_or_default();
    let numeric_prefix = initial.split('.').take(2).collect::<Vec<_>>();
    let valid_version = numeric_prefix.len() == 2
        && numeric_prefix
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
        && number
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || [b'.', b'_', b'-'].contains(&b));
    let version = valid_version.then(|| candidate.to_string());
    let title_suffix = if valid_version { tags } else { suffix };
    let title = format!("{} {}", name[..start].trim(), title_suffix)
        .trim()
        .to_string();
    Some((title, version))
}

pub fn known_title(hash: &str) -> Option<&'static str> {
    use zeebx::handset::types::GameId;
    let (id, _, _) = zeebx::handset::profile::IDENTITIES
        .iter()
        .find(|(_, known, _)| *known == hash)?;
    // Recursos de applet dos MIFs originais confirmam esses nomes. A versão do perfil
    // é do esquema, não do jogo; um hash do módulo sozinho não determina a versão do MIF.
    Some(match id {
        GameId::Asphalt2 => "Asphalt Urban GT 2 3D",
        GameId::Bia240 => "BIA: Earned In Blood",
        GameId::Bowling3d => "JAMDAT Bowling 3D",
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn exact_profile_hashes_have_verified_applet_names_and_unknown_hashes_do_not() {
        for (hash, title) in [
            (
                "c4a114ae6391fec4b04080e2971465197b1867751b0436cb7e8891eabb1b1819",
                "Asphalt Urban GT 2 3D",
            ),
            (
                "e2cc4d4c05bb927569a26cdc87bf7f66978ddd7dce04583fe8f169089b979d6d",
                "BIA: Earned In Blood",
            ),
            (
                "1f5174a83a784d7ce67145b5d182d09ae734f78a434f2eac4bc80416503d8f60",
                "JAMDAT Bowling 3D",
            ),
        ] {
            assert_eq!(super::known_title(hash), Some(title));
        }
        assert_eq!(super::known_title("unknown"), None);
        assert_eq!(super::known_title("asphalt2"), None);
    }
}
