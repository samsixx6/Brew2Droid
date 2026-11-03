//! Listas de classes indexadas: evidência local confirma tipo binário, contagem e NUL final.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use zeebx::library;
use zeebx::loader::miffile::MifFile;

const BINARY: u16 = 0x5000;
const APPLET: u32 = 0x258b_bfe8;
const FLASH: u32 = 0x0103_990d;
const FLASH_CONTEXT: u32 = 0x0103_990c;

fn words(units: &[u32]) -> Vec<u8> {
    units.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn container(resources: &[(u16, u16, Vec<u8>)]) -> Vec<u8> {
    let count = resources.len();
    let table = 32 + count * 8;
    let mut data = vec![0; table + (count + 1) * 4];
    data[..6].copy_from_slice(&[0x11, 0, 1, 0, 1, 0]);
    data[6..8].copy_from_slice(&(count as u16).to_le_bytes());
    data[8..12].copy_from_slice(&32u32.to_le_bytes());
    data[12..16].copy_from_slice(&(count as u32 * 8).to_le_bytes());
    data[16..20].copy_from_slice(&(table as u32).to_le_bytes());
    data[20..24].copy_from_slice(&(count as u32).to_le_bytes());
    let first = data.len() as u32;
    data[24..28].copy_from_slice(&first.to_le_bytes());
    for (index, (kind, id, payload)) in resources.iter().enumerate() {
        let at = 32 + index * 8;
        data[at..at + 2].copy_from_slice(&kind.to_le_bytes());
        data[at + 2..at + 4].copy_from_slice(&id.to_le_bytes());
        data[at + 6..at + 8].copy_from_slice(&(index as u16).to_le_bytes());
        let offset = data.len() as u32;
        data[table + index * 4..table + index * 4 + 4].copy_from_slice(&offset.to_le_bytes());
        data.extend_from_slice(payload);
    }
    let end = data.len() as u32;
    data[table + count * 4..table + count * 4 + 4].copy_from_slice(&end.to_le_bytes());
    data[28..32].copy_from_slice(&(end - first).to_le_bytes());
    data
}

fn resources(
    applet: bool,
    kind: u16,
    id: u16,
    declared: u16,
    payload: Vec<u8>,
) -> Vec<(u16, u16, Vec<u8>)> {
    let mut info = vec![0; 40];
    info[..4].copy_from_slice(&0x1002u32.to_le_bytes());
    info[24..26].copy_from_slice(&5u16.to_le_bytes());
    info[28..30].copy_from_slice(&u16::from(applet).to_le_bytes());
    let count_offset = if id == 1 { 26 } else { 30 };
    info[count_offset..count_offset + 2].copy_from_slice(&declared.to_le_bytes());
    // Um byte anterior deixa a lista desalinhada no arquivo, como o MIF real de Bunny.
    let mut entries = vec![(BINARY, 0, info), (1, 20, vec![3])];
    if applet {
        let mut record = vec![0; 20];
        record[..4].copy_from_slice(&APPLET.to_le_bytes());
        record[8..10].copy_from_slice(&20u16.to_le_bytes());
        entries.push((BINARY, 2, record));
    }
    entries.push((kind, id, payload));
    entries
}

fn manifest(applet: bool, id: u16, declared: u16, units: &[u32]) -> Vec<u8> {
    container(&resources(applet, BINARY, id, declared, words(units)))
}

#[test]
fn mif_array_imports_two_cids_from_the_declared_binary_resource() {
    let data = manifest(true, 3, 2, &[FLASH, FLASH_CONTEXT, 0]);
    let before = data.clone();
    let parsed = MifFile::parse(&data).unwrap();
    assert_eq!(parsed.applets, [APPLET]);
    assert_eq!(parsed.classes, [FLASH, FLASH_CONTEXT]);
    assert!(parsed.extensao().is_empty());
    assert_eq!(
        data, before,
        "leitura não altera máscara de privilégios nem outro campo"
    );
}

#[test]
fn mif_array_exports_multiple_cids_in_source_order() {
    // extension3d e qc3dwidget reais têm listas de sete e duas classes, respectivamente.
    let parsed = MifFile::parse(&manifest(false, 1, 2, &[FLASH_CONTEXT, FLASH, 0])).unwrap();
    assert_eq!(parsed.extensao(), [FLASH_CONTEXT, FLASH]);
}

#[test]
fn mif_array_keeps_imports_and_exports_in_their_documented_roles() {
    let own_service = 0x1234_5678;
    let mut game = resources(true, BINARY, 3, 2, words(&[FLASH, FLASH_CONTEXT, 0]));
    game[0].2[26..28].copy_from_slice(&1u16.to_le_bytes());
    game.push((BINARY, 1, words(&[own_service, 0])));
    assert_eq!(
        MifFile::parse(&container(&game)).unwrap().classes,
        [FLASH, FLASH_CONTEXT]
    );

    let mut extension = resources(false, BINARY, 1, 2, words(&[FLASH, FLASH_CONTEXT, 0]));
    extension[0].2[30..32].copy_from_slice(&1u16.to_le_bytes());
    extension.push((BINARY, 3, words(&[own_service, 0])));
    assert_eq!(
        MifFile::parse(&container(&extension)).unwrap().extensao(),
        [FLASH, FLASH_CONTEXT]
    );
}

#[test]
fn mif_array_keeps_single_cid_and_unindexed_legacy_records() {
    assert_eq!(
        MifFile::parse(&manifest(true, 3, 1, &[FLASH, 0]))
            .unwrap()
            .classes,
        [FLASH]
    );
    assert_eq!(
        MifFile::parse(&manifest(false, 1, 1, &[FLASH, 0]))
            .unwrap()
            .extensao(),
        [FLASH]
    );
    let mut data = vec![0; 52];
    data[..2].copy_from_slice(&0x11u16.to_le_bytes());
    data[16..20].copy_from_slice(&32u32.to_le_bytes());
    data[20..24].copy_from_slice(&1u32.to_le_bytes());
    data[32..36].copy_from_slice(&44u32.to_le_bytes());
    data[36..40].copy_from_slice(&52u32.to_le_bytes());
    data[44..52].copy_from_slice(&words(&[FLASH, 0]));
    assert_eq!(MifFile::parse(&data).unwrap().extensao(), [FLASH]);
}

fn classless(data: &[u8]) {
    match MifFile::parse(data) {
        Ok(parsed) => assert!(
            parsed.classes.is_empty(),
            "metadado inválido não anuncia classes: {:?}",
            parsed.classes
        ),
        Err(_) => {}
    }
}

#[test]
fn mif_array_does_not_reinterpret_other_resource_types_or_ids() {
    for (kind, id) in [(1, 3), (6, 3), (BINARY, 5), (BINARY, 99)] {
        classless(&container(&resources(
            true,
            kind,
            id,
            1,
            words(&[FLASH, 0]),
        )));
    }
}

#[test]
fn mif_array_checks_declaration_length_before_advertising_any_class() {
    for (declared, units) in [
        (0, vec![FLASH, 0]),
        (2, vec![FLASH, 0]),
        (1, vec![FLASH, FLASH_CONTEXT, 0]),
        (3, vec![FLASH, FLASH_CONTEXT, 0]),
        (u16::MAX, vec![FLASH, FLASH_CONTEXT, 0]),
    ] {
        classless(&manifest(true, 3, declared, &units));
        classless(&manifest(false, 1, declared, &units));
    }
    let mut no_known_declaration = resources(true, BINARY, 3, 2, words(&[FLASH, FLASH_CONTEXT, 0]));
    no_known_declaration[0].2[..4].copy_from_slice(&0x9999u32.to_le_bytes());
    classless(&container(&no_known_declaration));
}

#[test]
fn mif_array_rejects_truncated_unaligned_or_unterminated_payloads_and_interior_zero() {
    for payload in [
        words(&[FLASH, FLASH_CONTEXT, FLASH]),
        words(&[FLASH, 0, 0]),
        words(&[0, FLASH, 0]),
        words(&[FLASH, FLASH_CONTEXT, 0])[..11].to_vec(),
    ] {
        classless(&container(&resources(true, BINARY, 3, 2, payload)));
    }
    let data = manifest(true, 3, 2, &[FLASH, FLASH_CONTEXT, 0]);
    assert!(MifFile::parse(&data[..data.len() - 1]).is_err());
    assert!(
        MifFile::parse(&manifest(true, 3, 0, &[0]))
            .unwrap()
            .classes
            .is_empty()
    );
}

#[test]
fn mif_array_deduplicates_valid_ids_without_advertising_zero() {
    assert_eq!(
        MifFile::parse(&manifest(true, 3, 2, &[FLASH, FLASH, 0]))
            .unwrap()
            .classes,
        [FLASH]
    );
}

#[test]
fn mif_array_invalid_index_ranges_do_not_fall_back_to_payload_shape() {
    let data = manifest(true, 3, 1, &[FLASH, 0]);
    let mut outside_index = data.clone();
    outside_index[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    classless(&outside_index);
    let mut truncated_index = data.clone();
    truncated_index[6..8].copy_from_slice(&u16::MAX.to_le_bytes());
    classless(&truncated_index);
    let mut bad_section = data.clone();
    let entries = u16::from_le_bytes(data[6..8].try_into().unwrap()) as usize;
    let section_field = 32 + (entries - 1) * 8 + 6;
    bad_section[section_field..section_field + 2].copy_from_slice(&u16::MAX.to_le_bytes());
    classless(&bad_section);
}

struct Collection(PathBuf);
impl Drop for Collection {
    fn drop(&mut self) {
        let directory = self.0.canonicalize().unwrap();
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests")
            .canonicalize()
            .unwrap();
        assert!(directory.starts_with(root));
        std::fs::remove_dir_all(directory).unwrap();
    }
}

#[test]
fn mif_array_production_discovery_follows_multi_cid_imports_and_exports() {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let collection = Collection(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests")
            .join(format!(
                "mif-arrays-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )),
    );
    let game = collection.0.join("Game/game/game.mod");
    let engine = collection.0.join("Engine/engine/engine.mod");
    for (path, data) in [
        (&game, manifest(true, 3, 2, &[FLASH, FLASH_CONTEXT, 0])),
        (&engine, manifest(false, 1, 2, &[FLASH, FLASH_CONTEXT, 0])),
    ] {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, words(&[0xe3a0_0000, 0xe12f_ff1e])).unwrap();
        let dir = path.parent().unwrap();
        std::fs::write(
            dir.parent()
                .unwrap()
                .join(dir.file_name().unwrap())
                .with_extension("mif"),
            data,
        )
        .unwrap();
    }
    assert_eq!(
        library::extensoes(&game),
        [(engine, vec![FLASH, FLASH_CONTEXT])]
    );
}
