//! Exceções de ABI verificadas para um par exato de módulo e manifesto.
use super::*;

const LEGACY_DEVICE_INFO: (&str, &str) = (
    "3aee9085c06917473039cb9a30eb34d8a822f7321f304557e479f88f86c669b9",
    "b9645ff9c0a1a744a5fdd6a2e67732a4e95a55b837a0295a46bc04289efb1c55",
);

// O BitBlt destes pares recebe {altura, largura, pixels}, com pixels RGB565.
// Os construtores e a conversão da paleta foram conferidos no guest; não é
// seguro reconhecer esse formato por valores que por acaso pareçam dimensões.
const OEM_DESCRIPTOR_565: &[(&str, &str)] = &[
    (
        "f693a0ff75f98efe034c88f36e0dd24ec6b7302190b7dbc34d6a4914e53a07e8",
        "8149b5e92bf6a9c51730acaa271e698ea068778b35e98106d0069f4904c42e95",
    ),
    (
        "0ce4c727fad23abd0588a65b1b42de2b338cdbb972d34f431424ca7d5ed9c372",
        "23ee170d578cc4d8edc993dcb2ed10766b1189e72d6ebbc6f248f115451c443c",
    ),
    (
        "9fe4954e114fcbeeb9c16452c0108ea8536b5a02b09b2c131c861113c4537f4d",
        "dcd418e20007ce90254fb1b3f520d295aa8b18a163f5635cc48d586389f6e604",
    ),
];

const CACHED_NATIVE_IMAGE: (&str, &str) = (
    "3786a3f4af6841f489a279e2575dc64282afb47cb1b3f39ad4d70d8cb17043a7",
    "bda94290d1941ec2ea2d1f731568fd080ab700636515eb1e7f294e199a290a5f",
);

// Fruit Escape carrega uma imagem nativa 120x82 e a centraliza em (28,69).
// Em 0x11148/0x11168/0x11178, MVN/MOV/STR passam deliberadamente -1 nos
// dois tamanhos do BitBlt. O SDK 4 não define esse sentinela: vale só para
// este par, nas imagens convertidas e no caminho transparente observado.
const NATIVE_IMAGE_FULL_BLIT: (&str, &str) = (
    "ec6468e48bac24002744577387db4036f4e877fa5cc4d23b250fceacb935273a",
    "cd917c2c0c359cfc970d578a0a3fb2195df22c5471cc76c9fc25b6e308f0a127",
);


// O despacho real do Vlingo trata EVT_APP_START_WINDOW e inicializa CDisplayCanvas,
// CRootContainer e IWindowMgr nesse ramo. Apps clássicos que devolvem TRUE para
// eventos desconhecidos conservam APP_START; não inferimos flags privadas do MIF.
const WINDOW_STARTUP_PROFILE: (&str, &str) = (
    "bc89cbfd7565995672bbca0602fa9709e3b8fe1ad38eaf4636688bade5ae56ab",
    "513b4ff61e718a285346f3a50a392b433f068e62c9ac36429398e2ff2c121746",
);

impl<C: CpuBackend> Machine<C> {
    /// O launcher seleciona a ABI antes de executar código do pacote.
    pub fn set_handset_runtime_identity(&mut self, module: &str, mif: &str) -> bool {
        let window_start = module.eq_ignore_ascii_case(WINDOW_STARTUP_PROFILE.0)
            && mif.eq_ignore_ascii_case(WINDOW_STARTUP_PROFILE.1);
        self.set_window_start_capability(window_start);
        let native_text = self.set_native_text_identity(module, mif);
        self.legacy_device_info = module.eq_ignore_ascii_case(LEGACY_DEVICE_INFO.0)
            && mif.eq_ignore_ascii_case(LEGACY_DEVICE_INFO.1);
        self.descritor_oem_565 = OEM_DESCRIPTOR_565
            .iter()
            .any(|&(m, f)| module.eq_ignore_ascii_case(m) && mif.eq_ignore_ascii_case(f));
        self.native_image_full_blit = module.eq_ignore_ascii_case(NATIVE_IMAGE_FULL_BLIT.0)
            && mif.eq_ignore_ascii_case(NATIVE_IMAGE_FULL_BLIT.1);
        let cached_image = module.eq_ignore_ascii_case(CACHED_NATIVE_IMAGE.0)
            && mif.eq_ignore_ascii_case(CACHED_NATIVE_IMAGE.1);
        self.native_image_padding = if cached_image {
            self.cached_native_image_padding().unwrap_or(16)
        } else {
            0
        };
        let font_loop = self.apply_pocket_god_font_compat(module, mif);
        self.pocket_god_font_compat = font_loop;
        let palette = self.apply_penoa_palette_compat(module, mif);
        self.penoa_palette_compat = palette;
        let background = self.apply_three_kingdoms_background_compat(module, mif);
        self.three_kingdoms_background_compat = background;
        let public_framebuffer = self.apply_jumong_public_framebuffer_compat(module, mif);
        self.jumong_public_framebuffer_compat = public_framebuffer;
        self.jjayo_graphics_compat = self.apply_jjayo_graphics_compat(module, mif);
        self.mini_resource_layout_compat = if module.eq_ignore_ascii_case(MINI_RESOURCE_LAYOUT_PAIR.0)
            && mif.eq_ignore_ascii_case(MINI_RESOURCE_LAYOUT_PAIR.1)
        {
            Some(self.apply_mini_resource_layout_compat())
        } else { None };
        self.kingdom_swerve_compat = self.apply_kingdom_swerve_compat(module, mif);
        self.epic_public_framebuffer = epic_framebuffer::EpicFramebufferPort::select(module, mif);
        native_text
            || window_start
            || self.epic_public_framebuffer.is_some()
            || self.jjayo_graphics_compat.is_some()
            || self.mini_resource_layout_compat == Some(true)
            || self.kingdom_swerve_compat.as_ref().is_some_and(|port| port.active())
            || font_loop
            || palette
            || background
            || public_framebuffer
            || self.legacy_device_info
            || self.descritor_oem_565
            || self.native_image_full_blit
            || cached_image
    }

