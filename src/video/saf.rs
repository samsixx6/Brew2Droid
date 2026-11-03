//! SAF de 8 bits usado pelos recursos BREW coreanos.
//!
//! A observação dos arquivos e os cabeçalhos SAF/SMLZ de NeoMtel identificam o
//! contêiner e a tabela externa. A reconstrução abaixo é própria: reinsere essa
//! descrição no bloco DEFLATE da RFC 1951 e entrega a descompressão ao flate2.
//! Não incorpora o decodificador proprietário. Quadros independentes com
//! posicionamentos simples e espelhamento horizontal são compostos integralmente.
//! Outras transformações, efeitos e alfa global continuam recusados; nenhum
//! comando de desenho é descartado.

use super::icon::{Image, ImageError};
use flate2::{Decompress, FlushDecompress, Status};

type Result<T> = std::result::Result<T, ImageError>;

fn invalid() -> ImageError {
    ImageError::Malformed("SAF inválido ou não suportado")
}

fn take<'a>(data: &'a [u8], at: &mut usize, count: usize) -> Result<&'a [u8]> {
    let end = at.checked_add(count).ok_or_else(invalid)?;
    let value = data.get(*at..end).ok_or_else(invalid)?;
    *at = end;
    Ok(value)
}

fn be16(bytes: &[u8]) -> usize {
    usize::from(u16::from_be_bytes([bytes[0], bytes[1]]))
}

#[derive(Default)]
struct Header {
    width: usize,
    height: usize,
    objects: usize,
    frames: usize,
    palette: Option<Vec<[u8; 3]>>,
}

const MAX_OBJECTS: usize = 128;
const MAX_OBJECT_PIXELS: usize = MAX_OBJECTS * 255 * 255;
const MAX_FRAMES: usize = 128;
const MAX_FRAME_PIXELS: usize = MAX_FRAMES * 255 * 255;

struct Object {
    width: usize,
    height: usize,
    transparent: u8,
    pixels: Vec<u8>,
}

// O esquema global 2 não seleciona outro codec para objetos MLZ (tipo 1).
// A referência usa o tipo de cada objeto; o bit 0x80 da paleta também não
// altera o tipo da paleta. Outros codecs e paletas não RGB são recusados.
fn header(bytes: &[u8]) -> Result<Header> {
    let mut at = 0;
    let mut result = Header::default();
    let mut dimensions = false;
    let mut object_info = false;
    while at < bytes.len() {
        let prefix = take(bytes, &mut at, 2)?;
        let part = take(bytes, &mut at, usize::from(prefix[1]))?;
        match prefix[0] {
            1 if !dimensions && part.len() >= 6 && part[2] == 8 && matches!(part[3], 2 | 4) => {
                result.width = usize::from(part[0]);
                result.height = usize::from(part[1]);
                dimensions = true;
            }
            // O comprimento da paleta explícita e seus bytes ficam depois do
            // TLV de dois bytes, ainda dentro do grupo de cabeçalhos. Tratá-los
            // como novos comandos desalinhará o próximo cabeçalho de quadros.
            2 if !object_info && part.len() == 2 && matches!(part[1], 0 | 0x80 | 0x20 | 0xa0)
                && usize::from(part[0]) <= MAX_OBJECTS =>
            {
                object_info = true;
                result.objects = usize::from(part[0]);
                if part[1] & 0x60 == 0x20 {
                    let size = be16(take(bytes, &mut at, 2)?);
                    if size != 256 * 3 {
                        return Err(invalid());
                    }
                    result.palette = Some(take(bytes, &mut at, size)?.chunks_exact(3)
                        .map(|rgb| [rgb[0], rgb[1], rgb[2]]).collect());
                }
            }
            3 if result.frames == 0 && part.len() == 3 && part[1..] == [0, 7]
                && (1..=MAX_FRAMES).contains(&usize::from(part[0])) =>
            {
                result.frames = usize::from(part[0]);
            }
            10 if part.len() == 2 && part != [0, 0] => {}
            255 => {} // identificação do empacotador, sem operação de desenho
            _ => return Err(invalid()),
        }
    }
    if !dimensions || !object_info || result.frames == 0 || result.width == 0 || result.height == 0 {
        return Err(invalid());
    }
    Ok(result)
}

struct Bits<'a> {
    data: &'a [u8],
    at: usize,
}

