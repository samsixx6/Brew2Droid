//! Contexto de AECHAR nativo, separado do Unicode usado pelo rasterizador.
//! Os perfis dependem do módulo e do MIF escolhido, ou de ausência auditada de MIF;
//! nunca do nome ou do glifo encontrado.

use super::*;
use encoding_rs::EUC_KR;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeTextEncoding {
    #[default]
    Unicode,
    EucKr,
    PackedEucKr,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct NativeTextContext {
    encoding: NativeTextEncoding,
    identity: Option<(String, String)>,
}

#[derive(Serialize, Deserialize)]
struct SavedNativeText {
    version: u32,
    context: NativeTextContext,
}

const SSA_WITHOUT_MANIFEST: &str =
    "74968a7d74d9a5ac19b51e00d157abb5d63fd166961b277943bc21c187d15fcf";
// Infinite Rook: o consumidor em 0x1058b passa cinco AECHAR para os dez
// bytes de "Copy Game!". O terminador também é de byte, como no perfil SSA.
const PACKED_PAIRS: &[(&str, &str)] = &[(
    "698c839f56b3b7beb096dc1b58a2818afd2253ae0c7d55a6af35b2d8e8ad43af",
    "9bb8fab4fc4fe54b939c1cc654b7591e69b417a3a432f14f10a95a0780aafbfe",
)];

// Os pares auditados recebem palavras KSC5601 empacotadas, não escalares Unicode.
// Vetores de DrawText e de entrada de WSTRNCOPYN foram conferidos na investigação
// privada de 29/10/2026. Parte desses vetores são avisos de erro de download da
// operadora; o perfil verifica a codificação, sem afirmar compatibilidade de gameplay.
const KOREAN_PAIRS: &[(&str, &str)] = &[
    // Bowling2: player/stats chegam em AECHAR CP949 (C7C3 B7B9...), enquanto
    // LoadResString normaliza os rotulos. O contexto nativo preserva ambos;
    // o replay privado manteve os rotulos e corrigiu player/power/spin/rank.
    (
        "2f89e3126a26eed73a2451621a07636b9cce1db005138c708131919d7024bb64",
        "e11d8cf13406b75f309fd1345a5879c4ec3f5f077439a4e2d97c2479514e44ea",
    ),
    (
        "8622d31bcb9b5649a14b7dd5894e6e1902c1a9f2f3281984a8dc138c9f814b9e",
        "afce4811ad7c04b7487bc9657b81d8f2c81285114f6413cbbc1b21d2cf995cde",
    ),
    (
        "c0202c963131bcade14fcef381e8bdfa85887c9061ce21d257cfa298e4329731",
        "bb819fc45d521e4505c7c1346d081744ce025fb934fb00a19f50c212a70c2bca",
    ),
    (
        "cffab4954291cfe43c7b897b933f0991ca99f2c5a14d33d353eee59b099814e5",
        "7c274380a800e780075f1a31de5916edc4b814af8c521c17ffd090c1f5215798",
    ),
    (
        "a55e9818182c38fb7f0c3d467e5f21f6a623be75473103140e9e357588f12a9d",
        "cd7b71720e7d2f93b14547e9c8e5142d9bb6c21af4074e93b3771faa6d95fd6f",
    ),
    (
        "e14a20c0029c38253b9c8e8ac4e5efbd15bd80e60eed5338064c3f1a35c1e76d",
        "314e5ebd26a22623854d9562c3aeac30481a3def118f5b460fd8168d535dd3fc",
    ),
    (
        "10ce143b64717bdaed65730210edd6fdac7e0aef9622ffd155e6186e40b4ec5d",
        "8c292b1111766b11fa10b87f061c66698f04e6e1ec1734ccb390f96c3e9df981",
    ),
    (
        "928bd005c599e22304a47919881893758b5500f358d62f93c339f8bccea4f4d5",
        "1647abd16addb82e00e00ebc8c5ad7b9c9863479702f28c75f2b28363bb514b9",
    ),
    (
        "1352a148e6c29fefe80ae6b2e1d3ad51390966494bd831722886f14cb327a1bc",
        "c7edc84ec8e4f9ccebe9b1e4c67a139e6f0661d256f20166b45af27a1b3dea0f",
    ),
    (
        "3d1da5501fd56223c11dba24e9c800d1683c18c00d3f1ddeeb5a81825087a809",
        "c40006056acf34d00f17395dad0a05c3712f6afa42d9bad41ae32e0830fc5d2a",
    ),
    (
        "67a61b9d238d10e978dfccc388f98019f6ae6ee73e349932709fb74b4b89754e",
        "5f3da78ed87d5418972e8c5b4a8fb571d60c9275c41a7c78c6109863bddc5e9e",
    ),
    (
        "67a61b9d238d10e978dfccc388f98019f6ae6ee73e349932709fb74b4b89754e",
        "6e965ef12ca68601861ef17f243464324b5a79802d57e9a94866be94372f4ebe",
    ),
    (
        "f693456e7cc6f6f1fda4a8d7fb0b7afb7a51b47f261d91899aa8a2ed53e99355",
        "47262c5f65cf96e1a36f3bc799f75bbb247ec1bf109ad2d88bce749093b940ce",
    ),
    (
        "312f3baf45baab325fb16315ac6fc0fa3ed042cd74f2f1ba1ec9cab67fb4c906",
        "aa71090e35eba16da8724081a27eead575495d670e864a119b7a467ebf4c5bb0",
    ),
    (
        "02e28d47a99c2da1eba2a6f7d60d15d8285e25aec51d767463cb249fa7d01fdb",
        "058f4e244e6354950e4a37cf4535254567852982810d64d4f33819aee236a505",
    ),
    // Rainbow Six: STREXPAND preserva BCAD/B1E2/B3E2; DrawText recebe AECHAR nativo.
    (
        "e0192d2358038eb19b61d6d9409341f66aa03f8a65692482578584e259b81c44",
        "988ff5106202457cbe605115a3a2684872a7f022975496d2b86718a9123a17dc",
    ),
    // Kang Ho-dong: dez linhas KSC5601 do aviso 19+ conferidas no guest.
    (
        "68274af9696982310270a99d3909502567647bc4a174e88786b7ff3198ec9268",
        "43129338d05ef8934842905db3fead2121f3165772478b49a3011a18411cd688",
    ),
];

impl NativeTextContext {
    pub(super) fn matches_identity(&self, module: &str, mif: &str) -> bool {
        self.identity
            .as_ref()
            .is_some_and(|(m, f)| m == module && f == mif)
    }

    fn for_identity(module: &str, mif: &str) -> Self {
        let hash =
            |value: &str| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit());
        if !hash(module) {
            return Self::default();
        }
        let (module, mif) = (module.to_ascii_lowercase(), mif.to_ascii_lowercase());
        // O factory deste módulo foi auditado sem manifesto. Vazio significa
        // ausência real; nenhum outro módulo ganha perfil sem os dois hashes.
        if module == SSA_WITHOUT_MANIFEST && mif.is_empty() {
            return Self {
                encoding: NativeTextEncoding::PackedEucKr,
                identity: Some((module, mif)),
            };
        }
        if !hash(&mif) {
            return Self::default();
        }
        let encoding = if PACKED_PAIRS
            .iter()
            .any(|&(m, f)| m == module.as_str() && f == mif.as_str())
        {
            NativeTextEncoding::PackedEucKr
        } else if KOREAN_PAIRS
            .iter()
            .any(|&(m, f)| m == module.as_str() && f == mif.as_str())
        {
            NativeTextEncoding::EucKr
        } else {
            NativeTextEncoding::Unicode
        };
        Self {
            encoding,
            identity: Some((module, mif)),
        }
    }
}

