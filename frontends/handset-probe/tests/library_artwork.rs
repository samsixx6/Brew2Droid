//! Arte emprestada depende de binário idêntico e de doador ainda presente.
#[path = "../../android/src/native_ui_model.rs"]
#[allow(dead_code,unused_imports)] mod model;
use std::path::{Path,PathBuf};
use std::sync::atomic::{AtomicU64,Ordering};
static NEXT:AtomicU64=AtomicU64::new(0);
struct Pasta(PathBuf);
impl Pasta {
 fn nova()->Self {let p=std::env::temp_dir().join(format!("brew-art-cache-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));std::fs::create_dir_all(p.join("games")).unwrap();Self(p)}
 fn modulo(&self,nome:&str,conteudo:&[u8],cor:Option<[u8;3]>)->PathBuf {
  let p=self.0.join("games").join(format!("{nome}.mod"));std::fs::write(&p,conteudo).unwrap();
  if let Some(cor)=cor {let mut b=vec![0;54];b[..2].copy_from_slice(b"BM");b[10]=54;b[14]=40;b[18]=1;b[22]=1;b[26]=1;b[28]=24;b.extend([cor[2],cor[1],cor[0],0]);std::fs::write(p.with_extension("bmp"),b).unwrap();}p
 }
 fn refresh(&self)->serde_json::Value {model::library_refresh(&self.0.join("games"),&self.0.join("art"),&self.0.join("profiles"),false).unwrap()}
}
impl Drop for Pasta {fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
fn linha<'a>(lista:&'a serde_json::Value,path:&Path)->&'a serde_json::Value {lista["games"].as_array().unwrap().iter().find(|r|r["path"].as_str()==path.to_str()).unwrap()}
#[test]
fn binario_identico_recupera_arte_sem_persistir_emprestimo_apos_remover_doador() {
 let p=Pasta::nova();let a=p.modulo("a",b"same binary",Some([20,80,160]));let b=p.modulo("b",b"same binary",None);let c=p.modulo("c",b"different binary",None);
 let lista=p.refresh();assert_eq!(linha(&lista,&b)["art"],linha(&lista,&a)["art"]);assert!(linha(&lista,&b)["art"].is_string());assert!(linha(&lista,&c)["art"].is_null());
 let quente=model::library_cached(&p.0.join("games"),&p.0.join("art")).unwrap();assert_eq!(linha(&quente,&b)["art"],linha(&lista,&a)["art"]);
 std::fs::remove_file(&a).unwrap();std::fs::remove_file(a.with_extension("bmp")).unwrap();let lista=p.refresh();assert!(linha(&lista,&b)["art"].is_null());
}
#[test]
fn artes_conflitantes_nao_sao_emprestadas_e_artes_iguais_nao_conflitam() {
 let p=Pasta::nova();p.modulo("a",b"same",Some([255,0,0]));p.modulo("b",b"same",Some([0,255,0]));let c=p.modulo("c",b"same",None);
 assert!(linha(&p.refresh(),&c)["art"].is_null());
 let b=p.0.join("games/b.mod");std::fs::remove_file(b.with_extension("bmp")).unwrap();p.modulo("d",b"same",Some([255,0,0]));
 assert!(linha(&p.refresh(),&c)["art"].is_string());
}
#[test]
fn extensao_confirmada_pelo_mif_e_ocultada_sem_excluir_o_modulo() {
 let p=Pasta::nova();let a=p.modulo("extension",b"extension",None);let mut mif=vec![0;48];mif[..4].copy_from_slice(&[17,0,1,0]);mif[16]=32;mif[20]=1;mif[32]=40;mif[36]=48;mif[40..44].copy_from_slice(&0x010292c3u32.to_le_bytes());std::fs::write(a.with_extension("mif"),mif).unwrap();
 assert!(p.refresh()["games"].as_array().unwrap().is_empty());assert!(a.exists());
}

#[test]
fn copia_sem_mif_de_extensao_identica_nao_vira_jogo_e_applet_explicito_tem_prioridade() {
 let p=Pasta::nova();let ext=p.modulo("extension",b"extension shared",None);let unknown=p.modulo("unknown",b"extension shared",None);
 let mut mif=vec![0;48];mif[..4].copy_from_slice(&[17,0,1,0]);mif[16]=32;mif[20]=1;mif[32]=40;mif[36]=48;mif[40..44].copy_from_slice(&0x010292c3u32.to_le_bytes());std::fs::write(ext.with_extension("mif"),mif).unwrap();
 assert!(p.refresh()["games"].as_array().unwrap().is_empty());assert!(unknown.exists());
 let app=p.modulo("applet",b"extension shared",None);let mut mif=vec![0;60];mif[..4].copy_from_slice(&[17,0,1,0]);mif[16]=32;mif[20]=1;mif[32]=40;mif[36]=60;mif[40..44].copy_from_slice(&0x01012345u32.to_le_bytes());std::fs::write(app.with_extension("mif"),mif).unwrap();
 let result=p.refresh();assert_eq!(result["games"].as_array().unwrap().len(),2);assert!(linha(&result,&app).is_object());assert!(linha(&result,&unknown).is_object());
 std::fs::remove_file(&ext).unwrap();std::fs::remove_file(ext.with_extension("mif")).unwrap();std::fs::remove_file(&app).unwrap();std::fs::remove_file(app.with_extension("mif")).unwrap();
 assert_eq!(p.refresh()["games"].as_array().unwrap().len(),1);
}