    // O cache do pacote guarda o deslocamento real dos pixels. Preservar também
    // os caches 36/64 já calibrados evita trocar o provedor sob um save existente.
    // A escolha só ocorre no par verificado e não modifica o arquivo do jogo.
    fn cached_native_image_padding(&self) -> Option<u32> {
        use std::io::Read;
        let path = self.vfs.resolve("lcdinfo.dat")?;
        let metadata = std::fs::symlink_metadata(&path).ok()?;
        if !metadata.file_type().is_file() || metadata.len() != 7 {
            return None;
        }
        let mut bytes = Vec::with_capacity(8);
        std::fs::File::open(path)
            .ok()?
            .take(8)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() != 7 || bytes[4..] != [16, 0, 3] {
            return None;
        }
        let offset = u32::from_le_bytes(bytes[..4].try_into().ok()?);
        matches!(offset, 36 | 52 | 64).then(|| offset - 36)
    }
}

// Pocket God 1.0.0I tem seis fontes em grades. A última declara 19 colunas e 5 linhas,
// como as cinco anteriores, mas dois limites em 0x4db80/0x4dba0 fazem 19*19.
// A leitura de 361 entradas atravessa a tabela de 95 e o fim da pilha. R7 conserva 5.
// Só este par MOD/MIF, com ambas as instruções originais, admite trocar MUL r2,r1,r1
// por MUL r2,r1,r7. O arquivo permanece intacto; alteramos apenas a cópia em memória.
const POCKET_GOD_FONT_PAIR: (&str, &str) = (
    "605fbf1d43f9edb1a4c3e21c6336a5f145457d6d8285c817f8b67e0815b03a60",
    "12ded40869036ceec929b4e0fa4086b8ce6517d99252f68cd9c5529fcdeea89f",
);
const POCKET_GOD_FONT_AT: u32 = 0x0004_db80;
fn pocket_god_font_patch(bytes: &[u8]) -> Option<[u8; 36]> {
    let mut patch: [u8; 36] = bytes.try_into().ok()?;
    let words = [
        u32::from_le_bytes(patch[..4].try_into().ok()?),
        u32::from_le_bytes(patch[32..36].try_into().ok()?),
    ];
    if words != [0xe002_0191; 2] && words != [0xe002_0791; 2] {
        return None;
    }
    for offset in [0, 32] {
        patch[offset..offset + 4].copy_from_slice(&0xe002_0791u32.to_le_bytes());
    }
    Some(patch)
}
impl<C: CpuBackend> Machine<C> {
    fn apply_pocket_god_font_compat(&mut self, module: &str, mif: &str) -> bool {
        if !module.eq_ignore_ascii_case(POCKET_GOD_FONT_PAIR.0)
            || !mif.eq_ignore_ascii_case(POCKET_GOD_FONT_PAIR.1)
        {
            return false;
        }
        if !self.module.mem.regions().iter().any(|region| {
            region.writable
                && region.base <= POCKET_GOD_FONT_AT
                && u64::from(region.base) + region.bytes.len() as u64
                    >= u64::from(POCKET_GOD_FONT_AT) + 36
        }) {
            return false;
        }
        let Some(template) = self
            .module
            .mem
            .read(POCKET_GOD_FONT_AT, 36)
            .ok()
            .and_then(pocket_god_font_patch)
        else {
            return false;
        };
        // Normalmente o launcher chama antes de reset. Se já executou, a escrita do backend
        // invalida os blocos JIT; conferir as duas palavras antes evita alteração parcial.
        let mut live = [0u8; 36];
        if self.cpu.read_mem(POCKET_GOD_FONT_AT, &mut live).is_ok() {
            let Some(patch) = pocket_god_font_patch(&live) else {
                return false;
            };
            if self.cpu.write_mem(POCKET_GOD_FONT_AT, &patch).is_err() {
                return false;
            }
        }
        self.module.mem.write(POCKET_GOD_FONT_AT, &template).is_ok()
    }
}
impl<C: CpuBackend> Machine<C> {
    // O save contém código gravável. Estados anteriores à correção ainda carregam os
    // dois MUL antigos; preparar a cópia antes da restauração impede reintroduzir o fault.
    pub(super) fn prepare_pocket_god_font_restore(
        &self,
        name: &str,
        base: u32,
        bytes: &mut [u8],
    ) -> Result<(), crate::save_state::Erro> {
        if !self.pocket_god_font_compat || name != "module" {
            return Ok(());
        }
        let fail = || crate::save_state::Erro::Secao {
            nome: "mem.module".into(),
            motivo: "as instruções da fonte não correspondem ao par verificado".into(),
        };
        let offset = POCKET_GOD_FONT_AT.checked_sub(base).ok_or_else(fail)? as usize;
        let range = bytes.get_mut(offset..offset + 36).ok_or_else(fail)?;
        let patch = pocket_god_font_patch(range).ok_or_else(fail)?;
        range.copy_from_slice(&patch);
        Ok(())
    }
}

