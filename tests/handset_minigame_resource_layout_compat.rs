use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
use zeebx::{
    cpu::{BackendPadrao, CpuBackend, Reg},
    handset::types::DisplayGeometry,
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    storage::StoragePaths,
};
const MODULE: &str = "9bc21120c00d32dc53f30bcc4fc09d381af60e6cc9e854d9ead7e45cd377ec0c";
const MIF: &str = "b553d4da8e10a279162e7d6cb74eaaff29ca26b7eda17a1cdbcd8c29cd3bf434";
const START: u32 = 0x27a40;
const ORIGINAL: &[u8] = &[
    240, 181, 6, 28, 107, 32, 64, 1, 53, 24, 40, 137, 127, 33, 137, 1, 143, 25, 202, 40, 142, 176,
    1, 208, 207, 40, 17, 209, 14, 32, 248, 97, 22, 32, 56, 98, 33, 32, 184, 98, 107, 137, 19, 33,
    21, 59, 9, 147, 168, 137, 5, 145, 24, 33, 14, 56, 7, 144, 26, 34, 8, 145, 14, 224, 0, 32, 248,
    97, 56, 98, 184, 98, 107, 137, 12, 33, 16, 59, 9, 147, 168, 137, 5, 145, 11, 33, 9, 56, 7, 144,
    21, 34, 8, 145, 169, 72, 6, 146, 48, 24, 192, 121, 65, 33, 137, 1, 140, 25, 1, 40, 6, 209, 255,
    35, 17, 51, 0, 34, 0, 33, 32, 106, 233, 247, 219, 254, 161, 72, 48, 56, 50, 24, 208, 107, 129,
    30, 15, 41, 56, 216, 8, 28, 0, 41, 0, 218, 3, 48, 128, 16, 0, 41, 4, 218, 73, 66, 137, 7, 137,
    15, 73, 66, 1, 224, 137, 7, 137, 15, 0, 41, 5, 209, 58, 106, 249, 105, 58, 50, 29, 49, 65, 67,
    230, 224, 1, 41, 5, 209, 58, 106, 249, 105, 58, 50, 29, 49, 65, 67, 210, 224, 2, 41, 5, 209,
    58, 106, 249, 105, 58, 50, 29, 49, 65, 67, 190, 224, 3, 41, 0, 208, 229, 224, 251, 105, 58,
    106, 29, 51, 25, 28, 65, 67, 184, 106, 58, 50, 76, 48, 2, 146, 0, 144, 1, 145, 96, 106, 0, 34,
    3, 240, 242, 250, 213, 224, 18, 40, 23, 209, 99, 106, 216, 105, 2, 6, 88, 106, 153, 106, 18,
    14, 4, 146, 2, 144, 3, 145, 25, 107, 26, 105, 0, 145, 1, 146, 88, 107, 217, 106, 3, 104, 58,
    106, 159, 105, 58, 50, 11, 28, 0, 33, 7, 240, 245, 252, 187, 224, 20, 40, 23, 209, 163, 106,
    216, 105, 2, 6, 88, 106, 153, 106, 18, 14, 4, 146, 2, 144, 3, 145, 25, 107, 26, 105, 0, 145, 1,
    146, 88, 107, 217, 106, 3, 104, 58, 106, 159, 105, 58, 50, 11, 28, 0, 33, 7, 240, 219, 252,
    161, 224, 21, 40, 23, 209, 99, 106, 216, 105, 2, 6, 88, 106, 153, 106, 18, 14, 4, 146, 2, 144,
    3, 145, 25, 107, 26, 105, 0, 145, 1, 146, 88, 107, 217, 106, 3, 104, 58, 106, 159, 105, 58, 50,
    11, 28, 0, 33, 7, 240, 193, 252, 135, 224, 22, 40, 23, 209, 163, 106, 216, 105, 2, 6, 88, 106,
    153, 106, 18, 14, 4, 146, 2, 144, 3, 145, 25, 107, 26, 105, 0, 145, 1, 146, 88, 107, 217, 106,
    3, 104, 58, 106, 159, 105, 58, 50, 11, 28, 0, 33, 7, 240, 167, 252, 109, 224, 23, 40, 23, 209,
    99, 106, 216, 105, 2, 6, 88, 106, 153, 106, 18, 14, 4, 146, 2, 144, 3, 145, 25, 107, 26, 105,
    0, 145, 1, 146, 88, 107, 217, 106, 3, 104, 58, 106, 159, 105, 58, 50, 11, 28, 0, 33, 7, 240,
    141, 252, 83, 224, 193, 31, 19, 57, 11, 41, 65, 216, 3, 32, 7, 240, 144, 253, 3, 34, 22, 26, 3,
    46, 10, 145, 20, 218, 58, 106, 112, 28, 58, 50, 13, 146, 251, 105, 29, 51, 25, 28, 65, 67, 3,
    49, 12, 145, 32, 106, 11, 144, 64, 136, 2, 146, 13, 154, 15, 56, 0, 144, 11, 152, 1, 145, 3,
    240, 78, 250, 10, 153, 0, 41, 8, 209, 58, 106, 248, 105, 58, 50, 29, 48, 112, 67, 1, 28, 3, 49,
    96, 107, 22, 224, 10, 153, 1, 41, 8, 209, 58, 106, 248, 105, 58, 50, 29, 48, 112, 67, 1, 28, 3,
    49, 32, 107, 10, 224, 10, 153, 2, 41, 23, 209, 58, 106, 248, 105, 58, 50, 29, 48, 112, 67, 1,
    28, 224, 106, 3, 49, 0, 35, 6, 224, 38, 40, 7, 209, 255, 35, 17, 51, 0, 34, 32, 106, 0, 33,
    233, 247, 202, 253, 3, 224, 40, 40, 1, 209, 0, 35, 211, 99, 168, 137, 5, 153, 255, 35, 66, 26,
    17, 51, 0, 33, 224, 107, 233, 247, 188, 253, 6, 34, 2, 146, 3, 33, 7, 152, 1, 145, 0, 144, 224,
    104, 0, 34, 0, 33, 9, 155, 233, 247, 146, 254, 3, 33, 1, 145, 6, 34, 9, 155, 2, 146, 7, 152,
    155, 24, 0, 34, 0, 144, 224, 104, 4, 33, 233, 247, 133, 254, 160, 107, 8, 153, 6, 154, 0, 35,
    233, 247, 157, 253, 14, 176, 240, 188, 8, 188, 24, 71, 0, 0, 240, 30, 0, 0,
];
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn root() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "brew-mini-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    root
}
fn machine(root: &Path, context: &[u8]) -> Machine<BackendPadrao> {
    let mut data = vec![0; 0x20000];
    data[..8].copy_from_slice(&[0, 0, 0xa0, 0xe3, 0x1e, 0xff, 0x2f, 0xe1]);
    data[0x17a40..0x17d48].copy_from_slice(context);
    // O ramo pequeno cai no despacho no mesmo bloco JIT. Um destino de parada
    // fora da funcao verificada recebe essa chamada sem simular nenhum desenho.
    data[0x1874..0x1876].copy_from_slice(&[0xfe, 0xe7]);
    let image = ModImage::parse(data).unwrap();
    let storage = StoragePaths::from_root(root.join("runtime"));
    let mut m = Machine::new_with_storage_geometry(
        BackendPadrao::new().unwrap(),
        loader::load_with(&image, &[]).unwrap(),
        root,
        &storage,
        None,
        DisplayGeometry {
            width: 240,
            height: 320,
            depth: 16,
        },
    )
    .unwrap();
    assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
    m
}
fn original_asset(root: &Path) {
    let source = std::env::var_os("BREW_MINI_ASSET")
        .expect("BREW_MINI_ASSET must name original mini2005.mbr");
    std::fs::copy(source, root.join("mini2005.mbr")).unwrap();
}
fn layout(m: &mut Machine<BackendPadrao>) -> [u32; 3] {
    let app = loader::HEAP_BASE;
    let context = app + 0xd60;
    for (at, value) in [(8, 207u16), (10, 240), (12, 320)] {
        m.cpu_mut()
            .write_mem(context + at, &value.to_le_bytes())
            .unwrap();
    }
    // Executa o prefixo ARM/Thumb real ate o despacho, sem modificar suas
    // instrucoes; conservar o JIT permite testar a troca em codigo ja compilado.
    let regs = [
        Reg::R0,
        Reg::R1,
        Reg::R2,
        Reg::R3,
        Reg::R4,
        Reg::R5,
        Reg::R6,
        Reg::R7,
        Reg::R8,
        Reg::R9,
        Reg::R10,
        Reg::R11,
        Reg::R12,
        Reg::Sp,
        Reg::Lr,
        Reg::Pc,
    ];
    let saved = regs.map(|r| m.cpu().read_reg(r));
    let cpsr = m.cpu().cpsr();
    m.cpu_mut().write_reg(Reg::R0, app);
    let mut pc = START | 1;
    let mut reached = false;
    for _ in 0..1000 {
        assert_eq!(
            m.cpu_mut().run(pc, 1).unwrap(),
            zeebx::cpu::StopReason::Budget
        );
        pc = m.cpu().read_reg(Reg::Pc);
        if pc & !1 == 0x27a9e || pc & !1 == 0x11874 {
            reached = true;
            break;
        }
        pc |= 1;
    }
    assert!(reached, "reached original sprite-dispatch boundary");
    for (r, v) in regs.into_iter().zip(saved) {
        m.cpu_mut().write_reg(r, v);
    }
    m.cpu_mut().set_cpsr(cpsr);
    [28, 32, 40]
        .map(|off| u32::from_le_bytes(m.dump(app + 0x1fc0 + off, 4).unwrap().try_into().unwrap()))
}
#[test]
fn original_height_category_selects_the_incompatible_large_layout() {
    let root = root();
    let mut m = machine(&root, ORIGINAL);
    assert_eq!(layout(&mut m), [14, 22, 33]);
}
#[test]
fn absent_wrong_and_oversized_resources_cannot_enable_the_port() {
    for kind in 0..3 {
        let root = root();
        if kind > 0 {
            std::fs::write(
                root.join("mini2005.mbr"),
                vec![0; if kind == 1 { 74955 } else { 74956 }],
            )
            .unwrap();
        }
        let mut m = machine(&root, ORIGINAL);
        assert!(!m.set_handset_runtime_identity(MODULE, MIF));
        assert_eq!(m.dump(START, 776).unwrap(), ORIGINAL);
    }
}
#[cfg(windows)]
#[test]
fn an_unreadable_locked_resource_cannot_enable_the_port() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = root();
    let path = root.join("mini2005.mbr");
    std::fs::write(&path, vec![0; 74955]).unwrap();
    let _locked = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(path)
        .unwrap();
    let mut m = machine(&root, ORIGINAL);
    assert!(!m.set_handset_runtime_identity(MODULE, MIF));
    assert_eq!(m.dump(START, 776).unwrap(), ORIGINAL);
}
#[test]
#[ignore = "requires original mini2005.mbr via BREW_MINI_ASSET"]
fn exact_pair_and_original_resource_use_the_existing_small_asset_layout() {
    let root = root();
    original_asset(&root);
    let mut m = machine(&root, ORIGINAL);
    assert_eq!(layout(&mut m), [14, 22, 33]);
    assert!(m.set_handset_runtime_identity(MODULE, MIF));
    assert_eq!(layout(&mut m), [0, 0, 0]);
    let before = m.dump(START, 776).unwrap();
    assert!(m.set_handset_runtime_identity(MODULE, MIF));
    assert_eq!(m.dump(START, 776).unwrap(), before);
}
#[test]
#[ignore = "requires original mini2005.mbr via BREW_MINI_ASSET"]
fn both_hashes_and_every_function_context_byte_are_required() {
    let root = root();
    original_asset(&root);
    for (module, mif) in [(MODULE, "unknown"), ("unknown", MIF)] {
        let mut m = machine(&root, ORIGINAL);
        assert!(!m.set_handset_runtime_identity(module, mif));
    }
    for pos in 0..776 {
        let mut bad = ORIGINAL.to_vec();
        bad[pos] ^= 0x10;
        let mut m = machine(&root, &bad);
        assert!(!m.set_handset_runtime_identity(MODULE, MIF), "byte {pos}");
        assert_eq!(m.dump(START, 776).unwrap(), bad);
    }
}
fn rewrite_save(m: &Machine<BackendPadrao>, change: impl FnOnce(&mut [u8], &mut [u8])) -> Vec<u8> {
    use zeebx::save_state::{self, Leitor};
    let state = m.grava_estado();
    let r = Leitor::abre(&state).unwrap();
    let base = m
        .module()
        .mem
        .regions()
        .iter()
        .find(|r| r.name == "module")
        .unwrap()
        .base;
    let mut code = r.secao("mem.module").unwrap().to_vec();
    let mut regs = r.secao("cpu.regs").unwrap().to_vec();
    change(
        &mut code[(START - base) as usize..(START - base) as usize + 776],
        &mut regs,
    );
    let parts = r
        .nomes()
        .into_iter()
        .map(|n| {
            (
                n.to_owned(),
                if n == "mem.module" {
                    code.clone()
                } else if n == "cpu.regs" {
                    regs.clone()
                } else {
                    r.secao(n).unwrap().to_vec()
                },
            )
        })
        .collect::<Vec<_>>();
    save_state::escreve(&parts)
}
#[test]
#[ignore = "requires original mini2005.mbr via BREW_MINI_ASSET"]
fn original_saves_upgrade_and_changed_or_incompatible_saves_reject_atomically() {
    let root = root();
    original_asset(&root);
    let mut m = machine(&root, ORIGINAL);
    assert!(m.set_handset_runtime_identity(MODULE, MIF));
    let original = rewrite_save(&m, |c, _| c.copy_from_slice(ORIGINAL));
    m.restaura_estado(&original).unwrap();
    assert_eq!(m.dump(START + 22, 2).unwrap(), 0xe013u16.to_le_bytes());
    let bad = rewrite_save(&m, |c, _| c[0] ^= 0x10);
    let before = m.grava_estado();
    assert!(m.restaura_estado(&bad).is_err());
    assert_eq!(m.grava_estado(), before);
    let patched = m.grava_estado();
    std::fs::write(root.join("mini2005.mbr"), vec![0; 74955]).unwrap();
    let mut other = machine(&root, ORIGINAL);
    assert!(!other.set_handset_runtime_identity(MODULE, MIF));
    let before = other.grava_estado();
    assert!(other.restaura_estado(&patched).is_err());
    assert_eq!(other.grava_estado(), before);
}
#[test]
#[ignore = "requires original mini2005.mbr via BREW_MINI_ASSET"]
fn old_inflight_pc_and_lr_with_thumb_bits_reject_atomically() {
    let root = root();
    original_asset(&root);
    let mut m = machine(&root, ORIGINAL);
    assert!(m.set_handset_runtime_identity(MODULE, MIF));
    for index in [14usize, 15] {
        let old = rewrite_save(&m, |c, regs| {
            c.copy_from_slice(ORIGINAL);
            regs[4 + index * 4..8 + index * 4].copy_from_slice(&(START + 23).to_le_bytes());
        });
        let before = m.grava_estado();
        assert!(m.restaura_estado(&old).is_err());
        assert_eq!(m.grava_estado(), before);
    }
}

