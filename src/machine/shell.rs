//! IShell: recursos, informação do aparelho e o arranque do applet.

use super::*;

// AEEDeviceItems.h: AEEFSFreeSpace descreve a flash interna com dois uint64.
const DEVICEITEM_FSSPACE_ROOT: u32 = 65;

impl<C: CpuBackend> Machine<C> {
    /// Os três métodos de timer do `IShell`, todos com a assinatura
    /// `(IShell *, [int32 dwMsecs,] PFNNOTIFY pfn, void *pUser)`.
    ///
    /// `PFNNOTIFY` é `void (*)(void *pUser)`; o par `(pfn, pUser)` identifica o timer, e é por
    /// ele que `CancelTimer` e `GetTimerExpiration` o encontram.
    pub(super) fn shell_timer_call(&mut self, name: &str) -> Result<u32, CpuError> {
        let (a1, a2, a3) = (
            self.cpu.read_reg(Reg::R1),
            self.cpu.read_reg(Reg::R2),
            self.cpu.read_reg(Reg::R3),
        );
        Ok(match name {
            "SetTimer" => {
                let callback = Callback {
                    function: a2,
                    context: a3,
                };
                if callback.function == 0 {
                    return Ok(EBADPARM);
                }
                // Rearmar o mesmo par `(pfn, pUser)` substitui o timer anterior, em vez de
                // acumular dois — é o que o BREW faz, e o que o jogo espera ao rearmar dentro
                // do próprio callback.
                self.timers.retain(|timer| timer.callback != callback);
                // **O prazo é `int32`, e negativo quer dizer "já venceu".** O laço de quadro de
                // um jogo arma "duração do quadro menos o que já gastou"; o Powerboat Challenge,
                // depois de carregar a fase inteira num quadro só, pede -2 ms. Lido sem sinal,
                // isso era um prazo de 49 dias, o timer nunca vencia e o jogo ficava para
                // sempre na tela de carregamento.
                let espera = (a1 as i32).max(0) as u32;
                self.timers.push(Timer {
                    deadline_ms: self.now_ms().wrapping_add(espera),
                    callback,
                });
                SUCCESS
            }
            "CancelTimer" => {
                let (function, context) = (a1, a2);
                // `pfn` nulo cancela todos os timers daquele contexto.
                self.timers.retain(|timer| {
                    timer.callback.context != context
                        || (function != 0 && timer.callback.function != function)
                });
                SUCCESS
            }
            // Devolve quanto falta, em milissegundos; zero se não há timer armado.
            _ => {
                let callback = Callback {
                    function: a1,
                    context: a2,
                };
                self.timers
                    .iter()
                    .find(|timer| timer.callback == callback)
                    .map(|timer| {
                        (timer.deadline_ms.wrapping_sub(self.now_ms()) as i32).max(0) as u32
                    })
                    .unwrap_or(0)
            }
        })
    }

    /// `int ISHELL_DetectType(IShell *, const void *cpBuf, uint32 *pdwSize,
    /// const char *cpszName, const char **pcpszMIME)`.
    ///
    /// Descobre o tipo MIME de um conteúdo. O jogo usa a resposta para achar, no registro do
    /// BREW, qual classe sabe abrir aquele arquivo.
    pub(super) fn shell_detect_type(&mut self) -> Result<u32, CpuError> {
        let (buffer, size_ptr, name_ptr, mime_ptr) =
            (self.arg(1), self.arg(2), self.arg(3), self.arg(4));

        // Sem dados e sem nome, a pergunta é "de quantos bytes você precisa?".
        if buffer == 0 && name_ptr == 0 {
            self.write_at(size_ptr, DETECT_TYPE_BYTES)?;
            return Ok(ENEEDMORE);
        }

        let available = if size_ptr == 0 {
            0
        } else {
            self.cpu.read_u32(size_ptr)?
        };
        let bytes = if buffer == 0 {
            Vec::new()
        } else {
            self.read_bytes(buffer, available.min(DETECT_TYPE_BYTES))?
        };
        let name = self.cpu.read_cstring(name_ptr, MAX_STRING);

        match detect_mime(&bytes, &name) {
            Some(mime) => {
                let text = self.intern(mime)?;
                self.write_at(mime_ptr, text)?;
                Ok(SUCCESS)
            }
            None => Ok(ENOTYPE),
        }
    }

    /// `int ISHELL_Resume(IShell *, AEECallback *pcb)` — agenda o callback para a próxima
    /// volta do laço de eventos.
    ///
    /// É o mecanismo em que as threads cooperativas se apoiam: o jogo pede a retomada por
    /// aqui e só então chama `Suspend`, para que a thread tenha como voltar.
    pub(super) fn shell_resume(&mut self) -> Result<u32, CpuError> {
        let pcb = self.cpu.read_reg(Reg::R1);
        if let Some(&thread) = self.resume_callbacks.get(&pcb) {
            if !self.pending_threads.contains(&thread) {
                self.pending_threads.push(thread);
            }
            return Ok(SUCCESS);
        }
        // A identidade do AEECallback é necessária para substituir e cancelar Resume.
        self.schedule_callback(pcb, 0)?;
        Ok(SUCCESS)
    }

    /// `IBase *ISHELL_LoadResObject(IShell *po, const char *pszResFile, uint16 nResID,
    /// AEECLSID cls)`.
    ///
    /// Com `nResID` zero o arquivo inteiro é o recurso — é assim que o Quake carrega
    /// `fs:/~/../id1/splash_title.png`. Com `nResID` diferente de zero o que ele nomeia é um
    /// `.bar`, e a imagem é a entrada daquele número lá dentro.
    ///
    /// Ignorar o `nResID` custou caro: o Tekken 2 pede a entrada 5034 do `tekken2.bar` e nós
    /// tentávamos decodificar os 734 KB do `.bar` inteiro como PNG. Falhava, devolvia nulo, e o
    /// jogo seguia com uma imagem sem tamanho — que é divisão por zero na hora de montar a
    /// tela. O relatório dizia o que estava acontecendo o tempo todo, na linha "um recurso
    /// pedido por LoadResObject não é um PNG que saibamos ler".
    ///
    /// Com `cls` zero, o BREW deduz a classe pelo conteúdo; aqui a única que sabemos produzir é
    /// a imagem, e é só o que os jogos pedem.
    pub(super) fn shell_load_res_object(&mut self) -> Result<u32, CpuError> {
        let guest_path = self
            .cpu
            .read_cstring(self.cpu.read_reg(Reg::R1), MAX_STRING);
        let id = self.cpu.read_reg(Reg::R2) as u16;
        let cls = self.cpu.read_reg(Reg::R3);
        let Some(path) = self.vfs.resolve(&guest_path) else {
            self.missing_files.insert(guest_path);
            return Ok(0);
        };
        let bytes = match id {
            0 => match std::fs::read(&path) {
                Ok(bytes) => bytes,
                Err(_) => {
                    self.missing_files.insert(guest_path);
                    return Ok(0);
                }
            },
            _ => {
                let raw = self
                    .resources
                    .open(&path)
                    .and_then(|res| res.get(crate::loader::resfile::RESTYPE_IMAGE, id))
                    .map(<[u8]>::to_vec);
                let Some(raw) = raw else {
                    self.missing_files
                        .insert(format!("{guest_path} (recurso {id})"));
                    return Ok(0);
                };
                self.recursos_lidos.insert(id);
                // O cabeçalho `AEEResBlob` é nosso para pular: quem pediu foi um **objeto** de
                // imagem, não o bloco bruto que o `LoadResData` entrega.
                crate::loader::resfile::blob_data(&raw)
                    .unwrap_or(&raw)
                    .to_vec()
            }
        };
        let Some(decoded) = self.decode_resource_image(&bytes) else {
            self.assumptions
                .insert("um recurso pedido por LoadResObject veio num formato que não sabemos ler");
            return Ok(0);
        };

        // A classe pedida decide o que sai daqui. O Tekken 2 pede `AEEIID_IBITMAP` — ele quer
        // desenhar com `IDISPLAY_BitBlt`, não com `IIMAGE_Draw` —, e devolver um `IImage` fazia
        // ele chamar um método de `IBitmap` numa vtable de `IImage`: o slot 12, que em `IImage`
        // não existe.
        if cls == AEEIID_IBITMAP {
            let bitmap = self.bitmap_from_decoded(&decoded)?;
            if bitmap != 0 {
                self.publica_bmp_indexado(bitmap, &bytes)?;
            }
            return Ok(bitmap);
        }
        let image = self.new_object(Interface::Image)?;
        if image != 0 {
            self.images.insert(image, std::rc::Rc::new(decoded));
        }
        Ok(image)
    }

