use zeebx::{
    cpu::{BackendPadrao, CpuBackend, Reg},
    handset::types::DisplayGeometry,
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    storage::StoragePaths,
};
const START: u32 = 0x12368;
const LENGTH: usize = 0x1d0;
const PATCH_OFFSET: usize = 0x90;
const FUNCTION: &[u8] = &[
    240, 79, 45, 233, 60, 208, 77, 226, 100, 144, 157, 229, 112, 128, 157, 229, 96, 176, 157, 229,
    180, 160, 217, 225, 0, 64, 160, 225, 182, 0, 217, 225, 1, 80, 160, 225, 0, 16, 160, 227, 36, 0,
    141, 229, 36, 16, 132, 229, 2, 112, 160, 225, 3, 96, 160, 225, 0, 0, 89, 227, 125, 1, 0, 10,
    10, 0, 217, 229, 0, 0, 80, 227, 4, 0, 0, 26, 0, 0, 153, 229, 0, 0, 80, 227, 119, 1, 0, 10, 64,
    128, 136, 227, 9, 16, 201, 229, 1, 0, 118, 227, 180, 96, 217, 1, 182, 176, 217, 1, 0, 192, 160,
    3, 0, 0, 160, 3, 108, 0, 141, 5, 104, 192, 141, 5, 0, 0, 90, 227, 10, 0, 86, 17, 36, 0, 0, 218,
    0, 192, 160, 227, 26, 0, 0, 234, 104, 16, 141, 226, 3, 0, 145, 232, 12, 32, 141, 226, 3, 4,
    130, 232, 24, 128, 141, 229, 16, 0, 24, 227, 7, 0, 0, 10, 0, 0, 153, 229, 80, 16, 148, 229, 12,
    32, 133, 224, 1, 48, 128, 224, 8, 48, 141, 229, 0, 12, 141, 232, 24, 16, 148, 229, 7, 0, 0,
    234, 0, 16, 153, 229, 80, 0, 148, 229, 12, 32, 133, 224, 0, 48, 129, 224, 8, 48, 141, 229, 0,
    12, 141, 232, 104, 16, 148, 229, 0, 16, 129, 224, 7, 48, 160, 225, 4, 0, 160, 225, 7, 253, 255,
    235, 56, 192, 157, 229, 10, 0, 140, 224, 56, 0, 141, 229, 6, 0, 80, 225, 224, 255, 255, 218, 6,
    0, 92, 225, 72, 1, 0, 170, 12, 80, 133, 224, 12, 96, 70, 224, 36, 0, 157, 229, 0, 0, 80, 227,
    36, 0, 157, 21, 0, 0, 91, 17, 39, 0, 0, 218, 0, 192, 160, 227, 28, 0, 0, 234, 104, 16, 141,
    226, 3, 0, 145, 232, 12, 32, 141, 226, 3, 4, 130, 232, 24, 128, 141, 229, 16, 0, 24, 227, 8, 0,
    0, 10, 0, 0, 153, 229, 80, 16, 148, 229, 36, 32, 157, 229, 1, 48, 128, 224, 12, 0, 141, 233, 0,
    96, 141, 229, 24, 16, 148, 229, 12, 48, 135, 224, 8, 0, 0, 234, 0, 16, 153, 229, 80, 0, 148,
    229, 36, 32, 157, 229, 0, 48, 129, 224, 12, 0, 141, 233, 0, 96, 141, 229, 104, 16, 148, 229,
    12, 48, 135, 224, 0, 16, 129, 224, 5, 32, 160, 225, 4, 0, 160, 225, 219, 252, 255, 235, 52,
    192, 157, 229, 36, 0, 157, 229, 0, 0, 140, 224, 52, 0, 141, 229, 11, 0, 80, 225, 221, 255, 255,
    218, 11, 0, 92, 225, 27, 1, 0, 170, 12, 112, 135, 224, 12, 176, 75, 224, 16, 0, 24, 227, 6, 1,
    0, 10, 16, 16, 132, 226, 3, 0, 145, 232, 0, 0, 85, 225, 1, 0, 87, 209, 18, 1, 0, 202, 6, 32,
    133, 224, 0, 0, 82, 227, 11, 32, 135, 160, 0, 0, 82, 163, 13, 1, 0, 186, 0, 0, 85, 227, 6, 0,
    0, 170, 4, 0, 24, 227, 104, 192, 157, 5, 5, 96, 134, 0, 5, 192, 76, 0, 5, 96, 134, 16, 0, 80,
    160, 227, 104, 192, 141, 5, 0, 0, 87, 227, 108, 32, 157, 181, 7, 176, 139, 176, 7, 32, 66, 176,
    108, 32, 141, 181, 6, 32, 133, 224, 0, 112, 160, 179, 0, 0, 82, 225, 5, 0, 0, 218, 4, 0, 24,
    227, 104, 192, 157, 21, 0, 0, 66, 224, 12, 192, 128, 16, 104, 192, 141, 21, 0, 96, 70, 224, 11,
    0, 135, 224, 1, 0, 80, 225, 3, 0, 0, 218, 1, 0, 87, 225, 240, 0, 0, 170, 1, 0, 64, 224, 0, 176,
    75, 224, 6, 48, 160, 225, 7, 32, 160, 225, 5, 16, 160, 225, 4, 0, 160, 225, 0, 176, 141, 229,
    195, 76, 0, 235, 36, 0, 132, 229, 0, 0, 80, 227, 22, 0, 0, 26, 44, 0, 148, 229, 7, 16, 128,
    224, 20, 0, 148, 229, 221, 246, 255, 235, 40, 0, 148, 229, 1, 112, 160, 225, 5, 16, 128, 224,
    16, 0, 148, 229, 216, 246, 255, 235, 1, 192, 160, 225, 104, 16, 141, 226, 3, 0, 145, 232, 12,
    32, 141, 226, 3, 4, 130, 232, 24, 128, 141, 229, 0, 0, 153, 229, 80, 16, 148, 229, 1, 48, 128,
    224, 8, 48, 141, 229, 64, 8, 141, 232, 24, 16, 148, 229, 7, 48, 160, 225, 102, 0, 0, 234, 1, 0,
    80, 227, 49, 0, 0, 26, 16, 0, 148, 229, 5, 16, 64, 224, 40, 0, 148, 229, 48, 0, 141, 229, 0, 0,
    65, 224, 32, 0, 141, 229, 44, 0, 148, 229, 7, 16, 128, 224, 20, 0, 148, 229, 190, 246, 255,
    235, 1, 192, 160, 225, 104, 16, 141, 226, 3, 0, 145, 232, 12, 32, 141, 226, 3, 4, 130, 232, 24,
    128, 141, 229, 0, 0, 153, 229, 80, 16, 148, 229, 1, 48, 128, 224, 32, 16, 157, 229, 2, 8, 141,
    232, 48, 0, 157, 229, 8, 48, 141, 229, 5, 32, 128, 224, 4, 0, 160, 225, 24, 16, 148, 229, 12,
    48, 160, 225, 104, 252, 255, 235, 44, 0, 148, 229, 7, 16, 128, 224, 20, 0, 148, 229, 168, 246,
    255, 235, 32, 0, 157, 229, 104, 224, 157, 229, 1, 192, 160, 225, 108, 16, 157, 229, 12, 32,
    141, 226, 0, 0, 142, 224, 3, 4, 130, 232, 24, 128, 141, 229, 0, 0, 153, 229, 80, 16, 148, 229,
    1, 48, 128, 224, 32, 0, 157, 229, 0, 16, 70, 224, 2, 8, 141, 232, 8, 48, 141, 229, 24, 16, 148,
    229, 12, 48, 160, 225, 135, 0, 0, 234, 2, 0, 80, 227, 50, 0, 0, 26, 20, 0, 148, 229, 7, 16, 64,
    224, 44, 0, 148, 229, 44, 0, 141, 229, 0, 0, 65, 224, 32, 0, 141, 229, 40, 0, 148, 229, 5, 16,
    128, 224, 16, 0, 148, 229, 138, 246, 255, 235, 1, 192, 160, 225, 104, 16, 141, 226, 3, 0, 145,
    232, 12, 32, 141, 226, 3, 4, 130, 232, 24, 128, 141, 229, 0, 0, 153, 229, 80, 16, 148, 229, 32,
    32, 157, 229, 1, 48, 128, 224, 12, 0, 141, 233, 44, 0, 157, 229, 0, 96, 141, 229, 7, 48, 128,
    224, 4, 0, 160, 225, 24, 16, 148, 229, 12, 32, 160, 225, 52, 252, 255, 235, 40, 0, 148, 229, 5,
    16, 128, 224, 16, 0, 148, 229, 116, 246, 255, 235, 1, 192, 160, 225, 108, 16, 157, 229, 32, 0,
    157, 229, 12, 32, 141, 226, 0, 16, 129, 224, 104, 0, 157, 229, 3, 4, 130, 232, 24, 128, 141,
    229, 0, 0, 153, 229, 80, 16, 148, 229, 1, 48, 128, 224, 32, 0, 157, 229, 0, 32, 75, 224, 12, 0,
    141, 233, 0, 96, 141, 229, 24, 16, 148, 229, 0, 48, 160, 227, 12, 32, 160, 225, 99, 0, 0, 234,
    3, 0, 80, 227, 99, 0, 0, 26, 20, 0, 148, 229, 44, 192, 148, 229, 7, 0, 64, 224, 12, 16, 64,
    224, 32, 16, 141, 229, 16, 0, 148, 229, 40, 224, 148, 229, 5, 0, 64, 224, 14, 0, 64, 224, 28,
    0, 141, 229, 104, 16, 141, 226, 3, 0, 145, 232, 12, 32, 141, 226, 3, 4, 130, 232, 24, 128, 141,
    229, 0, 0, 153, 229, 80, 16, 148, 229, 28, 32, 141, 226, 1, 48, 128, 224, 6, 0, 146, 232, 14,
    0, 141, 232, 24, 16, 148, 229, 7, 48, 140, 224, 5, 32, 142, 224, 4, 0, 160, 225, 1, 252, 255,
    235, 28, 0, 157, 229, 104, 192, 157, 229, 108, 16, 157, 229, 0, 0, 140, 224, 40, 0, 141, 229,
    12, 32, 141, 226, 3, 4, 130, 232, 24, 128, 141, 229, 0, 0, 153, 229, 80, 16, 148, 229, 28, 32,
    141, 226, 1, 48, 128, 224, 5, 0, 146, 232, 0, 16, 70, 224, 14, 0, 141, 232, 44, 0, 148, 229, 1,
    96, 160, 225, 7, 48, 128, 224, 4, 0, 160, 225, 24, 16, 148, 229, 0, 32, 160, 227, 235, 251,
    255, 235, 108, 0, 157, 229, 32, 16, 157, 229, 12, 32, 141, 226, 1, 16, 128, 224, 104, 0, 157,
    229, 1, 112, 160, 225, 3, 4, 130, 232, 24, 128, 141, 229, 0, 0, 153, 229, 80, 16, 148, 229, 1,
    48, 128, 224, 32, 16, 157, 229, 1, 32, 75, 224, 28, 16, 157, 229, 2, 176, 160, 225, 14, 0, 141,
    232, 40, 0, 148, 229, 24, 16, 148, 229, 5, 32, 128, 224, 4, 0, 160, 225, 0, 48, 160, 227, 213,
    251, 255, 235, 40, 0, 157, 229, 12, 16, 141, 226, 129, 4, 129, 232, 24, 128, 141, 229, 0, 0,
    153, 229, 80, 16, 148, 229, 1, 48, 128, 224, 8, 48, 141, 229, 64, 8, 141, 232, 24, 16, 148,
    229, 0, 48, 160, 227, 0, 32, 160, 227, 14, 0, 0, 234, 104, 16, 141, 226, 3, 0, 145, 232, 12, 0,
    141, 229, 20, 160, 141, 229, 24, 128, 141, 229, 16, 16, 141, 229, 0, 16, 153, 229, 80, 0, 148,
    229, 5, 32, 160, 225, 0, 48, 129, 224, 8, 48, 141, 229, 64, 8, 141, 232, 104, 16, 148, 229, 7,
    48, 160, 225, 0, 16, 129, 224, 4, 0, 160, 225, 183, 251, 255, 235, 60, 208, 141, 226, 240, 79,
    189, 232, 30, 255, 47, 225,
];
const PAIRS: &[(&str, &str)] = &[
    (
        "577625c36a78945e8bc892871747f177b86abf53a40ae8e22da160d0663db3eb",
        "20abd42961d1c31f6d6e64f37077d32035d57e666b7ca01d5a2963ce726b3625",
    ),
    (
        "1d94991c74b20caea894e174067d641d9d76ec5a130767eb31408512d1d0b82a",
        "21e5b7f6e181eb8411c1a0aa39a2a0a32da241e162f555d84a7a818b4b0b385d",
    ),
    (
        "fae3b42fcdf65947fa339a39057bea02b35a73c8b1d9898d620c895347a82d42",
        "5767c13bab1079d1c586b082d28c20dc1372854a7a9914f94a0d48560954d54b",
    ),
];

