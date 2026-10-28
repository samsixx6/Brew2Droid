//! Inspeção somente leitura. Nenhum parser daqui cria CPU, mapeia memória ou escreve saves.

use crate::loader::{miffile::MifFile, modfile::ModImage};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

use super::profile::{
    IDENTITIES, resolve_profile, validate_mif_identity, validate_release_profile,
};
use super::report::{PackageReport, ResourceReport};

#[derive(Debug)]
pub enum ParseError {
    Module(crate::loader::modfile::ParseError),
    UnsupportedEntry,
    InvalidEntry,
    InvalidMif(&'static str),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Module(error) => write!(f, "Module: {error}"),
            Self::UnsupportedEntry => {
                f.write_str("Unverified module entry format; ARM entry required")
            }
            Self::InvalidEntry => f.write_str("Module entry is outside the image or misaligned"),
            Self::InvalidMif(reason) => write!(f, "Invalid MIF: {reason}"),
        }
    }
}

impl std::error::Error for ParseError {}

pub fn parse_module(bytes: Vec<u8>) -> Result<ModImage, ParseError> {
    if bytes.len() < 8 {
        return ModImage::parse(bytes).map_err(ParseError::Module);
    }
    if bytes.len() >= 4 && bytes[3] >> 4 != 0xe {
        return Err(ParseError::UnsupportedEntry);
    }
    let module = ModImage::parse(bytes).map_err(ParseError::Module)?;
    let entry = module.entry() as usize;
    if entry % 4 != 0
        || entry
            .checked_add(4)
            .is_none_or(|end| end > module.image().len())
    {
        return Err(ParseError::InvalidEntry);
    }
    if let Some(header) = module.header() {
        if header.version != 1 || header.code_offset != 0x40 || entry < 0x40 {
            return Err(ParseError::InvalidEntry);
        }
    }
    Ok(module)
}

pub fn parse_mif(data: &[u8]) -> Result<MifFile, ParseError> {
    if data.len() < 32 || data[..4] != [0x11, 0, 1, 0] {
        return Err(ParseError::InvalidMif(
            "requires a complete version-1 header",
        ));
    }
    let u32_at = |at| u32::from_le_bytes(data[at..at + 4].try_into().unwrap()) as u64;
    let offset = u32_at(16);
    let count = u32_at(20);
    let end = offset + (count + 1) * 4;
    // u64 comporta toda soma de campos u32. Só converte índices depois de provar o limite.
    if offset < 32 || end > data.len() as u64 {
        return Err(ParseError::InvalidMif("section table is outside the file"));
    }
    let mut previous = end;
    for index in 0..=count {
        let bound = u32_at((offset + index * 4) as usize);
        if bound < previous || bound > data.len() as u64 {
            return Err(ParseError::InvalidMif("section bounds are invalid"));
        }
        previous = bound;
    }
    MifFile::parse(data).map_err(|_| ParseError::InvalidMif("unsupported section contents"))
}

#[derive(Debug)]
pub enum PackageError {
    Io(std::io::Error),
    Parse(ParseError),
    Invalid(String),
}

