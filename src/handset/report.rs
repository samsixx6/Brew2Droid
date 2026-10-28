//! Relatórios de inspeção descrevem evidência, não promovem startup a gameplay.

use serde::Serialize;
use std::path::PathBuf;

use super::{profile::GameProfile, types::GameId};

#[derive(Debug, Serialize)]
pub struct ResourceReport {
    pub path: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Serialize)]
pub struct PackageReport {
    pub schema_version: u32,
    pub source_module: PathBuf,
    pub source_mif: PathBuf,
    pub module_sha256: String,
    pub mif_sha256: String,
    pub image_bytes: usize,
    pub entry_offset: u32,
    pub module_format: String,
    pub applet_clsid: Option<u32>,
    pub declared_classes: Vec<u32>,
    pub game_id: Option<GameId>,
    pub profile: Option<GameProfile>,
    pub release_profile_error: Option<String>,
    pub resources: Vec<ResourceReport>,
}
