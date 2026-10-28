//! Índice somente leitura de MIFs compartilhados. A biblioteca monta um por varredura.
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
const MAX_DEPTH: usize = 8;
const MAX_FILES: usize = 4000;
#[derive(Default)]
pub struct Index {
    files: BTreeMap<String, Vec<PathBuf>>,
    local: RefCell<BTreeMap<PathBuf, Result<BTreeMap<String, Vec<PathBuf>>, String>>>,
    pub shared: bool,
    error: Option<String>,
}

fn key(name: &std::ffi::OsStr) -> String {
    name.to_string_lossy().to_lowercase()
}
fn mif(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case("mif"))
}
fn children_named(root: &Path, name: &str) -> Vec<PathBuf> {
    let mut found: Vec<_> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.file_type().is_ok_and(|t| t.is_dir())
                && e.file_name().to_string_lossy().eq_ignore_ascii_case(name)
        })
        .map(|e| e.path())
        .collect();
    found.sort();
    found
}
impl Index {
    pub fn for_root(root: &Path) -> Self {
        let roots = root
            .ancestors()
            .take(MAX_DEPTH + 2)
            .filter(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("mod"))
            })
            .filter_map(|p| p.parent())
            .map(|p| children_named(p, "mif"))
            .find(|paths| !paths.is_empty())
            .unwrap_or_else(|| children_named(root, "mif"));
        let mut index = Self {
            shared: !roots.is_empty(),
            ..Self::default()
        };
        let mut count = 0;
        for root in roots {
            index.collect(&root, 0, &mut count);
        }
        for paths in index.files.values_mut() {
            paths.sort();
        }
        index
    }
    fn collect(&mut self, root: &Path, depth: usize, count: &mut usize) {
        if depth > MAX_DEPTH || *count >= MAX_FILES {
            self.error = Some("MIF scan limit reached; select a smaller game library".into());
            return;
        }
        let entries = match std::fs::read_dir(root) {
            Ok(e) => e,
            Err(e) => {
                self.error = Some(format!("Cannot read MIF folder {}: {e}", root.display()));
                return;
            }
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            if *count >= MAX_FILES {
                self.error = Some("MIF scan limit reached; select a smaller game library".into());
                break;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                self.collect(&entry.path(), depth + 1, count);
            } else if kind.is_file() && mif(&entry.path()) {
                if let Some(stem) = entry.path().file_stem() {
                    self.files.entry(key(stem)).or_default().push(entry.path());
                    *count += 1;
                }
            }
        }
    }
    pub fn find(&self, module: &Path) -> Result<Option<PathBuf>, String> {
        let dir = module.parent().ok_or("Module has no game folder")?;
        let mut names = Vec::new();
        if let Some(name) = module.file_stem() {
            names.push(key(name));
        }
        if let Some(name) = dir.file_name() {
            let name = key(name);
            if !names.contains(&name) {
                names.push(name);
            }
        }
        if let Some(path) = self.local_match(dir, &names)? {
            return Ok(Some(path));
        }
        // Pacotes BREW também usam Título/<id>.mif e Título/<id>/outro-nome.mod.
        // Só o id da pasta vale aqui: um genérico game.mif do diretório pai é alheio.
        if let (Some(parent), Some(name)) = (dir.parent(), dir.file_name()) {
            if let Some(path) = self.local_match(parent, &[key(name)])? {
                return Ok(Some(path));
            }
        }
        // Índice parcial não comprova unicidade: outra cópia diferente pode estar além do limite.
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        for name in &names {
            if let Some(paths) = self.files.get(name) {
                return same_manifest(paths).map(Some);
            }
        }
        Ok(None)
    }
    fn local_match(&self, dir: &Path, names: &[String]) -> Result<Option<PathBuf>, String> {
        if !self.local.borrow().contains_key(dir) {
            let result = (|| {
                let mut files: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
                let entries = std::fs::read_dir(dir)
                    .map_err(|e| format!("Cannot read MIF folder {}: {e}", dir.display()))?;
                for (count, entry) in entries.enumerate() {
                    if count >= MAX_FILES {
                        return Err("MIF scan limit reached; select a smaller game library".into());
                    }
                    let entry = entry.map_err(|e| e.to_string())?;
                    if entry.file_type().map_err(|e| e.to_string())?.is_file() && mif(&entry.path())
                    {
                        if let Some(stem) = entry.path().file_stem() {
                            files.entry(key(stem)).or_default().push(entry.path());
                        }
                    }
                }
                Ok(files)
            })();
            // O pai pode ser a coleção inteira; inspecioná-lo uma vez evita trabalho quadrático.
            self.local.borrow_mut().insert(dir.to_path_buf(), result);
        }
        let cache = self.local.borrow();
        let files = cache.get(dir).unwrap().as_ref().map_err(Clone::clone)?;
        for name in names {
            if let Some(paths) = files.get(name) {
                return same_manifest(paths).map(Some);
            }
        }
        Ok(None)
    }
}
fn same_manifest(paths: &[PathBuf]) -> Result<PathBuf, String> {
    let mut paths = paths.to_vec();
    paths.sort();
    let first = paths.first().ok_or("Empty MIF match")?;
    if paths.len() > 1 {
        let read = |path: &Path| -> Result<Vec<u8>, String> {
            let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
            if size > 1024 * 1024 {
                return Err("MIF exceeds inspection limit".into());
            }
            std::fs::read(path).map_err(|e| e.to_string())
        };
        let original = read(first)?;
        for other in paths.iter().skip(1) {
            if read(other)? != original {
                return Err(format!(
                    "Several different MIF files match {}. Put the correct MIF beside the module to choose it.",
                    first.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
        }
    }
    Ok(first.clone())
}
