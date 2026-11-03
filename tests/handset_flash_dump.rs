//! Recuperação do dump auditado comparada ao módulo original intacto do pacote.
use std::{path::PathBuf, time::{Duration, Instant}};
use zeebx::{loader::modfile::ModImage, handset::{launch, display}, session::{Session, Step}, storage::StoragePaths};

fn source() -> PathBuf {
    PathBuf::from(std::env::var_os("BREW_SUDOKU_FLASH_MODULE")
        .expect("BREW_SUDOKU_FLASH_MODULE aponta para sudoku4.mod original"))
}

#[test]
#[ignore = "requer os módulos originais em BREW_SUDOKU_FLASH_MODULE"]
fn original_flash_records_decode_to_the_independently_supplied_intact_module() {
    let path = source();
    let original = std::fs::read(&path).unwrap();
    let intact = std::fs::read(path.with_file_name("sudoku7.mod")).unwrap();
    assert_eq!(original.len(), 168764);
    assert_eq!(intact.len(), 164764);
    let image = ModImage::parse(original.clone()).unwrap();
    assert!(image.image() == intact, "saída inteira deve igualar o original intacto");
    assert_eq!(std::fs::read(&path).unwrap(), original);
    // O parser recebe somente bytes: recuperar não consulta um módulo vizinho.
    assert_eq!(ModImage::parse(original).unwrap().image(), intact);
}

#[test]
#[ignore = "requer o dump original em BREW_SUDOKU_FLASH_MODULE"]
fn changed_or_truncated_flash_dump_is_never_partially_recovered() {
    let original = std::fs::read(source()).unwrap();
    let mut changed = original.clone();
    changed[464] ^= 1;
    assert_eq!(ModImage::parse(changed.clone()).unwrap().image(), changed);
    let mut short = original;
    short.pop();
    assert_eq!(ModImage::parse(short.clone()).unwrap().image(), short);
}

#[test]
#[ignore = "requer pacote original em BREW_SUDOKU_FLASH_MODULE; replay de cinco segundos"]
fn recovered_dump_starts_through_ordinary_session_without_changing_original_files() {
    let path = source();
    let original = std::fs::read(&path).unwrap();
    let package = launch::read(&path).unwrap();
    let geometry = display::automatic(&package.module_sha256, &path).geometry;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("local-tests/wave23-work-20261101/test-runtime")
        .join(format!("flash-{}", std::process::id()));
    let storage = StoragePaths::from_root(root);
    storage.create_dirs().unwrap();
    let mut session = Session::start_handset_with_storage(&path, geometry, &storage, None)
        .expect("o módulo recuperado deve completar a fábrica real");
    let start = Instant::now();
    let mut previous = start;
    while start.elapsed() < Duration::from_secs(5) {
        let now = Instant::now();
        assert_ne!(session.step(now.duration_since(previous), true), Step::Stopped);
        previous = now;
        while session.mostra_quadro_intermediario() {}
        assert!(!session.saiu_sozinho());
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(session.screen().escritas() > 100000);
    assert_eq!(std::fs::read(path).unwrap(), original);
}
