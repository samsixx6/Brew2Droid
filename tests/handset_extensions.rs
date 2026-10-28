use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use zeebx::library;
use zeebx::loader::miffile::MifFile;

const MICRO3D: u32 = 0x0102_92c3;
const OTHER_ENGINE: u32 = 0x0102_bbfc;
const GAME: u32 = 0x0108_1970;

struct Collection(PathBuf);

impl Collection {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .canonicalize()
            .unwrap();
        let root = workspace.join("local-tests").join(format!(
            "extensions-{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        assert!(root.canonicalize().unwrap().starts_with(workspace));
        Self(root)
    }

    fn module(&self, relative: &str, applet: Option<u32>, classes: &[u32]) -> PathBuf {
        let module = self.0.join(relative);
        std::fs::create_dir_all(module.parent().unwrap()).unwrap();
        // Código sintético válido para a sessão também poder carregar a extensão.
        std::fs::write(
            &module,
            [0xe3a0_0000u32.to_le_bytes(), 0xe12f_ff1eu32.to_le_bytes()].concat(),
        )
        .unwrap();
        let directory = module.parent().unwrap();
        let manifest = directory
            .parent()
            .unwrap()
            .join(directory.file_name().unwrap())
            .with_extension("mif");
        std::fs::write(manifest, mif(applet, classes)).unwrap();
        module
    }
}

impl Drop for Collection {
    fn drop(&mut self) {
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .canonicalize()
            .unwrap();
        let root = self.0.canonicalize().unwrap();
        assert!(root.starts_with(workspace.join("local-tests")));
        std::fs::remove_dir_all(root).unwrap();
    }
}

fn mif(applet: Option<u32>, classes: &[u32]) -> Vec<u8> {
    let mut sections = Vec::new();
    if let Some(applet) = applet {
        let mut record = vec![0u8; 20];
        record[..4].copy_from_slice(&applet.to_le_bytes());
        record[8..12].copy_from_slice(&1u32.to_le_bytes());
        sections.push(record);
    }
    for class in classes {
        let mut record = vec![0u8; 8];
        record[..4].copy_from_slice(&class.to_le_bytes());
        sections.push(record);
    }
    let mut data = vec![0u8; 0x20 + 4 * (sections.len() + 1)];
    data[..2].copy_from_slice(&0x11u16.to_le_bytes());
    data[2..4].copy_from_slice(&1u16.to_le_bytes());
    data[4..6].copy_from_slice(&1u16.to_le_bytes());
    data[0x10..0x14].copy_from_slice(&0x20u32.to_le_bytes());
    data[0x14..0x18].copy_from_slice(&(sections.len() as u32).to_le_bytes());
    for (index, section) in sections.iter().enumerate() {
        let offset = data.len() as u32;
        data[0x20 + index * 4..0x24 + index * 4].copy_from_slice(&offset.to_le_bytes());
        data.extend_from_slice(section);
    }
    let end = data.len() as u32;
    data[0x20 + sections.len() * 4..0x24 + sections.len() * 4].copy_from_slice(&end.to_le_bytes());
    // Os arquivos são fixtures do formato, e não dependem de ROMs locais.
    let parsed = MifFile::parse(&data).unwrap();
    assert_eq!(parsed.applets, applet.into_iter().collect::<Vec<_>>());
    assert_eq!(parsed.classes, classes);
    data
}

#[test]
fn sibling_package_supplies_only_the_class_the_game_imports() {
    let collection = Collection::new();
    let game = collection.module("240x320/Devil May Cry/dmc/dmc.mod", Some(GAME), &[MICRO3D]);
    let engine = collection.module("240x320/Micro3D/imicro3d/imicro3d.mod", None, &[MICRO3D]);
    collection.module(
        "240x320/Other engine/renderer/renderer.mod",
        None,
        &[OTHER_ENGINE],
    );
    collection.module(
        "240x320/Another game/game/game.mod",
        Some(GAME + 1),
        &[MICRO3D],
    );
    assert_eq!(library::extensoes(&game), vec![(engine, vec![MICRO3D])]);
}

#[test]
fn local_provider_wins_over_a_sibling_copy_without_losing_other_local_extensions() {
    let collection = Collection::new();
    let game = collection.module("240x320/Game/game/game.mod", Some(GAME), &[MICRO3D]);
    let local = collection.module("240x320/Game/local/local.mod", None, &[MICRO3D]);
    let other = collection.module("240x320/Game/other/other.mod", None, &[OTHER_ENGINE]);
    collection.module(
        "240x320/AAA shared engine/engine/engine.mod",
        None,
        &[MICRO3D],
    );
    assert_eq!(
        library::extensoes(&game),
        vec![(local, vec![MICRO3D]), (other, vec![OTHER_ENGINE])]
    );
}

#[test]
fn identical_sibling_engine_variants_map_once_in_sorted_path_order() {
    let collection = Collection::new();
    let game = collection.module("240x320/Game/game/game.mod", Some(GAME), &[MICRO3D]);
    let mut first = None;
    for index in (0..9).rev() {
        let path = collection.module(
            &format!("240x320/Engine {index}/imicro3d/imicro3d.mod"),
            None,
            &[MICRO3D],
        );
        if index == 0 {
            first = Some(path);
        }
    }
    let expected = vec![(first.unwrap(), vec![MICRO3D])];
    assert_eq!(library::extensoes(&game), expected);
    assert_eq!(library::extensoes(&game), expected);
}

#[test]
fn missing_import_is_filled_even_when_a_different_local_extension_exists() {
    let collection = Collection::new();
    let game = collection.module(
        "240x320/Game/game/game.mod",
        Some(GAME),
        &[MICRO3D, OTHER_ENGINE],
    );
    let local = collection.module("240x320/Game/renderer/renderer.mod", None, &[OTHER_ENGINE]);
    let shared = collection.module("240x320/Micro3D/imicro3d/imicro3d.mod", None, &[MICRO3D]);
    assert_eq!(
        library::extensoes(&game),
        vec![(local, vec![OTHER_ENGINE]), (shared, vec![MICRO3D])]
    );
}

#[test]
fn applet_without_imports_keeps_package_local_extension_discovery() {
    let collection = Collection::new();
    let game = collection.module("240x320/Game/game/game.mod", Some(GAME), &[]);
    let local = collection.module("240x320/Game/engine/engine.mod", None, &[MICRO3D]);
    collection.module(
        "240x320/Shared engine/other/other.mod",
        None,
        &[OTHER_ENGINE],
    );
    assert_eq!(library::extensoes(&game), vec![(local, vec![MICRO3D])]);
}

#[test]
fn missing_dependency_search_stays_inside_the_collection_bucket() {
    let collection = Collection::new();
    let game = collection.module("240x320/Game/game/game.mod", Some(GAME), &[MICRO3D]);
    collection.module(
        "Other resolution/Engine/imicro3d/imicro3d.mod",
        None,
        &[MICRO3D],
    );
    assert!(library::extensoes(&game).is_empty());
}

#[test]
fn console_layout_keeps_its_package_local_provider() {
    let collection = Collection::new();
    let game = collection.module("Game/mod/app/game.mod", Some(GAME), &[MICRO3D]);
    let engine = collection.module("Game/mod/engine/engine.mod", None, &[MICRO3D]);
    let root = collection.0.join("Game");
    std::fs::create_dir_all(root.join("mif")).unwrap();
    std::fs::rename(root.join("mod/app.mif"), root.join("mif/app.mif")).unwrap();
    std::fs::rename(root.join("mod/engine.mif"), root.join("mif/engine.mif")).unwrap();
    assert_eq!(library::extensoes(&game), vec![(engine, vec![MICRO3D])]);
}

#[test]
fn malformed_sibling_mif_cannot_export_bytes_from_its_header() {
    let collection = Collection::new();
    let game = collection.module("240x320/Game/game/game.mod", Some(GAME), &[MICRO3D]);
    let corrupt = collection.module("240x320/AAA corrupt/engine/engine.mod", None, &[MICRO3D]);
    let valid = collection.module("240x320/ZZZ valid/engine/engine.mod", None, &[MICRO3D]);
    // Inofensivo: uma seção aponta para o próprio cabeçalho. Não testar contagens
    // gigantes aqui, pois o parser sem limites tentaria alocar antes de recusá-las.
    let mut bytes = vec![0u8; 48];
    bytes[..4].copy_from_slice(&[0x11, 0, 1, 0]);
    bytes[8..12].copy_from_slice(&MICRO3D.to_le_bytes());
    bytes[16..20].copy_from_slice(&32u32.to_le_bytes());
    bytes[20..24].copy_from_slice(&1u32.to_le_bytes());
    bytes[32..36].copy_from_slice(&8u32.to_le_bytes());
    bytes[36..40].copy_from_slice(&16u32.to_le_bytes());
    std::fs::write(
        corrupt
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("engine.mif"),
        bytes,
    )
    .unwrap();
    assert_eq!(library::extensoes(&game), vec![(valid, vec![MICRO3D])]);
}

#[test]
fn flat_game_layout_does_not_expand_into_another_resolution_bucket() {
    let collection = Collection::new();
    let game = collection.module("240x320/FlatGame/game.mod", Some(GAME), &[MICRO3D]);
    // Este layout deixa manifesto e módulo lado a lado.
    std::fs::write(game.with_extension("mif"), mif(Some(GAME), &[MICRO3D])).unwrap();
    collection.module(
        "Other resolution/Engine/imicro3d/imicro3d.mod",
        None,
        &[MICRO3D],
    );
    assert!(library::extensoes(&game).is_empty());
}

#[test]
fn invalid_first_sibling_module_does_not_hide_a_valid_provider() {
    let collection = Collection::new();
    let game = collection.module("240x320/Game/game/game.mod", Some(GAME), &[MICRO3D]);
    let corrupt = collection.module("240x320/AAA corrupt/engine/engine.mod", None, &[MICRO3D]);
    std::fs::write(corrupt, []).unwrap();
    let valid = collection.module("240x320/ZZZ valid/engine/engine.mod", None, &[MICRO3D]);
    assert_eq!(library::extensoes(&game), vec![(valid, vec![MICRO3D])]);
    assert_eq!(zeebx::session::extensoes_de(&game).len(), 1);
}
