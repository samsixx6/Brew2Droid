//! Pacotes originais opacos: fatos de abertura não autorizam alterar o MIF.
use std::{path::{Path, PathBuf}, sync::atomic::{AtomicU64, Ordering}};
use zeebx::{handset::{launch, package}, library};
static NEXT: AtomicU64 = AtomicU64::new(0);
const CASES: &[(&str,u32)] = &[
    ("asphalt3",0x0105b08c),("assassinscreed2_3d",0x010911f2),
    ("avp3d",0x01034789),("bio3d",0x0109005b),("dmc_mas",0x01070e36),
    ("doom2rpg",0x0109010f),("doomrpg",0x01035893),("ducati_3d",0x01039966),
    ("duke3d",0x01034b16),("dukemp",0x010b88f1),("ff53d",0x01072750),
    ("fox_moto_3d",0x01039a81),("gof",0x62edaeba),("helistrike",0x12324eab),
    ("nascar09",0x010751a5),("nfs09",0x01074912),("nfs10",0x0109008d),
    ("nfs11",0x0109f7c4),("nfs12",0x010fa44d),("nfsps",0x01061194),
    ("orcsandelves2",0x010619c3),("predator3d",0x01085689),
    ("tekken2",0x010882b9),("wolfensteinrpg",0x01081949),
];
fn source(name:&str)->PathBuf {
    PathBuf::from(std::env::var_os("BREW_OPAQUE_ROOT").expect("BREW_OPAQUE_ROOT contém os 25 diretórios originais"))
        .join(name).join(format!("{name}.mod"))
}
struct Fixture { root:PathBuf, module:PathBuf }
impl Fixture {
    fn new(name:&str)->Self {
        let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("local-tests").join(format!("opaque-package-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
        let module=root.join("collection/game/renamed.mod");std::fs::create_dir_all(module.parent().unwrap()).unwrap();
        let original=source(name);std::fs::copy(&original,&module).unwrap();std::fs::copy(original.with_extension("mif"),module.with_extension("mif")).unwrap();
        Self {root,module}
    }
    fn edit(&self,mif:bool) {
        let file=if mif {self.module.with_extension("mif")} else {self.module.clone()};
        let mut b=std::fs::read(&file).unwrap();let last=b.len()-1;b[last]^=1;std::fs::write(file,b).unwrap();
    }
}
fn manifest(class:u32,applet:bool)->Vec<u8> {
    let size=if applet {20} else {8};let mut b=vec![0u8;40+size];b[..4].copy_from_slice(&[0x11,0,1,0]);
    for (at,value) in [(16,32),(20,1),(32,40),(36,(40+size)as u32),(40,class)] {b[at..at+4].copy_from_slice(&value.to_le_bytes());}
    if applet {b[48..52].copy_from_slice(&1u32.to_le_bytes());} b
}

#[test]
#[ignore="requer os pares originais de BREW_OPAQUE_ROOT"]
fn original_pairs_open_without_rewriting_or_parsing_the_opaque_manifest() {
    for &(name,class) in CASES {
        let module=source(name);let before=std::fs::read(&module).unwrap();let mif=std::fs::read(module.with_extension("mif")).unwrap();
        assert!(package::parse_mif(&mif).is_err());
        let p=launch::read(&module).unwrap_or_else(|e|panic!("{name}: {e}"));
        assert_eq!(p.applet_class,class,"{name}");let m=p.manifest.unwrap();assert!(!m.parsed);assert_eq!(m.bytes,mif);
        assert_eq!(std::fs::read(&module).unwrap(),before);assert_eq!(std::fs::read(module.with_extension("mif")).unwrap(),mif);
        assert_eq!(library::applet_clsid(&module),Some(class),"{name}: catálogo e abertura devem concordar");
    }
}
#[test]
#[ignore="requer os pares originais de BREW_OPAQUE_ROOT"]
fn recognized_pairs_still_work_when_renamed_or_using_a_shared_manifest() {
    let f=Fixture::new("bio3d");let bytes=std::fs::read(f.module.with_extension("mif")).unwrap();
    assert_eq!(launch::read(&f.module).unwrap().applet_class,0x0109005b);
    let shared=f.root.join("Mif/renamed.mif");std::fs::create_dir_all(shared.parent().unwrap()).unwrap();
    let local=f.module.with_extension("mif");std::fs::rename(&local,&shared).unwrap();
    let moddir=f.root.join("Mod/game");std::fs::create_dir_all(&moddir).unwrap();let moved=moddir.join("renamed.mod");std::fs::rename(&f.module,&moved).unwrap();
    assert_eq!(launch::read(&moved).unwrap().applet_class,0x0109005b);assert_eq!(std::fs::read(&shared).unwrap(),bytes);assert!(!local.exists());
}
#[test]
#[ignore="requer os pares originais de BREW_OPAQUE_ROOT"]
fn changing_either_member_or_pairing_another_games_manifest_does_not_inherit_identity() {
    for mif in [false,true] {let f=Fixture::new("bio3d");f.edit(mif);assert!(launch::read(&f.module).is_err());assert_eq!(library::applet_clsid(&f.module),None);}
    let f=Fixture::new("bio3d");std::fs::copy(source("doomrpg").with_extension("mif"),f.module.with_extension("mif")).unwrap();assert!(launch::read(&f.module).is_err());
}
#[test]
#[ignore="requer os pares originais de BREW_OPAQUE_ROOT"]
fn readable_manifest_keeps_its_own_declared_class_and_ambiguous_manifests_remain_errors() {
    let f=Fixture::new("bio3d");std::fs::write(f.module.with_extension("mif"),manifest(0x23456789,true)).unwrap();
    let p=launch::read(&f.module).unwrap();assert_eq!(p.applet_class,0x23456789);assert!(p.manifest.unwrap().parsed);
    let f=Fixture::new("bio3d");let bytes=std::fs::read(f.module.with_extension("mif")).unwrap();
    let dir=f.root.join("Mod/game");std::fs::create_dir_all(&dir).unwrap();let module=dir.join("renamed.mod");std::fs::copy(&f.module,&module).unwrap();
    for (n,b) in [("one",bytes),("two",manifest(0x23456789,true))] {let p=f.root.join("Mif").join(n).join("renamed.mif");std::fs::create_dir_all(p.parent().unwrap()).unwrap();std::fs::write(p,b).unwrap();}
    assert!(launch::read(&module).err().unwrap().to_string().contains("Several different MIF"));
}
#[test]
#[ignore="requer os pares originais de BREW_OPAQUE_ROOT"]
fn verified_imports_find_a_deep_shared_provider_but_an_edited_pair_does_not() {
    let f=Fixture::new("dmc_mas");let provider=f.root.join("collection/runtime/deep/engine/provider.mod");std::fs::create_dir_all(provider.parent().unwrap()).unwrap();
    std::fs::write(&provider,[0xe3a00000u32,0xe12fff1e].into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>()).unwrap();
    std::fs::write(provider.with_extension("mif"),manifest(0x010292c3,false)).unwrap();
    let found=library::extensoes(&f.module);assert!(found.iter().any(|(p,c)|p==&provider && c.contains(&0x010292c3)),"a importação real deve alcançar o provedor compartilhado");
    f.edit(true);assert!(library::extensoes(&f.module).is_empty());
}
#[test]
#[ignore="requer os pares originais de BREW_OPAQUE_ROOT"]
fn shared_manifest_imports_use_the_same_manifest_selected_by_launch() {
    let f=Fixture::new("dmc_mas");
    let module=f.root.join("collection/package/Mod/game/renamed.mod");
    let mif=f.root.join("collection/package/Mif/renamed.mif");
    for p in [&module,&mif] {std::fs::create_dir_all(p.parent().unwrap()).unwrap();}
    std::fs::copy(&f.module,&module).unwrap();std::fs::copy(f.module.with_extension("mif"),&mif).unwrap();
    let provider=f.root.join("collection/runtime/engine/provider.mod");std::fs::create_dir_all(provider.parent().unwrap()).unwrap();
    std::fs::write(&provider,[0xe3a00000u32,0xe12fff1e].into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>()).unwrap();
    std::fs::write(provider.with_extension("mif"),manifest(0x010292c3,false)).unwrap();
    assert_eq!(launch::read(&module).unwrap().applet_class,0x01070e36);
    assert!(library::extensoes(&module).iter().any(|(p,c)|p==&provider && c.contains(&0x010292c3)));
}
#[test]
#[ignore="requer os pares originais de BREW_OPAQUE_ROOT"]
fn unaudited_orcs_variant_is_not_assigned_an_invented_class() {
    let f=Fixture::new("orcsandelves");assert!(launch::read(&f.module).is_err());assert_eq!(library::applet_clsid(&f.module),None);
}
