//! ISprite móvel, conforme AEESprite.h do SDK 4.0.2.
//! O atlas vertical e o destino são retidos, não copiados: Blitzkrieg solta o bitmap
//! logo depois de SetTileBuffer. O livro de vínculos fica no host: bytes privados
//! graváveis pelo guest não provam propriedade de uma referência ao liberar ou restaurar.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(super) struct SpriteState {
    matrix: u32,
    destination: u32,
    sprite_buffers: [u32; 4],
    tile_buffers: [u32; 4],
}

const SDK_BAD_PARAM: u32 = 14;
const SDK_NO_MEMORY: u32 = 2;
const SPRITE_CLASS: u32 = 0x0100_1028;
const IQI: u32 = 0x0100_0001;
const MAX_COMMANDS: usize = 4096;
const MAX_MAPS: usize = 128;
const MAX_SPRITES: usize = 1024;

enum SpriteBinding {
    Destination,
    Sprite(usize),
    Tile(usize),
}

#[derive(Serialize, Deserialize)]
struct SavedSprites {
    version: u32,
    sprites: Vec<(u32, SpriteState)>,
}

impl SpriteState {
    fn retained_bitmaps(&self) -> impl Iterator<Item = u32> {
        std::iter::once(self.destination)
            .chain(self.sprite_buffers)
            .chain(self.tile_buffers)
    }

    fn binding(&mut self, which: SpriteBinding) -> &mut u32 {
        match which {
            SpriteBinding::Destination => &mut self.destination,
            SpriteBinding::Sprite(index) => &mut self.sprite_buffers[index],
            SpriteBinding::Tile(index) => &mut self.tile_buffers[index],
        }
    }
}

#[derive(Clone, Copy)]
struct SpriteCommand {
    x: i32,
    y: i32,
    transform: u16,
    matrix: u8,
    index: u8,
    size: u8,
    composite: u8,
    layer: u8,
}
#[derive(Clone, Copy)]
struct TileMap {
    array: u32,
    flags: u32,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    size: u8,
}

impl<C: CpuBackend> Machine<C> {
    pub(super) fn cria_sprite_legado(&mut self, out: u32) -> Result<u32, CpuError> {
        if out == 0 {
            return Ok(SDK_BAD_PARAM);
        }
        self.cpu.write_u32(out, 0)?;
        if self.legacy_sprites.len() >= MAX_SPRITES {
            return Ok(SDK_NO_MEMORY);
        }
        let object = self.new_object(Interface::Sprite)?;
        if object == 0 {
            return Ok(SDK_NO_MEMORY);
        }
        // ObjectStore reutiliza slots: o atlas da instância morta não pertence à nova.
        self.cpu.write_mem(object + 4, &[0; 40])?;
        self.legacy_sprites.insert(object, SpriteState::default());
        self.cpu.write_u32(out, object)?;
        Ok(SUCCESS)
    }

    fn sprite_bitmap_valido(&self, bitmap: u32) -> bool {
        self.objects.kind_of(bitmap) == Some(Interface::Bitmap)
            && self.bitmaps.contains_key(&bitmap)
    }

    fn sprite_solta_bitmap(&mut self, bitmap: u32) {
        if bitmap == 0 || self.objects.kind_of(bitmap) != Some(Interface::Bitmap) {
            return;
        }
        if bitmap == self.device_bitmap && self.objects.contagem(bitmap) <= 1 {
            return;
        }
        if self.objects.release(bitmap) == 0 {
            self.solta_dib(bitmap);
            self.bitmaps.remove(&bitmap);
            self.transparency.remove(&bitmap);
        }
    }

    fn sprite_troca_bitmap(
        &mut self,
        object: u32,
        which: SpriteBinding,
        bitmap: u32,
    ) -> Result<u32, CpuError> {
        if bitmap != 0 && !self.sprite_bitmap_valido(bitmap) {
            return Ok(SDK_BAD_PARAM);
        }
        let Some(state) = self.legacy_sprites.get_mut(&object) else {
            return Ok(SDK_BAD_PARAM);
        };
        let binding = state.binding(which);
        let previous = *binding;
        if previous != bitmap {
            if bitmap != 0 {
                self.objects.add_ref(bitmap);
            }
            *binding = bitmap;
            self.sprite_solta_bitmap(previous);
        }
        Ok(SUCCESS)
    }