const CALIBRATION: &[(u32, &[u8])] = &[
    (
        157104,
        &[
            32, 16, 148, 229, 0, 0, 160, 227, 16, 0, 81, 227, 24, 0, 0, 26, 1, 80, 71, 226, 10, 16,
            71, 226, 17, 0, 0, 234, 0, 0, 80, 227, 9, 0, 0, 10, 64, 0, 148, 229, 5, 0, 208, 231,
            31, 0, 80, 227, 5, 0, 0, 26, 6, 16, 160, 225, 4, 0, 160, 225, 199, 36, 0, 235, 80, 80,
            132, 229, 248, 64, 189, 232, 30, 255, 47, 225, 64, 0, 148, 229, 5, 0, 208, 231, 1, 80,
            69, 226, 248, 0, 80, 227, 0, 0, 160, 19, 1, 0, 160, 3, 1, 0, 85, 225, 0, 0, 85, 163,
            234, 255, 255, 170, 23, 0, 0, 234,
        ],
    ),
    (
        157128,
        &[
            32, 16, 148, 229, 0, 0, 160, 227, 16, 0, 81, 227, 24, 0, 0, 26, 1, 80, 71, 226, 10, 16,
            71, 226, 17, 0, 0, 234, 0, 0, 80, 227, 9, 0, 0, 10, 64, 0, 148, 229, 5, 0, 208, 231,
            31, 0, 80, 227, 5, 0, 0, 26, 6, 16, 160, 225, 4, 0, 160, 225, 203, 36, 0, 235, 80, 80,
            132, 229, 248, 64, 189, 232, 30, 255, 47, 225, 64, 0, 148, 229, 5, 0, 208, 231, 1, 80,
            69, 226, 248, 0, 80, 227, 0, 0, 160, 19, 1, 0, 160, 3, 1, 0, 85, 225, 0, 0, 85, 163,
            234, 255, 255, 170, 23, 0, 0, 234,
        ],
    ),
    (
        155100,
        &[
            32, 16, 148, 229, 0, 0, 160, 227, 16, 0, 81, 227, 24, 0, 0, 26, 1, 80, 71, 226, 10, 16,
            71, 226, 17, 0, 0, 234, 0, 0, 80, 227, 9, 0, 0, 10, 64, 0, 148, 229, 5, 0, 208, 231,
            31, 0, 80, 227, 5, 0, 0, 26, 6, 16, 160, 225, 4, 0, 160, 225, 168, 36, 0, 235, 80, 80,
            132, 229, 248, 64, 189, 232, 30, 255, 47, 225, 64, 0, 148, 229, 5, 0, 208, 231, 1, 80,
            69, 226, 248, 0, 80, 227, 0, 0, 160, 19, 1, 0, 160, 3, 1, 0, 85, 225, 0, 0, 85, 163,
            234, 255, 255, 170, 23, 0, 0, 234,
        ],
    ),
];
fn unstarted_case(context: &[u8], case: usize) -> Machine<BackendPadrao> {
    let mut data = vec![0; 0x21000];
    let (cal_at, cal_bytes) = CALIBRATION[case];
    let off = (cal_at - 0x10000) as usize;
    data[off..off + cal_bytes.len()].copy_from_slice(cal_bytes);
    data[..8].copy_from_slice(&[0, 0, 0xa0, 0xe3, 0x1e, 0xff, 0x2f, 0xe1]);
    data[0x2368..0x2368 + FUNCTION.len()].copy_from_slice(FUNCTION);
    data[0x2368..0x2368 + LENGTH].copy_from_slice(context);
    // A rotina original pede as copias; o destino registra seus retangulos sem normaliza-los.
    let sink: [u32; 15] = [
        0xe590c080, 0xe8ac000c, 0xe59d2000, 0xe48c2004, 0xe59d2004, 0xe48c2004, 0xe59d200c,
        0xe48c2004, 0xe59d2010, 0xe48c2004, 0xe59d2018, 0xe48c2004, 0xe580c080, 0xe3a00000,
        0xe12fff1e,
    ];
    for (i, w) in sink.iter().enumerate() {
        data[0x1880 + i * 4..0x1884 + i * 4].copy_from_slice(&w.to_le_bytes());
    }
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("local-tests/startup-repairs-wave20-20261031/graphics/fixture");
    let image = ModImage::parse(data).unwrap();
    let storage = StoragePaths::from_root(root.join("runtime"));
    Machine::new_with_storage_geometry(
        BackendPadrao::new().unwrap(),
        loader::load_with(&image, &[]).unwrap(),
        &root,
        &storage,
        None,
        DisplayGeometry {
            width: 240,
            height: 320,
            depth: 16,
        },
    )
    .unwrap()
}
fn unstarted(context: &[u8]) -> Machine<BackendPadrao> {
    unstarted_case(context, 2)
}
fn machine_case(case: usize) -> Machine<BackendPadrao> {
    let mut m = unstarted_case(&FUNCTION[..LENGTH], case);
    assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
    m
}
fn machine() -> Machine<BackendPadrao> {
    let mut m = unstarted(&FUNCTION[..LENGTH]);
    assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
    m
}
fn draw(
    m: &mut Machine<BackendPadrao>,
    width: u32,
    height: u32,
    image_width: u16,
    image_height: u16,
    flags: u32,
) -> Vec<[u32; 7]> {
    let ctx = loader::HEAP_BASE;
    let image = ctx + 0x200;
    let log = ctx + 0x400;
    m.cpu_mut().write_mem(ctx, &vec![0; 0x1000]).unwrap();
    m.cpu_mut()
        .write_mem(ctx + 0x80, &log.to_le_bytes())
        .unwrap();
    let mut descriptor = [0u8; 12];
    descriptor[..4].copy_from_slice(&(ctx + 0x1000).to_le_bytes());
    descriptor[4..6].copy_from_slice(&image_width.to_le_bytes());
    descriptor[6..8].copy_from_slice(&image_height.to_le_bytes());
    m.cpu_mut().write_mem(image, &descriptor).unwrap();
    let sp = m.cpu().read_reg(Reg::Sp) - 32;
    let args = [height, image, 0, 0, flags];
    let bytes: Vec<u8> = args.iter().flat_map(|w| w.to_le_bytes()).collect();
    m.cpu_mut().write_mem(sp, &bytes).unwrap();
    m.cpu_mut().write_reg(Reg::Sp, sp);
    let before = [
        Reg::R4,
        Reg::R5,
        Reg::R6,
        Reg::R7,
        Reg::R8,
        Reg::R9,
        Reg::R10,
        Reg::R11,
    ]
    .map(|r| m.cpu().read_reg(r));
    let result = m.call_guest(START, [ctx, 7, 11, width], 50000).unwrap();
    assert!(matches!(result, Outcome::Returned { .. }), "{result:?}");
    assert_eq!(m.cpu().read_reg(Reg::Sp), sp);
    let after = [
        Reg::R4,
        Reg::R5,
        Reg::R6,
        Reg::R7,
        Reg::R8,
        Reg::R9,
        Reg::R10,
        Reg::R11,
    ]
    .map(|r| m.cpu().read_reg(r));
    assert_eq!(before, after, "AAPCS preserved registers");
    m.cpu_mut().write_reg(Reg::Sp, sp + 32);
    let end = u32::from_le_bytes(m.dump(ctx + 0x80, 4).unwrap().try_into().unwrap());
    let b = m.dump(log, (end - log) as usize).unwrap();
    b.chunks_exact(28)
        .map(|c| {
            std::array::from_fn(|i| u32::from_le_bytes(c[i * 4..i * 4 + 4].try_into().unwrap()))
        })
        .collect()
}
fn expected_tiles(draws: &[[u32; 7]], w: u32, h: u32, iw: u32, ih: u32) {
    let mut seen = vec![0u8; (w * h) as usize];
    for &[x, y, cw, ch, sx, sy, _] in draws {
        assert!(
            cw <= iw && ch <= ih,
            "copy exceeds original image: {:?}",
            (x, y, cw, ch)
        );
        assert_eq!((sx, sy), (0, 0));
        assert!(x >= 7 && y >= 11 && x + cw <= 7 + w && y + ch <= 11 + h);
        for dy in 0..ch {
            for dx in 0..cw {
                let p = ((y - 11 + dy) * w + x - 7 + dx) as usize;
                seen[p] += 1;
                assert_eq!(((x - 7 + dx) % iw, (y - 11 + dy) % ih), (sx + dx, sy + dy));
            }
        }
    }
    assert!(
        seen.iter().all(|&n| n == 1),
        "all destination pixels copied once"
    );
}
#[test]
fn original_width_loop_reproduces_the_overheight_copy() {
    let mut m = machine();
    let a = draw(&mut m, 240, 202, 120, 146, 1);
    assert_eq!(a.len(), 2);
    assert_eq!(a[0][3], 202);
}
#[test]
fn exact_pairs_nest_height_tiling_for_full_and_partial_columns() {
    for (case, &(module, mif)) in PAIRS.iter().enumerate() {
        let mut m = machine_case(case);
        assert!(m.set_handset_runtime_identity(module, mif));
        for &(w, h, iw, ih, flags) in &[
            (240, 202, 120, 146, 1),
            (250, 202, 120, 146, 1),
            (240, 292, 120, 146, 16),
            (240, 202, 176, 202, 1),
            (120, 146, 120, 146, 1),
            (60, 73, 120, 146, 1),
        ] {
            let a = draw(&mut m, w, h, iw, ih, flags);
            expected_tiles(&a, w, h, iw.into(), ih.into());
        }
    }
}
#[test]
fn live_jit_patch_and_idempotence_use_the_real_tiler() {
    let mut m = machine();
    assert_eq!(draw(&mut m, 240, 202, 120, 146, 1).len(), 2);
    assert!(m.set_handset_runtime_identity(PAIRS[2].0, PAIRS[2].1));
    expected_tiles(&draw(&mut m, 240, 202, 120, 146, 1), 240, 202, 120, 146);
    let a = m.dump(START, LENGTH).unwrap();
    assert!(m.set_handset_runtime_identity(PAIRS[2].0, PAIRS[2].1));
    assert_eq!(m.dump(START, LENGTH).unwrap(), a);
}
#[test]
fn both_hashes_and_exact_pairing_are_required() {
    for (module, mif) in [
        (PAIRS[0].0, "unknown"),
        ("unknown", PAIRS[0].1),
        (PAIRS[0].0, PAIRS[1].1),
    ] {
        let mut m = machine();
        assert!(!m.set_handset_runtime_identity(module, mif));
        assert_eq!(draw(&mut m, 240, 202, 120, 146, 1).len(), 2);
    }
}
#[test]
fn before_reset_patch_uses_the_original_guard() {
    let mut m = unstarted(&FUNCTION[..LENGTH]);
    assert!(m.set_handset_runtime_identity(PAIRS[2].0, PAIRS[2].1));
    assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
    expected_tiles(&draw(&mut m, 250, 202, 120, 146, 1), 250, 202, 120, 146);
}
#[test]
fn changed_context_rejects_before_any_template_or_live_write() {
    for pos in 0..LENGTH {
        let mut altered = FUNCTION[..LENGTH].to_vec();
        altered[pos] ^= 0x10;
        let mut m = unstarted(&altered);
        assert!(
            !m.set_handset_runtime_identity(PAIRS[2].0, PAIRS[2].1),
            "byte {pos}"
        );
        assert_eq!(m.module().mem.read(START, LENGTH as u32).unwrap(), altered);
    }
}
fn rewritten_save(m: &mut Machine<BackendPadrao>, change: impl FnOnce(&mut [u8])) -> Vec<u8> {
    use zeebx::save_state::{self, Leitor};
    let data = m.grava_estado();
    let l = Leitor::abre(&data).unwrap();
    let base = m
        .module()
        .mem
        .regions()
        .iter()
        .find(|r| r.name == "module")
        .unwrap()
        .base;
    let mut change = Some(change);
    let parts = l
        .nomes()
        .into_iter()
        .map(|name| {
            let mut b = l.secao(name).unwrap().to_vec();
            if name == "mem.module" {
                change.take().unwrap()(
                    &mut b[(START - base) as usize..(START - base) as usize + LENGTH],
                );
            }
            (name.to_owned(), b)
        })
        .collect::<Vec<_>>();
    save_state::escreve(&parts)
}
#[test]
fn old_original_save_is_upgraded_before_execution() {
    let mut m = machine();
    assert!(m.set_handset_runtime_identity(PAIRS[2].0, PAIRS[2].1));
    let saved = rewritten_save(&mut m, |b| b.copy_from_slice(&FUNCTION[..LENGTH]));
    m.restaura_estado(&saved).unwrap();
    expected_tiles(&draw(&mut m, 240, 202, 120, 146, 1), 240, 202, 120, 146);
}
#[test]
fn changed_saved_context_is_rejected_atomically() {
    let mut m = machine();
    assert!(m.set_handset_runtime_identity(PAIRS[2].0, PAIRS[2].1));
    for pos in [0, PATCH_OFFSET, PATCH_OFFSET + 51, LENGTH - 1] {
        let saved = rewritten_save(&mut m, |b| b[pos] ^= 0x10);
        let before = m.grava_estado();
        assert!(m.restaura_estado(&saved).is_err(), "byte {pos}");
        assert_eq!(m.grava_estado(), before);
    }
}

