//! IBitmap e o DIB: as superfícies, o blit e a sincronia com a memória do jogo.

use super::*;

// O código OEM de Pandora soma 12 ao retorno antes de editar os pixels. O prefixo
// é opaco para o jogo; IDisplay interpreta o mesmo ponteiro com as dimensões guardadas.
pub(super) const PREFIXO_NATIVO_565: u32 = 12;

/// Antes do BREW 1.2, BitBlt aceitava um formato definido pelo OEM. A identidade
/// evita interpretar um IBitmap estrangeiro ou um ponteiro qualquer como pixels.
/// O layout foi conferido no apresentador e no raster de SplinterCell fornecido.
pub(super) fn raster_legado_do_modulo(module: &loader::LoadedModule) -> Option<(u16, u16)> {
    use sha2::{Digest, Sha256};
    let region = module.mem.regions().iter().find(|r| r.name == "module")?;
    let prefix = loader::MODULE_PREFIX as usize;
    let size = region.bytes.len().checked_sub(prefix + loader::MODULE_BSS_SLACK)?;
    if size != 54_440 { return None; }
    let digest: [u8; 32] = Sha256::digest(region.bytes.get(prefix..prefix + size)?).into();
    let splinter = [
        0xd9,0xaa,0x56,0x41,0x2d,0xe4,0x7d,0xae,0xe7,0x5a,0x77,0x49,0x21,0x05,0xc3,0x9b,
        0x1c,0x80,0x9b,0x85,0x5c,0x50,0x31,0x21,0x3c,0x40,0x11,0x26,0xec,0x48,0x6a,0xb6,
    ];
    (digest == splinter).then_some((120, 130))
}

pub(super) fn bitmap_nativo_565_do_modulo(module: &loader::LoadedModule) -> bool {
    use sha2::{Digest, Sha256};
    let Some(region) = module.mem.regions().iter().find(|r| r.name == "module") else { return false; };
    let prefix = loader::MODULE_PREFIX as usize;
    if region.bytes.len() != prefix + 101_192 + loader::MODULE_BSS_SLACK { return false; }
    let digest: [u8; 32] = Sha256::digest(&region.bytes[prefix..prefix + 101_192]).into();
    // Pandora edita o retorno de CONVERTBMP como pixels compactos, não como IBitmap.
    digest == [
        0xa3,0xbc,0x53,0x69,0xf8,0x84,0xc9,0x1f,0xf3,0x6f,0x48,0x2a,0x68,0xcf,0x69,0x94,
        0x6f,0x93,0xe2,0x69,0xe3,0x7e,0x30,0x8e,0x0b,0x4c,0x9e,0x76,0x4b,0x23,0x94,0x42,
    ]
}

impl<C: CpuBackend> Machine<C> {
    pub(super) fn solta_bitmap_nativo(&mut self, ptr: u32) {
        if self.converted_bitmap_blocks.contains(&ptr) {
            self.objects.forget_external(ptr);
            self.solta_dib(ptr);
            return;
        }
        if self.bitmap_nativo_565
            && (loader::HEAP_BASE..loader::HEAP_BASE + loader::HEAP_SIZE as u32).contains(&ptr)
        {
            self.bitmaps.remove(&ptr);
        }
    }

    pub(super) fn realoca_bitmap_nativo(&mut self, antigo: u32, novo: u32, tamanho: u32) {
        if self.bitmap_nativo_565
            && (loader::HEAP_BASE..loader::HEAP_BASE + loader::HEAP_SIZE as u32).contains(&antigo)
        {
            if let Some(surface) = self.bitmaps.remove(&antigo) {
                // Só o raster completo continua válido depois da mudança. Um bloco encolhido
                // deixa de ser uma imagem, mas não pode deixar metadados no endereço liberado.
                if u64::from(PREFIXO_NATIVO_565) + u64::from(surface.width()) * u64::from(surface.height()) * 2 <= u64::from(tamanho) {
                    self.bitmaps.insert(novo, surface);
                }
            }
        }
    }
    /// Desenha uma imagem numa superfície do jogo chamando o `BltIn` dela.
    ///
    /// É a mesma coreografia do BREW: montamos um `IBitmap` nosso com a imagem decodificada e
    /// passamos como origem. O `BltIn` do jogo então pede `QueryInterface(AEECLSID_DIB)` no
    /// nosso bitmap e lê os pixels pelos campos públicos, que é justamente o que o `IDIB`
    /// existe para oferecer. Conferido na desmontagem: o `BltIn` do Bejeweled Twist faz esse
    /// `QueryInterface` com `0x01001045` e depois lê `cx`, `cy` e `nColorScheme`.
    pub(super) fn blit_into_foreign(
        &mut self,
        blit: PendingBlit,
        budget: u64,
    ) -> Result<(), CpuError> {
        let Some(info) = self.images.get(&blit.image).cloned() else {
            return Ok(());
        };
        let (frame_width, offset) = match (blit.frame, info.frame_width) {
            (Some(n), width) if width > 0 => (width as u32, n * width as u32),
            _ => (info.width, 0),
        };
        let src_x = blit.src_x.max(0) as u32;
        let src_y = blit.src_y.max(0) as u32;
        let largura = blit.width.min(frame_width.saturating_sub(src_x));
        let altura = blit.height.min(info.height.saturating_sub(src_y));
        if largura == 0 || altura == 0 {
            return Ok(());
        }

        // A origem é uma superfície nossa, criada só para esta chamada.
        let source = self.new_object(Interface::Bitmap)?;
        if source == 0 {
            return Ok(());
        }
        let mut surface = Framebuffer::new(largura, altura);
        for row in 0..altura {
            for column in 0..largura {
                let index =
                    ((row + src_y) * info.width + column + src_x + offset) as usize;
                let opaque = info.opaque.get(index).copied().unwrap_or(true);
                let pixel = match (opaque, info.pixels.get(index)) {
                    (true, Some(&pixel)) => pixel,
                    // Sem canal alfa no destino, o transparente vira uma cor reservada — é
                    // como o BREW resolve, e o `BltIn` respeita a `ncTransparent` do `IDIB`.
                    _ => TRANSPARENT_KEY,
                };
                surface.set_pixel_native(column as i32, row as i32, pixel);
            }
        }
        self.bitmaps.insert(source, surface);
        self.transparency.insert(source, TRANSPARENT_KEY);
        self.publica_origem_888(source, &info, largura, altura, src_x + offset, src_y)?;

        let vtable = self.cpu.read_u32(blit.target)?;
        let blt_in = self.cpu.read_u32(vtable + BITMAP_BLT_IN_SLOT * 4)?;
        // BltIn(po, xDst, yDst, dx, dy, pSrc, xSrc, ySrc, rop)
        let outcome = self.call_guest_with_stack(
            blt_in,
            [blit.target, blit.x as u32, blit.y as u32, largura],
            &[altura, source, 0, 0, blit.rop],
            budget,
        )?;
        if !matches!(outcome, Outcome::Returned { code: 0 }) {
            self.assumptions
                .insert("o BltIn de uma superfície do jogo recusou o desenho");
        }

        self.objects.release(source);
        self.bitmaps.remove(&source);
        // **O buffer volta para a região pelo caminho normal.** Tirando só o `dib_buffers` aqui,
        // a capacidade anotada ficava para trás: o próximo bitmap a nascer neste endereço via a
        // anotação, concluía que o buffer dele já cabia e publicava um `IDIB` com `pBmp` nulo. O
        // Bejeweled Twist morria nisso — o `BltIn` dele monta a tabela de linhas a partir do
        // `pBmp`, e sem ela o objeto de origem ficava sem pixels e o jogo caía no primeiro
        // desenho.
        self.solta_dib(source);
        self.cpu.unwatch_dirty(source);
        self.transparency.remove(&source);
        Ok(())
    }

    /// Compoe uma superfície temporária nossa numa superfície do jogo pelo `BltIn` dela.
    pub(super) fn blit_surface_into_foreign(
        &mut self,
        blit: PendingSurfaceBlit,
        budget: u64,
    ) -> Result<(), CpuError> {
        self.publica_framebuffer_888(blit.source)?;

        let vtable = self.cpu.read_u32(blit.target)?;
        let blt_in = self.cpu.read_u32(vtable + BITMAP_BLT_IN_SLOT * 4)?;
        let outcome = self.call_guest_with_stack(
            blt_in,
            [blit.target, blit.x as u32, blit.y as u32, blit.width],
            &[blit.height, blit.source, 0, 0, blit.rop],
            budget,
        )?;
        if !matches!(outcome, Outcome::Returned { code: 0 }) {
            self.assumptions
                .insert("o BltIn de uma superfície do jogo recusou uma primitiva 2D");
        }

        self.objects.release(blit.source);
        self.bitmaps.remove(&blit.source);
        self.solta_dib(blit.source);
        self.cpu.unwatch_dirty(blit.source);
        self.transparency.remove(&blit.source);
        Ok(())
    }

    /// Publica o `IDIB` da origem de um `BltIn` estrangeiro em 888: 24 bits por pixel, ou 32
    /// com o alfa no quarto byte quando a imagem tem transparência.
    ///
    /// **Não pode ser o nosso 565.** O `BltIn` do Bejeweled Twist (0x139d8) só aceita
    /// `nColorScheme` 0, que ele lê como paleta de 8 bits, ou 24; qualquer outro valor desiste
    /// sem criar a superfície de origem, e o primeiro blit dela saltava para o endereço zero
    /// (0x32c1c) ou lia 0x24. É o formato em que o decodificador do BREW entrega a imagem.
    ///
    /// A ordem é B, G, R, A — o `0x00RRGGBB` do BREW em little-endian. Conferido na conversão
    /// dele para 565 (0x23798): o byte 2 vai para o vermelho e o byte 0 para o azul. Expandir o
    /// nosso 565 não perde nada: o jogo converte de volta para o formato da tela.
    fn publica_origem_888(
        &mut self,
        source: u32,
        info: &DecodedImage,
        largura: u32,
        altura: u32,
        offset_x: u32,
        offset_y: u32,
    ) -> Result<(), CpuError> {
        let total = (info.width * info.height) as usize;
        let com_alfa = !info.alfa.is_empty() || info.opaque.iter().take(total).any(|&o| !o);
        let canais: usize = if com_alfa { 4 } else { 3 };
        let mut bytes = Vec::with_capacity(largura as usize * altura as usize * canais);
        for row in 0..altura {
            for column in 0..largura {
                let index = ((row + offset_y) * info.width + column + offset_x) as usize;
                let cor = Rgb::from_rgb565(info.pixels.get(index).copied().unwrap_or(0));
                bytes.extend_from_slice(&[cor.b, cor.g, cor.r]);
                if com_alfa {
                    let opaco = info.opaque.get(index).copied().unwrap_or(true);
                    let alfa = match info.alfa.get(index) {
                        Some(&alfa) => alfa,
                        None if opaco => 0xff,
                        None => 0,
                    };
                    bytes.push(alfa);
                }
            }
        }
        let Some((buffer, capacidade)) = self.reserva_superficie(bytes.len() as u32) else {
            return Ok(());
        };
        self.cpu.write_mem(buffer, &bytes)?;
        // Registrado como o `IDIB` de um decodificador: o `expose_dib` do `QueryInterface` não
        // o reescreve em 565, e o `solta_dib` da limpeza devolve o buffer.
        self.dib_do_decodificador.insert(source, (buffer, capacidade));
        let passo = largura as usize * canais;
        self.cpu.write_u32(source + 4, 0)?; // pPaletteMap
        self.cpu.write_u32(source + 8, buffer)?; // pBmp
        self.cpu.write_u32(source + 12, 0)?; // pRGB
        self.cpu
            .write_u32(source + 16, to_rgbval(Rgb::from_rgb565(TRANSPARENT_KEY)))?; // ncTransparent
        self.cpu.write_mem(source + 20, &(largura as u16).to_le_bytes())?;
        self.cpu.write_mem(source + 22, &(altura as u16).to_le_bytes())?;
        self.cpu.write_mem(source + 24, &(passo as i16).to_le_bytes())?;
        self.cpu.write_mem(source + 26, &0u16.to_le_bytes())?; // cntRGB
        self.cpu
            .write_mem(source + 28, &[(canais * 8) as u8, IDIB_COLORSCHEME_888])?;
        self.cpu.write_mem(source + 30, &[0u8; 6])?;
        Ok(())
    }

