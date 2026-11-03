fn indexed(version: u32, classes: &[u32], applet: bool) -> Vec<u8> {
    let count = if applet { 3usize } else { 2 };
    let table = 0x20 + count * 8;
    let start = table + (count + 1) * 4;
    let mut info = vec![0u8; 40];
    info[..4].copy_from_slice(&version.to_le_bytes());
    let at = if applet { 30 } else { 26 };
    info[at..at + 2].copy_from_slice(&(classes.len() as u16).to_le_bytes());
    let mut list: Vec<u8> = classes
        .iter()
        .chain(std::iter::once(&0))
        .flat_map(|id| id.to_le_bytes())
        .collect();
    let mut parts = vec![info, std::mem::take(&mut list)];
    if applet {
        let mut record = vec![0u8; 20];
        record[..4].copy_from_slice(&0x0101f197u32.to_le_bytes());
        parts.push(record);
    }
    let total = start + parts.iter().map(Vec::len).sum::<usize>();
    let mut data = vec![0u8; total];
    data[..2].copy_from_slice(&0x11u16.to_le_bytes());
    data[2..4].copy_from_slice(&1u16.to_le_bytes());
    data[6..8].copy_from_slice(&(count as u16).to_le_bytes());
    data[8..12].copy_from_slice(&0x20u32.to_le_bytes());
    data[12..16].copy_from_slice(&((count * 8) as u32).to_le_bytes());
    data[16..20].copy_from_slice(&(table as u32).to_le_bytes());
    data[20..24].copy_from_slice(&(count as u32).to_le_bytes());
    let ids = if applet {
        vec![0u16, 3, 2]
    } else {
        vec![0u16, 1]
    };
    let mut cursor = start;
    for (i, part) in parts.iter().enumerate() {
        let at = 0x20 + i * 8;
        data[at..at + 2].copy_from_slice(&0x5000u16.to_le_bytes());
        data[at + 2..at + 4].copy_from_slice(&ids[i].to_le_bytes());
        data[at + 6..at + 8].copy_from_slice(&(i as u16).to_le_bytes());
        data[table + i * 4..table + i * 4 + 4].copy_from_slice(&(cursor as u32).to_le_bytes());
        data[cursor..cursor + part.len()].copy_from_slice(part);
        cursor += part.len();
    }
    data[table + count * 4..table + count * 4 + 4].copy_from_slice(&(cursor as u32).to_le_bytes());
    data
}

struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new(imports: &[u32]) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "zeebx-extension-exports-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let f = Self(root);
        f.add("game", "game", imports, true);
        f
    }
    fn add(&self, folder: &str, name: &str, classes: &[u32], applet: bool) {
        let package = self.0.join(folder);
        let content = package.join(name);
        std::fs::create_dir_all(&content).unwrap();
        std::fs::write(
            content.join(format!("{name}.mod")),
            [0xe3a00000u32, 0xe12fff1e]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
        std::fs::write(
            package.join(format!("{name}.mif")),
            indexed(0x1002, classes, applet),
        )
        .unwrap();
    }
    fn providers(&self) -> Vec<(std::path::PathBuf, Vec<u32>)> {
        zeebx::library::extensoes(&self.0.join("game/game/game.mod"))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
fn provedor_selecionado_conserva_exportacoes_para_suas_fabricas_internas() {
    let f = Fixture::new(&[0x010179be]);
    f.add("a-provider", "bf2gen", &[0x010179be, 0x010179c6], false);
    f.add("z-unrelated", "other", &[0x01020001], false);
    let p = f.providers();
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].1, vec![0x010179be, 0x010179c6]);
}
#[test]
fn provedores_compartilhados_conservam_prioridade_deterministica_sem_duplicar_classes() {
    let f = Fixture::new(&[0x010179be, 0x010179c8]);
    f.add("a-provider", "first", &[0x010179be, 0x010179c6], false);
    f.add(
        "b-provider",
        "second",
        &[0x010179c8, 0x010179c6, 0x010179c0],
        false,
    );
    let p = f.providers();
    assert_eq!(p.len(), 2);
    assert_eq!(p[0].1, vec![0x010179be, 0x010179c6]);
    assert_eq!(p[1].1, vec![0x010179c8, 0x010179c0]);
}
