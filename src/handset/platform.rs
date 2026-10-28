//! Identificação conservadora: o mesmo BREW header também existe fora do console.
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path};
pub const RALLY_MODULE: &str = "12252627d50bbd0646cdf1fb8a8a87b271714578b8e0e58a62e0fe0c719b94d7";
const RALLY_ZIP: &str = "bd5ac2bb784d56710bc42ebfc16b6cc9cec187a6d3ed5bdb2a9bcb910168ce8a";
pub fn known(hash: &str) -> bool {
    matches!(hash, RALLY_MODULE | RALLY_ZIP)
}
pub fn recognize(hash: &str, path: &Path) -> bool {
    if known(hash) {
        return true;
    }
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
    {
        return false;
    }
    // Reempacotar/renomear não altera a identidade do módulo. Nunca extrair para detectar.
    let inspect = (|| -> Option<bool> {
        let name = crate::loader::archive::find_module(path)?;
        let mut zip = zip::ZipArchive::new(std::fs::File::open(path).ok()?).ok()?;
        let item = zip.by_name(&name).ok()?;
        if item.size() > 16 * 1024 * 1024 {
            return None;
        }
        let mut bytes = Vec::new();
        item.take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() > 16 * 1024 * 1024 {
            return None;
        }
        let digest: String = Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        Some(known(&digest))
    })();
    inspect.unwrap_or(false)
}