impl Bits<'_> {
    fn read(&mut self, count: usize) -> Result<u16> {
        if count > 15 || self.at.checked_add(count).is_none_or(|end| end > self.data.len() * 8) {
            return Err(invalid());
        }
        let mut result = 0;
        for shift in 0..count {
            result |= u16::from((self.data[self.at / 8] >> (self.at % 8)) & 1) << shift;
            self.at += 1;
        }
        Ok(result)
    }
}

/// Quantos bits ocupa a descrição dinâmica, sem os três bits BFINAL/BTYPE.
/// Só é necessário ler a pequena árvore de comprimentos: os pixels e o LZ77
/// pertencem ao flate2, que também valida as árvores de literais e distâncias.
fn table_bits(table: &[u8]) -> Result<usize> {
    let mut bits = Bits { data: table, at: 0 };
    let literals = usize::from(bits.read(5)?) + 257;
    let distances = usize::from(bits.read(5)?) + 1;
    let count = usize::from(bits.read(4)?) + 4;
    if literals > 286 || distances > 30 {
        return Err(invalid());
    }
    let order = [16,17,18,0,8,7,9,6,10,5,11,4,12,3,13,2,14,1,15];
    let mut lengths = [0usize; 19];
    let mut counts = [0u16; 16];
    for &symbol in &order[..count] {
        lengths[symbol] = usize::from(bits.read(3)?);
        if lengths[symbol] != 0 {
            counts[lengths[symbol]] += 1;
        }
    }
    let mut available = 1i32;
    for &used in &counts[1..] {
        available = available * 2 - i32::from(used);
        if available < 0 {
            return Err(invalid());
        }
    }
    if counts.iter().sum::<u16>() == 0 {
        return Err(invalid());
    }
    let mut next = [0u16; 16];
    for width in 1..16 {
        next[width] = (next[width - 1] + counts[width - 1]) << 1;
    }
    let mut tree = Vec::new();
    for (symbol, &width) in lengths.iter().enumerate() {
        if width != 0 {
            let reversed = next[width].reverse_bits() >> (16 - width);
            tree.push((width, reversed, symbol));
            next[width] += 1;
        }
    }
    let target = literals + distances;
    let mut decoded = Vec::with_capacity(target);
    while decoded.len() < target {
        let mut code = 0;
        let mut symbol = None;
        for width in 1..16 {
            code |= bits.read(1)? << (width - 1);
            if let Some(entry) = tree.iter().find(|entry| entry.0 == width && entry.1 == code) {
                symbol = Some(entry.2);
                break;
            }
        }
        let (value, repeat) = match symbol.ok_or_else(invalid)? {
            value @ 0..=15 => (value as u8, 1),
            16 => (*decoded.last().ok_or_else(invalid)?, usize::from(bits.read(2)?) + 3),
            17 => (0, usize::from(bits.read(3)?) + 3),
            18 => (0, usize::from(bits.read(7)?) + 11),
            _ => return Err(invalid()),
        };
        if decoded.len() + repeat > target {
            return Err(invalid());
        }
        decoded.resize(decoded.len() + repeat, value);
    }
    let used = bits.at.div_ceil(8);
    // Os arquivos reais incluem um byte zero depois da descrição. Ele não é
    // parte da árvore e não pode ser inserido entre a árvore e os códigos LZ77.
    if decoded[256] == 0 || table.len() > used + 1 || table.get(used..).is_none_or(|tail| tail.iter().any(|&b| b != 0)) {
        return Err(invalid());
    }
    Ok(bits.at)
}

fn append_bits(output: &mut Vec<u8>, written: &mut usize, data: &[u8], start: usize, count: usize) {
    for at in start..start + count {
        if *written % 8 == 0 {
            output.push(0);
        }
        let index = output.len() - 1;
        output[index] |= ((data[at / 8] >> (at % 8)) & 1) << (*written % 8);
        *written += 1;
    }
}

