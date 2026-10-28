//! Preferências do frontend por conteúdo; a geometria continua no perfil de display existente.
use serde_json::{Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use zeebx::storage::ContentId;

static ESCRITA: Mutex<()> = Mutex::new(());
static PROXIMO_TEMPORARIO: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScreenOrientation {
    #[default]
    Auto,
    Portrait,
    Landscape,
}

impl ScreenOrientation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Portrait => "portrait",
            Self::Landscape => "landscape",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScreenGravity {
    Top,
    #[default]
    Center,
    Bottom,
}

impl ScreenGravity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Center => "center",
            Self::Bottom => "bottom",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OnScreenControls {
    On,
    Off,
    #[default]
    SystemDefault,
}

impl OnScreenControls {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
            Self::SystemDefault => "system_default",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameOptions {
    pub orientation: ScreenOrientation,
    pub gravity: ScreenGravity,
    pub controls: OnScreenControls,
}

impl GameOptions {
    /// Arquivo ausente herda os defaults; erro é explícito para preservar um perfil corrompido.
    /// O lançamento pode registrar o erro e usar `unwrap_or_default()` sem gravar nada.
    pub fn load(raiz: &Path, conteudo: &ContentId) -> Result<Self, String> {
        // No Windows, abrir filho de arquivo pode dar NotFound, assim como um perfil ausente.
        match std::fs::metadata(raiz) {
            Ok(meta) if !meta.is_dir() => return Err("Game options root is not a directory".into()),
            Err(erro) if erro.kind() != std::io::ErrorKind::NotFound => {
                return Err(format!("Cannot read game options folder: {erro}"));
            }
            _ => {}
        }
        let caminho = caminho(raiz, conteudo);
        let bytes = match std::fs::read(&caminho) {
            Ok(bytes) => bytes,
            Err(erro) if erro.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(erro) => {
                return Err(format!(
                    "Cannot read game options {}: {erro}",
                    caminho.display()
                ));
            }
        };
        let mut opcoes = Self::default();
        let valor: Value = serde_json::from_slice(&bytes)
            .map_err(|erro| format!("Invalid game options {}: {erro}", caminho.display()))?;
        opcoes
            .aplicar(&valor)
            .map_err(|erro| format!("Invalid game options {}: {erro}", caminho.display()))?;
        Ok(opcoes)
    }

    /// O mesmo formato serve ao arquivo e ao JNI, sem adicionar dependências ao frontend.
    pub fn as_json(self) -> Value {
        json!({
            "orientation": self.orientation.as_str(),
            "gravity": self.gravity.as_str(),
            "controls": self.controls.as_str(),
        })
    }

    fn aplicar(&mut self, patch: &Value) -> Result<(), String> {
        let campos = patch
            .as_object()
            .ok_or("Game options patch must be an object")?;
        // Validar numa cópia impede que um campo inválido aceite a metade de um patch.
        let mut novas = *self;
        for (campo, valor) in campos {
            match (campo.as_str(), valor.as_str()) {
                ("orientation", Some("auto")) => novas.orientation = ScreenOrientation::Auto,
                ("orientation", Some("portrait")) => {
                    novas.orientation = ScreenOrientation::Portrait
                }
                ("orientation", Some("landscape")) => {
                    novas.orientation = ScreenOrientation::Landscape
                }
                ("gravity", Some("top")) => novas.gravity = ScreenGravity::Top,
                ("gravity", Some("center")) => novas.gravity = ScreenGravity::Center,
                ("gravity", Some("bottom")) => novas.gravity = ScreenGravity::Bottom,
                ("controls", Some("on")) => novas.controls = OnScreenControls::On,
                ("controls", Some("off")) => novas.controls = OnScreenControls::Off,
                ("controls", Some("system_default")) => {
                    novas.controls = OnScreenControls::SystemDefault
                }
                _ => return Err(format!("Unsupported game option or value: {campo}")),
            }
        }
        *self = novas;
        Ok(())
    }
}

fn caminho(raiz: &Path, conteudo: &ContentId) -> PathBuf {
    raiz.join(format!("{}.json", conteudo.as_str()))
}

fn salvar(raiz: &Path, conteudo: &ContentId, opcoes: GameOptions) -> Result<(), String> {
    std::fs::create_dir_all(raiz)
        .map_err(|erro| format!("Cannot create game options folder: {erro}"))?;
    let destino = caminho(raiz, conteudo);
    let temporario = raiz.join(format!(
        ".brew-game-options-{}-{}.tmp",
        std::process::id(),
        PROXIMO_TEMPORARIO.fetch_add(1, Ordering::Relaxed)
    ));
    // Nunca truncar o original: a sessão lê ou a versão anterior ou a escrita completa.
    let resultado = (|| -> std::io::Result<()> {
        let bytes = serde_json::to_vec_pretty(&opcoes.as_json())?;
        let mut arquivo = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporario)?;
        arquivo.write_all(&bytes)?;
        arquivo.sync_all()?;
        drop(arquivo);
        std::fs::rename(&temporario, &destino)
    })();
    if let Err(erro) = resultado {
        let _ = std::fs::remove_file(&temporario);
        return Err(format!(
            "Cannot save game options {}: {erro}",
            destino.display()
        ));
    }
    Ok(())
}

/// Patch vazio consulta; demais patches validam e persistem apenas as opções deste conteúdo.
pub fn view(raiz: &Path, modulo: &Path, patch: &str) -> Result<Value, String> {
    let conteudo = ContentId::from_reader(
        std::fs::File::open(modulo).map_err(|erro| format!("Cannot open game module: {erro}"))?,
    )
    .map_err(|erro| format!("Cannot identify game module: {erro}"))?;
    if patch.is_empty() {
        return Ok(GameOptions::load(raiz, &conteudo)?.as_json());
    }

    let valor = serde_json::from_str(patch)
        .map_err(|erro| format!("Invalid game options patch: {erro}"))?;
    // Activities distintas podem escrever o mesmo conteúdo; a leitura e o rename são uma transação.
    let _escrita = ESCRITA
        .lock()
        .map_err(|_| "Game options writer unavailable")?;
    let mut opcoes = GameOptions::load(raiz, &conteudo)?;
    opcoes.aplicar(&valor)?;
    salvar(raiz, &conteudo, opcoes)?;
    Ok(opcoes.as_json())
}
