//! A lista de jogos que o emulador encontra numa pasta.
//!
//! Duas disposições convivem. A do console instala cada título como
//! `<Título>/mod/<id>/<nome>.mod`, com o `.mif` num `mif/` irmão; os exemplos do SDK deixam o
//! `.mod` e o `.mif` lado a lado. Como as duas aparecem na prática, a varredura procura
//! `.mod` em qualquer profundidade razoável e deduz o título de onde o arquivo está.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::loader::archive;
use crate::video::icon::{self, Image};
use crate::loader::miffile::MifFile;
use crate::config;

/// Extensões aceitas para uma capa deixada ao lado do jogo.
const COVER_EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "bmp"];

/// Oito níveis permitem agrupar coleções sem varrer árvores de profundidade arbitrária.
const MAX_DEPTH: usize = 8;

/// Teto de jogos por varredura, para uma pasta escolhida sem querer não travar a interface.
const MAX_GAMES: usize = 2000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Game {
    /// Como o jogo aparece na lista.
    pub title: String,
    /// O `.mod` a carregar, ou o `.zip` que o contém.
    pub path: PathBuf,
    /// O ClassID do applet, lido do manifesto inclusive dentro de pacotes `.zip`.
    pub clsid: Option<u32>,
    /// Se o caminho é um pacote que precisa ser extraído antes de rodar.
    pub packed: bool,
    /// A imagem que representa o jogo na biblioteca. Vem de uma capa deixada ao lado do
    /// arquivo, ou, na falta dela, do maior ícone que o `.mif` guarda.
    pub art: Option<Image>,
}

/// De onde veio a informação de um título no catálogo do console.
///
/// A NAND descreve o aparelho como ele saiu de fábrica; ROMs locais completam esse catálogo
/// com títulos instalados pelo usuário, inclusive homebrews. A origem evita que uma nova
/// varredura apague registros que extraímos da firmware.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogSource {
    Nand,
    Rom,
}

/// Um título que o console pode apresentar ou iniciar.
///
/// O ClassID é a chave: nomes de arquivos e de ZIPs são livres, mas é esse valor do `.mif` que
/// `ISHELL_StartApplet` recebe e que a Z-Wheel grava nas tabelas dela.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub applet_class: u32,
    pub title: String,
    pub rom_path: PathBuf,
    pub source: CatalogSource,
}

/// Índice persistente que une o inventário da NAND às ROMs escolhidas pelo usuário.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CatalogIndex {
    pub titles: Vec<CatalogEntry>,
}

impl CatalogIndex {
    /// Incorpora a lista de ROMs atual sem mexer nos títulos que vieram da NAND.
    pub fn refresh_roms(&mut self, games: &[Game]) {
        self.titles.retain(|entry| entry.source != CatalogSource::Rom);
        self.titles.extend(games.iter().filter_map(|game| {
            Some(CatalogEntry {
                applet_class: game.clsid?,
                title: game.title.clone(),
                rom_path: game.path.clone(),
                source: CatalogSource::Rom,
            })
        }));
        // Dois pacotes podem anunciar o mesmo applet. Manter o primeiro, em ordem de título e
        // caminho, torna o resultado repetível e deixa a pessoa resolver a duplicata na pasta.
        self.titles.sort_by(|a, b| {
            a.applet_class
                .cmp(&b.applet_class)
                .then(a.source.cmp(&b.source))
                .then(a.rom_path.cmp(&b.rom_path))
        });
        self.titles.dedup_by_key(|entry| entry.applet_class);
    }

    pub fn load_from(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }
}

/// Caminho estável do catálogo, separado dos ZIPs e de seus caches de extração.
pub fn catalog_path() -> PathBuf {
    config::config_dir().join("catalog.json")
}

/// Atualiza e persiste os títulos encontrados na pasta configurada.
pub fn sync_catalog(games: &[Game]) -> std::io::Result<CatalogIndex> {
    let mut index = CatalogIndex::load_from(&catalog_path());
    index.refresh_roms(games);
    index.save_to(&catalog_path())?;
    Ok(index)
}

/// Procura jogos em `root`, em ordem de título.
pub fn scan(root: &Path) -> Vec<Game> {
    let mut games = Vec::new();
    scan_each(root, |game| games.push(game));
    games.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    games
}