    /// Publica um framebuffer temporário como `IDIB` 888 para o `BltIn` do jogo.
    fn publica_framebuffer_888(&mut self, source: u32) -> Result<(), CpuError> {
        let Some(fb) = self.bitmaps.get(&source) else {
            return Ok(());
        };
        let (largura, altura) = (fb.width(), fb.height());
        let mut bytes = Vec::with_capacity(largura as usize * altura as usize * 3);
        for row in 0..altura {
            for column in 0..largura {
                let cor = Rgb::from_rgb565(fb.get_pixel(column as i32, row as i32));
                bytes.extend_from_slice(&[cor.b, cor.g, cor.r]);
            }
        }
        let Some((buffer, capacidade)) = self.reserva_superficie(bytes.len() as u32) else {
            return Ok(());
        };
        self.cpu.write_mem(buffer, &bytes)?;
        self.dib_do_decodificador.insert(source, (buffer, capacidade));
        let passo = largura as usize * 3;
        self.cpu.write_u32(source + 4, 0)?; // pPaletteMap
        self.cpu.write_u32(source + 8, buffer)?; // pBmp
        self.cpu.write_u32(source + 12, 0)?; // pRGB
        self.cpu
            .write_u32(source + 16, to_rgbval(Rgb::from_rgb565(TRANSPARENT_KEY)))?; // ncTransparent
        self.cpu.write_mem(source + 20, &(largura as u16).to_le_bytes())?;
        self.cpu.write_mem(source + 22, &(altura as u16).to_le_bytes())?;
        self.cpu.write_mem(source + 24, &(passo as i16).to_le_bytes())?;
        self.cpu.write_mem(source + 26, &0u16.to_le_bytes())?; // cntRGB
        self.cpu
            .write_mem(source + 28, &[24, IDIB_COLORSCHEME_888])?;
        self.cpu.write_mem(source + 30, &[0u8; 6])?;
        Ok(())
    }

    /// Descobre onde ficam os pixels de um `IBitmap` implementado pelo próprio jogo.
    ///
    /// É o mesmo caminho que o BREW usa: `QueryInterface(AEECLSID_DIB)` no objeto, e o `IDIB`
    /// que volta traz `pBmp`, `cx`, `cy` e `nPitch` como campos públicos. Com isso a superfície
    /// do jogo entra no mesmo mecanismo de sincronização das nossas — desenhamos no host e o
    /// resultado é copiado para a memória dele.
    pub(super) fn probe_foreign_surface(
        &mut self,
        target: u32,
        budget: u64,
    ) -> Result<(), CpuError> {
        let Ok(vtable) = self.cpu.read_u32(target) else {
            return Ok(());
        };
        let Ok(query) = self.cpu.read_u32(vtable + BITMAP_QUERY_INTERFACE_SLOT * 4) else {
            return Ok(());
        };
        // O ponteiro de saída precisa viver na memória do guest.
        let Some(out) = self.heap.alloc(4) else {
            return Ok(());
        };
        // O `IDIB` tem dois IIDs: o atual e o que o BREW 2.0 usava. Um bitmap escrito para a
        // plataforma antiga só reconhece o segundo.
        let mut dib = 0;
        let mut outcome = Outcome::Returned { code: EFAILED };
        for iid in [AEECLSID_DIB, AEEIID_DIB_20] {
            self.cpu.write_u32(out, 0)?;
            outcome = self.call_guest(query, [target, iid, out, 0], budget)?;
            dib = self.cpu.read_u32(out).unwrap_or(0);
            if matches!(outcome, Outcome::Returned { code: 0 }) && dib != 0 {
                break;
            }
        }
        self.heap.free(out);

        let failed = !matches!(outcome, Outcome::Returned { code: 0 });
        if failed || dib == 0 {
            // O Bejeweled Twist é assim: o `QueryInterface` da superfície dele é literalmente
            // `mov r0, #0x14; bx lr` — devolve `ECLASSNOTSUPPORT` sempre. Nessa superfície o
            // único método de desenho implementado de verdade é o `BltIn`.
            self.assumptions
                .insert("uma superfície do jogo não expõe IDIB; o desenho nela passa pelo BltIn quando possível");
            return Ok(());
        }

        let buffer = self.cpu.read_u32(dib + 8)?;
        let mut fields = [0u8; 10];
        self.cpu.read_mem(dib + 20, &mut fields)?;
        let cx = u16::from_le_bytes([fields[0], fields[1]]) as u32;
        let cy = u16::from_le_bytes([fields[2], fields[3]]) as u32;
        let pitch = i16::from_le_bytes([fields[4], fields[5]]) as i32;
        let depth = fields[8];

        // Só sabemos tratar o formato da tela do console: RGB565, linhas contíguas e para
        // baixo. Qualquer outra coisa é melhor recusar do que desenhar torto.
        if cx == 0
            || cy == 0
            || buffer == 0
            || depth != COLOR_DEPTH as u8
            || pitch != ((cx * 2).div_ceil(4) * 4) as i32
        {
            self.assumptions
                .insert("uma superfície do jogo usa um formato que ainda não sabemos desenhar");
            return Ok(());
        }

        self.bitmaps.insert(target, Framebuffer::new(cx, cy));
        self.dib_buffers.insert(target, buffer);
        // Aqui o buffer é do próprio jogo, e é dele que os pixels vêm.
        self.dib_herdados.remove(&target);
        self.cpu.watch_dirty(target, buffer, pitch as u32 * cy)?;
        self.sync_from_guest(target)?;
        Ok(())
    }

    /// O bitmap da tela, criado na primeira vez que alguém pede.
    pub(super) fn device_bitmap(&mut self) -> Result<u32, CpuError> {
        if self.device_bitmap != 0 {
            return Ok(self.device_bitmap);
        }
        let addr = self.new_object(Interface::Bitmap)?;
        if addr != 0 {
            let screen = std::mem::replace(&mut self.screen, Framebuffer::new(1, 1));
            self.bitmaps.insert(addr, screen);
            self.device_bitmap = addr;
            self.display_target = addr;
            // **Uma referência nossa, que nunca é solta.** Quem pede o bitmap da tela solta o
            // que recebeu, como manda a convenção; mas o dono dele é o display, não quem
            // pediu. Sem esta contagem o endereço voltava para a lista de livres e o
            // `CreateCompatibleBitmap` seguinte gravava a superfície dele por cima da tela: a
            // Z-Wheel entrou no Stage, criou uma superfície de 214×34, e era ela que aparecia
            // na janela no lugar dos 640×480.
            self.objects.add_ref(addr);
        }
        Ok(addr)
    }

