use std::path::{Path, PathBuf};
use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend, Reg};
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

const OUT: u32 = loader::STACK_BASE + 0x100;
const NAME: u32 = OUT + 0x100;
const FIELDS: u32 = OUT + 0x200;
const DATA: u32 = OUT + 0x400;
const META: u32 = OUT + 0x600;

fn work() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("local-tests")
        .join(format!(
            "db-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
    std::fs::create_dir_all(root.join("content")).unwrap();
    root
}

fn machine(root: &Path) -> Machine<BackendPadrao> {
    machine_with_overlay(root, true)
}

fn machine_with_overlay(root: &Path, overlay: bool) -> Machine<BackendPadrao> {
    let storage = StoragePaths::from_root(root.join("runtime"));
    storage.create_dirs().unwrap();
    std::fs::create_dir_all(root.join("save-overlay")).unwrap();
    let image = ModImage::parse(
        [0xe3a0_0000u32, 0xe12f_ff1e]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect(),
    )
    .unwrap();
    let mut m = Machine::new_with_storage(
        BackendPadrao::new().unwrap(),
        loader::load_with(&image, &[]).unwrap(),
        root.join("content"),
        &storage,
        overlay.then(|| root.join("save-overlay")),
    );
    assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
    m
}

fn invoke(m: &mut Machine<BackendPadrao>, function: u32, args: [u32; 4]) -> u32 {
    match m.call_guest(function, args, 1000).unwrap() {
        Outcome::Returned { code } => code,
        other => panic!("database call: {other:?}"),
    }
}
fn call(m: &mut Machine<BackendPadrao>, object: u32, slot: u32, a1: u32, a2: u32, a3: u32) -> u32 {
    let table = m.cpu().read_u32(object).unwrap();
    let function = m.cpu().read_u32(table + slot * 4).unwrap();
    invoke(m, function, [object, a1, a2, a3])
}
fn manager(m: &mut Machine<BackendPadrao>) -> u32 {
    assert_eq!(
        invoke(
            m,
            aee::encode(Interface::Shell, 2),
            [loader::OBJECT_BASE, 0x0100_1008, OUT, 0]
        ),
        0,
        "classe DBMGR legada"
    );
    let object = m.cpu().read_u32(OUT).unwrap();
    assert_ne!(object, 0);
    object
}
fn open(m: &mut Machine<BackendPadrao>, mgr: u32, name: &str, create: bool) -> u32 {
    m.cpu_mut()
        .write_mem(NAME, format!("{name}\0").as_bytes())
        .unwrap();
    let sp = m.cpu().read_reg(Reg::Sp);
    m.cpu_mut().write_reg(Reg::Sp, sp - 32);
    m.cpu_mut().write_u32(sp - 32, 20).unwrap(); // quinto argumento wMinRecs, como o Chess.
    let result = call(m, mgr, 2, NAME, u32::from(create), 1000);
    m.cpu_mut().write_reg(Reg::Sp, sp);
    result
}
fn field(m: &mut Machine<BackendPadrao>, index: u32, kind: u8, name: u8, bytes: &[u8]) {
    let data = DATA + index * 0x40;
    m.cpu_mut().write_mem(data, bytes).unwrap();
    let mut header = vec![kind, name];
    header.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
    header.extend_from_slice(&data.to_le_bytes());
    m.cpu_mut().write_mem(FIELDS + index * 8, &header).unwrap();
}
fn bytes(m: &Machine<BackendPadrao>, at: u32, size: usize) -> Vec<u8> {
    let mut data = vec![0; size];
    m.cpu().read_mem(at, &mut data).unwrap();
    data
}
fn clean(root: PathBuf) {
    assert!(
        root.canonicalize().unwrap().starts_with(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .canonicalize()
                .unwrap()
        )
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_database_factory_create_flag_and_overlay_persistence() {
    let root = work();
    let mut m = machine(&root);
    let mgr = manager(&mut m);
    assert_eq!(open(&mut m, mgr, "save.db", false), 0);
    let db = open(&mut m, mgr, "save.db", true);
    assert_ne!(db, 0);
    assert_eq!(
        call(&mut m, db, 2, 0, 0, 0),
        0,
        "capacity hints do not create records"
    );
    field(&mut m, 0, 5, 1, b"saved");
    let record = call(&mut m, db, 6, FIELDS, 1, 0);
    assert_ne!(record, 0);
    let id = call(&mut m, record, 5, 0, 0, 0);
    assert_ne!(id, 0xffff);
    assert_eq!(call(&mut m, db, 1, 0, 0, 0), 1, "record retains database");
    assert_eq!(call(&mut m, record, 1, 0, 0, 0), 0);
    drop(m);
    assert!(!root.join("content/save.db").exists());
    assert!(root.join("save-overlay/save.db").is_file());
    let mut m = machine(&root);
    let mgr = manager(&mut m);
    let db = open(&mut m, mgr, "save.db", false);
    assert_ne!(db, 0);
    assert_eq!(call(&mut m, db, 2, 0, 0, 0), 1);
    let record = call(&mut m, db, 5, id, 0, 0);
    assert_ne!(record, 0);
    assert_eq!(call(&mut m, record, 6, META, META + 2, 0), 5);
    let value = call(&mut m, record, 7, 0, 0, 0);
    assert_eq!(bytes(&m, value, 5), b"saved");
    drop(m);
    clean(root);
}

#[test]
fn legacy_database_copies_fields_and_writes_arm_sized_metadata() {
    let root = work();
    let mut m = machine(&root);
    let mgr = manager(&mut m);
    let db = open(&mut m, mgr, "fields", true);
    field(&mut m, 0, 3, 1, &0x1234_5678u32.to_le_bytes());
    field(&mut m, 1, 4, 17, &[b'H', 0, b'i', 0, 0, 0]);
    field(&mut m, 2, 5, 2, &[9, 0, 8, 7]);
    let r = call(&mut m, db, 6, FIELDS, 3, 0);
    assert_ne!(r, 0);
    m.cpu_mut().write_mem(DATA, &[0; 0xc0]).unwrap();
    m.cpu_mut().write_mem(META, &[0xaa; 16]).unwrap();
    assert_eq!(call(&mut m, r, 6, META, META + 4, 0), 3);
    assert_eq!(bytes(&m, META, 8), [1, 0xaa, 0xaa, 0xaa, 4, 0, 0xaa, 0xaa]);
    assert_eq!(call(&mut m, r, 9, META + 8, 0, 0), 1);
    assert_eq!(m.cpu().read_u32(META + 8).unwrap(), 0x1234_5678);
    assert_eq!(call(&mut m, r, 8, META + 12, 0, 0), 0);
    assert_eq!(m.cpu().read_u32(META + 12).unwrap(), 0xaaaa_aaaa);
    assert_eq!(call(&mut m, r, 6, 0, 0, 0), 4);
    let string = call(&mut m, r, 10, 0, 0, 0);
    assert_eq!(bytes(&m, string, 6), [b'H', 0, b'i', 0, 0, 0]);
    assert_eq!(call(&mut m, r, 6, 0, 0, 0), 5);
    let binary = call(&mut m, r, 7, META, META + 2, META + 4);
    assert_eq!(bytes(&m, binary, 4), [9, 0, 8, 7]);
    assert_eq!(bytes(&m, META, 8), [2, 0xaa, 5, 0xaa, 4, 0, 0xaa, 0xaa]);
    assert_eq!(call(&mut m, r, 6, 0, 0, 0), 0);
    drop(m);
    clean(root);
}

#[test]
fn legacy_database_record_ids_update_delete_and_enumeration() {
    let root = work();
    let mut m = machine(&root);
    let mgr = manager(&mut m);
    let db = open(&mut m, mgr, "records", true);
    field(&mut m, 0, 2, 1, &[0x34, 0x12]);
    let a = call(&mut m, db, 6, FIELDS, 1, 0);
    let b = call(&mut m, db, 6, FIELDS, 1, 0);
    let aid = call(&mut m, a, 5, 0, 0, 0);
    let bid = call(&mut m, b, 5, 0, 0, 0);
    assert_ne!(aid, bid);
    field(&mut m, 0, 5, 3, b"new");
    assert_eq!(call(&mut m, b, 3, FIELDS, 1, 0), 0);
    assert_eq!(call(&mut m, b, 2, 0, 0, 0), 0);
    assert_eq!(call(&mut m, b, 6, 0, 0, 0), 5);
    let value = call(&mut m, b, 7, 0, 0, 0);
    assert_eq!(bytes(&m, value, 3), b"new");
    assert_eq!(call(&mut m, a, 4, 0, 0, 0), 0);
    assert_eq!(call(&mut m, db, 5, aid, 0, 0), 0);
    assert_eq!(call(&mut m, db, 2, 0, 0, 0), 1);
    assert_eq!(call(&mut m, db, 3, 0, 0, 0), 0);
    let next = call(&mut m, db, 4, 0, 0, 0);
    assert_ne!(next, 0);
    assert_eq!(call(&mut m, next, 5, 0, 0, 0), bid);
    assert_eq!(call(&mut m, db, 4, 0, 0, 0), 0);
    drop(m);
    clean(root);
}

#[test]
fn legacy_database_imports_only_the_known_empty_seed_without_altering_content() {
    let root = work();
    let seed = [0, 0, 0xff, 0xff, 0, 0, 0, 0];
    std::fs::write(root.join("content/memopad"), seed).unwrap();
    std::fs::write(root.join("content/memopad.idx"), []).unwrap();
    std::fs::write(root.join("content/unknown"), b"not a known database").unwrap();
    let mut m = machine(&root);
    let mgr = manager(&mut m);
    assert_eq!(open(&mut m, mgr, "unknown", true), 0);
    let db = open(&mut m, mgr, "MemoPad", false);
    assert_ne!(db, 0);
    assert_eq!(call(&mut m, db, 2, 0, 0, 0), 0);
    field(&mut m, 0, 4, 17, &[b'A', 0, 0, 0]);
    assert_ne!(call(&mut m, db, 6, FIELDS, 1, 0), 0);
    assert_eq!(std::fs::read(root.join("content/memopad")).unwrap(), seed);
    assert_eq!(
        std::fs::read(root.join("content/unknown")).unwrap(),
        b"not a known database"
    );
    drop(m);
    clean(root);
}

#[test]
fn legacy_database_refuses_native_seed_conversion_without_a_save_overlay() {
    let root = work();
    let seed = [0, 0, 0xff, 0xff, 0, 0, 0, 0];
    std::fs::write(root.join("content/memopad"), seed).unwrap();
    std::fs::write(root.join("content/memopad.idx"), []).unwrap();
    let mut m = machine_with_overlay(&root, false);
    let mgr = manager(&mut m);
    assert_eq!(
        open(&mut m, mgr, "MemoPad", false),
        0,
        "seed precisa de cópia gravável"
    );
    assert_eq!(open(&mut m, mgr, "MemoPad", true), 0);
    assert_eq!(std::fs::read(root.join("content/memopad")).unwrap(), seed);
    assert!(
        std::fs::read(root.join("content/memopad.idx"))
            .unwrap()
            .is_empty()
    );
    drop(m);
    clean(root);
}

#[test]
fn legacy_database_invalid_updates_do_not_replace_records_and_remove_is_honest() {
    let root = work();
    let mut m = machine(&root);
    let mgr = manager(&mut m);
    let db = open(&mut m, mgr, "remove", true);
    field(&mut m, 0, 1, 1, &[7]);
    let r = call(&mut m, db, 6, FIELDS, 1, 0);
    assert_eq!(call(&mut m, r, 3, 0, 1, 0), 0x303);
    assert_eq!(call(&mut m, r, 3, FIELDS, u32::MAX, 0), 0x304);
    assert_eq!(call(&mut m, r, 6, 0, 0, 0), 1);
    let ptr = call(&mut m, r, 7, 0, 0, 0);
    assert_eq!(bytes(&m, ptr, 1), [7]);
    call(&mut m, r, 1, 0, 0, 0);
    call(&mut m, db, 1, 0, 0, 0);
    assert_eq!(call(&mut m, mgr, 3, NAME, 0, 0), 0);
    assert_eq!(open(&mut m, mgr, "remove", false), 0);
    drop(m);
    clean(root);
}

#[test]
fn legacy_database_state_keeps_both_cursors_and_accepts_states_without_database_section() {
    let root = work();
    let mut m = machine(&root);
    let mgr = manager(&mut m);
    let db = open(&mut m, mgr, "state", true);
    field(&mut m, 0, 1, 1, &[7]);
    field(&mut m, 1, 2, 2, &[0x78, 0x56]);
    let r = call(&mut m, db, 6, FIELDS, 2, 0);
    assert_eq!(call(&mut m, r, 6, 0, 0, 0), 1);
    let ptr = call(&mut m, r, 7, 0, 0, 0);
    assert_eq!(bytes(&m, ptr, 1), [7]);
    let enumerated = call(&mut m, db, 4, 0, 0, 0);
    assert_ne!(enumerated, 0);
    let state = m.grava_estado();
    let mut restored = machine(&root);
    restored.restaura_estado(&state).unwrap();
    assert_eq!(bytes(&restored, ptr, 1), [7]);
    assert_eq!(call(&mut restored, r, 6, 0, 0, 0), 2);
    assert_eq!(call(&mut restored, r, 8, META, 0, 0), 1);
    assert_eq!(bytes(&restored, META, 2), [0x78, 0x56]);
    assert_eq!(call(&mut restored, db, 4, 0, 0, 0), 0);
    let old = machine(&root).grava_estado();
    let reader = zeebx::save_state::Leitor::abre(&old).unwrap();
    let sections: Vec<_> = reader
        .nomes()
        .into_iter()
        .filter(|name| *name != "db.legacy")
        .map(|name| (name.to_string(), reader.secao(name).unwrap().to_vec()))
        .collect();
    restored
        .restaura_estado(&zeebx::save_state::escreve(&sections))
        .unwrap();
    drop(restored);
    drop(m);
    clean(root);
}

fn buffered_state() -> (PathBuf, Vec<u8>, u32, u32, u32, u32) {
    let root = work();
    let mut m = machine(&root);
    let mgr = manager(&mut m);
    let db = open(&mut m, mgr, "state-validation", true);
    field(&mut m, 0, 5, 1, b"first");
    field(&mut m, 1, 5, 2, b"other");
    let r = call(&mut m, db, 6, FIELDS, 2, 0);
    assert_ne!(r, 0);
    assert_eq!(call(&mut m, r, 6, 0, 0, 0), 5);
    let first = call(&mut m, r, 7, 0, 0, 0);
    assert_eq!(call(&mut m, r, 6, 0, 0, 0), 5);
    let second = call(&mut m, r, 7, 0, 0, 0);
    assert_ne!(first, second);
    let enumerated = call(&mut m, db, 4, 0, 0, 0);
    assert_ne!(enumerated, 0);
    let state = m.grava_estado();
    drop(m);
    (root, state, db, r, first, second)
}

fn replace_section(state: &[u8], section: &str, replacement: Vec<u8>) -> Vec<u8> {
    let reader = zeebx::save_state::Leitor::abre(state).unwrap();
    assert!(reader.secao(section).is_some());
    let sections: Vec<_> = reader
        .nomes()
        .into_iter()
        .map(|name| {
            (
                name.to_string(),
                if name == section {
                    replacement.clone()
                } else {
                    reader.secao(name).unwrap().to_vec()
                },
            )
        })
        .collect();
    zeebx::save_state::escreve(&sections)
}

fn replace_database_text(state: &[u8], from: &str, to: &str) -> Vec<u8> {
    let reader = zeebx::save_state::Leitor::abre(state).unwrap();
    let text = std::str::from_utf8(reader.secao("db.legacy").unwrap()).unwrap();
    assert!(text.contains(from), "fixture precisa conter {from}");
    replace_section(state, "db.legacy", text.replacen(from, to, 1).into_bytes())
}

#[test]
fn legacy_database_state_rejects_unallocated_field_buffer() {
    let (root, state, _, _, first, _) = buffered_state();
    let corrupt = replace_database_text(
        &state,
        &format!("[{first},5]"),
        &format!("[{},5]", loader::HEAP_BASE + 0x100_0000),
    );
    assert!(
        machine(&root).restaura_estado(&corrupt).is_err(),
        "endereço mapeado não é alocação viva"
    );
    clean(root);
}

#[test]
fn legacy_database_state_rejects_field_buffer_larger_than_its_allocation() {
    let (root, state, _, _, first, _) = buffered_state();
    let corrupt =
        replace_database_text(&state, &format!("[{first},5]"), &format!("[{first},65535]"));
    assert!(
        machine(&root).restaura_estado(&corrupt).is_err(),
        "capacidade não pode exceder o bloco vivo"
    );
    clean(root);
}

#[test]
fn legacy_database_state_rejects_duplicate_field_buffer_ownership() {
    let (root, state, _, _, first, second) = buffered_state();
    let corrupt = replace_database_text(&state, &format!("[{second},5]"), &format!("[{first},5]"));
    assert!(
        machine(&root).restaura_estado(&corrupt).is_err(),
        "dois campos não podem liberar o mesmo bloco"
    );
    clean(root);
}

#[test]
fn legacy_database_state_rejects_handles_with_different_object_interfaces() {
    let (root, state, db, record, _, _) = buffered_state();
    let reader = zeebx::save_state::Leitor::abre(&state).unwrap();
    for object in [db, record] {
        let kinds = reader
            .pares("objects.kinds")
            .unwrap()
            .into_iter()
            .map(|(id, kind)| {
                (
                    id,
                    if id == object {
                        aee::codigo(Interface::FileMgr)
                    } else {
                        kind
                    },
                )
            });
        let mut sections = zeebx::save_state::Secoes::nova();
        sections.poe_mapa("objects.kinds", kinds);
        let encoded = sections.fecha();
        let encoded = zeebx::save_state::Leitor::abre(&encoded).unwrap();
        let corrupt = replace_section(
            &state,
            "objects.kinds",
            encoded.secao("objects.kinds").unwrap().to_vec(),
        );
        assert!(
            machine(&root).restaura_estado(&corrupt).is_err(),
            "interface errada para {object:#x}"
        );
    }
    clean(root);
}

#[test]
fn legacy_database_state_rejects_never_issued_record_and_enumeration_ids() {
    let (root, state, _, _, _, _) = buffered_state();
    for (from, to) in [
        ("\"state-validation\",0", "\"state-validation\",65534"),
        ("\"id\":0", "\"id\":65534"),
    ] {
        let corrupt = replace_database_text(&state, from, to);
        assert!(
            machine(&root).restaura_estado(&corrupt).is_err(),
            "ID do registro nunca foi emitido"
        );
    }
    clean(root);
}

#[test]
fn native_database_imports_typed_records_into_overlay_without_modifying_source() {
    let root = work();
    // OEMDB.c: cabeçalho 8, registro 4, campos packed de AEEDB.h e índice no final.
    let mut seed = vec![1,0,7,0,20,0,0,0, 7,0,8,0, 5,2,4,0,b's',b'a',b'v',b'e'];
    seed.extend_from_slice(&[1,0,0,0,7,0,0,0,8,0,0,0]);
    std::fs::write(root.join("content/native.db"), &seed).unwrap();
    let mut m = machine(&root); let mgr = manager(&mut m);
    let db = open(&mut m, mgr, "native.db", false);
    assert_ne!(db,0,"banco OEM não vazio tem de preservar os registros");
    assert_eq!(call(&mut m,db,2,0,0,0),1);
    let rec=call(&mut m,db,5,7,0,0); assert_ne!(rec,0);
    assert_eq!(call(&mut m,rec,6,META,META+2,0),5);
    let ptr=call(&mut m,rec,7,0,0,0);assert_eq!(bytes(&m,ptr,4),b"save");
    assert_eq!(std::fs::read(root.join("content/native.db")).unwrap(),seed);
    drop(m); clean(root);
}

#[test]
fn malformed_native_database_is_rejected_before_creating_an_overlay() {
    let root=work();let valid=[1,0,7,0,20,0,0,0,7,0,8,0,5,2,4,0,b's',b'a',b'v',b'e',1,0,0,0,7,0,0,0,8,0,0,0];
    for (i,at,value) in [(0,0,2),(1,8,6),(2,10,99),(3,12,9),(4,28,250)] {
        let mut bad=valid.to_vec();bad[at]=value;
        let name=format!("bad-{i}.db");std::fs::write(root.join("content").join(&name),&bad).unwrap();
        let mut m=machine(&root);let mgr=manager(&mut m);
        assert_eq!(open(&mut m,mgr,&name,true),0,"não converter índice/campo inválido");
        assert!(!root.join("save-overlay").join(&name).exists());
        assert_eq!(std::fs::read(root.join("content").join(&name)).unwrap(),bad);
    }
    clean(root);
}
