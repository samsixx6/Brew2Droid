# QCELP portátil

Decoder derivado do FFmpeg 8.0, commit
`140fd653aed8cad774f991ba083e2d01e86420c7` (tag `n8.0`). Licença
LGPL-2.1-or-later, texto em `COPYING.LGPLv2.1`. Não usa o código de referência
Qualcomm/Lucent nem bibliotecas proprietárias.

`upstream/` preserva os arquivos consultados de `libavcodec/` e
`libavutil/float_dsp.c`. Origem:
https://github.com/FFmpeg/FFmpeg/tree/140fd653aed8cad774f991ba083e2d01e86420c7

`qcelp.c` mantém o algoritmo do `qcelpdec.c`; retira a inscrição no registro de
codecs e a configuração de canais do libavcodec. A API limitada recebe um pacote
validado e escreve 160 floats, sem alocar por quadro. O estado é privado a uma
instância; `new/free` rodam fora do callback nativo. O leitor de bits limitado
substitui `get_bits.h` e exclui o byte de taxa da extensão acessível.

`support.h` contém apenas as primitivas utilizadas de `acelp_vectors.c`,
`acelp_filters.c`, `celp_filters.c` e `lsp.c`, com pequenos tipos de adaptação.
O produto escalar é um laço escalar. Em `lsp2polyf`, o índice equivale ao original
sem criar um ponteiro anterior ao início do array. A compilação desliga contração
FMA para permitir comparação numérica com o vetor FATE.

Copyrights dos algoritmos: Reynaldo H. Verdejo Pinochet (2007) e Vladimir
Voroshilov (2008); os avisos integrais acompanham as fontes originais. A saída do
produto inclui o aviso e a licença por `ferramentas/avisos_de_terceiros.py`.

Framing QCP é implementado no Rust por RFC 3625; CMF usa o rascunho público
Qualcomm `draft-atarius-cmf-00`. Estes parsers não executam demuxers do FFmpeg.
O teste privado usa `https://fate-suite.ffmpeg.org/qcp/0036580847.QCP` e seu PCM
de referência, sem incorporar áudio de jogos ou vetores externos ao repositório.
