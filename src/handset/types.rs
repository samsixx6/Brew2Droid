//! Identidades e valores guest não carregam ponteiros do host.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct GuestAddr(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum GameId {
    #[serde(rename = "bowling3d")]
    Bowling3d,
    #[serde(rename = "asphalt2")]
    Asphalt2,
    #[serde(rename = "bia240")]
    Bia240,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum GuestIsa {
    ArmV4T,
    ArmV5TE,
    ArmV6K,
    ArmV6T2,
    ArmV7A,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayGeometry {
    pub width: u16,
    pub height: u16,
    pub depth: u16,
}
