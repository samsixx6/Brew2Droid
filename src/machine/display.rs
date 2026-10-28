//! IDisplay e IGraphics: o desenho 2D e o texto na tela.

use super::*;

impl<C: CpuBackend> Machine<C> {
    /// Métodos de `IDisplay`. Despachamos pelo **nome** do slot, não pelo número: a tabela de
    /// nomes vem dos headers do SDK, então um método fora de ordem viraria erro de compilação
    /// aqui em vez de desenho errado lá.
    pub(super) fn display_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        // **O quadro do OpenGL vem antes do 2D.** Um HUD desenhado por cima de uma tela que
        // ainda não recebeu a cena apagaria a cena — ver [`Machine::materializa_quadro_gl`].
        self.materializa_quadro_gl();
        let Some(name) = Interface::Display.method(slot) else {
            return Ok(None);
        };
        let result = match name {
            // O `IDisplay` não tem contagem própria: o objeto é a tela, e ela vive enquanto o
            // emulador viver. Contamos as referências só para o jogo ver o número que espera.
            "AddRef" => self.objects.add_ref(self.cpu.read_reg(Reg::R0)),
            "Release" => self.objects.release(self.cpu.read_reg(Reg::R0)),
            // A Machine mantém o dispositivo habilitado e segura seu bitmap até o fim;
            // destinos fora da tela também são sempre habilitados (AEEIDisplay.h).
            "IsEnabled" => TRUE,
            "NotifyEnable" => {
                // O SDK só dispara o callback numa mudança de estado, não ao registrar.
                // Como este runtime não desabilita nem destrói o dispositivo durante
                // sua execução, aceitar o registro não deve inventar uma notificação.
                // AEEError.h móvel: EBADPARM é 14 (o valor histórico do core difere).
                if self.cpu.read_reg(Reg::R1) == 0 {
                    14
                } else {
                    SUCCESS
                }
            }
            // RGBVAL IDISPLAY_SetColor(IDisplay *p, AEEClrItem clr, RGBVAL rgb)
            "SetColor" => {
                let item = self.cpu.read_reg(Reg::R1) as usize;
                let previous = self.colors.get(item).copied().unwrap_or(Rgb::BLACK);
                if let Some(slot) = self.colors.get_mut(item) {
                    *slot = Rgb::from_rgbval(self.cpu.read_reg(Reg::R2));
                }
                to_rgbval(previous)
            }
            "DrawFrame" => {
                let estilo = self.arg(2);
                // DRIV3R usa BOX para a seleção dos menus. Os estilos 3D e de
                // abas continuam explícitos como não implementados.
                let margem: i16 = match estilo {
                    0 => 0,     // NONE
                    1 | 5 => 1, // EMPTY / BOX
                    2 => 2,     // 3D_EMPTY
                    _ => return Ok(None),
                };
                let endereco = self.arg(1);
                if let Some(rect) = self.read_rect(endereco)? {
                    let interno = Rect {
                        x: rect.x.saturating_add(margem),
                        y: rect.y.saturating_add(margem),
                        width: rect.width.saturating_sub(2 * margem).max(0),
                        height: rect.height.saturating_sub(2 * margem).max(0),
                    };
                    // O argumento é in/out; recortar o desenho não muda a
                    // margem que o jogo recebe para posicionar seu texto.
                    let mut bytes = [0u8; 8];
                    for (campo, valor) in bytes.chunks_exact_mut(2).zip([
                        interno.x,
                        interno.y,
                        interno.width,
                        interno.height,
                    ]) {
                        campo.copy_from_slice(&valor.to_le_bytes());
                    }
                    self.cpu.write_mem(endereco, &bytes)?;
                    let target = self.target()?;
                    let fill = self.arg(3);
                    if fill != RGB_NONE {
                        if let (Some(area), Some(fb)) =
                            (self.clip_rect(interno), self.bitmaps.get_mut(&target))
                        {
                            fb.fill_rect(area, Rgb::from_rgbval(fill));
                        }
                    }
                    if estilo == 5 && rect.width > 0 && rect.height > 0 {
                        let bordas = [
                            Rect { height: 1, ..rect },
                            Rect {
                                y: rect.y.saturating_add(rect.height - 1),
                                height: 1,
                                ..rect
                            },
                            Rect { width: 1, ..rect },
                            Rect {
                                x: rect.x.saturating_add(rect.width - 1),
                                width: 1,
                                ..rect
                            },
                        ];
                        let cor = self.colors[CLR_USER_LINE];
                        for borda in bordas {
                            // Recortar cada aresta evita inventar bordas no
                            // limite do clip quando o quadro o atravessa.
                            if let (Some(area), Some(fb)) =
                                (self.clip_rect(borda), self.bitmaps.get_mut(&target))
                            {
                                fb.fill_rect(area, cor);
                            }
                        }
                    }
                }
                margem as u32
            }
            // void IDISPLAY_DrawRect(IDisplay *p, const AEERect *pr, RGBVAL cf, RGBVAL cfill,
            //                        uint32 flags)
            "DrawRect" => {
                let target = self.target()?;
                // **Ponteiro nulo é a superfície inteira.** É como o `ClearScreen` do SDK é
                // escrito, e é a única forma de limpar a tela que o BREW oferece.
                let pedido = match self.read_rect(self.cpu.read_reg(Reg::R1))? {
                    Some(rect) => Some(rect),
                    None => self.bitmaps.get(&target).map(|fb| Rect {
                        x: 0,
                        y: 0,
                        width: fb.width().min(i16::MAX as u32) as i16,
                        height: fb.height().min(i16::MAX as u32) as i16,
                    }),
                };
                let rect = pedido.and_then(|rect| self.clip_rect(rect));
                // `RGB_NONE` quer dizer "a cor corrente" — a de fundo para o preenchimento e a
                // de linha para a moldura, ambas do `IDISPLAY_SetColor`.
                let cor = |valor: u32, item: usize, cores: &[Rgb]| match valor {
                    RGB_NONE => cores[item],
                    valor => Rgb::from_rgbval(valor),
                };
                let border = cor(self.cpu.read_reg(Reg::R2), CLR_USER_LINE, &self.colors);
                let fill = cor(
                    self.cpu.read_reg(Reg::R3),
                    CLR_USER_BACKGROUND,
                    &self.colors,
                );
                // **Quem manda no que é desenhado é o `flags`, não a cor.** O Quake pede moldura
                // sozinha (`IDF_RECT_FRAME`) passando preto no preenchimento: honrar a cor e
                // ignorar o sinalizador pintava um retângulo preto que ele não pediu.
                //
                // Sinalizador nenhum mantém o comportamento antigo — desenhar os dois. Nenhum
                // jogo do acervo chama assim, e na dúvida é melhor continuar desenhando do que
                // apagar uma tela por causa de uma leitura que não deu para conferir.
                let flags = match self.stack_arg(0)? {
                    0 => IDF_RECT_FRAME | IDF_RECT_FILL,
                    flags => flags,
                };
                if let (Some(rect), Some(fb)) = (rect, self.bitmaps.get_mut(&target)) {
                    if flags & IDF_RECT_FILL != 0 {
                        fb.fill_rect(rect, fill);
                    }
                    if flags & IDF_RECT_FRAME != 0 {
                        fb.draw_frame(rect, border);
                    }
                }
                SUCCESS
            }
            // int IDISPLAY_GetDeviceBitmap(IDisplay *p, IBitmap **ppIBitmap).
            //
            // Devolve **código de erro** e entrega a superfície pelo ponteiro de saída, ao
            // contrário do `GetDestination` logo abaixo, que devolve o `IBitmap *` direto. Eu
            // tinha implementado os dois iguais, e o Bejeweled Twist — que testa
            // `if (retorno != 0) falhou` — desistia da inicialização por causa disso.
            "GetDeviceBitmap" => {
                let out = self.cpu.read_reg(Reg::R1);
                let bitmap = self.device_bitmap()?;
                // Asphalt 3 lê pBmp e guarda o endereço antes de QueryInterface. A tela
                // precisa nascer com pixels públicos; publicar só na consulta deixa o jogo
                // desenhando pelo nulo que já guardou. expose_dib reaproveita o buffer e não
                // copia a tela inteira nas consultas seguintes.
                if bitmap != 0 {
                    self.expose_dib(bitmap)?;
                }
                if out != 0 {
                    self.cpu.write_u32(out, bitmap)?;
                }
                if bitmap == 0 {
                    ENOMEMORY
                } else {
                    // Devolve uma referência: o jogo dá `Release` quando termina, e faz isso
                    // uma vez por quadro. Sem o `AddRef` a contagem zerava e a superfície da
                    // tela era destruída no meio da execução.
                    self.objects.add_ref(bitmap);
                    SUCCESS
                }
            }
            "CreateDIBitmap" => self.create_dib(
                self.arg(1),
                self.arg(2) as u8,
                self.arg(3) & 0xffff,
                self.stack_arg(0)? & 0xffff,
            )?,
            // int SetDestination(IDisplay *, IBitmap *pbmDest)
            //
            // O destino nem sempre é uma superfície nossa: o Bejeweled Twist implementa o
            // próprio `IBitmap`, com a vtable embutida no objeto (o `DECLARE_VTBL` do BREW).
            // Para desenhar nela é preciso perguntar a ela onde ficam os pixels — o que exige
            // entrar no guest, e por isso fica para a fronteira da chamada.
            "SetDestination" => {
                let target = self.cpu.read_reg(Reg::R1);

                if target != 0 && !self.bitmaps.contains_key(&target) && self.probed.insert(target)
                {
                    self.pending_probes.push(target);
                }
                // **O display segura o destino.** A transição da Z-Wheel (`0x74e20`) cria um
                // bitmap fora da tela, aponta o display para ele e solta a referência dela logo
                // em seguida. Sem a nossa, o endereço voltava para a lista de livres, o bitmap
                // seguinte nascia no lugar, e a tela nova era desenhada num objeto que já não
                // era o destino — a transição ficava parada no menu antigo para sempre.
                let anterior = self.display_target;
                if target != anterior {
                    if target != 0 && self.bitmaps.contains_key(&target) {
                        self.objects.add_ref(target);
                    }
                    if anterior != 0
                        && anterior != self.device_bitmap
                        && self.bitmaps.contains_key(&anterior)
                        && self.objects.release(anterior) == 0
                    {
                        self.solta_dib(anterior);
                    }
                }
                self.display_target = target;
                SUCCESS
            }
            // Devolve o `IBitmap *` direto — e **com contagem**, porque quem recebe solta.
            //
            // Sem o `AddRef`, cada `GetDestination` que o jogo soltava tirava uma referência
            // que ninguém tinha posto. Medido na Z-Wheel: o bitmap da tela chegava a zero, o
            // endereço voltava para a lista de livres **com a superfície ainda viva**, e o
            // objeto seguinte — o widget do roller — nascia por cima da tela.
            "GetDestination" => {
                let alvo = self.target()?;
                if alvo != 0 {
                    self.objects.add_ref(alvo);
                }
                alvo
            }
            // void IDISPLAY_BitBlt(IDisplay *p, int xd, int yd, int w, int h,
            //                      const void *pbmSource, int xs, int ys, AEERasterOp rop)
            "BitBlt" => {
                let dst_pos = (
                    self.cpu.read_reg(Reg::R1) as i32,
                    self.cpu.read_reg(Reg::R2) as i32,
                );
                let size = (self.cpu.read_reg(Reg::R3) as i32, self.stack_arg(0)? as i32);
                let src = self.stack_arg(1)?;
                let origin = (self.stack_arg(2)? as i32, self.stack_arg(3)? as i32);
                let rop = self.stack_arg(4)?;
                let target = self.target()?;
                if let Some((dst_pos, size, origin)) = self.clip_blit(dst_pos, size, origin) {
                    self.blit_display(target, dst_pos, size, src, origin, rop)?;
                }
                SUCCESS
            }
            // void IDISPLAY_DrawText(IDisplay *, AEEFont, const AECHAR *pcText, int nChars,
            //                        int x, int y, const AEERect *prcBackground, uint32 dwFlags)
            // int IDISPLAY_DrawText(IDisplay *p, AEEFont nFont, const AECHAR *pcText, int nChars,
            //                       int x, int y, const AEERect *prcBackground, uint32 dwFlags)
            //
            // O retângulo e os sinalizadores mandam na posição: com alinhamento pedido num eixo,
            // o `x` ou o `y` daquele eixo não conta, e o texto não sai do retângulo. A barra de
            // abas da Z-Wheel desenha cada rótulo em `(0, 0)` pedindo centro e meio num
            // retângulo de 214 pixels; ignorar isso encostava tudo à esquerda e jogava o rótulo
            // anterior para fora da barra.
            "DrawText" => {
                let mut units = self.read_aechar_units(self.cpu.read_reg(Reg::R2))?;
                let chars = self.cpu.read_reg(Reg::R3) as i32;
                if chars >= 0 {
                    units.truncate(chars as usize);
                }
                let text = String::from_utf16_lossy(&units);
                let (x, y) = (self.stack_arg(0)? as i32, self.stack_arg(1)? as i32);
                let fundo = self.read_rect(self.stack_arg(2)?)?;
                let flags = self.stack_arg(3)?;
                if let (Some(rect), true) = (fundo, flags & IDF_RECT_FILL != 0) {
                    let cor = self
                        .colors
                        .get(CLR_USER_BACKGROUND)
                        .copied()
                        .unwrap_or(Rgb::WHITE);
                    let target = self.target()?;
                    if let (Some(rect), Some(fb)) =
                        (self.clip_rect(rect), self.bitmaps.get_mut(&target))
                    {
                        fb.fill_rect(rect, cor);
                    }
                }
                let medida = self.font.as_ref().map(|fonte| {
                    (
                        fonte.width(&text, FONT_SIZE) as i32,
                        (fonte.ascent(FONT_SIZE) + fonte.descent(FONT_SIZE)) as i32,
                    )
                });
                let (x, y) = match (fundo, medida) {
                    (Some(rect), Some(medida)) => posicao_do_texto((x, y), rect, flags, medida),
                    _ => (x, y),
                };
                // O texto fica dentro do retângulo: é o recorte que o BREW aplica, e é o que
                // corta o rótulo vizinho na borda da barra em vez de deixá-lo vazar.
                let recorte_anterior = self.clip;
                let visivel = match fundo {
                    Some(rect) => match self.clip_rect(rect) {
                        Some(dentro) => {
                            self.clip = Some(dentro);
                            true
                        }
                        None => false,
                    },
                    None => true,
                };
                let desenhou = match visivel {
                    true => self.draw_text(&text, x, y),
                    false => Ok(true),
                };
                self.clip = recorte_anterior;
                match desenhou? {
                    true => {}
                    // Sem fonte, o texto continua indo só para o relatório: é o que permite
                    // saber que o jogo *quer* escrever mesmo quando não há com o quê.
                    false => {
                        // A comparação só roda enquanto há espaço; cheia, a lista custa um
                        // teste de tamanho por chamada.
                        if self.pending_text.len() < MAX_TEXT && !self.pending_text.contains(&text)
                        {
                            self.pending_text.push(text);
                        }
                    }
                }
                SUCCESS
            }
            // Sem efeito visível para nós: o framebuffer já está sempre atualizado.
            // void IDISPLAY_GetClipRect(IDisplay *p, AEERect *prc)
            //
            // O recorte é ignorado no desenho, então o retângulo corrente é a tela inteira —
            // que é o que o BREW devolve quando ninguém apertou o recorte. O Pac-Mania lê isto
            // para guardar e restaurar depois.
            "GetClipRect" => {
                let out = self.cpu.read_reg(Reg::R1);
                if out != 0 {
                    let rect = self.clip.unwrap_or_else(|| {
                        let screen = self.screen();
                        Rect {
                            x: 0,
                            y: 0,
                            width: screen.width() as i16,
                            height: screen.height() as i16,
                        }
                    });
                    let mut bytes = [0u8; 8];
                    bytes[0..2].copy_from_slice(&rect.x.to_le_bytes());
                    bytes[2..4].copy_from_slice(&rect.y.to_le_bytes());
                    bytes[4..6].copy_from_slice(&rect.width.to_le_bytes());
                    bytes[6..8].copy_from_slice(&rect.height.to_le_bytes());
                    self.cpu.write_mem(out, &bytes)?;
                }
                SUCCESS
            }
            // int IDISPLAY_GetFontMetrics(IDisplay *p, AEEFont font, int *pnAscent,
            //                              int *pnDescent)
            //
            // Devolve a altura da fonte. Ainda não desenhamos texto, mas os jogos medem antes
            // de posicionar: sem números aqui, o Pac-Mania nem chega a montar a tela. Os
            // valores são os de uma fonte de tela pequena, coerentes entre si.
            // int IDISPLAY_MeasureTextEx(IDisplay *, AEEFont, const AECHAR *pcText,
            //                             int nChars, int nMaxWidth, int *pnFits)
            //
            // Devolve a largura em pixels e, em `pnFits`, quantos caracteres cabem em
            // `nMaxWidth`. Como ainda não desenhamos texto, a largura sai de um avanço fixo por
            // caractere, coerente com a altura que o `GetFontMetrics` informa. É medida
            // aproximada de propósito: serve para o jogo centralizar e quebrar linha, e um
            // número plausível o deixa seguir — sem nenhum, o Pac-Mania para na escolha do
            // idioma.
            "MeasureTextEx" => {
                let text = self.read_aechar_units(self.cpu.read_reg(Reg::R2))?;
                let chars = self.cpu.read_reg(Reg::R3) as i32;
                let count = match chars < 0 {
                    true => text.len(),
                    false => (chars as usize).min(text.len()),
                };
                let max_width = self.stack_arg(0)? as i32;
                // Com fonte, a largura é medida caractere a caractere até estourar o limite;
                // sem ela sobra a largura fixa, que é chute honesto mas chute.
                let (fits, largura) = match &self.font {
                    Some(fonte) => {
                        let mut cabem = 0;
                        let mut largura = 0;
                        for fim in 1..=count {
                            let trecho: String = text[..fim]
                                .iter()
                                .filter_map(|u| char::from_u32(u32::from(*u)))
                                .collect();
                            let candidata = fonte.width(&trecho, FONT_SIZE) as i32;
                            if max_width >= 0 && candidata > max_width {
                                break;
                            }
                            (cabem, largura) = (fim, candidata);
                        }
                        (cabem, largura as u32)
                    }
                    None => {
                        let cabem = match max_width < 0 {
                            true => count,
                            false => count.min((max_width / FONT_ADVANCE).max(0) as usize),
                        };
                        (cabem, (cabem as i32 * FONT_ADVANCE) as u32)
                    }
                };
                let out = self.stack_arg(1)?;
                if out != 0 {
                    self.cpu.write_u32(out, fits as u32)?;
                }
                largura
            }
            "GetFontMetrics" => {
                // Com fonte de verdade, a medida é dela: um menu que centraliza pela altura da
                // linha fica torto se a altura for chute.
                let (ascent, descent) = match &self.font {
                    Some(fonte) => (fonte.ascent(FONT_SIZE), fonte.descent(FONT_SIZE)),
                    None => (FONT_ASCENT, FONT_DESCENT),
                };
                self.write_at(self.cpu.read_reg(Reg::R2), ascent)?;
                self.write_at(self.cpu.read_reg(Reg::R3), descent)?;
                ascent + descent
            }
            // int IDISPLAY_SetClipRect(IDisplay *p, const AEERect *prc)
            //
            // Ponteiro nulo volta ao recorte cheio, que é o que o BREW define. Ignorar esta
            // chamada custava caro: o Pac-Mania desenha a folha de fontes inteira e conta com
            // o recorte para que só a letra apareça — sem ele, a folha toda ia para a tela.
            "SetClipRect" => {
                self.clip = self.read_rect(self.cpu.read_reg(Reg::R1))?;
                SUCCESS
            }
            // int IDISPLAY_Clone(IDisplay *po, IDisplay **ppNew)
            //
            // Uma cópia do objeto de tela, para o app desenhar noutro estado sem mexer no
            // corrente. A tela é uma só, então o que sai daqui é o mesmo objeto com uma
            // referência a mais — é o que o BREW faz quando o dispositivo tem uma tela.
            "Clone" => {
                let out = self.cpu.read_reg(Reg::R1);
                if out == 0 {
                    return Ok(Some(EBADPARM));
                }
                // Um objeto novo, não o mesmo com uma referência a mais: o app solta a cópia
                // quando termina com ela, e devolver o original faria essa soltura derrubar a
                // tela que ele ainda usa. O estado de desenho é da `Machine`, não do objeto,
                // então dois objetos apontam para a mesma tela sem se atrapalharem.
                let clone = self.new_object(Interface::Display)?;
                if clone == 0 {
                    return Ok(Some(ENOMEMORY));
                }
                self.cpu.write_u32(out, clone)?;
                SUCCESS
            }
            "Update" | "UpdateEx" => {
                self.guarda_quadro_do_update()?;
                SUCCESS
            }
            // int IDISPLAY_SetPrefs(IDisplay *, const char *pchPrefs, int cPrefs).
            // O Undercover para em 0x119a8 pedindo os três bytes de "a:0":
            // ocultar os anunciadores. Nosso framebuffer já ocupa a tela inteira,
            // então esse pedido está satisfeito sem redesenho nem troca de tamanho.
            // Outras preferências ainda não são implementadas; devolver o erro
            // BREW preserva a geometria real em vez de confirmar uma mudança fictícia.
            "SetPrefs" => {
                let length = self.cpu.read_reg(Reg::R2) as i32;
                if !(0..=MAX_STRING as i32).contains(&length) {
                    return Ok(Some(EBADPARM));
                }
                if length == 0 {
                    SUCCESS
                } else {
                    let address = self.cpu.read_reg(Reg::R1);
                    if address == 0 {
                        return Ok(Some(EBADPARM));
                    }
                    let mut prefs = vec![0; length as usize];
                    self.cpu.read_mem(address, &mut prefs)?;
                    if prefs == b"a:0" {
                        SUCCESS
                    } else {
                        EUNSUPPORTED
                    }
                }
            }
            "SetFont" | "SetAnnunciators" | "Backlight" => SUCCESS,
            _ => return Ok(None),
        };
        Ok(Some(result))
    }

    /// Superfície onde o `IDisplay` desenha.
    pub(super) fn target(&mut self) -> Result<u32, CpuError> {
        if self.display_target == 0 {
            return self.device_bitmap();
        }
        Ok(self.display_target)
    }

    /// `IGraphics` — a API 2D do BREW, desenhada sobre a mesma superfície do `IDisplay`.
    ///
    /// O estado (cor de traço, cor de preenchimento, se preenche ou não, translação) fica no
    /// host; as primitivas viram operações no framebuffer.
    pub(super) fn graphics_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        // **O `IGraphics` tem o seu recorte, e o `IDisplay` o dele.** Os dois dividiam o `clip`:
        // o Zuma's Revenge põe no `IGraphics` o recorte da tela (640 de largura) e depois
        // desenha por `IImage` o fundo de 726x360 numa superfície dele. O recorte vazado cortava
        // o blit em 640, o motor do jogo dimensionava a superfície por ele, e a cópia para a
        // textura, que anda 726 pixels por linha, embaralhava o fundo em listras.
        let recorte_do_display = std::mem::replace(&mut self.clip, self.clip_graficos);
        let resultado = self.graphics_call_no_recorte(slot);
        self.clip_graficos = std::mem::replace(&mut self.clip, recorte_do_display);
        resultado
    }

    fn graphics_call_no_recorte(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        // **O quadro do OpenGL vem antes do 2D.** Um HUD desenhado por cima de uma tela que
        // ainda não recebeu a cena apagaria a cena — ver [`Machine::materializa_quadro_gl`].
        self.materializa_quadro_gl();
        let Some(name) = Interface::Graphics.method(slot) else {
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
            "Release" => self.objects.release(this),
            // As cores chegam como três ou quatro `uint8` soltos, não como RGBVAL.
            "SetBackground" => {
                let previous = self.graphics.background;
                self.graphics.background = Rgb {
                    r: a1 as u8,
                    g: a2 as u8,
                    b: a3 as u8,
                };
                to_rgbval(previous)
            }
            "SetColor" => {
                let previous = self.graphics.stroke;
                self.graphics.stroke = Rgb {
                    r: a1 as u8,
                    g: a2 as u8,
                    b: a3 as u8,
                };
                to_rgbval(previous)
            }
            "SetFillColor" => {
                let previous = self.graphics.fill;
                self.graphics.fill = Rgb {
                    r: a1 as u8,
                    g: a2 as u8,
                    b: a3 as u8,
                };
                to_rgbval(previous)
            }
            "SetFillMode" => {
                let previous = self.graphics.fill_mode;
                self.graphics.fill_mode = a1 != 0;
                u32::from(previous)
            }
            "GetFillMode" => u32::from(self.graphics.fill_mode),
            "SetPointSize" => {
                let previous = self.graphics.point_size;
                self.graphics.point_size = a1 as u8;
                previous as u32
            }
            "GetPointSize" => self.graphics.point_size as u32,
            "GetColorDepth" => COLOR_DEPTH as u32,
            "Translate" => {
                self.graphics.origin = (a1 as i16 as i32, a2 as i16 as i32);
                SUCCESS
            }
            // int DrawPoint(IGraphics *, AEEPoint *) — AEEPoint é { int16 x, y }.
            "DrawPoint" => {
                let (x, y) = self.read_point(a1)?;
                let (x, y) = self.translated(x, y);
                let color = self.graphics.stroke;
                self.with_target_or_foreign(x, y, 1, 1, AEE_RO_TRANSPARENT, |fb, ox, oy| {
                    fb.set_pixel(x + ox, y + oy, color)
                })?;
                SUCCESS
            }
            // int DrawLine(IGraphics *, AEELine *) — AEELine é { int16 sx, sy, ex, ey }.
            "DrawLine" => {
                let mut bytes = [0u8; 8];
                self.cpu.read_mem(a1, &mut bytes)?;
                let read = |i: usize| i16::from_le_bytes([bytes[i], bytes[i + 1]]) as i32;
                let (sx, sy) = self.translated(read(0), read(2));
                let (ex, ey) = self.translated(read(4), read(6));
                let color = self.graphics.stroke;
                let x = sx.min(ex);
                let y = sy.min(ey);
                let largura = (sx.max(ex) - x + 1) as u32;
                let altura = (sy.max(ey) - y + 1) as u32;
                self.with_target_or_foreign(
                    x,
                    y,
                    largura,
                    altura,
                    AEE_RO_TRANSPARENT,
                    |fb, ox, oy| fb.draw_line(sx + ox, sy + oy, ex + ox, ey + oy, color),
                )?;
                SUCCESS
            }
            "DrawRoundRectangle" | "DrawEllipse" | "DrawEllipticalArc" | "DrawArc" | "DrawPie" => {
                let Some(rect) = self.read_rect(a1)? else {
                    return Ok(Some(14));
                };
                // AEEEllipse contém centro e semieixos; AEERect contém canto e tamanho.
                let (mut x, mut y, mut w, mut h) = (
                    i32::from(rect.x),
                    i32::from(rect.y),
                    i32::from(rect.width),
                    i32::from(rect.height),
                );
                let (mut start_angle, mut sweep, mut pie) = (0, 360, false);
                if name == "DrawEllipse" {
                    if w < 0 || h < 0 {
                        return Ok(Some(14));
                    }
                    x -= w;
                    y -= h;
                    w = 2 * w + 1;
                    h = 2 * h + 1;
                } else if name == "DrawArc" || name == "DrawPie" {
                    let radius = w;
                    if radius < 0 {
                        return Ok(Some(14));
                    }
                    start_angle = h;
                    let bytes = self.read_bytes(a1 + 8, 2)?;
                    sweep = i32::from(i16::from_le_bytes([bytes[0], bytes[1]]));
                    x -= radius;
                    y -= radius;
                    w = 2 * radius + 1;
                    h = w;
                    pie = name == "DrawPie";
                } else if name == "DrawEllipticalArc" {
                    start_angle = i32::from(a2 as i16);
                    sweep = i32::from(a3 as i16);
                }
                if w < 0 || h < 0 || !(0..=360).contains(&sweep) {
                    return Ok(Some(14));
                }
                if w == 0 || h == 0 || sweep == 0 {
                    return Ok(Some(SUCCESS));
                }
                let (aw, ah) = if name == "DrawRoundRectangle" {
                    (i32::from(a2 as i16), i32::from(a3 as i16))
                } else {
                    (w, h)
                };
                if aw < 0 || ah < 0 {
                    return Ok(Some(14));
                }
                let (x, y) = self.translated(x, y);
                let (aw, ah) = (aw.min(w), ah.min(h));
                // Um semieixo int16 pode produzir diâmetro 65535; i128 evita overflow.
                let (aw2, ah2) = (i128::from(aw).pow(2), i128::from(ah).pow(2));
                let ellipse_inside = |px: i32, py: i32| {
                    if px < 0 || py < 0 || px >= w || py >= h {
                        return false;
                    }
                    if aw == 0 || ah == 0 {
                        return true;
                    }
                    let (cx, cy) = (2 * px + 1, 2 * py + 1);
                    let dx = i128::from((aw - cx).max(cx - (2 * w - aw)).max(0));
                    let dy = i128::from((ah - cy).max(cy - (2 * h - ah)).max(0));
                    dx * dx * ah2 + dy * dy * aw2 <= aw2 * ah2
                };
                let inside = |px: i32, py: i32| {
                    if !ellipse_inside(px, py) {
                        return false;
                    }
                    if sweep == 360 {
                        return true;
                    }
                    let (dx, dy) = (2 * px + 1 - w, h - 2 * py - 1);
                    if dx == 0 && dy == 0 {
                        return true;
                    }
                    let angle = f64::from(dy).atan2(f64::from(dx)).to_degrees();
                    (angle - f64::from(start_angle)).rem_euclid(360.0) <= f64::from(sweep)
                };
                let (mut x0, mut y0, mut x1, mut y1) = (x, y, x + w, y + h);
                let target = self.target()?;
                if let Some(fb) = self.bitmaps.get(&target) {
                    x0 = x0.max(0);
                    y0 = y0.max(0);
                    x1 = x1.min(fb.width() as i32);
                    y1 = y1.min(fb.height() as i32);
                }
                if let Some(clip) = self.clip {
                    x0 = x0.max(i32::from(clip.x));
                    y0 = y0.max(i32::from(clip.y));
                    x1 = x1.min(i32::from(clip.x) + i32::from(clip.width));
                    y1 = y1.min(i32::from(clip.y) + i32::from(clip.height));
                }
                if x0 >= x1 || y0 >= y1 {
                    return Ok(Some(SUCCESS));
                }
                let (width, height) = ((x1 - x0) as u32, (y1 - y0) as u32);
                // Um destino estrangeiro pede superfície temporária. Recusar área
                // excessiva antes da alocação mantém o mesmo teto das leituras do guest.
                if u64::from(width) * u64::from(height) * 2 > u64::from(TETO_DA_LEITURA) {
                    return Ok(Some(2)); // ENOMEMORY do SDK móvel.
                }
                let (stroke, fill, filled) = (
                    self.graphics.stroke,
                    self.graphics.fill,
                    self.graphics.fill_mode,
                );
                self.with_target_or_foreign(
                    x0,
                    y0,
                    width,
                    height,
                    AEE_RO_TRANSPARENT,
                    |fb, ox, oy| {
                        for py in (y0 + oy).max(0)..(y1 + oy).min(fb.height() as i32) {
                            for px in (x0 + ox).max(0)..(x1 + ox).min(fb.width() as i32) {
                                let (lx, ly) = (px - ox - x, py - oy - y);
                                if !inside(lx, ly) {
                                    continue;
                                }
                                // A borda depende da forma original; o clip não deve criar
                                // um traço novo no meio do preenchimento.
                                let border = if pie {
                                    !inside(lx - 1, ly)
                                        || !inside(lx + 1, ly)
                                        || !inside(lx, ly - 1)
                                        || !inside(lx, ly + 1)
                                } else {
                                    !ellipse_inside(lx - 1, ly)
                                        || !ellipse_inside(lx + 1, ly)
                                        || !ellipse_inside(lx, ly - 1)
                                        || !ellipse_inside(lx, ly + 1)
                                };
                                if border {
                                    fb.set_pixel(px, py, stroke);
                                } else if filled && name != "DrawArc" {
                                    fb.set_pixel(px, py, fill);
                                }
                            }
                        }
                    },
                )?;
                SUCCESS
            }
            "DrawRect" | "ClearRect" => {
                let Some(rect) = self.read_rect(a1)? else {
                    return Ok(Some(EBADPARM));
                };
                let rect = self.translated_rect(rect);
                let (stroke, fill, filled) = (
                    self.graphics.stroke,
                    self.graphics.fill,
                    self.graphics.fill_mode,
                );
                // `ClearRect` pinta com a cor de fundo; `DrawRect` respeita o modo de
                // preenchimento e sempre desenha a borda.
                if name == "ClearRect" {
                    let background = self.graphics.background;
                    self.with_target_or_foreign(
                        rect.x as i32,
                        rect.y as i32,
                        rect.width.max(0) as u32,
                        rect.height.max(0) as u32,
                        AEE_RO_COPY,
                        |fb, ox, oy| {
                            fb.fill_rect(
                                Rect {
                                    x: rect.x + ox as i16,
                                    y: rect.y + oy as i16,
                                    ..rect
                                },
                                background,
                            )
                        },
                    )?;
                } else {
                    self.with_target_or_foreign(
                        rect.x as i32,
                        rect.y as i32,
                        rect.width.max(0) as u32,
                        rect.height.max(0) as u32,
                        AEE_RO_TRANSPARENT,
                        |fb, ox, oy| {
                            let rect = Rect {
                                x: rect.x + ox as i16,
                                y: rect.y + oy as i16,
                                ..rect
                            };
                            if filled {
                                fb.fill_rect(rect, fill);
                            }
                            fb.draw_frame(rect, stroke);
                        },
                    )?;
                }
                SUCCESS
            }
            // AEECircle é { int16 cx, cy, r }.
            "DrawCircle" => {
                let mut bytes = [0u8; 6];
                self.cpu.read_mem(a1, &mut bytes)?;
                let read = |i: usize| i16::from_le_bytes([bytes[i], bytes[i + 1]]) as i32;
                let (cx, cy) = self.translated(read(0), read(2));
                let radius = read(4);
                let (stroke, fill, filled) = (
                    self.graphics.stroke,
                    self.graphics.fill,
                    self.graphics.fill_mode,
                );
                let tamanho = (radius.max(0) * 2 + 1) as u32;
                self.with_target_or_foreign(
                    cx - radius,
                    cy - radius,
                    tamanho,
                    tamanho,
                    AEE_RO_TRANSPARENT,
                    |fb, ox, oy| {
                        if filled {
                            fb.fill_circle(cx + ox, cy + oy, radius, fill);
                        }
                        fb.draw_circle(cx + ox, cy + oy, radius, stroke);
                    },
                )?;
                SUCCESS
            }
            // AEETriangle é { int16 x0, y0, x1, y1, x2, y2 }.
            "DrawTriangle" => {
                let mut bytes = [0u8; 12];
                self.cpu.read_mem(a1, &mut bytes)?;
                let read = |i: usize| i16::from_le_bytes([bytes[i], bytes[i + 1]]) as i32;
                let points: Vec<(i32, i32)> = (0..3)
                    .map(|i| self.translated(read(i * 4), read(i * 4 + 2)))
                    .collect();
                self.draw_shape(&points, true)?;
                SUCCESS
            }
            // AEEPolygon e AEEPolyline são { int16 len; AEEPoint *points }.
            "DrawPolygon" | "DrawPolyline" => {
                let count = self.cpu.read_u32(a1)? as u16 as usize;
                let array = self.cpu.read_u32(a1 + 4)?;
                let mut points = Vec::with_capacity(count);
                for i in 0..count.min(MAX_POLYGON_POINTS) {
                    let (x, y) = self.read_point(array + i as u32 * 4)?;
                    points.push(self.translated(x, y));
                }
                self.draw_shape(&points, name == "DrawPolygon")?;
                SUCCESS
            }
            "ClearViewport" => {
                let background = self.graphics.background;
                let rect = Rect {
                    x: 0,
                    y: 0,
                    width: self.screen().width() as i16,
                    height: self.screen().height() as i16,
                };
                self.with_target(|fb| fb.fill_rect(rect, background))?;
                SUCCESS
            }
            "SetDestination" => {
                self.display_target = a1;
                SUCCESS
            }
            // Com contagem, pela mesma razão do `IDisplay` — ver o `GetDestination` de lá.
            "GetDestination" => {
                let alvo = self.target()?;
                if alvo != 0 {
                    self.objects.add_ref(alvo);
                }
                alvo
            }
            "SetClip" => {
                // O Bejeweled Twist passa uma estrutura de 16 bytes: um campo de estado seguido
                // por um `AEERect`. Ler desde o início transforma esse estado em coordenada e
                // corrompe o fluxo ainda na carga.
                self.clip = if a1 == 0 || self.cpu.read_u32(a1)? & 0xff == 0 {
                    None
                } else if self.cpu.read_u32(a1)? & 0xff == 1 {
                    self.read_rect(a1 + 4)?
                } else {
                    return Ok(Some(0));
                };
                1
            }
            // Sem efeito para nós: já desenhamos direto na superfície.
            "Update" | "EnableDoubleBuffer" | "SetPaintMode" | "GetClip" | "SetViewport"
            | "SetAlgorithmHint" | "SetStrokeStyle" | "Pan" => SUCCESS,
            _ => return Ok(None),
        };
        Ok(Some(result))
    }

    /// Desenha um polígono fechado (ou uma polilinha aberta) com o estado atual.
    pub(super) fn draw_shape(
        &mut self,
        points: &[(i32, i32)],
        closed: bool,
    ) -> Result<(), CpuError> {
        if points.is_empty() {
            return Ok(());
        }
        let (stroke, fill, filled) = (
            self.graphics.stroke,
            self.graphics.fill,
            self.graphics.fill_mode && closed,
        );
        let points = points.to_vec();
        let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for &(x, y) in &points {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        self.with_target_or_foreign(
            x0,
            y0,
            (x1 - x0 + 1) as u32,
            (y1 - y0 + 1) as u32,
            AEE_RO_TRANSPARENT,
            move |fb, ox, oy| {
                let points: Vec<_> = points.iter().map(|&(x, y)| (x + ox, y + oy)).collect();
                if filled {
                    fb.fill_polygon(&points, fill);
                }
                let last = if closed {
                    points.len()
                } else {
                    points.len() - 1
                };
                for i in 0..last {
                    let (x0, y0) = points[i];
                    let (x1, y1) = points[(i + 1) % points.len()];
                    fb.draw_line(x0, y0, x1, y1, stroke);
                }
            },
        )
    }

    /// Executa uma operação de desenho na superfície corrente.
    pub(super) fn with_target(
        &mut self,
        draw: impl FnOnce(&mut Framebuffer),
    ) -> Result<(), CpuError> {
        let target = self.target()?;
        if let Some(fb) = self.bitmaps.get_mut(&target) {
            draw(fb);
        }
        Ok(())
    }

    /// Desenha direto quando o alvo é nosso; quando é uma superfície do jogo, enfileira uma
    /// superfície temporária para o `BltIn` estrangeiro compor na fronteira segura.
    pub(super) fn with_target_or_foreign(
        &mut self,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        rop: u32,
        draw: impl FnOnce(&mut Framebuffer, i32, i32),
    ) -> Result<(), CpuError> {
        if width == 0 || height == 0 {
            return Ok(());
        }
        let target = self.target()?;
        if let Some(fb) = self.bitmaps.get_mut(&target) {
            if let Some(clip) = self.clip {
                fb.com_recorte(clip, |fb| draw(fb, 0, 0));
            } else {
                draw(fb, 0, 0);
            }
            return Ok(());
        }

        let (mut x, mut y, mut width, mut height) = (x, y, width, height);
        if let Some(clip) = self.clip {
            let x0 = x.max(clip.x as i32);
            let y0 = y.max(clip.y as i32);
            let x1 = (x + width as i32).min(clip.x as i32 + clip.width as i32);
            let y1 = (y + height as i32).min(clip.y as i32 + clip.height as i32);
            if x0 >= x1 || y0 >= y1 {
                return Ok(());
            }
            x = x0;
            y = y0;
            width = (x1 - x0) as u32;
            height = (y1 - y0) as u32;
        }

        let source = self.new_object(Interface::Bitmap)?;
        if source == 0 {
            return Ok(());
        }
        let mut fb = Framebuffer::new(width, height);
        fb.fill_rect_native(
            Rect {
                x: 0,
                y: 0,
                width: width.min(i16::MAX as u32) as i16,
                height: height.min(i16::MAX as u32) as i16,
            },
            TRANSPARENT_KEY,
        );
        draw(&mut fb, -x, -y);
        self.bitmaps.insert(source, fb);
        self.transparency.insert(source, TRANSPARENT_KEY);
        self.pending_surface_blits.push(PendingSurfaceBlit {
            source,
            target,
            x,
            y,
            width,
            height,
            rop,
        });
        Ok(())
    }

    /// Lê um `AEEPoint` — dois `int16`.
    pub(super) fn read_point(&self, addr: u32) -> Result<(i32, i32), CpuError> {
        let mut bytes = [0u8; 4];
        self.cpu.read_mem(addr, &mut bytes)?;
        Ok((
            i16::from_le_bytes([bytes[0], bytes[1]]) as i32,
            i16::from_le_bytes([bytes[2], bytes[3]]) as i32,
        ))
    }

    /// Aplica a translação corrente do `IGraphics`.
    pub(super) fn translated(&self, x: i32, y: i32) -> (i32, i32) {
        (x + self.graphics.origin.0, y + self.graphics.origin.1)
    }

    pub(super) fn translated_rect(&self, rect: Rect) -> Rect {
        Rect {
            x: rect.x + self.graphics.origin.0 as i16,
            y: rect.y + self.graphics.origin.1 as i16,
            ..rect
        }
    }

    /// Desenha `text` na superfície corrente. `false` quando não há fonte para desenhá-lo.
    ///
    /// A cor é a do `CLR_USER_TEXT`, que é o que o `IDISPLAY_SetColor` ajusta, e o recorte vale
    /// aqui como em qualquer outro desenho.
    pub(super) fn draw_text(&mut self, text: &str, x: i32, y: i32) -> Result<bool, CpuError> {
        // O registro é do que **foi** desenhado, e não do que faltou: um jogo que mostra várias
        // telas de aviso ao longo da execução precisa que o relatório diga qual delas veio quando.
        if text.chars().any(|c| !c.is_whitespace()) {
            if self.textos_desenhados.len() >= MAX_TEXTOS_DESENHADOS {
                self.textos_desenhados.pop_front();
            }
            self.textos_desenhados
                .push_back((self.clock_ms(), x, y, text.to_string()));
        }
        let Some(fonte) = self.font.as_ref() else {
            return Ok(false);
        };
        let glifos = fonte.layout(text, FONT_SIZE);
        if glifos.is_empty() {
            return Ok(true);
        }
        let cor = self
            .colors
            .get(CLR_USER_TEXT)
            .copied()
            .unwrap_or(Rgb::BLACK);
        self.escreve(text, x, y, cor)
    }

    /// O texto desenhado, em ordem, com o instante virtual e a posição.
    pub fn drawn_text(&self) -> impl Iterator<Item = &(u32, i32, i32, String)> {
        self.textos_desenhados.iter()
    }

    /// Escreve com a fonte carregada, numa cor dada. É o miolo do [`Machine::draw_text`],
    /// separado porque o widget traz a cor dele na propriedade e não usa a da paleta.
    pub(super) fn escreve(
        &mut self,
        text: &str,
        x: i32,
        y: i32,
        cor: Rgb,
    ) -> Result<bool, CpuError> {
        let Some(fonte) = self.font.as_ref() else {
            return Ok(false);
        };
        let glifos = fonte.layout(text, FONT_SIZE);
        if glifos.is_empty() {
            return Ok(true);
        }
        let nativo = cor.to_rgb565();
        let recorte = self.clip;
        let target = self.target()?;
        let Some(surface) = self.bitmaps.get_mut(&target) else {
            return Ok(true);
        };
        for glifo in glifos {
            for linha in 0..glifo.height {
                for coluna in 0..glifo.width {
                    // Meio-tom não existe numa superfície sem canal alfa: ou a letra cobre o
                    // pixel, ou não cobre. Metade é o corte que deixa a borda parecida com a
                    // do console, que também não mistura.
                    if glifo.coverage[(linha * glifo.width + coluna) as usize] < 128 {
                        continue;
                    }
                    let (px, py) = (x + glifo.x + coluna as i32, y + glifo.y + linha as i32);
                    if let Some(clip) = recorte {
                        let dentro = px >= clip.x as i32
                            && py >= clip.y as i32
                            && px < clip.x as i32 + clip.width as i32
                            && py < clip.y as i32 + clip.height as i32;
                        if !dentro {
                            continue;
                        }
                    }
                    surface.set_pixel_native(px, py, nativo);
                }
            }
        }
        Ok(true)
    }

    /// A tela, como está agora.
    pub fn screen(&self) -> &Framebuffer {
        self.bitmaps
            .get(&self.device_bitmap)
            .unwrap_or(&self.screen)
    }

    /// Põe na tela uma imagem RGB565 do tamanho dela, antes de o applet desenhar.
    ///
    /// É o papel do firmware entre um applet e outro: a tela do aparelho não é apagada quando
    /// um applet sai e outro entra, e o que estava nela fica até alguém desenhar por cima. Devolve
    /// `false`, sem tocar em nada, quando a imagem não tem o tamanho da tela.
    pub fn pinta_tela_rgb565(&mut self, bytes: &[u8]) -> bool {
        let tela = match self.bitmaps.get_mut(&self.device_bitmap) {
            Some(tela) => tela,
            None => &mut self.screen,
        };
        if bytes.len() != tela.width() as usize * tela.height() as usize * 2 {
            return false;
        }
        tela.load_rgb565_bytes(bytes);
        true
    }

    /// Textos que o jogo pediu para desenhar mas que ainda não sabemos rasterizar.
    pub fn pending_text(&self) -> &[String] {
        &self.pending_text
    }
}

