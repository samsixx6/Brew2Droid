use zeebx::{brew::aee::{self, Interface}, cpu::{BackendPadrao, CpuBackend}, loader::{self, modfile::ModImage}, machine::{Machine, Outcome}};

const OUT: u32 = loader::STACK_BASE + 0x100;

fn machine() -> Machine<BackendPadrao> {
    let code = [0xe3a00000u32, 0xe12fff1e].into_iter().flat_map(u32::to_le_bytes).collect();
    let mut m = Machine::new(BackendPadrao::new().unwrap(), loader::load_with(&ModImage::parse(code).unwrap(), &[]).unwrap(), ".");
    m.run(1000).unwrap();
    m
}
fn api(m: &mut Machine<BackendPadrao>, iface: Interface, name: &str, args: [u32;4]) -> u32 {
    let slot = (0..256).find(|&n| iface.method(n) == Some(name)).unwrap();
    match m.call_guest(aee::encode(iface, slot), args, 10000).unwrap() {
        Outcome::Returned { code } => code,
        stopped => panic!("{name}: {stopped:?}"),
    }
}
fn create(m: &mut Machine<BackendPadrao>, cls: u32) -> u32 {
    assert_eq!(api(m, Interface::Shell, "CreateInstance", [loader::OBJECT_BASE,cls,OUT,0]),0);
    m.cpu().read_u32(OUT).unwrap()
}

#[test]
fn offline_network_linger_returns_previous_uint16_and_survives_save_restore() {
    let mut m = machine();
    let net = create(&mut m, 0x01001105);
    assert_eq!(api(&mut m, Interface::NetLegacy, "SetLinger", [net, 12, 0, 0]),30);
    assert_eq!(api(&mut m, Interface::NetLegacy, "SetLinger", [net, 0x10007, 0, 0]),12);
    let state = m.grava_estado();
    assert_eq!(api(&mut m, Interface::NetLegacy, "SetLinger", [net, 0, 0, 0]),7);
    m.restaura_estado(&state).unwrap();
    assert_eq!(api(&mut m, Interface::NetLegacy, "SetLinger", [net, 0, 0, 0]),7);
    assert_eq!(api(&mut m, Interface::NetLegacy, "GetMyIPAddr", [net,0,0,0]),0);
    api(&mut m, Interface::NetLegacy, "Release", [net,0,0,0]);
    let fresh = create(&mut m, 0x01001105);
    assert_eq!(api(&mut m, Interface::NetLegacy, "SetLinger", [fresh, 3, 0, 0]),30);
}

#[test]
fn sms_rejects_invalid_arguments_and_offline_remote_delivery_without_callbacks() {
    let mut m = machine();
    let tapi = create(&mut m, 0x01001007);
    assert_eq!(api(&mut m, Interface::TapiLegacy, "SendSMS", [tapi,0,0,0]),14);
    m.cpu_mut().write_mem(OUT+32,b"123\0hello\0").unwrap();
    assert_eq!(api(&mut m, Interface::TapiLegacy, "SendSMS", [tapi,OUT+32,OUT+36,0]),1);
    assert_eq!(api(&mut m, Interface::TapiLegacy, "SendSMS", [tapi,0,OUT+36,0x12345678]),0);
}