fn profile_fixture() -> (PathBuf, PathBuf, PathBuf) {
    let source = PathBuf::from(std::env::var_os("BREW_MINI_ASSET").expect("BREW_MINI_ASSET"));
    let root = root();
    let folder = root.join("mini2005");
    std::fs::create_dir(&folder).unwrap();
    let module = folder.join("mini2005.mod");
    std::fs::copy(source.with_file_name("mini2005.mod"), &module).unwrap();
    std::fs::copy(&source, folder.join("mini2005.mbr")).unwrap();
    let mif = std::env::var_os("BREW_MINI_MIF")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            source
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("mini2005.mif")
        });
    std::fs::copy(mif, root.join("mini2005.mif")).unwrap();
    let profiles = root.join("profiles");
    (root, module, profiles)
}
#[test]
#[ignore = "requires original Mini package next to BREW_MINI_ASSET"]
fn original_package_selects_audited_compatible_viewport_and_rejects_wrong_dependencies() {
    use zeebx::handset::display::{self, Source};
    let (root, module, _) = profile_fixture();
    let g = DisplayGeometry {
        width: 120,
        height: 146,
        depth: 16,
    };
    assert_eq!(display::automatic(MODULE, &module).geometry, g);
    assert_eq!(display::automatic(MODULE, &module).source, Source::Known);
    assert_eq!(
        display::automatic("unknown", &module).source,
        Source::Fallback
    );
    std::fs::write(root.join("mini2005.mif"), b"wrong manifest").unwrap();
    assert_eq!(display::automatic(MODULE, &module).source, Source::Fallback);
    let (_, module, _) = profile_fixture();
    std::fs::write(module.with_file_name("mini2005.mbr"), vec![0; 74955]).unwrap();
    assert_eq!(display::automatic(MODULE, &module).source, Source::Fallback);
    std::fs::remove_file(module.with_file_name("mini2005.mbr")).unwrap();
    assert_eq!(display::automatic(MODULE, &module).source, Source::Fallback);
}
#[test]
#[ignore = "requires original Mini package next to BREW_MINI_ASSET"]
fn compatible_auto_keeps_manual_resolution_and_reset_returns_to_the_viewport() {
    use zeebx::handset::display::{self, Source};
    let (_, module, profiles) = profile_fixture();
    let manual = DisplayGeometry {
        width: 400,
        height: 240,
        depth: 16,
    };
    let selected = display::set_override(&module, &profiles, Some(manual)).unwrap();
    assert_eq!(selected.geometry, manual);
    assert_eq!(selected.source, Source::Manual);
    assert_eq!(
        selected.automatic.geometry,
        DisplayGeometry {
            width: 120,
            height: 146,
            depth: 16
        }
    );
    std::fs::write(module.with_file_name("mini2005.mbr"), vec![0; 74955]).unwrap();
    let still = display::resolve(&module, &profiles).unwrap();
    assert_eq!(still.geometry, manual);
    assert_eq!(still.automatic.source, Source::Fallback);
    original_asset(module.parent().unwrap());
    let reset = display::set_override(&module, &profiles, None).unwrap();
    assert_eq!(reset.source, Source::Known);
    assert_eq!(
        reset.geometry,
        DisplayGeometry {
            width: 120,
            height: 146,
            depth: 16
        }
    );
}
