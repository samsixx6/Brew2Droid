use zeebx::{brew::aee::{self, Interface}, cpu::{BackendPadrao, CpuBackend},
    loader::{self, modfile::ModImage}, machine::{Machine, Outcome}};

const RECT: u32 = loader::STACK_BASE + 0x100;

fn api(m: &mut Machine<BackendPadrao>, iface: Interface, name: &str, args: [u32; 4]) -> u32 {
    let slot = (0..256).find(|&n| iface.method(n) == Some(name)).unwrap();
    match m.call_guest(aee::encode(iface, slot), args, 10000).unwrap() {
        Outcome::Returned { code } => code,
        stopped => panic!("{name}: {stopped:?}"),
    }
}

#[test]
fn raised_and_sunken_frames_draw_opposite_bevels_and_inset_the_caller_rectangle() {
    let code = [0xe3a00000u32, 0xe12fff1e].into_iter().flat_map(u32::to_le_bytes).collect();
    let mut m = Machine::new(BackendPadrao::new().unwrap(), loader::load_with(&ModImage::parse(code).unwrap(), &[]).unwrap(), ".");
    m.run(1000).unwrap();
    assert_eq!(api(&mut m, Interface::Shell, "CreateInstance", [loader::OBJECT_BASE, 0x01001001, RECT, 0]), 0);
    let display = m.cpu().read_u32(RECT).unwrap();
    let mut edges = Vec::new();
    for frame in [3, 4] {
        let bytes = [4i16, 4, 16, 16].into_iter().flat_map(i16::to_le_bytes).chain([0xa5; 8]).collect::<Vec<_>>();
        m.cpu_mut().write_mem(RECT, &bytes).unwrap();
        assert_eq!(api(&mut m, Interface::Display, "DrawFrame", [display, RECT, frame, u32::MAX]), 2);
        let mut result = [0; 16];m.cpu().read_mem(RECT, &mut result).unwrap();
        assert_eq!(&result[..8], &[6i16, 6, 12, 12].into_iter().flat_map(i16::to_le_bytes).collect::<Vec<_>>());
        assert_eq!(&result[8..], &[0xa5; 8]);
        let at = |x, y| m.screen().get_pixel(x, y);
        edges.push((at(8,4),at(8,19),at(8,5),at(8,18)));
    }
    assert!(edges[0].0 > edges[0].1 && edges[0].2 > edges[0].3, "RAISED deve iluminar as duas arestas superiores");
    assert_eq!(edges[0], (edges[1].1, edges[1].0, edges[1].3, edges[1].2));
}
