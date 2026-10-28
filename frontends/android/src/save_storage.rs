//! Seleção dos dados persistentes do handset, feita somente antes de abrir a sessão.
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use zeebx::storage::{ContentId, StoragePaths};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub fn prepare_game(
    legacy: &StoragePaths,
    public_data: &Path,
    game: &Path,
    content: &ContentId,
) -> io::Result<StoragePaths> {
    let public_save = public_data.join("saves").join(content.as_str());
    let established = match fs::symlink_metadata(&public_save) {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => true,
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "An existing save path is not a folder",
            ));
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => false,
        Err(e) => return Err(e),
    };
    if !established {
        // Recupera escritas de versões antigas no overlay privado antes da publicação.
        // Um save público já estabelecido prevalece também sobre o cache antigo.
        legacy.migrate_legacy_package_writes(game, content)?;
    }
    prepare(legacy, public_data)
}

pub fn prepare(legacy: &StoragePaths, public_data: &Path) -> io::Result<StoragePaths> {
    ensure_directory(public_data)?;
    let probe = unique_path(public_data, ".write-check");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)?;
    let result = (|| {
        file.write_all(b"brew2droid")?;
        file.sync_all()
    })();
    drop(file);
    // Não iniciar uma sessão sem confirmar que o destino continua gravável, mesmo se já existir.
    let _ = fs::remove_file(&probe);
    result?;
    let mut storage = legacy.clone();
    storage.root = public_data.to_owned();
    storage.saves = public_data.join("saves");
    storage.device = public_data.join("shared");
    storage.metadata = public_data.join("metadata");
    storage.overlay = true;
    ensure_directory(&storage.saves)?;
    migrate_tree(&legacy.device, &storage.device)?;
    migrate_tree(&legacy.metadata, &storage.metadata)?;
    match fs::read_dir(&legacy.saves) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry?;
                migrate_tree(&entry.path(), &storage.saves.join(entry.file_name()))?;
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    storage.create_dirs()?;
    Ok(storage)
}

fn ensure_directory(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Save directory must be a real folder",
        ));
    }
    Ok(())
}

fn unique_path(parent: &Path, prefix: &str) -> PathBuf {
    parent.join(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

fn migrate_tree(source: &Path, target: &Path) -> io::Result<()> {
    match fs::symlink_metadata(target) {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => return Ok(()),
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "An existing save path is not a folder",
            ));
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    match fs::symlink_metadata(source) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
        Ok(meta) if !meta.is_dir() || meta.file_type().is_symlink() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Legacy saves must be a real folder",
            ));
        }
        Ok(_) => {}
    }
    let parent = target
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Save folder has no parent"))?;
    ensure_directory(parent)?;
    let stage = unique_path(parent, ".migration");
    fs::create_dir(&stage)?;
    let guard = StagedDirectory(stage.clone());
    copy_tree(source, &stage, 0)?;
    // Uma árvore incompleta nunca vira o save ativo. O original privado permanece como backup.
    if !target.exists() {
        fs::rename(&stage, target)?;
    }
    drop(guard);
    Ok(())
}

fn copy_tree(source: &Path, target: &Path, depth: usize) -> io::Result<()> {
    if depth > 64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Legacy save folders are nested too deeply",
        ));
    }
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let output = target.join(entry.file_name());
        if kind.is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Legacy saves contain a linked file",
            ));
        }
        if kind.is_dir() {
            fs::create_dir(&output)?;
            copy_tree(&entry.path(), &output, depth + 1)?;
        } else if kind.is_file() {
            let mut input = fs::File::open(entry.path())?;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output)?;
            io::copy(&mut input, &mut file)?;
            file.sync_all()?;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Legacy saves contain an unsupported file",
            ));
        }
    }
    Ok(())
}

struct StagedDirectory(PathBuf);
impl Drop for StagedDirectory {
    fn drop(&mut self) {
        // Só limpa a árvore recém-criada por esta migração, nunca a origem nem o destino ativo.
        if self
            .0
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with(".migration-"))
        {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