    /// O canvas `0x0101e443`: um bitmap visto como lugar onde um `IDisplay` desenha.
    ///
    /// A Z-Wheel pede este canvas ao bitmap em que desenha uma subárvore de widgets (`0x4166c`),
    /// chama o slot 7 com `&saída` e usa o que sai como `IDisplay` — slots 14, 15 e 19, que são
    /// `SetDestination`, `GetDestination` e `GetClipRect`. Com o display na mão ela lê o destino,
    /// cria um bitmap compatível do tamanho do widget e desenha nele. Recusado, a `0x41d68`
    /// recebia nulo e desreferenciava: era o acesso a `0x00000000` em `0x41dbc` depois de
    /// confirmar em "Jogar".
    pub(super) fn canvas_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let Some(name) = Interface::Canvas.method(slot) else {
            return Ok(None);
        };
        if aee::e_marcador(name) {
            return Ok(None);
        }
        let this = self.cpu.read_reg(Reg::R0);
        let result = match name {
            "AddRef" => self.objects.add_ref(this),
            "Release" => {
                let restam = self.objects.release(this);
                if restam == 0 {
                    self.canvases.remove(&this);
                }
                restam
            }
            "GetDisplay" => {
                let Some(&bitmap) = self.canvases.get(&this) else {
                    return Ok(Some(EBADPARM));
                };
                let out = self.cpu.read_reg(Reg::R1);
                let display = self.new_object(Interface::Display)?;
                if display == 0 {
                    return Ok(Some(ENOMEMORY));
                }
                // O display do canvas desenha no bitmap dele. Os nossos displays compartilham um
                // destino só, então entregar um é apontar esse destino para o bitmap.
                self.display_target = bitmap;
                self.assumptions.insert(
                    "o canvas 0x0101e443 entregou um IDisplay apontado para o bitmap dele — leitura pelo uso",
                );
                if out != 0 {
                    self.cpu.write_u32(out, display)?;
                }
                SUCCESS
            }
            _ => return Ok(None),
        };
        Ok(Some(result))
    }

    /// Métodos de `ITransform`.
    pub(super) fn transform_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let Some(name) = Interface::Transform.method(slot) else {
            return Ok(None);
        };
        let this = self.cpu.read_reg(Reg::R0);
        let result = match name {
            "AddRef" => self.objects.add_ref(this),
            "Release" => {
                let restam = self.objects.release(this);
                if restam == 0 {
                    self.transformacoes.remove(&this);
                }
                restam
            }
            // int TransformBltSimple(ITransform *p, int x, int y, IBitmap *pSrc, int xSrc,
            //                        int ySrc, unsigned dx, unsigned dy, uint16 unTransform,
            //                        uint8 nComposite)
            //
            // As mesmas regras do `TransformBltComplex`, com a matriz montada das flags: rotação
            // em quartos de volta nos bits 0-1, espelho em X no bit 2 e a escala a partir do bit
            // 3. O Action Hero 3D passa `unTransform = 8` com `x = 160`, `y = 120` e um canvas
            // 320x240 — o quadro dobrado centrado na tela 640x480, então `8` é a escala 2x.
            "TransformBltSimple" => {
                let Some(&destino) = self.transformacoes.get(&this) else {
                    return Ok(Some(EBADPARM));
                };
                let (x, y) = (
                    self.cpu.read_reg(Reg::R1) as i32,
                    self.cpu.read_reg(Reg::R2) as i32,
                );
                let origem = self.cpu.read_reg(Reg::R3);
                let x_origem = self.stack_arg(0)? as i32;
                let y_origem = self.stack_arg(1)? as i32;
                let largura = self.stack_arg(2)? as i32;
                let altura = self.stack_arg(3)? as i32;
                let flags = self.stack_arg(4)? & 0xffff;
                let escala = match (flags >> 3) & 0x7 {
                    0 => 1.0,
                    1 => 2.0,
                    _ => {
                        self.assumptions.insert(
                            "ITransform::TransformBltSimple com uma escala além de 1x e 2x — desenhada em 1x",
                        );
                        1.0
                    }
                };
                let espelho_y = if flags & 0x4 != 0 { -1.0 } else { 1.0 };
                // AEETransform reflete SOBRE o eixo X (inverte Y) e gira no sentido
                // anti-horário. O minimapa do NFS usa 6: reflexão + 180° = espelho horizontal.
                let (cos, sin) = match flags & 0x3 {
                    0 => (1.0, 0.0),
                    1 => (0.0, 1.0),
                    2 => (-1.0, 0.0),
                    _ => (0.0, -1.0),
                };
                // R · espelho · escala
                let m = [
                    cos * escala,
                    sin * espelho_y * escala,
                    -sin * escala,
                    cos * espelho_y * escala,
                ];
                self.transforma(TransformBlt {
                    destino,
                    origem,
                    x,
                    y,
                    x_origem,
                    y_origem,
                    largura,
                    altura,
                    m,
                    composto: self.stack_arg(5)? as u8,
                })
            }
            // int TransformBltComplex(ITransform *p, int x, int y, IBitmap *pSrc, int xSrc,
            //                         int ySrc, unsigned dx, unsigned dy,
            //                         const AEETransformMatrix *pMatrix, uint8 nComposite)
            //
            // A matriz é `{ int16 A, B, C, D }` em ponto fixo 8.8, aplicada **em volta do centro**
            // do retângulo de origem, e `(x, y)` é onde o canto dele cairia sem transformação.
            // A leitura sai do uso: o Zenonia passa `x = 160`, `y = 120`, um canvas 320x240 e
            // escala 1,9; o centro do resultado fica em (320, 240), no meio da tela 640x480.
            "TransformBltComplex" => {
                let Some(&destino) = self.transformacoes.get(&this) else {
                    return Ok(Some(EBADPARM));
                };
                let (x, y) = (
                    self.cpu.read_reg(Reg::R1) as i32,
                    self.cpu.read_reg(Reg::R2) as i32,
                );
                let origem = self.cpu.read_reg(Reg::R3);
                let x_origem = self.stack_arg(0)? as i32;
                let y_origem = self.stack_arg(1)? as i32;
                let largura = self.stack_arg(2)? as i32;
                let altura = self.stack_arg(3)? as i32;
                let matriz = self.stack_arg(4)?;
                let mut campos = [0u8; 8];
                self.cpu.read_mem(matriz, &mut campos)?;
                let campo = |i: usize| i16::from_le_bytes([campos[i], campos[i + 1]]) as f32 / 256.0;
                let m = [campo(0), campo(2), campo(4), campo(6)];
                self.transforma(TransformBlt {
                    destino,
                    origem,
                    x,
                    y,
                    x_origem,
                    y_origem,
                    largura,
                    altura,
                    m,
                    composto: self.stack_arg(5)? as u8,
                })
            }
            _ => return Ok(None),
        };
        Ok(Some(result))
    }

    /// Desenha `origem` em `destino` pela matriz, amostrando o pixel mais próximo.
    ///
    /// Percorre o destino, não a origem: com escala maior que um, andar pela origem deixaria
    /// buracos entre os pixels. Cada pixel do destino volta pela inversa da matriz até a origem.
    fn transforma(&mut self, t: TransformBlt) -> u32 {
        let [a, b, c, d] = t.m;
        let det = a * d - b * c;
        if det.abs() < f32::EPSILON || t.largura <= 0 || t.altura <= 0 {
            return EBADPARM;
        }
        let inversa = [d / det, -b / det, -c / det, a / det];
        let Some(fonte) = self.bitmaps.get(&t.origem) else {
            return EBADPARM;
        };
        // COMPOSITE_KEYCOLOR é zero; 0xff e os valores indefinidos são opacos.
        // Ignorar o argumento copiava o magenta do atlas na metade espelhada do minimapa.
        let transparente = (t.composto == 0).then(||
            self.transparency.get(&t.origem).copied().unwrap_or(TRANSPARENT_KEY));
        let (meio_w, meio_h) = (t.largura as f32 / 2.0, t.altura as f32 / 2.0);
        // Copiar o retângulo de origem primeiro deixa ler e escrever quando os dois são o mesmo
        // mapa de superfícies — e quando origem e destino são o mesmo bitmap.
        let mut pixels = Vec::with_capacity((t.largura * t.altura) as usize);
        for linha in 0..t.altura {
            for coluna in 0..t.largura {
                pixels.push(fonte.get_pixel(t.x_origem + coluna, t.y_origem + linha));
            }
        }
        let Some(alvo) = self.bitmaps.get_mut(&t.destino) else {
            return EBADPARM;
        };
        let (centro_x, centro_y) = (t.x as f32 + meio_w, t.y as f32 + meio_h);
        // A caixa do resultado: os quatro cantos da origem transformados.
        let cantos = [(-meio_w, -meio_h), (meio_w, -meio_h), (-meio_w, meio_h), (meio_w, meio_h)];
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (u, v) in cantos {
            let (px, py) = (a * u + b * v + centro_x, c * u + d * v + centro_y);
            x0 = x0.min(px);
            y0 = y0.min(py);
            x1 = x1.max(px);
            y1 = y1.max(py);
        }
        let limite_x = alvo.width() as i32;
        let limite_y = alvo.height() as i32;
        for py in (y0.floor() as i32).max(0)..(y1.ceil() as i32).min(limite_y) {
            for px in (x0.floor() as i32).max(0)..(x1.ceil() as i32).min(limite_x) {
                let (u, v) = (px as f32 + 0.5 - centro_x, py as f32 + 0.5 - centro_y);
                let sx = (inversa[0] * u + inversa[1] * v + meio_w).floor() as i32;
                let sy = (inversa[2] * u + inversa[3] * v + meio_h).floor() as i32;
                if sx < 0 || sy < 0 || sx >= t.largura || sy >= t.altura {
                    continue;
                }
                let pixel = pixels[(sy * t.largura + sx) as usize];
                if Some(pixel) != transparente {
                    alvo.set_pixel_native(px, py, pixel);
                }
            }
        }
        SUCCESS
    }

    /// Métodos de `IBitmap`, despachados pelo nome do slot.
    pub(super) fn bitmap_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        // Blitar para a tela é desenhar por cima dela: o quadro do OpenGL tem de estar nela
        // antes. Ver [`Machine::materializa_quadro_gl`].
        self.materializa_quadro_gl();
        let Some(name) = Interface::Bitmap.method(slot) else {
            return Ok(None);
        };
        let this = self.cpu.read_reg(Reg::R0);
        if self.raw_dib(this) && matches!(name,"RGBToNative"|"NativeToRGB") {
            let meta=self.cpu.read_u32(this+24)?;let count=(meta>>16) as usize;
            let colors=self.cpu.read_u32(this+12)?;
            if count==0 || count>256 || colors==0{return Ok(Some(EUNSUPPORTED));}
            let mut palette=vec![0;count*4];self.cpu.read_mem(colors,&mut palette)?;
            let rgb=self.cpu.read_reg(Reg::R1);
            if name=="NativeToRGB" {
                return Ok(Some(if (rgb as usize)<count{
                    let at=rgb as usize*4;to_rgbval(Rgb{r:palette[at+2],g:palette[at+1],b:palette[at]})
                }else{0}));
            }
            let color=Rgb::from_rgbval(rgb);
            let index=palette.chunks_exact(4).enumerate().min_by_key(|(_,p)|{
                let r=i32::from(color.r)-i32::from(p[2]);let g=i32::from(color.g)-i32::from(p[1]);let b=i32::from(color.b)-i32::from(p[0]);r*r+g*g+b*b
            }).map_or(0,|(n,_)|n as u32);
            return Ok(Some(index));
        }
        // CreateDIBitmap entrega dados públicos, não um canvas de desenho nativo (SDK 1.2).
        if self.raw_dib(this) && matches!(name, "RGBToNative" | "NativeToRGB" | "DrawPixel"
            | "GetPixel" | "SetPixels" | "DrawHScanline" | "FillRect" | "BltOut") {
            return Ok(Some(EUNSUPPORTED));
        }
        let result = match name {
            // `IBitmap` tem `QueryInterface`, então AddRef e Release precisam ser tratados aqui:
            // o braço genérico do despacho só alcança interfaces que não têm tratamento próprio.
            "AddRef" => self.objects.add_ref(this),
            "Release" => {
                // **O bitmap da tela não morre.** O dono dele é o display, não quem pediu; um
                // `Release` a mais devolvia o endereço para a lista de livres com a superfície
                // ainda viva, e o objeto seguinte nascia por cima da tela. A trava vale mesmo
                // com o `GetDestination` já contando: é o endereço da tela que não pode ser
                // reciclado, e qualquer caminho novo que o entregue sem contar reabriria isto.
                if this == self.device_bitmap && self.objects.contagem(this) <= 1 {
                    if self.serial.is_some() {
                        self.registra_serial(format!(
                            "<release a mais no bitmap da tela {this:#x}; segurando>"
                        ));
                    }
                    return Ok(Some(1));
                }
                let restantes = self.objects.release(this);
                if restantes == 0 {
                    self.solta_dib(this);
                }
                restantes
            }
            // int QueryInterface(IBitmap *, AEECLSID, void **) — o jogo usa isto para pedir um
            // `IDIB`, que dá acesso direto aos pixels. Ainda não oferecemos essa interface, e
            // `ECLASSNOTSUPPORT` é a resposta correta para isso: o BREW espera que o app tenha
            // caminho alternativo quando a plataforma não expõe o DIB.
            "QueryInterface" => {
                let requested = self.cpu.read_reg(Reg::R1);
                let out = self.cpu.read_reg(Reg::R2);
                let answer = match requested {
                    AEEIID_IBITMAP => Some(this),
                    // Um `IDIB` *é* um `IBitmap` — a struct começa com a vtable de `IBitmap` e
                    // só acrescenta campos públicos. Então o próprio objeto serve, desde que
                    // os campos estejam preenchidos.
                    AEECLSID_DIB | AEEIID_DIB_20 => {
                        self.expose_dib(this)?;
                        Some(this)
                    }
                    AEEIID_CANVAS => {
                        let canvas = self.new_object(Interface::Canvas)?;
                        if canvas == 0 {
                            return Ok(Some(ENOMEMORY));
                        }
                        self.canvases.insert(canvas, this);
                        if out != 0 {
                            self.cpu.write_u32(out, canvas)?;
                        }
                        return Ok(Some(SUCCESS));
                    }
                    // O `ITransform` é outro objeto, que desenha **neste** bitmap.
                    AEEIID_TRANSFORM => {
                        let transform = self.new_object(Interface::Transform)?;
                        if transform == 0 {
                            return Ok(Some(ENOMEMORY));
                        }
                        self.transformacoes.insert(transform, this);
                        if out != 0 {
                            self.cpu.write_u32(out, transform)?;
                        }
                        return Ok(Some(SUCCESS));
                    }
                    _ => None,
                };
                match answer {
                    Some(pointer) => {
                        if out != 0 {
                            self.cpu.write_u32(out, pointer)?;
                        }
                        self.objects.add_ref(this);
                        SUCCESS
                    }
                    None => {
                        if out != 0 {
                            self.cpu.write_u32(out, 0)?;
                        }
                        self.unknown_classes.insert(requested);
                        ECLASSNOTSUPPORT
                    }
                }
            }
            // NativeColor no nosso caso é o próprio RGB565 do framebuffer.
            "RGBToNative" => Rgb::from_rgbval(self.cpu.read_reg(Reg::R1)).to_rgb565() as u32,
            "NativeToRGB" => {
                let native = self.cpu.read_reg(Reg::R1) as u16;
                to_rgbval(Rgb::from_rgb565(native))
            }
            // int DrawPixel(IBitmap *po, unsigned x, unsigned y, NativeColor c, AEERasterOp rop)
            "DrawPixel" => {
                let (x, y) = (
                    self.cpu.read_reg(Reg::R1) as i32,
                    self.cpu.read_reg(Reg::R2) as i32,
                );
                let color = self.cpu.read_reg(Reg::R3) as u16;
                if let Some(fb) = self.bitmaps.get_mut(&this) {
                    fb.set_pixel_native(x, y, color);
                }
                // O buffer do jogo é a fonte da verdade quando ele existe, então o pixel vai
                // para os dois lugares — e só ele, não a superfície inteira.
                if let Some(at) = self.dib_pixel(this, x, y) {
                    self.cpu.write_mem(at, &color.to_le_bytes())?;
                }
                SUCCESS
            }
            // int SetPixels(IBitmap *po, unsigned cnt, AEEPoint *pPoint, NativeColor color,
            //               AEERasterOp rop)
            //
            // O `DrawPixel` em lote: `cnt` pontos `{ int16 x; int16 y }`, todos na mesma cor. O
            // Zenonia usa isto no meio do jogo, e sem ele parava — nos dois motores, no mesmo
            // ponto, depois de quase 1.400 voltas.
            "SetPixels" => {
                let quantos = self.cpu.read_reg(Reg::R1);
                let pontos = self.cpu.read_reg(Reg::R2);
                let color = self.cpu.read_reg(Reg::R3) as u16;
                if pontos == 0 {
                    return Ok(Some(EBADPARM));
                }
                // Um contador absurdo vindo do guest não pode virar um laço de bilhões.
                for i in 0..quantos.min(1 << 20) {
                    let mut ponto = [0u8; 4];
                    self.cpu.read_mem(pontos + i * 4, &mut ponto)?;
                    let x = i16::from_le_bytes([ponto[0], ponto[1]]) as i32;
                    let y = i16::from_le_bytes([ponto[2], ponto[3]]) as i32;
                    if let Some(fb) = self.bitmaps.get_mut(&this) {
                        fb.set_pixel_native(x, y, color);
                    }
                    if let Some(at) = self.dib_pixel(this, x, y) {
                        self.cpu.write_mem(at, &color.to_le_bytes())?;
                    }
                }
                SUCCESS
            }
            "GetPixel" => {
                let (x, y) = (
                    self.cpu.read_reg(Reg::R1) as i32,
                    self.cpu.read_reg(Reg::R2) as i32,
                );
                // Do buffer do jogo, quando há um: ele pode ter escrito ali direto.
                let value = match self.dib_pixel(this, x, y) {
                    Some(at) => {
                        let mut bytes = [0u8; 2];
                        self.cpu.read_mem(at, &mut bytes)?;
                        u16::from_le_bytes(bytes)
                    }
                    None => self
                        .bitmaps
                        .get(&this)
                        .map(|fb| fb.get_pixel(x, y))
                        .unwrap_or(0),
                };
                let out = self.cpu.read_reg(Reg::R3);
                if out != 0 {
                    self.cpu.write_u32(out, value as u32)?;
                }
                SUCCESS
            }
            // int DrawHScanline(IBitmap *po, unsigned y, unsigned xMin, unsigned xMax, ...)
            "DrawHScanline" => {
                let y = self.cpu.read_reg(Reg::R1) as i32;
                let x_min = self.cpu.read_reg(Reg::R2) as i32;
                let x_max = self.cpu.read_reg(Reg::R3) as i32;
                let color = self.stack_arg(0)? as u16;
                if let Some(fb) = self.bitmaps.get_mut(&this) {
                    for x in x_min..=x_max {
                        fb.set_pixel_native(x, y, color);
                    }
                }
                SUCCESS
            }
            // int FillRect(IBitmap *po, const AEERect *prc, NativeColor color, AEERasterOp rop)
            // int FillRect(IBitmap *po, const AEERect *prc, NativeColor color, AEERasterOp rop)
            //
            // `IBITMAP_FillRect.htm`: só `AEE_RO_COPY` e `AEE_RO_XOR` valem; qualquer outra
            // operação é `EUNSUPPORTED` e **não desenha**. Enquanto ignorávamos o `rop`, o
            // Bejeweled Twist pintava a tela inteira de preto uma vez por quadro com um
            // `AEE_RO_TRANSPARENT` que o console teria recusado.
            "FillRect" => {
                let rop = self.cpu.read_reg(Reg::R3);
                let rect = self.read_rect(self.cpu.read_reg(Reg::R1))?;
                let color = self.cpu.read_reg(Reg::R2) as u16;
                // Com `AEE_RO_TRANSPARENT`, preencher com a própria cor transparente não
                // escreve nada. Ignorar o `rop` custava caro: o Bejeweled Twist chama exatamente
                // assim, com cor zero, e a tela inteira era apagada uma vez por quadro.
                let transparent = self.transparency.get(&this).copied().unwrap_or(0);
                if rop == AEE_RO_TRANSPARENT && color == transparent {
                    return Ok(Some(SUCCESS));
                }
                if let (Some(rect), Some(fb)) = (rect, self.bitmaps.get_mut(&this)) {
                    // `AEE_RO_COPY` é o normal; qualquer outra operação que chegue aqui ainda
                    // pinta, porque recusar o desenho é pior do que pintar demais.
                    if rop == AEE_RO_XOR {
                        fb.xor_rect_native(rect, color);
                    } else {
                        fb.fill_rect_native(rect, color);
                    }
                }
                SUCCESS
            }
            // int BltIn(IBitmap *po, int xDst, int yDst, int dx, int dy,
            //           IBitmap *pSrc, int xSrc, int ySrc, AEERasterOp rop)
            "BltIn" => {
                let dst = (
                    self.cpu.read_reg(Reg::R1) as i32,
                    self.cpu.read_reg(Reg::R2) as i32,
                );
                let size = (self.cpu.read_reg(Reg::R3) as i32, self.stack_arg(0)? as i32);
                let src = self.stack_arg(1)?;
                let origin = (self.stack_arg(2)? as i32, self.stack_arg(3)? as i32);
                let rop = self.stack_arg(4)?;
                self.blit(this, dst, size, src, origin, rop)?
            }
            // BltOut inverte os papéis: a fonte é `po` e o destino vem no argumento.
            "BltOut" => {
                let dst_pos = (
                    self.cpu.read_reg(Reg::R1) as i32,
                    self.cpu.read_reg(Reg::R2) as i32,
                );
                let size = (self.cpu.read_reg(Reg::R3) as i32, self.stack_arg(0)? as i32);
                let dst = self.stack_arg(1)?;
                let origin = (self.stack_arg(2)? as i32, self.stack_arg(3)? as i32);
                let rop = self.stack_arg(4)?;
                self.blit(dst, dst_pos, size, this, origin, rop)?
            }
            // int GetInfo(IBitmap *po, AEEBitmapInfo *pinfo, int nSize)
            "GetInfo" => {
                let out = self.cpu.read_reg(Reg::R1);
                if out != 0 {
                    let (cx, cy, depth) = if self.raw_dib(this) {
                        let size = self.cpu.read_u32(this + 20)?;
                        let mut depth = [0];
                        self.cpu.read_mem(this + 28, &mut depth)?;
                        (size & 0xffff, size >> 16, u32::from(depth[0]))
                    } else {
                        self.bitmaps.get(&this).map(|fb| (fb.width(), fb.height(), COLOR_DEPTH as u32))
                            .unwrap_or((0, 0, 0))
                    };
                    self.cpu.write_u32(out, cx)?;
                    self.cpu.write_u32(out + 4, cy)?;
                    self.cpu.write_u32(out + 8, depth)?;
                }
                SUCCESS
            }
            // int CreateCompatibleBitmap(IBitmap *po, IBitmap **ppIBitmap, uint16 w, uint16 h)
            "CreateCompatibleBitmap" => {
                let out = self.cpu.read_reg(Reg::R1);
                let width = self.cpu.read_reg(Reg::R2) & 0xffff;
                let height = self.cpu.read_reg(Reg::R3) & 0xffff;
                let addr = self.new_object(Interface::Bitmap)?;
                if addr == 0 {
                    return Ok(Some(ENOMEMORY));
                }
                // **Nasce com a cor transparente, e não preto.** O Toy Raid monta cada sprite
                // virado num par de bitmaps compatíveis: `BltIn` transparente do atlas para o
                // primeiro, cópia espelhada pelo `IDIB` para o segundo, e `BitBlt` transparente
                // do segundo na tela. O fundo que sobra do primeiro vai junto, e tem de ser a
                // chave: nascido preto, cada helicóptero virado para a direita, avião e explosão
                // saía dentro de um retângulo preto.
                let mut novo = Framebuffer::new(width, height);
                novo.fill_rect_native(
                    Rect { x: 0, y: 0, width: width as i16, height: height as i16 },
                    TRANSPARENT_KEY,
                );
                self.bitmaps.insert(addr, novo);
                // **O bitmap compatível já nasce `IDIB`, com os campos públicos preenchidos.** O
                // Zenonia cria o canvas 320x240 assim e lê `cx`, `cy` e `pBmp` direto da struct,
                // sem `QueryInterface` nenhum. Com os campos zerados o canvas tinha tamanho zero:
                // o jogo desenhava pixel a pixel por `SetPixels` — 197 mil chamadas em trinta
                // segundos — e apresentava um retângulo vazio, com a tela preta.
                self.expose_dib(addr)?;
                if out != 0 {
                    self.cpu.write_u32(out, addr)?;
                }
                SUCCESS
            }
            "SetTransparencyColor" => {
                if self.raw_dib(this) {self.cpu.write_u32(this+16,self.cpu.read_reg(Reg::R1))?;}
                self.transparency
                    .insert(this, self.cpu.read_reg(Reg::R1) as u16);
                SUCCESS
            }
            "GetTransparencyColor" => {
                let value = self.transparency.get(&this).copied().unwrap_or(TRANSPARENT_KEY);
                let out = self.cpu.read_reg(Reg::R1);
                if out != 0 {
                    self.cpu.write_u32(out, value as u32)?;
                }
                SUCCESS
            }
            _ => return Ok(None),
        };
        Ok(Some(result))
    }

    /// Prepara um bitmap para ser usado como `IDIB`: reserva o buffer de pixels na memória do
    /// guest, copia o conteúdo atual para lá e preenche os campos públicos da struct.
    ///
    /// Layout, de `inc/AEEIDIB.h`: `pvt`, `pPaletteMap`, `pBmp`, `pRGB`, `ncTransparent`,
    /// `cx`, `cy`, `nPitch`, `cntRGB`, `nDepth`, `nColorScheme` e seis bytes reservados.
    pub(super) fn expose_dib(&mut self, bitmap: u32) -> Result<(), CpuError> {
        if self.converted_bitmap_blocks.contains(&bitmap) {
            return self.write_dib_header(bitmap);
        }
        // O `IDIB` do decodificador já está publicado, no formato dele.
        if self.dib_do_decodificador.contains_key(&bitmap) {
            return Ok(());
        }
        let Some(fb) = self.bitmaps.get(&bitmap) else {
            return Ok(());
        };
        let cy = fb.height();
        let precisa = fb.passo_do_dib() as u32 * cy;
        // **O cabeçalho é reescrito toda vez, e não só na primeira.** O endereço de um objeto
        // volta a ser usado quando o anterior é liberado, e o bitmap novo tem outro tamanho:
        // com a checagem por endereço, o `IDIB` de uma imagem nova continuava anunciando o
        // tamanho da imagem anterior.
        //
        // Foi o que quebrou o texto do Tekken 2. Ele decodifica nove imagens em sequência,
        // liberando cada uma antes da seguinte — todas nasceram no mesmo endereço. O `IDIB`
        // dizia 200x112 para todas, então o jogo criava uma página de 200x112 para uma folha de
        // letras de 360x280, copiava só o canto dela e depois pedia cada glifo por coordenada
        // da folha inteira. O que caía fora virava bloco: o menu inteiro saía com as palavras
        // como retângulos laranja.
        //
        // O buffer é reaproveitado quando cabe. Reservar outro a cada exposição também acerta o
        // tamanho, mas a região de superfícies **não recicla**: um jogo que decodifique centenas
        // de imagens a esgotaria.
        let cabe = self
            .dib_capacity
            .get(&bitmap)
            .is_some_and(|&tinha| tinha >= precisa);
        if !cabe {
            // O buffer que não cabe mais volta para a região antes de pedir outro.
            // Só com capacidade registrada o buffer é nosso; sem ela, é do jogo.
            if let (Some(&antigo), Some(_)) =
                (self.dib_buffers.get(&bitmap), self.dib_capacity.get(&bitmap))
            {
                self.solta_superficie(antigo);
            }
            let Some((buffer, capacidade)) = self.reserva_superficie(precisa) else {
                self.dib_buffers.remove(&bitmap);
                self.dib_capacity.remove(&bitmap);
                return Ok(());
            };
            self.dib_buffers.insert(bitmap, buffer);
            self.dib_capacity.insert(bitmap, capacidade);
            self.dib_publicado.remove(&bitmap);
            self.cpu.watch_dirty(bitmap, buffer, precisa)?;
            // A primeira exposição precisa publicar os pixels atuais. Exposições seguintes
            // apenas atualizam o cabeçalho: reescrever a superfície inteira em cada
            // QueryInterface apagava alterações feitas pelo guest e custava dezenas de ms em
            // jogos que consultam o bitmap a cada quadro.
            self.sync_to_guest(bitmap)?;
        } else if self.dib_herdados.contains(&bitmap) {
            // O buffer cabe, mas é do objeto que morreu aqui: para este bitmap, esta **é** a
            // primeira exposição.
            self.sync_to_guest(bitmap)?;
        }
        self.write_dib_header(bitmap)
    }

    /// Escreve os campos públicos do `IDIB` de um bitmap: tamanho, passo, profundidade e o
    /// ponteiro para os pixels — este último só quando eles já existem.
    ///
    /// Um `IBitmap` de software do BREW **é** um `IDIB`: a struct começa com a vtable de
    /// `IBitmap` e segue com campos públicos, e o jogo lê esses campos direto, sem pedir nada.
    /// O Peggle é o caso: ele decodifica o PNG, pergunta o tamanho pelos campos e imprime
    /// `-size 0/0` no log dele quando não acha. Aí monta cada sprite como um quadrado de lado
    /// zero — 76.618 dos 77.208 triângulos de um quadro saíam degenerados, e a tela ficava
    /// preta com o jogo desenhando o tempo todo.
    ///
    /// Fora do decodificador, os pixels continuam sendo alocados só no `QueryInterface`, que é
    /// quando o jogo declara que vai mexer neles: a região de superfícies não recicla, e toda
    /// superfície publicada entra no laço que sincroniza os pixels a cada chamada que os toca.
    /// Escrever o cabeçalho sem os pixels seria pior que não escrever nada — o jogo passa a
    /// confiar no `pBmp` e desreferencia o zero.
    pub(super) fn write_dib_header(&mut self, bitmap: u32) -> Result<(), CpuError> {
        let Some(fb) = self.bitmaps.get(&bitmap) else {
            return Ok(());
        };
        let (cx, cy) = (fb.width(), fb.height());
        let pitch = fb.passo_do_dib() as u32;
        let buffer = self.dib_buffers.get(&bitmap).copied().unwrap_or(0);
        let transparent = self.transparency.get(&bitmap).copied().unwrap_or(TRANSPARENT_KEY) as u32;
        self.cpu.write_u32(bitmap + 4, 0)?; // pPaletteMap
        self.cpu.write_u32(bitmap + 8, buffer)?; // pBmp
        self.cpu.write_u32(bitmap + 12, 0)?; // pRGB: RGB565 não tem paleta
        self.cpu.write_u32(bitmap + 16, transparent)?;
        self.cpu
            .write_mem(bitmap + 20, &(cx as u16).to_le_bytes())?;
        self.cpu
            .write_mem(bitmap + 22, &(cy as u16).to_le_bytes())?;
        self.cpu
            .write_mem(bitmap + 24, &(pitch as i16).to_le_bytes())?;
        self.cpu.write_mem(bitmap + 26, &0u16.to_le_bytes())?; // cntRGB
        // `nDepth` em bits e `nColorScheme` com o código de `AEEIDIB.h`. Sem o esquema correto o
        // jogo não sabe como interpretar os pixels e desiste.
        self.cpu
            .write_mem(bitmap + 28, &[COLOR_DEPTH as u8, IDIB_COLORSCHEME_565])?;
        self.cpu.write_mem(bitmap + 30, &[0u8; 6])?;
        Ok(())
    }

    /// Devolve à região o buffer de um bitmap que morreu.
    ///
    /// Só os buffers que **nós** reservamos voltam — os que têm capacidade anotada. O de uma
    /// superfície do jogo (`IDIB` dele) é memória dele.
    pub(super) fn solta_dib(&mut self, bitmap: u32) {
        if self.converted_bitmap_blocks.remove(&bitmap) {
            self.dib_buffers.remove(&bitmap);
            self.dib_capacity.remove(&bitmap);
            self.dib_herdados.remove(&bitmap);
            self.dib_publicado.remove(&bitmap);
            self.bitmaps.remove(&bitmap);
            self.transparency.remove(&bitmap);
            self.cpu.unwatch_dirty(bitmap);
            self.heap.free(bitmap);
            return;
        }
        if let Some((buffer, _)) = self.dib_do_decodificador.remove(&bitmap) {
            self.solta_superficie(buffer);
        }
        // Sem capacidade registrada o buffer não é nosso — é a superfície do próprio jogo.
        if self.dib_capacity.remove(&bitmap).is_none() {
            return;
        }
        if let Some(buffer) = self.dib_buffers.remove(&bitmap) {
            self.solta_superficie(buffer);
        }
        self.dib_herdados.remove(&bitmap);
        self.dib_publicado.remove(&bitmap);
        self.cpu.unwatch_dirty(bitmap);
    }

    /// Devolve um buffer à região de superfícies.
    ///
    /// **A região de superfícies não reciclava, e a Z-Wheel a esgotava.** A barra de abas cria um
    /// bitmap por quadro, alternando 440 e 441 de largura, e cada página da lista decodifica
    /// capas novas; medido, os 8 MB acabavam aos 27 s de navegação. Dali em diante o canvas da
    /// caixa de mensagem não tinha pixels, a caixa não abria, e confirmar um jogo derrubava a
    /// Z-Wheel num salto para o endereço zero em vez de lançá-lo.
    ///
    /// Reciclar só o bloco inteiro também não bastava: sem fundir vizinhos nem dividir blocos,
    /// buracos pequenos não serviam a pedidos grandes e a região esgotava do mesmo jeito, só
    /// mais devagar. Por isso ela usa o mesmo [`Heap`] do jogo.
    pub(super) fn solta_superficie(&mut self, endereco: u32) {
        self.superficies.free(endereco);
    }

    /// Reserva um buffer na região de superfícies. Devolve endereço e capacidade.
    pub(super) fn reserva_superficie(&mut self, bytes: u32) -> Option<(u32, u32)> {
        let endereco = self.superficies.alloc(bytes)?;
        Some((endereco, self.superficies.size_of(endereco).unwrap_or(bytes)))
    }

    /// Copia os pixels do host para o buffer que o jogo enxerga.
    pub(super) fn sync_to_guest(&mut self, bitmap: u32) -> Result<(), CpuError> {
        let Some(&buffer) = self.dib_buffers.get(&bitmap) else {
            return Ok(());
        };
        let herdado = self.dib_herdados.contains(&bitmap);
        let publicado = self.dib_publicado.get(&bitmap).copied();
        let Some(fb) = self.bitmaps.get_mut(&bitmap) else {
            return Ok(());
        };
        let serie = fb.serie();
        let sujeira = fb.toma_sujeira();
        // **Só o que mudou vai para o jogo.** Esta superfície já foi publicada inteira, e o
        // buffer só ficou para trás no retângulo que desenhamos desde então. Um sprite muda
        // alguns milhares de pixels; reescrever os 600 KB dela a cada `IIMAGE_Draw` era 92% do
        // tempo de API do Pac-Mania, que desenha 160 mil sprites em cinco segundos virtuais.
        //
        // A faixa escrita vai do primeiro pixel da caixa ao último, inclusive o que fica entre
        // as linhas fora dela: ali os dois lados já são iguais, porque a importação roda antes
        // de todo desenho, e uma escrita contígua sai mais barata que uma por linha.
        if !herdado && publicado == Some(serie) {
            let Some([x0, y0, x1, y1]) = sujeira else {
                return Ok(());
            };
            let largura = fb.width() as usize;
            let passo = fb.passo_do_dib();
            if passo == largura * 2 {
                let inicio = y0 as usize * largura + x0 as usize;
                let fim = (y1 as usize - 1) * largura + x1 as usize;
                let bytes = fb.rgb565_intervalo(inicio, fim);
                self.cpu.write_mem(buffer + inicio as u32 * 2, &bytes)?;
            } else {
                // Com enchimento no fim da linha, a faixa contígua não bate com o buffer: vai
                // linha a linha.
                for linha in y0 as usize..y1 as usize {
                    let inicio = linha * largura + x0 as usize;
                    let bytes = fb.rgb565_intervalo(inicio, linha * largura + x1 as usize);
                    let destino = linha * passo + x0 as usize * 2;
                    self.cpu.write_mem(buffer + destino as u32, &bytes)?;
                }
            }
        } else {
            let bytes = fb.to_dib_bytes();
            self.cpu.write_mem(buffer, &bytes)?;
        }
        self.dib_herdados.remove(&bitmap);
        self.dib_publicado.insert(bitmap, serie);
        // A superfície do guest acabou de ficar **idêntica** à nossa — e foi a escrita acima que
        // ligou o sinalizador. Limpar aqui é o que permite pular a importação seguinte: sem
        // isto toda saída sujaria tudo de novo e a vigia não economizaria nada.
        self.cpu.take_dirty(bitmap);
        Ok(())
    }

    /// Traz de volta o que o jogo escreveu direto no buffer.
    /// O endereço de um pixel dentro do buffer que o jogo enxerga, quando ele existe.
    ///
    /// É o que permite `DrawPixel` e `GetPixel` mexerem em dois bytes em vez de mandarem a
    /// superfície inteira de um lado para o outro.
    pub(super) fn dib_pixel(&self, bitmap: u32, x: i32, y: i32) -> Option<u32> {
        let &buffer = self.dib_buffers.get(&bitmap)?;
        let fb = self.bitmaps.get(&bitmap)?;
        let (width, height) = (fb.width() as i32, fb.height() as i32);
        if x < 0 || y < 0 || x >= width || y >= height {
            return None;
        }
        Some(buffer + (y as usize * fb.passo_do_dib()) as u32 + x as u32 * 2)
    }

    pub(super) fn sync_from_guest(&mut self, bitmap: u32) -> Result<(), CpuError> {
        let Some(&buffer) = self.dib_buffers.get(&bitmap) else {
            return Ok(());
        };
        let Some(fb) = self.bitmaps.get(&bitmap) else {
            return Ok(());
        };
        let tamanho = fb.passo_do_dib() * fb.height() as usize;
        // Bytes do objeto anterior: o host está à frente, e não há nada do jogo para trazer.
        if self.dib_herdados.contains(&bitmap) {
            return Ok(());
        }
        // **Só lê quando o guest escreveu nesta superfície.** É a mesma economia que o color
        // buffer do pbuffer já tinha, agora por superfície: a Z-Wheel copiava 1,57 GB em treze
        // segundos virtuais só para descobrir que quase nada tinha mudado.
        if !self.cpu.take_dirty(bitmap) {
            return Ok(());
        }
        let mut bytes = vec![0u8; tamanho];
        self.cpu.read_mem(buffer, &mut bytes)?;
        if let Some(fb) = self.bitmaps.get_mut(&bitmap) {
            fb.load_dib_bytes(&bytes);
            // Acabamos de copiar o buffer inteiro por cima da nossa cópia: os dois lados estão
            // iguais, e nada do que desenhamos antes ainda precisa ir para o jogo.
            fb.toma_sujeira();
            self.dib_publicado.insert(bitmap, fb.serie());
        }
        Ok(())
    }

    /// Sincroniza todas as superfícies que o jogo pode ter alterado direto.
    ///
    /// Chamado só nas interfaces que mexem em pixels: um jogo faz dezenas de milhares de
    /// chamadas de outras APIs, e copiar 600 KB em cada uma seria inviável.
    pub(super) fn sync_surfaces_in(&mut self) -> Result<(), CpuError> {
        for bitmap in self.dib_buffers.keys().copied().collect::<Vec<_>>() {
            self.sync_from_guest(bitmap)?;
        }
        Ok(())
    }

    pub(super) fn sync_surfaces_out(&mut self) -> Result<(), CpuError> {
        for bitmap in self.dib_buffers.keys().copied().collect::<Vec<_>>() {
            self.sync_to_guest(bitmap)?;
        }
        Ok(())
    }

    // Reutiliza a propriedade do buffer já serializada para DIBs de decodificadores.
    // O DIB criado não tem framebuffer 565: seu formato e ponteiros pertencem ao jogo.
    fn raw_dib(&self, bitmap: u32) -> bool {
        self.dib_do_decodificador.contains_key(&bitmap) && !self.bitmaps.contains_key(&bitmap)
    }

    pub(super) fn create_dib(&mut self, out: u32, depth: u8, width: u32, height: u32) -> Result<u32, CpuError> {
        if out == 0 { return Ok(EBADPARM); }
        self.cpu.write_u32(out, 0)?;
        // MegaMan cria só o descritor e depois o preenche para apontar a pixels externos.
        // Profundidade zero não define armazenamento; só cabe quando ambas dimensões são zero.
        if !matches!(depth, 1 | 2 | 4 | 8 | 16 | 24 | 32) && !(depth == 0 && width == 0 && height == 0) { return Ok(EUNSUPPORTED); }
        let pitch = (width * u32::from(depth)).div_ceil(32) * 4;
        if pitch > i16::MAX as u32 { return Ok(EBADPARM); }
        let Some(bytes) = pitch.checked_mul(height) else { return Ok(ENOMEMORY); };
        let (buffer, capacity) = if bytes == 0 { (0, 0) } else {
            let Some(allocation) = self.reserva_superficie(bytes) else { return Ok(ENOMEMORY); };
            allocation
        };
        let bitmap = self.new_object(Interface::Bitmap)?;
        if bitmap == 0 {
            if buffer != 0 { self.solta_superficie(buffer); }
            return Ok(ENOMEMORY);
        }
        // O pool pode reutilizar o endereço de um canvas 565 já liberado.
        self.solta_dib(bitmap);
        self.bitmaps.remove(&bitmap);
        self.dib_buffers.remove(&bitmap);
        self.dib_herdados.remove(&bitmap);
        self.cpu.unwatch_dirty(bitmap);
        if bytes != 0 { self.cpu.write_mem(buffer, &vec![0; bytes as usize])?; }
        self.dib_do_decodificador.insert(bitmap, (buffer, capacity));
        self.cpu.write_mem(bitmap + 4, &[0; 32])?;
        self.cpu.write_u32(bitmap + 8, buffer)?;
        self.cpu.write_mem(bitmap + 20, &(width as u16).to_le_bytes())?;
        self.cpu.write_mem(bitmap + 22, &(height as u16).to_le_bytes())?;
        self.cpu.write_mem(bitmap + 24, &(pitch as i16).to_le_bytes())?;
        self.cpu.write_mem(bitmap + 28, &[depth])?;
        self.cpu.write_u32(out, bitmap)?;
        Ok(SUCCESS)
    }

    /// Converte apenas a região visível do DIB no blit. Não altera a paleta ou o buffer
    /// públicos; Sonic 2 e Edge os preenchem depois da criação, às vezes com memória externa.
    fn blit_raw_dib(&mut self, dst: u32, dst_pos: (i32, i32), size: (i32, i32),
        src: u32, src_pos: (i32, i32), rop: u32) -> Result<u32, CpuError> {
        let Some(target) = self.bitmaps.get(&dst) else { return Ok(EUNSUPPORTED); };
        let (dw, dh) = (target.width() as i32, target.height() as i32);
        let mut header = [0; 32];
        self.cpu.read_mem(src + 4, &mut header)?;
        let word = |at| u32::from_le_bytes(header[at..at+4].try_into().unwrap());
        let pixels = word(4);
        let palette = word(8);
        let transparent = word(12);
        let width = u16::from_le_bytes([header[16], header[17]]) as i32;
        let height = u16::from_le_bytes([header[18], header[19]]) as i32;
        let pitch = i16::from_le_bytes([header[20], header[21]]) as i32;
        let count = u16::from_le_bytes([header[22], header[23]]) as usize;
        let (depth, scheme) = (header[24], header[25]);
        if !matches!(depth, 1 | 2 | 4 | 8 | 16 | 24 | 32) { return Ok(EUNSUPPORTED); }
        // IDIB: mapa vazio copia valores NativeColor, não exige uma paleta fictícia.
        let empty = count == 0 && scheme == 0;
        let indexed = !empty && scheme == 0 && depth <= 8;
        if (indexed && (count == 0 || count > (1usize << depth) || palette == 0))
            || (empty && depth > 16)
            || (!empty && !indexed && !matches!((depth, scheme), (16, 16) | (24 | 32, 24))) {
            return Ok(EUNSUPPORTED);
        }
        let (dx, dy) = dst_pos;
        let (sx, sy) = src_pos;
        let x0 = 0i64.max(-i64::from(dx)).max(-i64::from(sx));
        let y0 = 0i64.max(-i64::from(dy)).max(-i64::from(sy));
        let x1 = i64::from(size.0).min(i64::from(dw) - i64::from(dx)).min(i64::from(width) - i64::from(sx));
        let y1 = i64::from(size.1).min(i64::from(dh) - i64::from(dy)).min(i64::from(height) - i64::from(sy));
        if pixels == 0 || x0 >= x1 || y0 >= y1 { return Ok(SUCCESS); }
        let (x0, y0, x1, y1) = (x0 as i32, y0 as i32, x1 as i32, y1 as i32);
        let mut colors = vec![0; if indexed { count * 4 } else { 0 }];
        if indexed { self.cpu.read_mem(palette, &mut colors)?; }
        let start_bit = (sx + x0) as usize * depth as usize;
        let end_bit = (sx + x1) as usize * depth as usize;
        let mut row = vec![0; end_bit.div_ceil(8) - start_bit / 8];
        for y in y0..y1 {
            let Some(address) = pixels.checked_add_signed(pitch * (sy + y))
                .and_then(|base| base.checked_add((start_bit / 8) as u32)) else { return Ok(EBADPARM); };
            self.cpu.read_mem(address, &mut row)?;
            for x in x0..x1 {
                let bit = (sx + x) as usize * depth as usize - (start_bit / 8) * 8;
                let at = bit / 8;
                let native = match depth {
                    1 | 2 | 4 => u32::from((row[at] >> (8 - depth as usize - bit % 8)) & ((1u8 << depth) - 1)),
                    8 => u32::from(row[at]),
                    16 => u32::from(u16::from_le_bytes([row[at], row[at+1]])),
                    24 => u32::from_le_bytes([row[at], row[at+1], row[at+2], 0]),
                    32 => u32::from_le_bytes(row[at..at+4].try_into().unwrap()),
                    _ => unreachable!(),
                };
                if rop == AEE_RO_TRANSPARENT && native == transparent { continue; }
                let rgb565 = if indexed {
                    if native as usize >= count { continue; }
                    let at = native as usize * 4;
                    Rgb { b: colors[at], g: colors[at+1], r: colors[at+2] }.to_rgb565()
                } else if empty || depth == 16 { native as u16 } else {
                    Rgb { b: native as u8, g: (native >> 8) as u8, r: (native >> 16) as u8 }.to_rgb565()
                };
                self.bitmaps.get_mut(&dst).unwrap().set_pixel_native(dx+x, dy+y, rgb565);
            }
        }
        Ok(SUCCESS)
    }

    /// O contrato OEM anterior a 1.2 pertence ao IDisplay, nunca ao IBitmap.
    pub(super) fn blit_display(
        &mut self,
        dst: u32,
        dst_pos: (i32, i32),
        size: (i32, i32),
        src: u32,
        src_pos: (i32, i32),
        rop: u32,
    ) -> Result<u32, CpuError> {
        if dst == src {
            return Ok(SUCCESS);
        }
        if self.raw_dib(dst) {
            // Não anunciar cópia concluída quando ainda não há conversão para o DIB destino.
            return Ok(EUNSUPPORTED);
        }
        if self.raw_dib(src) {
            return self.blit_raw_dib(dst, dst_pos, size, src, src_pos, rop);
        }
        if self.bitmap_nativo_565
            && (loader::HEAP_BASE..loader::HEAP_BASE + loader::HEAP_SIZE as u32).contains(&src)
        {
            if let Some(source) = self.bitmaps.get(&src) {
                let layout = (source.width() as u16, source.height() as u16);
                return self.blit_raster_legado(dst, dst_pos, size, src, src_pos, rop, layout, true);
            }
        }
        if !self.bitmaps.contains_key(&src) {
            if let Some(layout) = self.raster_legado {
                return self.blit_raster_legado(dst, dst_pos, size, src, src_pos, rop, layout, false);
            }
        }
        // CONVERTBMP alimenta o IDisplay anterior a 1.2 com OLDMASK. The Terminator
        // pede 4 para os sprites magenta; o alias pertence ao display, não ao IBitmap.
        let rop = if rop == 4 { AEE_RO_TRANSPARENT } else { rop };
        self.blit(dst, dst_pos, size, src, src_pos, rop)
    }

    /// Copia superfícies IBitmap; ponteiros de pixels do contrato OEM não são objetos.
    pub(super) fn blit(
        &mut self, dst: u32, dst_pos: (i32, i32), size: (i32, i32), src: u32,
        src_pos: (i32, i32), rop: u32,
    ) -> Result<u32, CpuError> {
        if dst == src { return Ok(SUCCESS); }
        if self.raw_dib(dst) { return Ok(EUNSUPPORTED); }
        if self.raw_dib(src) {
            return self.blit_raw_dib(dst, dst_pos, size, src, src_pos, rop);
        }
        if self.bitmap_nativo_565
            && (loader::HEAP_BASE..loader::HEAP_BASE + loader::HEAP_SIZE as u32).contains(&src)
            && self.bitmaps.contains_key(&src)
        {
            return Ok(EUNSUPPORTED);
        }
        let Some(mut target) = self.bitmaps.remove(&dst) else {
            return Ok(SUCCESS);
        };
        if let Some(source) = self.bitmaps.get(&src) {
            let transparent = if rop == AEE_RO_TRANSPARENT {
                // Sem cor pedida, vale a do BREW: `RGB_MASK_COLOR`, o magenta. O Action Hero 3D
                // desenha cada letra com `AEE_RO_TRANSPARENT` de uma folha de fundo magenta
                // sem nunca chamar `SetTransparencyColor`; sem o padrão, a folha inteira
                // aparecia na tela a cada letra.
                Some(self.transparency.get(&src).copied().unwrap_or(TRANSPARENT_KEY))
            } else {
                None
            };
            target.blit(
                dst_pos.0,
                dst_pos.1,
                size.0,
                size.1,
                source,
                src_pos.0,
                src_pos.1,
                transparent,
            );
        }
        self.bitmaps.insert(dst, target);
        Ok(SUCCESS)
    }

    fn blit_raster_legado(
        &mut self, dst: u32, dst_pos: (i32, i32), size: (i32, i32), src: u32,
        src_pos: (i32, i32), rop: u32, layout: (u16, u16), native565: bool,
    ) -> Result<u32, CpuError> {
        // OLDMASK (4) pertence ao IDisplay anterior a 1.2; no display colorido
        // a chave é magenta, como TRANSPARENT. Não se estende esse alias ao IBitmap.
        if rop != AEE_RO_COPY && !(native565 && matches!(rop, 4 | AEE_RO_TRANSPARENT)) {
            return Ok(EUNSUPPORTED);
        }
        if src == 0 { return Ok(EBADPARM); }
        let Some(target) = self.bitmaps.get(&dst) else { return Ok(EUNSUPPORTED); };
        let (dx, dy) = dst_pos;
        let (sx, sy) = src_pos;
        let (width, height) = (i64::from(layout.0), i64::from(layout.1));
        let x0 = 0i64.max(-i64::from(dx)).max(-i64::from(sx));
        let y0 = 0i64.max(-i64::from(dy)).max(-i64::from(sy));
        let x1 = i64::from(size.0).min(i64::from(target.width()) - i64::from(dx)).min(width - i64::from(sx));
        let y1 = i64::from(size.1).min(i64::from(target.height()) - i64::from(dy)).min(height - i64::from(sy));
        if x0 >= x1 || y0 >= y1 { return Ok(SUCCESS); }
        let mut row = vec![0u8; (x1 - x0) as usize * 2];
        for y in y0..y1 {
            let offset = ((i64::from(sy) + y) * width + i64::from(sx) + x0) * 2;
            let prefix = if native565 { PREFIXO_NATIVO_565 } else { 0 };
            let Some(address) = u32::try_from(offset).ok()
                .and_then(|n| n.checked_add(prefix)).and_then(|n| src.checked_add(n)) else {
                return Ok(EBADPARM);
            };
            self.cpu.read_mem(address, &mut row)?;
            let target = self.bitmaps.get_mut(&dst).unwrap();
            for (column, bytes) in row.chunks_exact(2).enumerate() {
                let color = u16::from_le_bytes([bytes[0], bytes[1]]);
                if native565 && rop != AEE_RO_COPY && color == TRANSPARENT_KEY { continue; }
                let pixel = if native565 { color } else {
                    Rgb {
                        r: ((color >> 8) & 15) as u8 * 17,
                        g: ((color >> 4) & 15) as u8 * 17,
                        b: (color & 15) as u8 * 17,
                    }.to_rgb565()
                };
                target.set_pixel_native(
                    (i64::from(dx) + x0) as i32 + column as i32,
                    (i64::from(dy) + y) as i32, pixel,
                );
            }
        }
        Ok(SUCCESS)
    }

    /// Lê um `AEERect` da memória do guest. `None` quando o ponteiro é nulo.
    pub(super) fn clip_rect(&self, rect: Rect) -> Option<Rect> {
        clip_rect(self.clip, rect)
    }

    pub(super) fn clip_blit(
        &self,
        dst: (i32, i32),
        size: (i32, i32),
        src: (i32, i32),
    ) -> Option<Blit> {
        clip_blit(self.clip, dst, size, src)
    }

    pub(super) fn read_rect(&self, addr: u32) -> Result<Option<Rect>, CpuError> {
        if addr == 0 {
            return Ok(None);
        }
        let mut bytes = [0u8; 8];
        self.cpu.read_mem(addr, &mut bytes)?;
        Ok(Some(Rect::from_bytes(bytes)))
    }

    /// Cada superfície viva como um BMP, para inspeção: endereço, tamanho e bytes.
    ///
    /// Existe porque "o jogo desenha e a tela fica preta" tem duas causas possíveis, e só o
    /// conteúdo das superfícies as separa: ou o jogo desenhou em algo que não vai para a tela,
    /// ou não desenhou. Descobrir isso por instrumentação temporária custou duas investigações.
    pub fn superficies(&self) -> Vec<(u32, u32, u32, Vec<u8>)> {
        self.bitmaps
            .iter()
            .map(|(&addr, fb)| (addr, fb.width(), fb.height(), fb.to_bmp()))
            .collect()
    }
}

