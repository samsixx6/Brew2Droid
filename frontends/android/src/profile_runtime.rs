//! Aplica a configuração efetiva uma vez, antes de construir a sessão.
use serde_json::Value;
use zeebx::ui::settings::Settings;
use zeebx::handset::types::DisplayGeometry;
pub struct Prepared {pub zeebo:bool,pub zeebo_bindings:Value,pub volume_keys_mappable:bool,pub options:crate::game_options::GameOptions,pub transparency:u8,pub bindings:Value,pub layout:Value,pub resolution:Option<DisplayGeometry>}
pub fn prepare(raw:&Value,settings:&mut Settings)->Result<Prepared,String>{
    let values=crate::profile_store::values::normalize(raw)?;
    let scaling=serde_json::from_value(values["scaling"].clone()).map_err(|e|format!("Invalid profile scaling: {e}"))?;
    let options=crate::game_options::GameOptions{
        orientation:match values["orientation"].as_str(){Some("portrait")=>crate::game_options::ScreenOrientation::Portrait,Some("landscape")=>crate::game_options::ScreenOrientation::Landscape,_=>crate::game_options::ScreenOrientation::Auto},
        gravity:match values["gravity"].as_str(){Some("top")=>crate::game_options::ScreenGravity::Top,Some("bottom")=>crate::game_options::ScreenGravity::Bottom,_=>crate::game_options::ScreenGravity::Center},
        controls:if values["controls_enabled"].as_bool()==Some(false){crate::game_options::OnScreenControls::Off}else{crate::game_options::OnScreenControls::On},
    };
    let resolution=values["resolution"].as_object().map(|v|DisplayGeometry{width:v["width"].as_u64().unwrap() as u16,height:v["height"].as_u64().unwrap() as u16,depth:16});
    settings.graphics.scaling=scaling;settings.graphics.smooth=values["smooth"].as_bool().unwrap_or(false);settings.graphics.keep_aspect=values["keep_aspect"].as_bool().unwrap_or(true);
    Ok(Prepared{zeebo:values["button_type"]=="zeebo",zeebo_bindings:values["zeebo_bindings"].clone(),volume_keys_mappable:values["volume_keys_mappable"].as_bool().unwrap_or(false),options,transparency:values["transparency"].as_u64().unwrap_or(50) as u8,bindings:values["bindings"].clone(),layout:values["layout"].clone(),resolution})
}
pub fn geometry(module:&std::path::Path,hash:&str,resolution:Option<DisplayGeometry>)->DisplayGeometry {
    // Os overrides antigos já foram migrados para Custom. Auto não os consulta novamente.
    resolution.unwrap_or_else(||zeebx::handset::display::automatic(hash,module).geometry)
}
