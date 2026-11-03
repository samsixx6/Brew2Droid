//! Port guardado do consumidor KH para o provedor Swerve2.1 auditado.
//! A fábrica, os objetos, o parser M3G e o rasterizador continuam sendo ARM do provedor.
use super::*;
use sha2::{Digest, Sha256};

const KH_MOD: &str = "10f18a0c8e77715459c7eefbb276b8054fe7d8afdb5f69a105aa370c1193b804";
const KH_MIF: &str = "624b4d689c21b25ed9aba934a05ba0a2c907c34d171cf734188f95b7dbbfa4db";
const PROVIDER_CLASS: u32 = 0x0102_bbfc;
const PROVIDER_SIZE: u32 = 273188;
const PROVIDER_HASH: [u8; 32] = [
    0x1e, 0x76, 0x41, 0x2e, 0x15, 0x8b, 0xc3, 0x94, 0x8b, 0xf6, 0xad, 0x17, 0x3b, 0x18, 0x13, 0x68,
    0xe0, 0xf0, 0xd5, 0x54, 0xd6, 0xb0, 0xe1, 0x48, 0x8a, 0x29, 0x40, 0x2d, 0x96, 0xb0, 0xcd, 0xab,
];
const START: u32 = 0x0002_9d58;
const END: u32 = 0x0003_4ca4;
const LENGTH: u32 = END - START;
const WORDS: [(u32, u32, u32); 3] = [
    (0x0002_a028, 0x0101_64ea, 0x0102_bbfc),
    (0x0002_9eb4, 0xe59c_c024, 0xe59c_c020),
    (0x0003_4c9c, 0xe592_2048, 0xe592_2044),
];
// O caller da fábrica também precisa continuar consumindo o literal verificado.
const CONSUMER: [(u32, u32, [u8; 32]); 4] = [
    (
        0x29d58,
        36,
        [
            0x9b, 0x63, 0xc0, 0xd2, 0xa2, 0x6d, 0xa7, 0xb5, 0xae, 0x87, 0x8f, 0xb4, 0xfd, 0x79,
            0x22, 0x11, 0xca, 0xdf, 0xa0, 0x65, 0x1a, 0x9f, 0x0d, 0x63, 0xfb, 0xfb, 0x57, 0xd7,
            0xb9, 0x1f, 0x81, 0xda,
        ],
    ),
    (
        0x2a020,
        24,
        [
            0x22, 0x68, 0x4f, 0x72, 0x97, 0x0f, 0x45, 0xad, 0xb9, 0x75, 0x6d, 0x40, 0x1d, 0x6b,
            0x58, 0x2d, 0x32, 0x14, 0x0d, 0xde, 0x19, 0x69, 0x17, 0x26, 0x56, 0x8a, 0xd2, 0x29,
            0x49, 0xe0, 0xa4, 0x3a,
        ],
    ),
    (
        0x29e80,
        72,
        [
            0xd8, 0x92, 0x91, 0xe8, 0x63, 0xbe, 0x15, 0x61, 0xdb, 0xc5, 0x17, 0xc7, 0xd3, 0x54,
            0xdf, 0x94, 0x61, 0x18, 0x40, 0x5d, 0xd4, 0xb0, 0xb2, 0x11, 0xfc, 0x8d, 0x7a, 0xaa,
            0x09, 0x6d, 0x22, 0xdb,
        ],
    ),
    (
        0x34c90,
        20,
        [
            0xfe, 0x04, 0x38, 0x28, 0x11, 0x24, 0x8b, 0xd7, 0x6b, 0x03, 0xdb, 0xf8, 0x0e, 0xe7,
            0x3a, 0x79, 0x85, 0x66, 0x45, 0x32, 0x7d, 0x30, 0xad, 0xb0, 0x0b, 0x42, 0x6f, 0x1f,
            0xd8, 0xc8, 0x9c, 0xe0,
        ],
    ),
];
// Apenas código imutável, não .data/BSS inicializados legitimamente pelo guest.
// Os deslocamentos são relativos ao início real da extensão, não à base de disassembly0x10000.
const PROVIDER_CODE: [(u32, u32, [u8; 32]); 5] = [
    (
        0x2894,
        52,
        [
            0xe8, 0xb3, 0x61, 0xc5, 0xd2, 0x34, 0x91, 0x24, 0xa2, 0x26, 0xe7, 0xf7, 0xd0, 0xc0,
            0x3f, 0x65, 0x28, 0x6c, 0x94, 0xf0, 0x50, 0x1c, 0x3e, 0x3b, 0x88, 0xb4, 0xaa, 0xab,
            0xf5, 0xc8, 0x1b, 0x75,
        ],
    ),
    (
        0x2479c,
        476,
        [
            0xb2, 0x9a, 0x26, 0x5e, 0x60, 0x08, 0x19, 0x7a, 0xf4, 0x8b, 0x58, 0xc3, 0x37, 0x1b,
            0x76, 0x60, 0xf8, 0x21, 0x3c, 0xe7, 0x1a, 0xe3, 0xa0, 0x76, 0x8d, 0x1a, 0x2a, 0x18,
            0xa0, 0x10, 0x9c, 0x92,
        ],
    ),
    (
        0x2ff38,
        128,
        [
            0xdd, 0x88, 0x3a, 0xb8, 0x00, 0x21, 0xda, 0x4e, 0xf1, 0x40, 0x03, 0x84, 0x9d, 0x1c,
            0x32, 0x27, 0x4a, 0xe2, 0x29, 0xbb, 0xdc, 0x8f, 0x8a, 0x22, 0xb4, 0x24, 0x17, 0x30,
            0x23, 0x40, 0xd1, 0x5a,
        ],
    ),
    (
        0x2fcf8,
        128,
        [
            0xc5, 0x24, 0xcd, 0xfb, 0xfb, 0x3e, 0x51, 0x4d, 0x8d, 0xed, 0xfd, 0x21, 0xad, 0xeb,
            0xf3, 0x78, 0x3d, 0x15, 0x75, 0x16, 0x5f, 0xa6, 0x0f, 0xef, 0x99, 0xcf, 0x38, 0x2a,
            0x2a, 0x27, 0xb4, 0x9b,
        ],
    ),
    (
        0x2feb8,
        128,
        [
            0x95, 0xb1, 0xcd, 0x8f, 0xe5, 0x3c, 0xd5, 0xef, 0xa8, 0x61, 0xe4, 0x3c, 0xb6, 0x98,
            0xb3, 0x5f, 0x1a, 0x85, 0xb0, 0x41, 0x59, 0x75, 0x4e, 0x5b, 0x8d, 0x0b, 0x4a, 0xc4,
            0x30, 0xc2, 0xcb, 0x5f,
        ],
    ),
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Version {
    Original,
    Adapted,
}
#[derive(Clone, Copy, Debug)]
struct Provider {
    entry: u32,
    region: &'static str,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct KingdomSwervePort {
    provider: Option<Provider>,
    active: bool,
}
impl KingdomSwervePort {
    pub(super) fn active(&self) -> bool {
        self.active
    }
}
fn hash_matches(bytes: &[u8], expected: &[u8; 32]) -> bool {
    let actual: [u8; 32] = Sha256::digest(bytes).into();
    actual == *expected
}
fn range(bytes: &[u8], base: u32, at: u32, len: u32) -> Option<&[u8]> {
    let offset = usize::try_from(at.checked_sub(base)?).ok()?;
    let end = offset.checked_add(usize::try_from(len).ok()?)?;
    bytes.get(offset..end)
}
fn provider_code_matches(bytes: &[u8], base: u32, entry: u32) -> bool {
    PROVIDER_CODE.iter().all(|&(offset, len, ref hash)| {
        entry
            .checked_add(offset)
            .and_then(|at| range(bytes, base, at, len))
            .is_some_and(|slice| hash_matches(slice, hash))
    })
}
// Valida todas as palavras e seus contextos antes de devolver qualquer cópia modificada.
// Uma combinação parcial não se torna uma versão aceita por normalização de hashes.
fn plan(bytes: &[u8], base: u32) -> Option<(Vec<u8>, Version)> {
    let window = range(bytes, base, START, LENGTH)?;
    let mut normalized = window.to_vec();
    let mut version = None;
    for &(at, original, adapted) in &WORDS {
        let pos = usize::try_from(at.checked_sub(START)?).ok()?;
        let word = u32::from_le_bytes(normalized.get(pos..pos + 4)?.try_into().ok()?);
        let this = if word == original {
            Version::Original
        } else if word == adapted {
            Version::Adapted
        } else {
            return None;
        };
        if version.is_some_and(|prior| prior != this) {
            return None;
        }
        version = Some(this);
        normalized[pos..pos + 4].copy_from_slice(&original.to_le_bytes());
    }
    if !CONSUMER.iter().all(|&(at, len, ref hash)| {
        range(&normalized, START, at, len).is_some_and(|slice| hash_matches(slice, hash))
    }) {
        return None;
    }
    for &(at, _, adapted) in &WORDS {
        let pos = usize::try_from(at - START).ok()?;
        normalized[pos..pos + 4].copy_from_slice(&adapted.to_le_bytes());
    }
    Some((normalized, version?))
}
fn stale_original(pc: u32, lr: u32, provider: Provider) -> bool {
    [pc & !1, lr & !1].into_iter().any(|at| {
        (0x29d58..0x29d7c).contains(&at)
            || (0x29e80..0x29ec8).contains(&at)
            || (0x34c90..0x34ca8).contains(&at)
            || (u64::from(provider.entry)..u64::from(provider.entry) + u64::from(PROVIDER_SIZE))
                .contains(&u64::from(at))
    })
}
impl<C: CpuBackend> Machine<C> {
    fn kingdom_provider(&self) -> Option<Provider> {
        // Shell despacha pela primeira extensão dona da classe. Uma cópia auditada
        // posterior não pode autorizar o port sobre um provedor anterior diferente.
        let extension = self
            .module
            .extensions
            .iter()
            .find(|extension| extension.classes.contains(&PROVIDER_CLASS))?;
        let bytes = self.module.mem.read(extension.entry, PROVIDER_SIZE).ok()?;
        if !hash_matches(bytes, &PROVIDER_HASH) {
            return None;
        }
        self.module
            .mem
            .regions()
            .iter()
            .find(|r| {
                r.base <= extension.entry
                    && u64::from(r.base) + r.bytes.len() as u64
                        >= u64::from(extension.entry) + u64::from(PROVIDER_SIZE)
            })
            .map(|r| Provider {
                entry: extension.entry,
                region: r.name,
            })
    }
    // None significa backend ainda não inicializado; Some(false), código presente mas divergente.
    fn kingdom_live_provider_code(&self, provider: Provider) -> Option<bool> {
        let mut readable = None;
        for &(offset, len, ref hash) in &PROVIDER_CODE {
            let at = provider.entry.checked_add(offset)?;
            let mut bytes = vec![0; len as usize];
            let present = self.cpu.read_mem(at, &mut bytes).is_ok();
            if let Some(was) = readable {
                if was != present {
                    return Some(false);
                }
            } else {
                readable = Some(present);
            }
            if present && !hash_matches(&bytes, hash) {
                return Some(false);
            }
        }
        readable.filter(|present| *present).map(|_| true)
    }
    fn kingdom_commit(&mut self, provider: Provider) -> bool {
        // Um único intervalo gravável cobre as três trocas. Não haverá escrita atravessando regiões.
        if !self.module.mem.regions().iter().any(|r| {
            r.name == "module"
                && r.writable
                && r.base <= START
                && u64::from(r.base) + r.bytes.len() as u64 >= u64::from(END)
        }) {
            return false;
        }
        let Ok(template) = self.module.mem.read(START, LENGTH) else {
            return false;
        };
        let Some((template_patch, _)) = plan(template, START) else {
            return false;
        };
        let mut live = vec![0; LENGTH as usize];
        let has_live = self.cpu.read_mem(START, &mut live).is_ok();
        let live_code = self.kingdom_live_provider_code(provider);
        if has_live != live_code.is_some() || live_code == Some(false) {
            return false;
        }
        let live_patch = if has_live {
            let Some((patch, version)) = plan(&live, START) else {
                return false;
            };
            if version == Version::Original
                && stale_original(
                    self.cpu.read_reg(Reg::Pc),
                    self.cpu.read_reg(Reg::Lr),
                    provider,
                )
            {
                return false;
            }
            Some(patch)
        } else {
            None
        };
        // write_mem é o caminho normal de invalidação JIT. A validação termina antes da primeira escrita.
        if let Some(patch) = live_patch.as_deref() {
            if self.cpu.write_mem(START, patch).is_err() {
                let _ = self.cpu.write_mem(START, &live);
                return false;
            }
        }
        // GuestMemory::write valida o intervalo inteiro antes de copiar; os limites acima já o provaram.
        if self.module.mem.write(START, &template_patch).is_err() {
            if has_live {
                let _ = self.cpu.write_mem(START, &live);
            }
            return false;
        }
        true
    }
    pub(super) fn apply_kingdom_swerve_compat(
        &mut self,
        module: &str,
        mif: &str,
    ) -> Option<KingdomSwervePort> {
        if !module.eq_ignore_ascii_case(KH_MOD) || !mif.eq_ignore_ascii_case(KH_MIF) {
            return None;
        }
        let provider = self.kingdom_provider();
        let active = provider.is_some_and(|p| self.kingdom_commit(p));
        Some(KingdomSwervePort { provider, active })
    }
    pub(super) fn prepare_kingdom_swerve_restore(
        &self,
        name: &str,
        base: u32,
        bytes: &mut [u8],
        regs: &[u32],
    ) -> Result<(), crate::save_state::Erro> {
        let Some(port) = self.kingdom_swerve_compat else {
            return Ok(());
        };
        let fail = || crate::save_state::Erro::Secao {
            nome: format!("mem.{name}"),
            motivo: "o consumidor KH ou o provedor Swerve não corresponde ao contrato verificado"
                .into(),
        };
        if name == "module" {
            let (patch, version) = plan(bytes, base).ok_or_else(fail)?;
            // Mesmo a seleção sem port conserva a recusa de um save já convertido sem seu provedor.
            if !port.active {
                return if version == Version::Original {
                    Ok(())
                } else {
                    Err(fail())
                };
            }
            let provider = port.provider.ok_or_else(fail)?;
            if version == Version::Original {
                let lr = *regs.get(14).ok_or_else(fail)?;
                let pc = *regs.get(15).ok_or_else(fail)?;
                if stale_original(pc, lr, provider) {
                    return Err(fail());
                }
            }
            let pos =
                usize::try_from(START.checked_sub(base).ok_or_else(fail)?).map_err(|_| fail())?;
            let end = pos.checked_add(patch.len()).ok_or_else(fail)?;
            bytes
                .get_mut(pos..end)
                .ok_or_else(fail)?
                .copy_from_slice(&patch);
        } else if let Some(provider) = port.provider {
            if name == provider.region && !provider_code_matches(bytes, base, provider.entry) {
                return Err(fail());
            }
        }
        Ok(())
    }
}
