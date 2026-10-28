//! IUnzipAStream e os fluxos: o inflate e a leitura em pedaços.

use super::*;

/// Um envelope antigo conserva o cabeçalho zlib, mas omite o Adler32. A exceção
/// só vale quando o DEFLATE termina exatamente no fim da entrada: aceitar saída
/// parcial também esconderia truncamentos ou um checksum presente e corrompido.
fn inflate_unzip_stream(compressed: &[u8]) -> Option<(Vec<u8>, bool)> {
    if compressed.starts_with(&[0x1f, 0x8b]) {
        use std::io::Read;
        let mut output = Vec::new();
        let result = flate2::read::GzDecoder::new(compressed)
            .take(u64::from(TETO_DO_INFLATE) + 1)
            .read_to_end(&mut output);
        if output.len() > TETO_DO_INFLATE as usize { return None; }
        let incomplete = match result {
            Ok(_) => false,
            // Um carregador pode fornecer somente o prefixo comprimido para
            // inspecionar seu cabeçalho. Os bytes produzidos continuam legíveis;
            // CRC/ISIZE inválidos de uma entrada completa nunca entram aqui.
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof && !output.is_empty() => true,
            Err(_) => return None,
        };
        return Some((output, incomplete));
    }
    let zlib_header = compressed.len() >= 2
        && compressed[0] & 0x0f == 8
        && compressed[0] >> 4 <= 7
        && u16::from_be_bytes([compressed[0], compressed[1]]) % 31 == 0;
    if zlib_header {
        // Não há dicionário fornecido ao IUnzipAStream; pular FDICT inventaria dados.
        if compressed[1] & 0x20 != 0 { return None; }
        if let Some((output, _)) = inflate_complete(compressed, true) {
            return Some((output, false));
        }
        let body = &compressed[2..];
        let (output, consumed) = inflate_complete(body, false)?;
        // Sobrar qualquer byte inclui Adler parcial, Adler inválido e lixo após o
        // DEFLATE. Nenhum deles é o formato observado sem checksum.
        return (consumed == body.len()).then_some((output, false));
    }
    inflate_complete(compressed, false).map(|(output, _)| (output, false))
}

/// `Read::read_to_end` sozinho não demonstra que o bloco terminou. StreamEnd
/// confirma também o Adler quando `zlib` é verdadeiro; o cursor permite distinguir
/// checksum ausente de checksum presente sem materializar saída acima do teto.
fn inflate_complete(compressed: &[u8], zlib: bool) -> Option<(Vec<u8>, usize)> {
    let mut decoder = flate2::Decompress::new(zlib);
    let mut output = Vec::new();
    let mut block = [0; 8192];
    loop {
        let before_in = decoder.total_in();
        let before_out = decoder.total_out();
        let input = compressed.get(before_in as usize..)?;
        let status = decoder.decompress(input, &mut block, flate2::FlushDecompress::None).ok()?;
        let produced = (decoder.total_out() - before_out) as usize;
        if output.len().checked_add(produced)? > TETO_DO_INFLATE as usize { return None; }
        output.extend_from_slice(&block[..produced]);
        if status == flate2::Status::StreamEnd {
            return Some((output, decoder.total_in() as usize));
        }
        if decoder.total_in() == before_in && decoder.total_out() == before_out {
            return None;
        }
    }
}

