//! Catálogo instalado móvel: ClassID nunca é inferido do título de uma pasta.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstalledApplet {
    pub class: u32,
    pub module: PathBuf,
    pub mif: PathBuf,
    pub module_sha256: String,
    pub mif_sha256: String,
    pub module_id: String,
}
impl InstalledApplet {
    pub fn inspect(module: &Path, mif: &Path) -> Result<Self, String> {
        let package =
            crate::handset::package::read_module_pair(module, mif).map_err(|e| e.to_string())?;
        let class = package
            .mif
            .main_applet()
            .ok_or("Installed module is not an applet")?;
        if package.mif.applets.len() != 1 || !package.mif.extensao().is_empty() {
            return Err("Installed module must declare one applet".into());
        }
        let report = package.report;
        Ok(Self {
            class,
            module: report.source_module,
            mif: report.source_mif,
            module_sha256: report.module_sha256,
            mif_sha256: report.mif_sha256,
            module_id: mif
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("Invalid installed manifest name")?
                .into(),
        })
    }
    pub fn verify(&self) -> Result<Self, String> {
        let selected = crate::library::resolve_mif(&self.module)?
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if selected != self.mif.canonicalize().map_err(|e| e.to_string())? {
            return Err("Installed manifest pairing changed".into());
        }
        let actual = Self::inspect(&self.module, &self.mif)?;
        if actual.class != self.class
            || !actual
                .module_sha256
                .eq_ignore_ascii_case(&self.module_sha256)
            || !actual.mif_sha256.eq_ignore_ascii_case(&self.mif_sha256)
        {
            return Err("Installed package identity changed; refresh the library".into());
        }
        Ok(actual)
    }
}
#[derive(Clone, Debug, Default)]
pub struct InstalledCatalog {
    entries: BTreeMap<u32, Vec<InstalledApplet>>,
}
impl InstalledCatalog {
    pub fn new(entries: impl IntoIterator<Item = InstalledApplet>) -> Self {
        let mut result = Self::default();
        for mut app in entries {
            let hash = |v: &str| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit());
            if app.class == 0
                || !hash(&app.module_sha256)
                || !hash(&app.mif_sha256)
                || app.module_id.is_empty()
            {
                continue;
            }
            app.module_sha256.make_ascii_lowercase();
            app.mif_sha256.make_ascii_lowercase();
            result.entries.entry(app.class).or_default().push(app);
        }
        for entries in result.entries.values_mut() {
            entries.sort_by(|a, b| a.module.cmp(&b.module).then(a.mif.cmp(&b.mif)));
            entries.dedup();
        }
        result
    }
    pub fn with(self, app: InstalledApplet) -> Self {
        Self::new(
            self.entries
                .into_values()
                .flatten()
                .chain(std::iter::once(app)),
        )
    }
    pub fn resolve(&self, class: u32) -> Result<InstalledApplet, String> {
        let entries = self
            .entries
            .get(&class)
            .ok_or_else(|| format!("Installed applet {class:08x} not found"))?;
        if !Self::unambiguous(entries) {
            return Err(format!("Installed applet {class:08x} is ambiguous"));
        }
        let mut failure = String::from("Empty installed identity");
        for app in entries {
            match app.verify() {
                Ok(app) => return Ok(app),
                Err(error) => failure = error,
            }
        }
        Err(failure)
    }
    fn unambiguous(entries: &[InstalledApplet]) -> bool {
        entries.first().is_some_and(|first| {
            entries
                .iter()
                .all(|a| a.module_sha256 == first.module_sha256 && a.mif_sha256 == first.mif_sha256)
        })
    }
    pub fn shell_entries(&self) -> Vec<(u32, String)> {
        self.entries
            .iter()
            .filter(|(_, a)| Self::unambiguous(a))
            .filter_map(|(&class, a)| a.first().map(|a| (class, a.module_id.clone())))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("local-tests")
                .join(format!(
                    "installed-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                ));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn app(&self, name: &str, class: u32, value: u32) -> InstalledApplet {
            let dir = self.0.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            let module = dir.join("game.mod");
            let mif = dir.join("game.mif");
            std::fs::write(
                &module,
                [0xe3a00000 | value, 0xe12fff1eu32]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
            let mut bytes = vec![0u8; 60];
            bytes[..4].copy_from_slice(&[17, 0, 1, 0]);
            for (at, v) in [
                (16, 32u32),
                (20, 1),
                (32, 40),
                (36, 60),
                (40, class),
                (48, 1),
            ] {
                bytes[at..at + 4].copy_from_slice(&v.to_le_bytes());
            }
            std::fs::write(&mif, bytes).unwrap();
            InstalledApplet::inspect(&module, &mif).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let p = self.0.canonicalize().unwrap();
            assert!(
                p.starts_with(
                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("local-tests")
                        .canonicalize()
                        .unwrap()
                )
            );
            std::fs::remove_dir_all(p).unwrap();
        }
    }
    #[test]
    fn installed_catalog_resolves_real_class_and_shell_manifest() {
        let f = Fixture::new();
        let app = f.app("target", 0x01001234, 0);
        let c = InstalledCatalog::new([app.clone()]);
        assert_eq!(c.resolve(0x01001234).unwrap(), app);
        assert_eq!(c.shell_entries(), [(0x01001234, "game".into())]);
        assert!(c.resolve(0x01005678).is_err());
    }
    #[test]
    fn installed_catalog_deduplicates_identical_pairs_but_rejects_conflicting_classes() {
        let f = Fixture::new();
        let a = f.app("a", 0x01001234, 0);
        let b = f.app("b", 0x01001234, 0);
        let c = InstalledCatalog::new([b, a.clone()]);
        assert_eq!(c.resolve(a.class).unwrap(), a);
        let other = f.app("c", 0x01001234, 1);
        let c = InstalledCatalog::new([a, other]);
        assert!(c.resolve(0x01001234).is_err());
        assert!(c.shell_entries().is_empty());
    }
    #[test]
    fn installed_catalog_rechecks_target_module_and_manifest_before_launch() {
        let f = Fixture::new();
        let app = f.app("target", 0x01001234, 0);
        std::fs::write(
            &app.module,
            [0xe3a00001u32, 0xe12fff1e]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
        assert!(
            app.verify().is_err(),
            "cached identity cannot launch changed module"
        );
        let app = f.app("manifest", 0x01001234, 0);
        let mut bytes = std::fs::read(&app.mif).unwrap();
        bytes[40..44].copy_from_slice(&0x01005678u32.to_le_bytes());
        std::fs::write(&app.mif, bytes).unwrap();
        assert!(
            app.verify().is_err(),
            "cached identity cannot launch changed manifest"
        );
    }
}
