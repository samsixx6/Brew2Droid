//! IDBMgr/IDatabase/IDBRecord do AEEDB.h: registros tipados, separados do ISQLMgr.
//!
//! O SDK publica a ABI dos campos, mas não o formato de arquivos do fabricante. Bancos novos
//! usam um esquema SQLite próprio; bancos OEM com índice e campos AEEDB validados são
//! importados no overlay. Um formato desconhecido é recusado, sem apagar o conteúdo.

use super::*;
use crate::brew::vfs::OpenIntent;
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

const BAD_RECORD: u32 = 0x300;
const BAD_STATE: u32 = 0x301;
const NULL_FIELD: u32 = 0x303;
const BAD_FIELD_COUNT: u32 = 0x304;
const BAD_FILENAME: u32 = 0x103;
const NO_MEMORY: u32 = 2;
const MAX_FIELDS: usize = 1024;
const MAX_DATA: usize = 32 * 1024 * 1024;
const EMPTY_SEED: &[u8] = &[0, 0, 0xff, 0xff, 0, 0, 0, 0];

struct Field {
    kind: u8,
    name: u8,
    data: Vec<u8>,
}

pub(super) struct LegacyDatabase {
    name: String,
    cursor: Option<u16>,
    connection: Connection,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct LegacyRecord {
    database: u32,
    id: u16,
    cursor: i32,
    /// Ponteiros do guest, nunca ponteiros para os buffers do SQLite. A memória entra no save
    /// com o heap; estes pares preservam a posse para Release e o crescimento dos campos.
    buffers: BTreeMap<u32, (u32, u32)>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Snapshot {
    version: u32,
    databases: Vec<(u32, String, Option<u16>)>,
    records: BTreeMap<u32, LegacyRecord>,
}

// OEMDB.c + AEEDB.h: índice <recID:u32, offset:u32> e campos packed, sem ponteiros.
// Validar tudo antes de criar o overlay impede tratar um banco desconhecido como vazio.
struct NativeSeed {
    next: u32,
    records: Vec<(u16, Vec<Field>)>,
}
fn native_seed(bytes: &[u8]) -> Option<NativeSeed> {
    let word = |at: usize| -> Option<u16> {
        Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
    };
    let dword = |at: usize| -> Option<u32> {
        Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    };
    let count = word(0)? as usize;
    let last = word(2)?;
    let index = dword(4)? as usize;
    if index < 8 || index + 4 > bytes.len() || count > 65535 {
        return None;
    }
    let entries = word(index)? as usize;
    if entries > 65535 || index.checked_add(4 + entries * 8)? != bytes.len() {
        return None;
    }
    let mut records = Vec::new();
    let mut ids = std::collections::BTreeSet::new();
    let mut spans = Vec::new();
    let mut total = 0usize;
    for i in 0..entries {
        let id = dword(index + 4 + i * 8)?;
        let offset = dword(index + 8 + i * 8)? as usize;
        if id >= 65535 || !ids.insert(id) {
            return None;
        }
        if offset == 0 {
            continue;
        }
        let size = word(offset + 2)? as usize;
        let end = offset.checked_add(4 + size)?;
        if offset < 8 || end > index || word(offset)? as u32 != id {
            return None;
        }
        spans.push((offset, end));
        let mut at = offset + 4;
        let mut fields = Vec::new();
        while at < end {
            let kind = bytes[at];
            if kind == 0 && at + 1 == end {
                break;
            }
            let name = *bytes.get(at + 1)?;
            at += 2;
            let size = match kind {
                1 => 1,
                2 => 2,
                3 => 4,
                4..=8 => {
                    let n = word(at)? as usize;
                    at += 2;
                    n
                }
                _ => return None,
            };
            if at.checked_add(size)? > end || fields.len() >= MAX_FIELDS {
                return None;
            }
            total = total.checked_add(size)?;
            if total > MAX_DATA {
                return None;
            }
            fields.push(Field {
                kind,
                name,
                data: bytes[at..at + size].to_vec(),
            });
            at += size;
        }
        records.push((id as u16, fields));
    }
    spans.sort_unstable();
    if spans.windows(2).any(|s| s[0].1 > s[1].0) || records.len() != count {
        return None;
    }
    let max = ids.last().copied().unwrap_or(0);
    if last != 0xffff && u32::from(last) < max {
        return None;
    }
    let next = if count == 0 && last == 0xffff {
        0
    } else {
        u32::from(last).max(max) + 1
    };
    if next > 65535 {
        return None;
    }
    Some(NativeSeed { next, records })
}

// O arquivo temporário é nosso (create_new) e fica no mesmo diretório para publicação
// por rename. Uma falha SQLite não pode criar um overlay que esconda o pacote original.
fn initialize_database(
    target: &std::path::Path,
    imported: Option<NativeSeed>,
) -> Result<(), String> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let temporary = loop {
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = target.with_file_name(format!(".brewdb-import-{}-{n}.tmp", std::process::id()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => {
                drop(file);
                break path;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    };
    let result = (|| {
        let mut connection =
            Connection::open_with_flags(&temporary, OpenFlags::SQLITE_OPEN_READ_WRITE)
                .map_err(|e| e.to_string())?;
        let transaction = connection.transaction().map_err(|e| e.to_string())?;
        transaction.execute_batch("
            CREATE TABLE zeebx_legacy_db(version INTEGER NOT NULL,next_id INTEGER NOT NULL);
            INSERT INTO zeebx_legacy_db VALUES(1,0);
            CREATE TABLE records(id INTEGER PRIMARY KEY CHECK(id>=0 AND id<65535));
            CREATE TABLE fields(record_id INTEGER NOT NULL,ordinal INTEGER NOT NULL,kind INTEGER NOT NULL,
                name INTEGER NOT NULL,data BLOB NOT NULL,PRIMARY KEY(record_id,ordinal));").map_err(|e|e.to_string())?;
        if let Some(seed) = imported {
            for (id, fields) in seed.records {
                transaction
                    .execute("INSERT INTO records(id) VALUES(?1)", [id])
                    .map_err(|e| e.to_string())?;
                LegacyDatabase::insert_fields(&transaction, id, &fields)
                    .map_err(|e| format!("campo nativo: {e}"))?;
            }
            transaction
                .execute("UPDATE zeebx_legacy_db SET next_id=?1", [seed.next])
                .map_err(|e| e.to_string())?;
        }
        transaction.commit().map_err(|e| e.to_string())?;
        connection.close().map_err(|(_, e)| e.to_string())?;
        std::fs::rename(&temporary, target).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod import_failure_tests {
    use super::*;
    #[test]
    fn failed_import_does_not_publish_or_replace_the_overlay() {
        let root = std::env::temp_dir().join(format!("brewdb-failure-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let target = root.join("test.db");
        for existing in [false, true] {
            if existing {
                std::fs::write(&target, EMPTY_SEED).unwrap();
            }
            // Injetar uma falha real de INSERT depois de criar esquema e primeira linha.
            let seed = NativeSeed {
                next: 2,
                records: vec![(1, Vec::new()), (1, Vec::new())],
            };
            assert!(initialize_database(&target, Some(seed)).is_err());
            if existing {
                assert_eq!(std::fs::read(&target).unwrap(), EMPTY_SEED);
                std::fs::remove_file(&target).unwrap();
            } else {
                assert!(!target.exists());
            }
            assert_eq!(
                std::fs::read_dir(&root).unwrap().count(),
                0,
                "nenhum temporário pode ocultar o pacote"
            );
        }
        std::fs::remove_dir(&root).unwrap();
    }
}

impl LegacyDatabase {
    fn issued(&self, id: u16) -> Result<bool, u32> {
        let next: u32 = self
            .connection
            .query_row("SELECT next_id FROM zeebx_legacy_db", [], |row| row.get(0))
            .map_err(|_| BAD_STATE)?;
        Ok(u32::from(id) < next && next <= 0xffff)
    }

    fn exists(&self, id: u16) -> Result<bool, u32> {
        self.connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM records WHERE id=?1)",
                [id],
                |row| row.get(0),
            )
            .map_err(|_| BAD_STATE)
    }

    fn fields(&self, id: u16) -> Result<Option<Vec<Field>>, u32> {
        if !self.exists(id)? {
            return Ok(None);
        }
        let mut statement = self.connection.prepare(
            "SELECT kind,name,length(data),data FROM fields WHERE record_id=?1 ORDER BY ordinal LIMIT 1025",
        ).map_err(|_| BAD_STATE)?;
        let mut rows = statement.query([id]).map_err(|_| BAD_STATE)?;
        let mut fields = Vec::new();
        let mut total = 0usize;
        while let Some(row) = rows.next().map_err(|_| BAD_STATE)? {
            let kind: u8 = row.get(0).map_err(|_| BAD_STATE)?;
            let name: u8 = row.get(1).map_err(|_| BAD_STATE)?;
            let size: i64 = row.get(2).map_err(|_| BAD_STATE)?;
            if !(1..=8).contains(&kind)
                || !(0..=65535).contains(&size)
                || fields.len() >= MAX_FIELDS
                || total + size as usize > MAX_DATA
            {
                return Err(BAD_STATE);
            }
            let data: Vec<u8> = row.get(3).map_err(|_| BAD_STATE)?;
            total += data.len();
            fields.push(Field { kind, name, data });
        }
        Ok(Some(fields))
    }

    fn insert_fields(
        transaction: &rusqlite::Transaction<'_>,
        id: u16,
        fields: &[Field],
    ) -> Result<(), u32> {
        for (ordinal, field) in fields.iter().enumerate() {
            transaction
                .execute(
                    "INSERT INTO fields(record_id,ordinal,kind,name,data) VALUES(?1,?2,?3,?4,?5)",
                    params![id, ordinal as u32, field.kind, field.name, &field.data],
                )
                .map_err(|_| BAD_STATE)?;
        }
        Ok(())
    }

    fn create(&mut self, fields: &[Field]) -> Result<u16, u32> {
        let transaction = self.connection.transaction().map_err(|_| BAD_STATE)?;
        let id: u32 = transaction
            .query_row("SELECT next_id FROM zeebx_legacy_db", [], |row| row.get(0))
            .map_err(|_| BAD_STATE)?;
        let used: u64 = transaction
            .query_row(
                "SELECT coalesce(sum(length(data)),0) FROM fields",
                [],
                |row| row.get(0),
            )
            .map_err(|_| BAD_STATE)?;
        if id >= 0xffff
            || used + fields.iter().map(|f| f.data.len() as u64).sum::<u64>() > MAX_DATA as u64
        {
            return Err(NO_MEMORY);
        }
        transaction
            .execute("INSERT INTO records(id) VALUES(?1)", [id])
            .map_err(|_| BAD_STATE)?;
        Self::insert_fields(&transaction, id as u16, fields)?;
        transaction
            .execute("UPDATE zeebx_legacy_db SET next_id=?1", [id + 1])
            .map_err(|_| BAD_STATE)?;
        transaction.commit().map_err(|_| BAD_STATE)?;
        Ok(id as u16)
    }

    fn update(&mut self, id: u16, fields: &[Field]) -> Result<(), u32> {
        if !self.exists(id)? {
            return Err(BAD_RECORD);
        }
        let transaction = self.connection.transaction().map_err(|_| BAD_STATE)?;
        let other: u64 = transaction
            .query_row(
                "SELECT coalesce(sum(length(data)),0) FROM fields WHERE record_id<>?1",
                [id],
                |row| row.get(0),
            )
            .map_err(|_| BAD_STATE)?;
        if other + fields.iter().map(|f| f.data.len() as u64).sum::<u64>() > MAX_DATA as u64 {
            return Err(NO_MEMORY);
        }
        transaction
            .execute("DELETE FROM fields WHERE record_id=?1", [id])
            .map_err(|_| BAD_STATE)?;
        Self::insert_fields(&transaction, id, fields)?;
        transaction.commit().map_err(|_| BAD_STATE)
    }

    fn remove_record(&mut self, id: u16) -> Result<(), u32> {
        if !self.exists(id)? {
            return Err(BAD_RECORD);
        }
        let transaction = self.connection.transaction().map_err(|_| BAD_STATE)?;
        transaction
            .execute("DELETE FROM fields WHERE record_id=?1", [id])
            .map_err(|_| BAD_STATE)?;
        transaction
            .execute("DELETE FROM records WHERE id=?1", [id])
            .map_err(|_| BAD_STATE)?;
        transaction.commit().map_err(|_| BAD_STATE)
    }
}

impl<C: CpuBackend> Machine<C> {
    fn open_legacy_database(&self, name: &str, create: bool) -> Result<LegacyDatabase, String> {
        if name.is_empty() || name.len() > 4096 {
            return Err("nome inválido".into());
        }
        let source = self
            .vfs
            .open_target(name, OpenIntent::Read)
            .map(|target| target.path);
        if source.is_none() && !create {
            return Err("banco ausente".into());
        }
        let target = self
            .vfs
            .open_target(name, OpenIntent::ReadWrite)
            .ok_or_else(|| "caminho recusado".to_string())?
            .path;
        let mut initialize = source.is_none();
        let mut imported = None;
        if let Some(source) = &source {
            let metadata = std::fs::metadata(source).map_err(|e| e.to_string())?;
            if !metadata.is_file() || metadata.len() > 64 * 1024 * 1024 {
                return Err("arquivo de banco fora dos limites".into());
            }
            let raw = std::fs::read(source).map_err(|e| e.to_string())?;
            let seed = raw == EMPTY_SEED;
            if seed {
                // Converter o seed exige um alvo separado no overlay. Sem perfil de saves,
                // ReadWrite pode devolver o próprio pacote; nem bCreate autoriza truncá-lo.
                if source == &target && !self.vfs.is_overlay_path(&target) {
                    return Err("seed nativo precisa de overlay gravável".into());
                }
                // O índice do único formato nativo reconhecido também deve estar vazio.
                let mut index = source.as_os_str().to_os_string();
                index.push(".idx");
                if std::fs::metadata(std::path::PathBuf::from(index)).is_ok_and(|m| m.len() != 0) {
                    return Err("índice nativo desconhecido".into());
                }
                initialize = true;
            } else if !raw.starts_with(b"SQLite format 3\0") {
                let seed = native_seed(&raw)
                    .ok_or_else(|| "formato de banco nativo ainda não suportado".to_string())?;
                if source == &target {
                    return Err("importação nativa exige cópia separada no overlay".into());
                }
                imported = Some(seed);
                initialize = true;
            } else {
                // Confere antes de copiar ou abrir para escrita: um arquivo desconhecido não
                // vira um banco vazio só porque bCreate foi passado como TRUE.
                let check = Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY)
                    .map_err(|e| e.to_string())?;
                let (version, next): (u32, u32) = check
                    .query_row("SELECT version,next_id FROM zeebx_legacy_db", [], |row| {
                        Ok((row.get(0)?, row.get(1)?))
                    })
                    .map_err(|_| "formato de banco nativo ainda não suportado".to_string())?;
                if version != 1 || next > 0xffff {
                    return Err("versão de banco inválida".into());
                }
            }
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        if initialize {
            initialize_database(&target, imported)?;
        } else if let Some(source) = source.filter(|source| source != &target) {
            std::fs::copy(source, &target).map_err(|e| e.to_string())?;
        }
        let connection = Connection::open_with_flags(&target, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(|e| e.to_string())?;
        Ok(LegacyDatabase {
            name: name.to_string(),
            cursor: None,
            connection,
        })
    }

    fn read_legacy_fields(&self, pointer: u32, count: u32) -> Result<Vec<Field>, u32> {
        if (count as i32) < 0 || count as usize > MAX_FIELDS {
            return Err(BAD_FIELD_COUNT);
        }
        if pointer == 0 {
            return Err(NULL_FIELD);
        }
        let mut fields = Vec::new();
        let mut total = 0usize;
        for index in 0..count {
            let at = pointer.checked_add(index * 8).ok_or(NULL_FIELD)?;
            let header = self.read_bytes(at, 8).map_err(|_| NULL_FIELD)?;
            let kind = header[0];
            let size = match kind {
                1 => 1,
                2 => 2,
                3 => 4,
                4..=8 => u16::from_le_bytes([header[2], header[3]]) as u32,
                _ => return Err(NULL_FIELD),
            };
            total += size as usize;
            if total > MAX_DATA {
                return Err(NO_MEMORY);
            }
            let data_pointer = u32::from_le_bytes(header[4..8].try_into().unwrap());
            if data_pointer == 0 && size != 0 {
                return Err(NULL_FIELD);
            }
            let data = self
                .read_bytes(data_pointer, size)
                .map_err(|_| NULL_FIELD)?;
            fields.push(Field {
                kind,
                name: header[1],
                data,
            });
        }
        Ok(fields)
    }

    fn new_legacy_record(&mut self, database: u32, id: u16) -> Result<u32, CpuError> {
        let object = self.new_object(Interface::DBRecordLegacy)?;
        if object != 0 {
            self.objects.add_ref(database);
            self.legacy_records.insert(
                object,
                LegacyRecord {
                    database,
                    id,
                    cursor: -1,
                    buffers: BTreeMap::new(),
                },
            );
        }
        Ok(object)
    }

    fn release_legacy_database(&mut self, object: u32) -> u32 {
        let count = self.objects.release(object);
        if count == 0 {
            self.legacy_databases.remove(&object);
        }
        count
    }

    fn current_legacy_field(&self, object: u32) -> Option<Field> {
        let record = self.legacy_records.get(&object)?;
        let fields = self
            .legacy_databases
            .get(&record.database)?
            .fields(record.id)
            .ok()??;
        fields.into_iter().nth(usize::try_from(record.cursor).ok()?)
    }

    fn legacy_field_buffer(&mut self, object: u32, bytes: &[u8]) -> Result<u32, CpuError> {
        let index = self.legacy_records[&object].cursor as u32;
        let existing = self.legacy_records[&object].buffers.get(&index).copied();
        let capacity = bytes.len().max(1) as u32;
        let pointer = match existing {
            Some((pointer, size)) if size >= capacity => pointer,
            _ => {
                let pointer = self.malloc(capacity)?;
                if pointer == 0 {
                    return Ok(0);
                }
                if let Some((old, _)) = existing {
                    self.heap.free(old);
                }
                self.legacy_records
                    .get_mut(&object)
                    .unwrap()
                    .buffers
                    .insert(index, (pointer, capacity));
                pointer
            }
        };
        if !bytes.is_empty() {
            self.cpu.write_mem(pointer, bytes)?;
        }
        Ok(pointer)
    }

    fn legacy_field_metadata(
        &mut self,
        name: u32,
        kind: u32,
        length: u32,
        field: &Field,
    ) -> Result<(), CpuError> {
        // AEEDBFieldName e AEEDBFieldType são int8 no ARM; uint32 só no simulador Win32.
        if name != 0 {
            self.cpu.write_mem(name, &[field.name])?;
        }
        if kind != 0 {
            self.cpu.write_mem(kind, &[field.kind])?;
        }
        if length != 0 {
            self.cpu
                .write_mem(length, &(field.data.len() as u16).to_le_bytes())?;
        }
        Ok(())
    }

    pub(super) fn db_legacy_call(
        &mut self,
        interface: Interface,
        slot: u32,
    ) -> Result<Option<u32>, CpuError> {
        let this = self.cpu.read_reg(Reg::R0);
        let (a1, a2, a3) = (
            self.cpu.read_reg(Reg::R1),
            self.cpu.read_reg(Reg::R2),
            self.cpu.read_reg(Reg::R3),
        );
        let result = match (interface, slot) {
            (_, 0) => self.objects.add_ref(this),
            (Interface::DBMgrLegacy, 1) => self.objects.release(this),
            (Interface::DatabaseLegacy, 1) => self.release_legacy_database(this),
            (Interface::DBRecordLegacy, 1) => {
                let count = self.objects.release(this);
                if count == 0
                    && let Some(record) = self.legacy_records.remove(&this)
                {
                    for (pointer, _) in record.buffers.values() {
                        self.heap.free(*pointer);
                    }
                    self.release_legacy_database(record.database);
                }
                count
            }
            (Interface::DBMgrLegacy, 2) => {
                let name = self.cpu.read_cstring(a1, MAX_STRING);
                // dwMinSize (r3) e wMinRecs (stack[0]) reservam capacidade; não são registros.
                let _minimum_records = self.stack_arg(0)? as u16;
                match self.open_legacy_database(&name, a2 != 0) {
                    Ok(database) => {
                        let object = self.new_object(Interface::DatabaseLegacy)?;
                        if object != 0 {
                            self.legacy_databases.insert(object, database);
                        }
                        object
                    }
                    Err(reason) => {
                        if self.fs_log.len() >= MAX_FS_LOG {
                            self.fs_log.pop_front();
                        }
                        self.fs_log
                            .push_back(format!("IDBMgr::OpenDatabase {name:?} -> 0 ({reason})"));
                        0
                    }
                }
            }
            (Interface::DBMgrLegacy, 3) => {
                let name = self.cpu.read_cstring(a1, MAX_STRING);
                let path = self
                    .vfs
                    .open_target(&name, OpenIntent::Read)
                    .map(|target| target.path);
                // Sem whiteouts no VFS, remover só a cópia faria reaparecer o seed do pacote.
                // Recusar esse caso é preferível a prometer que o banco foi removido.
                let packaged = self
                    .vfs
                    .resolve_new(&name)
                    .is_some_and(|p| p.starts_with(self.vfs.root()) && p.exists());
                if packaged
                    || self
                        .legacy_databases
                        .values()
                        .any(|db| db.name.eq_ignore_ascii_case(&name))
                {
                    BAD_STATE
                } else {
                    path.and_then(|path| std::fs::remove_file(path).ok())
                        .map_or(BAD_FILENAME, |_| SUCCESS)
                }
            }
            (Interface::DBMgrLegacy, 4) => SUCCESS, // cache é uma otimização opcional no SDK.
            (Interface::DatabaseLegacy, 2) => self
                .legacy_databases
                .get(&this)
                .and_then(|db| {
                    db.connection
                        .query_row("SELECT count(*) FROM records", [], |row| {
                            row.get::<_, u32>(0)
                        })
                        .ok()
                })
                .unwrap_or(0),
            (Interface::DatabaseLegacy, 3) => {
                if let Some(db) = self.legacy_databases.get_mut(&this) {
                    db.cursor = None;
                }
                SUCCESS
            }
            (Interface::DatabaseLegacy, 4) => {
                let id = self.legacy_databases.get(&this).and_then(|db| {
                    db.connection
                        .query_row(
                            "SELECT id FROM records WHERE id>?1 ORDER BY id LIMIT 1",
                            [db.cursor.map(i64::from).unwrap_or(-1)],
                            |row| row.get::<_, u16>(0),
                        )
                        .optional()
                        .ok()
                        .flatten()
                });
                match id {
                    Some(id) => {
                        let object = self.new_legacy_record(this, id)?;
                        if object != 0 {
                            self.legacy_databases.get_mut(&this).unwrap().cursor = Some(id);
                        }
                        object
                    }
                    None => 0,
                }
            }
            (Interface::DatabaseLegacy, 5) => {
                if self
                    .legacy_databases
                    .get(&this)
                    .is_some_and(|db| db.exists(a1 as u16).unwrap_or(false))
                {
                    self.new_legacy_record(this, a1 as u16)?
                } else {
                    0
                }
            }
            (Interface::DatabaseLegacy, 6) => {
                let fields = match self.read_legacy_fields(a1, a2) {
                    Ok(fields) => fields,
                    Err(_) => return Ok(Some(0)),
                };
                let Some(database) = self.legacy_databases.get_mut(&this) else {
                    return Ok(Some(0));
                };
                match database.create(&fields) {
                    Ok(id) => self.new_legacy_record(this, id)?,
                    Err(_) => 0,
                }
            }
            (Interface::DBRecordLegacy, 2) => {
                // Reset descreve o primeiro campo como atual, mas NextField também documenta
                // o avanço a partir de -1. Usamos essa leitura para Reset -> NextField do Chess.
                if let Some(record) = self.legacy_records.get_mut(&this) {
                    record.cursor = -1;
                }
                SUCCESS
            }
            (Interface::DBRecordLegacy, 3) => {
                let fields = match self.read_legacy_fields(a1, a2) {
                    Ok(fields) => fields,
                    Err(code) => return Ok(Some(code)),
                };
                let Some(record) = self.legacy_records.get(&this) else {
                    return Ok(Some(BAD_RECORD));
                };
                let (database, id) = (record.database, record.id);
                let result = self
                    .legacy_databases
                    .get_mut(&database)
                    .ok_or(BAD_STATE)
                    .and_then(|db| db.update(id, &fields));
                if result.is_ok() {
                    let record = self.legacy_records.get_mut(&this).unwrap();
                    for (pointer, _) in std::mem::take(&mut record.buffers).values() {
                        self.heap.free(*pointer);
                    }
                    record.cursor = -1;
                }
                result.err().unwrap_or(SUCCESS)
            }
            (Interface::DBRecordLegacy, 4) => {
                let Some(record) = self.legacy_records.get(&this) else {
                    return Ok(Some(BAD_RECORD));
                };
                self.legacy_databases
                    .get_mut(&record.database)
                    .ok_or(BAD_STATE)
                    .and_then(|db| db.remove_record(record.id))
                    .err()
                    .unwrap_or(SUCCESS)
            }
            (Interface::DBRecordLegacy, 5) => self
                .legacy_records
                .get(&this)
                .filter(|record| {
                    self.legacy_databases
                        .get(&record.database)
                        .is_some_and(|db| db.exists(record.id).unwrap_or(false))
                })
                .map_or(0xffff, |record| u32::from(record.id)),
            (Interface::DBRecordLegacy, 6) => {
                if let Some(record) = self.legacy_records.get_mut(&this) {
                    record.cursor = record.cursor.saturating_add(1).min(MAX_FIELDS as i32);
                }
                match self.current_legacy_field(this) {
                    Some(field) => {
                        self.legacy_field_metadata(a1, 0, a2, &field)?;
                        u32::from(field.kind)
                    }
                    None => 0,
                }
            }
            (Interface::DBRecordLegacy, 7) => match self.current_legacy_field(this) {
                Some(field) => {
                    let pointer = self.legacy_field_buffer(this, &field.data)?;
                    self.legacy_field_metadata(a1, a2, a3, &field)?;
                    pointer
                }
                None => 0,
            },
            (Interface::DBRecordLegacy, 8 | 9) => match self.current_legacy_field(this) {
                Some(field)
                    if (slot == 8 && field.kind == 2 && field.data.len() == 2)
                        || (slot == 9 && field.kind == 3 && field.data.len() == 4) =>
                {
                    if a1 == 0 {
                        0
                    } else {
                        self.cpu.write_mem(a1, &field.data)?;
                        TRUE
                    }
                }
                _ => 0,
            },
            (Interface::DBRecordLegacy, 10) => match self.current_legacy_field(this) {
                Some(field) if field.kind == 4 => self.legacy_field_buffer(this, &field.data)?,
                _ => 0,
            },
            _ => return Ok(None),
        };
        Ok(Some(result))
    }

    pub(super) fn grava_db_legado(&self, sections: &mut crate::save_state::Secoes) {
        let snapshot = Snapshot {
            version: 1,
            records: self.legacy_records.clone(),
            databases: self
                .legacy_databases
                .iter()
                .map(|(id, db)| (*id, db.name.clone(), db.cursor))
                .collect(),
        };
        sections.poe(
            "db.legacy",
            serde_json::to_vec(&snapshot).expect("estado do banco só contém números e texto"),
        );
    }

    pub(super) fn restaura_db_legado(
        &mut self,
        reader: &crate::save_state::Leitor<'_>,
    ) -> Result<(), crate::save_state::Erro> {
        let error = |reason: String| crate::save_state::Erro::Secao {
            nome: "db.legacy".into(),
            motivo: reason,
        };
        let Some(bytes) = reader.secao("db.legacy") else {
            self.legacy_records.clear();
            self.legacy_databases.clear();
            return Ok(());
        };
        if bytes.len() > MAX_DATA {
            return Err(error("metadados do banco fora dos limites".into()));
        }
        let snapshot: Snapshot = serde_json::from_slice(bytes).map_err(|e| error(e.to_string()))?;
        if snapshot.version != 1 {
            return Err(error("versão de banco desconhecida".into()));
        }
        if snapshot
            .databases
            .len()
            .saturating_add(snapshot.records.len())
            > self.objects.live_count()
        {
            return Err(error("mais handles de banco do que objetos vivos".into()));
        }
        let mut databases = BTreeMap::new();
        for (id, name, cursor) in snapshot.databases {
            if self.objects.kind_of(id) != Some(Interface::DatabaseLegacy) {
                return Err(error("objeto não é um banco legado".into()));
            }
            let mut db = self.open_legacy_database(&name, false).map_err(&error)?;
            if cursor.is_some_and(|id| !db.issued(id).unwrap_or(false)) {
                return Err(error("cursor aponta para ID nunca emitido".into()));
            }
            db.cursor = cursor;
            if databases.insert(id, db).is_some() {
                return Err(error("banco repetido".into()));
            }
        }
        let mut buffers = std::collections::BTreeSet::new();
        for (id, record) in &snapshot.records {
            if self.objects.kind_of(*id) != Some(Interface::DBRecordLegacy)
                || !databases
                    .get(&record.database)
                    .is_some_and(|db| db.issued(record.id).unwrap_or(false))
                || record.cursor < -1
                || record.cursor > MAX_FIELDS as i32
            {
                return Err(error("registro ou cursor inválido".into()));
            }
            for (index, (pointer, capacity)) in &record.buffers {
                if *index >= MAX_FIELDS as u32
                    || *capacity == 0
                    || *capacity > 65535
                    || self
                        .heap
                        .size_of(*pointer)
                        .is_none_or(|size| size < *capacity)
                    || !buffers.insert(*pointer)
                {
                    return Err(error(
                        "buffer de campo sem alocação ou com posse repetida".into(),
                    ));
                }
            }
        }
        self.legacy_databases = databases;
        self.legacy_records = snapshot.records;
        Ok(())
    }
}
