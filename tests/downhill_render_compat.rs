use std::path::PathBuf;
use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend, Reg};
use zeebx::handset::types::DisplayGeometry;
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

fn call(
    m: &mut Machine<BackendPadrao>,
    interface: Interface,
    name: &str,
    args: [u32; 4],
    stack: &[u32],
) -> u32 {
    let slot = (0..256)
        .find(|s| interface.method(*s) == Some(name))
        .unwrap();
    let previous_sp = m.cpu().read_reg(Reg::Sp);
    let sp = loader::STACK_BASE + 0x400;
    for (i, value) in stack.iter().enumerate() {
        m.cpu_mut().write_u32(sp + i as u32 * 4, *value).unwrap();
    }
    m.cpu_mut().write_reg(Reg::Sp, sp);
    let result = m
        .call_guest(aee::encode(interface, slot), args, 1000)
        .unwrap();
    m.cpu_mut().write_reg(Reg::Sp, previous_sp);
    match result {
        Outcome::Returned { code } => code,
        other => panic!("{name}: {other:?}"),
    }
}

fn fixture() -> (Machine<BackendPadrao>, PathBuf) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let work = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("local-tests")
        .join(format!(
            "downhill-render-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
    std::fs::create_dir_all(&work).unwrap();
    let storage = StoragePaths::from_root(work.join("runtime"));
    storage.create_dirs().unwrap();
    let image = ModImage::parse(
        [0xe3a0_0000u32, 0xe12f_ff1e]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect(),
    )
    .unwrap();
    let mut m = Machine::new_with_storage_geometry(
        BackendPadrao::new().unwrap(),
        loader::load_with(&image, &[]).unwrap(),
        &work,
        &storage,
        None,
        DisplayGeometry {
            width: 8,
            height: 8,
            depth: 16,
        },
    )
    .unwrap();
    assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
    (m, work)
}

fn generated_bmp(declared_size: u32) -> Vec<u8> {
    // Downhill reconstrói BMPs de tamanhos distintos com o bfSize fixo do cabeçalho modelo.
    // 5x3, 8 bits: offset 1078 e três linhas de oito bytes, total 1102.
    let mut bmp = vec![0; 1102];
    bmp[..2].copy_from_slice(b"BM");
    for (offset, value) in [(2, declared_size), (10, 1078), (14, 40), (18, 5), (22, 3)] {
        bmp[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
    bmp[28..30].copy_from_slice(&8u16.to_le_bytes());
    bmp[58..62].copy_from_slice(&[0, 0, 255, 0]);
    bmp[62..66].copy_from_slice(&[0, 255, 0, 0]);
    bmp[1078..1102].copy_from_slice(&[
        1, 2, 1, 2, 1, 0, 0, 0, 1, 1, 1, 1, 1, 0, 0, 0, 2, 1, 2, 1, 2, 0, 0, 0,
    ]);
    bmp
}

#[test]
fn generated_native_bmp_uses_complete_allocated_raster_when_bfsize_is_stale() {
    let (mut m, _work) = fixture();
    for declared in [1086, 2048] {
        let bmp = generated_bmp(declared);
        let input = call(&mut m, Interface::Helpers, "malloc", [1102, 0, 0, 0], &[]);
        m.cpu_mut().write_mem(input, &bmp).unwrap();
        let info = loader::STACK_BASE + 0x200;
        let native = call(
            &mut m,
            Interface::Helpers,
            "SetupNativeImage",
            [0x01004001, input, info, info + 16],
            &[],
        );
        assert_ne!(
            native, 0,
            "o raster completo alocado deve prevalecer sobre bfSize={declared}"
        );
        assert_eq!(m.cpu().read_u32(info).unwrap(), 0x0003_0005);
        call(
            &mut m,
            Interface::Display,
            "BitBlt",
            [loader::OBJECT_BASE + 16, 1, 2, 5],
            &[3, native, 0, 0, 2],
        );
        assert_eq!(
            (1..6)
                .map(|x| m.screen().get_pixel(x, 2))
                .collect::<Vec<_>>(),
            [0x07e0, 0xf800, 0x07e0, 0xf800, 0x07e0]
        );
        assert_eq!(
            (1..6)
                .map(|x| m.screen().get_pixel(x, 4))
                .collect::<Vec<_>>(),
            [0xf800, 0x07e0, 0xf800, 0x07e0, 0xf800]
        );
    }
}

#[test]
fn native_bmp_cannot_read_the_missing_raster_from_an_adjacent_allocation() {
    let (mut m, _work) = fixture();
    let bmp = generated_bmp(1102);
    let input = call(&mut m, Interface::Helpers, "malloc", [1086, 0, 0, 0], &[]);
    let adjacent = call(&mut m, Interface::Helpers, "malloc", [128, 0, 0, 0], &[]);
    m.cpu_mut().write_mem(adjacent, &[0; 128]).unwrap();
    m.cpu_mut().write_mem(input, &bmp[..1086]).unwrap();
    assert_eq!(
        call(
            &mut m,
            Interface::Helpers,
            "SetupNativeImage",
            [0x01004001, input, 0, 0],
            &[]
        ),
        0,
        "a imagem não pode usar os bytes da alocação vizinha"
    );
}

#[test]
fn legacy_display_oldmask_preserves_pixels_beneath_native_magenta() {
    let (mut m, _work) = fixture();
    let mut bmp = generated_bmp(1102);
    bmp[62..66].copy_from_slice(&[255, 0, 255, 0]);
    let input = call(&mut m, Interface::Helpers, "malloc", [1102, 0, 0, 0], &[]);
    m.cpu_mut().write_mem(input, &bmp).unwrap();
    let native = call(
        &mut m,
        Interface::Helpers,
        "SetupNativeImage",
        [0x01004001, input, 0, 0],
        &[],
    );
    assert_ne!(native, 0);
    let display = loader::OBJECT_BASE + 16;
    let destination = call(
        &mut m,
        Interface::Display,
        "GetDestination",
        [display, 0, 0, 0],
        &[],
    );
    for y in 2..5 {
        for x in 1..6 {
            call(
                &mut m,
                Interface::Bitmap,
                "DrawPixel",
                [destination, x, y, 0x001f],
                &[],
            );
        }
    }
    // The Terminator pede OLDMASK (4) no IDisplay, após CONVERTBMP.
    call(
        &mut m,
        Interface::Display,
        "BitBlt",
        [display, 1, 2, 5],
        &[3, native, 0, 0, 4],
    );
    assert_eq!(
        (1..6)
            .map(|x| m.screen().get_pixel(x, 2))
            .collect::<Vec<_>>(),
        [0x001f, 0xf800, 0x001f, 0xf800, 0x001f]
    );
    assert_eq!(
        (1..6)
            .map(|x| m.screen().get_pixel(x, 4))
            .collect::<Vec<_>>(),
        [0xf800, 0x001f, 0xf800, 0x001f, 0xf800]
    );
}