/// Entrega um jogo por vez para consumidores que reduzem/descartam a capa imediatamente.
/// A lista de caminhos é limitada por collect; imagens não se acumulam antes do callback.
pub fn scan_each(root: &Path, mut receive: impl FnMut(Game)) {
    let index=crate::handset::mif_lookup::Index::for_root(root);
    let mut found = Vec::new();
    collect(root, 0, &mut found);
    for path in found { if let Some(game) = describe(path, &index) { receive(game); } }
}

/// O caminho que abre a Z-Wheel a partir do que a pessoa apontou: o `.zip`, o `.mod` ou uma
/// pasta que a contenha. `None` quando ali não há Z-Wheel — quem diz é o ClassID do `.mif`.
pub fn z_wheel_em(caminho: &Path) -> Option<PathBuf> {
    let z_wheel = Some(crate::session::Z_WHEEL);
    let index = crate::handset::mif_lookup::Index::for_root(caminho);
    if caminho.is_dir() {
        let mut achados = Vec::new();
        collect(caminho, 0, &mut achados);
        return achados
            .into_iter()
            .filter_map(|path| describe(path, &index))
            .find(|jogo| jogo.clsid == z_wheel)
            .map(|jogo| jogo.path);
    }
    describe(caminho.to_path_buf(), &index)
        .filter(|jogo| jogo.clsid == z_wheel)
        .map(|jogo| jogo.path)
}

/// De onde abrir a Z-Wheel: o caminho configurado vale primeiro; sem ele, uma Z-Wheel que esteja
/// entre os jogos da pasta de ROMs serve do mesmo jeito.
///
/// É a regra que as janelas usam para atender o pedido de voltar à Z-Wheel, e por isso mora aqui,
/// e não em cada uma delas.
pub fn z_wheel_de(configurada: Option<&Path>, jogos: &[Game]) -> Option<PathBuf> {
    configurada.and_then(z_wheel_em).or_else(|| {
        jogos
            .iter()
            .find(|jogo| jogo.clsid == Some(crate::session::Z_WHEEL))
            .map(|jogo| jogo.path.clone())
    })
}

/// Procura a Z-Wheel sem perguntar: primeiro entre os jogos da pasta de ROMs, depois ao lado
/// dela, em `Downloads` e na pasta pessoal, em tudo que tenha "wheel" ou "tectoy" no nome.
/// Olhar só esses nomes evita varrer o disco.
pub fn detecta_z_wheel(roms: Option<&Path>, jogos: &[Game]) -> Option<PathBuf> {
    if let Some(jogo) = jogos.iter().find(|jogo| jogo.clsid == Some(crate::session::Z_WHEEL)) {
        return Some(jogo.path.clone());
    }
    let casa = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    let pastas = [
        roms.map(Path::to_path_buf),
        roms.and_then(Path::parent).map(Path::to_path_buf),
        casa.as_ref().map(|casa| casa.join("Downloads")),
        casa,
    ];
    pastas.into_iter().flatten().find_map(|pasta| {
        let mut candidatos: Vec<PathBuf> = std::fs::read_dir(&pasta)
            .ok()?
            .flatten()
            .map(|entrada| entrada.path())
            .filter(|caminho| {
                caminho.file_name().and_then(|n| n.to_str()).is_some_and(|nome| {
                    let nome = nome.to_lowercase();
                    nome.contains("wheel") || nome.contains("tectoy")
                })
            })
            .collect();
        candidatos.sort();
        candidatos.iter().find_map(|caminho| z_wheel_em(caminho))
    })
}

/// Transforma um arquivo encontrado em jogo. `None` para o que não é jogo — um `.zip` sem
/// módulo dentro é só um zip.
fn describe(path: PathBuf, index:&crate::handset::mif_lookup::Index) -> Option<Game> {
    let packed = crate::loader::archive::embalado(&path);
    if !packed {
        let data=manifest_with_index(&path,index);
        let clsid = data.as_deref().and_then(|data|MifFile::parse(data).ok()).and_then(|mif|mif.main_applet())
            .or_else(||if index.shared {None}else{applet_clsid(&path)});
        // O catálogo legado procura outro MIF local quando o primeiro é de extensão.
        // Essa identificação do applet precisa acontecer antes de excluir a linha.
        if clsid.is_none() && data.as_deref().and_then(|b| crate::handset::package::parse_mif(b).ok())
            .is_some_and(|mif| !mif.extensao().is_empty()) { return None; }
        return Some(Game {
            title: title_for(&path),
            clsid,
            art: cover(&path).or_else(|| manifest_art(data.as_deref()?)),
            path,
            packed,
        });
    }
    let module = archive::find_module(&path)?;
    let manifest = archive::find_manifest(&path, &module);
    if manifest.as_deref().and_then(|b| crate::handset::package::parse_mif(b).ok())
        .is_some_and(|mif| !mif.extensao().is_empty()) { return None; }
    Some(Game {
        title: archive::title_of(&path, &module),
        clsid: manifest
            .as_deref()
            .and_then(|data| MifFile::parse(data).ok())
            .and_then(|mif| mif.main_applet()),
        art: cover(&path).or_else(|| manifest_art(manifest.as_deref()?)),
        path,
        packed,
    })
}