impl std::fmt::Display for PackageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "Package I/O: {error}"),
            Self::Parse(error) => error.fmt(f),
            Self::Invalid(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for PackageError {}

impl From<std::io::Error> for PackageError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<ParseError> for PackageError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>, PackageError> {
    let file = std::fs::File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(PackageError::Invalid(format!(
            "File exceeds inspection limits: {}",
            path.display()
        )));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(PackageError::Invalid(
            "File grew beyond inspection limit".into(),
        ));
    }
    Ok(bytes)
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn collect_resources(
    root: &Path,
    directory: &Path,
    module: &Path,
    depth: usize,
    remaining: &mut u64,
    resources: &mut Vec<ResourceReport>,
) -> Result<(), PackageError> {
    if depth > 16 {
        return Err(PackageError::Invalid(
            "Package directory nesting exceeds inspection limit".into(),
        ));
    }
    let mut entries = std::fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let kind = entry.file_type()?;
        let path = entry.path();
        let actual = path.canonicalize()?;
        if kind.is_symlink() || !actual.starts_with(root) {
            return Err(PackageError::Invalid(
                "Linked package content is not supported".into(),
            ));
        }
        if kind.is_dir() {
            collect_resources(root, &path, module, depth + 1, remaining, resources)?;
        } else if kind.is_file() && actual != module {
            if resources.len() >= 1024 {
                return Err(PackageError::Invalid(
                    "Too many package resources for inspection".into(),
                ));
            }
            let bytes = read_limited(&path, (*remaining).min(32 * 1024 * 1024))?;
            *remaining -= bytes.len() as u64;
            resources.push(ResourceReport {
                path: path
                    .strip_prefix(root)
                    .map_err(|_| PackageError::Invalid("Resource escapes package".into()))?
                    .to_string_lossy()
                    .replace('\\', "/"),
                size_bytes: bytes.len() as u64,
                sha256: sha256(&bytes),
            });
        }
    }
    Ok(())
}

pub struct InspectedPackage {
    pub report: PackageReport,
    pub image: ModImage,
    pub mif: MifFile,
}

/// A abertura precisa só do módulo e manifesto. Arquivos vizinhos podem ser outros jogos.
pub fn read_module_pair(source: &Path, mif: &Path) -> Result<InspectedPackage, PackageError> {
    let source: PathBuf = source.canonicalize()?;
    let mif = mif.canonicalize()?;
    let bytes = read_limited(&source, 16 * 1024 * 1024)?;
    let mif_bytes = read_limited(&mif, 1024 * 1024)?;
    let module_sha256 = sha256(&bytes);
    let mif_sha256 = sha256(&mif_bytes);
    let image = parse_module(bytes)?;
    let parsed_mif = parse_mif(&mif_bytes)?;
    let game_id = IDENTITIES
        .iter()
        .find(|(_, hash, _)| *hash == module_sha256)
        .map(|(id, _, _)| *id);
    let profile = game_id
        .map(|id| resolve_profile(id, &module_sha256))
        .transpose()
        .map_err(|error| PackageError::Invalid(error.to_string()))?;
    if let Some(profile) = &profile {
        validate_mif_identity(profile, &mif_sha256)
            .map_err(|error| PackageError::Invalid(error.to_string()))?;
    }
    let report = PackageReport {
        schema_version: 1,
        image_bytes: image.image().len(),
        entry_offset: image.entry(),
        module_format: format!("{:?}", image.variant()),
        applet_clsid: parsed_mif.main_applet(),
        declared_classes: parsed_mif.classes.clone(),
        source_module: source,
        source_mif: mif,
        module_sha256,
        mif_sha256,
        game_id,
        release_profile_error: profile
            .as_ref()
            .and_then(|profile| validate_release_profile(profile).err())
            .map(|error| error.to_string()),
        profile,
        resources: Vec::new(),
    };
    Ok(InspectedPackage {
        report,
        image,
        mif: parsed_mif,
    })
}

/// A bancada de inventário ainda inclui hashes dos recursos; o launcher não usa essa etapa.
pub fn read_package(source: &Path, mif: &Path) -> Result<InspectedPackage, PackageError> {
    let mut package=read_module_pair(source,mif)?;
    let source=&package.report.source_module;
    let root=source.parent().ok_or_else(||PackageError::Invalid("Module has no package directory".into()))?;
    let mut resources=Vec::new();
    collect_resources(root,root,source,0,&mut(128*1024*1024),&mut resources)?;
    package.report.resources=resources;
    Ok(package)
}

pub fn inspect_package(source: &Path, mif: &Path) -> Result<PackageReport, PackageError> {
    read_package(source, mif).map(|package| package.report)
}
