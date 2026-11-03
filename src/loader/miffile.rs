//! Parser do `.mif` — o Module Information File, que descreve o que um módulo BREW oferece.
//!
//! O formato não é público. O que está aqui foi levantado comparando 24 arquivos reais: os
//! `.mif` do BREW SDK 4.0.2 (cujos ClassIDs conhecemos pelos `.bid`), os módulos de sistema do
//! simulador, os homebrew do OpenZeebo e os títulos do console.
//!
//! Estrutura confirmada:
//!
//! ```text
//! 0x00  u16 magic = 0x0011
//! 0x02  u16 versão = 1
//! 0x04  u16 (sempre 1)
//! 0x06  u16 número de entradas do índice
//! 0x08  u32 offset do índice          (0x20 em todos os arquivos vistos)
//! 0x0c  u32 tamanho do índice         (entradas * 8)
//! 0x10  u32 offset da tabela de seções
//! 0x14  u32 número de seções
//! 0x18  u32 offset da primeira seção
//! 0x1c  u32 tamanho total das seções
//! ```
//!
//! A tabela de seções tem `n + 1` offsets: cada seção vai de `offs[i]` a `offs[i+1]`, e o
//! último valor é o tamanho do arquivo.
//!
//! O registro de applet tem **20 bytes** e começa com o `AEECLSID`. Em todos os `.mif` de
//! applet examinados existe exatamente uma seção desse tamanho. A regra foi validada contra
//! valores conhecidos por outra fonte: `mediaplayer.mif` devolve `0x01010EF6`, o mesmo ClassID
//! que a engenharia reversa da firmware do Zeebo registrou para o Media Player, e
//! `274755.mif` (o Z-Wheel) devolve `0x01070798`, o App ID que a firmware chama de "TECTOY".

use std::{collections::HashSet, fmt};

const MAGIC: u16 = 0x0011;
/// Tamanho do registro que descreve um applet.
const APPLET_RECORD_LEN: u32 = 20;
/// Forma legada de uma lista com uma classe: `<u32 ClassID> <u32 zero>`.
///
/// Ele aparece nos dois lados da mesma relação, com **bytes idênticos**: no `.mif` do jogo, que
/// declara a classe de que depende, e no `.mif` do módulo que a fornece. Quem distingue não é o
/// registro, é o arquivo em volta — ver [`MifFile::extensao`].
const CLASS_RECORD_LEN: u32 = 8;
const RESTYPE_BINARY: u16 = 0x5000;
/// Palavras do registro de applet que são sempre zero.
///
/// O registro é `ClassID`, um zero, um número, outro zero e um campo que varia. São esses dois
/// zeros que separam um registro de applet de outra seção que por acaso também tenha 20 bytes.
const APPLET_ZEROS: [usize; 2] = [4, 12];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MifFile {
    /// ClassIDs dos applets que o módulo expõe.
    pub applets: Vec<u32>,
    /// ClassIDs citados nos registros de classe: o que o módulo fornece, quando ele é uma
    /// extensão, ou o que ele exige, quando é um applet.
    pub classes: Vec<u32>,
    /// Limites de cada seção, como `(offset, tamanho)`.
    pub sections: Vec<(u32, u32)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MifError {
    BadMagic { found: u16 },
    Truncated { needed: usize, len: usize },
}

impl fmt::Display for MifError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic { found } => write!(f, "não parece um .mif (magic {found:#06x})"),
            Self::Truncated { needed, len } => {
                write!(f, ".mif truncado: precisa de {needed} bytes, tem {len}")
            }
        }
    }
}

impl std::error::Error for MifError {}