/// Uma imagem deixada ao lado do jogo, com o mesmo nome dele.
///
/// É a saída para quem quer uma capa de verdade: os `.mif` só guardam ícones de menu, que não
/// passam de 65×42, e nenhuma ROM traz arte maior que isso.
pub fn cover(path: &Path) -> Option<Image> {
    COVER_EXTENSIONS
        .iter()
        .map(|extension| path.with_extension(extension))
        .filter(|candidate| candidate != path)
        .filter_map(|candidate| std::fs::read(candidate).ok())
        .find_map(|data| icon::decode(&data).ok())
}

/// O maior ícone do `.mif` — o que rende a melhor imagem na tela.
pub fn manifest_art(data: &[u8]) -> Option<Image> {
    let mif = MifFile::parse(data).ok()?;
    mif.images(data)
        .into_iter()
        .filter_map(|image| icon::decode_mif(image).ok())
        .max_by_key(Image::pixels)
}

/// O conteúdo do `.mif` que acompanha um `.mod` solto no disco.
fn manifest(mod_path: &Path) -> Option<Vec<u8>> {
    manifest_paths(mod_path)
        .into_iter()
        .find_map(|path| std::fs::read(path).ok())
}

fn collect(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH || found.len() >= MAX_GAMES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    // Ordenar deixa a varredura repetível: `read_dir` não promete ordem nenhuma, e uma lista
    // que muda de posição entre duas aberturas é confusa para quem usa.
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        if found.len()>=MAX_GAMES {break;}
        let Ok(kind) = entry.file_type() else { continue; };
        // Links podem sair da pasta escolhida ou formar ciclos na coleção.
        if kind.is_symlink() { continue; }
        let path = entry.path();
        if kind.is_dir() {
            collect(&path, depth + 1, found);
        } else if kind.is_file() && (path.extension().is_some_and(|e|e.to_string_lossy().eq_ignore_ascii_case("mod"))
            || crate::loader::archive::embalado(&path))
        {
            found.push(path);
        }
    }
}

/// O nome a mostrar para um `.mod`.
///
/// No layout do console o nome do arquivo é um identificador numérico sem graça, e quem tem o
/// nome do jogo é a pasta do título, dois níveis acima. Fora desse layout, o nome do arquivo é
/// o que há.
/// O id do módulo de um jogo: a pasta em que o `.mod` mora, `mod/<id>/`. É também o nome do
/// `.mif` dele. Num pacote, sai do caminho interno, sem extrair.
pub fn id_do_modulo(path: &Path) -> Option<String> {
    let interno = match crate::loader::archive::embalado(path) {
        true => PathBuf::from(crate::loader::archive::find_module(path)?),
        false => path.to_path_buf(),
    };
    Some(interno.parent()?.file_name()?.to_str()?.to_string())
}

/// O nome com que as coisas de um jogo só são guardadas, como as configurações dele (ver
/// [`crate::ui::settings::ConfigDoJogo`]).
///
/// O ClassID do applet, quando o manifesto diz: ele é do jogo, e não do arquivo — renomear o
/// pacote ou mudá-lo de pasta não perde nada. Sem ele, o id do módulo, e por último o nome do
/// arquivo. Só caracteres que valem em nome de arquivo em qualquer sistema.
pub fn chave_do_jogo(path: &Path) -> Option<String> {
    let chave = match applet_clsid(path) {
        Some(classe) => format!("{classe:08x}"),
        None => id_do_modulo(path)
            .or_else(|| Some(path.file_stem()?.to_str()?.to_string()))?,
    };
    let limpa: String = chave
        .chars()
        .map(|c| match c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            true => c,
            false => '_',
        })
        .collect();
    (!limpa.is_empty()).then_some(limpa)
}

