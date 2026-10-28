//! A apresentação de ícones não altera os bitmaps consumidos pelo runtime.
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Pasta(PathBuf);
impl Pasta {
    fn nova() -> Self {
        let path = std::env::temp_dir().join(format!("brew-arte-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&path).unwrap(); Self(path)
    }
    fn modulo(&self, nome: &str, mif: Option<Vec<u8>>) -> PathBuf {
        let path = self.0.join(format!("{nome}.mod")); std::fs::write(&path, [0; 8]).unwrap();
        if let Some(mif) = mif { std::fs::write(path.with_extension("mif"), mif).unwrap(); } path
    }
}
impl Drop for Pasta { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }
fn mif(partes: &[Vec<u8>]) -> Vec<u8> {
    let inicio = 32 + (partes.len() + 1) * 4; let mut bytes = vec![0; inicio]; bytes[..4].copy_from_slice(&[17,0,1,0]);
    bytes[16..20].copy_from_slice(&32u32.to_le_bytes()); bytes[20..24].copy_from_slice(&(partes.len() as u32).to_le_bytes());
    for (i, parte) in partes.iter().enumerate() { let at = bytes.len() as u32; bytes[32+i*4..36+i*4].copy_from_slice(&at.to_le_bytes()); bytes.extend(parte); }
    let fim = bytes.len() as u32; bytes[32+partes.len()*4..36+partes.len()*4].copy_from_slice(&fim.to_le_bytes()); bytes
}
fn applet() -> Vec<u8> { let mut b = vec![0;20]; b[..4].copy_from_slice(&0x01012345u32.to_le_bytes()); b }
fn classe() -> Vec<u8> { let mut b = vec![0;8]; b[..4].copy_from_slice(&0x010292c3u32.to_le_bytes()); b }
fn bmp() -> Vec<u8> {
    // Magenta na borda e no centro isolado; verde ao redor do centro.
    let mut b=vec![0;54]; b[..2].copy_from_slice(b"BM"); b[10]=54;b[14]=40;b[18]=5;b[22]=5;b[26]=1;b[28]=24;
    for y in 0..5 { for x in 0..5 { b.extend(if x==0||x==4||y==0||y==4||(x==2&&y==2) {[255,0,255]}else{[0,255,0]}); } b.push(0); } b
}
fn imagem(raw: Vec<u8>) -> Vec<u8> { let mut b=vec![12,0]; b.extend(b"image/bmp\0");b.extend(raw);b }
#[test]
fn fundo_magenta_da_borda_do_mif_vira_alfa_sem_apagar_cor_interna() {
    let pasta=Pasta::nova();pasta.modulo("game",Some(mif(&[applet(),imagem(bmp())])));
    let arte=zeebx::library::scan(&pasta.0).remove(0).art.unwrap();
    assert_eq!(&arte.rgba[..4],&[255,0,255,0]);
    assert_eq!(&arte.rgba[48..52],&[255,0,255,255]);
    assert_eq!(&arte.rgba[24..28],&[0,255,0,255]);
}
#[test]
fn bmp_generico_e_capa_do_usuario_conservam_magenta_opaco() {
    let pasta=Pasta::nova();let path=pasta.modulo("game",None);std::fs::write(path.with_extension("bmp"),bmp()).unwrap();
    assert_eq!(zeebx::video::icon::decode(&bmp()).unwrap().rgba[3],255);
    assert_eq!(zeebx::library::scan(&pasta.0)[0].art.as_ref().unwrap().rgba[3],255);
}
#[test]
fn extensao_com_classes_sem_applet_nao_e_jogo() {
    let pasta=Pasta::nova();pasta.modulo("extension",Some(mif(&[classe()])));
    assert!(zeebx::library::scan(&pasta.0).is_empty());
}
#[test]
fn applet_com_dependencias_e_modulo_sem_mif_continuam_visiveis() {
    let pasta=Pasta::nova();pasta.modulo("game",Some(mif(&[applet(),classe()])));pasta.modulo("unknown",None);
    assert_eq!(zeebx::library::scan(&pasta.0).len(),2);
}
fn gif() -> Vec<u8> {
    // Um pixel transparente. O controle gráfico precede o primeiro quadro.
    vec![71,73,70,56,57,97,1,0,1,0,128,0,0,255,0,255,0,255,0,33,249,4,1,0,0,0,0,44,0,0,0,0,1,0,1,0,0,2,2,68,1,0,59]
}
#[test]
fn icone_gif_preserva_alfa_e_le_so_o_primeiro_quadro() {
    let mut bytes=gif();bytes.pop();bytes.extend([44,0,0,0,0,255,255,255,255,0]);
    let image=zeebx::video::icon::decode(&bytes).unwrap();assert_eq!((image.width,image.height),(1,1));assert_eq!(image.rgba[3],0);
}
#[test]
fn icone_gif_recusa_canvas_gigante_antes_de_alocar() {
    let mut bytes=gif();bytes[6..10].fill(255);assert!(zeebx::video::icon::decode(&bytes).is_err());
}

#[test]
fn icone_gif_limita_sub_blocos_antes_do_primeiro_quadro() {
    let base=gif();let mut bytes=base[..19].to_vec();bytes.extend([33,254]);
    for _ in 0..4200 { bytes.push(255);bytes.extend([b'x';255]); }
    bytes.push(0);bytes.extend(&base[19..]);
    assert!(zeebx::video::icon::decode(&bytes).is_err());
}

#[test]
fn fundo_magenta_com_variacao_de_paleta_na_borda_tambem_desaparece() {
    let pasta=Pasta::nova();let mut raw=bmp();
    for y in 0..5 { for x in 0..5 { if x==0||x==4||y==0||y==4 {raw[54+y*16+x*3+1]=8;} } }
    pasta.modulo("game",Some(mif(&[applet(),imagem(raw)])));
    let arte=zeebx::library::scan(&pasta.0).remove(0).art.unwrap();
    assert_eq!(&arte.rgba[..4],&[255,8,255,0]);assert_eq!(&arte.rgba[48..52],&[255,0,255,255]);
}