// Penoa II copia oito cores (32 bytes) para uma variável local de 0x20 bytes,
// seguida pelo cabeçalho BMP na mesma pilha. O deslocamento #10 pede 1024 bytes
// dessa variável: copia o cabeçalho e a pilha do chamador e atravessa o mapa.
// O BMP já foi preenchido com 0xff; trocar apenas #10 por #5 conserva as oito
// cores originais e os 992 bytes restantes. As sondas posteriores substituem
// a paleta inteira com dados dos recursos. Não há paleta criada pelo host.
const PENOA_PALETTE_PAIR: (&str, &str) = (
    "1d6e4265fb2f4093a986b2694812cbf2dd14b8ede7e53220d1e51dfee0572550",
    "2f2bbec33c41fa5df1ad705f47795281eb5eed6f0c12c9f6046e2b1aeae386df",
);
const PENOA_PALETTE_AT: u32 = 0x0001_62f8;
fn penoa_palette_patch(bytes: &[u8]) -> Option<[u8; 8]> {
    let original = [0x01, 0x22, 0x03, 0x68, 0x20, 0x1c, 0x92, 0x02];
    let fixed = [0x01, 0x22, 0x03, 0x68, 0x20, 0x1c, 0x52, 0x01];
    (bytes == original || bytes == fixed).then_some(fixed)
}
impl<C: CpuBackend> Machine<C> {
    fn apply_penoa_palette_compat(&mut self, module: &str, mif: &str) -> bool {
        if !module.eq_ignore_ascii_case(PENOA_PALETTE_PAIR.0)
            || !mif.eq_ignore_ascii_case(PENOA_PALETTE_PAIR.1)
        {
            return false;
        }
        if !self.module.mem.regions().iter().any(|region| {
            region.writable
                && region.base <= PENOA_PALETTE_AT
                && u64::from(region.base) + region.bytes.len() as u64
                    >= u64::from(PENOA_PALETTE_AT) + 8
        }) {
            return false;
        }
        let Some(template) = self
            .module
            .mem
            .read(PENOA_PALETTE_AT, 8)
            .ok()
            .and_then(penoa_palette_patch)
        else {
            return false;
        };
        // Conferir o contexto inteiro antes de qualquer escrita também impede
        // aplicar a correção sobre código alterado num estado já em execução.
        let mut live = [0u8; 8];
        if self.cpu.read_mem(PENOA_PALETTE_AT, &mut live).is_ok() {
            let Some(patch) = penoa_palette_patch(&live) else {
                return false;
            };
            if self.cpu.write_mem(PENOA_PALETTE_AT, &patch).is_err() {
                return false;
            }
        }
        self.module.mem.write(PENOA_PALETTE_AT, &template).is_ok()
    }
    pub(super) fn prepare_penoa_palette_restore(
        &self,
        name: &str,
        base: u32,
        bytes: &mut [u8],
    ) -> Result<(), crate::save_state::Erro> {
        if !self.penoa_palette_compat || name != "module" {
            return Ok(());
        }
        let fail = || crate::save_state::Erro::Secao {
            nome: "mem.module".into(),
            motivo: "as instruções da paleta não correspondem ao par verificado".into(),
        };
        let offset = PENOA_PALETTE_AT.checked_sub(base).ok_or_else(fail)? as usize;
        let range = bytes.get_mut(offset..offset + 8).ok_or_else(fail)?;
        let patch = penoa_palette_patch(range).ok_or_else(fail)?;
        range.copy_from_slice(&patch);
        Ok(())
    }
}

// O fundo original usa tiles de 52x24 com deslocamento alternado 0/-31.
// O limite fixo de quatro colunas termina em 208/177, embora GetDeviceInfo
// e o canvas aceitem larguras maiores. Em 240x320 sobram 14.952 pixels magenta.
// Completar o laço usa somente o tile original; não muda geometria ou recursos.
const THREE_KINGDOMS_BACKGROUND_PAIR: (&str, &str) = (
    "ec94b2c155bdece2f8106e391061bce55273af0a1a4cdd70244718409b6a8541",
    "da50aa48f15a60150b1b94c051d6e83d173e905bc8086e2c6ded6188f1a263e4",
);
const THREE_KINGDOMS_BACKGROUND_AT: u32 = 0x0002_051e;
const THREE_KINGDOMS_BACKGROUND_COUNT: usize = 56;
const THREE_KINGDOMS_BACKGROUND_ORIGINAL: [u8; 84] = [
    0x1d, 0x98, 0x00, 0x25, 0xc0, 0x07, 0xc0, 0x0f, 0x43, 0x01, 0xc0, 0x1a, 0x1f, 0x90, 0x18, 0x21,
    0x04, 0x91, 0x34, 0x21, 0x34, 0x20, 0x03, 0x90, 0x00, 0x22, 0x05, 0x92, 0x1f, 0x98, 0x1e, 0x9a,
    0x69, 0x43, 0x41, 0x18, 0x02, 0x92, 0x00, 0x22, 0x01, 0x91, 0x00, 0x97, 0x00, 0x23, 0x20, 0x1c,
    0x20, 0x99, 0xf2, 0xf7, 0xc0, 0xfd, 0x01, 0x35, 0x04, 0x2d, 0xe8, 0xdb, 0x1d, 0x98, 0x01, 0x30,
    0x1d, 0x90, 0x1d, 0x98, 0x18, 0x22, 0x42, 0x43, 0x1e, 0x92, 0x00, 0x23, 0xf0, 0x5e, 0x18, 0x30,
    0x82, 0x42, 0xd5, 0xdb,
];