pub fn title_for(mod_path: &Path) -> String {
    let stem = mod_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let parent = mod_path.parent();
    let is_console_layout = parent
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        == Some("mod");
    if is_console_layout {
        if let Some(title) = parent
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
        {
            return title.to_string();
        }
    }
    stem.to_string()
}

/// O nome de uma pasta de extração sem a impressão digital do pacote.
///
/// `Zeebo-Extreme-Boia-Cross-21503726-1788761080` vira `Zeebo Extreme Boia Cross`: os dois
/// números do fim são tamanho e data do `.zip`, e os hífens tomaram o lugar dos espaços. A chave
/// de hoje é o digest BLAKE3 do conteúdo, 64 dígitos hexadecimais
/// (`Crash-Bandicoot-Nitro-Kart-3D-0c9aef…`), e sai do mesmo jeito.
pub fn sem_impressao_digital(nome: &str) -> String {
    let mut partes: Vec<&str> = nome.split('-').collect();
    if partes.len() > 1
        && partes
            .last()
            .is_some_and(|p| p.len() == 64 && p.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        partes.pop();
    }
    for _ in 0..2 {
        if partes.len() > 1 && partes.last().is_some_and(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())) {
            partes.pop();
        }
    }
    partes
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Procura o `.mif` do título e devolve o ClassID do applet.
///
/// O layout instalado é `<titulo>/mod/<id>/<nome>.mod` com o `.mif` em `<titulo>/mif/<id>.mif`;
/// os exemplos do SDK deixam o `.mif` ao lado do `.mod`. Cobrimos os dois casos, e mais um: há
/// pacote que guarda o módulo numa pasta própria e o manifesto **um nível acima** dela.
///
/// Um `.mif` que não declara applet não serve de resposta: o pacote do Action Hero 3D traz dois
/// manifestos, e um deles declara só uma classe. Por isso a busca segue pelos candidatos até
/// achar um que diga qual applet criar, em vez de parar no primeiro que abre.
pub fn applet_clsid(mod_path: &Path) -> Option<u32> {
    let index=crate::handset::mif_lookup::Index::for_root(mod_path.parent()?);
    match index.find(mod_path) {
        Err(_)=>return None,
        Ok(Some(path))=>{
            let applet=std::fs::read(path).ok().and_then(|data|MifFile::parse(&data).ok()).and_then(|mif|mif.main_applet());
            if index.shared||applet.is_some(){return applet;}
        },
        Ok(None)=>{},
    }
    if index.shared{return None;}
    manifest_paths(mod_path)
        .iter()
        .filter_map(|p| std::fs::read(p).ok())
        .filter_map(|data| MifFile::parse(&data).ok())
        .find_map(|mif| mif.main_applet())
}

/// Pareação exata para abrir aparelhos: nome do módulo ou da pasta, sem escolher MIF alheio.
pub fn resolve_mif(mod_path:&Path)->Result<PathBuf,String>{
    let index=crate::handset::mif_lookup::Index::for_root(mod_path.parent().ok_or("Module has no game folder")?);
    index.find(mod_path)?.ok_or_else(||format!("No matching MIF for {} or its game folder. Place it beside the module or under games/Mif.",mod_path.display()))
}

fn manifest_with_index(path:&Path,index:&crate::handset::mif_lookup::Index)->Option<Vec<u8>>{
    // Reusar os diretórios já vistos evita reler a coleção para cada pacote local.
    match index.find(path){Ok(Some(p))=>return std::fs::read(p).ok(),Err(_)=>return None,Ok(None)=>{}}
    if index.shared {return None;}
    let local=crate::handset::mif_lookup::Index::for_root(path.parent()?);
    match local.find(path){Ok(Some(p))=>return std::fs::read(p).ok(),Err(_)=>return None,Ok(None)=>{}}
    if local.shared{return None;}
    manifest(path)
}

