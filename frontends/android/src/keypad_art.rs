//! Teclado da partida; Button Mapping conserva seus próprios assets e proporções.
use zeebx::handset::touch::{KeyRegion,TouchKeypad};

struct KeypadArt { textures: std::collections::HashMap<&'static str,egui::TextureHandle> }
impl KeypadArt {
    fn load(ctx: &egui::Context) -> Self {
        macro_rules! art { ($($name:literal),+ $(,)?) => {[$(($name,include_bytes!(concat!("../assets/keypad/",$name,"@2x.png")).as_slice())),+]}; }
        let mut textures=std::collections::HashMap::new();
        for (name,bytes) in art!(
            "button-0","button-1","button-2","button-3","button-4","button-5","button-6","button-7","button-8","button-9",
            "button-asterisk","button-hash","Dpad_Center_Rounded_Outline","button-left-soft-key",
            "button-left-up","button-right-down","button-left","button-right",
            "pressed-overlay-normal",
        ) {
            let mut decoder=png::Decoder::new(std::io::Cursor::new(bytes));
            decoder.set_transformations(png::Transformations::EXPAND|png::Transformations::STRIP_16);
            let Ok(mut reader)=decoder.read_info() else {continue};
            let Some(size)=reader.output_buffer_size() else {continue};
            let mut pixels=vec![0;size];
            let Ok(info)=reader.next_frame(&mut pixels) else {continue};
            let source=&pixels[..info.buffer_size()];
            let rgba=match info.color_type {
                png::ColorType::Rgba=>source.to_vec(),
                png::ColorType::Rgb=>source.chunks_exact(3).flat_map(|p|[p[0],p[1],p[2],255]).collect(),
                png::ColorType::Grayscale=>source.iter().flat_map(|v|[*v,*v,*v,255]).collect(),
                png::ColorType::GrayscaleAlpha=>source.chunks_exact(2).flat_map(|p|[p[0],p[0],p[0],p[1]]).collect(),
                _=>continue,
            };
            let image=egui::ColorImage::from_rgba_unmultiplied([info.width as usize,info.height as usize],&rgba);
            textures.insert(name,ctx.load_texture(name,image,egui::TextureOptions::LINEAR));
        }
        Self {textures}
    }
    fn draw(&self,painter:&egui::Painter,name:&str,rect:egui::Rect,alpha:u8) {
        if let Some(texture)=self.textures.get(name) {
            painter.image(texture.id(),rect,egui::Rect::from_min_max(egui::Pos2::ZERO,egui::pos2(1.0,1.0)),egui::Color32::from_white_alpha(alpha));
        }
    }
}

pub fn paint(keys:&mut TouchKeypad,ctx:&egui::Context,transparency:u8) {
    paint_layout(keys,ctx,transparency,&serde_json::Value::Null);
}

struct Geometry {size:[f32;2],layout:serde_json::Value,regions:Vec<crate::keypad_layout::Region>}

pub fn paint_layout(keys:&mut TouchKeypad,ctx:&egui::Context,transparency:u8,layout:&serde_json::Value) {
    keys.regions.clear();
    if !keys.is_enabled(){return;}
    let area=ctx.content_rect();let size=[area.width(),area.height()];
    let geometry_id=egui::Id::new("handset-keypad-geometry-v3");
    let previous=ctx.data_mut(|data|data.get_temp::<std::sync::Arc<Geometry>>(geometry_id));
    let geometry=previous.filter(|g|g.size==size&&g.layout==*layout).unwrap_or_else(||{
        let geometry=std::sync::Arc::new(Geometry{size,layout:layout.clone(),regions:crate::keypad_layout::regions(size[0],size[1],layout)});
        ctx.data_mut(|data|data.insert_temp(geometry_id,geometry.clone()));geometry
    });
    if !geometry.regions.iter().any(|r|r.visible){return;}
    let id=egui::Id::new("handset-game-keypad-art-v3");
    let art=ctx.data_mut(|data|data.get_temp::<std::sync::Arc<KeypadArt>>(id)).unwrap_or_else(||{
        let art=std::sync::Arc::new(KeypadArt::load(ctx));
        ctx.data_mut(|data|data.insert_temp(id,art.clone()));art
    });
    let alpha=(255u32*transparency.min(100) as u32/100) as u8;
    // A sobreposição não reserva espaço: Fit e gravidade usam a mesma área com ou sem teclas.
    let painter=ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground,egui::Id::new("handset-keypad-faces")));
    let held=keys.held();
    for region in geometry.regions.iter().filter(|r|r.visible){
        let rect=egui::Rect::from_min_max(area.min+egui::vec2(region.min[0],region.min[1]),area.min+egui::vec2(region.max[0],region.max[1]));
        art.draw(&painter,region.art,rect,alpha);
        if held&(1<<region.key as u8)!=0{art.draw(&painter,"pressed-overlay-normal",rect,alpha);}
        keys.regions.push(KeyRegion{key:region.key,min:[rect.min.x,rect.min.y],max:[rect.max.x,rect.max.y]});
    }
}
