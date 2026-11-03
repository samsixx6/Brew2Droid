//! Fronteira dos aparelhos BREW, separada das decisões do console.

pub mod clock;
pub mod display;
pub mod mif_lookup;
pub mod audio;
pub mod fps;
pub mod input;
pub mod package;
pub mod launch;
mod opaque_mif;
mod install;
pub mod probe;
pub mod profile;
pub mod report;
pub mod types;
pub mod touch;

pub mod platform;
pub mod installed;