    pub(super) fn sprite_legado_call(&mut self, slot: u32) -> Result<Option<u32>, CpuError> {
        let object = self.cpu.read_reg(Reg::R0);
        if self.objects.kind_of(object) != Some(Interface::Sprite)
            || !self.legacy_sprites.contains_key(&object)
        {
            return Ok(Some(SDK_BAD_PARAM));
        }
        let (a, b) = (self.cpu.read_reg(Reg::R1), self.cpu.read_reg(Reg::R2));
        let result = match slot {
            0 => self.objects.add_ref(object),
            1 => {
                let count = self.objects.release(object);
                if count == 0 {
                    if let Some(state) = self.legacy_sprites.remove(&object) {
                        for bitmap in state.retained_bitmaps() {
                            self.sprite_solta_bitmap(bitmap);
                        }
                    }
                }
                count
            }
            2 => {
                if b == 0 {
                    return Ok(Some(SDK_BAD_PARAM));
                }
                let matched = matches!(a, SPRITE_CLASS | IQI);
                self.cpu.write_u32(b, if matched { object } else { 0 })?;
                if matched {
                    self.objects.add_ref(object);
                    SUCCESS
                } else {
                    ECLASSNOTSUPPORT
                }
            }
            3 => {
                self.legacy_sprites.get_mut(&object).unwrap().matrix = a;
                SUCCESS
            }
            4 | 5 => {
                if a > 3 {
                    return Ok(Some(SDK_BAD_PARAM));
                }
                let which = if slot == 4 {
                    SpriteBinding::Sprite(a as usize)
                } else {
                    SpriteBinding::Tile(a as usize)
                };
                self.sprite_troca_bitmap(object, which, b)?
            }
            6 => self.sprite_troca_bitmap(object, SpriteBinding::Destination, a)?,
            7 => self.sprite_desenha_comandos(object, a)?,
            8 => self.sprite_desenha_mapas(object, a)?,
            _ => return Ok(None),
        };
        Ok(Some(result))
    }

    fn sprite_matriz(
        &self,
        object: u32,
        flags: u16,
        matrix: u8,
    ) -> Result<Option<[f32; 4]>, CpuError> {
        if flags & 0x8000 != 0 {
            let table = self
                .legacy_sprites
                .get(&object)
                .map_or(0, |state| state.matrix);
            let Some(at) = table
                .checked_add(u32::from(matrix) * 8)
                .filter(|_| table != 0)
            else {
                return Ok(None);
            };
            let mut bytes = [0; 8];
            self.cpu.read_mem(at, &mut bytes)?;
            let field = |n| i16::from_le_bytes([bytes[n], bytes[n + 1]]) as f32 / 256.0;
            return Ok(Some([field(0), field(2), field(4), field(6)]));
        }
        if flags & !0x3f != 0 {
            return Ok(None);
        }
        // ITransform usa reflexão SOBRE X e rotação anti-horária, com origem no centro.
        let scale = match (flags >> 3) & 7 {
            0 => 1.,
            1 => 2.,
            2 => 4.,
            3 => 8.,
            4 => 0.125,
            5 => 0.25,
            6 => 0.5,
            _ => return Ok(None),
        };
        let reflect = if flags & 4 != 0 { -1. } else { 1. };
        let (cos, sin) = match flags & 3 {
            0 => (1., 0.),
            1 => (0., 1.),
            2 => (-1., 0.),
            _ => (0., -1.),
        };
        Ok(Some([
            cos * scale,
            sin * reflect * scale,
            -sin * scale,
            cos * reflect * scale,
        ]))
    }

    fn sprite_desenha_comandos(&mut self, object: u32, ptr: u32) -> Result<u32, CpuError> {
        if ptr == 0 {
            return Ok(SDK_BAD_PARAM);
        }
        let Some(state) = self.legacy_sprites.get(&object).cloned() else {
            return Ok(SDK_BAD_PARAM);
        };
        let destination = state.destination;
        if !self.sprite_bitmap_valido(destination) {
            return Ok(SDK_BAD_PARAM);
        }
        let mut commands = Vec::new();
        let mut ended = false;
        for n in 0..MAX_COMMANDS {
            let Some(at) = ptr.checked_add(n as u32 * 16) else {
                return Ok(SDK_BAD_PARAM);
            };
            let mut b = [0; 16];
            self.cpu.read_mem(at, &mut b)?;
            if b[8] == 0xff {
                ended = true;
                break;
            }
            if b[8] > 3
                || !matches!(b[9], 0 | 0xff)
                || !matches!(b[10], 0..=3 | 0xff)
                || b[11..].iter().any(|&v| v != 0)
            {
                return Ok(SDK_BAD_PARAM);
            }
            commands.push(SpriteCommand {
                x: i16::from_le_bytes([b[0], b[1]]) as i32,
                y: i16::from_le_bytes([b[2], b[3]]) as i32,
                transform: u16::from_le_bytes([b[4], b[5]]),
                matrix: b[6],
                index: b[7],
                size: b[8],
                composite: b[9],
                layer: b[10],
            });
        }
        if !ended {
            return Ok(SDK_BAD_PARAM);
        }
        self.materializa_quadro_gl();
        self.sync_from_guest(destination)?;
        let mut synced = HashSet::new();
        for layer in 0..4 {
            for command in commands.iter().filter(|c| c.layer == layer) {
                let source = state.sprite_buffers[usize::from(command.size)];
                if synced.insert(source) {
                    self.sync_from_guest(source)?;
                }
                let Some(matrix) = self.sprite_matriz(object, command.transform, command.matrix)?
                else {
                    return Ok(SDK_BAD_PARAM);
                };
                let result = self.sprite_quadrado(
                    destination,
                    source,
                    command.x,
                    command.y,
                    8u32 << command.size,
                    u32::from(command.index),
                    matrix,
                    command.composite == 0,
                )?;
                if result != SUCCESS {
                    return Ok(result);
                }
            }
        }
        self.sync_to_guest(destination)?;
        Ok(SUCCESS)
    }

