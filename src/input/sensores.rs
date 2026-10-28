//! Os sensores de movimento dos controles do host, lidos direto dos dispositivos de entrada do
//! Linux.
//!
//! Os drivers do kernel separam o sensor do resto do controle, num dispositivo irmão marcado com
//! `INPUT_PROP_ACCELEROMETER`: "Pro Controller (IMU)" no `hid-nintendo`, "… Motion Sensors" no
//! `hid-playstation`. O gilrs não os enxerga — são eixos de outro dispositivo —, então a leitura
//! é nossa, como a do acelerômetro do Wii Remote (que continua em [`crate::input::wiimote`]).
//!
//! **O nó do sensor costuma não ter permissão.** O udev só dá acesso ao usuário da sessão
//! (`uaccess`) ao que ele marca como joystick, e o sensor é marcado como acelerômetro. O controle
//! abre, o sensor não; nesse caso o sensor aparece com [`EstadoDoSensor::sem_permissao`], e a
//! interface diz qual regra resolve.
//!
//! Fora do Linux não há leitura: a lista fica vazia.

use std::sync::{Arc, Mutex};

/// Um sensor de movimento encontrado, e o que ele mede agora.
#[derive(Debug, Clone, PartialEq)]
pub struct EstadoDoSensor {
    /// O nome do dispositivo do sensor, como o kernel o anuncia.
    pub nome: String,
    pub vendor: u16,
    pub product: u16,
    /// A aceleração em g, nos eixos do driver.
    pub aceleracao: [f32; 3],
    /// Se o sensor já mandou alguma leitura.
    pub com_leitura: bool,
    /// Se o nó existe e não pôde ser aberto.
    pub sem_permissao: bool,
}

impl EstadoDoSensor {
    /// Se este sensor é do controle de nome `nome_do_sistema` e par `vendor:product`.
    ///
    /// O driver dá ao sensor o nome do controle com um sufixo: "Pro Controller" e "Pro Controller
    /// (IMU)". O par sozinho não basta — um receptor pode ter mais de um aparelho —, e o nome
    /// sozinho também não, porque dois fabricantes podem usar o mesmo.
    pub fn e_do_controle(&self, nome_do_sistema: &str, vendor: u16, product: u16) -> bool {
        self.vendor == vendor
            && self.product == product
            && !nome_do_sistema.is_empty()
            && self.nome.starts_with(nome_do_sistema)
    }
}

/// A regra do udev que libera os sensores para o usuário da sessão, para a interface mostrar.
pub const REGRA_DO_UDEV: &str =
    r#"SUBSYSTEM=="input", KERNEL=="event*", ENV{ID_INPUT_ACCELEROMETER}=="1", TAG+="uaccess""#;

/// Onde a regra fica. O número é menor que o do `73-seat-late.rules`, que é quem aplica o
/// `uaccess`: uma regra depois dele marcaria o nó tarde demais.
pub const ARQUIVO_DA_REGRA: &str = "/etc/udev/rules.d/70-zeebx-sensores.rules";

/// Grava a [`REGRA_DO_UDEV`] e a aplica aos nós que já existem, pedindo a senha pelo `pkexec`.
///
/// É um comando só, para a janela de senha do sistema aparecer uma vez: grava o arquivo, recarrega
/// as regras e repassa os dispositivos de entrada por elas, o que dá a ACL aos sensores já ligados
/// sem precisar reconectar o controle. A leitura de quem estava sem permissão é tentada de novo a
/// cada segundo, e volta sozinha.
///
/// Bloqueia até a pessoa responder à janela: quem chama roda isto numa thread.
pub fn libera_sensores() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        use std::io::Write;
        let script = format!(
            "cat > {ARQUIVO_DA_REGRA} && udevadm control --reload-rules && \
             udevadm trigger --subsystem-match=input --action=change"
        );
        let mut filho = std::process::Command::new("pkexec")
            .args(["sh", "-c", &script])
            .stdin(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|erro| format!("pkexec: {erro}"))?;
        if let Some(mut entrada) = filho.stdin.take() {
            entrada
                .write_all(format!("{REGRA_DO_UDEV}\n").as_bytes())
                .map_err(|erro| erro.to_string())?;
        }
        let saida = filho.wait_with_output().map_err(|erro| erro.to_string())?;
        match saida.status.code() {
            Some(0) => Ok(()),
            // 126 é a janela fechada ou a senha recusada; 127, o polkit sem agente para perguntar.
            Some(126) => Err("autorização negada".into()),
            Some(127) => Err("não há como pedir a senha nesta sessão".into()),
            _ => Err(String::from_utf8_lossy(&saida.stderr).trim().to_string()),
        }
    }
    #[cfg(not(target_os = "linux"))]
    Err("só existe no Linux".into())
}