/// Alinhamentos do `IDISPLAY_DrawText`, de `AEEDisp.h`. Os valores foram conferidos no uso da
/// Z-Wheel (`0x30434`): o rótulo de uma linha pede `0x8020 | 0x200` — transparente, centro,
/// meio —, e o de duas linhas troca o `0x200` por `0x400` na primeira e `0x100` na segunda,
/// que é embaixo e em cima. Batem com o `IDF_RECT_FRAME`/`FILL` de `1` e `2`.
const IDF_ALIGN_LEFT: u32 = 0x10;
const IDF_ALIGN_CENTER: u32 = 0x20;
const IDF_ALIGN_RIGHT: u32 = 0x40;
const IDF_ALIGN_TOP: u32 = 0x100;
const IDF_ALIGN_MIDDLE: u32 = 0x200;
const IDF_ALIGN_BOTTOM: u32 = 0x400;

/// Onde o texto começa, dado o retângulo e os alinhamentos. Um eixo sem alinhamento fica com a
/// coordenada que o jogo passou.
fn posicao_do_texto(
    (x, y): (i32, i32),
    rect: Rect,
    flags: u32,
    (largura, altura): (i32, i32),
) -> (i32, i32) {
    let (rx, ry, rw, rh) = (
        i32::from(rect.x),
        i32::from(rect.y),
        i32::from(rect.width),
        i32::from(rect.height),
    );
    let x = match flags {
        f if f & IDF_ALIGN_CENTER != 0 => rx + (rw - largura) / 2,
        f if f & IDF_ALIGN_RIGHT != 0 => rx + rw - largura,
        f if f & IDF_ALIGN_LEFT != 0 => rx,
        _ => x,
    };
    let y = match flags {
        f if f & IDF_ALIGN_MIDDLE != 0 => ry + (rh - altura) / 2,
        f if f & IDF_ALIGN_BOTTOM != 0 => ry + rh - altura,
        f if f & IDF_ALIGN_TOP != 0 => ry,
        _ => y,
    };
    (x, y)
}

#[cfg(test)]
mod testes_do_texto {
    use super::*;

    #[test]
    fn o_rotulo_da_barra_de_abas_fica_no_centro_do_retangulo() {
        let rect = Rect {
            x: 0,
            y: 0,
            width: 214,
            height: 37,
        };
        assert_eq!(posicao_do_texto((0, 0), rect, 0x8220, (50, 17)), (82, 10));
    }

    #[test]
    fn sem_alinhamento_vale_a_coordenada_passada() {
        let rect = Rect {
            x: 0,
            y: 0,
            width: 640,
            height: 480,
        };
        assert_eq!(
            posicao_do_texto((48, 272), rect, 0x8000, (90, 17)),
            (48, 272)
        );
    }
}
