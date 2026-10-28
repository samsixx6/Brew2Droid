//! Geometria do teclado em coordenadas lógicas, sem IO ou estado de toque.
use serde_json::Value;
use zeebx::handset::input::Key;
#[derive(Clone,Copy)]
pub struct Region {pub id:&'static str,pub key:Key,pub art:&'static str,pub min:[f32;2],pub max:[f32;2],pub visible:bool}
pub const BUTTONS:[(&str,Key,&str);21]=[
    ("soft_left",Key::Soft1,"button-left-soft-key"),("digit_1",Key::Num1,"button-1"),("digit_2",Key::Num2,"button-2"),("digit_3",Key::Num3,"button-3"),("soft_right",Key::Soft2,"button-left-soft-key"),
    ("up_left",Key::Up,"button-left-up"),("digit_4",Key::Num4,"button-4"),("digit_5",Key::Num5,"button-5"),("digit_6",Key::Num6,"button-6"),("up_right",Key::Up,"button-left-up"),
    ("left",Key::Left,"button-left"),("digit_7",Key::Num7,"button-7"),("digit_8",Key::Num8,"button-8"),("digit_9",Key::Num9,"button-9"),("right",Key::Right,"button-right"),
    ("down_left",Key::Down,"button-right-down"),("star",Key::Star,"button-asterisk"),("digit_0",Key::Num0,"button-0"),("pound",Key::Pound,"button-hash"),("down_right",Key::Down,"button-right-down"),
    ("enter",Key::Select,"Dpad_Center_Rounded_Outline"),
];
const DEFAULT: &[&[usize]] = &[&[0,20,4],&[10,14,19,5],&[18,1,2,3],&[16,6,7,8],&[17,11,12,13]];
const DPAD: &[&[usize]] = &[&[0,5,4],&[10,20,14],&[16,19,18]];
const NUMPAD: &[&[usize]] = &[&[0,20,4],&[1,2,3],&[6,7,8],&[11,12,13],&[16,17,18]];
fn preset(layout: &Value) -> Option<&str> {
    if layout.as_object().is_none_or(|o|o.is_empty()) {Some("default")}
    else if layout["_geometry"]["version"]==2 {Some(layout["_geometry"]["preset"].as_str().unwrap_or("default"))}
    else {None}
}
fn design(layout:&Value)->(f32,f32,&'static [&'static [usize]]) {
    match preset(layout) {None=>(3828.0,2217.0,DEFAULT),Some("dpad")=>(2804.0,1653.0,DPAD),Some("numpad")=>(2604.0,2373.0,NUMPAD),_=>(3048.0,2217.0,DEFAULT)}
}
pub fn panel_height_layout(width:f32,height:f32,layout:&Value)->f32 {
    let (w,h,_)=design(layout);((width-8.0).max(1.0)*h/w+8.0).min(height*0.48).max(0.0)
}
pub fn regions(width:f32,height:f32,layout:&Value)->Vec<Region>{
    if !width.is_finite()||!height.is_finite()||width<=0.0||height<=0.0{return Vec::new();}
    let (design_width,design_height,rows)=design(layout);
    let legacy=preset(layout).is_none();
    let panel=panel_height_layout(width,height,layout);
    let base=((width-8.0).max(0.01)/design_width).min((panel-8.0).max(0.01)/design_height);
    let origin=[(width-design_width*base)/2.0,height-panel+4.0];
    BUTTONS.iter().enumerate().map(|(i,(id,key,art))|{
        let entry=&layout[*id];
        let read=|field:&str|entry[field].as_f64().filter(|n|n.is_finite()).map(|n|n as f32);
        let multiplier=read("scale").unwrap_or(1.0).clamp(0.25,3.0);
        let size=(base*multiplier).min(width/708.0).min(height/405.0);
        let half=[708.0*size/2.0,405.0*size/2.0];
        let cell=rows.iter().enumerate().find_map(|(r,row)|row.iter().position(|id|*id==i).map(|c|(r,c)));
        let (row,col)=if legacy {(i/5,if i==20{2.0}else{(i%5) as f32})} else {
            cell.map(|(r,c)|(r,if r==0&&preset(layout)==Some("default"){c as f32*1.5}else{c as f32})).unwrap_or((rows.len()-1,0.0))
        };
        let (step_x,step_y)=match preset(layout){Some("dpad")=>(1048.0,624.0),Some("numpad")=>(948.0,492.0),_=>(780.0,453.0)};
        let mut center=[origin[0]+(col*step_x+354.0)*base,origin[1]+(row as f32*step_y+202.5)*base];
        if let Some(x)=read("x"){center[0]=x.clamp(0.0,1.0)*width;}
        if let Some(y)=read("y"){center[1]=y.clamp(0.0,1.0)*height;}
        for (axis,extent) in [width,height].into_iter().enumerate(){center[axis]=center[axis].clamp(half[axis],(extent-half[axis]).max(half[axis]));}
        Region{id,key:*key,art,min:[center[0]-half[0],center[1]-half[1]],max:[center[0]+half[0],center[1]+half[1]],visible:entry["visible"].as_bool().unwrap_or(legacy||cell.is_some())}
    }).collect()
}
pub fn reserved_height(width:f32,height:f32,layout:&Value)->f32 {
    if regions(width,height,layout).iter().any(|r|r.visible){panel_height_layout(width,height,layout)}else{0.0}
}