fn mlz(table: &[u8], encoded: &[u8], expected: usize) -> Result<Vec<u8>> {
    if encoded.len() < 7 || encoded[0] & 15 != 8 || encoded[0] >> 4 > 7
        || u16::from_be_bytes([encoded[0], encoded[1]]) % 31 != 0
        || encoded[1] & 32 != 0 || encoded[2] & 7 != 5
    {
        return Err(invalid());
    }
    let descriptor_bits = table_bits(table)?;
    let stream = &encoded[2..encoded.len() - 4];
    let mut restored = Vec::with_capacity(stream.len() + table.len() + 1);
    let mut written = 0;
    append_bits(&mut restored, &mut written, stream, 0, 3);
    append_bits(&mut restored, &mut written, table, 0, descriptor_bits);
    append_bits(&mut restored, &mut written, stream, 3, stream.len() * 8 - 3);
    // Um byte além do raster detecta expansão excessiva sem materializá-la.
    // Dimensões SAF são bytes: nenhum raster deste subconjunto supera 65025.
    let mut output = vec![0; expected + 1];
    let mut decoder = Decompress::new(false);
    if decoder.decompress(&restored, &mut output, FlushDecompress::Finish).map_err(|_| invalid())? != Status::StreamEnd
        || decoder.total_out() != expected as u64
    {
        return Err(invalid());
    }
    let consumed = decoder.total_in() as usize;
    if restored.len() - consumed > 1 || restored[consumed..].iter().any(|&b| b != 0) {
        return Err(invalid());
    }
    output.truncate(expected);
    let (mut low, mut high) = (1u32, 0u32);
    for &byte in &output {
        low = (low + u32::from(byte)) % 65521;
        high = (high + low) % 65521;
    }
    let checksum = u32::from_be_bytes(encoded[encoded.len() - 4..].try_into().unwrap());
    if (high << 16) | low != checksum {
        return Err(invalid());
    }
    Ok(output)
}

pub(super) fn decode(data: &[u8]) -> Result<Image> {
    decode_inner(data, false).map(|(image, _)| image)
}

/// Todos os quadros numa tira horizontal, para o DrawFrame do IImage.
/// O segundo valor é zero para imagem estática, ou a largura de cada quadro.
/// O avanço por tempo permanece a cargo do consumidor, como nas tiras GIF.
pub(crate) fn decode_frames(data: &[u8]) -> Result<(Image, usize)> {
    decode_inner(data, true)
}