impl<C: CpuBackend> Machine<C> {
    /// O launcher fornece identidades verificadas do pacote antes de entrar no applet.
    /// A resposta indica se o par exige AECHAR nativo; um par desconhecido conserva Unicode.
    /// MIF vazio só seleciona o perfil do executável auditado sem manifesto.
    pub fn set_native_text_identity(&mut self, module_sha256: &str, mif_sha256: &str) -> bool {
        self.native_text = NativeTextContext::for_identity(module_sha256, mif_sha256);
        self.native_text.encoding != NativeTextEncoding::Unicode
    }

    pub fn native_text_encoding(&self) -> NativeTextEncoding {
        self.native_text.encoding
    }

    pub(super) fn native_device_encoding(&self) -> u16 {
        match self.native_text.encoding {
            NativeTextEncoding::Unicode => AEE_ENC_ISOLATIN1,
            NativeTextEncoding::EucKr | NativeTextEncoding::PackedEucKr => 0xfe,
        }
    }

    /// As APIs continuam contando palavras AECHAR; só este perfil usa byte NUL.
    pub(super) fn read_display_text_units(
        &self,
        address: u32,
        count: i32,
    ) -> Result<Vec<u16>, CpuError> {
        if self.native_text.encoding != NativeTextEncoding::PackedEucKr {
            let mut units = self.read_aechar_units(address)?;
            if count >= 0 {
                units.truncate(count as usize);
            }
            return Ok(units);
        }
        if address == 0 {
            return Ok(Vec::new());
        }
        let words = if count < 0 {
            MAX_STRING
        } else {
            (count as usize).min(MAX_STRING)
        };
        let mut bytes = Vec::new();
        // SSA passa char* sem IDF_TEXT_FORMAT_OEM e copia os mesmos bytes
        // por WSTRCPY. Ler blocos largos atravessaria o NUL de byte, inclusive
        // quando ele está na metade alta da última palavra ou no fim da região.
        for offset in 0..words * 2 {
            let at = address.checked_add(offset as u32).ok_or_else(|| {
                CpuError("texto nativo compacto: endereço fora da memória".into())
            })?;
            let mut byte = [0];
            self.cpu.read_mem(at, &mut byte)?;
            if byte[0] == 0 {
                break;
            }
            bytes.push(byte[0]);
        }
        Ok(bytes
            .chunks(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair.get(1).copied().unwrap_or(0)]))
            .collect())
    }

    pub(super) fn decode_guest_aechar_units(&self, units: &[u16]) -> Option<String> {
        match self.native_text.encoding {
            NativeTextEncoding::Unicode => Some(String::from_utf16_lossy(units)),
            NativeTextEncoding::EucKr => {
                // O guest mantém um AECHAR por caractere. Os dois bytes KSC5601
                // ficam na ordem da memória ARM; não são um escalar Unicode.
                let mut bytes = Vec::with_capacity(units.len().checked_mul(2)?);
                for &unit in units {
                    if unit <= 0x7f {
                        bytes.push(unit as u8);
                    } else {
                        bytes.extend_from_slice(&unit.to_le_bytes());
                    }
                }
                EUC_KR
                    .decode_without_bom_handling_and_without_replacement(&bytes)
                    .map(|text| text.into_owned())
            }
            NativeTextEncoding::PackedEucKr => {
                let bytes = units
                    .iter()
                    .flat_map(|unit| unit.to_le_bytes())
                    .take_while(|byte| *byte != 0)
                    .collect::<Vec<_>>();
                EUC_KR
                    .decode_without_bom_handling_and_without_replacement(&bytes)
                    .map(|text| text.into_owned())
            }
        }
    }

    pub(super) fn encode_guest_aechar_units(&self, unicode: &[u16]) -> Option<Vec<u16>> {
        match self.native_text.encoding {
            NativeTextEncoding::Unicode => Some(unicode.to_vec()),
            // Não houve LoadResString no caminho auditado de SSA. Falhar sem
            // escrever é preferível a inventar como um BAR largo vira char*.
            NativeTextEncoding::PackedEucKr => None,
            NativeTextEncoding::EucKr => {
                let text = String::from_utf16(unicode).ok()?;
                let (bytes, _, errors) = EUC_KR.encode(&text);
                if errors {
                    return None;
                }
                let mut bytes = bytes.iter().copied();
                let mut units = Vec::with_capacity(unicode.len());
                while let Some(first) = bytes.next() {
                    units.push(if first <= 0x7f {
                        u16::from(first)
                    } else {
                        u16::from_le_bytes([first, bytes.next()?])
                    });
                }
                Some(units)
            }
        }
    }

    pub(super) fn grava_texto_nativo(&self, sections: &mut crate::save_state::Secoes) {
        sections.poe(
            "text.native",
            serde_json::to_vec(&SavedNativeText {
                version: 1,
                context: self.native_text.clone(),
            })
            .expect("contexto de texto serializável"),
        );
    }

    pub(super) fn restaura_texto_nativo(
        &mut self,
        reader: &crate::save_state::Leitor<'_>,
    ) -> Result<(), crate::save_state::Erro> {
        let invalid = |reason: &str| crate::save_state::Erro::Secao {
            nome: "text.native".into(),
            motivo: reason.into(),
        };
        let Some(bytes) = reader.secao("text.native") else {
            // Um save anterior ao perfil conserva a identidade verificada no lançamento.
            return Ok(());
        };
        if bytes.len() > 1024 {
            return Err(invalid("contexto de texto acima do limite"));
        }
        let saved: SavedNativeText =
            serde_json::from_slice(bytes).map_err(|_| invalid("contexto de texto inválido"))?;
        let canonical = match saved.context.identity.as_ref() {
            Some((module, mif)) => NativeTextContext::for_identity(module, mif),
            None => NativeTextContext::default(),
        };
        if saved.version != 1
            || saved.context != canonical
            || saved.context.identity != self.native_text.identity
        {
            return Err(invalid(
                "codificação ou identidade do pacote não corresponde ao lançamento",
            ));
        }
        self.native_text = saved.context;
        Ok(())
    }
}