impl<C: CpuBackend> Machine<C> {
    /// `IUnzipAStream` (`AEECLSID_UNZIPSTREAM` = `0x01001014`), de `sdk/inc/AEEUnzipStream.h`.
    ///
    /// Estende o `IAStream` para ler um stream comprimido como se fosse texto claro: o jogo
    /// monta um stream sobre os bytes comprimidos, entrega em `SetStream`, e lê daqui. É o que
    /// o Double Dragon usa para abrir o `data.ggz` e o `sound.ggz`.
    ///
    /// A saída disponível fica guardada na primeira leitura. Uma origem gzip
    /// parcial também pode fornecer um cabeçalho válido sem o restante do arquivo;
    /// seu esgotamento fica distinto do EOF de um envelope verificado.
    pub(super) fn unzip_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let Some(name) = Interface::UnzipStream.method(slot) else {
            return Ok(None);
        };
        let this = self.cpu.read_reg(Reg::R0);
        let (a1, a2) = (self.cpu.read_reg(Reg::R1), self.cpu.read_reg(Reg::R2));
        let result = match name {
            "AddRef" => self.objects.add_ref(this),
            "Release" => {
                let remaining = self.objects.release(this);
                if remaining == 0 {
                    self.unzips.remove(&this);
                }
                remaining
            }
            // void SetStream(IUnzipAStream *, IAStream *pInIAStream)
            "SetStream" => {
                self.unzips.insert(
                    this,
                    UnzipState {
                        source: a1,
                        ..UnzipState::default()
                    },
                );
                SUCCESS
            }
            // O conteúdo já está inteiro na memória, então há sempre o que ler.
            "Readable" => {
                if a1 != 0 {
                    self.pending_signals.push(Callback {
                        function: a1,
                        context: a2,
                    });
                }
                SUCCESS
            }
            // int32 Read(IAStream *, void *pDest, uint32 nWant)
            "Read" => {
                if a2 == 0 { return Ok(Some(0)); }
                self.expand_unzip(this)?;
                let Some(state) = self.unzips.get(&this) else {
                    return Ok(Some(EFAILED));
                };
                if state.incomplete && state.position == state.output.len() {
                    // IAStream usa retorno assinado; -1 não anuncia EOF validado.
                    return Ok(Some(u32::MAX));
                }
                let want = (a2 as usize).min(state.output.len().saturating_sub(state.position));
                if want > 0 {
                    let at = state.position;
                    let bytes = state.output[at..at + want].to_vec();
                    self.cpu.write_mem(a1, &bytes)?;
                    if let Some(state) = self.unzips.get_mut(&this) {
                        state.position += want;
                    }
                }
                want as u32
            }
            "Cancel" => SUCCESS,
            _ => return Ok(None),
        };
        Ok(Some(result))
    }

    /// Lê **tudo** o que resta de um `IAStream` do guest.
    ///
    /// `None` quando o objeto não é um stream que saibamos ler por dentro — o que é diferente
    /// de um erro de leitura, que vem como `Some(Err(..))`.
    pub(super) fn drain_stream(&mut self, source: u32) -> Option<Result<Vec<u8>, CpuError>> {
        if let Some(adapter) = self.source_streams.get(&source).copied() {
            if let Some(bytes) = self.sources.get_mut(&adapter.source) {
                return Some(Ok(std::mem::take(bytes)));
            }
            if let Some(peek) = self.peeks.get_mut(&adapter.source) {
                let bytes = peek.bytes[peek.posicao..].to_vec();
                peek.posicao = peek.bytes.len();
                return Some(Ok(bytes));
            }
            return None;
        }
        if let Some(stream) = self.streams.get(&source).copied() {
            let available = stream.size.saturating_sub(stream.position);
            return Some(self.read_bytes(stream.buffer + stream.position, available));
        }
        // Um `IFile` aberto também é um `IAStream`, e é assim que o Double Dragon entrega o
        // `data.ggz`: lemos da posição corrente até o fim, como faria uma sequência de `Read`.
        let open = self.open_files.get_mut(&source)?;
        let mut bytes = Vec::new();
        Some(
            match std::io::Read::read_to_end(&mut open.file, &mut bytes) {
                Ok(_) => Ok(bytes),
                Err(_) => Ok(Vec::new()),
            },
        )
    }

    /// Descomprime a entrada de um `IUnzipAStream`, uma vez só.
    pub(super) fn expand_unzip(&mut self, this: u32) -> Result<(), CpuError> {
        let Some(state) = self.unzips.get(&this) else {
            return Ok(());
        };
        if state.expanded {
            return Ok(());
        }
        let source = state.source;
        if let Some(state) = self.unzips.get_mut(&this) {
            state.expanded = true;
        }

        // A entrada é um `IAStream`, e no BREW tanto um bloco de memória quanto um arquivo
        // aberto servem como um. O Double Dragon entrega o `IFile` do `data.ggz` direto.
        let Some(compressed) = self.drain_stream(source) else {
            // Dizer *qual* interface chegou é o que transforma isto de "não funcionou" em uma
            // pista: a próxima origem a suportar é a que aparecer aqui.
            let kind = self
                .objects
                .kind_of(source)
                .map(Interface::name)
                .unwrap_or("desconhecida");
            self.bad_pointers.insert(format!(
                "um IUnzipAStream recebeu como entrada um objeto de {kind}, que ainda não sabemos ler"
            ));
            return Ok(());
        };
        let compressed = compressed?;
        match inflate_unzip_stream(&compressed) {
            Some((output, incomplete)) => {
                if let Some(state) = self.unzips.get_mut(&this) {
                    state.output = output;
                    state.incomplete = incomplete;
                }
                // O stream de entrada foi consumido inteiro.
                if let Some(stream) = self.streams.get_mut(&source) {
                    stream.position = stream.size;
                }
            }
            None => {
                self.assumptions
                    .insert("um IUnzipAStream recebeu dados que não descomprimem");
            }
        }
        Ok(())
    }

    /// `IMemAStream` (`AEECLSID_MEMASTREAM` = `0x0100100c`), de `sdk/inc/AEE.h`.
    ///
    /// Apresenta um bloco de memória como stream: é assim que o BREW entrega dados a um
    /// decodificador de imagem. Aqui o "assíncrono" é trivial — os bytes já estão todos na
    /// memória, então `Readable` pode avisar o interessado na mesma hora.
    pub(super) fn stream_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let Some(name) = Interface::MemAStream.method(slot) else {
            return Ok(None);
        };
        let this = self.cpu.read_reg(Reg::R0);
        let (a1, a2, a3) = (
            self.cpu.read_reg(Reg::R1),
            self.cpu.read_reg(Reg::R2),
            self.cpu.read_reg(Reg::R3),
        );
        let result = match name {
            "AddRef" => self.objects.add_ref(this),
            "Release" => {
                let remaining = self.objects.release(this);
                if remaining == 0 {
                    self.solta_stream(this)?;
                }
                remaining
            }
            // void Set(IMemAStream *, byte *pBuff, uint32 dwSize, uint32 dwOffset,
            //          boolean bSysMem)
            //
            // **O buffer passa a ser do stream**, que o devolve ao ser destruído ou ao receber
            // outro — o `bSysMem` só diz de qual heap ele veio, e aqui os dois são o mesmo. O
            // Action Hero 3D monta bitmaps de 7616 bytes, vários por quadro, entrega ao stream e
            // nunca mais o toca: sem isso eram 1,9 MB por segundo de fase, e em três minutos
            // o `MALLOC` devolvia nulo.
            //
            // O `SetEx` troca o `bSysMem` por um `pfnFree` do jogo, e esse fica de fora: chamar
            // código do guest no meio de um despacho é o caminho que já derrubou jogo antes.
            "Set" | "SetEx" => {
                // Pôr de novo o mesmo buffer não é trocá-lo.
                if self.streams.get(&this).is_some_and(|s| s.buffer == a1) {
                    self.streams.remove(&this);
                }
                self.solta_stream(this)?;
                self.streams.insert(
                    this,
                    MemStream {
                        buffer: a1,
                        size: a2,
                        position: a3.min(a2),
                        dono: name == "Set",
                    },
                );
                SUCCESS
            }
            // void Readable(IAStream *, void (*pfnNotify)(void *), void *pUser)
            //
            // O stream está sempre pronto, então o callback vai direto para a fila — entregá-lo
            // aqui seria reentrar no guest no meio do despacho.
            "Readable" => {
                if let Some(adapter) = self.source_streams.get(&this).copied() {
                    self.cancel_callbacks_of(this)?;
                    self.cpu.write_u32(adapter.callback + 16, a1)?;
                    self.cpu.write_u32(adapter.callback + 20, a2)?;
                    if a1 != 0 { self.schedule_callback(adapter.callback, this)?; }
                    return Ok(Some(SUCCESS));
                }
                if a1 != 0 {
                    self.pending_signals.push(Callback {
                        function: a1,
                        context: a2,
                    });
                }
                SUCCESS
            }
            // int32 Read(IAStream *, void *pDest, uint32 nWant)
            "Read" => {
                if let Some(adapter) = self.source_streams.get(&this).copied() {
                    return self.read_source(adapter.source, a1, a2).map(Some);
                }
                let Some(stream) = self.streams.get(&this).copied() else {
                    return Ok(Some(EFAILED));
                };
                let want = a2.min(stream.size.saturating_sub(stream.position));
                if want > 0 {
                    let bytes = self.read_bytes(stream.buffer + stream.position, want)?;
                    self.cpu.write_mem(a1, &bytes)?;
                    if let Some(stream) = self.streams.get_mut(&this) {
                        stream.position += want;
                    }
                }
                want
            }
            "Cancel" => {
                if let Some(adapter) = self.source_streams.get(&this).copied()
                    && ((a1 == 0 && a2 == 0)
                        || (self.cpu.read_u32(adapter.callback + 16)? == a1
                            && self.cpu.read_u32(adapter.callback + 20)? == a2)) {
                    self.cancel_callbacks_of(this)?;
                }
                SUCCESS
            },
            _ => return Ok(None),
        };
        Ok(Some(result))
    }

    /// Esquece o buffer de um `IMemAStream`, devolvendo-o ao heap quando ele é do stream.
    fn solta_stream(&mut self, this: u32) -> Result<(), CpuError> {
        if let Some(adapter) = self.source_streams.remove(&this) {
            self.cancel_callbacks_of(this)?;
            self.heap.free(adapter.callback);
            self.release_source(adapter.source)?;
        }
        if let Some(stream) = self.streams.remove(&this)
            && stream.dono
        {
            self.heap.free(stream.buffer);
        }
        Ok(())
    }
}
