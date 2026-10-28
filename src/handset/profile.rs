//! Um startup que retorna não confirma ISA mínima, ABI inteira ou geometria do aparelho.

use serde::Serialize;

use super::types::{DisplayGeometry, GameId, GuestAddr, GuestIsa};

pub const IDENTITIES: [(GameId, &str, &str); 3] = [
    (
        GameId::Bowling3d,
        "1f5174a83a784d7ce67145b5d182d09ae734f78a434f2eac4bc80416503d8f60",
        "dee71ffa0af583fd931612a98f56fd68b0a22bc543c65db90fed575cf0048585",
    ),
    (
        GameId::Asphalt2,
        "c4a114ae6391fec4b04080e2971465197b1867751b0436cb7e8891eabb1b1819",
        "9c8584cabca8fbb442b63c6a09ede9739c9c26013f7d9fcd6b73fe8b405c6647",
    ),
    (
        GameId::Bia240,
        "e2cc4d4c05bb927569a26cdc87bf7f66978ddd7dce04583fe8f169089b979d6d",
        "38c43123ac16c16a4462b8c4819ea2fa9fa4284b5d752776290852f105bc72f1",
    ),
];

#[derive(Debug, Clone, Serialize)]
pub struct GameProfile {
    pub game_id: GameId,
    pub version: u32,
    pub module_sha256: String,
    pub mif_sha256: String,
    pub geometry: Option<DisplayGeometry>,
    pub minimum_isa: Option<GuestIsa>,
    pub bss_bytes: Option<u32>,
    pub calling_abi_verified: bool,
    pub memory_layout_verified: bool,
    pub startup_observation: StartupObservation,
}

#[derive(Debug, Clone, Serialize)]
pub struct StartupObservation {
    pub host_arch: &'static str,
    pub module_base: GuestAddr,
    pub entry_arguments: [&'static str; 3],
    pub imodule_create_slot: u32,
    pub iapplet_handle_event_slot: u32,
    pub helper_malloc_slot: u32,
    pub module_load_result: u32,
    pub create_applet_result: u32,
    pub app_start_result: u32,
    pub evidence: &'static str,
}

#[derive(Debug)]
pub struct ProfileError(pub String);

impl std::fmt::Display for ProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ProfileError {}

pub fn resolve_profile(id: GameId, module_sha256: &str) -> Result<GameProfile, ProfileError> {
    let (_, expected, mif) = IDENTITIES
        .iter()
        .find(|(game, hash, _)| *game == id && *hash == module_sha256)
        .ok_or_else(|| ProfileError("Unknown module identity for this handset game".into()))?;
    Ok(GameProfile {
        game_id: id,
        version: 1,
        module_sha256: (*expected).into(),
        mif_sha256: (*mif).into(),
        geometry: None,
        minimum_isa: None,
        bss_bytes: None,
        calling_abi_verified: false,
        memory_layout_verified: false,
        startup_observation: StartupObservation {
            host_arch: "x86_64",
            module_base: GuestAddr(0x10000),
            entry_arguments: ["r0=IShell*", "r1=AEEHelperFuncs*", "r2=IModule**"],
            imodule_create_slot: 2,
            iapplet_handle_event_slot: 2,
            helper_malloc_slot: 26,
            module_load_result: 0,
            create_applet_result: 0,
            app_start_result: 1,
            evidence: "docs/handset/foundation.md",
        },
    })
}

pub fn validate_mif_identity(profile: &GameProfile, mif_sha256: &str) -> Result<(), ProfileError> {
    if profile.mif_sha256 != mif_sha256 {
        return Err(ProfileError(
            "MIF does not match the identified mobile module".into(),
        ));
    }
    Ok(())
}

pub fn validate_release_profile(profile: &GameProfile) -> Result<(), ProfileError> {
    // Também confere a identidade: GameProfile é público para relatórios e não é um token de confiança.
    let expected = resolve_profile(profile.game_id, &profile.module_sha256)?;
    validate_mif_identity(&expected, &profile.mif_sha256)?;
    let mut missing = Vec::new();
    if profile.geometry.is_none() {
        missing.push("geometry");
    }
    if profile.minimum_isa.is_none() {
        missing.push("minimum ISA");
    }
    if profile.bss_bytes.is_none() {
        missing.push("BSS size");
    }
    if !profile.calling_abi_verified {
        missing.push("calling ABI");
    }
    if !profile.memory_layout_verified {
        missing.push("memory layout");
    }
    if !missing.is_empty() {
        return Err(ProfileError(format!(
            "Unverified release fields: {}",
            missing.join(", ")
        )));
    }
    Err(ProfileError(
        "Mobile release validation is not established by startup observations".into(),
    ))
}
