//! As texturas são os PNGs originais; direção muda por UV, sem novas cópias de arte.
use super::{
    zeebo_controls::{Controls, Rect, button_index, merge},
    zeebo_mapping::Map,
};
use std::{collections::BTreeMap, sync::Arc};
struct Art {
    textures: BTreeMap<&'static str, egui::TextureHandle>,
}
impl Art {
    fn load(ctx: &egui::Context) -> Self {
        macro_rules! assets{($($n:literal),*)=>{[$(($n,include_bytes!(concat!("../assets/zeebo/",$n,".png")).as_slice())),*]};}
        let mut textures = BTreeMap::new();
        for (name, bytes) in assets!(
            "Action_1",
            "Action_2",
            "Action_3",
            "Action_4",
            "Analog_Inner",
            "Analog_Outer",
            "Dpad_Up",
            "HOME",
            "ZL",
            "ZR",
            "Action_Pressed",
            "Dpad_Pressed",
            "Home_Pressed",
            "ZL_Pressed",
            "ZR_Pressed"
        ) {
            let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
            decoder
                .set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
            let Ok(mut reader) = decoder.read_info() else {
                continue;
            };
            let Some(size) = reader.output_buffer_size() else {
                continue;
            };
            let mut out = vec![0; size];
            let Ok(info) = reader.next_frame(&mut out) else {
                continue;
            };
            if info.color_type != png::ColorType::Rgba {
                continue;
            }
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [info.width as usize, info.height as usize],
                &out[..info.buffer_size()],
            );
            textures.insert(
                name,
                ctx.load_texture(format!("zeebo-{name}"), image, egui::TextureOptions::LINEAR),
            );
        }
        Self { textures }
    }
    fn draw(
        &self,
        painter: &egui::Painter,
        name: &str,
        rect: Rect,
        origin: egui::Pos2,
        alpha: u8,
        quarter: u8,
    ) {
        let Some(texture) = self.textures.get(name) else {
            return;
        };
        let r = egui::Rect::from_min_size(
            origin + egui::vec2(rect.x, rect.y),
            egui::vec2(rect.w, rect.h),
        );
        let mut mesh = egui::Mesh::with_texture(texture.id());
        let uv = [
            egui::pos2(0., 0.),
            egui::pos2(1., 0.),
            egui::pos2(1., 1.),
            egui::pos2(0., 1.),
        ];
        for (i, pos) in [
            r.left_top(),
            r.right_top(),
            r.right_bottom(),
            r.left_bottom(),
        ]
        .into_iter()
        .enumerate()
        {
            mesh.vertices.push(egui::epaint::Vertex {
                pos,
                uv: uv[(i + 4 - quarter as usize) % 4],
                color: egui::Color32::from_white_alpha(alpha),
            });
        }
        mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
        painter.add(egui::Shape::mesh(mesh));
    }
}
pub fn paint(
    controls: &mut Controls,
    mapping: &mut Map,
    ctx: &egui::Context,
    transparency: u8,
) -> bool {
    let area = ctx.content_rect();
    let changed = controls.viewport(area.width(), area.height(), [area.min.x, area.min.y]);
    if changed {
        mapping.release_all();
    }
    if !controls.enabled() {
        return changed;
    }
    let id = egui::Id::new("zeebo-controller-art-v1");
    let art = ctx
        .data_mut(|d| d.get_temp::<Arc<Art>>(id))
        .unwrap_or_else(|| {
            let art = Arc::new(Art::load(ctx));
            ctx.data_mut(|d| d.insert_temp(id, art.clone()));
            art
        });
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("zeebo-controller-faces"),
    ));
    let alpha = (255u32 * transparency.min(100) as u32 / 100) as u8;
    let pad = merge(mapping.pad(), controls.pad());
    for (name, rect) in &controls.layout.buttons {
        let (normal, pressed) = match *name {
            "b1" => ("Action_1", "Action_Pressed"),
            "b2" => ("Action_2", "Action_Pressed"),
            "b3" => ("Action_3", "Action_Pressed"),
            "b4" => ("Action_4", "Action_Pressed"),
            "zl" => ("ZL", "ZL_Pressed"),
            "zr" => ("ZR", "ZR_Pressed"),
            _ => ("HOME", "Home_Pressed"),
        };
        art.draw(&painter, normal, *rect, area.min, alpha, 0);
        if button_index(name).is_some_and(|i| pad.is_down(i)) {
            art.draw(&painter, pressed, *rect, area.min, alpha, 0);
        }
    }
    for name in ["up", "right", "down", "left"] {
        let (r, q) = controls.layout.direction(name);
        art.draw(&painter, "Dpad_Up", r, area.min, alpha, q);
        if button_index(name).is_some_and(|i| pad.is_down(i)) {
            art.draw(&painter, "Dpad_Pressed", r, area.min, alpha, q);
        }
    }
    for i in 0..2 {
        let r = controls.layout.sticks[i];
        art.draw(&painter, "Analog_Outer", r, area.min, alpha, 0);
        let c = controls.inner_center(i);
        let size = controls.layout.inner_sizes[i];
        art.draw(
            &painter,
            "Analog_Inner",
            Rect {
                x: c[0] - size / 2.,
                y: c[1] - size / 2.,
                w: size,
                h: size,
            },
            area.min,
            alpha,
            0,
        );
    }
    changed
}