impl MifFile {
    pub fn parse(data: &[u8]) -> Result<Self, MifError> {
        let magic = read_u16(data, 0)?;
        if magic != MAGIC {
            return Err(MifError::BadMagic { found: magic });
        }

        let table_offset = read_u32(data, 0x10)? as usize;
        let section_count = read_u32(data, 0x14)? as usize;

        // A tabela guarda n+1 offsets: o extra fecha a última seção.
        let mut bounds = Vec::with_capacity(section_count + 1);
        for i in 0..=section_count {
            bounds.push(read_u32(data, table_offset + i * 4)?);
        }

        // Em todos os arquivos reais examinados o último limite é exatamente o tamanho do
        // arquivo. Exigir isso evita que um `.mif` truncado passe adiante e simplesmente não
        // reporte applet nenhum.
        if let Some(&last) = bounds.last() {
            if last as usize > data.len() {
                return Err(MifError::Truncated {
                    needed: last as usize,
                    len: data.len(),
                });
            }
        }

        let sections: Vec<(u32, u32)> = bounds
            .windows(2)
            .map(|w| (w[0], w[1].saturating_sub(w[0])))
            .collect();

        let applets: Vec<u32> = sections
            .iter()
            .filter(|&&(_, len)| len == APPLET_RECORD_LEN)
            .filter(|&&(offset, _)| is_applet_record(data, offset as usize))
            .filter_map(|&(offset, _)| read_u32(data, offset as usize).ok())
            .filter(|&id| id != 0)
            .collect();

        let index_count = read_u16(data, 6)? as usize;
        let classes = if index_count == 0 {
            // Fixtures e manifestos legados sem índice conservam a forma conhecida.
            // Um índice presente, porém inválido, nunca cai nesta inferência por tamanho.
            sections
                .iter()
                .filter(|&&(_, len)| len == CLASS_RECORD_LEN)
                .filter_map(|&(offset, _)| {
                    let clsid = read_u32(data, offset as usize).ok()?;
                    let resto = read_u32(data, offset as usize + 4).ok()?;
                    (clsid != 0 && resto == 0).then_some(clsid)
                })
                .collect()
        } else {
            indexed_classes(data, &sections, index_count, !applets.is_empty()).unwrap_or_default()
        };

        Ok(Self {
            applets,
            classes,
            sections,
        })
    }

    /// As imagens do módulo — os ícones do título —, na ordem em que aparecem.
    ///
    /// Uma seção de imagem começa com `u16` de comprimento **do próprio cabeçalho**, seguido do
    /// tipo MIME terminado em zero; o arquivo vem logo depois. Os `.mif` reais gravam sempre 12
    /// e `image/png`, `image/bmp` ou `image/jpg`.
    pub fn images<'d>(&self, data: &'d [u8]) -> Vec<&'d [u8]> {
        self.sections
            .iter()
            .filter_map(|&(offset, len)| {
                let section = data.get(offset as usize..(offset + len) as usize)?;
                let header = u16::from_le_bytes([*section.first()?, *section.get(1)?]) as usize;
                let mime = section.get(2..header)?.split(|&b| b == 0).next()?;
                mime.starts_with(b"image/")
                    .then(|| section.get(header..))
                    .flatten()
            })
            .collect()
    }

    /// ClassID do applet principal — o que o emulador instancia.
    pub fn main_applet(&self) -> Option<u32> {
        self.applets.first().copied()
    }

    /// As classes que este módulo **fornece**, quando ele é um módulo de extensão.
    ///
    /// Um módulo de extensão não tem applet: ele existe só para exportar classes, e o console o
    /// carrega quando alguém pede uma delas. Dois títulos que temos dependem disso, e em ambos o
    /// módulo da extensão vem dentro do próprio pacote:
    ///
    /// | título | módulo | classe |
    /// |---|---|---|
    /// | Action Hero 3D | `mod/12875/imicro3d.mod` | `0x010292c3` |
    /// | Kingdom Hearts | `Kingdon Hearts/kh.mod` | `0x0102bbfc` |
    ///
    /// O registro de classe é o mesmo nos dois `.mif` da relação, então "ter applet" é o que
    /// separa quem pede de quem fornece. Sem essa distinção, o `.mif` do jogo se ofereceria
    /// para atender a classe que ele próprio está pedindo.
    pub fn extensao(&self) -> &[u32] {
        if self.applets.is_empty() {
            &self.classes
        } else {
            &[]
        }
    }
}

