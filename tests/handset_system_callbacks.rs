use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend},
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
};

const PCB: u32 = loader::STACK_BASE + 0x100;
const CONTEXT: u32 = PCB + 64;
const CALLBACK: u32 = PCB + 256;

fn machine() -> Machine<BackendPadrao> {
    let code = [0xe3a00000u32, 0xe12fff1e]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    let mut m = Machine::new(
        BackendPadrao::new().unwrap(),
        loader::load_with(&ModImage::parse(code).unwrap(), &[]).unwrap(),
        ".",
    );
    assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
    m
}

fn api(m: &mut Machine<BackendPadrao>, iface: Interface, name: &str, args: [u32; 4]) -> u32 {
    let slot = (0..256)
        .find(|&slot| iface.method(slot) == Some(name))
        .unwrap();
    match m.call_guest(aee::encode(iface, slot), args, 10000).unwrap() {
        Outcome::Returned { code } => code,
        other => panic!("{name}: {other:?}"),
    }
}

fn malloc(m: &mut Machine<BackendPadrao>, size: u32) -> u32 {
    api(
        m,
        Interface::Helpers,
        "malloc",
        [size | 0x80000000, 0, 0, 0],
    )
}

fn fill_and_register(m: &mut Machine<BackendPadrao>, event: u32) {
    let cache = malloc(m, 512 * 1024);
    assert_ne!(cache, 0);
    let remain = m.heap_retrato().maior_buraco;
    assert_ne!(malloc(m, remain - 1024), 0);
    m.cpu_mut().write_u32(CONTEXT, cache).unwrap();
    m.cpu_mut().write_u32(CONTEXT + 4, 0).unwrap();
    let free_slot = (0..256)
        .find(|&slot| Interface::Helpers.method(slot) == Some("free"))
        .unwrap();
    // Libera cache real, limpa o ponteiro e incrementa um contador no contexto do guest.
    let code = [
        0xe92d4010u32,
        0xe1a04000,
        0xe5940000,
        0xe59fc018,
        0xe12fff3c,
        0xe3a01000,
        0xe5841000,
        0xe5941004,
        0xe2811001,
        0xe5841004,
        0xe8bd8010,
        aee::encode(Interface::Helpers, free_slot),
    ];
    m.cpu_mut()
        .write_mem(
            CALLBACK,
            &code
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    m.cpu_mut().write_mem(PCB, &[0; 28]).unwrap();
    m.cpu_mut().write_u32(PCB + 16, CALLBACK).unwrap();
    m.cpu_mut().write_u32(PCB + 20, CONTEXT).unwrap();
    api(
        m,
        Interface::Shell,
        "RegisterSystemCallback",
        [loader::OBJECT_BASE, PCB, event, 0],
    );
    assert_eq!(
        m.cpu().read_u32(CONTEXT + 4).unwrap(),
        0,
        "registro não deve executar callback"
    );
}

#[test]
fn low_ram_callback_frees_real_cache_before_allocation_is_retried() {
    let mut m = machine();
    fill_and_register(&mut m, 2);
    assert_ne!(m.cpu().read_u32(PCB + 8).unwrap(), 0);
    assert_ne!(malloc(&mut m, 16 * 1024), 0);
    assert_eq!(m.cpu().read_u32(CONTEXT + 4).unwrap(), 1);
    assert_eq!(m.cpu().read_u32(PCB + 8).unwrap(), 0);
}

#[test]
fn critical_callback_also_runs_below_threshold_after_a_successful_allocation() {
    let mut m = machine();
    fill_and_register(&mut m, 7);
    assert!(m.heap_retrato().livre > 128 && m.heap_retrato().livre < 64 * 1024);
    assert_ne!(malloc(&mut m, 32), 0);
    assert_eq!(m.cpu().read_u32(CONTEXT + 4).unwrap(), 1);
}

#[test]
fn synchronous_cleanup_is_not_limited_by_the_outer_instruction_quantum() {
    let mut m = machine();
    fill_and_register(&mut m, 2);
    let wrapper = CALLBACK + 0x200;
    let words = std::iter::repeat_n(0xe1a00000u32,1000).chain([0xe51ff004,CALLBACK]);
    m.cpu_mut().write_mem(wrapper,&words.flat_map(u32::to_le_bytes).collect::<Vec<_>>()).unwrap();
    m.cpu_mut().write_u32(PCB+16,wrapper).unwrap();
    let slot = (0..256).find(|&n| Interface::Helpers.method(n)==Some("malloc")).unwrap();
    let mut result = m.call_guest(aee::encode(Interface::Helpers,slot),[16*1024,0,0,0],64).unwrap();
    for _ in 0..100 { if result!=Outcome::Budget {break;} result=m.advance(64).unwrap().into_iter().last().unwrap_or(Outcome::Budget); }
    assert!(matches!(result,Outcome::Returned{code} if code!=0));
    assert_eq!(m.cpu().read_u32(CONTEXT+4).unwrap(),1);
}

#[test]
fn replacing_a_system_registration_can_finish_its_cancel_stub_with_a_small_outer_quantum() {
    let mut m = machine();
    fill_and_register(&mut m, 2);
    let cancel=m.cpu().read_u32(PCB+8).unwrap();
    let wrapper=CALLBACK+0x200;
    let words=std::iter::repeat_n(0xe1a00000u32,1000).chain([0xe51ff004,cancel]);
    m.cpu_mut().write_mem(wrapper,&words.flat_map(u32::to_le_bytes).collect::<Vec<_>>()).unwrap();
    m.cpu_mut().write_u32(PCB+8,wrapper).unwrap();
    let slot = (0..256).find(|&n| Interface::Shell.method(n)==Some("RegisterSystemCallback")).unwrap();
    let mut result=m.call_guest(aee::encode(Interface::Shell,slot),[loader::OBJECT_BASE,PCB,7,0],64).unwrap();
    for _ in 0..100 { if result!=Outcome::Budget {break;} result=m.advance(64).unwrap().into_iter().last().unwrap_or(Outcome::Budget); }
    assert_eq!(result,Outcome::Returned{code:0});
    assert_ne!(malloc(&mut m,16*1024),0);
    assert_eq!(m.cpu().read_u32(CONTEXT+4).unwrap(),1);
}

#[test]
fn storage_registration_is_cancelable_and_is_not_a_heap_pressure_event() {
    let mut m = machine();
    fill_and_register(&mut m, 3);
    let saved = m.grava_estado();
    assert_eq!(malloc(&mut m, 16 * 1024), 0);
    assert_eq!(m.cpu().read_u32(CONTEXT + 4).unwrap(), 0);
    m.restaura_estado(&saved).unwrap();
    let cancel = m.cpu().read_u32(PCB + 8).unwrap();
    assert_ne!(cancel, 0);
    assert!(matches!(m.call_guest(cancel, [PCB,0,0,0],1000).unwrap(), Outcome::Returned { .. }));
    assert_eq!(m.cpu().read_u32(PCB + 8).unwrap(),0);
}

#[test]
fn cancelled_low_ram_callback_does_not_run_and_oom_stays_honest() {
    let mut m = machine();
    fill_and_register(&mut m, 2);
    let cancel = m.cpu().read_u32(PCB + 8).unwrap();
    assert!(matches!(
        m.call_guest(cancel, [PCB, 0, 0, 0], 1000).unwrap(),
        Outcome::Returned { .. }
    ));
    assert_eq!(malloc(&mut m, 16 * 1024), 0);
    assert_eq!(m.cpu().read_u32(CONTEXT + 4).unwrap(), 0);
}

#[test]
fn reregistered_identity_uses_critical_event_only_once() {
    let mut m = machine();
    fill_and_register(&mut m, 2);
    api(
        &mut m,
        Interface::Shell,
        "RegisterSystemCallback",
        [loader::OBJECT_BASE, PCB, 7, 0],
    );
    assert_ne!(malloc(&mut m, 16 * 1024), 0);
    assert_eq!(m.cpu().read_u32(CONTEXT + 4).unwrap(), 1);
}

#[test]
fn low_ram_registration_survives_save_restore_and_old_states_have_none() {
    let mut m = machine();
    fill_and_register(&mut m, 2);
    let before = m.grava_estado();
    assert_ne!(malloc(&mut m, 16 * 1024), 0);
    m.restaura_estado(&before).unwrap();
    assert_ne!(malloc(&mut m, 16 * 1024), 0);
    assert_eq!(m.cpu().read_u32(CONTEXT + 4).unwrap(), 1);
    let reader = zeebx::save_state::Leitor::abre(&before).unwrap();
    let old = zeebx::save_state::escreve(
        &reader
            .nomes()
            .into_iter()
            .filter(|name| *name != "agenda.systemcallbacks")
            .map(|name| (name.to_owned(), reader.secao(name).unwrap().to_vec()))
            .collect::<Vec<_>>(),
    );
    m.restaura_estado(&old).unwrap();
    assert_eq!(malloc(&mut m, 16 * 1024), 0);
    assert_eq!(m.cpu().read_u32(CONTEXT + 4).unwrap(), 0);
}