#[test]
fn every_live_guard_byte_is_checked_before_either_copy_is_changed() {
    for pos in 0..LENGTH {
        let mut m = machine();
        let mut b = FUNCTION[..LENGTH].to_vec();
        b[pos] ^= 0x10;
        m.cpu_mut().write_mem(START, &b).unwrap();
        assert!(
            !m.set_handset_runtime_identity(PAIRS[2].0, PAIRS[2].1),
            "byte {pos}"
        );
        assert_eq!(m.dump(START, LENGTH).unwrap(), b);
        assert_eq!(
            m.module().mem.read(START, LENGTH as u32).unwrap(),
            &FUNCTION[..LENGTH]
        );
    }
}
#[test]
fn saved_patched_context_roundtrips_and_rejects_partial_patch() {
    let mut m = machine();
    assert!(m.set_handset_runtime_identity(PAIRS[2].0, PAIRS[2].1));
    let saved = m.grava_estado();
    m.restaura_estado(&saved).unwrap();
    expected_tiles(&draw(&mut m, 250, 202, 120, 146, 1), 250, 202, 120, 146);
    let partial = rewritten_save(&mut m, |b| {
        b[PATCH_OFFSET..PATCH_OFFSET + 4].copy_from_slice(&FUNCTION[PATCH_OFFSET..PATCH_OFFSET + 4])
    });
    let before = m.grava_estado();
    assert!(m.restaura_estado(&partial).is_err());
    assert_eq!(m.grava_estado(), before);
}