/// Listas binárias 1 (exportações) e 3 (dependências), não qualquer seção de mesmo tamanho.
/// No corpus local, 402 listas têm `(contagem + 1) * 4` bytes e NUL final. No ModInfo
/// 0x1000/0x1002 de 40 bytes, as contagens uint16 ficam em +26 e +30, respectivamente.
/// Ler só oito bytes descartava as duas dependências reais da lista de 12 bytes.
fn indexed_classes(
    data: &[u8],
    sections: &[(u32, u32)],
    index_count: usize,
    has_applet: bool,
) -> Option<Vec<u32>> {
    let offset = read_u32(data, 8).ok()? as usize;
    if offset < 32 {
        return None;
    }
    let end = offset.checked_add(index_count.checked_mul(8)?)?;
    let index = data.get(offset..end)?;
    // O mesmo módulo pode oferecer serviços e também consumir outros. A lista de
    // quem fornece não pode virar uma dependência, nem uma importação virar exportação.
    let (resource_id, count_offset) = if has_applet { (3, 30) } else { (1, 26) };
    let payload = indexed_binary(data, sections, index, resource_id)?;
    if payload.len() < 4 || payload.len() % 4 != 0 {
        return None;
    }
    // O formato0x1000 do bf2gen e do Texas Hold'em conserva estes campos.
    // Ignorá-lo apagava tanto as dezesseis exportações quanto as seis importações,
    // impedindo que a busca de extensões encontrasse o provedor original da Summus.
    let declared = indexed_binary(data, sections, index, 0)
        .filter(|info| info.len() == 40 && matches!(read_u32(info, 0), Ok(0x1000 | 0x1002)))
        .and_then(|info| read_u16(info, count_offset).ok());
    match declared {
        Some(count) if payload.len() == (usize::from(count) + 1) * 4 => {}
        // A forma de uma única classe já era aceita antes deste formato de ModInfo.
        None if payload.len() == CLASS_RECORD_LEN as usize => {}
        _ => return None,
    }
    let prefix = payload.len() - 4;
    if read_u32(payload, prefix).ok()? != 0 {
        return None;
    }
    let mut seen = HashSet::new();
    let mut classes = Vec::new();
    for bytes in payload[..prefix].chunks_exact(4) {
        let class = u32::from_le_bytes(bytes.try_into().ok()?);
        if class == 0 {
            // Não anunciar uma lista parcial quando o terminador contradiz a contagem.
            return None;
        }
        if seen.insert(class) {
            classes.push(class);
        }
    }
    Some(classes)
}

fn indexed_binary<'a>(
    data: &'a [u8],
    sections: &[(u32, u32)],
    index: &[u8],
    id: u16,
) -> Option<&'a [u8]> {
    for entry in index.chunks_exact(8) {
        if read_u16(entry, 0).ok()? != RESTYPE_BINARY {
            continue;
        }
        let first = read_u16(entry, 2).ok()?;
        let Some(delta) = id.checked_sub(first) else {
            continue;
        };
        if delta > read_u16(entry, 4).ok()? {
            continue;
        }
        let section = usize::from(read_u16(entry, 6).ok()?).checked_add(usize::from(delta))?;
        let &(offset, length) = sections.get(section)?;
        let start = offset as usize;
        return data.get(start..start.checked_add(length as usize)?);
    }
    None
}

