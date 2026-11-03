use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend},
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
};

const DATE: u32 = loader::STACK_BASE + 0x100;

fn machine() -> Machine<BackendPadrao> {
    let bytes = [0xe3a0_0000u32, 0xe12f_ff1e]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    let mut machine = Machine::new(
        BackendPadrao::new().unwrap(),
        loader::load_with(&ModImage::parse(bytes).unwrap(), &[]).unwrap(),
        ".",
    );
    assert_eq!(machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
    machine
}

fn helper(machine: &mut Machine<BackendPadrao>, name: &str, args: [u32; 4]) -> u32 {
    api(machine, Interface::Helpers, name, args)
}

fn api(
    machine: &mut Machine<BackendPadrao>,
    interface: Interface,
    name: &str,
    args: [u32; 4],
) -> u32 {
    let slot = (0..256)
        .find(|slot| interface.method(*slot) == Some(name))
        .unwrap();
    match machine
        .call_guest(aee::encode(interface, slot), args, 1000)
        .unwrap()
    {
        Outcome::Returned { code } => code,
        outcome => panic!("{name}: {outcome:?}"),
    }
}

fn set_date(machine: &mut Machine<BackendPadrao>, date: [u16; 7]) {
    let bytes: Vec<_> = date.into_iter().flat_map(u16::to_le_bytes).collect();
    machine.cpu_mut().write_mem(DATE, &bytes).unwrap();
}

#[test]
fn julian_to_seconds_uses_brew_epoch_and_ignores_weekday() {
    let mut m = machine();
    for (date, expected) in [
        ([1980, 1, 6, 0, 0, 0, 6], 0),
        ([1980, 1, 7, 13, 24, 35, 0], 134_675),
        ([1980, 2, 29, 0, 0, 0, 0], 4_665_600),
        ([2000, 1, 1, 0, 0, 0, 0], 630_720_000),
        ([2000, 2, 29, 23, 59, 59, 0], 635_903_999),
        ([2100, 3, 1, 0, 0, 0, 0], 3_791_577_600),
    ] {
        set_date(&mut m, date);
        assert_eq!(
            helper(&mut m, "aee_JulianToSeconds", [DATE, 0, 0, 0]),
            expected,
            "{date:?}"
        );
    }
}

#[test]
fn julian_helpers_round_trip_the_entire_representable_second_range() {
    let mut m = machine();
    for seconds in [
        1,
        86_399,
        86_400,
        4_665_600,
        630_720_000,
        635_903_999,
        3_791_577_600,
        u32::MAX,
    ] {
        helper(&mut m, "aee_GetJulianDate", [seconds, DATE, 0, 0]);
        assert_eq!(
            helper(&mut m, "aee_JulianToSeconds", [DATE, 0, 0, 0]),
            seconds
        );
    }
}

#[test]
fn invalid_julian_input_returns_zero_without_normalizing_bad_dates() {
    let mut m = machine();
    assert_eq!(helper(&mut m, "aee_JulianToSeconds", [0, 0, 0, 0]), 0);
    for date in [
        [0; 7],
        [1980, 1, 5, 23, 59, 59, 0],
        [2100, 2, 29, 0, 0, 0, 0],
        [2000, 13, 1, 0, 0, 0, 0],
        [2000, 1, 1, 24, 0, 0, 0],
        [65535, 1, 1, 0, 0, 0, 0],
    ] {
        set_date(&mut m, date);
        assert_eq!(helper(&mut m, "aee_JulianToSeconds", [DATE, 0, 0, 0]), 0);
    }
}

#[test]
fn inet_aton_parses_network_order_and_documented_url_delimiters() {
    let mut m = machine();
    for text in [
        "12.34.56.78",
        "12.34.56.78:80",
        "12.34.56.78/path",
        "12.34.56.78,1.2.3.4",
    ] {
        m.cpu_mut()
            .write_mem(DATE, format!("{text}\0").as_bytes())
            .unwrap();
        m.cpu_mut().write_mem(DATE + 100, &[0xa5; 8]).unwrap();
        assert_eq!(helper(&mut m, "inet_aton", [DATE, DATE + 100, 0, 0]), 1);
        let mut bytes = [0; 8];
        m.cpu().read_mem(DATE + 100, &mut bytes).unwrap();
        assert_eq!(bytes, [12, 34, 56, 78, 0xa5, 0xa5, 0xa5, 0xa5]);
    }
}

#[test]
fn inet_aton_failure_preserves_output_and_ntoa_bounds_its_writes() {
    let mut m = machine();
    for text in [
        "",
        "hostname",
        "256.2.3.4",
        "1.2.3",
        "1.2.3.4.5",
        "-1.2.3.4",
    ] {
        m.cpu_mut()
            .write_mem(DATE, format!("{text}\0").as_bytes())
            .unwrap();
        m.cpu_mut().write_mem(DATE + 100, &[0xa5; 8]).unwrap();
        assert_eq!(helper(&mut m, "inet_aton", [DATE, DATE + 100, 0, 0]), 0);
        assert_eq!(m.cpu().read_u32(DATE + 100).unwrap(), 0xa5a5a5a5);
    }
    let address = u32::from_le_bytes([12, 34, 56, 78]);
    for capacity in [0, 1, 4, 16] {
        m.cpu_mut().write_mem(DATE + 100, &[0xa5; 20]).unwrap();
        helper(&mut m, "inet_ntoa", [address, DATE + 100, capacity, 0]);
        let mut result = [0; 20];
        m.cpu().read_mem(DATE + 100, &mut result).unwrap();
        let text = b"12.34.56.78";
        if capacity > 0 {
            let count = (capacity as usize - 1).min(text.len());
            assert_eq!(&result[..count], &text[..count]);
            assert_eq!(result[count], 0);
        }
        assert!(result[capacity as usize..].iter().all(|x| *x == 0xa5));
    }
}

#[test]
fn graphics_default_viewport_returns_screen_rect_and_one_byte_boolean() {
    let mut m = machine();
    let shell = loader::OBJECT_BASE;
    assert_eq!(
        api(
            &mut m,
            Interface::Shell,
            "CreateInstance",
            [shell, 0x01002001, DATE + 40, 0]
        ),
        0
    );
    let graphics = m.cpu().read_u32(DATE + 40).unwrap();
    m.cpu_mut().write_mem(DATE, &[0xa5; 20]).unwrap();
    assert_eq!(
        api(
            &mut m,
            Interface::Graphics,
            "GetViewport",
            [graphics, DATE, DATE + 12, 0]
        ),
        1
    );
    let mut result = [0; 20];
    m.cpu().read_mem(DATE, &mut result).unwrap();
    let expected: Vec<_> = [0i16, 0, 640, 480]
        .into_iter()
        .flat_map(i16::to_le_bytes)
        .collect();
    assert_eq!(&result[..8], expected);
    assert_eq!(result[12], 0);
    assert!(
        result[8..12]
            .iter()
            .chain(result[13..].iter())
            .all(|x| *x == 0xa5)
    );
    assert_eq!(
        api(
            &mut m,
            Interface::Graphics,
            "GetViewport",
            [graphics, 0, DATE + 12, 0]
        ),
        0
    );
}

#[test]
fn wwritelongex_tracks_capacity_padding_and_the_append_pointer() {
    let mut m = machine();
    for (value, pad, capacity, expected, remaining) in [
        (245i32, 5, 12, "00245", 7),
        (-7, 4, 12, "-007", 8),
        (245, 12, 12, "245", 9),
        (245, 5, 4, "002", 1),
        (i32::MIN, 0, 12, "-2147483648", 1),
    ] {
        m.cpu_mut().write_mem(DATE, &[0xa5; 40]).unwrap();
        m.cpu_mut().write_u32(DATE + 100, capacity).unwrap();
        let end = helper(
            &mut m,
            "wwritelongex",
            [DATE, value as u32, pad, DATE + 100],
        );
        assert_eq!(end, DATE + expected.len() as u32 * 2);
        assert_eq!(m.cpu().read_u32(DATE + 100).unwrap(), remaining);
        let mut actual = vec![0; (expected.len() + 1) * 2];
        m.cpu().read_mem(DATE, &mut actual).unwrap();
        let wanted: Vec<_> = expected
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(actual, wanted);
        let mut after = [0; 2];
        m.cpu().read_mem(DATE + capacity * 2, &mut after).unwrap();
        assert_eq!(after, [0xa5; 2]);
    }
}

#[test]
fn query_class_manifest_path_supports_basename_extraction_and_resource_reads() {
    let mut m = machine();
    let class = 0x01017777u32;
    let text = b"\x03Selected version\0";
    let mut mif = vec![0u8; 72];
    mif[..4].copy_from_slice(&[0x11, 0, 1, 0]);
    mif[6..8].copy_from_slice(&1u16.to_le_bytes());
    for (at, value) in [
        (8, 32u32),
        (12, 8),
        (16, 40),
        (20, 2),
        (24, 52),
        (28, 20 + text.len() as u32),
        (40, 52),
        (44, 72),
        (48, 72 + text.len() as u32),
        (52, class),
        (60, 1000),
    ] {
        mif[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    for (at, value) in [(32, 1u16), (34, 8), (38, 1)] {
        mif[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }
    mif.extend_from_slice(text);
    assert!(m.set_applet_mif_named(&mif, class, "chosen.mif"));
    assert_eq!(
        api(
            &mut m,
            Interface::Shell,
            "QueryClass",
            [loader::OBJECT_BASE, class, DATE, 0]
        ),
        1
    );
    let name = m.cpu().read_u32(DATE + 4).unwrap();
    let slash = helper(&mut m, "strrchr", [name, u32::from(b'/'), 0, 0]);
    assert_ne!(
        slash, 0,
        "o caminho deve ter separador, como os jogos esperam"
    );
    assert_eq!(m.cpu().read_cstring(slash + 1, 64), "chosen.mif");
    let stack = m.cpu().read_reg(zeebx::cpu::Reg::Sp) - 32;
    m.cpu_mut().write_reg(zeebx::cpu::Reg::Sp, stack);
    m.cpu_mut().write_u32(stack, 64).unwrap();
    m.set_tracing(true);
    assert_eq!(
        api(
            &mut m,
            Interface::Shell,
            "LoadResString",
            [loader::OBJECT_BASE, name, 8, DATE + 64]
        ),
        16
    );
    let mut wide = [0; 34];
    m.cpu().read_mem(DATE + 64, &mut wide).unwrap();
    assert_eq!(
        wide.to_vec(),
        "Selected version"
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
        "{:?}",
        m.trace()
    );
    let path = m.cpu().read_cstring(name, 64);
    m.cpu_mut()
        .write_mem(DATE + 160, format!("{path}\0").as_bytes())
        .unwrap();
    let raw = api(
        &mut m,
        Interface::Shell,
        "LoadResData",
        [loader::OBJECT_BASE, DATE + 160, 8, 1],
    );
    assert_ne!(raw, 0);
    assert_eq!(m.cpu().read_cstring(raw, 64), "\x03Selected version");
    m.cpu_mut()
        .write_mem(DATE + 160, b"fs:/~/other.mif\0")
        .unwrap();
    assert_eq!(
        api(
            &mut m,
            Interface::Shell,
            "LoadResString",
            [loader::OBJECT_BASE, DATE + 160, 8, DATE + 64]
        ),
        0
    );
    // O nome retornado pertence ao guest: trocar .mif por .bar não pode mudar a
    // identidade dos bytes do manifesto mantidos pelo host.
    m.cpu_mut().write_mem(name, b"fs:/~/chosen.bar\0").unwrap();
    assert_eq!(
        api(
            &mut m,
            Interface::Shell,
            "LoadResString",
            [loader::OBJECT_BASE, name, 8, DATE + 64]
        ),
        0
    );
}

fn privilege_manifest(class: u32, privileges: u32, dependency: u32) -> Vec<u8> {
    let mut mif = vec![0u8; 140];
    mif[..4].copy_from_slice(&[0x11, 0, 1, 0]);
    mif[6..8].copy_from_slice(&3u16.to_le_bytes());
    for (at, value) in [
        (8, 32u32),
        (12, 24),
        (16, 56),
        (20, 3),
        (24, 72),
        (28, 68),
        (56, 72),
        (60, 112),
        (64, 132),
        (68, 140),
        (72, 0x1002),
        (96, privileges),
        (112, class),
        (120, 1000),
        (132, dependency),
    ] {
        mif[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    for (at, value) in [(32, 0x5000u16), (40, 0x5000), (42, 2), (46, 1),
        (48, 0x5000), (50, 3), (54, 2), (102, 1)] {
        mif[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }
    mif
}

#[test]
fn shell_privilege_query_uses_manifest_bits_and_declared_groups() {
    let mut m = machine();
    let class = 0x01017777;
    let dependency = 0x01028888;
    let mif = privilege_manifest(class, 3, dependency);
    assert!(m.set_applet_mif_named(&mif, class, "chosen.mif"));
    let _ = m.create_applet(class, 1000).unwrap();
    for (group, granted) in [
        (0, 1),
        (1, 1),
        (2, 1),
        (3, 1),
        (8, 1),
        (4, 0),
        (class, 1),
        (dependency, 1),
        (0x01abcdef, 0),
        (0xffff, 0),
    ] {
        assert_eq!(
            api(
                &mut m,
                Interface::Shell,
                "CheckPrivLevel",
                [loader::OBJECT_BASE, group, 1, 0]
            ),
            granted,
            "{group:#x}"
        );
        assert!(!m.pediu_para_fechar());
    }
    // Permissão para uma dependência não anuncia a implementação da classe.
    assert_eq!(
        api(
            &mut m,
            Interface::Shell,
            "QueryClass",
            [loader::OBJECT_BASE, dependency, 0, 0]
        ),
        0
    );
    assert_eq!(
        api(
            &mut m,
            Interface::Shell,
            "CheckPrivLevel",
            [loader::OBJECT_BASE, 4, 0, 0]
        ),
        0
    );
    assert!(m.pediu_para_fechar());
}

#[test]
fn selected_manifest_resources_keep_the_exact_emitted_filename_bytes() {
    let mut m = machine();
    let class = 0x01017777;
    assert!(m.set_applet_mif_named(&privilege_manifest(class, 3, 0), class, "café.mif"));
    assert_eq!(
        api(
            &mut m,
            Interface::Shell,
            "QueryClass",
            [loader::OBJECT_BASE, class, DATE, 0]
        ),
        1
    );
    let name = m.cpu().read_u32(DATE + 4).unwrap();
    assert_eq!(m.cpu().read_cbytes(name, 64), "fs:/~/café.mif".as_bytes());
    assert_ne!(
        api(
            &mut m,
            Interface::Shell,
            "LoadResData",
            [loader::OBJECT_BASE, name, 0, 0x5000]
        ),
        0
    );
}

#[test]
fn denied_nonquery_privilege_stops_before_guest_continuation() {
    let mut m = machine();
    let slot = (0..256)
        .find(|slot| Interface::Shell.method(*slot) == Some("CheckPrivLevel"))
        .unwrap();
    let code = [
        0xe92d4010u32,
        0xe1a04003,
        0xe59fc010,
        0xe12fff3c,
        0xe3a01001,
        0xe5841000,
        0xe8bd4010,
        0xe12fff1e,
        aee::encode(Interface::Shell, slot),
    ];
    m.cpu_mut()
        .write_mem(
            DATE + 256,
            &code
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    m.cpu_mut().write_u32(DATE, 0).unwrap();
    assert!(matches!(
        m.call_guest(DATE + 256, [loader::OBJECT_BASE, 4, 0, DATE], 1000)
            .unwrap(),
        Outcome::Returned { .. }
    ));
    assert!(m.pediu_para_fechar());
    assert_eq!(
        m.cpu().read_u32(DATE).unwrap(),
        0,
        "a instrução após a negação não pode executar"
    );
}

#[test]
fn restoring_before_privilege_denial_resumes_and_old_states_default_to_running() {
    let mut m = machine();
    m.cpu_mut().write_mem(DATE, b"abc\0").unwrap();
    let before = m.grava_estado();
    assert_eq!(
        api(
            &mut m,
            Interface::Shell,
            "CheckPrivLevel",
            [loader::OBJECT_BASE, 4, 0, 0]
        ),
        0
    );
    let denied = m.grava_estado();
    m.restaura_estado(&before).unwrap();
    assert!(!m.pediu_para_fechar());
    assert_eq!(helper(&mut m, "strlen", [DATE, 0, 0, 0]), 3);
    m.restaura_estado(&denied).unwrap();
    assert!(m.pediu_para_fechar());
    assert_eq!(helper(&mut m, "strlen", [DATE, 0, 0, 0]), 0);
    let reader = zeebx::save_state::Leitor::abre(&before).unwrap();
    let old = zeebx::save_state::escreve(
        &reader
            .nomes()
            .into_iter()
            .filter(|name| *name != "esc.privilege_denied")
            .map(|name| (name.to_owned(), reader.secao(name).unwrap().to_vec()))
            .collect::<Vec<_>>(),
    );
    m.restaura_estado(&old).unwrap();
    assert_eq!(helper(&mut m, "strlen", [DATE, 0, 0, 0]), 3);
}