#[test]
fn every_calibration_guard_byte_is_checked_before_the_tiler_is_changed() {
    for case in 0..3 {
        for pos in 0..116 {
            let mut m = machine_case(case);
            let (at, original) = CALIBRATION[case];
            let mut bad = original.to_vec();
            bad[pos] ^= 0x10;
            m.cpu_mut().write_mem(at, &bad).unwrap();
            assert!(
                !m.set_handset_runtime_identity(PAIRS[case].0, PAIRS[case].1),
                "case {case}, byte {pos}"
            );
            assert_eq!(m.dump(START, LENGTH).unwrap(), FUNCTION[..LENGTH]);
            assert_eq!(m.dump(at, 116).unwrap(), bad);
            assert_eq!(
                m.module().mem.read(START, LENGTH as u32).unwrap(),
                &FUNCTION[..LENGTH]
            );
        }
    }
}
#[test]
fn changed_calibration_save_is_rejected_before_restoring_any_state() {
    use zeebx::save_state::{self, Leitor};
    let mut m = machine();
    assert!(m.set_handset_runtime_identity(PAIRS[2].0, PAIRS[2].1));
    let before = m.grava_estado();
    let reader = Leitor::abre(&before).unwrap();
    let base = m
        .module()
        .mem
        .regions()
        .iter()
        .find(|r| r.name == "module")
        .unwrap()
        .base;
    let parts = reader
        .nomes()
        .into_iter()
        .map(|n| {
            let mut b = reader.secao(n).unwrap().to_vec();
            if n == "mem.module" {
                b[(CALIBRATION[2].0 - base) as usize + 8] ^= 0x10;
            }
            (n.to_owned(), b)
        })
        .collect::<Vec<_>>();
    let bad = save_state::escreve(&parts);
    assert!(m.restaura_estado(&bad).is_err());
    assert_eq!(m.grava_estado(), before);
}

