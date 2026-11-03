use std::path::PathBuf;
use zeebx::{brew::aee::{self,Interface},cpu::{BackendPadrao,CpuBackend},loader::{self,modfile::ModImage},machine::{Machine,Outcome},save_state::{Leitor,escreve},storage::StoragePaths};

struct Fixture {m:Machine<BackendPadrao>}
impl Fixture {
    fn new()->Self {
        static NEXT:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);
        let work=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local-tests").join(format!("realloc-{}-{}",std::process::id(),NEXT.fetch_add(1,std::sync::atomic::Ordering::Relaxed)));
        let storage=StoragePaths::from_root(work.join("runtime"));storage.create_dirs().unwrap();
        let image=ModImage::parse([0xe3a00000u32,0xe12fff1e].into_iter().flat_map(u32::to_le_bytes).collect()).unwrap();
        let mut m=Machine::new_with_storage(BackendPadrao::new().unwrap(),loader::load_with(&image,&[]).unwrap(),&work,&storage,None);
        assert_eq!(m.run(1000).unwrap(),Outcome::Returned{code:0});Self{m}
    }
    fn api(&mut self,name:&str,a:u32,b:u32)->u32 {
        let slot=(0..256).find(|s|Interface::Helpers.method(*s)==Some(name)).unwrap();
        match self.m.call_guest(aee::encode(Interface::Helpers,slot),[a,b,0,0],1000).unwrap(){Outcome::Returned{code}=>code,v=>panic!("{name}: {v:?}")}
    }
    fn read(&self,p:u32,n:usize)->Vec<u8>{let mut out=vec![0;n];self.m.cpu().read_mem(p,&mut out).unwrap();out}
    fn write(&mut self,p:u32,data:&[u8]){self.m.cpu_mut().write_mem(p,data).unwrap();}
}

#[test]
fn realoc_preserva_o_terminador_utf16_ao_montar_o_nome_da_missao(){
    let mut f=Fixture::new();let prefix:Vec<u8>="/missions/".encode_utf16().flat_map(u16::to_le_bytes).collect();
    let p=f.api("malloc",22,0);f.write(p,&prefix);let p=f.api("realloc",p,24);f.write(p+20,&[b'1',0]);
    assert_eq!(f.read(p+22,2),[0,0],"a expansão deve criar o terminador, não copiar a folga antiga");
    let p=f.api("realloc",p,32);let suffix:Vec<u8>=".trl".encode_utf16().flat_map(u16::to_le_bytes).collect();f.write(p+22,&suffix);
    let expected:Vec<u8>="/missions/1.trl\0".encode_utf16().flat_map(u16::to_le_bytes).collect();assert_eq!(f.read(p,32),expected);
}

#[test]
fn toda_expansao_zerada_preserva_so_os_bytes_logicos_anteriores(){
    let mut f=Fixture::new();
    for old in 1..=33u32 {for extra in 1..=9u32 {
        let p=f.api("malloc",old,0);let initial=vec![0x5a;old as usize];f.write(p,&initial);
        let q=f.api("realloc",p,old+extra);let mut expected=initial;expected.resize((old+extra)as usize,0);
        assert_eq!(f.read(q,expected.len()),expected,"pedido {old} → {}",old+extra);f.api("free",q,0);
    }}
}

#[test]
fn realoc_com_no_zmem_reduz_sem_escrever_no_bloco_vizinho(){
    let mut f=Fixture::new();let hole=f.api("malloc",8,0);let neighbor=f.api("malloc",8,0);f.write(neighbor,&[0x6d;8]);
    let p=f.api("malloc",40,0);f.write(p,&[0x7c;40]);f.api("free",hole,0);
    let q=f.api("realloc",p,0x80000001);assert_eq!(f.read(q,1),[0x7c]);assert_eq!(f.read(neighbor,8),[0x6d;8],"a flag não pode virar parte do tamanho copiado");
}