    /// Decodifica uma imagem de recurso pelo que ela é.
    ///
    /// O PNG passa pelo caminho próprio, que traz o canal alfa que os jogos usam para recortar
    /// o sprite. O resto — BMP e JPEG — vem pelo decodificador dos ícones, que já sabe lê-los e
    /// entrega tudo opaco, que é o que esses dois formatos são.
    pub(super) fn decode_resource_image(&mut self, bytes: &[u8]) -> Option<DecodedImage> {
        if let Some(decoded) = decode_png(bytes) {
            return Some(decoded);
        }
        if let Some(gif) = crate::video::gif::decodifica(bytes) {
            return Some(tira_de_quadros(&gif));
        }
        let image = crate::video::icon::decode(bytes).ok()?;
        let count = image.width * image.height;
        let mut pixels = Vec::with_capacity(count);
        for at in (0..count * 4).step_by(4) {
            pixels.push(
                Rgb {
                    r: image.rgba[at],
                    g: image.rgba[at + 1],
                    b: image.rgba[at + 2],
                }
                .to_rgb565(),
            );
        }
        Some(DecodedImage {
            width: image.width as u32,
            height: image.height as u32,
            pixels,
            opaque: vec![true; count],
            alfa: Vec::new(),
            frame_width: 0,
        })
    }

    /// Abre o arquivo de recursos que o jogo nomeou.
    ///
    /// O nome pode vir nulo, e o Peggle manda nulo: o BREW entende isso como "o arquivo de
    /// recursos deste applet". Como não há convenção de nome que sirva — o Peggle chama o dele
    /// de `resources.bar` e o Pac-Mania de `pacmania.bar` —, o que resta é o único `.bar` que
    /// existe ao lado do módulo. Havendo mais de um, não há como escolher, e ninguém abre.
    pub(super) fn open_res_file(
        &mut self,
        pointer: u32,
    ) -> Option<&crate::loader::resfile::ResFile> {
        let path = match pointer {
            0 => self.default_res_file()?,
            _ => {
                let guest_path = self.cpu.read_cstring(pointer, MAX_STRING);
                match self.vfs.resolve(&guest_path) {
                    Some(path) => path,
                    None => {
                        self.missing_files.insert(guest_path);
                        return None;
                    }
                }
            }
        };
        self.resources.open(&path)
    }