/// Se a seção de 20 bytes em `offset` tem a forma de um registro de applet.
///
/// A regra era outra: exigir que o ClassID caísse na faixa `0x01xxxxxx` da Qualcomm. Ela
/// funcionava para 62 dos 67 registros dos jogos que temos, e deixava de fora o Zenonia
/// (`0xbf2e2021`) — que não abria por isso. A forma acerta os 67: pega os 62 de antes, mais o
/// Zenonia e um homebrew de ClassID `0x12345678`, e continua recusando as três seções de 20
/// bytes que não são applet nenhum (Prey Evil, Reckless Racing e Zeebo App, todas começando em
/// `0x00xxfeff`).
fn is_applet_record(data: &[u8], offset: usize) -> bool {
    APPLET_ZEROS
        .iter()
        .all(|&campo| read_u32(data, offset + campo) == Ok(0))
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16, MifError> {
    let bytes = slice(data, offset, 2)?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, MifError> {
    let b = slice(data, offset, 4)?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn slice(data: &[u8], offset: usize, len: usize) -> Result<&[u8], MifError> {
    let end = offset.checked_add(len).ok_or(MifError::Truncated {
        needed: len,
        len: data.len(),
    })?;
    data.get(offset..end).ok_or(MifError::Truncated {
        needed: end,
        len: data.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_imagens_saem_das_secoes_que_anunciam_um_tipo_mime() {
        // O cabeçalho conta o próprio tamanho: ler o comprimento como se fosse só do texto
        // deixaria dois bytes do arquivo para trás e nenhum decodificador aceitaria a imagem.
        let mut section = Vec::new();
        section.extend_from_slice(&12u16.to_le_bytes());
        section.extend_from_slice(b"image/png\0");
        section.extend_from_slice(b"CONTEUDO");

        let mut outra = Vec::new();
        outra.extend_from_slice(&12u16.to_le_bytes());
        outra.extend_from_slice(b"text/plain");
        outra.extend_from_slice(b"nao e imagem");

        let mut data = vec![0u8; 4];
        let mut sections = Vec::new();
        for part in [&section, &outra] {
            sections.push((data.len() as u32, part.len() as u32));
            data.extend_from_slice(part);
        }
        let mif = MifFile {
            applets: Vec::new(),
            classes: Vec::new(),
            sections,
        };
        assert_eq!(mif.images(&data), vec![b"CONTEUDO".as_slice()]);
    }

    #[test]
    fn secao_truncada_nao_vira_imagem() {
        let mif = MifFile {
            applets: Vec::new(),
            classes: Vec::new(),
            sections: vec![(0, 40)],
        };
        assert!(mif.images(&[0u8; 4]).is_empty());
    }

    /// Monta um `.mif` mínimo com duas seções, sendo a segunda o registro de applet.
    fn mif_with_applet(clsid: u32) -> Vec<u8> {
        let table_offset = 0x20u32;
        let sections = [0x40u32, 0x50, 0x64];
        let mut data = vec![0u8; 0x64];
        data[0..2].copy_from_slice(&MAGIC.to_le_bytes());
        data[2..4].copy_from_slice(&1u16.to_le_bytes());
        data[4..6].copy_from_slice(&1u16.to_le_bytes());
        data[6..8].copy_from_slice(&2u16.to_le_bytes());
        data[0x10..0x14].copy_from_slice(&table_offset.to_le_bytes());
        data[0x14..0x18].copy_from_slice(&2u32.to_le_bytes());
        for (i, offset) in sections.iter().enumerate() {
            let at = table_offset as usize + i * 4;
            data[at..at + 4].copy_from_slice(&offset.to_le_bytes());
        }
        // Seção 1: 0x40..0x50 (16 bytes, não é applet). Seção 2: 0x50..0x64 (20 bytes).
        data[0x50..0x54].copy_from_slice(&clsid.to_le_bytes());
        data
    }

    /// Monta um `.mif` mínimo com três seções de 8 bytes: um registro de classe, um zerado e
    /// mais um. É a forma dos dois `.mif` de extensão reais, que trazem o registro seguido de
    /// uma seção de 8 bytes toda zerada.
    fn mif_de_extensao(clsid: u32, applet: Option<u32>) -> Vec<u8> {
        let table_offset = 0x20u32;
        let sections = [0x40u32, 0x48, 0x50, 0x64];
        let mut data = vec![0u8; 0x64];
        data[0..2].copy_from_slice(&MAGIC.to_le_bytes());
        data[0x10..0x14].copy_from_slice(&table_offset.to_le_bytes());
        data[0x14..0x18].copy_from_slice(&3u32.to_le_bytes());
        for (i, offset) in sections.iter().enumerate() {
            let at = table_offset as usize + i * 4;
            data[at..at + 4].copy_from_slice(&offset.to_le_bytes());
        }
        // 0x40..0x48: o registro de classe. 0x48..0x50: os oito bytes zerados.
        data[0x40..0x44].copy_from_slice(&clsid.to_le_bytes());
        // 0x50..0x64: vinte bytes, que só viram applet se tiverem a forma de um.
        if let Some(applet) = applet {
            data[0x50..0x54].copy_from_slice(&applet.to_le_bytes());
        } else {
            // Quebra a forma do registro de applet pondo valor num dos campos que são zero.
            data[0x54..0x58].copy_from_slice(&1u32.to_le_bytes());
        }
        data
    }

    /// O registro de 8 bytes é o que nomeia a classe de uma extensão. O de oito bytes **zerado**
    /// que vem depois dele nos arquivos reais não pode entrar na lista: daria um `0x0` que se
    /// ofereceria para atender qualquer pedido de ClassID zero.
    #[test]
    fn o_registro_de_classe_sai_e_o_zerado_nao() {
        let mif = MifFile::parse(&mif_de_extensao(0x0102_92c3, None)).unwrap();
        assert_eq!(mif.classes, vec![0x0102_92c3]);
    }

    /// **A regra que separa quem fornece de quem pede.** Os dois `.mif` da relação trazem o
    /// mesmo registro, com os mesmos bytes: o do jogo declarando a dependência e o da extensão
    /// declarando o que fornece. Sem "ter applet" como critério, o manifesto do Action Hero 3D
    /// se ofereceria para atender a `0x010292c3` que ele próprio está pedindo — e o emulador
    /// chamaria o `AEEMod_Load` do jogo de novo, de dentro do despacho, para criá-la.
    #[test]
    fn so_o_mif_sem_applet_se_oferece_para_fornecer_a_classe() {
        let extensao = MifFile::parse(&mif_de_extensao(0x0102_92c3, None)).unwrap();
        assert_eq!(extensao.extensao(), &[0x0102_92c3]);

        let jogo = MifFile::parse(&mif_de_extensao(0x0102_92c3, Some(0x0108_1970))).unwrap();
        assert_eq!(jogo.applets, vec![0x0108_1970], "este é o do jogo");
        assert_eq!(jogo.classes, vec![0x0102_92c3], "e ele cita a mesma classe");
        assert!(
            jogo.extensao().is_empty(),
            "mas não se oferece para fornecê-la"
        );
    }

    #[test]
    fn extrai_o_classid_do_applet() {
        let mif = MifFile::parse(&mif_with_applet(0x0100_9ff0)).unwrap();
        assert_eq!(mif.applets, vec![0x0100_9ff0]);
        assert_eq!(mif.main_applet(), Some(0x0100_9ff0));
    }

    /// O Zenonia declara o applet dele com ClassID `0xbf2e2021`, e um homebrew do conjunto usa
    /// `0x12345678`. A regra antiga exigia a faixa `0x01xxxxxx` da Qualcomm e recusava os dois:
    /// o Zenonia não abria por causa disso.
    #[test]
    fn aceita_classid_fora_da_faixa_da_qualcomm() {
        let mif = MifFile::parse(&mif_with_applet(0xbf2e_2021)).unwrap();
        assert_eq!(mif.main_applet(), Some(0xbf2e_2021));
    }

    /// Nem toda seção de 20 bytes é applet: o Prey Evil, o Reckless Racing e o Zeebo App têm
    /// uma que começa em `0x00xxfeff` e não é. O que as separa é a forma — o registro de applet
    /// tem zeros no segundo e no quarto campo.
    #[test]
    fn ignora_secao_de_20_bytes_sem_forma_de_applet() {
        let mut data = mif_with_applet(0x0050_feff);
        data[0x54..0x58].copy_from_slice(&0x1234_5678u32.to_le_bytes());
        let mif = MifFile::parse(&data).unwrap();
        assert!(mif.applets.is_empty());
        assert_eq!(mif.main_applet(), None);
    }

    #[test]
    fn le_os_limites_das_secoes() {
        let mif = MifFile::parse(&mif_with_applet(0x0100_9ff0)).unwrap();
        assert_eq!(mif.sections, vec![(0x40, 0x10), (0x50, 20)]);
    }

    #[test]
    fn recusa_arquivo_com_magic_errado() {
        let mut data = mif_with_applet(0x0100_9ff0);
        data[0] = 0xff;
        assert!(matches!(
            MifFile::parse(&data),
            Err(MifError::BadMagic { .. })
        ));
    }

    #[test]
    fn recusa_arquivo_truncado() {
        let data = mif_with_applet(0x0100_9ff0);
        assert!(matches!(
            MifFile::parse(&data[..0x30]),
            Err(MifError::Truncated { .. })
        ));
    }
}
#[cfg(test)]
mod legacy_modinfo_contracts {
    use super::MifFile;

    // O bf2gen da Summus usa ModInfo0x1000 de40bytes e dezesseis exportações.
    // O Texas Hold'em usa a mesma estrutura para as seis importações reais.
    fn indexed(version: u32, classes: &[u32], applet: bool) -> Vec<u8> {
        let count = if applet {3usize} else {2};
        let table = 0x20 + count * 8;
        let start = table + (count + 1) * 4;
        let mut info = vec![0u8;40];
        info[..4].copy_from_slice(&version.to_le_bytes());
        let at = if applet {30} else {26};
        info[at..at+2].copy_from_slice(&(classes.len() as u16).to_le_bytes());
        let mut list: Vec<u8> = classes.iter().chain(std::iter::once(&0)).flat_map(|id|id.to_le_bytes()).collect();
        let mut parts = vec![info,std::mem::take(&mut list)];
        if applet { let mut record=vec![0u8;20];record[..4].copy_from_slice(&0x0101f197u32.to_le_bytes());parts.push(record); }
        let total = start + parts.iter().map(Vec::len).sum::<usize>();
        let mut data=vec![0u8;total];
        data[..2].copy_from_slice(&0x11u16.to_le_bytes());
        data[2..4].copy_from_slice(&1u16.to_le_bytes());
        data[6..8].copy_from_slice(&(count as u16).to_le_bytes());
        data[8..12].copy_from_slice(&0x20u32.to_le_bytes());
        data[12..16].copy_from_slice(&((count*8) as u32).to_le_bytes());
        data[16..20].copy_from_slice(&(table as u32).to_le_bytes());
        data[20..24].copy_from_slice(&(count as u32).to_le_bytes());
        let ids = if applet {vec![0u16,3,2]} else {vec![0u16,1]};
        let mut cursor=start;
        for (i,part) in parts.iter().enumerate() {
            let at=0x20+i*8;
            data[at..at+2].copy_from_slice(&0x5000u16.to_le_bytes());
            data[at+2..at+4].copy_from_slice(&ids[i].to_le_bytes());
            data[at+6..at+8].copy_from_slice(&(i as u16).to_le_bytes());
            data[table+i*4..table+i*4+4].copy_from_slice(&(cursor as u32).to_le_bytes());
            data[cursor..cursor+part.len()].copy_from_slice(part);cursor+=part.len();
        }
        data[table+count*4..table+count*4+4].copy_from_slice(&(cursor as u32).to_le_bytes());
        data
    }

    #[test]
    fn modinfo_1000_preserva_dezesseis_exportacoes() {
        let classes:Vec<_>=(0x010179be..0x010179ce).collect();
        let mif=MifFile::parse(&indexed(0x1000,&classes,false)).unwrap();
        assert_eq!(mif.extensao(),classes);
    }
    #[test]
    fn modinfo_1000_preserva_importacoes_sem_exportar_o_applet() {
        let classes=[0x010179c8,0x010179c7,0x010179c3,0x010179c1,0x010179c0,0x010179be];
        let mif=MifFile::parse(&indexed(0x1000,&classes,true)).unwrap();
        assert_eq!(mif.classes,classes);assert!(mif.extensao().is_empty());
    }
    #[test]
    fn modinfo_1002_conserva_a_lista_com_contagem() {
        let classes=[0x010179c1,0x010179c0];
        assert_eq!(MifFile::parse(&indexed(0x1002,&classes,false)).unwrap().extensao(),classes);
    }
    #[test]
    fn modinfo_1000_recusa_contagem_divergente_e_lista_sem_nul() {
        let mut data=indexed(0x1000,&[0x010179c1,0x010179c0],false);
        let table=u32::from_le_bytes(data[16..20].try_into().unwrap()) as usize;
        let info=u32::from_le_bytes(data[table..table+4].try_into().unwrap()) as usize;
        data[info+26..info+28].copy_from_slice(&3u16.to_le_bytes());
        assert!(MifFile::parse(&data).unwrap().classes.is_empty());
        data[info+26..info+28].copy_from_slice(&2u16.to_le_bytes());
        let end=data.len();data[end-4..].copy_from_slice(&0x010179beu32.to_le_bytes());
        assert!(MifFile::parse(&data).unwrap().classes.is_empty());
    }
    #[test]
    fn versao_desconhecida_nao_autoriza_lista_de_multiplas_classes() {
        assert!(MifFile::parse(&indexed(0x1001,&[0x010179c1,0x010179c0],false)).unwrap().classes.is_empty());
    }
}