fn three_kingdoms_background_patch(bytes: &[u8], width: u32) -> Option<[u8; 84]> {
    let columns = u8::try_from(width.checked_add(31)?.div_ceil(52).max(4)).ok()?;
    let mut patch: [u8; 84] = bytes.try_into().ok()?;
    // Um save antigo pode ter o original. Um save corrigido deve corresponder
    // à geometria atual; outro imediato, mesmo plausível, é código alterado.
    if patch[THREE_KINGDOMS_BACKGROUND_COUNT] != 4
        && patch[THREE_KINGDOMS_BACKGROUND_COUNT] != columns
    {
        return None;
    }
    patch[THREE_KINGDOMS_BACKGROUND_COUNT] = 4;
    if patch != THREE_KINGDOMS_BACKGROUND_ORIGINAL {
        return None;
    }
    patch[THREE_KINGDOMS_BACKGROUND_COUNT] = columns;
    Some(patch)
}
impl<C: CpuBackend> Machine<C> {
    fn apply_three_kingdoms_background_compat(&mut self, module: &str, mif: &str) -> bool {
        if !module.eq_ignore_ascii_case(THREE_KINGDOMS_BACKGROUND_PAIR.0)
            || !mif.eq_ignore_ascii_case(THREE_KINGDOMS_BACKGROUND_PAIR.1)
        {
            return false;
        }
        if !self.module.mem.regions().iter().any(|region| {
            region.writable
                && region.base <= THREE_KINGDOMS_BACKGROUND_AT
                && u64::from(region.base) + region.bytes.len() as u64
                    >= u64::from(THREE_KINGDOMS_BACKGROUND_AT) + 84
        }) {
            return false;
        }
        let width = self.screen().width();
        let Some(template) = self
            .module
            .mem
            .read(THREE_KINGDOMS_BACKGROUND_AT, 84)
            .ok()
            .and_then(|bytes| three_kingdoms_background_patch(bytes, width))
        else {
            return false;
        };
        // Conferir ambos os contextos antes de escrever mantém template e CPU
        // intactos se o código foi alterado. write_mem invalida blocos JIT vivos.
        let mut live = [0u8; 84];
        if self
            .cpu
            .read_mem(THREE_KINGDOMS_BACKGROUND_AT, &mut live)
            .is_ok()
        {
            let Some(patch) = three_kingdoms_background_patch(&live, width) else {
                return false;
            };
            if self
                .cpu
                .write_mem(THREE_KINGDOMS_BACKGROUND_AT, &patch)
                .is_err()
            {
                return false;
            }
        }
        self.module
            .mem
            .write(THREE_KINGDOMS_BACKGROUND_AT, &template)
            .is_ok()
    }
    pub(super) fn prepare_three_kingdoms_background_restore(
        &self,
        name: &str,
        base: u32,
        bytes: &mut [u8],
    ) -> Result<(), crate::save_state::Erro> {
        if !self.three_kingdoms_background_compat || name != "module" {
            return Ok(());
        }
        let fail = || crate::save_state::Erro::Secao {
            nome: "mem.module".into(),
            motivo: "o laço do fundo não corresponde ao par e à geometria verificados".into(),
        };
        let offset = THREE_KINGDOMS_BACKGROUND_AT
            .checked_sub(base)
            .ok_or_else(fail)? as usize;
        let range = bytes.get_mut(offset..offset + 84).ok_or_else(fail)?;
        let patch =
            three_kingdoms_background_patch(range, self.screen().width()).ok_or_else(fail)?;
        range.copy_from_slice(&patch);
        Ok(())
    }
}

// O classificador de Jumong reconhece somente BREW 1.0, 1.2 e 2.1; o 4.0.2
// verdadeiro deixa a flag antiga em zero e escolhe offsets privados por modelo.
// O próprio getter já contém GetDeviceBitmap -> QueryInterface(AEEDIB).
// Neste par, selecionar esse caminho público conserva a versão anunciada e os
// campos AEEDIB originais (+4 para pBmp, +24 para colorDepth), sem imitar OEM.
const JUMONG_PUBLIC_FRAMEBUFFER_PAIR: (&str, &str) = (
    "5fbb0a3d36da30c592fc01cbe4e4cd5ece4cf3e41421d965cd5a67cd70badd7a",
    "872bac57d05185a938e37809254ff3cf9528f85bf367c482f9044ece2bc5a118",
);
const JUMONG_PUBLIC_FRAMEBUFFER_AT: u32 = 0x0001_e984;
fn jumong_public_framebuffer_patch(bytes: &[u8]) -> Option<[u8; 12]> {
    let original = [
        0x68, 0x68, 0x00, 0x28, 0x2c, 0xd0, 0xe9, 0x68, 0x89, 0x07, 0x23, 0xd5,
    ];
    let fixed = [
        0x68, 0x68, 0x00, 0x28, 0x2c, 0xd0, 0xe9, 0x68, 0x89, 0x07, 0xc0, 0x46,
    ];
    (bytes == original || bytes == fixed).then_some(fixed)
}
impl<C: CpuBackend> Machine<C> {
    fn apply_jumong_public_framebuffer_compat(&mut self, module: &str, mif: &str) -> bool {
        if !module.eq_ignore_ascii_case(JUMONG_PUBLIC_FRAMEBUFFER_PAIR.0)
            || !mif.eq_ignore_ascii_case(JUMONG_PUBLIC_FRAMEBUFFER_PAIR.1)
        {
            return false;
        }
        if !self.module.mem.regions().iter().any(|region| {
            region.writable
                && region.base <= JUMONG_PUBLIC_FRAMEBUFFER_AT
                && u64::from(region.base) + region.bytes.len() as u64
                    >= u64::from(JUMONG_PUBLIC_FRAMEBUFFER_AT) + 12
        }) {
            return false;
        }
        let Some(template) = self
            .module
            .mem
            .read(JUMONG_PUBLIC_FRAMEBUFFER_AT, 12)
            .ok()
            .and_then(jumong_public_framebuffer_patch)
        else {
            return false;
        };
        let mut live = [0u8; 12];
        if self
            .cpu
            .read_mem(JUMONG_PUBLIC_FRAMEBUFFER_AT, &mut live)
            .is_ok()
        {
            let Some(patch) = jumong_public_framebuffer_patch(&live) else {
                return false;
            };
            if self
                .cpu
                .write_mem(JUMONG_PUBLIC_FRAMEBUFFER_AT, &patch)
                .is_err()
            {
                return false;
            }
        }
        self.module
            .mem
            .write(JUMONG_PUBLIC_FRAMEBUFFER_AT, &template)
            .is_ok()
    }
    pub(super) fn prepare_jumong_public_framebuffer_restore(
        &self,
        name: &str,
        base: u32,
        bytes: &mut [u8],
    ) -> Result<(), crate::save_state::Erro> {
        if !self.jumong_public_framebuffer_compat || name != "module" {
            return Ok(());
        }
        let fail = || crate::save_state::Erro::Secao {
            nome: "mem.module".into(),
            motivo: "o caminho público de framebuffer não corresponde ao par verificado".into(),
        };
        let offset = JUMONG_PUBLIC_FRAMEBUFFER_AT
            .checked_sub(base)
            .ok_or_else(fail)? as usize;
        let range = bytes.get_mut(offset..offset + 12).ok_or_else(fail)?;
        let patch = jumong_public_framebuffer_patch(range).ok_or_else(fail)?;
        range.copy_from_slice(&patch);
        Ok(())
    }
}