    /// O único `.bar` ao lado do módulo, se houver exatamente um.
    pub(super) fn default_res_file(&self) -> Option<std::path::PathBuf> {
        let mut found = std::fs::read_dir(self.vfs.root())
            .ok()?
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("bar"));
        let first = found.next()?;
        match found.next() {
            None => Some(first),
            Some(_) => None,
        }
    }

    /// Escolhe um idioma quando o banco de preferências ainda não tem um.
    ///
    /// A Z-Wheel guarda em `PREFSINFO.Lang` a **etiqueta** do idioma empacotada em quatro
    /// bytes — `"pt  "` vira `0x20207470` —, e não um índice. Quem monta a tela do z-pad
    /// percorre a lista de idiomas do módulo comparando a etiqueta de cada um com esse valor;
    /// sem achar, devolve zero e o jogo repete `Couldn't create z-pad instruction form (6)`
    /// para sempre. O pacote nasce com `Lang = 0`, que é "ninguém escolheu ainda": no console
    /// quem preenche isso é a tela de primeira configuração, que ainda não alcançamos.
    ///
    /// Escolher por conta é uma hipótese, e fica anotada como tal. É o português porque é o
    /// idioma do aparelho que a TecToy vendeu, e porque `tectoy_pt.brf` está no pacote.
    pub(super) fn escolhe_idioma(&mut self, db: &crate::brew::sql::Database) {
        /// `"pt  "` lido como uma palavra de 32 bits, que é a forma como o módulo compara.
        const PORTUGUES: u32 = u32::from_le_bytes(*b"pt  ");
        let sem_escolha = db
            .exec("SELECT dwValue FROM PREFSINFO WHERE PREFSINFO.name = 'Lang'")
            .ok()
            .and_then(|linhas| linhas.into_iter().next())
            .and_then(|linha| linha.values.into_iter().next().flatten())
            .is_some_and(|valor| valor == "0");
        if !sem_escolha {
            return;
        }
        let gravou = db.exec(&format!(
            "UPDATE PREFSINFO SET dwValue = {PORTUGUES} WHERE PREFSINFO.name = 'Lang'"
        ));
        if gravou.is_ok() {
            self.assumptions
                .insert("o idioma não estava escolhido no banco e assumimos português");
        }
    }

    /// `int ISHELL_LoadResString(IShell *po, const char *pszResFile, int16 nResID,
    /// AECHAR *pBuff, int nSize)`.
    ///
    /// Devolve **quantos caracteres** foram escritos, e zero quando o recurso não existe — que
    /// é resposta legítima, não erro.
    pub(super) fn shell_load_res_string(&mut self) -> Result<u32, CpuError> {
        let (file, id) = (
            self.cpu.read_reg(Reg::R1),
            self.cpu.read_reg(Reg::R2) as u16,
        );
        let (buffer, capacity) = (self.cpu.read_reg(Reg::R3), self.stack_arg(0)?);
        let Some(text) = self.open_res_file(file).and_then(|res| res.string(id)) else {
            return Ok(0);
        };
        if buffer == 0 || capacity < 2 {
            return Ok(0);
        }
        // `nSize` é em **bytes**, e cada `AECHAR` ocupa dois; o terminador entra na conta.
        let room = (capacity as usize / 2).saturating_sub(1);
        let written = text.len().min(room);
        let bytes: Vec<u8> = text[..written]
            .iter()
            .chain(std::iter::once(&0))
            .flat_map(|u| u.to_le_bytes())
            .collect();
        self.cpu.write_mem(buffer, &bytes)?;
        Ok(written as u32)
    }

    /// `void *ISHELL_LoadResData(IShell *po, const char *pszResFile, uint16 nResID,
    /// ResType nType)` e a variante `Ex`, que ainda recebe `void *pBuf, uint32 *pnBufSize`.
    ///
    /// A documentação é explícita: o que sai daqui é o conteúdo **bruto** do recurso, cabeçalho
    /// `AEEResBlob` incluído. Quem interpreta é o jogo, com o `RESBLOB_DATA()`.
    pub(super) fn shell_load_res_data(&mut self, with_buffer: bool) -> Result<u32, CpuError> {
        let (file, id) = (
            self.cpu.read_reg(Reg::R1),
            self.cpu.read_reg(Reg::R2) as u16,
        );
        let kind = self.cpu.read_reg(Reg::R3) as u16;
        let Some(data) = self
            .open_res_file(file)
            .and_then(|res| res.get(kind, id))
            .map(<[u8]>::to_vec)
        else {
            return Ok(0);
        };
        let len = data.len() as u32;

        if !with_buffer {
            // Sem buffer do chamador, o BREW aloca — e quem libera é o `FreeResData`.
            let ptr = self.malloc(len)?;
            if ptr != 0 {
                self.cpu.write_mem(ptr, &data)?;
            }
            return Ok(ptr);
        }

        let (buffer, size_out) = (self.stack_arg(0)?, self.stack_arg(1)?);
        // `pBuf` valendo -1 pede só o tamanho, sem copiar nada.
        if buffer == u32::MAX {
            self.write_at(size_out, len)?;
            return Ok(u32::MAX);
        }
        if buffer == 0 {
            let ptr = self.malloc(len)?;
            if ptr != 0 {
                self.cpu.write_mem(ptr, &data)?;
            }
            self.write_at(size_out, len)?;
            return Ok(ptr);
        }
        // Com buffer do chamador, `*pnBufSize` chega dizendo o tamanho dele. Não cabendo, a
        // documentação manda devolver nulo.
        let declared = match size_out {
            0 => 0,
            _ => self.cpu.read_u32(size_out)?,
        };
        // Nem todo jogo preenche o `*pnBufSize` de entrada. O Peggle consulta o tamanho do
        // recurso, aloca os 64.629 bytes que a consulta devolveu e chama de novo com o
        // `*pnBufSize` valendo 6 — o número do tipo, que ficou na variável. Recusar por causa
        // disso deixava o buffer zerado, e o decodificador recebia sessenta mil bytes de zeros:
        // o jogo quebrava logo depois, num ponteiro nulo que ele nem confere.
        //
        // O tamanho do bloco no heap é a medida honesta: não é o número que o jogo disse, é o
        // que ele de fato reservou. Assim a cópia continua limitada ao que existe.
        let capacity = declared.max(self.heap.size_of(buffer).unwrap_or(0));
        if capacity < len {
            self.write_at(size_out, len)?;
            return Ok(0);
        }
        self.cpu.write_mem(buffer, &data)?;
        self.write_at(size_out, len)?;
        Ok(buffer)
    }

    /// `uint32 ISHELL_GetClassItemID(IShell *po, AEECLSID cls)`.
    ///
    /// O item ID é o número que a loja do BREW dá ao pacote que instalou o módulo — e é
    /// literalmente o nome do diretório em que o `.mod` vive (`mod/277083/bjt.mod`). Não é
    /// palpite: é a mesma numeração que aparece no `.mif` e no caminho da ROM.
    ///
    /// A documentação manda devolver 0 quando a classe não é de um módulo baixado, que é o
    /// que fazemos para qualquer classe que não seja a do applet carregado.
    pub(super) fn shell_get_class_item_id(&mut self) -> u32 {
        let cls = self.cpu.read_reg(Reg::R1);
        self.item_id_de(cls)
    }

    /// O item ID de uma classe instalada. Ver [`Machine::shell_get_class_item_id`].
    ///
    /// **Esta correção não foi a causa do gesto da Z-Wheel, e é bom que fique dito.** Eu cheguei a
    /// ela lendo o desmonte do tratador de evento da roda, que chama um método por um deslocamento
    /// de vtable — e o instrumento mostrou que `GetClassItemID` **nunca é despachado**. O
    /// deslocamento era de outro objeto. O que fica é o que a API manda: o item ID é do módulo
    /// instalado, e devolver zero para um applet que existe é resposta errada, com ou sem a
    /// Z-Wheel.
    fn item_id_de(&self, cls: u32) -> u32 {
        // **O item ID vale para qualquer applet instalado, e não só para o carregado.** Era aqui
        // que a Z-Wheel parava: ela pergunta o id do jogo **escolhido**, que por definição não é a
        // classe dela, e a resposta era sempre zero. Sem o id, o lançamento nunca acontece — ela
        // navega, a tela muda, e `StartApplet` jamais é chamado. Medido no rastreio do `IShell`,
        // com a chamada aparecendo a cada tecla e o valor sempre zero.
        //
        // O número sai do nome do `.mif`, que é o número da pasta do módulo — a mesma numeração do
        // caminho e do `ISHELL_GetClassItemID` do console.
        if let Some((_, nome)) = self
            .modulos_instalados
            .iter()
            .find(|(classe, _)| *classe == cls)
        {
            if let Ok(id) = nome.parse() {
                return id;
            }
        }
        // A classe do applet carregado responde pelo próprio diretório, como antes: é o caso do
        // jogo que roda sozinho e pergunta o id de si mesmo.
        if cls == self.applet_class {
            return self
                .vfs
                .root()
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.parse().ok())
                .unwrap_or(0);
        }
        0
    }

    /// `ISHELL_GetDeviceInfo(IShell *po, AEEDeviceInfo *pi)`.
    ///
    /// Preenchemos só os campos do começo da struct, que têm posição inequívoca: os oito
    /// `uint16` iniciais. O que vem depois depende de como o compilador empacota bitfields, e
    /// chutar layout aqui seria pior do que deixar zerado. Os campos a partir de `wStructSize`
    /// são preenchidos pelo próprio chamador e não são tocados.
    pub(super) fn shell_get_device_info(&mut self) -> Result<u32, CpuError> {
        let info = self.cpu.read_reg(Reg::R1);
        if info == 0 {
            return Ok(SUCCESS);
        }

        // O chamador escreve `wStructSize` **antes** da chamada para pedir os campos
        // estendidos, e o Bejeweled Twist pede: manda 64. Enquanto só preenchíamos os 44
        // primeiros bytes, ele recebia `wMaxPath = 0` — nenhum caminho de arquivo caberia.
        let mut requested = [0u8; 2];
        self.cpu.read_mem(info + 44, &mut requested)?;
        let requested = u16::from_le_bytes(requested) as u32;

        // Zera até `dwLang`, o último campo antes da parte que o chamador preenche.
        self.cpu.write_mem(info, &[0u8; 44])?;

        let fields: [u16; 8] = [
            self.screen().width() as u16,
            self.screen().height() as u16,
            0, // cxAltScreen: o Zeebo não tem segunda tela
            0, // cyAltScreen
            8, // cxScrollBar
            AEE_ENC_ISOLATIN1,
            0, // wMenuTextScroll
            COLOR_DEPTH,
        ];
        for (i, value) in fields.iter().enumerate() {
            self.cpu
                .write_mem(info + i as u32 * 2, &value.to_le_bytes())?;
        }
        // AEEShell.h: os flg ocupam 32 bits em +28 no ARM. O áudio suporta MIDI/CMX;
        // o frontend móvel também entrega PEN/POINTER. Zerar estes bits fazia jogos
        // desabilitarem recursos que o runtime já oferece.
        let capabilities = (1u32 << 3)
            | (1 << 6)
            | (1 << 7)
            | if self.handset_clock.is_some() {
                1 << 8
            } else {
                0
            };
        self.cpu.write_u32(info + 28, capabilities)?;
        // `dwRAM` — a documentação chama de "tamanho inicial do heap do BREW".
        self.cpu.write_u32(info + 24, loader::HEAP_SIZE as u32)?;

        if requested >= DEVICE_INFO_SIZE {
            self.cpu
                .write_mem(info + 44, &(DEVICE_INFO_SIZE as u16).to_le_bytes())?;
            self.cpu.write_u32(info + 48, 0)?; // dwNetLinger: sem rede, nada a manter aberto
            self.cpu.write_u32(info + 52, 0)?; // dwSleepDefer: o console não dorme
            self.cpu
                .write_mem(info + 56, &(AEE_MAX_FILE_NAME as u16).to_le_bytes())?;
            self.cpu.write_u32(info + 60, 0)?; // dwPlatformID: não sabemos o do Zeebo
        }
        Ok(SUCCESS)
    }

    /// `ISHELL_GetDeviceInfoEx(IShell *po, AEEDeviceItem nItem, void *pBuff, int *pnSize)`.
    ///
    /// `pnSize` é de entrada e saída: entra com o tamanho do buffer e sai com o que o item
    /// precisa. Com `pBuff` nulo a chamada serve só para perguntar esse tamanho, e é assim que
    /// o Zeebo Sports Peteca pede o IMEI — devolver sucesso sem escrever ali deixava o jogo
    /// alocar com lixo e ler fora da memória.
    pub(super) fn shell_get_device_info_ex(&mut self) -> Result<u32, CpuError> {
        let (item, buffer, size) = (
            self.cpu.read_reg(Reg::R1),
            self.cpu.read_reg(Reg::R2),
            self.cpu.read_reg(Reg::R3),
        );
        if size == 0 {
            return Ok(EBADPARM);
        }
        // **O item pedido vai para a captura de serial**, e é o que faltava para responder "a
        // Z-Wheel não monta a loja" com uma medida em vez de um palpite: um `EUNSUPPORTED` sem
        // registro é indistinguível de nunca ter sido perguntado. O item que interessa aqui é o
        // `AEE_DEVICEITEM_ADS` (29, do `AEEDeviceItems.h`): o nome do servidor de onde a loja
        // baixa o catálogo.
        if self.serial.is_some() {
            let como = match item {
                DEVICEITEM_IMEI | DEVICEITEM_FSSPACE_ROOT => "atendido",
                _ => "não atendido",
            };
            self.registra_serial(format!("<aparelho item {item} -> {como}>"));
        }
        if item == DEVICEITEM_FSSPACE_ROOT {
            let capacity = self.cpu.read_u32(size)?;
            // OEMAppFuncs_Sim.c só informa o tamanho quando não cabe a struct inteira.
            // Shift lê qwFreeSpace em +8 e exige 20.000 bytes antes de abrir os dados;
            // EUNSUPPORTED deixava esse campo zerado e acionava o erro de arranque.
            if buffer == 0 || capacity < 16 {
                self.cpu.write_u32(size, 16)?;
            } else {
                self.cpu
                    .write_mem(buffer, &u64::from(FS_TOTAL_BYTES).to_le_bytes())?;
                self.cpu
                    .write_mem(buffer + 8, &u64::from(FS_FREE_BYTES).to_le_bytes())?;
            }
            return Ok(SUCCESS);
        }
        let value = match item {
            DEVICEITEM_IMEI => IMEI,
            _ => return Ok(EUNSUPPORTED),
        };
        let capacity = self.cpu.read_u32(size)? as usize;
        self.cpu.write_u32(size, value.len() as u32)?;
        if buffer != 0 {
            // Cabendo ou não, o que couber é escrito: a documentação prevê o preenchimento
            // parcial, com `pnSize` dizendo quanto faltou.
            self.cpu
                .write_mem(buffer, &value[..capacity.min(value.len())])?;
        }
        Ok(SUCCESS)
    }

    /// Manda `EVT_APP_START` para o applet, que é o que dá partida no jogo.
    ///
    /// `boolean IAPPLET_HandleEvent(IApplet *po, AEEEvent evt, uint16 wParam, uint32 dwParam)`
    /// — slot 2 da vtable de `IApplet`. Em `EVT_APP_START`, `dwParam` aponta para um
    /// `AEEAppStart`, que montamos no heap.
    pub fn start_applet(
        &mut self,
        applet: u32,
        clsid: u32,
        budget: u64,
    ) -> Result<Outcome, CpuError> {
        let vtable = self.cpu.read_u32(applet)?;
        let handle_event = self.cpu.read_u32(vtable + 2 * 4)?;
        // A partir daqui `GetAppInstance` tem o que devolver — e passa a responder sem sair
        // da CPU.
        self.current_applet = applet;
        self.install_app_instance_stub(applet)?;

        let display = self.objects.create(Interface::Display).unwrap_or(0);
        if display != 0 {
            self.cpu
                .write_u32(display, loader::vtable_addr(Interface::Display))?;
        }

        // AEEAppStart: error, clsApp, pDisplay, rc (4 × int16), pszArgs = 24 bytes.
        let start = self.heap.alloc(24).unwrap_or(0);
        if start != 0 {
            self.cpu.write_mem(start, &[0u8; 24])?;
            self.cpu.write_u32(start + 4, clsid)?;
            self.cpu.write_u32(start + 8, display)?;
            for (i, value) in [
                0i16,
                0,
                self.screen().width() as i16,
                self.screen().height() as i16,
            ]
            .iter()
            .enumerate()
            {
                self.cpu
                    .write_mem(start + 12 + i as u32 * 2, &value.to_le_bytes())?;
            }
        }

        self.call_guest(handle_event, [applet, EVT_APP_START, 0, start], budget)
    }

    /// Enfileira uma tecla do teclado, pelo código virtual do BREW.
    ///
    /// Não vai direto: teclado chega ao jogo como **evento**, e evento só pode ser entregue na
    /// fronteira entre duas chamadas de API — chamar o tratador do jogo no meio de um despacho é
    /// o caminho que já derrubou o Zeeboids. A fila é esvaziada em [`Machine::deliver_signals`].
    /// Os applets instalados, cada um com o id do módulo — o nome do `.mif` sem extensão.
    pub fn set_installed_applets(&mut self, applets: impl IntoIterator<Item = (u32, String)>) {
        self.modulos_instalados = applets.into_iter().collect();
        self.installed_applets = self.modulos_instalados.iter().map(|(c, _)| *c).collect();
    }

    /// Entrega ao applet um evento com `wParam`, como um widget faria ao ser ativado.
    ///
    /// **É instrumento de teste, e só isso.** O evento `0x7000` com `w = 0x4ea` é o que o tratador
    /// da Z-Wheel lê para chamar `IShell::StartApplet` (medido no desmonte, em `0x7b4c0` e
    /// `0x7b51c`): quem o manda no console é um widget, e no emulador a lista de widgets da roda
    /// está vazia. Isto permite exercitar **o resto do ciclo** — o pedido de abertura, a troca de
    /// sessão e a volta — sem depender do widget que falta.
    pub fn entrega_evento_ao_applet(&mut self, evt: u32, w: u16) -> Result<u32, CpuError> {
        self.send_applet_event(self.applet_class, evt, w, 0)
    }

    /// Entrada handset de topo: conserva Budget/fault e nunca interrompe outro tratador.
    pub fn deliver_handset_event(
        &mut self,
        event: crate::handset::input::KeyEvent,
        budget: u64,
    ) -> Result<Outcome, CpuError> {
        self.deliver_handset_input(event.kind.code(), u32::from(event.key.code()), 0, budget)
    }

    /// AVKs Zeebo usam a mesma fronteira segura do teclado móvel, sem virar dígitos.
    pub fn deliver_handset_avk(
        &mut self,
        avk: u32,
        down: bool,
        budget: u64,
    ) -> Result<Outcome, CpuError> {
        self.deliver_handset_input(
            if down {
                crate::input::EVT_KEY
            } else {
                crate::input::EVT_KEY_RELEASE
            },
            avk,
            0,
            budget,
        )
    }

    pub fn deliver_handset_pen(
        &mut self,
        event: crate::handset::input::PenEvent,
        budget: u64,
    ) -> Result<Outcome, CpuError> {
        let (applet, handler) = self.handset_input_handler()?;
        if matches!(
            self.copyright_input(event.kind.code(), 0)?,
            super::copyright::CopyrightInput::Consume
        ) {
            return Ok(Outcome::Returned { code: TRUE });
        }

        // AEE.h: POINTER traz texto e precede PEN; só FALSE autoriza o fallback.
        // A build Samsung de Assassin's Creed lê x/y nos offsets 2/13 desse texto.
        let text = format!(
            "x={:08X},y={:08X},time={:08X},clkcnt=1,modifiers=0000000000000000,\0",
            event.x,
            event.y,
            self.now_ms()
        );
        const FRAME_SIZE: u32 = 128;
        let original_sp = self.cpu.read_reg(Reg::Sp);
        let sp = original_sp
            .checked_sub(FRAME_SIZE)
            .filter(|sp| *sp >= loader::STACK_BASE + 16)
            .filter(|_| original_sp <= loader::STACK_BASE + loader::STACK_SIZE as u32)
            .ok_or_else(|| CpuError("No stack space for handset pointer event".into()))?
            & !7;
        let mut frame = [0u8; FRAME_SIZE as usize];
        for (index, word) in [handler, event.kind.code(), event.packed(), original_sp]
            .into_iter()
            .enumerate()
        {
            frame[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
        frame[16..16 + text.len()].copy_from_slice(text.as_bytes());

        // O adaptador termina no guest: Budget pode cortar o handler ou o fallback sem
        // perder argumentos, duplicar toque ou depender de estado do host fora do save.
        // O primeiro stub (GetAppInstance) ocupa 12 bytes; este começa em +0x40.
        const POINTER_STUB: u32 = loader::STUB_BASE + 0x40;
        let code = [
            0xe92d_4070u32, // push {r4,r5,r6,lr}; mantém alinhamento AAPCS
            0xe1a0_4000,    // mov r4,r0 (applet)
            0xe59d_5010,    // ldr r5,[sp,#16] (handler)
            0xe1a0_e00f,
            0xe12f_ff15, // mov lr,pc; bx r5 (ARM/Thumb)
            0xe350_0000, // cmp r0,#0
            0x1a00_0005, // bne fim
            0xe1a0_0004, // mov r0,r4
            0xe59d_1014, // ldr r1,[sp,#20] (EVT_PEN)
            0xe3a0_2000, // mov r2,#0
            0xe59d_3018, // ldr r3,[sp,#24] (x/y empacotados)
            0xe1a0_e00f,
            0xe12f_ff15, // mov lr,pc; bx r5
            0xe8bd_4070, // fim: pop {r4,r5,r6,lr}
            0xe59d_d00c, // ldr sp,[sp,#12] (pilha original)
            0xe12f_ff1e, // bx lr
        ];
        self.cpu.write_mem(
            POINTER_STUB,
            &code
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )?;
        self.cpu.write_mem(sp, &frame)?;
        self.cpu.write_reg(Reg::Sp, sp);
        self.call_guest(
            POINTER_STUB,
            [applet, event.kind.code() + 7, text.len() as u32, sp + 16],
            budget,
        )
    }

    fn handset_input_handler(&self) -> Result<(u32, u32), CpuError> {
        if self.trecho_interrompido.is_some() || self.nesting != 0 || self.current_thread.is_some()
        {
            return Err(CpuError(
                "Guest continuation must finish before handset input".into(),
            ));
        }
        let applet = self.current_applet;
        if applet == 0 {
            return Err(CpuError("Handset applet is not running".into()));
        }
        let table = self.cpu.read_u32(applet)?;
        let handler = self.cpu.read_u32(table + 8)?;
        if handler == 0 {
            return Err(CpuError("Handset applet has no event handler".into()));
        }
        Ok((applet, handler))
    }

    fn deliver_handset_input(
        &mut self,
        code: u32,
        w: u32,
        dw: u32,
        budget: u64,
    ) -> Result<Outcome, CpuError> {
        let (applet, handler) = self.handset_input_handler()?;
        match self.copyright_input(code, w as u16)? {
            super::copyright::CopyrightInput::Consume => {
                return Ok(Outcome::Returned { code: TRUE });
            }
            super::copyright::CopyrightInput::End => {
                return self.call_guest(
                    handler,
                    [applet, super::copyright::COPYRIGHT_END, 0, 0],
                    budget,
                );
            }
            super::copyright::CopyrightInput::Pass => {}
        }
        self.call_guest(handler, [applet, code, w, dw], budget)
    }

    /// O host troca de sessão depois que a chamada do guest terminou.
    pub fn take_launch_request(&mut self) -> Option<u32> {
        self.pending_launch.take()
    }

    // O SDK dá à versão 3.1.3 outro ClassID, com a mesma vtable de IFileMgr.
    // DiRT pede esta classe e usa GetInfo já no construtor do applet.
    // O decodificador é um por formato no console, e todos são `IImage` alimentados por
    // `IAStream`. O nosso olha a assinatura dos bytes, então os quatro caem no mesmo
    // lugar — e faltando o JPEG na lista, o Zuma's Revenge recebia recusa e seguia com
    // um ponteiro nulo até quebrar.
    // As outras cinco extensões gráficas, para quem as pedir por `CreateInstance` em vez
    // de `QueryInterface` — o Prey Evil usa o segundo caminho, mas a fábrica é a porta
    // documentada do BREW.
    fn native_class(clsid: u32) -> Option<Interface> {
        Some(match clsid {
            AEECLSID_DISPLAY | AEECLSID_DISPLAY1 => Interface::Display,
            AEECLSID_FILEMGR | 0x0102_d575 => Interface::FileMgr,
            AEECLSID_HID => Interface::Hid,
            AEECLSID_SIGNAL_CB_FACTORY => Interface::SignalCbFactory,
            AEECLSID_GRAPHICS => Interface::Graphics,
            AEECLSID_SOUND => Interface::Sound,
            0x0100_100b => Interface::SoundLegacy,
            AEECLSID_SOUNDPLAYER => Interface::SoundPlayer,
            AEECLSID_HEAP => Interface::Heap,
            AEECLSID_UNZIPSTREAM => Interface::UnzipStream,
            AEECLSID_LICENSE => Interface::License,
            AEECLSID_MEMASTREAM => Interface::MemAStream,
            AEECLSID_PNG | AEECLSID_BMP | AEECLSID_JPEG | AEECLSID_GIF => Interface::Image,
            AEECLSID_THREAD => Interface::Thread,
            AEECLSID_QEGL => Interface::Egl,
            AEECLSID_GLES11EXT => Interface::Gles11Ext,
            AEEIID_GLES10EXT => Interface::Gles10Ext,
            AEEIID_GLES11EXTPAK => Interface::Gles11ExtPak,
            AEEIID_EGLGETCOLORBUFFER => Interface::EglGetColorBuffer,
            AEEIID_EGLGETPOWERLEVEL => Interface::EglGetPowerLevel,
            AEEIID_EGLOESSWAPINTERVAL => Interface::EglOesSwapInterval,
            AEECLSID_IJOYSTICK1 | AEECLSID_IJOYSTICK2 => Interface::Joystick,
            AEECLSID_EGL => Interface::EglLegacy,
            AEECLSID_GL => Interface::GlLegacy,
            AEECLSID_MEDIAUTIL => Interface::MediaUtil,
            AEECLSID_WEB => Interface::Web,
            0x0100_1105 | 0x0100_102e => Interface::NetLegacy,
            0x0100_1007 => Interface::TapiLegacy,
            0x0100_5001 => Interface::WebUtil,
            AEECLSID_COLLECTION => Interface::Collection,
            AEECLSID_SQLMGR => Interface::SqlMgr,
            0x0100_1008 => Interface::DBMgrLegacy,
            AEECLSID_SOURCEUTIL => Interface::SourceUtil,
            _ if FAMILIA_DOS_WIDGETS.contains(&clsid) => Interface::Widget,
            AEECLSID_CONTROL => Interface::Control,
            AEECLSID_ZEEBOMCP => Interface::ZeeboMcp,
            AEECLSID_CONFIG => Interface::Config,
            AEECLSID_VETOR => Interface::Vetor,
            AEECLSID_28E3C => Interface::Classe28e3c,
            AEECLSID_CM => Interface::Cm,
            AEECLSID_SYSTEMCTL => Interface::SystemCtl,
            // AEECLSID_SIMCARDCTL => Interface::SimCardCtl,
            AEECLSID_TYPEFACE => Interface::Typeface,
            AEECLSID_MD5 | 0x0100_1034 => Interface::Hash,
            AEECLSID_MD5CTX => Interface::HashCtx,
            0x0100_101a => Interface::Rsa,
            AEECLSID_CIPHER_FACTORY => Interface::CipherFactory,
            AEECLSID_MEDIA
            | AEECLSID_MEDIAMIDI
            | AEECLSID_MEDIAMP3
            | AEECLSID_MEDIAMIDIOUTMSG
            | AEECLSID_MEDIAADPCM
            | AEECLSID_MEDIAPCM => Interface::Media,
            _ if AEECLSID_MEDIA_FAMILIA.contains(&clsid) => Interface::Media,
            AEECLSID_PNGDECODER | AEECLSID_PNGDECODER_BREW | AEECLSID_JPEGDECODER_BREW => {
                Interface::ImageDecoder
            }
            _ => return None,
        })
    }

    /// QueryClass não cria objetos nem anuncia um serviço desconhecido como disponível.
    pub(super) fn shell_query_class(&mut self) -> Result<u32, CpuError> {
        let class = self.arg(1);
        let out = self.arg(2);
        let info = self.applet_manifest.as_ref().and_then(|(bytes, name)| {
            let mif = crate::loader::miffile::MifFile::parse(bytes).ok()?;
            if !mif.applets.contains(&class) {
                return None;
            }
            let &(offset, _) = mif.sections.iter().find(|&&(offset, length)| {
                length == 20
                    && bytes.get(offset as usize..offset as usize + 4) == Some(&class.to_le_bytes())
            })?;
            let record: [u8; 20] = bytes[offset as usize..offset as usize + 20]
                .try_into()
                .ok()?;
            Some((record, *name))
        });
        if let Some((mut info, name)) = info {
            if out != 0 {
                let path = name;
                info[4..8].copy_from_slice(&path.to_le_bytes());
                self.cpu.write_mem(out, &info)?;
            }
            return Ok(TRUE);
        }
        if out != 0 {
            return Ok(FALSE);
        }
        Ok(u32::from(
            Self::native_class(class).is_some()
                || self.tem_extensao_para(class)
                || crate::machine::font::CLASSES_DE_FONTE.contains(&class)
                || matches!(
                    class,
                    0x01003000
                        | 0x01003001
                        | 0x01003100
                        | 0x01003101
                        | 0x01003009
                        | 0x0100100a
                        | 0x0100110a
                ),
        ))
    }

    /// `ISHELL_CreateInstance(IShell *po, AEECLSID ClsId, void **ppobj)`.
    ///
    /// É a fábrica de objetos do BREW inteiro. Cada ClassID conhecido vira um objeto nosso com
    /// a vtable da interface correspondente; o que não conhecemos devolve `ECLASSNOTSUPPORT`,
    /// que é uma resposta legítima — o jogo trata classe ausente como plataforma sem aquele
    /// recurso, em vez de quebrar.
    pub(super) fn shell_create_instance(&mut self) -> Result<u32, CpuError> {
        let clsid = self.cpu.read_reg(Reg::R1);
        let out = self.cpu.read_reg(Reg::R2);
        // Toda classe pedida entra no relatório, e não só a que não soubemos atender: sem a lista
        // completa não há como ver que o jogo pediu uma classe que respondemos com a **interface
        // errada** — que foi o caso das fontes do sistema.
        *self.classes_pedidas.entry(clsid).or_insert(0) += 1;
        if self.serial.is_some() {
            self.registra_serial(format!("<classe {clsid:#010x}>"));
        }

        let iface = match Self::native_class(clsid) {
            Some(iface) => iface,
            None => match clsid {
                // AEEMenu.h conserva os 26 slots originais antes das extensões 1.NNN.
                // As classes 1.0 usam a mesma ABI; LOTR ainda as pede ao abrir o menu.
                0x0100_3000 | 0x0100_3001 | 0x0100_3100 | 0x0100_3101 | 0x0100_3009
                | 0x0100_100a | 0x0100_110a => {
                    use super::legacy_controls::LegacyControlKind;
                    let kind = match clsid {
                        0x0100_3000 | 0x0100_3100 => LegacyControlKind::Menu,
                        0x0100_3001 | 0x0100_3101 => LegacyControlKind::Softkey,
                        0x0100_3009 => LegacyControlKind::Text100,
                        _ => LegacyControlKind::Static,
                    };
                    return self.cria_controle_legado(kind, out);
                }
                // **`0x01006c01` estava de fora**, e a propria Z-Wheel diz o preco: ela registra
                // `ERROR: Unable to create instance of AEECLSID_LCT_SIMCARDCTL, cannot do SIM check`
                // (`tectoymain.c:1668`). A interface existe e e atendida desde sempre
                // ([`Interface::SimCardCtl`]); o que faltava era a fabrica conhecer a classe, como ja
                // conhecia a vizinha `0x01006c02`. Um `CreateInstance` que devolve nulo vira ponteiro
                // nulo dentro do applet, e e assim que "falta uma classe" reaparece adiante como
                // "acesso invalido a 0x0", tres camadas depois.
                // **`AEECLSID_SIMCARDCTL` fica de FORA, e é medido duas vezes.** O `268d6fb` a pôs
                // aqui para calar o `tectoymain.c:1668 ERROR: Unable to create instance of
                // AEECLSID_LCT_SIMCARDCTL, cannot do SIM check`, e o comentário longo da
                // [`Interface::SimCardCtl`](crate::brew::aee::Interface::SimCardCtl) já dizia que
                // aquele log **é o jogo tomando o caminho certo**: recusada, a Z-Wheel põe o estado
                // em `0x27` e o `0x82464` chama a `0x1f7b4`, que avança a interface; oferecida, o
                // slot 3 (a verificação) responde zero, o estado vira `0x28`, e o `0x82464` **não faz
                // nada com ele** — a tela fica onde está, calada. Medido no harness sem janela em
                // 22/09/2026: com a classe oferecida, a roda para depois de 7 s de relógio virtual,
                // com três widgets na árvore (a raiz e dois objetos vazios), nunca cria o palco
                // (`0x01028e05`) nem o roller (`0x01028e14`), e sete teclas do roteiro não mudam um
                // pixel. É o `0x28` que o comentário descreve.
                // **`IFont`, não `ITypeface`.** O `AEECLSID_ROLLER_FONT` (0x0102f67c) é o
                // `FONT_STANDARD18B`, uma fonte do sistema — estava mapeado para o `ITypeface`, que é
                // outra interface, com outros métodos.
                _ if crate::machine::font::CLASSES_DE_FONTE.contains(&clsid) => {
                    let resultado = self.cria_fonte(clsid, out)?;
                    if resultado != SUCCESS {
                        self.unknown_classes.insert(clsid);
                    }
                    return Ok(resultado);
                }
                // O módulo de extensão entra antes da sonda e antes da recusa, porque ele é a
                // resposta **certa**: a classe existe, implementada em ARM pelo próprio pacote do
                // jogo, e é assim que o console a atende.
                _ if self.tem_extensao_para(clsid) => {
                    let objeto = self.cria_pela_extensao(clsid)?;
                    if objeto == 0 {
                        self.unknown_classes.insert(clsid);
                        if out != 0 {
                            self.cpu.write_u32(out, 0)?;
                        }
                        return Ok(ECLASSNOTSUPPORT);
                    }
                    if out != 0 {
                        self.cpu.write_u32(out, objeto)?;
                    }
                    return Ok(SUCCESS);
                }
                // **`AEECLSID_DOWNLOAD`.** Ver [`AEECLSID_DOWNLOAD`]: a classe do `IDownload`, que
                // o SDK lista como `AEECLSID_PRIV` (= `QVERSION` = `0x01000000`). Atendida pela
                // sonda por enquanto — o que a Z-Wheel chama nela sai no relatório com
                // `ZEEBX_ROM_SONDA`, e é essa medição que diz quais slots valem implementar.
                AEECLSID_DOWNLOAD => {
                    let object = self.new_object(Interface::Probe)?;
                    if object == 0 {
                        return Ok(ENOMEMORY);
                    }
                    self.assumptions.insert(
                        "a classe do download é atendida por um objeto que responde sucesso a tudo",
                    );
                    self.probe_objects.insert(object, clsid);
                    if out != 0 {
                        self.cpu.write_u32(out, object)?;
                    }
                    return Ok(SUCCESS);
                }
                // Uma classe do firmware que o jogo usa sem conferir: ver [`CLASSES_POR_OBSERVACAO`].
                _ if CLASSES_POR_OBSERVACAO.contains(&clsid) => {
                    let object = self.new_object(Interface::Probe)?;
                    if object == 0 {
                        return Ok(ENOMEMORY);
                    }
                    self.assumptions.insert(
                    "uma classe do firmware é atendida por um objeto que responde sucesso a tudo",
                );
                    self.probe_objects.insert(object, clsid);
                    if out != 0 {
                        self.cpu.write_u32(out, object)?;
                    }
                    return Ok(SUCCESS);
                }
                // A sonda entra antes da recusa: o jogo recebe um objeto que não faz nada e segue,
                // e o que ele chamar nele vai para o relatório. É como se descobre que interface a
                // classe é, sem header e sem adivinhação.
                _ if self.probe_classes.contains(&clsid) => {
                    let object = self.new_object(Interface::Probe)?;
                    if object == 0 {
                        return Ok(ENOMEMORY);
                    }
                    self.probe_objects.insert(object, clsid);
                    if out != 0 {
                        self.cpu.write_u32(out, object)?;
                    }
                    return Ok(SUCCESS);
                }
                _ => {
                    self.unknown_classes.insert(clsid);
                    if out != 0 {
                        self.cpu.write_u32(out, 0)?;
                    }
                    return Ok(ECLASSNOTSUPPORT);
                }
            },
        };

        let Some(obj) = self.objects.create(iface) else {
            return Ok(ENOMEMORY);
        };
        self.cpu.write_u32(obj, loader::vtable_addr(iface))?;
        self.transparency.remove(&obj);
        if iface == Interface::Rsa {
            self.rsas.insert(obj, rsa::RsaState::default());
        }
        if iface == Interface::Hash {
            self.hashes.insert(
                obj,
                HashState {
                    sha1: (clsid == 0x0100_1034).then(crate::brew::sha1::Sha1::default),
                    ..HashState::default()
                },
            );
        }
        // Uma coleção nasce vazia e com o cursor no começo. Sem esse registro ela não existiria
        // para os métodos, e um `AtEnd` numa coleção desconhecida responderia "acabou" por
        // acaso — a resposta certa pelo motivo errado.
        if iface == Interface::Collection {
            self.collections.insert(obj, (Vec::new(), 0));
        }
        // Pelo mesmo motivo da coleção: um widget sem registro responderia "não tenho esse
        // filho" por não existir, e não por não ter o filho.
        // Mesma razão da coleção: sem o registro, um `Tamanho` numa lista desconhecida
        // responderia zero por ela não existir, e não por estar vazia.
        if iface == Interface::Vetor {
            self.vetores.insert(obj, (Vec::new(), 0));
        }
        if iface == Interface::Widget {
            // Um widget nasce visível: o jogo só chama o slot 6 para **esconder**.
            self.proximo_serial += 1;
            self.widgets.insert(
                obj,
                Widget {
                    visivel: true,
                    classe: clsid,
                    serial: self.proximo_serial,
                    ..Widget::default()
                },
            );
        }
        if out != 0 {
            self.cpu.write_u32(out, obj)?;
        }
        Ok(SUCCESS)
    }

    /// Cria a instância do applet chamando `IModule::CreateInstance` no módulo carregado.
    ///
    /// Assinatura, de `AEEModGen.c`:
    /// `int AEEMod_CreateInstance(IModule *po, IShell *pIShell, AEECLSID ClsId, void **ppObj)`.
    /// `CreateInstance` é o slot 2 da vtable de `IModule` (depois de `AddRef` e `Release`).
    /// Se algum módulo de extensão do pacote declara fornecer esta classe.
    fn tem_extensao_para(&self, clsid: u32) -> bool {
        self.module
            .extensions
            .iter()
            .any(|ext| ext.classes.contains(&clsid))
    }

    /// Cria um objeto pedindo-o ao módulo de extensão que o fornece.
    ///
    /// São dois passos, e os mesmos que o console dá: `AEEMod_Load` na primeira vez, para a
    /// extensão entregar o `IModule*` dela, e depois `IModule::CreateInstance` com o ClassID
    /// pedido. O objeto que volta é implementado em ARM pela extensão — daí em diante o jogo
    /// conversa direto com ele, e nós não precisamos saber que interface é.
    ///
    /// Devolve zero quando não deu, e o chamador transforma isso em `ECLASSNOTSUPPORT`.
    fn cria_pela_extensao(&mut self, clsid: u32) -> Result<u32, CpuError> {
        let budget = self.orcamento.max(1);
        let Some(i) = self
            .module
            .extensions
            .iter()
            .position(|ext| ext.classes.contains(&clsid))
        else {
            return Ok(0);
        };
        let (entry, out_module) = (
            self.module.extensions[i].entry,
            self.module.extensions[i].out_module,
        );

        // Primeira vez: carrega o módulo. `Some(0)` marca a tentativa que falhou, para um jogo
        // que peça a classe a cada quadro não pagar a carga toda vez nem enchê-la de log.
        if self.ext_modules[i].is_none() {
            self.cpu.write_u32(out_module, 0)?;
            let desfecho = self.call_guest_aninhado(
                entry,
                [self.module.shell, self.module.helpers, out_module, 0],
                budget,
            )?;
            let ptr = if matches!(desfecho, Outcome::Returned { code: 0 }) {
                self.cpu.read_u32(out_module).unwrap_or(0)
            } else {
                0
            };
            if ptr == 0 {
                self.assumptions
                    .insert("um módulo de extensão do pacote não carregou");
            }
            self.ext_modules[i] = Some(ptr);
        }
        let modulo = self.ext_modules[i].unwrap_or(0);
        if modulo == 0 {
            return Ok(0);
        }

        // `IModule::CreateInstance(po, shell, clsid, &saida)` — o slot 2, o mesmo do applet.
        let vtable = self.cpu.read_u32(modulo)?;
        let create_instance = self.cpu.read_u32(vtable + 2 * 4)?;
        let saida = out_module + 4;
        self.cpu.write_u32(saida, 0)?;
        let desfecho = self.call_guest_aninhado(
            create_instance,
            [modulo, self.module.shell, clsid, saida],
            budget,
        )?;
        if !matches!(desfecho, Outcome::Returned { code: 0 }) {
            return Ok(0);
        }
        self.cpu.read_u32(saida)
    }

    pub fn create_applet(&mut self, clsid: u32, budget: u64) -> Result<AppletResult, CpuError> {
        self.applet_class = clsid;
        let module_ptr = self.cpu.read_u32(self.module.out_module)?;
        if module_ptr == 0 {
            return Ok(AppletResult::NoModule);
        }
        let vtable = self.cpu.read_u32(module_ptr)?;
        let create_instance = self.cpu.read_u32(vtable + 2 * 4)?;

        // O ponteiro de saída fica logo depois do `IModule*`, na área de objetos.
        let out_applet = self.module.out_module + 4;
        self.cpu.write_u32(out_applet, 0)?;

        let outcome = self.call_guest(
            create_instance,
            [module_ptr, self.module.shell, clsid, out_applet],
            budget,
        )?;
        match outcome {
            Outcome::Returned { code } => Ok(AppletResult::Called {
                code,
                applet: self.cpu.read_u32(out_applet)?,
            }),
            other => Ok(AppletResult::Stopped(other)),
        }
    }
}

#[cfg(test)]
mod testes_do_item_id {
    use super::*;

    /// **O item ID vale para qualquer applet instalado**, e não só para o carregado.
    ///
    /// O número é o da pasta do módulo, que é o nome do `.mif`. Devolver zero para um applet que
    /// existe é resposta errada: a documentação manda zero só para classe que **não** é de módulo
    /// baixado.
    #[test]
    fn o_item_id_sai_para_applet_instalado() {
        // O menor módulo que o carregador aceita: o alvo aqui é a tabela de applets, não o código.
        let code = [
            0xe3a0_0010u32.to_le_bytes(), // mov r0, #16
            0xe12f_ff1eu32.to_le_bytes(), // bx lr
        ]
        .concat();
        let image = crate::loader::modfile::ModImage::parse(code).unwrap();
        let module = crate::loader::load(&image).unwrap();
        let mut machine = Machine::new(crate::cpu::BackendPadrao::new().unwrap(), module, ".");
        machine.cpu.reset(&machine.module.mem).unwrap();
        machine.set_installed_applets([
            (0x0102_8e35u32, "274755".to_string()),
            (0x0102_8e36, "279888".to_string()),
        ]);
        assert_eq!(machine.item_id_de(0x0102_8e35), 274755);
        assert_eq!(machine.item_id_de(0x0102_8e36), 279888);
        // Classe que não é de módulo instalado continua devolvendo zero, como a API manda.
        assert_eq!(machine.item_id_de(0x0100_0001), 0);
    }
}

#[cfg(test)]
mod testes_do_filemgr_313 {
    use super::*;
    use crate::cpu::BackendPadrao;

    // O SDK preserva OpenFile no slot 2 de IFileMgr; recusar a classe 3.1.3 deixa
    // o construtor de DiRT chamar um ponteiro nulo antes de EVT_APP_START.
    #[test]
    fn filemgr_313_abre_e_le_recurso_pela_vtable() {
        let root = std::env::temp_dir().join(format!(
            "zeebx-filemgr-313-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("device.conf"), b"NUM_KEY_CODES=0\n").unwrap();
        let image = crate::loader::modfile::ModImage::parse(
            [0xe3a0_0000u32.to_le_bytes(), 0xe12f_ff1eu32.to_le_bytes()].concat(),
        )
        .unwrap();
        let module = loader::load(&image).unwrap();
        let storage = crate::storage::StoragePaths::from_root(&root.join("storage"));
        let mut machine =
            Machine::new_with_storage(BackendPadrao::new().unwrap(), module, &root, &storage, None);
        machine.cpu.reset(&machine.module.mem).unwrap();
        let out = loader::HEAP_BASE + 0x1000;
        let path = out + 4;
        let buffer = out + 0x100;
        machine.cpu.write_mem(path, b"device.conf\0").unwrap();
        let shell = machine.module.shell;
        assert_eq!(
            machine
                .call_guest(
                    aee::encode(Interface::Shell, 2),
                    [shell, 0x0102_d575, out, 0],
                    1000
                )
                .unwrap(),
            Outcome::Returned { code: 0 }
        );
        let manager = machine.cpu.read_u32(out).unwrap();
        assert_ne!(manager, 0);
        let table = machine.cpu.read_u32(manager).unwrap();
        let open = machine.cpu.read_u32(table + 2 * 4).unwrap();
        let Outcome::Returned { code: file } = machine
            .call_guest(open, [manager, path, 1, 0], 1000)
            .unwrap()
        else {
            panic!("OpenFile deve retornar");
        };
        assert_ne!(file, 0, "o objeto da classe deve abrir o recurso real");
        let table = machine.cpu.read_u32(file).unwrap();
        let read = machine.cpu.read_u32(table + 3 * 4).unwrap();
        assert_eq!(
            machine
                .call_guest(read, [file, buffer, 16, 0], 1000)
                .unwrap(),
            Outcome::Returned { code: 16 }
        );
        let mut bytes = [0; 16];
        machine.cpu.read_mem(buffer, &mut bytes).unwrap();
        assert_eq!(&bytes, b"NUM_KEY_CODES=0\n");
        drop(machine);
        std::fs::remove_dir_all(root).unwrap();
    }
}
