use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend},
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
};
const INFO: u32 = loader::STACK_BASE + 0x100;

#[test]
fn device_info_reports_a_documented_default_iso_language_for_resource_selection() {
    let image = ModImage::parse(
        [0xe3a0_0000u32, 0xe12f_ff1e]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect(),
    )
    .unwrap();
    let mut machine = Machine::new(
        BackendPadrao::new().unwrap(),
        loader::load_with(&image, &[]).unwrap(),
        ".",
    );
    assert_eq!(machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
    machine.enable_handset_clock();
    machine.cpu_mut().write_mem(INFO, &[0xa5; 48]).unwrap();
    machine
        .cpu_mut()
        .write_mem(INFO + 44, &0u16.to_le_bytes())
        .unwrap();
    let slot = (0..256)
        .find(|&slot| Interface::Shell.method(slot) == Some("GetDeviceInfo"))
        .unwrap();
    assert_eq!(
        machine
            .call_guest(
                aee::encode(Interface::Shell, slot),
                [loader::OBJECT_BASE, INFO, 0, 0],
                10000
            )
            .unwrap(),
        Outcome::Returned { code: 0 }
    );
    // AEEShell.h: dwLang em +40; AEELngCode.h: ISO 639-1 'en  ', little-endian.
    let language = machine.cpu().read_u32(INFO + 40).unwrap();
    let selected_bar = match language {
        0x2020_687a | 0x6e63_687a => Some("escape_cn.bar"),
        0x2020_6e65 => Some("escape_en.bar"),
        _ => None,
    };
    assert_eq!(language, 0x2020_6e65);
    assert_eq!(
        selected_bar,
        Some("escape_en.bar"),
        "o seletor real não pode receber idioma indefinido"
    );
    assert_eq!(
        machine.cpu().read_u32(INFO + 44).unwrap(),
        0xa5a5_0000,
        "a estrutura antiga não ganha campos extras"
    );
}

#[test]
fn console_language_is_portuguese_and_stable_for_old_and_extended_device_info() {
    let image = ModImage::parse(
        [0xe3a0_0000u32, 0xe12f_ff1e]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect(),
    )
    .unwrap();
    let mut machine = Machine::new(
        BackendPadrao::new().unwrap(),
        loader::load_with(&image, &[]).unwrap(),
        ".",
    );
    assert_eq!(machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
    let slot = (0..256)
        .find(|&slot| Interface::Shell.method(slot) == Some("GetDeviceInfo"))
        .unwrap();
    for requested in [0u16, 60, 64, 65535] {
        machine.cpu_mut().write_mem(INFO, &[0xa5; 68]).unwrap();
        machine
            .cpu_mut()
            .write_mem(INFO + 44, &requested.to_le_bytes())
            .unwrap();
        assert_eq!(
            machine
                .call_guest(
                    aee::encode(Interface::Shell, slot),
                    [loader::OBJECT_BASE, INFO, 0, 0],
                    10000
                )
                .unwrap(),
            Outcome::Returned { code: 0 }
        );
        assert_eq!(
            machine.cpu().read_u32(INFO + 40).unwrap(),
            0x2020_7470,
            "wStructSize={requested}"
        );
        assert_eq!(machine.cpu().read_u32(INFO + 64).unwrap(), 0xa5a5_a5a5);
        if requested != 64 {
            assert_eq!(machine.cpu().read_u32(INFO + 48).unwrap(), 0xa5a5_a5a5);
        }
    }
}
