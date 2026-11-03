//! Registro MIME do BREW: apenas provedores disponíveis e seleções explícitas.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

const REG_BAD_CLASS: u32 = 10;
const REG_BAD_PARAM: u32 = 14;
const REG_ALREADY: u32 = 26;
const MAX_ENTRIES: usize = 512;
const MAX_MIME: usize = 1024;
const MAX_PROFILE: usize = 1024 * 1024;
const VIEW: u32 = 0x0100_4000;
const APPLET: u32 = 0x0101_0100;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    base: u32,
    mime: String,
    handler: u32,
}
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RegistryState {
    loaded: bool,
    invalid_profile: bool,
    entries: Vec<Entry>,
    strings: BTreeMap<String, u32>,
}
fn valid_mime(mime: &str) -> bool {
    !mime.is_empty() && mime.len() <= MAX_MIME && !mime.as_bytes().contains(&0)
}
fn validate_entries(entries: &[Entry]) -> bool {
    if entries.len() > MAX_ENTRIES {
        return false;
    }
    let mut keys = BTreeSet::new();
    entries
        .iter()
        .all(|e| valid_mime(&e.mime) && keys.insert((e.base, e.mime.to_ascii_lowercase())))
}
fn native_entries() -> Vec<Entry> {
    let images = [
        "image/png",
        "image/jpeg",
        "image/gif",
        "image/bmp",
        "image/x-ms-bmp",
    ];
    let audio = [
        "audio/mid",
        "audio/midi",
        "audio/mpeg",
        "audio/mp3",
        "audio/wav",
        "audio/x-wav",
        "audio/vnd.qcelp",
        "audio/qcp",
        "application/cmf",
        "audio/pmd",
    ];
    let mut entries = Vec::new();
    for mime in images {
        for base in [0, VIEW] {
            entries.push(Entry {
                base,
                mime: mime.into(),
                handler: handler_for(mime),
            });
        }
    }
    for mime in audio {
        entries.push(Entry {
            base: AEECLSID_MEDIA,
            mime: mime.into(),
            handler: handler_for(mime),
        });
        for base in [1, AEECLSID_SOUNDPLAYER] {
            entries.push(Entry {
                base,
                mime: mime.into(),
                handler: AEECLSID_SOUNDPLAYER,
            });
        }
    }
    entries
}
impl RegistryState {
    pub(super) fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("estado do registro contém apenas dados JSON")
    }
    pub(super) fn decode(
        bytes: Option<&[u8]>,
        saved_heap: Option<&[u8]>,
    ) -> Result<Self, crate::save_state::Erro> {
        let fail = || crate::save_state::Erro::Secao {
            nome: "tab.registry".into(),
            motivo: "estado de registro inválido".into(),
        };
        let Some(bytes) = bytes else {
            return Ok(Self::default());
        };
        if bytes.len() > MAX_PROFILE * 2 {
            return Err(fail());
        }
        let state: Self = serde_json::from_slice(bytes).map_err(|_| fail())?;
        if !validate_entries(&state.entries)
            || state.strings.len() > MAX_ENTRIES + 32
            || (!state.loaded
                && (!state.entries.is_empty()
                    || !state.strings.is_empty()
                    || state.invalid_profile))
            || state.strings.iter().any(|(mime, ptr)| {
                !valid_mime(mime)
                    || *ptr < loader::HEAP_BASE
                    || ptr
                        .checked_add(mime.len() as u32 + 1)
                        .is_none_or(|end| end > loader::HEAP_BASE + loader::HEAP_SIZE as u32)
            })
        {
            return Err(fail());
        }
        // A faixa mapeada do heap é maior que o conteúdo gravado. Conferir os
        // bytes do próprio snapshot evita restaurar ponteiros para a cauda não
        // salva ou atribuir a um MIME o texto de outro registro.
        for (mime, ptr) in &state.strings {
            let start = (*ptr - loader::HEAP_BASE) as usize;
            let end = start + mime.len() + 1;
            let Some(text) = saved_heap.and_then(|heap| heap.get(start..end)) else {
                return Err(fail());
            };
            if &text[..mime.len()] != mime.as_bytes() || text[mime.len()] != 0 {
                return Err(fail());
            }
        }
        Ok(state)
    }
}
impl<C: CpuBackend> Machine<C> {
    pub(super) fn registry_load(&mut self) {
        if self.registry.loaded {
            return;
        }
        self.registry.loaded = true;
        let path = self.vfs.profile_file("brew-registry", "handlers.json");
        if !path.exists() {
            return;
        }
        let entries = std::fs::metadata(&path)
            .ok()
            .filter(|m| m.is_file() && m.len() <= MAX_PROFILE as u64)
            .and_then(|_| std::fs::read(path).ok())
            .and_then(|b| serde_json::from_slice::<Vec<Entry>>(&b).ok())
            .filter(|entries| validate_entries(entries));
        if let Some(entries) = entries {
            self.registry.entries = entries;
        } else {
            self.registry.invalid_profile = true;
            self.assumptions
                .insert("o perfil de handlers MIME é inválido; seleções não foram substituídas");
        }
    }
    fn registry_supports(&self, base: u32, class: u32) -> bool {
        match class {
            AEECLSID_PNG | AEECLSID_JPEG | AEECLSID_GIF | AEECLSID_BMP => matches!(base, 0 | VIEW),
            AEECLSID_SOUNDPLAYER => base == 1 || base == AEECLSID_SOUNDPLAYER,
            _ if matches!(
                class,
                AEECLSID_MEDIA
                    | AEECLSID_MEDIAMIDI
                    | AEECLSID_MEDIAMP3
                    | AEECLSID_MEDIAMIDIOUTMSG
                    | AEECLSID_MEDIAADPCM
                    | AEECLSID_MEDIAPCM
            ) || AEECLSID_MEDIA_FAMILIA.contains(&class) =>
            {
                base == AEECLSID_MEDIA
            }
            _ if class != 0
                && (class == self.applet_class || self.installed_applets.contains(&class)) =>
            {
                base == 2 || base == APPLET
            }
            _ => false,
        }
    }
    fn registry_active(&self, base: u32, mime: &str) -> u32 {
        if let Some(e) = self
            .registry
            .entries
            .iter()
            .find(|e| e.base == base && e.mime.eq_ignore_ascii_case(&mime))
        {
            return if e.handler == 0 || self.registry_supports(base, e.handler) {
                e.handler
            } else {
                0
            };
        }
        native_entries()
            .into_iter()
            .find(|e| e.base == base && e.mime.eq_ignore_ascii_case(&mime))
            .map_or(0, |e| e.handler)
    }
    pub(super) fn registry_get_handler(&mut self, base: u32, mime_ptr: u32) -> u32 {
        self.registry_load();
        if mime_ptr == 0 {
            return 0;
        }
        let mime = self
            .cpu
            .read_cstring(mime_ptr, MAX_MIME + 1)
            .to_ascii_lowercase();
        if !valid_mime(&mime) {
            return 0;
        }
        self.registry_active(base, &mime)
    }
    fn registry_persist(&self, entries: &[Entry]) -> std::io::Result<()> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = self.vfs.profile_file("brew-registry", "handlers.json");
        let parent = path.parent().unwrap();
        std::fs::create_dir_all(parent)?;
        let next = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let temporary = parent.join(format!(".handlers-{}-{next}.tmp", std::process::id()));
        let result = (|| {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(&serde_json::to_vec(entries)?)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&temporary, &path)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(temporary);
        }
        result
    }
    pub(super) fn registry_set_handler(
        &mut self,
        base: u32,
        mime_ptr: u32,
        class: u32,
        replace: bool,
    ) -> u32 {
        self.registry_load();
        if mime_ptr == 0 {
            return REG_BAD_PARAM;
        }
        let input = self.cpu.read_cstring(mime_ptr, MAX_MIME + 1);
        let mimes: Vec<_> = if replace {
            vec![input]
        } else {
            input.split(',').map(|s| s.trim().to_string()).collect()
        };
        if mimes.is_empty() || mimes.iter().any(|s| !valid_mime(s)) {
            return REG_BAD_PARAM;
        }
        if (class == 0 && replace) || (class != 0 && !self.registry_supports(base, class)) {
            return REG_BAD_CLASS;
        }
        if mimes.iter().any(|mime| {
            let old = self.registry_active(base, mime);
            if replace {
                old == class
            } else {
                class != 0 && old != 0
            }
        }) {
            return REG_ALREADY;
        }
        if self.registry.invalid_profile {
            return EFAILED;
        }
        let mut entries = self.registry.entries.clone();
        for mime in mimes {
            if let Some(e) = entries
                .iter_mut()
                .find(|e| e.base == base && e.mime.eq_ignore_ascii_case(&mime))
            {
                e.handler = class;
            } else {
                entries.push(Entry {
                    base,
                    mime,
                    handler: class,
                });
            }
        }
        if !validate_entries(&entries) {
            return REG_BAD_PARAM;
        }
        if self.registry_persist(&entries).is_err() {
            return EFAILED;
        }
        self.registry.entries = entries;
        SUCCESS
    }
    fn registry_string(&mut self, mime: &str) -> Result<u32, CpuError> {
        if let Some(ptr) = self.registry.strings.get(mime) {
            return Ok(*ptr);
        }
        if self.registry.strings.len() >= MAX_ENTRIES + 32 {
            return Err(CpuError("cache de tipos MIME esgotado".into()));
        }
        let ptr = self
            .heap
            .alloc(mime.len() as u32 + 1)
            .ok_or_else(|| CpuError("memória insuficiente para tipo MIME".into()))?;
        self.cpu.write_mem(ptr, &[mime.as_bytes(), &[0]].concat())?;
        self.registry.strings.insert(mime.into(), ptr);
        Ok(ptr)
    }
    fn registry_enum(&mut self, function: u32, context: u32, all: bool) -> Result<(), CpuError> {
        self.registry_load();
        if function == 0 {
            return Ok(());
        }
        if self.nesting >= MAX_NESTING {
            return Err(CpuError("enumeração de registro aninhada demais".into()));
        }
        // Preservar a grafia pública: Celltop usa strbegins, sensível à caixa.
        // A chave de comparação continua canônica para não duplicar tipos MIME.
        let mut records: BTreeMap<(u32, String, u32), (String, bool)> = BTreeMap::new();
        for e in native_entries()
            .into_iter()
            .chain(self.registry.entries.clone())
        {
            if e.handler == 0 || !self.registry_supports(e.base, e.handler) {
                continue;
            }
            let active = self.registry_active(e.base, &e.mime) == e.handler;
            if all || active {
                records.insert(
                    (e.base, e.mime.to_ascii_lowercase(), e.handler),
                    (e.mime, active),
                );
            }
        }
        let frame = self
            .heap
            .alloc(16)
            .ok_or_else(|| CpuError("memória insuficiente para enumeração MIME".into()))?;
        let result = (|| {
            for ((base, _, handler), (mime, active)) in records {
                let text = self.registry_string(&mime)?;
                for (at, value) in [(0, base), (4, handler), (8, text), (12, u32::from(active))] {
                    self.cpu.write_u32(frame + at, value)?;
                }
                if self.serial.is_some() {
                    self.registra_serial(format!(
                        "<registro tipo={base:#x} mime={mime} classe={handler:#x} ativo={active}>"
                    ));
                }
                self.nesting += 1;
                let outer_budget = self.orcamento;
                let outcome =
                    self.call_guest_aninhado(function, [context, frame, 0, 0], QSORT_BUDGET);
                self.orcamento = outer_budget;
                self.nesting -= 1;
                match outcome? {
                    Outcome::Returned { code: 0 } => break,
                    Outcome::Returned { .. } => {}
                    _ => return Err(CpuError("callback da enumeração MIME não retornou".into())),
                }
            }
            Ok(())
        })();
        self.heap.free(frame);
        result
    }
    pub(super) fn registry_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let this = self.arg(0);
        let (a, b, c) = (self.arg(1), self.arg(2), self.arg(3));
        let result = match slot {
            0 => self.objects.add_ref(this),
            1 => self.objects.release(this),
            2 => self.registry_get_handler(a, b),
            3 => self.registry_set_handler(a, b, c, true),
            4 => {
                self.registry_enum(a, b, c != 0)?;
                SUCCESS
            }
            _ => return Ok(None),
        };
        Ok(Some(result))
    }
}
