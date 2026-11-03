//! Metadados de abertura; um executável auditado não ganha um MIF fabricado.

use super::package::{self, PackageError};
use crate::loader::modfile::ModImage;
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

pub struct Manifest {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    pub sha256: String,
    /// Falso para bytes opacos identificados por um par auditado, sem interpretar recursos.
    pub parsed: bool,
}

pub struct LaunchPackage {
    pub image: ModImage,
    pub module_sha256: String,
    pub applet_class: u32,
    pub manifest: Option<Manifest>,
    pub(crate) installation: Option<super::install::Installation>,
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect()
}

fn audited_class(hash: &str) -> Option<u32> {
    match hash {
        // SBS Fortress: o literal em 0x10860 é comparado em 0x1082c pelo
        // AEEClsCreateInstance original. A classe cria o applet e o caminho
        // normal desenha os recursos locais; não exige dados de manifesto.
        "5a54d8e523a18705e7c62b043b9817cf7f66b7c2d9a2250bccea858f5cb9f763" => Some(0x0102_cc31),
        // SSA: AEEClsCreateInstance em 0x107b8 compara o literal 0x10834.
        // A identidade também seleciona o texto OEM empacotado, sem criar MIF.
        "74968a7d74d9a5ac19b51e00d157abb5d63fd166961b277943bc21c187d15fcf" => Some(0x0101_3be4),
        _ => None,
    }
}

fn module_bytes(path: &Path) -> Result<Vec<u8>, PackageError> {
    let file = std::fs::File::open(path)?;
    let metadata = file.metadata()?;
    const LIMIT: u64 = 16 * 1024 * 1024;
    if !metadata.is_file() || metadata.len() > LIMIT {
        return Err(PackageError::Invalid(
            "Module exceeds inspection limits".into(),
        ));
    }
    let mut bytes = Vec::new();
    file.take(LIMIT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > LIMIT {
        return Err(PackageError::Invalid(
            "Module grew beyond inspection limit".into(),
        ));
    }
    Ok(bytes)
}

/// Usado somente depois que a busca de MIF confirmou ausência, nunca em ambiguidade.
pub(crate) fn executable_class(path: &Path) -> Option<u32> {
    audited_class(&digest(&module_bytes(path).ok()?))
}

const KINGDOM_MOD: &str = "10f18a0c8e77715459c7eefbb276b8054fe7d8afdb5f69a105aa370c1193b804";
const KINGDOM_MIF: &str = "624b4d689c21b25ed9aba934a05ba0a2c907c34d171cf734188f95b7dbbfa4db";
const KINGDOM_CLASS: u32 = 0x0102_6191;

fn opaque_pair_class(module: &str, mif: &str) -> Option<u32> {
    if super::install::identifies(module, mif) {
        Some(super::install::CLASS)
    } else if module == KINGDOM_MOD && mif == KINGDOM_MIF {
        // O factory ARM original compara esta classe; o MIF continua opaco.
        Some(KINGDOM_CLASS)
    } else {
        super::opaque_mif::identity(module, mif).map(|entry| entry.class)
    }
}

pub(crate) fn opaque_metadata_class(path: &Path, bytes: &[u8]) -> Option<u32> {
    let mif = digest(bytes);
    if !super::install::metadata_matches(bytes)
        && mif != KINGDOM_MIF
        && !super::opaque_mif::knows_mif(&mif)
    {
        return None;
    }
    opaque_pair_class(&digest(&module_bytes(path).ok()?), &mif)
}

pub(crate) fn opaque_metadata_dependencies(path: &Path, bytes: &[u8]) -> Option<&'static [u32]> {
    let mif = digest(bytes);
    if mif != KINGDOM_MIF && !super::opaque_mif::knows_mif(&mif) { return None; }
    let module = digest(&module_bytes(path).ok()?);
    if module == KINGDOM_MOD && mif == KINGDOM_MIF {
        Some(&[0x0102_bbfc][..])
    } else {
        super::opaque_mif::identity(&module, &mif).map(|entry| entry.dependencies)
    }
}

pub fn read(path: &Path) -> Result<LaunchPackage, PackageError> {
    let parent = path
        .parent()
        .ok_or_else(|| PackageError::Invalid("Module has no game folder".into()))?;
    let index = super::mif_lookup::Index::for_root(parent);
    let selected = index.find(path).map_err(PackageError::Invalid)?;
    if let Some(mif) = selected {
        let package = match package::read_module_pair(path, &mif) {
            Ok(package) => package,
            Err(error) => {
                // Só pares exatos auditados oferecem os fatos de abertura sem
                // interpretar, substituir ou regravar o MIF opaco original.
                if let Ok(bytes) = package::read_limited(&mif, 1024 * 1024) {
                    if let Some(class) = opaque_metadata_class(path, &bytes) {
                        let module = module_bytes(path)?;
                        let module_sha256 = digest(&module);
                        let mif_sha256 = digest(&bytes);
                        if opaque_pair_class(&module_sha256, &mif_sha256) == Some(class) {
                            let installation =
                                super::install::identifies(&module_sha256, &mif_sha256)
                                    .then(|| super::install::Installation::for_module(path));
                            return Ok(LaunchPackage {
                                image: package::parse_module(module)?,
                                module_sha256,
                                applet_class: class,
                                manifest: Some(Manifest {
                                    path: mif,
                                    sha256: mif_sha256,
                                    bytes,
                                    parsed: false,
                                }),
                                installation,
                            });
                        }
                    }
                }
                return Err(error);
            }
        };
        let applet_class = package
            .mif
            .main_applet()
            .ok_or_else(|| PackageError::Invalid("MIF declares no applet".into()))?;
        return Ok(LaunchPackage {
            image: package.image,
            module_sha256: package.report.module_sha256,
            applet_class,
            manifest: Some(Manifest {
                path: package.report.source_mif,
                bytes: package.mif_bytes,
                sha256: package.report.mif_sha256,
                parsed: true,
            }),
            installation: None,
        });
    }
    let bytes = module_bytes(path)?;
    let module_sha256 = digest(&bytes);
    let applet_class=audited_class(&module_sha256).ok_or_else(||PackageError::Invalid(format!(
        "No matching MIF for {} or its game folder. Place it beside the module or under games/Mif.",path.display())))?;
    let image = package::parse_module(bytes)?;
    Ok(LaunchPackage {
        image,
        module_sha256,
        applet_class,
        manifest: None,
        installation: None,
    })
}