    fn sprite_desenha_mapas(&mut self, object: u32, ptr: u32) -> Result<u32, CpuError> {
        if ptr == 0 {
            return Ok(SDK_BAD_PARAM);
        }
        let Some(state) = self.legacy_sprites.get(&object).cloned() else {
            return Ok(SDK_BAD_PARAM);
        };
        let destination = state.destination;
        if !self.sprite_bitmap_valido(destination) {
            return Ok(SDK_BAD_PARAM);
        }
        let mut maps = Vec::new();
        let mut ended = false;
        for n in 0..MAX_MAPS {
            let Some(at) = ptr.checked_add(n as u32 * 40) else {
                return Ok(SDK_BAD_PARAM);
            };
            let array = self.cpu.read_u32(at)?;
            if array == 0 {
                ended = true;
                break;
            }
            let mut b = [0; 40];
            self.cpu.read_mem(at, &mut b)?;
            let u32_at = |i| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
            let width = u16::from_le_bytes([b[32], b[33]]);
            let height = u16::from_le_bytes([b[34], b[35]]);
            if width > 10
                || height > 10
                || b[36] > 3
                || u32_at(4) & !1 != 0
                || b[8..24].iter().chain(b[37..].iter()).any(|&v| v != 0)
            {
                return Ok(SDK_BAD_PARAM);
            }
            maps.push(TileMap {
                array,
                flags: u32_at(4),
                x: u32_at(24) as i32,
                y: u32_at(28) as i32,
                width: 1u32 << width,
                height: 1u32 << height,
                size: b[36],
            });
        }
        if !ended {
            return Ok(SDK_BAD_PARAM);
        }
        self.materializa_quadro_gl();
        self.sync_from_guest(destination)?;
        let (dw, dh) = {
            let d = &self.bitmaps[&destination];
            (i64::from(d.width()), i64::from(d.height()))
        };
        let mut synced = HashSet::new();
        for map in maps {
            let source = state.tile_buffers[usize::from(map.size)];
            if synced.insert(source) {
                self.sync_from_guest(source)?;
            }
            let side = 8i64 << map.size;
            let (x, y) = (i64::from(map.x), i64::from(map.y));
            let wrap = map.flags & 1 != 0;
            let mut cols = (0 - x).div_euclid(side)..=(dw - 1 - x).div_euclid(side);
            let mut rows = (0 - y).div_euclid(side)..=(dh - 1 - y).div_euclid(side);
            if !wrap {
                cols = (*cols.start()).max(0)..=(*cols.end()).min(i64::from(map.width) - 1);
                rows = (*rows.start()).max(0)..=(*rows.end()).min(i64::from(map.height) - 1);
            }
            for row in rows {
                for col in cols.clone() {
                    let index = row.rem_euclid(i64::from(map.height)) * i64::from(map.width)
                        + col.rem_euclid(i64::from(map.width));
                    let Some(at) = map.array.checked_add(index as u32 * 2) else {
                        return Ok(SDK_BAD_PARAM);
                    };
                    let mut bytes = [0; 2];
                    self.cpu.read_mem(at, &mut bytes)?;
                    let tile = u16::from_le_bytes(bytes);
                    if tile & 0x03ff == 0x03ff {
                        continue;
                    }
                    if tile & 0x0c00 != 0 {
                        return Ok(SDK_BAD_PARAM);
                    }
                    let matrix = self.sprite_matriz(object, tile >> 13, 0)?.unwrap();
                    let result = self.sprite_quadrado(
                        destination,
                        source,
                        (x + col * side) as i32,
                        (y + row * side) as i32,
                        side as u32,
                        u32::from(tile & 0x03ff),
                        matrix,
                        tile & 0x1000 != 0,
                    )?;
                    if result != SUCCESS {
                        return Ok(result);
                    }
                }
            }
        }
        self.sync_to_guest(destination)?;
        Ok(SUCCESS)
    }

