use std::{path::PathBuf, sync::atomic::{AtomicU64, Ordering}};
use zeebx::{brew::aee::{self, Interface}, cpu::{BackendPadrao, CpuBackend}, loader::{self, modfile::ModImage}, machine::{Machine, Outcome}, storage::StoragePaths};

const CLASS:u32=0x0180_00fe;
const OUT:u32=loader::STACK_BASE+0x1000;
const RECORD:u32=OUT+0x100;
struct Fixture(PathBuf);
impl Fixture {
    fn new()->Self {
        static NEXT:AtomicU64=AtomicU64::new(0);
        let root=std::env::temp_dir().join(format!("brew-regional-model-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
        std::fs::create_dir_all(root.join("module")).unwrap();
        std::fs::create_dir(root.join("overlay")).unwrap();
        Self(root)
    }
    fn machine(&self,provider:Option<bool>)->Machine<BackendPadrao> {
        let mut code=vec![0u32;40];code[0]=0xe3a0_0000;code[1]=0xe12f_ff1e;
        if let Some(working)=provider {
            // Um IModule ARM próprio: Load escreve o módulo e CreateInstance entrega seu objeto.
            code[2..7].copy_from_slice(&[0xe59f_0008,0xe582_0000,if working{0xe3a0_0000}else{0xe3a0_0001},0xe12f_ff1e,loader::MODULE_BASE+0x60]);
            code[7..12].copy_from_slice(&[0xe59f_0008,0xe583_0000,0xe3a0_0000,0xe12f_ff1e,loader::MODULE_BASE+0x64]);
            code[0x60/4]=loader::MODULE_BASE+0x70;code[0x64/4]=loader::MODULE_BASE+0x80;
            code[0x70/4..0x7c/4].copy_from_slice(&[loader::MODULE_BASE+0x90,loader::MODULE_BASE+0x90,loader::MODULE_BASE+0x1c]);
            code[0x80/4..0x8c/4].fill(loader::MODULE_BASE+0x90);
            code[0x90/4..0x98/4].copy_from_slice(&[0xe3a0_0001,0xe12f_ff1e]);
        }
        let image=ModImage::parse(code.into_iter().flat_map(u32::to_le_bytes).collect()).unwrap();
        let mut loaded=loader::load_with(&image,&[]).unwrap();
        if provider.is_some(){loaded.extensions.push(loader::LoadedExtension{entry:loader::MODULE_BASE+8,out_module:loaded.out_module+8,classes:vec![CLASS]});}
        let storage=StoragePaths::from_root(self.0.join("runtime"));storage.create_dirs().unwrap();
        let mut m=Machine::new_with_storage(BackendPadrao::new().unwrap(),loaded,self.0.join("module"),&storage,Some(self.0.join("overlay")));
        assert_eq!(m.run(1000).unwrap(),Outcome::Returned{code:0});m
    }
}
impl Drop for Fixture {
    fn drop(&mut self){
        let root=self.0.canonicalize().unwrap();
        assert!(root.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(root.file_name().unwrap().to_string_lossy().starts_with("brew-regional-model-"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
fn shell(m:&mut Machine<BackendPadrao>,name:&str,args:[u32;3])->u32 {
    let slot=(0..256).find(|&s|Interface::Shell.method(s)==Some(name)).unwrap();
    match m.call_guest(aee::encode(Interface::Shell,slot),[loader::OBJECT_BASE,args[0],args[1],args[2]],10000).unwrap(){Outcome::Returned{code}=>code,o=>panic!("{name}: {o:?}")}
}
fn create(m:&mut Machine<BackendPadrao>)->u32 {
    assert_eq!(shell(m,"CreateInstance",[CLASS,OUT,0]),0,"a classe deve entregar somente a consulta de modelo verificada");
    m.cpu().read_u32(OUT).unwrap()
}
fn raw(m:&mut Machine<BackendPadrao>,object:u32,slot:u32,buffer:u32)->Outcome {
    let table=m.cpu().read_u32(object).unwrap();let method=m.cpu().read_u32(table+slot*4).unwrap();
    m.call_guest(method,[object,buffer,0xdead_beef,0xcafe_babe],10000).unwrap()
}
fn call(m:&mut Machine<BackendPadrao>,object:u32,slot:u32,buffer:u32)->u32 {
    match raw(m,object,slot,buffer){Outcome::Returned{code}=>code,o=>panic!("slot {slot}: {o:?}")}
}

#[test]
fn regional_query_writes_only_the_short_virtual_model_and_preserves_other_fields(){
    let f=Fixture::new();let mut m=f.machine(None);let object=create(&mut m);
    assert_eq!(shell(&mut m,"QueryClass",[CLASS,0,0]),1);
    m.cpu_mut().write_mem(RECORD,&[0xa5;48]).unwrap();
    assert_eq!(call(&mut m,object,2,RECORD+4),1);
    let bytes=m.dump(RECORD,48).unwrap();
    assert_eq!(&bytes[..4],&[0xa5;4]);assert_eq!(&bytes[4..12],b"B2Droid\0");
    assert_eq!(&bytes[12..],&[0xa5;36],"não preencher campos regionais não identificados");
    let table=m.cpu().read_u32(object).unwrap();let method=m.cpu().read_u32(table+8).unwrap();
    assert_eq!(aee::decode(method).unwrap().0 as u32,82,"vtable própria, sem QueryInterface no slot2");
    assert!(m.unknown_classes().is_empty());assert!(m.assumptions().is_empty());
}

#[test]
fn regional_query_rejects_null_unmapped_and_released_objects_without_writes(){
    let f=Fixture::new();let mut m=f.machine(None);let object=create(&mut m);
    assert_eq!(call(&mut m,object,2,0),0);
    assert_eq!(call(&mut m,object,2,0x7fff_fffc),0);
    m.cpu_mut().write_mem(RECORD,&[0x5a;48]).unwrap();
    assert_eq!(call(&mut m,object,1,0),0);
    assert_eq!(call(&mut m,object,2,RECORD),0);
    assert_eq!(m.dump(RECORD,48).unwrap(),[0x5a;48]);
}

#[test]
fn regional_query_validates_the_whole_output_before_writing_any_byte(){
    let f=Fixture::new();let mut m=f.machine(None);let object=create(&mut m);
    let readonly=m.dump(loader::VTABLE_BASE,8).unwrap();
    assert_eq!(call(&mut m,object,2,loader::VTABLE_BASE),0);
    assert_eq!(m.dump(loader::VTABLE_BASE,8).unwrap(),readonly);
    let tail=loader::STACK_BASE+loader::STACK_SIZE as u32-4;
    m.cpu_mut().write_mem(tail,&[0xa5;4]).unwrap();
    assert_eq!(call(&mut m,object,2,tail),0);
    assert_eq!(m.dump(tail,4).unwrap(),[0xa5;4],"nenhuma escrita parcial antes do fim do mapa");
}

#[test]
fn regional_factory_validates_null_readonly_and_unmapped_output_pointers(){
    let f=Fixture::new();let mut m=f.machine(None);
    let readonly=m.dump(loader::VTABLE_BASE,4).unwrap();
    for out in [0,loader::VTABLE_BASE,0x7fff_fffc]{
        assert_eq!(shell(&mut m,"CreateInstance",[CLASS,out,0]),14);
    }
    assert_eq!(m.dump(loader::VTABLE_BASE,4).unwrap(),readonly);
    assert_ne!(create(&mut m),0);
}

#[test]
fn regional_unknown_methods_remain_unimplemented_without_modifying_the_record(){
    let f=Fixture::new();let mut m=f.machine(None);let object=create(&mut m);
    m.cpu_mut().write_mem(RECORD,&[0x5a;48]).unwrap();
    assert!(matches!(raw(&mut m,object,3,RECORD),Outcome::Unimplemented{..}));
    assert_eq!(m.dump(RECORD,48).unwrap(),[0x5a;48]);
    assert!(!m.missing_apis().is_empty());
    assert_eq!(shell(&mut m,"CreateInstance",[0x0180_fffe,OUT,0]),20);
    assert_eq!(m.cpu().read_u32(OUT).unwrap(),0);
}

#[test]
fn regional_references_and_saved_objects_keep_normal_lifetimes(){
    let f=Fixture::new();let mut m=f.machine(None);let object=create(&mut m);
    assert_eq!(call(&mut m,object,0,0),2);let saved=m.grava_estado();
    assert_eq!(call(&mut m,object,1,0),1);assert_eq!(call(&mut m,object,1,0),0);
    m.restaura_estado(&saved).unwrap();
    assert_eq!(call(&mut m,object,2,RECORD),1);assert_eq!(call(&mut m,object,1,0),1);assert_eq!(call(&mut m,object,1,0),0);
}

#[test]
fn a_package_provider_keeps_precedence_over_the_virtual_model_query(){
    let f=Fixture::new();let mut m=f.machine(Some(true));
    assert_eq!(create(&mut m),loader::MODULE_BASE+0x64);
}

#[test]
fn a_failed_package_provider_is_not_replaced_by_a_successful_virtual_object(){
    let f=Fixture::new();let mut m=f.machine(Some(false));
    assert_eq!(shell(&mut m,"CreateInstance",[CLASS,OUT,0]),20);assert_eq!(m.cpu().read_u32(OUT).unwrap(),0);
}