// As colunas originais chamam o rasterizador com a altura total, mesmo quando
// a imagem é menor. Em 0319 isso copia 202 linhas de um BMP de 146. Reentrar
// no próprio tiler com uma coluna permite ao laço vertical dividir 146+56;
// o caso real teve 12.596 diferenças em 12.636 pixels antes e zero depois.
const JJAYO_NESTED_TILES_PAIRS: &[(&str, &str)] = &[
    (
        "577625c36a78945e8bc892871747f177b86abf53a40ae8e22da160d0663db3eb",
        "20abd42961d1c31f6d6e64f37077d32035d57e666b7ca01d5a2963ce726b3625",
    ),
    (
        "1d94991c74b20caea894e174067d641d9d76ec5a130767eb31408512d1d0b82a",
        "21e5b7f6e181eb8411c1a0aa39a2a0a32da241e162f555d84a7a818b4b0b385d",
    ),
    (
        "fae3b42fcdf65947fa339a39057bea02b35a73c8b1d9898d620c895347a82d42",
        "5767c13bab1079d1c586b082d28c20dc1372854a7a9914f94a0d48560954d54b",
    ),
];
const JJAYO_NESTED_TILES_AT: u32 = 0x12368;
const JJAYO_NESTED_TILES_ORIGINAL: [u32; 116] = [
    0xe92d4ff0, 0xe24dd03c, 0xe59d9064, 0xe59d8070, 0xe59db060, 0xe1d9a0b4, 0xe1a04000, 0xe1d900b6,
    0xe1a05001, 0xe3a01000, 0xe58d0024, 0xe5841024, 0xe1a07002, 0xe1a06003, 0xe3590000, 0x0a00017d,
    0xe5d9000a, 0xe3500000, 0x1a000004, 0xe5990000, 0xe3500000, 0x0a000177, 0xe3888040, 0xe5c91009,
    0xe3760001, 0x01d960b4, 0x01d9b0b6, 0x03a0c000, 0x03a00000, 0x058d006c, 0x058dc068, 0xe35a0000,
    0x1156000a, 0xda000024, 0xe3a0c000, 0xea00001a, 0xe28d1068, 0xe8910003, 0xe28d200c, 0xe8820403,
    0xe58d8018, 0xe3180010, 0x0a000007, 0xe5990000, 0xe5941050, 0xe085200c, 0xe0803001, 0xe58d3008,
    0xe88d0c00, 0xe5941018, 0xea000007, 0xe5991000, 0xe5940050, 0xe085200c, 0xe0813000, 0xe58d3008,
    0xe88d0c00, 0xe5941068, 0xe0811000, 0xe1a03007, 0xe1a00004, 0xebfffd07, 0xe59dc038, 0xe08c000a,
    0xe58d0038, 0xe1500006, 0xdaffffe0, 0xe15c0006, 0xaa000148, 0xe085500c, 0xe046600c, 0xe59d0024,
    0xe3500000, 0x159d0024, 0x115b0000, 0xda000027, 0xe3a0c000, 0xea00001c, 0xe28d1068, 0xe8910003,
    0xe28d200c, 0xe8820403, 0xe58d8018, 0xe3180010, 0x0a000008, 0xe5990000, 0xe5941050, 0xe59d2024,
    0xe0803001, 0xe98d000c, 0xe58d6000, 0xe5941018, 0xe087300c, 0xea000008, 0xe5991000, 0xe5940050,
    0xe59d2024, 0xe0813000, 0xe98d000c, 0xe58d6000, 0xe5941068, 0xe087300c, 0xe0811000, 0xe1a02005,
    0xe1a00004, 0xebfffcdb, 0xe59dc034, 0xe59d0024, 0xe08c0000, 0xe58d0034, 0xe150000b, 0xdaffffdd,
    0xe15c000b, 0xaa00011b, 0xe087700c, 0xe04bb00c,
];
fn jjayo_nested_tiles_patch(bytes: &[u8]) -> Option<[u8; 464]> {
    let original: Vec<u8> = JJAYO_NESTED_TILES_ORIGINAL
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect();
    let mut patch: [u8; 464] = original.as_slice().try_into().ok()?;
    let words: [u32; 13] = [
        0xe58d8010, 0xe59d0068, 0xe58d0008, 0xe59d006c, 0xe58d000c, 0xe58d9004, 0xe58db000,
        0xe1a0300a, 0xe085100c, 0xe1a02007, 0xe1a00004, 0xebffffcf, 0xea00000c,
    ];
    for (i, w) in words.iter().enumerate() {
        patch[0x90 + i * 4..0x94 + i * 4].copy_from_slice(&w.to_le_bytes());
    }
    if bytes != original && bytes != patch {
        return None;
    }
    Some(patch)
}
// A sonda reserva 48 bytes, mas os pixels reais ficam em 36/37. O limite
// antigo reserva-10 ignora o padrão; buscar até zero mantém a busca dentro
// da alocação observada e descobre o raster sem presumir tamanho de cabeçalho.
const JJAYO_CALIBRATION: &[(u32, [u32; 29])] = &[
    (
        0x265b0,
        [
            0xe5941020, 0xe3a00000, 0xe3510010, 0x1a000018, 0xe2475001, 0xe247100a, 0xea000011,
            0xe3500000, 0x0a000009, 0xe5940040, 0xe7d00005, 0xe350001f, 0x1a000005, 0xe1a01006,
            0xe1a00004, 0xeb0024c7, 0xe5845050, 0xe8bd40f8, 0xe12fff1e, 0xe5940040, 0xe7d00005,
            0xe2455001, 0xe35000f8, 0x13a00000, 0x03a00001, 0xe1550001, 0xa3550000, 0xaaffffea,
            0xea000017,
        ],
    ),
    (
        0x265c8,
        [
            0xe5941020, 0xe3a00000, 0xe3510010, 0x1a000018, 0xe2475001, 0xe247100a, 0xea000011,
            0xe3500000, 0x0a000009, 0xe5940040, 0xe7d00005, 0xe350001f, 0x1a000005, 0xe1a01006,
            0xe1a00004, 0xeb0024cb, 0xe5845050, 0xe8bd40f8, 0xe12fff1e, 0xe5940040, 0xe7d00005,
            0xe2455001, 0xe35000f8, 0x13a00000, 0x03a00001, 0xe1550001, 0xa3550000, 0xaaffffea,
            0xea000017,
        ],
    ),
    (
        0x25ddc,
        [
            0xe5941020, 0xe3a00000, 0xe3510010, 0x1a000018, 0xe2475001, 0xe247100a, 0xea000011,
            0xe3500000, 0x0a000009, 0xe5940040, 0xe7d00005, 0xe350001f, 0x1a000005, 0xe1a01006,
            0xe1a00004, 0xeb0024a8, 0xe5845050, 0xe8bd40f8, 0xe12fff1e, 0xe5940040, 0xe7d00005,
            0xe2455001, 0xe35000f8, 0x13a00000, 0x03a00001, 0xe1550001, 0xa3550000, 0xaaffffea,
            0xea000017,
        ],
    ),
];
fn jjayo_calibration_patch(bytes: &[u8], case: usize) -> Option<[u8; 116]> {
    let original: Vec<u8> = JJAYO_CALIBRATION
        .get(case)?
        .1
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect();
    let mut patch: [u8; 116] = original.as_slice().try_into().ok()?;
    patch[20..24].copy_from_slice(&0xe3a01000u32.to_le_bytes());
    if bytes != original && bytes != patch {
        return None;
    }
    Some(patch)
}
impl<C: CpuBackend> Machine<C> {
    fn apply_jjayo_graphics_compat(&mut self, module: &str, mif: &str) -> Option<usize> {
        let case = JJAYO_NESTED_TILES_PAIRS
            .iter()
            .position(|&(m, f)| module.eq_ignore_ascii_case(m) && mif.eq_ignore_ascii_case(f))?;
        let cal_at = JJAYO_CALIBRATION[case].0;
        for (at, len) in [(JJAYO_NESTED_TILES_AT, 464), (cal_at, 116)] {
            if !self.module.mem.regions().iter().any(|r| {
                r.writable
                    && r.base <= at
                    && u64::from(r.base) + r.bytes.len() as u64 >= u64::from(at) + len
            }) {
                return None;
            }
        }
        let template =
            jjayo_nested_tiles_patch(self.module.mem.read(JJAYO_NESTED_TILES_AT, 464).ok()?)?;
        let calibration = jjayo_calibration_patch(self.module.mem.read(cal_at, 116).ok()?, case)?;
        // Ambos os contextos, no template e na CPU, precisam passar antes da primeira
        // escrita. write_mem invalida os blocos compilados que contêm a rotina ARM.
        let mut live = [0u8; 464];
        let mut live_cal = [0u8; 116];
        let has_live = self.cpu.read_mem(JJAYO_NESTED_TILES_AT, &mut live).is_ok();
        let has_cal = self.cpu.read_mem(cal_at, &mut live_cal).is_ok();
        if has_live != has_cal {
            return None;
        }
        if has_live {
            let patch = jjayo_nested_tiles_patch(&live)?;
            let cal = jjayo_calibration_patch(&live_cal, case)?;
            self.cpu.write_mem(JJAYO_NESTED_TILES_AT, &patch).ok()?;
            if self.cpu.write_mem(cal_at, &cal).is_err() {
                let _ = self.cpu.write_mem(JJAYO_NESTED_TILES_AT, &live);
                return None;
            }
        }
        self.module
            .mem
            .write(JJAYO_NESTED_TILES_AT, &template)
            .ok()?;
        self.module.mem.write(cal_at, &calibration).ok()?;
        Some(case)
    }
    pub(super) fn prepare_jjayo_graphics_restore(
        &self,
        name: &str,
        base: u32,
        bytes: &mut [u8],
        regs: &[u32],
    ) -> Result<(), crate::save_state::Erro> {
        let Some(case) = self.jjayo_graphics_compat else {
            return Ok(());
        };
        if name != "module" {
            return Ok(());
        }
        let fail = || crate::save_state::Erro::Secao {
            nome: "mem.module".into(),
            motivo: "as rotinas gráficas Jjayo não correspondem aos contextos verificados".into(),
        };
        let tile_offset = JJAYO_NESTED_TILES_AT.checked_sub(base).ok_or_else(fail)? as usize;
        let cal_offset = JJAYO_CALIBRATION[case]
            .0
            .checked_sub(base)
            .ok_or_else(fail)? as usize;
        let saved_tile = bytes.get(tile_offset..tile_offset + 464).ok_or_else(fail)?;
        let tile = jjayo_nested_tiles_patch(saved_tile).ok_or_else(fail)?;
        // O port troca a preparação de argumentos, não só um imediato. Um save
        // antigo no caller ou num callee com LR nele usa a pilha e os registradores
        // da versão anterior. Saves já portados conservam esse contexto e retomam.
        if saved_tile[0x90..0x94] == 0xe28d1068u32.to_le_bytes()
            && regs.get(14..16).ok_or_else(fail)?.iter().any(|&ip| {
                let pc = ip & !1;
                (JJAYO_NESTED_TILES_AT..JJAYO_NESTED_TILES_AT + 464).contains(&pc)
            })
        {
            return Err(fail());
        }
        let cal = jjayo_calibration_patch(
            bytes.get(cal_offset..cal_offset + 116).ok_or_else(fail)?,
            case,
        )
        .ok_or_else(fail)?;
        bytes[tile_offset..tile_offset + 464].copy_from_slice(&tile);
        bytes[cal_offset..cal_offset + 116].copy_from_slice(&cal);
        Ok(())
    }
}