fn save_with_original_tiler_and_regs(m: &mut Machine<BackendPadrao>, pc: u32, lr: u32) -> Vec<u8> {
    use zeebx::save_state::{self, Leitor};
    let file = rewritten_save(m, |b| b.copy_from_slice(&FUNCTION[..LENGTH]));
    let reader = Leitor::abre(&file).unwrap();
    let sections = reader
        .nomes()
        .into_iter()
        .map(|name| {
            let mut b = reader.secao(name).unwrap().to_vec();
            if name == "cpu.regs" {
                b[4 + 14 * 4..4 + 15 * 4].copy_from_slice(&lr.to_le_bytes());
                b[4 + 15 * 4..4 + 16 * 4].copy_from_slice(&pc.to_le_bytes());
            }
            (name.to_owned(), b)
        })
        .collect::<Vec<_>>();
    let saved = save_state::escreve(&sections);
    let checked = Leitor::abre(&saved).unwrap().u32s("cpu.regs").unwrap();
    assert_eq!((checked[15], checked[14]), (pc, lr));
    saved
}
#[test]
fn original_inflight_tiler_pc_or_callee_return_is_rejected_atomically() {
    for (pc, lr) in [
        (0x123f8, 0),
        (0x123f9, 0),
        (0x12534, 0),
        (0x11880, 0x12460),
        (0x11880, 0x12461),
    ] {
        let mut m = machine();
        assert!(m.set_handset_runtime_identity(PAIRS[2].0, PAIRS[2].1));
        let bad = save_with_original_tiler_and_regs(&mut m, pc, lr);
        let before = m.grava_estado();
        assert!(m.restaura_estado(&bad).is_err(), "pc {pc:x} lr {lr:x}");
        assert_eq!(m.grava_estado(), before);
    }
}
#[test]
fn patched_mid_copy_save_resumes_the_actual_nested_guest_loop() {
    use zeebx::cpu::StopReason;
    let mut m = machine();
    assert!(m.set_handset_runtime_identity(PAIRS[2].0, PAIRS[2].1));
    let ctx = loader::HEAP_BASE;
    let image = ctx + 0x200;
    let log = ctx + 0x400;
    m.cpu_mut().write_mem(ctx, &vec![0; 0x1000]).unwrap();
    m.cpu_mut()
        .write_mem(ctx + 0x80, &log.to_le_bytes())
        .unwrap();
    let mut descriptor = [0u8; 12];
    descriptor[..4].copy_from_slice(&(ctx + 0x1000).to_le_bytes());
    descriptor[4..6].copy_from_slice(&120u16.to_le_bytes());
    descriptor[6..8].copy_from_slice(&146u16.to_le_bytes());
    m.cpu_mut().write_mem(image, &descriptor).unwrap();
    let sp = m.cpu().read_reg(Reg::Sp) - 32;
    let extra = [202u32, image, 0, 0, 1];
    m.cpu_mut()
        .write_mem(
            sp,
            &extra
                .iter()
                .flat_map(|w| w.to_le_bytes())
                .collect::<Vec<_>>(),
        )
        .unwrap();
    m.cpu_mut().write_reg(Reg::Sp, sp);
    for (r, v) in [(Reg::R0, ctx), (Reg::R1, 7), (Reg::R2, 11), (Reg::R3, 240)] {
        m.cpu_mut().write_reg(r, v);
    }
    let mut pc = START;
    let mut paused = false;
    for _ in 0..1000 {
        let stop = m.cpu_mut().run(pc, 1).unwrap();
        assert_eq!(stop, StopReason::Budget);
        pc = m.cpu().read_reg(Reg::Pc);
        if (0x11880..0x118bc).contains(&pc) {
            paused = true;
            break;
        }
    }
    assert!(paused, "guest stopped inside real nested copy destination");
    let saved = m.grava_estado();
    let stop = m.cpu_mut().run(pc, 50000).unwrap();
    assert_eq!(stop, StopReason::Returned);
    let end = u32::from_le_bytes(m.dump(ctx + 0x80, 4).unwrap().try_into().unwrap());
    let first = m.dump(log, (end - log) as usize).unwrap();
    m.restaura_estado(&saved).unwrap();
    let pc = m.cpu().read_reg(Reg::Pc);
    assert_eq!(m.cpu_mut().run(pc, 50000).unwrap(), StopReason::Returned);
    assert_eq!(m.dump(log, (end - log) as usize).unwrap(), first);
    let draws: Vec<[u32; 7]> = first
        .chunks_exact(28)
        .map(|b| {
            std::array::from_fn(|i| u32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap()))
        })
        .collect();
    expected_tiles(&draws, 240, 202, 120, 146);
}
