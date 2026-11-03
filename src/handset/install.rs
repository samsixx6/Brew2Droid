//! Dados de instalação verificados, materializados no overlay antes do boot.
use super::package::PackageError;
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const MOD: &str = "e6463e45a92ec908b1ea3a85f5e6c1bcd732500789e21a3a88acbb9f71fe50ba";
const MIF: &str = "4af850e861b1b288f0c13e737135ae9d0f897eb6b5edbf6dd1b54a73c5b178cb";
const COMPRESSED: &str = "00ed92476dbdbc3898a11627424dea36fe3d2452ee5db555fc66f5f48796b548";
const EXPANDED: &str = "993c2ae86b433161884e2b545675e5531b90b64989b1cd8faf6854792f7089da";
const EXPANDED_SIZE: usize = 998472;
const NAME: &str = "mgs.s3e";
pub(crate) const CLASS: u32 = 0x0106cbd5;
static NEXT: AtomicU64 = AtomicU64::new(0);

fn hash(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub(crate) fn identifies(module: &str, mif: &str) -> bool {
    module == MOD && mif == MIF
}
pub(crate) fn metadata_matches(bytes: &[u8]) -> bool {
    bytes.len() == 7709 && hash(bytes) == MIF
}
pub(crate) struct Installation {
    source: PathBuf,
}
impl Installation {
    pub(crate) fn for_module(module: &Path) -> Self {
        Self {
            source: module.with_file_name(NAME),
        }
    }
    pub(crate) fn prepare(&self, overlay: &Path) -> Result<(), PackageError> {
        let file = std::fs::File::open(&self.source)?;
        if !file.metadata()?.is_file() || file.metadata()?.len() > 2 * 1024 * 1024 {
            return Err(invalid());
        }
        let mut source = Vec::new();
        file.take(2 * 1024 * 1024 + 1).read_to_end(&mut source)?;
        let expanded = expand(source, COMPRESSED, EXPANDED, EXPANDED_SIZE)?;
        install(overlay, &expanded, COMPRESSED)
    }
}
fn invalid() -> PackageError {
    PackageError::Invalid("The game executable is damaged or from an unsupported package".into())
}
fn expand(
    source: Vec<u8>,
    compressed: &str,
    expanded: &str,
    size: usize,
) -> Result<Vec<u8>, PackageError> {
    let digest = hash(&source);
    let data = if digest == expanded {
        source
    } else if digest == compressed {
        let mut data = Vec::new();
        GzDecoder::new(source.as_slice())
            .take(size as u64 + 1)
            .read_to_end(&mut data)?;
        data
    } else {
        return Err(invalid());
    };
    // O loader original recusa gzip, mas aceita este mesmo U3EX descompactado.
    // CRC do gzip, comprimento e hash do resultado são conferidos; não se cria código.
    if data.len() != size || !data.starts_with(b"XE3U") || hash(&data) != expanded {
        return Err(invalid());
    }
    Ok(data)
}
fn read_overlay(path: &Path) -> Result<Option<Vec<u8>>, PackageError> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
        Ok(meta) => {
            if meta.file_type().is_symlink() || !meta.is_file() {
                return Err(PackageError::Invalid(
                    "Game installation destination is not a regular file".into(),
                ));
            }
            // Dados alterados pelo usuário não precisam ser carregados sem limite.
            if meta.len() > 2 * 1024 * 1024 {
                return Ok(Some(Vec::new()));
            }
            Ok(Some(std::fs::read(path)?))
        }
    }
}
fn install(overlay: &Path, data: &[u8], compressed: &str) -> Result<(), PackageError> {
    std::fs::create_dir_all(overlay)?;
    let destination = overlay.join(NAME);
    let previous = read_overlay(&destination)?;
    if let Some(bytes) = &previous {
        if bytes == data || hash(bytes) != compressed {
            return Ok(());
        }
    }
    let temporary = overlay.join(format!(
        ".brew-install-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> Result<(), PackageError> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(data)?;
        file.sync_all()?;
        drop(file);
        // Uma outra instalação idêntica é inofensiva; uma edição concorrente pertence ao usuário.
        if read_overlay(&destination)? != previous {
            return Ok(());
        }
        std::fs::rename(&temporary, &destination)?;
        Ok(())
    })();
    if temporary.exists() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::GzEncoder};
    fn packed(data: &[u8]) -> Vec<u8> {
        let mut gz = GzEncoder::new(Vec::new(), Compression::default());
        gz.write_all(data).unwrap();
        gz.finish().unwrap()
    }
    #[test]
    fn gzip_installation_checks_crc_length_magic_and_both_hashes() {
        let data = b"XE3U verified synthetic executable".to_vec();
        let source = packed(&data);
        assert_eq!(
            expand(source.clone(), &hash(&source), &hash(&data), data.len()).unwrap(),
            data
        );
        assert_eq!(
            expand(data.clone(), "unused", &hash(&data), data.len()).unwrap(),
            data
        );
        assert!(expand(source.clone(), "wrong", &hash(&data), data.len()).is_err());
        assert!(expand(source.clone(), &hash(&source), "wrong", data.len()).is_err());
        assert!(expand(source.clone(), &hash(&source), &hash(&data), data.len() - 1).is_err());
        let mut bad = source.clone();
        let n = bad.len();
        bad[n - 8] ^= 1;
        assert!(expand(bad.clone(), &hash(&bad), &hash(&data), data.len()).is_err());
        let wrong = b"ABCD valid compression is insufficient".to_vec();
        let gz = packed(&wrong);
        assert!(expand(gz.clone(), &hash(&gz), &hash(&wrong), wrong.len()).is_err());
    }
    #[test]
    fn installation_upgrades_only_the_known_seed_and_preserves_user_edits() {
        let root = std::env::temp_dir().join(format!(
            "brew-install-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(NAME);
        let data = b"XE3U expanded";
        let seed = b"compressed original";
        install(&root, data, &hash(seed)).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), data);
        std::fs::write(&path, seed).unwrap();
        install(&root, data, &hash(seed)).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), data);
        std::fs::write(&path, b"user edit").unwrap();
        install(&root, data, &hash(seed)).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"user edit");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