/// Os argumentos de um `TransformBltComplex`, já lidos do guest.
struct TransformBlt {
    destino: u32,
    origem: u32,
    x: i32,
    y: i32,
    x_origem: i32,
    y_origem: i32,
    largura: i32,
    altura: i32,
    /// `A, B, C, D`, já fora do ponto fixo.
    m: [f32; 4],
    /// AEETransform: 0 pula a cor nativa transparente; os demais valores copiam tudo.
    composto: u8,
}

#[cfg(test)]
mod testes_dib_criado {
    use super::*;
    use crate::cpu::BackendPadrao;

    fn machine() -> (Machine<BackendPadrao>, crate::scratch::TempDir) {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        // Cada teste roda em paralelo; um nome comum apagava a pasta do vizinho no Windows.
        let root = crate::scratch::TempDir::new(&format!("brew-dib-criado-{}-{}",
            std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        let image = loader::modfile::ModImage::parse(
            [0xe3a0_0000u32, 0xe12f_ff1e].into_iter().flat_map(u32::to_le_bytes).collect(),
        ).unwrap();
        let loaded = loader::load_with(&image, &[]).unwrap();
        let storage = crate::storage::StoragePaths::from_root(root.join("runtime"));
        let mut machine = Machine::new_with_storage(
            BackendPadrao::new().unwrap(), loaded, root.path(), &storage, None,
        );
        machine.run(1000).unwrap();
        (machine, root)
    }

    fn call(machine: &mut Machine<BackendPadrao>, iface: Interface, name: &str, regs: [u32; 4], extra: &[u32]) -> u32 {
        let slot = (0..256).find(|slot| iface.method(*slot) == Some(name)).unwrap();
        match machine.call_guest_with_stack(aee::encode(iface, slot), regs, extra, 1000).unwrap() {
            Outcome::Returned { code } => code,
            other => panic!("{name}: {other:?}"),
        }
    }

    #[test]
    fn raster_legado_444_substitui_o_quadro_com_cores_corretas() {
        let (mut machine, _root) = machine();
        machine.raster_legado = Some((3, 2));
        let display = loader::OBJECT_BASE + 0x10;
        let pixels = loader::HEAP_BASE + 0x800;
        machine.cpu.write_mem(pixels, &[
            0x00,0x0f, 0xf0,0x00, 0x0f,0x00,
            0xff,0x0f, 0x00,0x00, 0x88,0x08,
        ]).unwrap();
        call(&mut machine, Interface::Display, "BitBlt", [display,2,3,3], &[2,pixels,0,0,AEE_RO_COPY]);
        assert_eq!((2..5).map(|x| machine.screen().get_pixel(x,3)).collect::<Vec<_>>(), [0xf800,0x07e0,0x001f]);
        assert_eq!((2..5).map(|x| machine.screen().get_pixel(x,4)).collect::<Vec<_>>(), [0xffff,0,0x8c51]);
        // Um quadro preto deve apagar o anterior, inclusive texto que estivesse nele.
        machine.cpu.write_mem(pixels, &[0;12]).unwrap();
        call(&mut machine, Interface::Display, "BitBlt", [display,2,3,3], &[2,pixels,0,0,AEE_RO_COPY]);
        assert_eq!(machine.screen().get_pixel(2,3),0);
    }

    #[test]
    fn raster_legado_respeita_stride_origem_clip_e_limites_da_origem() {
        let (mut machine, _root) = machine();
        machine.raster_legado = Some((4,3));
        let display = loader::OBJECT_BASE + 0x10;
        let pixels = loader::HEAP_BASE + 0x800;
        let values = [0u16,0,0,0, 0x123,0x456,0x0f0,0x789, 0xabc,0xdef,0x00f,0xfff];
        machine.cpu.write_mem(pixels, &values.into_iter().flat_map(u16::to_le_bytes).collect::<Vec<_>>()).unwrap();
        let clip = pixels + 0x100;
        machine.cpu.write_mem(clip, &[3,0,3,0,1,0,2,0]).unwrap();
        call(&mut machine, Interface::Display, "SetClipRect", [display,clip,0,0], &[]);
        call(&mut machine, Interface::Display, "BitBlt", [display,2,3,3], &[2,pixels,1,1,AEE_RO_COPY]);
        assert_eq!(machine.screen().get_pixel(3,3),0x07e0);
        assert_eq!(machine.screen().get_pixel(3,4),0x001f);
        assert_eq!(machine.screen().get_pixel(2,3),0);
        assert_eq!(machine.screen().get_pixel(4,4),0);
        call(&mut machine, Interface::Display, "SetClipRect", [display,0,0,0], &[]);
        call(&mut machine, Interface::Display, "BitBlt", [display,0,0,i32::MAX as u32], &[i32::MAX as u32,pixels,0,0,AEE_RO_COPY]);
        assert_eq!(machine.screen().get_pixel(3,2),0xffff);
        assert_eq!(machine.screen().get_pixel(4,0),0);
    }

    #[test]
    fn origem_desconhecida_nao_e_adivinhada_como_rgb444() {
        let (mut machine, _root) = machine();
        let display = loader::OBJECT_BASE + 0x10;
        let pixels = loader::HEAP_BASE + 0x800;
        machine.cpu.write_mem(pixels, &[0,15,240,0,15,0]).unwrap();
        call(&mut machine, Interface::Display, "BitBlt", [display,0,0,3], &[1,pixels,0,0,AEE_RO_COPY]);
        assert_eq!(machine.screen().get_pixel(0,0),0);
    }

    #[test]
    fn bitmap_nativo_legado_devolve_pixels_editaveis_e_sysfree_os_libera() {
        let (mut machine, _root) = machine();
        machine.bitmap_nativo_565 = true;
        let input = loader::HEAP_BASE + 0x20000;
        let info = input + 0x100;
        let realocou = info + 0x20;
        let mut bmp = vec![0u8;58];
        bmp[..2].copy_from_slice(b"BM");
        bmp[2..6].copy_from_slice(&58u32.to_le_bytes());
        bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
        bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
        bmp[18..22].copy_from_slice(&1i32.to_le_bytes());
        bmp[22..26].copy_from_slice(&1i32.to_le_bytes());
        bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
        bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
        bmp[54..58].copy_from_slice(&[0,0,255,0]);
        machine.cpu.write_mem(input,&bmp).unwrap();
        let raw = call(&mut machine, Interface::Helpers, "SetupNativeImage", [0x01004001,input,info,realocou], &[]);
        assert!((loader::HEAP_BASE..loader::HEAP_BASE + loader::HEAP_SIZE as u32).contains(&raw),
            "o contrato antigo entrega pixels, não um IBitmap na região de objetos");
        let mut bytes = [0u8;2];
        machine.cpu.read_mem(raw+12,&mut bytes).unwrap();
        assert_eq!(u16::from_le_bytes(bytes),0xf800);
        // Pandora substitui o raster retornado pela CONVERTBMP com sua própria paleta.
        machine.cpu.write_mem(raw+12,&0x07e0u16.to_le_bytes()).unwrap();
        let display = loader::OBJECT_BASE + 0x10;
        call(&mut machine, Interface::Display, "BitBlt", [display,2,3,1], &[1,raw,0,0,AEE_RO_COPY]);
        assert_eq!(machine.screen().get_pixel(2,3),0x07e0);
        // OLDMASK usa a chave magenta do bitmap; o desenho deve conservar o destino.
        machine.cpu.write_mem(raw+12,&0xf81fu16.to_le_bytes()).unwrap();
        call(&mut machine, Interface::Display, "BitBlt", [display,2,3,1], &[1,raw,0,0,4]);
        assert_eq!(machine.screen().get_pixel(2,3),0x07e0);
        call(&mut machine, Interface::Helpers, "sysfree", [raw,0,0,0], &[]);
        assert!(!machine.bitmaps.contains_key(&raw));
        assert!(machine.heap.size_of(raw).is_none());
    }

    #[test]
    fn bitmap_nativo_legado_nao_estende_oldmask_ao_ibitmap() {
        let (mut machine, _root) = machine();
        machine.bitmap_nativo_565 = true;
        let pixels = machine.malloc(2).unwrap();
        machine.cpu.write_mem(pixels,&0xf81fu16.to_le_bytes()).unwrap();
        machine.bitmaps.insert(pixels,Framebuffer::new(1,1));
        let target = machine.target().unwrap();
        assert_eq!(call(&mut machine, Interface::Bitmap, "BltIn", [target,0,0,1], &[1,pixels,0,0,4]),EUNSUPPORTED);
    }

    #[test]
    fn bitmap_nativo_legado_aceita_bmp_gerado_com_tamanho_menor_que_pixels() {
        let (mut machine, _root) = machine();
        let mut bmp = vec![0u8;1090]; // paleta de 256 cores e raster 4x3, stride 4.
        bmp[..2].copy_from_slice(b"BM");
        bmp[2..6].copy_from_slice(&1086u32.to_le_bytes());
        bmp[10..14].copy_from_slice(&1078u32.to_le_bytes());
        bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
        bmp[18..22].copy_from_slice(&4i32.to_le_bytes());
        bmp[22..26].copy_from_slice(&3i32.to_le_bytes());
        bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
        bmp[28..30].copy_from_slice(&8u16.to_le_bytes());
        let input = machine.malloc(bmp.len() as u32).unwrap();
        machine.cpu.write_mem(input,&bmp).unwrap();
        // O raster completo alocado vale também no formato de saída IBitmap;
        // a saída OEM abaixo continua sendo um buffer de pixels editável.
        assert_ne!(call(&mut machine,Interface::Helpers,"SetupNativeImage",[0x01004001,input,0,0],&[]),0);
        machine.bitmap_nativo_565 = true;
        let native = call(&mut machine,Interface::Helpers,"SetupNativeImage",[0x01004001,input,0,0],&[]);
        assert_ne!(native,0);
        assert_eq!(machine.bitmaps[&native].width(),4);
        assert_eq!(machine.bitmaps[&native].height(),3);
        assert!(machine.heap.size_of(native).unwrap() >= 12+4*3*2);
        machine.cpu.write_u32(input+2,2048).unwrap();
        assert_eq!(machine.encoded_image_len(input).unwrap(),Some(1090),
            "o raster completo, e não bfSize, limita a leitura dentro da alocação");
        assert_ne!(call(&mut machine,Interface::Helpers,"SetupNativeImage",[0x01004001,input,0,0],&[]),0);
        machine.cpu.write_u32(input+22,10000).unwrap();
        assert_eq!(machine.encoded_image_len(input).unwrap(),None,
            "dimensões que ultrapassam a alocação não autorizam leitura vizinha");
    }

    #[test]
    fn bitmap_nativo_legado_realocado_nao_deixa_metadados_em_bloco_livre() {
        let (mut machine, _root) = machine();
        machine.bitmap_nativo_565 = true;
        for size in [0, 16] {
            let pixels = machine.malloc(2).unwrap();
            machine.bitmaps.insert(pixels,Framebuffer::new(1,1));
            let next = machine.realloc(pixels,size).unwrap();
            assert!(!machine.bitmaps.contains_key(&pixels));
            if size != 0 {
                assert_ne!(next,0);
                assert!(machine.bitmaps.contains_key(&next));
                machine.solta_bitmap_nativo(next);
                machine.heap.free(next);
            }
        }
    }

    #[test]
    fn dib_sem_mapeamento_copia_valores_nativos_literalmente() {
        let (mut machine, _root) = machine();
        let display = loader::OBJECT_BASE + 0x10;
        let out = loader::HEAP_BASE + 0x800;
        assert_eq!(call(&mut machine, Interface::Display, "CreateDIBitmap", [display,out,16,3], &[1]), SUCCESS);
        let dib = machine.cpu.read_u32(out).unwrap();
        let pixels = machine.cpu.read_u32(dib+8).unwrap();
        machine.cpu.write_mem(pixels, &[0,248,224,7,31,0,0,0]).unwrap();
        call(&mut machine, Interface::Display, "BitBlt", [display,0,0,3], &[1,dib,0,0,AEE_RO_COPY]);
        assert_eq!((0..3).map(|x| machine.screen().get_pixel(x,0)).collect::<Vec<_>>(), [0xf800,0x07e0,0x001f]);
    }

    #[test]
    fn dib_destino_recusa_copia_ainda_nao_implementada() {
        let (mut machine, _root) = machine();
        let display = loader::OBJECT_BASE + 0x10;
        let out = loader::HEAP_BASE + 0x800;
        call(&mut machine, Interface::Display, "GetDeviceBitmap", [display,out,0,0], &[]);
        let source = machine.cpu.read_u32(out).unwrap();
        call(&mut machine, Interface::Display, "CreateDIBitmap", [display,out,16,1], &[1]);
        let dib = machine.cpu.read_u32(out).unwrap();
        assert_eq!(call(&mut machine, Interface::Bitmap, "BltIn", [dib,0,0,1],
            &[1,source,0,0,AEE_RO_COPY]), EUNSUPPORTED);
    }

    #[test]
    fn dib_indexado_criado_preserva_paleta_e_blita_pixels_e_transparencia() {
        for depth in [4, 8] {
            let (mut machine, _root) = machine();
            let display = loader::OBJECT_BASE + 0x10;
            let out = loader::HEAP_BASE + 0x800;
            assert_eq!(call(&mut machine, Interface::Display, "CreateDIBitmap", [display, out, depth, 3], &[2]), SUCCESS);
            let dib = machine.cpu.read_u32(out).unwrap();
            let pixels = machine.cpu.read_u32(dib + 8).unwrap();
            assert_ne!(pixels, 0);
            let mut header = [0; 16];
            machine.cpu.read_mem(dib + 20, &mut header).unwrap();
            assert_eq!(&header[..10], &[3,0,2,0,4,0,0,0,depth as u8,0]);
            let palette = out + 0x100;
            machine.cpu.write_mem(palette, &[0,0,0,0, 0,0,255,0, 0,255,0,0]).unwrap();
            machine.cpu.write_u32(dib + 12, palette).unwrap();
            machine.cpu.write_mem(dib + 26, &3u16.to_le_bytes()).unwrap();
            let bytes = if depth == 4 { [0x12,0x10,0,0,0x21,0x20,0,0] } else { [1,2,1,0,2,1,2,0] };
            machine.cpu.write_mem(pixels, &bytes).unwrap();
            assert_eq!(call(&mut machine, Interface::Bitmap, "QueryInterface", [dib,AEECLSID_DIB,out+4,0], &[]), SUCCESS);
            assert_eq!(machine.cpu.read_u32(dib + 12).unwrap(), palette);
            assert_eq!(call(&mut machine, Interface::Display, "BitBlt", [display,0,0,3], &[2,dib,0,0,AEE_RO_COPY]), SUCCESS);
            assert_eq!((0..3).map(|x| machine.screen().get_pixel(x,0)).collect::<Vec<_>>(), [0xf800,0x07e0,0xf800]);
            assert_eq!((0..3).map(|x| machine.screen().get_pixel(x,1)).collect::<Vec<_>>(), [0x07e0,0xf800,0x07e0]);
            // A paleta e os pixels podem mudar ou ser externos; não basta converter só na criação.
            machine.cpu.write_mem(palette + 4, &[255,0,0,0]).unwrap();
            machine.cpu.write_u32(dib + 16, 2).unwrap();
            let external = out + 0x200;
            machine.cpu.write_mem(external, &bytes).unwrap();
            machine.cpu.write_u32(dib + 8, external).unwrap();
            assert_eq!(call(&mut machine, Interface::Display, "BitBlt", [display,0,2,3], &[2,dib,0,0,AEE_RO_TRANSPARENT]), SUCCESS);
            assert_eq!((0..3).map(|x| machine.screen().get_pixel(x,2)).collect::<Vec<_>>(), [0x001f,0,0x001f]);
            assert_eq!(call(&mut machine, Interface::Bitmap, "Release", [dib,0,0,0], &[]), 1);
            assert_eq!(call(&mut machine, Interface::Bitmap, "Release", [dib,0,0,0], &[]), 0);
        }
    }
}
