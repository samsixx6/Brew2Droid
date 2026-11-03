use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend, Reg},
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    storage::StoragePaths,
};
const AEEDIB: u32 = 0x01001023;
const IDIB: u32 = 0x01001045;
const OUT: u32 = loader::HEAP_BASE + 0x800;
fn machine() -> Machine<BackendPadrao> {
    let image = ModImage::parse(vec![0, 0, 0xa0, 0xe3, 0x1e, 0xff, 0x2f, 0xe1]).unwrap();
    let root =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local-tests/aeedib-fixture");
    let storage = StoragePaths::from_root(root.join("runtime"));
    let mut m = Machine::new_with_storage(
        BackendPadrao::new().unwrap(),
        loader::load_with(&image, &[]).unwrap(),
        &root,
        &storage,
        None,
    );
    assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
    m
}
fn call(
    m: &mut Machine<BackendPadrao>,
    iface: Interface,
    name: &str,
    args: [u32; 4],
    extra: &[u32],
) -> u32 {
    let slot = (0..256).find(|s| iface.method(*s) == Some(name)).unwrap();
    let old = m.cpu().read_reg(Reg::Sp);
    let sp = (old - 32) & !7;
    m.cpu_mut().write_reg(Reg::Sp, sp);
    for (i, v) in extra.iter().enumerate() {
        m.cpu_mut().write_u32(sp + i as u32 * 4, *v).unwrap();
    }
    let outcome = m.call_guest(aee::encode(iface, slot), args, 1000).unwrap();
    m.cpu_mut().write_reg(Reg::Sp, old);
    match outcome {
        Outcome::Returned { code } => code,
        other => panic!("{name}: {other:?}"),
    }
}
fn bitmap(m: &mut Machine<BackendPadrao>) -> u32 {
    assert_eq!(
        call(
            m,
            Interface::Display,
            "CreateDIBitmap",
            [loader::OBJECT_BASE + 0x10, OUT, 16, 2],
            &[2]
        ),
        0
    );
    m.cpu().read_u32(OUT).unwrap()
}
fn legacy(m: &mut Machine<BackendPadrao>, b: u32) -> u32 {
    assert_eq!(
        call(
            m,
            Interface::Bitmap,
            "QueryInterface",
            [b, AEEDIB, OUT, 0],
            &[]
        ),
        0
    );
    m.cpu().read_u32(OUT).unwrap()
}
#[test]
fn borrowed_aeedib_has_sdk_offsets_and_shares_real_pixels() {
    let mut m = machine();
    let b = bitmap(&mut m);
    let view = legacy(&mut m, b);
    let h = m.dump(view, 32).unwrap();
    assert_eq!(
        u32::from_le_bytes(h[4..8].try_into().unwrap()),
        m.cpu().read_u32(b + 8).unwrap()
    );
    assert_eq!(&h[16..25], &[2, 0, 2, 0, 4, 0, 0, 0, 16]);
    let pixels = m.cpu().read_u32(view + 4).unwrap();
    m.cpu_mut()
        .write_mem(pixels, &[0, 248, 224, 7, 31, 0, 255, 255])
        .unwrap();
    call(
        &mut m,
        Interface::Display,
        "BitBlt",
        [loader::OBJECT_BASE + 0x10, 0, 0, 2],
        &[2, b, 0, 0, 7],
    );
    assert_eq!(m.screen().get_pixel(0, 0), 0xf800);
    assert_eq!(m.screen().get_pixel(1, 1), 0xffff);
}
#[test]
fn repeated_plain_data_queries_do_not_acquire_unreleasable_references() {
    let mut m = machine();
    let b = bitmap(&mut m);
    let v = legacy(&mut m, b);
    for _ in 0..128 {
        assert_eq!(legacy(&mut m, b), v);
    }
    assert_eq!(
        call(&mut m, Interface::Bitmap, "Release", [b, 0, 0, 0], &[]),
        0
    );
    m.cpu_mut().write_u32(OUT, 0xdecafbad).unwrap();
    assert_eq!(
        call(
            &mut m,
            Interface::Bitmap,
            "QueryInterface",
            [b, AEEDIB, OUT, 0],
            &[]
        ),
        20
    );
    assert_eq!(m.cpu().read_u32(OUT).unwrap(), 0);
}
#[test]
fn modern_idib_query_keeps_its_existing_reference_contract() {
    let mut m = machine();
    let b = bitmap(&mut m);
    assert_eq!(
        call(
            &mut m,
            Interface::Bitmap,
            "QueryInterface",
            [b, IDIB, OUT, 0],
            &[]
        ),
        0
    );
    assert_eq!(m.cpu().read_u32(OUT).unwrap(), b);
    assert_eq!(
        call(&mut m, Interface::Bitmap, "Release", [b, 0, 0, 0], &[]),
        1
    );
    assert_eq!(
        call(&mut m, Interface::Bitmap, "Release", [b, 0, 0, 0], &[]),
        0
    );
}
#[test]
fn device_bitmap_remains_owned_after_borrowed_view_queries() {
    let mut m = machine();
    assert_eq!(
        call(
            &mut m,
            Interface::Display,
            "GetDeviceBitmap",
            [loader::OBJECT_BASE + 0x10, OUT, 0, 0],
            &[]
        ),
        0
    );
    let b = m.cpu().read_u32(OUT).unwrap();
    // Medir pelos métodos públicos antes da consulta separa o contrato da vista
    // emprestada das referências que o display já conserva para a própria tela.
    let with_probe = call(&mut m, Interface::Bitmap, "AddRef", [b, 0, 0, 0], &[]);
    let baseline = call(&mut m, Interface::Bitmap, "Release", [b, 0, 0, 0], &[]);
    assert_eq!(with_probe, baseline + 1);
    assert!(baseline > 1);
    for _ in 0..8 {
        legacy(&mut m, b);
    }
    assert_eq!(
        call(&mut m, Interface::Bitmap, "AddRef", [b, 0, 0, 0], &[]),
        baseline + 1
    );
    assert_eq!(
        call(&mut m, Interface::Bitmap, "Release", [b, 0, 0, 0], &[]),
        baseline
    );
    assert_eq!(
        call(&mut m, Interface::Bitmap, "Release", [b, 0, 0, 0], &[]),
        baseline - 1
    );
    let v = legacy(&mut m, b);
    assert_ne!(m.cpu().read_u32(v + 4).unwrap(), 0);
}