    pub(super) fn grava_sprites_legados(&self, sections: &mut crate::save_state::Secoes) {
        sections.poe(
            "tab.legacy_sprites",
            serde_json::to_vec(&SavedSprites {
                version: 1,
                sprites: self
                    .legacy_sprites
                    .iter()
                    .map(|(&object, state)| (object, state.clone()))
                    .collect(),
            })
            .expect("vínculos de sprites serializáveis"),
        );
    }

    pub(super) fn restaura_sprites_legados(
        &mut self,
        reader: &crate::save_state::Leitor<'_>,
    ) -> Result<(), crate::save_state::Erro> {
        let invalid = |reason: &str| crate::save_state::Erro::Secao {
            nome: "tab.legacy_sprites".into(),
            motivo: reason.into(),
        };
        let live = self
            .objects
            .live_by_kind()
            .into_iter()
            .find_map(|(kind, count)| (kind == Interface::Sprite).then_some(count))
            .unwrap_or(0);
        let Some(bytes) = reader.secao("tab.legacy_sprites") else {
            // Saves antigos sem sprites não precisam deste livro. Os campos opacos
            // antigos de um sprite vivo não permitem reconstruir donos com segurança.
            if live != 0 {
                return Err(invalid("sprite vivo sem livro de referências"));
            }
            self.legacy_sprites.clear();
            return Ok(());
        };
        if bytes.len() > 1024 * 1024 {
            return Err(invalid("vínculos de sprites acima do limite"));
        }
        let saved: SavedSprites =
            serde_json::from_slice(bytes).map_err(|_| invalid("estrutura de sprites inválida"))?;
        if saved.version != 1 || saved.sprites.len() > MAX_SPRITES || saved.sprites.len() != live {
            return Err(invalid("versão ou quantidade de sprites inválida"));
        }
        let mut restored = std::collections::BTreeMap::new();
        let mut owners = std::collections::BTreeMap::<u32, u32>::new();
        for (object, state) in saved.sprites {
            if self.objects.kind_of(object) != Some(Interface::Sprite)
                || self.objects.contagem(object) == 0
                || restored.contains_key(&object)
            {
                return Err(invalid("objeto sprite inválido ou repetido"));
            }
            if state.matrix != 0 {
                let mut matrix = [0; 8];
                if state.matrix.checked_add(7).is_none()
                    || self.cpu.read_mem(state.matrix, &mut matrix).is_err()
                {
                    return Err(invalid("tabela de matrizes fora da memória restaurada"));
                }
            }
            for bitmap in state.retained_bitmaps().filter(|&bitmap| bitmap != 0) {
                if !self.sprite_bitmap_valido(bitmap) || self.objects.contagem(bitmap) == 0 {
                    return Err(invalid("bitmap retido não existe no estado restaurado"));
                }
                *owners.entry(bitmap).or_default() += 1;
            }
            restored.insert(object, state);
        }
        for (bitmap, owners) in owners {
            // O bitmap da tela também tem a referência permanente do display.
            let required = owners + u32::from(bitmap == self.device_bitmap);
            if self.objects.contagem(bitmap) < required {
                return Err(invalid(
                    "referências de bitmap insuficientes para os sprites",
                ));
            }
        }
        // ObjectStore e bitmaps já foram restaurados: trocar o livro não solta
        // referências do estado anterior sobre os novos objetos.
        self.legacy_sprites = restored;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn sprite_quadrado(
        &mut self,
        destination: u32,
        source: u32,
        x: i32,
        y: i32,
        side: u32,
        index: u32,
        matrix: [f32; 4],
        transparent: bool,
    ) -> Result<u32, CpuError> {
        if !self.sprite_bitmap_valido(source) {
            return Ok(SDK_BAD_PARAM);
        }
        let source_y = index * side;
        let source_bitmap = &self.bitmaps[&source];
        if source_bitmap.width() < side || source_y + side > source_bitmap.height() {
            return Ok(SDK_BAD_PARAM);
        }
        let pixels: Vec<_> = (0..side)
            .flat_map(|v| (0..side).map(move |u| (u, v)))
            .map(|(u, v)| source_bitmap.get_pixel(u as i32, (source_y + v) as i32))
            .collect();
        let key = transparent.then(|| {
            self.transparency
                .get(&source)
                .copied()
                .unwrap_or(TRANSPARENT_KEY)
        });
        let [a, b, c, d] = matrix;
        let det = a * d - b * c;
        if !det.is_finite() || det.abs() < f32::EPSILON {
            return Ok(SDK_BAD_PARAM);
        }
        let half = side as f32 / 2.;
        let (cx, cy) = (x as f32 + half, y as f32 + half);
        let mut bounds = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for (u, v) in [(-half, -half), (half, -half), (-half, half), (half, half)] {
            let (px, py) = (a * u + b * v + cx, c * u + d * v + cy);
            bounds[0] = bounds[0].min(px);
            bounds[1] = bounds[1].min(py);
            bounds[2] = bounds[2].max(px);
            bounds[3] = bounds[3].max(py);
        }
        let target = self.bitmaps.get_mut(&destination).unwrap();
        for py in
            (bounds[1].floor() as i32).max(0)..(bounds[3].ceil() as i32).min(target.height() as i32)
        {
            for px in (bounds[0].floor() as i32).max(0)
                ..(bounds[2].ceil() as i32).min(target.width() as i32)
            {
                let (u, v) = (px as f32 + 0.5 - cx, py as f32 + 0.5 - cy);
                let sx = ((d * u - b * v) / det + half).floor() as i32;
                let sy = ((-c * u + a * v) / det + half).floor() as i32;
                if sx < 0 || sy < 0 || sx >= side as i32 || sy >= side as i32 {
                    continue;
                }
                let pixel = pixels[sy as usize * side as usize + sx as usize];
                if Some(pixel) != key {
                    target.set_pixel_native(px, py, pixel);
                }
            }
        }
        Ok(SUCCESS)
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::cpu::BackendPadrao;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn machine() -> (Machine<BackendPadrao>, crate::scratch::TempDir) {
        let root = crate::scratch::TempDir::new(&format!(
            "brew-sprite-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let image = loader::modfile::ModImage::parse(
            [0xe3a0_0000u32, 0xe12f_ff1e]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect(),
        )
        .unwrap();
        let loaded = loader::load_with(&image, &[]).unwrap();
        let storage = crate::storage::StoragePaths::from_root(root.join("runtime"));
        let mut m = Machine::new_with_storage(
            BackendPadrao::new().unwrap(),
            loaded,
            root.path(),
            &storage,
            None,
        );
        m.run(1000).unwrap();
        (m, root)
    }
    fn invoke(m: &mut Machine<BackendPadrao>, function: u32, regs: [u32; 4]) -> u32 {
        match m.call_guest(function, regs, 1000).unwrap() {
            Outcome::Returned { code } => code,
            other => panic!("sprite: {other:?}"),
        }
    }
    fn sprite(m: &mut Machine<BackendPadrao>) -> u32 {
        let out = loader::HEAP_BASE + 0x100;
        assert_eq!(
            invoke(
                m,
                aee::encode(Interface::Shell, 2),
                [loader::OBJECT_BASE, 0x0100_1028, out, 0]
            ),
            0
        );
        let object = m.cpu.read_u32(out).unwrap();
        assert_ne!(object, 0);
        object
    }
    fn call(m: &mut Machine<BackendPadrao>, sprite: u32, slot: u32, a: u32, b: u32) -> u32 {
        let vtable = m.cpu.read_u32(sprite).unwrap();
        let function = m.cpu.read_u32(vtable + slot * 4).unwrap();
        invoke(m, function, [sprite, a, b, 0])
    }
    fn bitmap(m: &mut Machine<BackendPadrao>, width: u32, height: u32, color: u16) -> u32 {
        let object = m.new_object(Interface::Bitmap).unwrap();
        let mut pixels = Framebuffer::new(width, height);
        pixels.fill_rect_native(
            Rect {
                x: 0,
                y: 0,
                width: width as i16,
                height: height as i16,
            },
            color,
        );
        m.bitmaps.insert(object, pixels);
        object
    }
    fn cmd(x: i16, y: i16, index: u8, composite: u8, layer: u8) -> [u8; 16] {
        let mut b = [0; 16];
        b[..2].copy_from_slice(&x.to_le_bytes());
        b[2..4].copy_from_slice(&y.to_le_bytes());
        b[7] = index;
        b[8] = 0;
        b[9] = composite;
        b[10] = layer;
        b
    }
    fn commands(m: &mut Machine<BackendPadrao>, entries: &[[u8; 16]]) -> u32 {
        let p = loader::HEAP_BASE + 0x1000;
        let mut bytes: Vec<_> = entries.iter().flatten().copied().collect();
        let mut end = [0; 16];
        end[8] = 0xff;
        bytes.extend_from_slice(&end);
        m.cpu.write_mem(p, &bytes).unwrap();
        p
    }

    #[test]
    fn sprite_retains_replaces_and_releases_bitmap_references() {
        let (mut m, _root) = machine();
        let s = sprite(&mut m);
        let src = bitmap(&mut m, 8, 16, 0xf800);
        let dst = bitmap(&mut m, 16, 16, 0);
        assert_eq!(
            call(&mut m, s, 4, 4, src),
            14,
            "o SDK recusa tamanho inválido"
        );
        assert_eq!(call(&mut m, s, 4, 0, src), 0);
        assert_eq!(call(&mut m, s, 6, dst, 0), 0);
        assert_eq!(m.objects.contagem(src), 2);
        assert_eq!(m.objects.contagem(dst), 2);
        assert_eq!(
            invoke(&mut m, aee::encode(Interface::Bitmap, 1), [src, 0, 0, 0]),
            1
        );
        assert_eq!(call(&mut m, s, 4, 0, 0), 0);
        assert_eq!(m.objects.contagem(src), 0);
        assert_eq!(call(&mut m, s, 1, 0, 0), 0);
        assert_eq!(m.objects.contagem(dst), 1);
    }

    #[test]
    fn sprite_draws_vertical_frames_layers_transparency_and_negative_positions() {
        let (mut m, _root) = machine();
        let s = sprite(&mut m);
        let src = bitmap(&mut m, 8, 16, 0xf800);
        let dst = bitmap(&mut m, 4, 4, 0x001f);
        for y in 8..16 {
            for x in 0..8 {
                m.bitmaps
                    .get_mut(&src)
                    .unwrap()
                    .set_pixel_native(x, y, 0x07e0);
            }
        }
        m.bitmaps
            .get_mut(&src)
            .unwrap()
            .set_pixel_native(2, 10, 0xf81f);
        assert_eq!(call(&mut m, s, 4, 0, src), 0);
        assert_eq!(call(&mut m, s, 6, dst, 0), 0);
        let p = commands(
            &mut m,
            &[
                cmd(-1, -1, 1, 0, 1),
                cmd(0, 0, 0, 0xff, 0),
                cmd(0, 0, 0, 0xff, 0xff),
            ],
        );
        assert_eq!(call(&mut m, s, 7, p, 0), 0);
        assert_eq!(m.bitmaps[&dst].get_pixel(0, 0), 0x07e0);
        assert_eq!(
            m.bitmaps[&dst].get_pixel(1, 1),
            0xf800,
            "a chave transparente conserva a camada inferior"
        );
        assert_eq!(m.bitmaps[&dst].get_pixel(3, 3), 0x07e0);
        let p = commands(&mut m, &[cmd(3, 3, 0, 0xff, 0)]);
        assert_eq!(call(&mut m, s, 7, p, 0), 0);
        assert_eq!(
            m.bitmaps[&dst].get_pixel(3, 3),
            0xf800,
            "o novo comando move e troca o quadro"
        );
        m.bitmaps
            .get_mut(&src)
            .unwrap()
            .set_pixel_native(7, 7, 0xffff);
        let mut rotated = cmd(0, 0, 0, 0xff, 0);
        rotated[4] = 2;
        let p = commands(&mut m, &[rotated]);
        assert_eq!(call(&mut m, s, 7, p, 0), 0);
        assert_eq!(
            m.bitmaps[&dst].get_pixel(0, 0),
            0xffff,
            "a rotação usa o centro do sprite"
        );
        assert_eq!(call(&mut m, s, 7, 0, 0), 14);
    }

    #[test]
    fn sprite_tile_maps_use_power_of_two_sizes_wrap_and_nothing_index() {
        let (mut m, _root) = machine();
        let s = sprite(&mut m);
        let src = bitmap(&mut m, 8, 16, 0xf800);
        let dst = bitmap(&mut m, 20, 8, 0x001f);
        for y in 8..16 {
            for x in 0..8 {
                m.bitmaps
                    .get_mut(&src)
                    .unwrap()
                    .set_pixel_native(x, y, 0x07e0);
            }
        }
        assert_eq!(call(&mut m, s, 5, 0, src), 0);
        assert_eq!(call(&mut m, s, 6, dst, 0), 0);
        m.cpu.write_u32(s + 8, 0xdeadbeef).unwrap();
        m.cpu.write_u32(s + 28, 0xdeadbeef).unwrap();
        let map = loader::HEAP_BASE + 0x2000;
        let indices = map + 0x100;
        m.cpu.write_mem(indices, &[0, 0, 1, 0]).unwrap();
        let mut bytes = [0; 80];
        bytes[..4].copy_from_slice(&indices.to_le_bytes());
        bytes[4..8].copy_from_slice(&1u32.to_le_bytes());
        bytes[32..34].copy_from_slice(&1u16.to_le_bytes()); // MAP_SIZE_2: duas colunas, não uma.
        m.cpu.write_mem(map, &bytes).unwrap();
        assert_eq!(call(&mut m, s, 8, map, 0), 0);
        assert_eq!(m.bitmaps[&dst].get_pixel(0, 0), 0xf800);
        assert_eq!(m.bitmaps[&dst].get_pixel(8, 0), 0x07e0);
        assert_eq!(
            m.bitmaps[&dst].get_pixel(16, 0),
            0xf800,
            "o mapa envolve a largura do destino"
        );
        m.cpu.write_mem(indices, &[0xff, 3, 1, 0]).unwrap();
        m.bitmaps
            .get_mut(&dst)
            .unwrap()
            .set_pixel_native(0, 0, 0x001f);
        assert_eq!(call(&mut m, s, 8, map, 0), 0);
        assert_eq!(m.bitmaps[&dst].get_pixel(0, 0), 0x001f);
    }

    #[test]
    fn sprite_rejects_invalid_bindings_without_releasing_the_previous_bitmap() {
        let (mut m, _root) = machine();
        let s = sprite(&mut m);
        let src = bitmap(&mut m, 8, 8, 0xf800);
        let dst = bitmap(&mut m, 8, 8, 0);
        assert_eq!(call(&mut m, s, 4, 0, src), 0);
        assert_eq!(call(&mut m, s, 6, dst, 0), 0);
        assert_eq!(call(&mut m, s, 4, 0, 0xdeadbeef), 14);
        assert_eq!(m.objects.contagem(src), 2);
        // A memória privada opaca não é a autoridade dos vínculos retidos pelo runtime.
        m.cpu.write_u32(s + 12, 0xdeadbeef).unwrap();
        let p = commands(&mut m, &[cmd(0, 0, 0, 0xff, 0)]);
        assert_eq!(call(&mut m, s, 7, p, 0), 0);
        assert_eq!(m.bitmaps[&dst].get_pixel(0, 0), 0xf800);
    }

    #[test]
    fn sprite_buffer_bindings_survive_machine_save_state() {
        let (mut m, _root) = machine();
        let s = sprite(&mut m);
        let src = bitmap(&mut m, 8, 8, 0xf800);
        let dst = bitmap(&mut m, 8, 8, 0);
        assert_eq!(call(&mut m, s, 4, 0, src), 0);
        assert_eq!(call(&mut m, s, 6, dst, 0), 0);
        let state = m.grava_estado();
        assert_eq!(call(&mut m, s, 4, 0, 0), 0);
        assert_eq!(call(&mut m, s, 6, 0, 0), 0);
        m.restaura_estado(&state).unwrap();
        let p = commands(&mut m, &[cmd(0, 0, 0, 0xff, 0)]);
        assert_eq!(call(&mut m, s, 7, p, 0), 0);
        assert_eq!(m.bitmaps[&dst].get_pixel(7, 7), 0xf800);
        assert_eq!(m.objects.contagem(src), 2);
    }

    #[test]
    fn sprite_private_guest_words_cannot_release_an_unowned_bitmap() {
        let (mut m, _root) = machine();
        let s = sprite(&mut m);
        let retained = bitmap(&mut m, 8, 8, 0xf800);
        let unrelated = bitmap(&mut m, 8, 8, 0x07e0);
        assert_eq!(call(&mut m, s, 4, 0, retained), 0);
        // O chamador entrega seu dono ao sprite; só o vínculo do runtime resta.
        assert_eq!(
            invoke(
                &mut m,
                aee::encode(Interface::Bitmap, 1),
                [retained, 0, 0, 0]
            ),
            1
        );
        m.cpu.write_u32(s + 12, unrelated).unwrap();
        assert_eq!(call(&mut m, s, 4, 0, 0), 0);
        assert_eq!(
            m.objects.contagem(unrelated),
            1,
            "rebind não pode soltar o dono de outro objeto"
        );
        assert!(m.bitmaps.contains_key(&unrelated));
        assert_eq!(
            m.objects.contagem(retained),
            0,
            "o bitmap que o sprite reteve precisa ser liberado"
        );

        let retained = bitmap(&mut m, 8, 8, 0xf800);
        assert_eq!(call(&mut m, s, 4, 0, retained), 0);
        assert_eq!(
            invoke(
                &mut m,
                aee::encode(Interface::Bitmap, 1),
                [retained, 0, 0, 0]
            ),
            1
        );
        m.cpu.write_u32(s + 12, unrelated).unwrap();
        assert_eq!(call(&mut m, s, 1, 0, 0), 0);
        assert_eq!(
            m.objects.contagem(unrelated),
            1,
            "Release também precisa do livro de donos do host"
        );
        assert_eq!(m.objects.contagem(retained), 0);
    }

    #[test]
    fn sprite_restore_rejects_bindings_without_enough_retained_references() {
        let (mut m, _root) = machine();
        let first = sprite(&mut m);
        let second = sprite(&mut m);
        let owned = bitmap(&mut m, 8, 8, 0xf800);
        assert_eq!(call(&mut m, first, 4, 0, owned), 0);
        assert_eq!(
            invoke(&mut m, aee::encode(Interface::Bitmap, 1), [owned, 0, 0, 0]),
            1
        );
        let saved = m.grava_estado();
        let reader = crate::save_state::Leitor::abre(&saved).unwrap();
        let state = serde_json::json!({
            "matrix": 0, "destination": 0,
            "sprite_buffers": [owned, 0, 0, 0], "tile_buffers": [0, 0, 0, 0]
        });
        let malformed = serde_json::json!({"version": 1, "sprites": [
            [first, state.clone()], [second, state]
        ]});
        let mut sections = reader
            .nomes()
            .into_iter()
            .filter(|name| *name != "tab.legacy_sprites")
            .map(|name| (name.to_owned(), reader.secao(name).unwrap().to_vec()))
            .collect::<Vec<_>>();
        sections.push((
            "tab.legacy_sprites".into(),
            serde_json::to_vec(&malformed).unwrap(),
        ));
        assert!(
            m.restaura_estado(&crate::save_state::escreve(&sections))
                .is_err(),
            "dois sprites não podem compartilhar um bitmap com uma única referência viva"
        );
    }

    #[test]
    fn sprite_restore_rejects_missing_host_state_for_a_live_sprite() {
        let (mut m, _root) = machine();
        let _s = sprite(&mut m);
        let saved = m.grava_estado();
        let reader = crate::save_state::Leitor::abre(&saved).unwrap();
        let old = crate::save_state::escreve(
            &reader
                .nomes()
                .into_iter()
                .filter(|name| *name != "tab.legacy_sprites")
                .map(|name| (name.to_owned(), reader.secao(name).unwrap().to_vec()))
                .collect::<Vec<_>>(),
        );
        assert!(
            m.restaura_estado(&old).is_err(),
            "os ponteiros privados antigos não provam propriedade de referências"
        );
    }

    #[test]
    fn sprite_restore_accepts_old_states_without_sprites() {
        let (mut m, _root) = machine();
        let saved = m.grava_estado();
        let reader = crate::save_state::Leitor::abre(&saved).unwrap();
        let old = crate::save_state::escreve(
            &reader
                .nomes()
                .into_iter()
                .filter(|name| *name != "tab.legacy_sprites")
                .map(|name| (name.to_owned(), reader.secao(name).unwrap().to_vec()))
                .collect::<Vec<_>>(),
        );
        m.restaura_estado(&old).unwrap();
        let s = sprite(&mut m);
        assert_eq!(call(&mut m, s, 1, 0, 0), 0);
    }

    #[test]
    fn sprite_restore_counts_every_binding_and_validates_objects_and_matrices() {
        let (mut m, _root) = machine();
        let first = sprite(&mut m);
        let second = sprite(&mut m);
        let owned = bitmap(&mut m, 8, 8, 0xf800);
        // Cada slot é dono independente, mesmo quando aponta para o mesmo bitmap.
        assert_eq!(call(&mut m, first, 4, 0, owned), 0);
        assert_eq!(call(&mut m, first, 5, 0, owned), 0);
        assert_eq!(m.objects.contagem(owned), 3);
        let saved = m.grava_estado();
        m.restaura_estado(&saved).unwrap();
        assert_eq!(call(&mut m, first, 1, 0, 0), 0);
        assert_eq!(m.objects.contagem(owned), 1);

        let reader = crate::save_state::Leitor::abre(&saved).unwrap();
        let original: serde_json::Value =
            serde_json::from_slice(reader.secao("tab.legacy_sprites").unwrap()).unwrap();
        let mut duplicate = original.clone();
        duplicate["sprites"][1][0] = first.into();
        let mut wrong_type = original.clone();
        wrong_type["sprites"][0][1]["destination"] = second.into();
        let mut bad_matrix = original.clone();
        bad_matrix["sprites"][0][1]["matrix"] = u32::MAX.into();
        let mut too_many_owners = original;
        too_many_owners["sprites"][0][1]["destination"] = owned.into();
        too_many_owners["sprites"][1][1]["destination"] = owned.into();
        for malformed in [duplicate, wrong_type, bad_matrix, too_many_owners] {
            let mut sections = reader
                .nomes()
                .into_iter()
                .filter(|name| *name != "tab.legacy_sprites")
                .map(|name| (name.to_owned(), reader.secao(name).unwrap().to_vec()))
                .collect::<Vec<_>>();
            sections.push((
                "tab.legacy_sprites".into(),
                serde_json::to_vec(&malformed).unwrap(),
            ));
            assert!(
                m.restaura_estado(&crate::save_state::escreve(&sections))
                    .is_err()
            );
        }
    }
}
