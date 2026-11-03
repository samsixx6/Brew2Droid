use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend, Reg},
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    storage::StoragePaths,
};
const NAME: u32 = loader::STACK_BASE + 0x1000;
const OUT: u32 = NAME + 0x400;
const COUNT: u32 = NAME + 0x800;
const REGISTRY: u32 = 0x0100_103e;
const PNG: u32 = 0x0100_4004;
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "brew-registry-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(p.join("module")).unwrap();
        std::fs::create_dir(p.join("overlay")).unwrap();
        Self(p)
    }
    fn machine(&self) -> Machine<BackendPadrao> {
        // O callback copia os campos públicos e o primeiro uint32 do MIME enquanto vivos.
        let callback = [
            0xe92d_4010u32,
            0xe590_2000,
            0xe282_3001,
            0xe580_3000,
            0xe280_0004,
            0xe080_0202,
            0xe591_2000,
            0xe591_3004,
            0xe591_4008,
            0xe594_4000,
            0xe591_c00c,
            0xe8a0_101c,
            0xe3a0_0001,
            0xe8bd_8010,
        ];
        let mut code = vec![0xe3a0_0000u32, 0xe12f_ff1e];
        code.extend(callback);
        let mut stop = callback;
        stop[12] = 0xe3a0_0000;
        code.extend(stop);
        let mut pointers = callback;
        pointers[9] = 0xe1a0_4004;
        code.extend(pointers);
        let image = ModImage::parse(code.into_iter().flat_map(u32::to_le_bytes).collect()).unwrap();
        let storage = StoragePaths::from_root(self.0.join("runtime"));
        storage.create_dirs().unwrap();
        let mut m = Machine::new_with_storage(
            BackendPadrao::new().unwrap(),
            loader::load_with(&image, &[]).unwrap(),
            self.0.join("module"),
            &storage,
            Some(self.0.join("overlay")),
        );
        assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
        m
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn shell(m: &mut Machine<BackendPadrao>, method: &str, args: [u32; 3]) -> u32 {
    let slot = (0..256)
        .find(|&s| Interface::Shell.method(s) == Some(method))
        .unwrap();
    match m
        .call_guest(
            aee::encode(Interface::Shell, slot),
            [loader::OBJECT_BASE, args[0], args[1], args[2]],
            10000,
        )
        .unwrap()
    {
        Outcome::Returned { code } => code,
        o => panic!("{method}: {o:?}"),
    }
}
fn create(m: &mut Machine<BackendPadrao>) -> u32 {
    assert_eq!(
        shell(m, "CreateInstance", [REGISTRY, OUT, 0]),
        0,
        "a fábrica deve expor a interface padrão real"
    );
    m.cpu().read_u32(OUT).unwrap()
}
fn call(m: &mut Machine<BackendPadrao>, o: u32, slot: u32, args: [u32; 3]) -> u32 {
    let table = m.cpu().read_u32(o).unwrap();
    let f = m.cpu().read_u32(table + slot * 4).unwrap();
    match m
        .call_guest(f, [o, args[0], args[1], args[2]], 10000)
        .unwrap()
    {
        Outcome::Returned { code } => code,
        outcome => panic!("slot {slot}: {outcome:?}"),
    }
}
fn text(m: &mut Machine<BackendPadrao>, s: &str) {
    m.cpu_mut()
        .write_mem(NAME, &[s.as_bytes(), &[0]].concat())
        .unwrap();
}
fn get(m: &mut Machine<BackendPadrao>, o: u32, base: u32, mime: &str) -> u32 {
    text(m, mime);
    call(m, o, 2, [base, NAME, 0])
}
fn set(m: &mut Machine<BackendPadrao>, o: u32, base: u32, mime: &str, class: u32) -> u32 {
    text(m, mime);
    call(m, o, 3, [base, NAME, class])
}
#[test]
fn registry_factory_queries_real_handlers_and_never_invents_celltop() {
    let f = Fixture::new();
    let mut m = f.machine();
    let o = create(&mut m);
    assert_eq!(shell(&mut m, "QueryClass", [REGISTRY, 0, 0]), 1);
    assert_eq!(get(&mut m, o, 0, "image/png"), PNG);
    assert_eq!(get(&mut m, o, 0x0100_4000, "image/png"), PNG);
    assert_eq!(get(&mut m, o, 0x0100_5500, "image/png"), 0);
    assert_eq!(get(&mut m, o, 2, "Celltop/alltel.frog.celltop"), 0);
}
#[test]
fn registry_set_uses_sdk_errors_and_owns_the_callers_mime_string() {
    let f = Fixture::new();
    let mut m = f.machine();
    let o = create(&mut m);
    assert_eq!(set(&mut m, o, 0, "image/png", PNG), 26);
    assert_eq!(set(&mut m, o, 0, "image/custom", 0x0180_fffe), 10);
    assert_eq!(call(&mut m, o, 3, [0, 0, PNG]), 14);
    assert_eq!(set(&mut m, o, 0, "image/custom", PNG), 0);
    text(&mut m, "overwritten");
    assert_eq!(get(&mut m, o, 0, "image/custom"), PNG);
    let other = create(&mut m);
    assert_eq!(call(&mut m, o, 1, [0; 3]), 0);
    assert_eq!(get(&mut m, other, 0, "image/custom"), PNG);
}
#[test]
fn registry_enum_is_synchronous_and_reports_real_public_item_layout() {
    let f = Fixture::new();
    let mut m = f.machine();
    let o = create(&mut m);
    m.cpu_mut().write_mem(COUNT, &[0; 4096]).unwrap();
    call(&mut m, o, 4, [loader::MODULE_BASE + 8, COUNT, 0]);
    let n = m.cpu().read_u32(COUNT).unwrap();
    assert!(n > 0 && n < 128);
    let mut saw_png = false;
    for i in 0..n {
        let at = COUNT + 4 + i * 16;
        let ty = m.cpu().read_u32(at).unwrap();
        let class = m.cpu().read_u32(at + 4).unwrap();
        let mime = m.cpu().read_u32(at + 8).unwrap();
        assert_eq!(m.cpu().read_u32(at + 12).unwrap(), 1);
        if ty == 0 && class == PNG {
            assert_eq!(mime, u32::from_le_bytes(*b"imag"));
            saw_png = true;
        }
    }
    assert!(saw_png);
}
#[test]
fn registry_enum_stops_immediately_when_guest_callback_returns_false() {
    let f = Fixture::new();
    let mut m = f.machine();
    let o = create(&mut m);
    m.cpu_mut().write_u32(COUNT, 0).unwrap();
    call(&mut m, o, 4, [loader::MODULE_BASE + 8 + 14 * 4, COUNT, 1]);
    assert_eq!(m.cpu().read_u32(COUNT).unwrap(), 1);
}
#[test]
fn explicit_registry_selection_persists_across_machine_restart() {
    let f = Fixture::new();
    {
        let mut m = f.machine();
        let o = create(&mut m);
        assert_eq!(set(&mut m, o, 0, "image/custom", PNG), 0);
    }
    let mut m = f.machine();
    let o = create(&mut m);
    assert_eq!(get(&mut m, o, 0, "image/custom"), PNG);
    assert_eq!(get(&mut m, o, 2, "Celltop/alltel.frog.celltop"), 0);
}
#[test]
fn registry_overrides_roundtrip_with_save_state() {
    let f = Fixture::new();
    let mut m = f.machine();
    let o = create(&mut m);
    let before = m.grava_estado();
    assert_eq!(set(&mut m, o, 0, "image/custom", PNG), 0);
    let after = m.grava_estado();
    m.restaura_estado(&before).unwrap();
    assert_eq!(get(&mut m, o, 0, "image/custom"), 0);
    m.restaura_estado(&after).unwrap();
    assert_eq!(get(&mut m, o, 0, "image/custom"), PNG);
}
#[test]
fn malformed_registry_snapshot_is_rejected_before_live_state_mutation() {
    let f = Fixture::new();
    let mut m = f.machine();
    let o = create(&mut m);
    assert_eq!(set(&mut m, o, 0, "image/custom", PNG), 0);
    let saved = m.grava_estado();
    let reader = zeebx::save_state::Leitor::abre(&saved).unwrap();
    let mut sections = zeebx::save_state::Secoes::nova();
    for name in reader.nomes() {
        if name != "tab.registry" {
            sections.poe(name, reader.secao(name).unwrap().to_vec());
        }
    }
    sections.poe("tab.registry", b"{malformed".to_vec());
    m.cpu_mut().write_reg(Reg::R4, 0x12345678);
    assert!(m.restaura_estado(&sections.fecha()).is_err());
    assert_eq!(m.cpu().read_reg(Reg::R4), 0x12345678);
    assert_eq!(get(&mut m, o, 0, "image/custom"), PNG);
}

#[test]
fn shell_handler_registration_and_registry_share_the_same_live_selection() {
    let f = Fixture::new();
    let mut m = f.machine();
    let o = create(&mut m);
    text(&mut m, "image/shared");
    assert_eq!(shell(&mut m, "RegisterHandler", [0, NAME, PNG]), 0);
    assert_eq!(get(&mut m, o, 0, "image/shared"), PNG);
    text(&mut m, "image/shared");
    assert_eq!(shell(&mut m, "GetHandler", [0, NAME, 0]), PNG);
    assert_eq!(shell(&mut m, "RegisterHandler", [0, NAME, PNG]), 26);
    assert_eq!(shell(&mut m, "RegisterHandler", [0, NAME, 0]), 0);
    assert_eq!(get(&mut m, o, 0, "image/shared"), 0);
}
#[test]
fn enum_mime_strings_remain_owned_after_the_callback_and_object_release() {
    let f = Fixture::new();
    let mut m = f.machine();
    let o = create(&mut m);
    m.cpu_mut().write_mem(COUNT, &[0; 4096]).unwrap();
    call(&mut m, o, 4, [loader::MODULE_BASE + 8 + 28 * 4, COUNT, 0]);
    let n = m.cpu().read_u32(COUNT).unwrap();
    let ptr = (0..n)
        .find_map(|i| {
            let at = COUNT + 4 + i * 16;
            (m.cpu().read_u32(at).unwrap() == 0 && m.cpu().read_u32(at + 4).unwrap() == PNG)
                .then(|| m.cpu().read_u32(at + 8).unwrap())
        })
        .unwrap();
    let before = m.dump(ptr, 10).unwrap();
    assert_eq!(before, b"image/png\0");
    let state = m.grava_estado();
    call(&mut m, o, 1, [0; 3]);
    assert_eq!(m.dump(ptr, 10).unwrap(), before);
    m.restaura_estado(&state).unwrap();
    assert_eq!(m.dump(ptr, 10).unwrap(), before);
    let o2 = create(&mut m);
    assert_eq!(get(&mut m, o2, 0, "image/png"), PNG);
}

fn malformed_cached_mime_snapshot(
    m: &Machine<BackendPadrao>,
    mutate: impl FnOnce(&mut serde_json::Value, &zeebx::save_state::Leitor<'_>),
) -> Vec<u8> {
    let saved = m.grava_estado();
    let reader = zeebx::save_state::Leitor::abre(&saved).unwrap();
    let mut registry: serde_json::Value =
        serde_json::from_slice(reader.secao("tab.registry").unwrap()).unwrap();
    mutate(&mut registry, &reader);
    let mut sections = zeebx::save_state::Secoes::nova();
    for name in reader.nomes() {
        if name != "tab.registry" {
            sections.poe(name, reader.secao(name).unwrap().to_vec());
        }
    }
    sections.poe("tab.registry", serde_json::to_vec(&registry).unwrap());
    sections.fecha()
}
#[test]
fn registry_restore_rejects_cached_mime_past_the_saved_heap_before_mutation() {
    let f = Fixture::new();
    let mut m = f.machine();
    let o = create(&mut m);
    m.cpu_mut().write_mem(COUNT, &[0; 4096]).unwrap();
    call(&mut m, o, 4, [loader::MODULE_BASE + 8, COUNT, 0]);
    let bad = malformed_cached_mime_snapshot(&m, |registry, reader| {
        registry["strings"]["image/png"] = serde_json::json!(
            loader::HEAP_BASE + reader.secao("mem.heap").unwrap().len() as u32 + 8
        );
    });
    m.cpu_mut().write_reg(Reg::R4, 0x12345678);
    assert!(m.restaura_estado(&bad).is_err());
    assert_eq!(m.cpu().read_reg(Reg::R4), 0x12345678);
    assert_eq!(get(&mut m, o, 0, "image/png"), PNG);
}
#[test]
fn registry_restore_rejects_cached_mime_pointing_to_different_saved_text() {
    let f = Fixture::new();
    let mut m = f.machine();
    let o = create(&mut m);
    m.cpu_mut().write_mem(COUNT, &[0; 4096]).unwrap();
    call(&mut m, o, 4, [loader::MODULE_BASE + 8, COUNT, 0]);
    let bad = malformed_cached_mime_snapshot(&m, |registry, _| {
        registry["strings"]["image/png"] = registry["strings"]["image/jpeg"].clone();
    });
    m.cpu_mut().write_reg(Reg::R4, 0x12345678);
    assert!(m.restaura_estado(&bad).is_err());
    assert_eq!(m.cpu().read_reg(Reg::R4), 0x12345678);
    assert_eq!(get(&mut m, o, 0, "image/png"), PNG);
}

#[test]
fn registry_enum_preserves_mime_spelling_while_lookup_ignores_ascii_case() {
    let f = Fixture::new();
    let mut m = f.machine();
    let o = create(&mut m);
    assert_eq!(set(&mut m, o, 0, "Vendor/Example", PNG), 0);
    assert_eq!(get(&mut m, o, 0, "vendor/example"), PNG);
    assert_eq!(set(&mut m, o, 0, "VENDOR/EXAMPLE", PNG), 26);
    m.cpu_mut().write_mem(COUNT, &[0; 4096]).unwrap();
    call(&mut m, o, 4, [loader::MODULE_BASE + 8 + 28 * 4, COUNT, 0]);
    let n = m.cpu().read_u32(COUNT).unwrap();
    let rows = (0..n)
        .map(|i| {
            let at = COUNT + 4 + i * 16;
            let p = m.cpu().read_u32(at + 8).unwrap();
            m.cpu().read_cstring(p, 1024)
        })
        .filter(|mime| mime.eq_ignore_ascii_case("Vendor/Example"))
        .collect::<Vec<_>>();
    assert_eq!(rows, ["Vendor/Example"]);
    let saved = m.grava_estado();
    m.restaura_estado(&saved).unwrap();
    assert_eq!(get(&mut m, o, 0, "VENDOR/EXAMPLE"), PNG);
    drop(m);
    let mut fresh = f.machine();
    let o = create(&mut fresh);
    assert_eq!(get(&mut fresh, o, 0, "vendor/example"), PNG);
    fresh.cpu_mut().write_mem(COUNT, &[0; 4096]).unwrap();
    call(
        &mut fresh,
        o,
        4,
        [loader::MODULE_BASE + 8 + 28 * 4, COUNT, 0],
    );
    let n = fresh.cpu().read_u32(COUNT).unwrap();
    assert!((0..n).any(|i| {
        let p = fresh.cpu().read_u32(COUNT + 4 + i * 16 + 8).unwrap();
        fresh.cpu().read_cstring(p, 1024) == "Vendor/Example"
    }));
}