/// Os módulos de **extensão** do pacote e as dependências declaradas que faltam nele.
///
/// Devolve, para cada um, o caminho e as classes que ele fornece. Um pacote do console pode
/// trazer mais de um módulo, e quando traz, os que não têm applet existem para exportar classe:
/// é assim que o Action Hero 3D recebe o `IMICRO3D` e o Kingdom Hearts, o motor 3D da
/// Superscape. Os dois pedem uma classe que o console não tem em lugar nenhum senão ali.
///
/// A busca começa uma pasta **acima** da do módulo e desce dois níveis, que é o que cobre as
/// duas disposições reais: `mod/<id>/x.mod` ao lado de `mod/<outro>/y.mod`, e
/// `<Título>/<Título>/x.mod` ao lado de `<Título>/<Título>_/y.mod`.
/// Só classes importadas pelo jogo e ausentes no pacote autorizam procurar um provedor nos
/// pacotes irmãos da mesma coleção. Provedores locais sempre têm preferência.
pub fn extensoes(mod_path: &Path) -> Vec<(PathBuf, Vec<u32>)> {
    let Some(raiz) = mod_path.parent().and_then(Path::parent) else {
        return Vec::new();
    };
    let mut modulos = Vec::new();
    colhe_modulos(raiz, 2, &mut modulos);
    modulos.sort();
    let mut extensoes: Vec<_> = modulos
        .into_iter()
        .filter(|caminho| caminho != mod_path)
        .filter_map(|caminho| {
            let mif = mif_da_pasta(&caminho)?;
            let data = le_arquivo_de_extensao(&mif, 1024 * 1024)?;
            let classes = crate::handset::package::parse_mif(&data).ok()?.extensao().to_vec();
            (!classes.is_empty()).then_some((caminho, classes))
        })
        .collect();

    // O registro de classe também aparece no MIF de quem depende dela. Só o MIF
    // exato do applet define as importações; um manifesto vizinho não serve aqui.
    let mif_jogo = mif_da_pasta(mod_path);
    let mut faltantes: BTreeSet<u32> = mif_jogo.as_deref()
        .and_then(|mif| le_arquivo_de_extensao(mif, 1024 * 1024))
        .and_then(|data| crate::handset::package::parse_mif(&data).ok())
        .filter(|mif| !mif.applets.is_empty())
        .map(|mif| mif.classes.into_iter().collect())
        .unwrap_or_default();
    for (_, classes) in &extensoes {
        for class in classes {
            faltantes.remove(class);
        }
    }
    if faltantes.is_empty() {
        return extensoes;
    }

    // Um MIF ao lado do módulo identifica o pacote plano. Nesse caso raiz já é
    // a coleção; subir a partir dela atravessaria a fronteira entre resoluções.
    // No console, raiz é <Título>/mod; no outro layout móvel é <Título>.
    let pacote = if mif_jogo.as_deref().and_then(Path::parent) == mod_path.parent() {
        mod_path.parent().unwrap()
    } else if raiz
        .file_name()
        .is_some_and(|nome| nome.to_string_lossy().eq_ignore_ascii_case("mod"))
    {
        raiz.parent().unwrap_or(raiz)
    } else {
        raiz
    };
    let Some(colecao) = pacote.parent() else {
        return extensoes;
    };
    let mut compartilhados = Vec::new();
    let mut restantes = 4000;
    colhe_modulos_compartilhados(colecao, 3, &mut restantes, &mut compartilhados);
    compartilhados.sort();
    for caminho in compartilhados {
        if caminho.starts_with(pacote) {
            continue;
        }
        let Some(mif) = mif_da_pasta(&caminho) else {
            continue;
        };
        let Some(mif) = le_arquivo_de_extensao(&mif, 1024 * 1024)
            .and_then(|data| crate::handset::package::parse_mif(&data).ok())
        else {
            continue;
        };
        if !mif.extensao().iter().any(|class| faltantes.contains(class)) {
            continue;
        }
        // Um MIF não comprova que o módulo possa ser carregado. Validar antes de
        // reservar a classe permite tentar a próxima cópia quando esta está ruim.
        if le_arquivo_de_extensao(&caminho, 16 * 1024 * 1024)
            .and_then(|bytes| crate::handset::package::parse_module(bytes).ok())
            .is_none()
        {
            continue;
        }
        // Só o primeiro provedor ordenado recebe cada classe faltante. Nove
        // cópias do mesmo motor não devem consumir nove regiões de extensão.
        let classes: Vec<_> = mif
            .extensao()
            .iter()
            .copied()
            .filter(|class| faltantes.remove(class))
            .collect();
        if !classes.is_empty() {
            extensoes.push((caminho, classes));
        }
        if faltantes.is_empty() {
            break;
        }
    }
    extensoes
}

fn le_arquivo_de_extensao(caminho: &Path, limite: u64) -> Option<Vec<u8>> {
    let arquivo = std::fs::File::open(caminho).ok()?;
    let metadata = arquivo.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > limite {
        return None;
    }
    let mut bytes = Vec::new();
    arquivo.take(limite + 1).read_to_end(&mut bytes).ok()?;
    // O arquivo pode crescer entre metadata e a leitura.
    (bytes.len() as u64 <= limite).then_some(bytes)
}