fn decode_inner(data: &[u8], allow_animation: bool) -> Result<(Image, usize)> {
    if data.len() < 7 || &data[..5] != b"SAF\0\x02" || be16(&data[5..7]) != data.len() {
        return Err(invalid());
    }
    let mut at = 7;
    let mut spec = None;
    let mut table = None;
    let mut objects: Vec<Option<Object>> = (0..MAX_OBJECTS).map(|_| None).collect();
    let mut object_count = 0;
    let mut decoded_pixels = 0usize;
    let mut frames = Vec::new();
    let mut ended = false;
    while at < data.len() {
        let prefix = take(data, &mut at, 3)?;
        let part = take(data, &mut at, be16(&prefix[1..]))?;
        match prefix[0] {
            7 if spec.is_none() && part.len() == 2 => {
                let parsed = header(take(data, &mut at, be16(part))?)?;
                // A miniatura estática não precisa descomprimir uma animação
                // inteira para então recusá-la; o cabeçalho já basta.
                if !allow_animation && parsed.frames != 1 {
                    return Err(invalid());
                }
                spec = Some(parsed);
            }
            11 if spec.is_some() && table.is_none() && object_count == 0 => table = Some(part),
            4 if spec.is_some() && frames.is_empty() && part.len() == 5
                && part[1] != 0 && part[2] != 0 && part[3] == 1 =>
            {
                // O bit alto é um sinalizador de descompressão do provedor SAF;
                // a referência do quadro usa só os sete bits baixos do objeto.
                // Aqui todos os objetos são descomprimidos antes da composição.
                let id = usize::from(part[0] & 0x7f);
                if objects[id].is_some() || object_count >= spec.as_ref().ok_or_else(invalid)?.objects {
                    return Err(invalid());
                }
                let count = be16(take(data, &mut at, 2)?);
                let encoded = take(data, &mut at, count)?;
                let width = usize::from(part[1]);
                let height = usize::from(part[2]);
                decoded_pixels = decoded_pixels.checked_add(width * height).ok_or_else(invalid)?;
                if decoded_pixels > MAX_OBJECT_PIXELS {
                    return Err(invalid());
                }
                let pixels = mlz(table.ok_or_else(invalid)?, encoded, width * height)?;
                objects[id] = Some(Object { width, height, transparent: part[4], pixels });
                object_count += 1;
            }
            6 if spec.is_some() && part.len() >= 2
                && frames.len() < spec.as_ref().ok_or_else(invalid)?.frames
                && usize::from(part[1]) <= MAX_OBJECTS + 1 =>
            {
                let mut cursor = 2;
                let mut placements = Vec::new();
                let mut metadata = false;
                for _ in 0..part[1] {
                    let command = take(part, &mut cursor, 2)?;
                    let payload = take(part, &mut cursor, usize::from(command[1]))?;
                    match command[0] {
                        0 if payload.len() == 5 && payload[1] & 0x1f == 0
                            && matches!(payload[2], 0 | 2)
                            && objects.get(usize::from(payload[0])).and_then(Option::as_ref).is_some() =>
                        {
                            placements.push(payload);
                        }
                        // O provedor de referência ignora este TLV desconhecido.
                        // Aceitar só a assinatura estática observada, não efeitos
                        // arbitrários nem uma operação de transformação inventada.
                        0x21 if !metadata && payload == [1, 0, 0] => metadata = true,
                        _ => return Err(invalid()),
                    }
                }
                // O provedor preenche o fundo antes dos movimentos; zero
                // movimentos é um quadro completo, não um recurso truncado.
                if cursor != part.len() || placements.len() > MAX_OBJECTS {
                    return Err(invalid());
                }
                frames.push((part[0], placements));
            }
            8 if part.is_empty() && at == data.len() => { ended = true; break; }
            _ => return Err(invalid()),
        }
    }
    let Header { width, height, objects: expected_objects, frames: expected_frames, palette } = spec.ok_or_else(invalid)?;
    if !ended || object_count != expected_objects || frames.len() != expected_frames {
        return Err(invalid());
    }
    let count = width.checked_mul(height).and_then(|n| n.checked_mul(frames.len())).ok_or_else(invalid)?;
    if count > MAX_FRAME_PIXELS {
        return Err(invalid());
    }
    let strip_width = width * frames.len();
    let mut indices = vec![0; count];
    for (number, (background, placements)) in frames.iter().enumerate() {
        // A referência reinicia cada quadro com seu próprio fundo; SAF não usa
        // os pixels do quadro anterior como uma atualização parcial.
        for row in indices.chunks_exact_mut(strip_width) {
            row[number * width..(number + 1) * width].fill(*background);
        }
        for placement in placements {
            let object = objects[usize::from(placement[0])].as_ref().ok_or_else(invalid)?;
            // Os bits altos dão o sinal, e os bytes de posição dão a magnitude.
            // Não são coordenadas i8: +255 e -255 são ambos representáveis.
            let left = i32::from(placement[3]) * if placement[1] & 0x80 == 0 { 1 } else { -1 };
            let top = i32::from(placement[4]) * if placement[1] & 0x40 == 0 { 1 } else { -1 };
            for y in 0..object.height {
                for x in 0..object.width {
                    let (dx, dy) = (x as i32 + left, y as i32 + top);
                    // Espelha as colunas do objeto antes de consultar a chave
                    // de transparência; a posição continua ancorando seu canto.
                    let sx = if placement[1] & 0x20 != 0 { object.width - 1 - x } else { x };
                    let index = object.pixels[y * object.width + sx];
                    if dx >= 0 && dy >= 0 && dx < width as i32 && dy < height as i32
                        && !(placement[2] == 2 && index == object.transparent)
                    {
                        indices[dy as usize * strip_width + number * width + dx as usize] = index;
                    }
                }
            }
        }
    }
    // Transparência de objeto é composição dentro do SAF. O fundo do quadro
    // permanece opaco; quem pediu OLDMASK no IImage aplica sua chave depois.
    let rgba = indices.into_iter().flat_map(|color| {
        let [r, g, b] = match &palette {
            Some(entries) => entries[usize::from(color)],
            None => [
                ((u16::from(color >> 5) * 255) / 7) as u8,
                ((u16::from((color >> 2) & 7) * 255) / 7) as u8,
                (color & 3) * 85,
            ],
        };
        [r, g, b, 255]
    }).collect();
    Ok((Image { width: strip_width, height, rgba }, if frames.len() > 1 { width } else { 0 }))
}