#[test]
fn err_realoc_respeita_o_tamanho_mascarado_e_atualiza_zero_com_flag(){
    let mut f=Fixture::new();let hole=f.api("malloc",8,0);let neighbor=f.api("malloc",8,0);f.write(neighbor,&[0x6d;8]);
    let p=f.api("malloc",40,0);f.write(p,&[0x7c;40]);f.api("free",hole,0);let output=loader::STACK_BASE+0x400;f.m.cpu_mut().write_u32(output,p).unwrap();
    assert_eq!(f.api("err_realloc",0x80000001,output),0);let q=f.m.cpu().read_u32(output).unwrap();assert_eq!(f.read(q,1),[0x7c]);assert_eq!(f.read(neighbor,8),[0x6d;8]);
    assert_eq!(f.api("err_realloc",0x80000000,output),0);assert_eq!(f.m.cpu().read_u32(output).unwrap(),0);
}

#[test]
fn realoc_zero_com_flag_libera_e_falha_de_expansao_preserva_o_original(){
    let mut f=Fixture::new();let p=f.api("malloc",22,0);f.write(p,&[0x4b;22]);
    assert_eq!(f.api("realloc",p,0x7fffffff),0);assert_eq!(f.read(p,22),[0x4b;22]);
    assert_eq!(f.api("realloc",p,0x80000000),0);assert_eq!(f.api("malloc",22,0),p);
}

#[test]
fn limite_logico_e_preservado_apos_restaurar_estado(){
    let mut f=Fixture::new();let p=f.api("malloc",22,0);f.write(p,&[0x34;22]);let saved=f.m.grava_estado();
    let q=f.api("realloc",p,64);f.write(q,&[0x22;64]);f.m.restaura_estado(&saved).unwrap();
    let q=f.api("realloc",p,24);let mut expected=vec![0x34;22];expected.extend_from_slice(&[0,0]);assert_eq!(f.read(q,24),expected);
}

#[test]
fn metadado_logico_invalido_e_recusado_sem_aplicar_o_estado(){
    let mut f=Fixture::new();let p=f.api("malloc",22,0);let saved=f.m.grava_estado();let reader=Leitor::abre(&saved).unwrap();
    let base:Vec<(String,Vec<u8>)>=reader.nomes().into_iter().filter(|n|*n!="heap.requested").map(|n|(n.into(),reader.secao(n).unwrap().to_vec())).collect();
    f.write(p,&[0x55;22]);let before=f.m.grava_estado();
    for words in [vec![2u32,p,u32::MAX],vec![2,p+1,1],vec![4,p,1,p,2]] {
        let mut sections=base.clone();let invalid=words.into_iter().flat_map(u32::to_le_bytes).collect();sections.push(("heap.requested".into(),invalid));
        match f.m.restaura_estado(&escreve(&sections)) {
            Err(zeebx::save_state::Erro::Secao{nome,motivo})=>{assert_eq!(nome,"heap.requested");assert!(motivo.contains("lógico"),"{motivo}");},
            other=>panic!("metadado lógico devia ser recusado: {other:?}"),
        }
        assert_eq!(f.m.grava_estado(),before,"recusar antes de alterar memória ou livros");
    }
}

#[test]
fn estados_anteriores_sem_tamanhos_logicos_continuam_legiveis(){
    let mut f=Fixture::new();let p=f.api("malloc",22,0);f.write(p,&[0x31;22]);let saved=f.m.grava_estado();let reader=Leitor::abre(&saved).unwrap();
    let sections:Vec<_>=reader.nomes().into_iter().filter(|n|!n.ends_with(".requested")).map(|n|(n.into(),reader.secao(n).unwrap().to_vec())).collect();
    f.m.restaura_estado(&escreve(&sections)).unwrap();assert_eq!(f.read(p,22),[0x31;22]);
    let q=f.api("malloc",22,0);let q=f.api("realloc",q,24);assert_eq!(f.read(q,24),[0;24]);
}