/// Busca compartilhada limitada em profundidade, entradas e tamanho dos manifestos.
/// Links não pertencem à coleção e poderiam sair dela ou formar ciclos.
fn colhe_modulos_compartilhados(
    dir: &Path,
    profundidade: usize,
    restantes: &mut usize,
    achados: &mut Vec<PathBuf>,
) {
    if *restantes == 0 {
        return;
    }
    let Ok(entradas) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entradas: Vec<_> = entradas.take(*restantes + 1).flatten().collect();
    // Uma pasta maior que o orçamento não oferece uma seleção estável: read_dir
    // não garante quais entradas apareceriam primeiro. Recusar o conjunto evita
    // escolher um motor diferente em cada abertura.
    if entradas.len() > *restantes {
        *restantes = 0;
        return;
    }
    *restantes -= entradas.len();
    entradas.sort_by_key(|entrada| entrada.file_name());
    for entrada in entradas {
        let Ok(kind) = entrada.file_type() else {
            continue;
        };
        let caminho = entrada.path();
        if kind.is_dir() && profundidade > 0 {
            colhe_modulos_compartilhados(&caminho, profundidade - 1, restantes, achados);
        } else if kind.is_file() && caminho.extension().and_then(|e| e.to_str()) == Some("mod") {
            achados.push(caminho);
        }
    }
}

/// Os `.mod` sob um diretório, até `profundidade` níveis.
fn colhe_modulos(dir: &Path, profundidade: usize, achados: &mut Vec<PathBuf>) {
    let Ok(entradas) = std::fs::read_dir(dir) else {
        return;
    };
    for entrada in entradas.flatten() {
        let caminho = entrada.path();
        if caminho.is_dir() {
            if profundidade > 0 {
                colhe_modulos(&caminho, profundidade - 1, achados);
            }
        } else if caminho.extension().and_then(|e| e.to_str()) == Some("mod") {
            achados.push(caminho);
        }
    }
}

/// O `.mif` que casa com um `.mod` pelo **nome da pasta** dele.
///
/// É a regra do BREW: um módulo instalado mora em `<nome>/` e o manifesto é `<nome>.mif`, ao
/// lado. Ela é mais estreita que a busca do [`manifest_paths`] de propósito — aqui pareação
/// errada não dá "manifesto não encontrado", dá **o módulo do jogo se oferecendo para atender a
/// classe que ele próprio está pedindo**, porque os dois `.mif` da relação citam o mesmo
/// ClassID no mesmo formato.
fn mif_da_pasta(mod_path: &Path) -> Option<PathBuf> {
    let dir = mod_path.parent()?;
    let nome = dir.file_name()?;
    let candidatos = [
        mod_path.with_extension("mif"),
        // `<Título>/<nome>/x.mod` -> `<Título>/<nome>.mif`
        dir.parent()?.join(nome).with_extension("mif"),
        // `<Título>/mod/<id>/x.mod` -> `<Título>/mif/<id>.mif`, a disposição do console
        dir.parent()?
            .parent()?
            .join("mif")
            .join(nome)
            .with_extension("mif"),
    ];
    candidatos.into_iter().find(|p| p.is_file())
}

/// Onde o `.mif` de um `.mod` pode estar, na ordem em que vale procurar.
///
/// Os dois primeiros candidatos são nomes calculados; os últimos vêm de varrer as pastas, e é o
/// que cobre pacote com disposição própria — o do Kingdom Hearts guarda o módulo em
/// `<Título>/<Título>_/swv21brew.mod` e o manifesto em `<Título>/<Título>.mif`, e nenhuma regra
/// de nome acha isso. Varrer é barato: são duas pastas, e só até o applet ser encontrado.
fn manifest_paths(mod_path: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![mod_path.with_extension("mif")];
    let dir = mod_path.parent();
    if let Some(dir) = dir
        && let Some(id) = dir.file_name()
        && let Some(title_dir) = dir.parent().and_then(|p| p.parent())
    {
        // .../mod/<id>/x.mod  ->  .../mif/<id>.mif
        candidates.push(title_dir.join("mif").join(id).with_extension("mif"));
    }
    // A pasta do módulo primeiro, e a de cima depois: o manifesto mais perto do `.mod` é o que
    // tem mais chance de ser dele, num pacote que traga mais de um jogo.
    for pasta in [dir, dir.and_then(Path::parent)].into_iter().flatten() {
        let Ok(entradas) = std::fs::read_dir(pasta) else {
            continue;
        };
        let mut mifs: Vec<PathBuf> = entradas
            .flatten()
            .map(|entrada| entrada.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("mif"))
            .collect();
        // Ordem estável: sem ela, dois manifestos na mesma pasta dariam respostas diferentes
        // conforme o sistema de arquivos listasse.
        mifs.sort();
        candidates.extend(mifs);
    }
    candidates.dedup();
    candidates
}