// O pacote traz body120x134, tiras120x77 e personagens27x76. A escolha
// por altura fisica202/207 usa colunas43/y80 e pede pixels alem da largura120.
// O ramo existente de recursos pequenos usa colunas29/y58, sem mudar o LCD.
const MINI_RESOURCE_LAYOUT_PAIR: (&str, &str) = (
    "9bc21120c00d32dc53f30bcc4fc09d381af60e6cc9e854d9ead7e45cd377ec0c",
    "b553d4da8e10a279162e7d6cb74eaaff29ca26b7eda17a1cdbcd8c29cd3bf434",
);
const MINI_RESOURCE_LAYOUT_AT: u32 = 0x27a40;
const MINI_RESOURCE_LAYOUT_SHA: [u8; 32] = [
    137, 18, 174, 36, 223, 22, 40, 205, 55, 165, 40, 79, 218, 123, 235, 3, 207, 9, 160, 211, 26,
    109, 192, 0, 221, 172, 62, 160, 139, 76, 167, 18,
];
const MINI_RESOURCE_LAYOUT_ORIGINAL: [u16; 388] = [
    0xb5f0, 0x1c06, 0x206b, 0x0140, 0x1835, 0x8928, 0x217f, 0x0189, 0x198f, 0x28ca, 0xb08e, 0xd001,
    0x28cf, 0xd111, 0x200e, 0x61f8, 0x2016, 0x6238, 0x2021, 0x62b8, 0x896b, 0x2113, 0x3b15, 0x9309,
    0x89a8, 0x9105, 0x2118, 0x380e, 0x9007, 0x221a, 0x9108, 0xe00e, 0x2000, 0x61f8, 0x6238, 0x62b8,
    0x896b, 0x210c, 0x3b10, 0x9309, 0x89a8, 0x9105, 0x210b, 0x3809, 0x9007, 0x2215, 0x9108, 0x48a9,
    0x9206, 0x1830, 0x79c0, 0x2141, 0x0189, 0x198c, 0x2801, 0xd106, 0x23ff, 0x3311, 0x2200, 0x2100,
    0x6a20, 0xf7e9, 0xfedb, 0x48a1, 0x3830, 0x1832, 0x6bd0, 0x1e81, 0x290f, 0xd838, 0x1c08, 0x2900,
    0xda00, 0x3003, 0x1080, 0x2900, 0xda04, 0x4249, 0x0789, 0x0f89, 0x4249, 0xe001, 0x0789, 0x0f89,
    0x2900, 0xd105, 0x6a3a, 0x69f9, 0x323a, 0x311d, 0x4341, 0xe0e6, 0x2901, 0xd105, 0x6a3a, 0x69f9,
    0x323a, 0x311d, 0x4341, 0xe0d2, 0x2902, 0xd105, 0x6a3a, 0x69f9, 0x323a, 0x311d, 0x4341, 0xe0be,
    0x2903, 0xd000, 0xe0e5, 0x69fb, 0x6a3a, 0x331d, 0x1c19, 0x4341, 0x6ab8, 0x323a, 0x304c, 0x9202,
    0x9000, 0x9101, 0x6a60, 0x2200, 0xf003, 0xfaf2, 0xe0d5, 0x2812, 0xd117, 0x6a63, 0x69d8, 0x0602,
    0x6a58, 0x6a99, 0x0e12, 0x9204, 0x9002, 0x9103, 0x6b19, 0x691a, 0x9100, 0x9201, 0x6b58, 0x6ad9,
    0x6803, 0x6a3a, 0x699f, 0x323a, 0x1c0b, 0x2100, 0xf007, 0xfcf5, 0xe0bb, 0x2814, 0xd117, 0x6aa3,
    0x69d8, 0x0602, 0x6a58, 0x6a99, 0x0e12, 0x9204, 0x9002, 0x9103, 0x6b19, 0x691a, 0x9100, 0x9201,
    0x6b58, 0x6ad9, 0x6803, 0x6a3a, 0x699f, 0x323a, 0x1c0b, 0x2100, 0xf007, 0xfcdb, 0xe0a1, 0x2815,
    0xd117, 0x6a63, 0x69d8, 0x0602, 0x6a58, 0x6a99, 0x0e12, 0x9204, 0x9002, 0x9103, 0x6b19, 0x691a,
    0x9100, 0x9201, 0x6b58, 0x6ad9, 0x6803, 0x6a3a, 0x699f, 0x323a, 0x1c0b, 0x2100, 0xf007, 0xfcc1,
    0xe087, 0x2816, 0xd117, 0x6aa3, 0x69d8, 0x0602, 0x6a58, 0x6a99, 0x0e12, 0x9204, 0x9002, 0x9103,
    0x6b19, 0x691a, 0x9100, 0x9201, 0x6b58, 0x6ad9, 0x6803, 0x6a3a, 0x699f, 0x323a, 0x1c0b, 0x2100,
    0xf007, 0xfca7, 0xe06d, 0x2817, 0xd117, 0x6a63, 0x69d8, 0x0602, 0x6a58, 0x6a99, 0x0e12, 0x9204,
    0x9002, 0x9103, 0x6b19, 0x691a, 0x9100, 0x9201, 0x6b58, 0x6ad9, 0x6803, 0x6a3a, 0x699f, 0x323a,
    0x1c0b, 0x2100, 0xf007, 0xfc8d, 0xe053, 0x1fc1, 0x3913, 0x290b, 0xd841, 0x2003, 0xf007, 0xfd90,
    0x2203, 0x1a16, 0x2e03, 0x910a, 0xda14, 0x6a3a, 0x1c70, 0x323a, 0x920d, 0x69fb, 0x331d, 0x1c19,
    0x4341, 0x3103, 0x910c, 0x6a20, 0x900b, 0x8840, 0x9202, 0x9a0d, 0x380f, 0x9000, 0x980b, 0x9101,
    0xf003, 0xfa4e, 0x990a, 0x2900, 0xd108, 0x6a3a, 0x69f8, 0x323a, 0x301d, 0x4370, 0x1c01, 0x3103,
    0x6b60, 0xe016, 0x990a, 0x2901, 0xd108, 0x6a3a, 0x69f8, 0x323a, 0x301d, 0x4370, 0x1c01, 0x3103,
    0x6b20, 0xe00a, 0x990a, 0x2902, 0xd117, 0x6a3a, 0x69f8, 0x323a, 0x301d, 0x4370, 0x1c01, 0x6ae0,
    0x3103, 0x2300, 0xe006, 0x2826, 0xd107, 0x23ff, 0x3311, 0x2200, 0x6a20, 0x2100, 0xf7e9, 0xfdca,
    0xe003, 0x2828, 0xd101, 0x2300, 0x63d3, 0x89a8, 0x9905, 0x23ff, 0x1a42, 0x3311, 0x2100, 0x6be0,
    0xf7e9, 0xfdbc, 0x2206, 0x9202, 0x2103, 0x9807, 0x9101, 0x9000, 0x68e0, 0x2200, 0x2100, 0x9b09,
    0xf7e9, 0xfe92, 0x2103, 0x9101, 0x2206, 0x9b09, 0x9202, 0x9807, 0x189b, 0x2200, 0x9000, 0x68e0,
    0x2104, 0xf7e9, 0xfe85, 0x6ba0, 0x9908, 0x9a06, 0x2300, 0xf7e9, 0xfd9d, 0xb00e, 0xbcf0, 0xbc08,
    0x4718, 0x0000, 0x1ef0, 0x0000,
];
fn recurso_sha256_com_tamanho(path: &std::path::Path, expected: u64) -> Option<[u8; 32]> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let metadata = std::fs::symlink_metadata(path).ok()?;
    if !metadata.file_type().is_file() || metadata.len() != expected {
        return None;
    }
    let limit = expected.checked_add(1)?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(limit)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 != expected {
        return None;
    }
    Some(Sha256::digest(bytes).into())
}
fn mini_resource_layout_patch(bytes: &[u8]) -> Option<[u8; 776]> {
    let original: Vec<u8> = MINI_RESOURCE_LAYOUT_ORIGINAL
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect();
    let mut patch: [u8; 776] = original.as_slice().try_into().ok()?;
    patch[22..24].copy_from_slice(&0xe013u16.to_le_bytes());
    if bytes != original && bytes != patch {
        return None;
    }
    Some(patch)
}
impl<C: CpuBackend> Machine<C> {
    fn mini_resource_layout_asset_matches(&self) -> bool {
        self.vfs
            .resolve("mini2005.mbr")
            .and_then(|p| recurso_sha256_com_tamanho(&p, 74955))
            == Some(MINI_RESOURCE_LAYOUT_SHA)
    }
    fn apply_mini_resource_layout_compat(&mut self) -> bool {
        if !self.mini_resource_layout_asset_matches() {
            return false;
        }
        if !self.module.mem.regions().iter().any(|r| {
            r.writable
                && r.base <= MINI_RESOURCE_LAYOUT_AT
                && u64::from(r.base) + r.bytes.len() as u64
                    >= u64::from(MINI_RESOURCE_LAYOUT_AT) + 776
        }) {
            return false;
        }
        let Some(template) = self
            .module
            .mem
            .read(MINI_RESOURCE_LAYOUT_AT, 776)
            .ok()
            .and_then(mini_resource_layout_patch)
        else {
            return false;
        };
        let mut live = [0u8; 776];
        if self
            .cpu
            .read_mem(MINI_RESOURCE_LAYOUT_AT, &mut live)
            .is_ok()
        {
            let Some(patch) = mini_resource_layout_patch(&live) else {
                return false;
            };
            if self.cpu.write_mem(MINI_RESOURCE_LAYOUT_AT, &patch).is_err() {
                return false;
            }
        }
        self.module
            .mem
            .write(MINI_RESOURCE_LAYOUT_AT, &template)
            .is_ok()
    }
    pub(super) fn prepare_mini_resource_layout_restore(
        &self,
        name: &str,
        base: u32,
        bytes: &mut [u8],
        regs: &[u32],
    ) -> Result<(), crate::save_state::Erro> {
        if self.mini_resource_layout_compat.is_none() || name != "module" {
            return Ok(());
        }
        let fail = || crate::save_state::Erro::Secao {
            nome: "mem.module".into(),
            motivo: "o layout Mini2005 nao corresponde ao recurso e ao contexto verificados".into(),
        };
        let offset = MINI_RESOURCE_LAYOUT_AT.checked_sub(base).ok_or_else(fail)? as usize;
        let range = bytes.get_mut(offset..offset + 776).ok_or_else(fail)?;
        let patch = mini_resource_layout_patch(range).ok_or_else(fail)?;
        let original = range[22..24] == 0xd001u16.to_le_bytes();
        // O recurso atual tambem precisa corresponder ao save portado. Reconhecer
        // o par sem ativar o port preserva essa rejeicao numa maquina nova.
        if !self.mini_resource_layout_asset_matches() {
            return if original { Ok(()) } else { Err(fail()) };
        }
        if original
            && regs.get(14..16).ok_or_else(fail)?.iter().any(|&ip| {
                (MINI_RESOURCE_LAYOUT_AT..MINI_RESOURCE_LAYOUT_AT + 776).contains(&(ip & !1))
            })
        {
            return Err(fail());
        }
        range.copy_from_slice(&patch);
        Ok(())
    }
}
#[cfg(test)]
mod mini_resource_hash_tests {
    use super::recurso_sha256_com_tamanho;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    fn path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "brew-mini-hash-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }
    #[test]
    fn resource_hash_reads_exactly_the_declared_regular_file() {
        use sha2::{Digest, Sha256};
        let p = path();
        std::fs::write(&p, b"resource").unwrap();
        assert_eq!(
            recurso_sha256_com_tamanho(&p, 8),
            Some(Sha256::digest(b"resource").into())
        );
        assert_eq!(recurso_sha256_com_tamanho(&p, 7), None);
        assert_eq!(recurso_sha256_com_tamanho(&p, 9), None);
        std::fs::remove_file(&p).unwrap();
        assert_eq!(recurso_sha256_com_tamanho(&p, 8), None);
    }
    #[test]
    fn resource_hash_rejects_a_directory_without_reading_it() {
        let p = path();
        std::fs::create_dir(&p).unwrap();
        assert_eq!(recurso_sha256_com_tamanho(&p, 8), None);
        std::fs::remove_dir(p).unwrap();
    }
}