/// Os sensores encontrados, pela ordem dos nós em `/dev/input`.
#[derive(Clone, Default)]
pub struct Sensores {
    estados: Arc<Mutex<Vec<(std::path::PathBuf, EstadoDoSensor)>>>,
}

impl Sensores {
    /// Começa a procurar sensores em segundo plano.
    pub fn inicia() -> Self {
        let sensores = Self::default();
        #[cfg(target_os = "linux")]
        {
            let estados = sensores.estados.clone();
            let _ = std::thread::Builder::new()
                .name("sensores-busca".into())
                .spawn(move || linux::busca(estados));
        }
        sensores
    }

    /// O `ordem`-ésimo sensor do controle de nome `nome_do_sistema` e par `vendor:product`.
    ///
    /// A ordem é a de chegada dos nós, a mesma com que o sistema lista os controles: com dois
    /// Pro Controller ligados, o segundo controle fica com o segundo sensor.
    pub fn do_controle(
        &self,
        nome_do_sistema: &str,
        vendor: u16,
        product: u16,
        ordem: usize,
    ) -> Option<EstadoDoSensor> {
        let estados = self.estados.lock().ok()?;
        estados
            .iter()
            .map(|(_, estado)| estado)
            .filter(|estado| estado.e_do_controle(nome_do_sistema, vendor, product))
            .nth(ordem)
            .cloned()
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::collections::HashSet;
    use std::io::Read;
    use std::os::fd::AsRawFd;
    use std::path::{Path, PathBuf};

    const EV_ABS: u16 = 3;
    /// O bit de `INPUT_PROP_ACCELEROMETER` no arquivo `properties`.
    const PROP_ACELEROMETRO: u64 = 1 << 6;
    /// `struct input_event` em 64 bits: `timeval` (16), `type` (2), `code` (2), `value` (4).
    const TAMANHO_DO_EVENTO: usize = 24;
    /// Unidades por g quando o driver não declara resolução.
    const UNIDADES_POR_G_SEM_RESOLUCAO: f32 = 100.0;

    /// Procura sensores para sempre, abrindo uma leitura para cada nó novo.
    pub(super) fn busca(estados: Arc<Mutex<Vec<(PathBuf, EstadoDoSensor)>>>) {
        let abertos: Arc<Mutex<HashSet<PathBuf>>> = Default::default();
        loop {
            for (no, mut estado, eixos) in dispositivos() {
                let novo = abertos.lock().is_ok_and(|mut a| a.insert(no.clone()));
                if !novo {
                    continue;
                }
                if let Ok(mut lista) = estados.lock() {
                    // Um sensor tentado de novo depois de ficar sem permissão já está na lista,
                    // e continua marcado até a tentativa nova decidir: sem isso a prévia
                    // piscaria entre "sem permissão" e "esperando" a cada segundo.
                    if let Some((_, antigo)) = lista.iter().find(|(caminho, _)| *caminho == no) {
                        estado.sem_permissao = antigo.sem_permissao;
                    }
                    lista.retain(|(caminho, _)| *caminho != no);
                    lista.push((no.clone(), estado));
                    lista.sort_by(|a, b| numero_do_no(&a.0).cmp(&numero_do_no(&b.0)));
                }
                let (estados, abertos) = (estados.clone(), abertos.clone());
                let _ = std::thread::Builder::new()
                    .name("sensor".into())
                    .spawn(move || {
                        let sem_permissao = le(&no, eixos, &estados);
                        // Sem permissão o sensor continua na lista, marcado, para a interface
                        // explicar, e sai de `abertos` para ser tentado de novo na volta
                        // seguinte: é assim que ele volta sozinho depois da regra aplicada. Os
                        // outros saem da lista quando o controle desconecta.
                        if let Ok(mut a) = abertos.lock() {
                            a.remove(&no);
                        }
                        if !sem_permissao && let Ok(mut lista) = estados.lock() {
                            lista.retain(|(caminho, _)| *caminho != no);
                        }
                    });
            }
            // Um sensor sem permissão que sumiu (o controle desconectou) sai da lista.
            if let Ok(mut lista) = estados.lock() {
                lista.retain(|(no, estado)| !estado.sem_permissao || no.exists());
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }

    fn numero_do_no(no: &Path) -> u32 {
        no.file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_prefix("event"))
            .and_then(|n| n.parse().ok())
            .unwrap_or(u32::MAX)
    }

    /// `(nó em /dev/input, estado inicial, códigos dos eixos X, Y e Z)` de cada sensor.
    ///
    /// O acelerômetro do Wii Remote fica de fora: ele é lido junto com os botões, em
    /// [`crate::input::wiimote`].
    fn dispositivos() -> Vec<(PathBuf, EstadoDoSensor, [u16; 3])> {
        let Ok(entradas) = std::fs::read_dir("/sys/class/input") else {
            return Vec::new();
        };
        entradas
            .flatten()
            .filter_map(|entrada| {
                let nome = entrada.file_name().into_string().ok()?;
                if !nome.starts_with("event") {
                    return None;
                }
                let base = entrada.path().join("device");
                let le = |arquivo: &str| std::fs::read_to_string(base.join(arquivo)).ok();
                let propriedades = u64::from_str_radix(le("properties")?.trim(), 16).ok()?;
                if propriedades & PROP_ACELEROMETRO == 0 {
                    return None;
                }
                let rotulo = le("name")?.trim().to_string();
                if rotulo.starts_with("Nintendo Wii Remote") {
                    return None;
                }
                let hex = |arquivo: &str| u16::from_str_radix(le(arquivo)?.trim(), 16).ok();
                let (vendor, product) = (hex("id/vendor")?, hex("id/product")?);
                // Os controles com giroscópio põem a aceleração em X, Y e Z e a rotação em RX,
                // RY e RZ; quem só tem acelerômetro pode usar os R.
                let eixos_abs = le("capabilities/abs")?;
                let ultimo = eixos_abs.split_whitespace().last()?;
                let bits = u64::from_str_radix(ultimo, 16).ok()?;
                let eixos = match bits & 0b111 == 0b111 {
                    true => [0, 1, 2],
                    false => [3, 4, 5],
                };
                let estado = EstadoDoSensor {
                    nome: rotulo,
                    vendor,
                    product,
                    aceleracao: [0.0, 0.0, 1.0],
                    com_leitura: false,
                    sem_permissao: false,
                };
                Some((Path::new("/dev/input").join(nome), estado, eixos))
            })
            .collect()
    }

    /// A resolução de um eixo, em unidades por g. É o `resolution` do `struct input_absinfo`,
    /// que o `hid-nintendo` e o `hid-playstation` preenchem com a escala do sensor.
    fn unidades_por_g(arquivo: &std::fs::File, eixo: u16) -> f32 {
        // `EVIOCGABS(eixo)`: `_IOR('E', 0x40 + eixo, struct input_absinfo)`, 24 bytes.
        let pedido = 0x8000_0000u64 | (24 << 16) | ((b'E' as u64) << 8) | (0x40 + eixo as u64);
        let mut info = [0i32; 6];
        // SAFETY: o `ioctl` escreve exatamente os 24 bytes do `input_absinfo` em `info`.
        let resultado =
            unsafe { libc::ioctl(arquivo.as_raw_fd(), pedido as _, info.as_mut_ptr()) };
        match (resultado, info[5]) {
            (0, resolucao) if resolucao > 0 => resolucao as f32,
            _ => UNIDADES_POR_G_SEM_RESOLUCAO,
        }
    }

    /// Lê o sensor até ele sumir. Devolve se o nó não pôde ser aberto por falta de permissão.
    fn le(no: &Path, eixos: [u16; 3], estados: &Mutex<Vec<(PathBuf, EstadoDoSensor)>>) -> bool {
        let mut arquivo = match std::fs::File::open(no) {
            Ok(arquivo) => arquivo,
            Err(erro) => {
                let sem_permissao = erro.kind() == std::io::ErrorKind::PermissionDenied;
                if sem_permissao
                    && let Ok(mut lista) = estados.lock()
                    && let Some((_, estado)) = lista.iter_mut().find(|(n, _)| n == no)
                {
                    estado.sem_permissao = true;
                }
                return sem_permissao;
            }
        };
        if let Ok(mut lista) = estados.lock()
            && let Some((_, estado)) = lista.iter_mut().find(|(n, _)| n == no)
        {
            estado.sem_permissao = false;
        }
        let escala = eixos.map(|eixo| unidades_por_g(&arquivo, eixo));
        let mut bruto = [0u8; TAMANHO_DO_EVENTO];
        let mut aceleracao = [0f32; 3];
        while arquivo.read_exact(&mut bruto).is_ok() {
            let tipo = u16::from_ne_bytes([bruto[16], bruto[17]]);
            let codigo = u16::from_ne_bytes([bruto[18], bruto[19]]);
            let valor = i32::from_ne_bytes([bruto[20], bruto[21], bruto[22], bruto[23]]);
            if tipo != EV_ABS {
                continue;
            }
            let Some(i) = eixos.iter().position(|&e| e == codigo) else {
                continue;
            };
            aceleracao[i] = valor as f32 / escala[i];
            let Ok(mut lista) = estados.lock() else {
                return false;
            };
            if let Some((_, estado)) = lista.iter_mut().find(|(n, _)| n == no) {
                estado.aceleracao = aceleracao;
                estado.com_leitura = true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sensor(nome: &str, vendor: u16, product: u16) -> EstadoDoSensor {
        EstadoDoSensor {
            nome: nome.into(),
            vendor,
            product,
            aceleracao: [0.0, 0.0, 1.0],
            com_leitura: false,
            sem_permissao: false,
        }
    }

    #[test]
    fn o_sensor_se_liga_ao_controle_pelo_nome_e_pelo_par() {
        let imu = sensor("Pro Controller (IMU)", 0x057e, 0x2009);
        assert!(imu.e_do_controle("Pro Controller", 0x057e, 0x2009));
        // Outro aparelho no mesmo par, ou outro controle com nome parecido, não é ele.
        assert!(!imu.e_do_controle("Joy-Con (L)", 0x057e, 0x2009));
        assert!(!imu.e_do_controle("Pro Controller", 0x057e, 0x2006));
        assert!(!imu.e_do_controle("", 0x057e, 0x2009));
    }

    #[test]
    fn o_segundo_controle_igual_fica_com_o_segundo_sensor() {
        let sensores = Sensores::default();
        {
            let mut lista = sensores.estados.lock().unwrap();
            let mut primeiro = sensor("Pro Controller (IMU)", 0x057e, 0x2009);
            primeiro.aceleracao = [1.0, 0.0, 0.0];
            let mut segundo = sensor("Pro Controller (IMU)", 0x057e, 0x2009);
            segundo.aceleracao = [0.0, 1.0, 0.0];
            lista.push(("/dev/input/event22".into(), primeiro));
            lista.push(("/dev/input/event30".into(), segundo));
        }
        let segundo = sensores.do_controle("Pro Controller", 0x057e, 0x2009, 1).unwrap();
        assert_eq!(segundo.aceleracao, [0.0, 1.0, 0.0]);
        assert!(sensores.do_controle("Pro Controller", 0x057e, 0x2009, 2).is_none());
    }
}