/// Se o título tem todas as palavras da busca, em qualquer ordem e em qualquer parte.
///
/// Maiúsculas, acentos e pontuação não contam: "cnk" acha "C.N.K.", "joao" acha "João" e
/// "extreme boia" acha "Zeebo Extreme Bóia Cross".
pub fn casa_com_a_busca(titulo: &str, busca: &str) -> bool {
    let titulo = sem_acento(titulo);
    let compacto: String = titulo.split_whitespace().collect();
    sem_acento(busca)
        .split_whitespace()
        .all(|palavra| titulo.contains(palavra) || compacto.contains(palavra))
}

/// O texto em minúsculas, sem acento, com a pontuação trocada por espaço e as siglas com ponto
/// juntas ("C.N.K." vira "cnk").
fn sem_acento(texto: &str) -> String {
    let mut saida = String::with_capacity(texto.len());
    for c in texto.chars().flat_map(char::to_lowercase) {
        let c = match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            '.' | '\'' | '’' => continue,
            c if c.is_alphanumeric() => c,
            _ => ' ',
        };
        saida.push(c);
    }
    saida
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_busca_ignora_caixa_acento_e_pontuacao() {
        assert!(casa_com_a_busca("Zeebo Extreme Bóia Cross", "boia"));
        assert!(casa_com_a_busca("Zeebo Extreme Bóia Cross", "EXTREME boia"));
        assert!(casa_com_a_busca("Zeebo Extreme Bóia Cross", "cross zeebo"));
        assert!(casa_com_a_busca("C.N.K. 3D", "cnk"));
        assert!(casa_com_a_busca("Resident Evil 4", "resident evil"));
        assert!(casa_com_a_busca("Double Dragon", "doubledragon"));
        assert!(!casa_com_a_busca("Double Dragon", "rolima"));
        assert!(!casa_com_a_busca("Zeebo Extreme Rolimã", "rolima boia"));
    }

    #[test]
    fn a_busca_vazia_mostra_todos() {
        assert!(casa_com_a_busca("Tênis", ""));
        assert!(casa_com_a_busca("Tênis", "   "));
    }
    use super::*;

    #[test]
    fn o_digest_do_cache_sai_do_titulo() {
        let digest = "0c9aef377a62f6211acb6e1dd1d79ce97c39d5fa2a3db82d36fcf25de2f2222f";
        assert_eq!(
            sem_impressao_digital(&format!("Crash-Bandicoot-Nitro-Kart-3D-{digest}")),
            "Crash Bandicoot Nitro Kart 3D"
        );
        assert_eq!(
            sem_impressao_digital("Zeebo-Extreme-Boia-Cross-21503726-1788761080"),
            "Zeebo Extreme Boia Cross"
        );
        // Um nome que é só o digest continua sendo o que há.
        assert_eq!(sem_impressao_digital(digest), digest);
    }

    /// Monta uma árvore de arquivos vazios e devolve a raiz.
    fn tree(name: &str, files: &[&str]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("zeebx-testes-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        for file in files {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, []).unwrap();
        }
        root
    }

    #[test]
    fn catalogo_atualiza_roms_sem_apagar_o_que_veio_da_nand() {
        let mut index = CatalogIndex {
            titles: vec![CatalogEntry {
                applet_class: 7,
                title: "Jogo de fábrica".into(),
                rom_path: PathBuf::from("/nand/jogo.mod"),
                source: CatalogSource::Nand,
            }],
        };
        let games = vec![Game {
            title: "Homebrew".into(),
            path: PathBuf::from("/roms/homebrew.mod"),
            clsid: Some(9),
            packed: false,
            art: None,
        }];

        index.refresh_roms(&games);
        assert_eq!(index.titles.len(), 2);
        assert!(index.titles.iter().any(|entry| entry.applet_class == 7));
        assert!(index.titles.iter().any(|entry| entry.applet_class == 9));

        index.refresh_roms(&[]);
        assert_eq!(index.titles.len(), 1);
        assert_eq!(index.titles[0].source, CatalogSource::Nand);
    }

    /// O manifesto do Kingdom Hearts fica um nível acima da pasta do módulo.
    ///
    /// `<Título>/<Título>_/swv21brew.mod` com o `.mif` em `<Título>/<Título>.mif`: nenhuma regra
    /// de nome acha isso, e o jogo não abria por não sabermos qual applet criar. É a razão de a
    /// busca varrer as pastas em vez de só calcular nomes.
    #[test]
    fn o_manifesto_pode_estar_um_nivel_acima_da_pasta_do_modulo() {
        let root = tree(
            "mif-acima",
            &[
                "Kingdon Hearts/Kingdon Hearts_/swv21brew.mod",
                "Kingdon Hearts/Kingdon Hearts.mif",
            ],
        );
        let modulo = root.join("Kingdon Hearts/Kingdon Hearts_/swv21brew.mod");
        let candidatos = manifest_paths(&modulo);
        assert!(
            candidatos.contains(&root.join("Kingdon Hearts/Kingdon Hearts.mif")),
            "{candidatos:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A pasta do módulo vem antes da de cima, e a ordem não depende do sistema de arquivos.
    ///
    /// Num pacote com mais de um jogo, o manifesto mais perto do `.mod` é o que tem mais chance
    /// de ser dele — e dois `.mif` na mesma pasta precisam ser tentados sempre na mesma ordem,
    /// ou o mesmo jogo abriria diferente em duas máquinas.
    #[test]
    fn o_manifesto_mais_perto_do_modulo_vem_primeiro() {
        let root = tree(
            "mif-ordem",
            &[
                "jogo/mod/b.mif",
                "jogo/mod/a.mif",
                "jogo/mod/x.mod",
                "jogo/de-cima.mif",
            ],
        );
        let candidatos = manifest_paths(&root.join("jogo/mod/x.mod"));
        let so_dentro: Vec<&PathBuf> = candidatos
            .iter()
            .filter(|p| p.starts_with(root.join("jogo/mod")))
            .collect();
        assert_eq!(
            so_dentro,
            [
                &root.join("jogo/mod/x.mif"),
                &root.join("jogo/mod/a.mif"),
                &root.join("jogo/mod/b.mif"),
            ]
        );
        let de_cima = candidatos
            .iter()
            .position(|p| p == &root.join("jogo/de-cima.mif"));
        assert_eq!(de_cima, Some(candidatos.len() - 1));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn o_titulo_vem_da_pasta_no_layout_do_console() {
        // `<Título>/mod/<id>/<nome>.mod` — o nome do arquivo é um identificador, o título está
        // três níveis acima.
        let path = Path::new("/roms/Zeebo Sports Peteca/mod/279159/zeebopeteca.mod");
        assert_eq!(title_for(path), "Zeebo Sports Peteca");
    }

    #[test]
    fn fora_do_layout_do_console_o_titulo_e_o_nome_do_arquivo() {
        assert_eq!(title_for(Path::new("/roms/helloworld.mod")), "helloworld");
        // Uma pasta chamada `mod` no lugar errado não faz do avô um título.
        assert_eq!(title_for(Path::new("/mod/x.mod")), "x");
    }

    #[test]
    fn a_varredura_acha_as_duas_disposicoes_e_ordena_por_titulo() {
        let root = tree(
            "biblioteca",
            &[
                "Quake/mod/274802/quake.mod",
                "Quake/mif/274802.mif",
                "Crash/mod/274214/cnk2.mod",
                "avulso.mod",
                "leiame.txt",
                "Quake/mod/274802/dados.bin",
            ],
        );
        let games = scan(&root);
        let titles: Vec<&str> = games.iter().map(|g| g.title.as_str()).collect();
        assert_eq!(titles, ["avulso", "Crash", "Quake"]);
        // O `.mif` está vazio, então não há ClassID — mas o jogo continua na lista, porque
        // quem diz se o módulo presta é o emulador, não a varredura.
        assert!(games.iter().all(|g| g.clsid.is_none()));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pasta_que_nao_existe_devolve_lista_vazia() {
        let games = scan(&std::env::temp_dir().join("zeebx-nao-existe-mesmo"));
        assert!(games.is_empty());
    }
}
