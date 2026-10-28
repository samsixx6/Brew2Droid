//! Contrato persistido dos ajustes e do layout; não depende de Android ou egui.
use serde_json::{Value,json};

pub const BUTTON_IDS: [&str;21] = ["soft_left","digit_1","digit_2","digit_3","soft_right",
    "up_left","digit_4","digit_5","digit_6","up_right","left","digit_7","digit_8","digit_9","right",
    "down_left","star","digit_0","pound","down_right","enter"];
pub const TARGETS:[&str;20]=["0","1","2","3","4","5","6","7","8","9","*","#","up","down","left","right","select","soft1","soft2","clear"];

pub const ZEEBO_TARGETS:[&str;19]=["b1","b2","b3","b4","zl","zr","home","up","down","left","right","lx-","lx+","ly-","ly+","rx-","rx+","ry-","ry+"];
pub fn button_type(raw:&Value,detected:bool)->&'static str {match raw["button_type"].as_str(){Some("zeebo")=>"zeebo",Some("cellphone")=>"cellphone",_=>if detected{"zeebo"}else{"cellphone"}}}
pub fn defaults()->Value {json!({"resolution":null,"orientation":"auto","gravity":"top","scaling":"fit",
    "keep_aspect":true,"smooth":false,"controls_enabled":true,"transparency":50,"volume_keys_mappable":false,"bindings":{},"zeebo_bindings":{},"button_type":"auto","layout":{}})}

fn token(value:&str)->bool {
    let parts:Vec<_>=value.split(':').collect();
    match parts.as_slice() {
        ["key",code]=>code.parse::<i32>().ok().is_some_and(|n|(1..=1023).contains(&n)&&![3,4,26,164,187,223,224].contains(&n)),
        ["key","4","scan",scan]=>scan.parse::<i32>().ok().is_some_and(|n|n>=0),
        ["axis",axis,sign]=>axis.parse::<i32>().ok().is_some_and(|n|[0,1,11,12,13,14,15,16,17,18,22,23].contains(&n))&&["+","-"].contains(sign),
        _=>false,
    }
}

fn field(name:&str,value:&Value)->Result<(),String> {
    let valid=match name {
        "resolution"=>value.is_null()||value.as_object().is_some_and(|v|v.keys().all(|k|["width","height","depth"].contains(&k.as_str()))
            &&["width","height"].iter().all(|k|v.get(*k).and_then(Value::as_u64).is_some_and(|n|(1..=1024).contains(&n)))
            &&v.get("depth").is_none_or(|v|v.as_u64()==Some(16))),
        "button_type"=>value.as_str().is_some_and(|v|["auto","cellphone","zeebo"].contains(&v)),
        "orientation"=>value.as_str().is_some_and(|v|["auto","portrait","landscape"].contains(&v)),
        "gravity"=>value.as_str().is_some_and(|v|["top","center","bottom"].contains(&v)),
        "scaling"=>value.as_str().is_some_and(|v|["fit","integer","stretch"].contains(&v)),
        "keep_aspect"|"smooth"|"controls_enabled"|"volume_keys_mappable"=>value.is_boolean(),
        "transparency"=>value.as_u64().is_some_and(|n|n<=100),
        "bindings"|"zeebo_bindings"=>{
            let targets:&[&str]=if name=="bindings"{&TARGETS}else{&ZEEBO_TARGETS};
            let mut used=std::collections::BTreeSet::new();
            value.as_object().is_some_and(|v|v.len()<=targets.len()&&v.iter().all(|(k,v)|targets.contains(&k.as_str())&&v.as_str().is_some_and(|s|s.len()<=64&&token(s)&&used.insert(s.to_owned()))))
        },
        "layout"=>value.as_object().is_some_and(|v|v.len()<=22&&v.iter().all(|(id,item)|if id=="_geometry" {
            item.as_object().is_some_and(|m|m.len()==2&&m.get("version").and_then(Value::as_u64)==Some(2)&&m.get("preset").and_then(Value::as_str).is_some_and(|p|["default","dpad","numpad"].contains(&p)))
        }else{BUTTON_IDS.contains(&id.as_str())&&item.as_object().is_some_and(|p|p.iter().all(|(k,v)|match k.as_str(){
            "x"|"y"=>v.as_f64().is_some_and(|n|n.is_finite()&&(0.0..=1.0).contains(&n)),
            "scale"=>v.as_f64().is_some_and(|n|n.is_finite()&&(0.25..=3.0).contains(&n)),
            "visible"=>v.is_boolean(),_=>false,
        }))})),
        _=>return Err(format!("Unsupported profile setting: {name}")),
    };
    if valid {Ok(())}else{Err(format!("Invalid profile setting: {name}"))}
}

pub fn normalize(raw:&Value)->Result<Value,String> {
    let object=raw.as_object().ok_or("Profile values must be an object")?;
    let mut result=defaults();
    for (name,value) in object {
        // Campos futuros continuam no arquivo; patches do aplicativo só aceitam os conhecidos.
        if result.get(name).is_some(){field(name,value)?;}
        result[name]=value.clone();
    }
    Ok(result)
}

pub fn patch(old:&Value,patch:&Value)->Result<Value,String> {
    let object=patch.as_object().ok_or("Profile patch must be an object")?;
    for (name,value) in object {field(name,value)?;}
    let mut result=normalize(old)?;
    for (name,value) in object {result[name]=value.clone();}
    Ok(result)
}
