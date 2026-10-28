use std::path::PathBuf;
use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend};
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

const FIRST: u32 = loader::MODULE_BASE + 0x20;
const SECOND: u32 = loader::MODULE_BASE + 0x80;
const MARKER: u32 = loader::HEAP_BASE + 0x10000;

fn slot(name: &str) -> u32 {
    (0..256)
        .find(|index| Interface::Shell.method(*index) == Some(name))
        .unwrap()
}

fn machine(first: &[u32]) -> (Machine<BackendPadrao>, PathBuf) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut image = vec![0u8; 0x100];
    for (offset, words) in [
        (0usize, &[0xe3a00000, 0xe12fff1e][..]),
        (0x20, first),
        (0x80, &[0xe3a01002, 0xe5801000, 0xe12fff1e][..]),
    ] {
        for (index, word) in words.iter().enumerate() {
            image[offset + index * 4..offset + index * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
    }
    let image = ModImage::parse(image).unwrap();
    let loaded = loader::load_with(&image, &[]).unwrap();
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    let work = workspace.join("local-tests").join(format!(
        "events-{}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&work).unwrap();
    assert!(work.canonicalize().unwrap().starts_with(workspace));
    let storage = StoragePaths::from_root(work.join("runtime"));
    storage.create_dirs().unwrap();
    let mut machine =
        Machine::new_with_storage(BackendPadrao::new().unwrap(), loaded, &work, &storage, None);
    assert_eq!(machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
    (machine, work)
}

fn shell(machine: &mut Machine<BackendPadrao>, name: &str, args: [u32; 3]) -> u32 {
    let receiver = machine.module().shell;
    match machine
        .call_guest(
            aee::encode(Interface::Shell, slot(name)),
            [receiver, args[0], args[1], args[2]],
            1000,
        )
        .unwrap()
    {
        Outcome::Returned { code } => code,
        other => panic!("{name}: {other:?}"),
    }
}

#[test]
fn budgeted_timer_callback_finishes_before_another_due_timer_starts() {
    let (mut machine, work) = machine(&[
        0xe3a010c8, 0xe2511001, 0x1afffffd, // 200 voltas; o corte cai no laço.
        0xe3a01001, 0xe5801000, 0xe12fff1e,
    ]);
    shell(&mut machine, "SetTimer", [0, FIRST, MARKER]);
    shell(&mut machine, "SetTimer", [0, SECOND, MARKER + 4]);
    assert_eq!(machine.advance(16).unwrap(), vec![Outcome::Budget]);
    assert_eq!(machine.next_guest_work_delay(), Some(std::time::Duration::ZERO));
    assert_eq!(machine.cpu().read_u32(MARKER + 4).unwrap(), 0);
    assert_eq!(machine.armed_timers(), 1);
    for _ in 0..100 {
        if machine.advance(16).unwrap() == vec![Outcome::Returned { code: MARKER }] {
            break;
        }
    }
    assert_eq!(machine.cpu().read_u32(MARKER).unwrap(), 1);
    assert_eq!(machine.cpu().read_u32(MARKER + 4).unwrap(), 0);
    machine.advance(1000).unwrap();
    assert_eq!(machine.cpu().read_u32(MARKER + 4).unwrap(), 2);
    drop(machine);
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn timer_cancelled_before_dispatch_never_runs() {
    let (mut machine, work) = machine(&[
        0xe92d4010,
        0xe59f0010,
        0xe59f1010,
        0xe59f2010,
        0xe59f3010,
        0xe12fff33,
        0xe8bd8010,
        loader::OBJECT_BASE,
        SECOND,
        MARKER + 4,
        aee::encode(Interface::Shell, slot("CancelTimer")),
    ]);
    shell(&mut machine, "SetTimer", [0, FIRST, MARKER]);
    shell(&mut machine, "SetTimer", [0, SECOND, MARKER + 4]);
    machine.advance(1000).unwrap();
    machine.advance(1000).unwrap();
    assert_eq!(machine.cpu().read_u32(MARKER + 4).unwrap(), 0);
    assert_eq!(machine.armed_timers(), 0);
    drop(machine);
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn timer_deadline_and_expiration_survive_u32_clock_wrap() {
    let (mut machine, work) = machine(&[0xe12fff1e]);
    // Esta sonda exercita o relógio determinístico herdado, não fixa a frequência de handset.
    machine
        .cpu_mut()
        .set_instructions(u64::from(u32::MAX - 5) * 528_000);
    assert_eq!(machine.clock_ms(), u32::MAX - 5);
    shell(&mut machine, "SetTimer", [10, SECOND, MARKER]);
    assert_eq!(machine.next_guest_work_delay(), Some(std::time::Duration::from_millis(10)));
    assert_eq!(
        shell(&mut machine, "GetTimerExpiration", [SECOND, MARKER, 0]),
        10
    );
    machine.advance(1000).unwrap();
    assert_eq!(machine.cpu().read_u32(MARKER).unwrap(), 2);
    assert!(machine.clock_ms() < 16);
    drop(machine);
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn idle_poll_delay_uses_the_earliest_live_timer_without_advancing_the_clock() {
    let (mut machine, work) = machine(&[0xe12fff1e]);
    assert_eq!(machine.next_guest_work_delay(), None);
    shell(&mut machine, "SetTimer", [100, FIRST, MARKER]);
    shell(&mut machine, "SetTimer", [20, SECOND, MARKER + 4]);
    let clock = machine.clock_ms();
    assert_eq!(machine.next_guest_work_delay(), Some(std::time::Duration::from_millis(20)));
    assert_eq!(machine.clock_ms(), clock);
    shell(&mut machine, "CancelTimer", [SECOND, MARKER + 4, 0]);
    assert_eq!(machine.next_guest_work_delay(), Some(std::time::Duration::from_millis(100)));
    shell(&mut machine, "SetTimer", [0, SECOND, MARKER + 4]);
    assert_eq!(machine.next_guest_work_delay(), Some(std::time::Duration::ZERO));
    drop(machine);
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn queued_key_waits_while_a_timer_callback_is_suspended() {
    let (mut machine, work) = machine(&[0xe3a010c8, 0xe2511001, 0x1afffffd, 0xe12fff1e]);
    let applet = MARKER + 0x100;
    let table = applet + 0x100;
    let handler = loader::MODULE_BASE + 0xb0;
    machine
        .cpu_mut()
        .write_mem(
            handler,
            &[0xe5801004u32.to_le_bytes(), 0xe12fff1eu32.to_le_bytes()].concat(),
        )
        .unwrap();
    machine.cpu_mut().write_u32(applet, table).unwrap();
    machine.cpu_mut().write_u32(table + 8, handler).unwrap();
    machine.start_applet(applet, 0, 1000).unwrap();
    machine.cpu_mut().write_u32(applet + 4, 0).unwrap();
    shell(&mut machine, "SetTimer", [0, FIRST, MARKER]);
    assert_eq!(machine.advance(16).unwrap(), vec![Outcome::Budget]);
    machine.set_key(0xe035, true);
    machine.deliver_signals(1000).unwrap();
    machine.deliver_callbacks(1000).unwrap();
    assert_eq!(
        machine.cpu().read_u32(applet + 4).unwrap(),
        0,
        "a tecla deve continuar na fila"
    );
    assert!(
        !machine.is_idle(),
        "uma continuação pendente não é ociosidade"
    );
    for _ in 0..100 {
        if machine
            .advance(16)
            .unwrap()
            .iter()
            .all(|outcome| matches!(outcome, Outcome::Returned { .. }))
        {
            break;
        }
    }
    machine.deliver_signals(1000).unwrap();
    assert_eq!(machine.cpu().read_u32(applet + 4).unwrap(), 0x100);
    drop(machine);
    std::fs::remove_dir_all(work).unwrap();
}

fn input_applet(machine: &mut Machine<BackendPadrao>) -> u32 {
    let applet = MARKER + 0x100;
    let table = applet + 0x100;
    let handler = loader::MODULE_BASE + 0xb0;
    let code = [0xe5801004u32, 0xe5802008, 0xe580300c, 0xe3a00001, 0xe12fff1e];
    machine
        .cpu_mut()
        .write_mem(
            handler,
            &code
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    machine.cpu_mut().write_u32(applet, table).unwrap();
    machine.cpu_mut().write_u32(table + 8, handler).unwrap();
    assert_eq!(
        machine.start_applet(applet, 0, 1000).unwrap(),
        Outcome::Returned { code: 1 }
    );
    applet
}

#[test]
fn handset_pen_marshals_down_move_up_coordinates_and_waits_for_a_suspended_handler() {
    use zeebx::handset::input::{PenEvent, PenKind};
    let (mut machine, work) = machine(&[0xe3a010c8, 0xe2511001, 0x1afffffd, 0xe12fff1e]);
    let applet = legacy_pen_applet(&mut machine);
    for kind in [PenKind::Down, PenKind::Move, PenKind::Up] {
        let event = PenEvent { kind, x:799, y:479 };
        assert_eq!(machine.deliver_handset_pen(event,1000).unwrap(),Outcome::Returned{code:1});
        assert_eq!(machine.cpu().read_u32(applet+4).unwrap(),kind.code());
        assert_eq!(machine.cpu().read_u32(applet+8).unwrap(),0);
        assert_eq!(machine.cpu().read_u32(applet+12).unwrap(),0x031f01df);
    }
    shell(&mut machine,"SetTimer",[0,FIRST,MARKER]);
    assert_eq!(machine.advance(16).unwrap(),vec![Outcome::Budget]);
    assert!(machine.deliver_handset_pen(PenEvent{kind:PenKind::Down,x:1,y:2},1000).is_err());
    assert_eq!(machine.cpu().read_u32(applet+4).unwrap(),PenKind::Up.code());
    drop(machine);
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn handset_input_marshals_the_exact_event_and_refuses_an_active_continuation() {
    use zeebx::handset::input::{Key, KeyEvent, KeyKind};
    let (mut machine, work) = machine(&[0xe3a010c8, 0xe2511001, 0x1afffffd, 0xe12fff1e]);
    let applet = input_applet(&mut machine);
    let event = KeyEvent {
        key: Key::Select,
        kind: KeyKind::Press,
        timestamp_us: 0,
    };
    assert_eq!(
        machine.deliver_handset_event(event, 1000).unwrap(),
        Outcome::Returned { code: 1 }
    );
    assert_eq!(machine.cpu().read_u32(applet + 4).unwrap(), 0x101);
    assert_eq!(machine.cpu().read_u32(applet + 8).unwrap(), 0xe035);
    shell(&mut machine, "SetTimer", [0, FIRST, MARKER]);
    assert_eq!(machine.advance(16).unwrap(), vec![Outcome::Budget]);
    assert!(machine.deliver_handset_event(event, 1000).is_err());
    drop(machine);
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn handset_input_does_not_swallow_a_handler_instruction_limit() {
    use zeebx::handset::input::{Key, KeyEvent, KeyKind};
    let (mut machine, work) = machine(&[0xe12fff1e]);
    input_applet(&mut machine);
    let handler = loader::MODULE_BASE + 0xb0;
    let code = [
        0xe3a030c8u32,
        0xe2533001,
        0x1afffffd,
        0xe3a00001,
        0xe12fff1e,
    ];
    machine
        .cpu_mut()
        .write_mem(
            handler,
            &code
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    let event = KeyEvent {
        key: Key::Num5,
        kind: KeyKind::Key,
        timestamp_us: 0,
    };
    assert_eq!(
        machine.deliver_handset_event(event, 16).unwrap(),
        Outcome::Budget
    );
    let mut finished = false;
    for _ in 0..100 {
        if machine.advance(16).unwrap() == vec![Outcome::Returned { code: 1 }] {
            finished = true;
            break;
        }
    }
    assert!(finished);
    drop(machine);
    std::fs::remove_dir_all(work).unwrap();
}


#[test]
fn handset_pointer_marshals_sdk_text_and_does_not_also_send_legacy_pen() {
    use zeebx::handset::input::{PenEvent, PenKind};
    use zeebx::cpu::Reg;
    let (mut machine, work) = machine(&[0xe12fff1e]);
    let applet = input_applet(&mut machine);
    let sp = machine.cpu().read_reg(Reg::Sp);
    for (kind, code) in [(PenKind::Down,0x50b),(PenKind::Move,0x50c),(PenKind::Up,0x50d)] {
        let timestamp=machine.clock_ms();
        assert_eq!(machine.deliver_handset_pen(PenEvent{kind,x:239,y:399},1000).unwrap(),Outcome::Returned{code:1});
        assert_eq!(machine.cpu().read_u32(applet+4).unwrap(),code);
        let text=format!("x=000000EF,y=0000018F,time={timestamp:08X},clkcnt=1,modifiers=0000000000000000,\0");
        let length=machine.cpu().read_u32(applet+8).unwrap();
        assert_eq!(length,text.len() as u32);
        let pointer=machine.cpu().read_u32(applet+12).unwrap();
        let mut bytes=vec![0;text.len()];machine.cpu().read_mem(pointer,&mut bytes).unwrap();
        assert_eq!(bytes,text.as_bytes());
        assert_eq!(machine.cpu().read_reg(Reg::Sp),sp);
    }
    drop(machine);std::fs::remove_dir_all(work).unwrap();
}


fn legacy_pen_applet(machine: &mut Machine<BackendPadrao>) -> u32 {
    let applet=input_applet(machine);
    let handler=MARKER+0x1000;
    // Registra todas as entregas; eventos novos trabalham antes de recusar o formato.
    let code=[0xe590c010u32,0xe28cc001,0xe580c010,0xe5801004,0xe5802008,0xe580300c,
        0xe59fc020,0xe151000c,0x3a000004,0xe3a020c8,0xe2522001,0x1afffffd,
        0xe3a00000,0xe12fff1e,0xe3a00001,0xe12fff1e,0x50b];
    machine.cpu_mut().write_mem(handler,&code.into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>()).unwrap();
    machine.cpu_mut().write_u32(applet+0x100+8,handler).unwrap();
    applet
}

#[test]
fn pointer_fallback_survives_budget_and_save_without_duplicate_delivery_or_stack_leak() {
    use zeebx::handset::input::{PenEvent,PenKind};
    use zeebx::cpu::Reg;
    let (mut machine,work)=machine(&[0xe12fff1e]);
    // O save corta o heap no último bloco alocado; o applet de teste precisa pertencer a ele.
    machine.call_guest(aee::encode(Interface::Helpers,26),[0x12000,0,0,0],1000).unwrap();
    let applet=legacy_pen_applet(&mut machine);
    let sp=machine.cpu().read_reg(Reg::Sp);
    assert_eq!(machine.deliver_handset_pen(PenEvent{kind:PenKind::Down,x:799,y:479},16).unwrap(),Outcome::Budget);
    assert!(machine.deliver_handset_pen(PenEvent{kind:PenKind::Up,x:0,y:0},1000).is_err());
    let saved=machine.grava_estado();machine.restaura_estado(&saved).unwrap();
    let mut returned=false;
    for _ in 0..100 {
        if machine.advance(16).unwrap()==vec![Outcome::Returned{code:1}] {returned=true;break;}
    }
    assert!(returned);
    assert_eq!(machine.cpu().read_u32(applet+4).unwrap(),0x504);
    assert_eq!(machine.cpu().read_u32(applet+8).unwrap(),0);
    assert_eq!(machine.cpu().read_u32(applet+12).unwrap(),0x031f01df);
    assert_eq!(machine.cpu().read_u32(applet+16).unwrap(),2);
    assert_eq!(machine.cpu().read_reg(Reg::Sp),sp);
    drop(machine);std::fs::remove_dir_all(work).unwrap();
}


#[test]
fn pointer_dispatch_interworks_with_thumb_applet_handlers() {
    use zeebx::handset::input::{PenEvent,PenKind};
    use zeebx::cpu::Reg;
    let (mut machine,work)=machine(&[0xe12fff1e]);
    let applet=input_applet(&mut machine);
    let handler=MARKER+0x1000;
    let thumb=[0x6041u16,0x6082,0x60c3,0x2001,0x4770];
    machine.cpu_mut().write_mem(handler,&thumb.into_iter().flat_map(u16::to_le_bytes).collect::<Vec<_>>()).unwrap();
    machine.cpu_mut().write_u32(applet+0x100+8,handler|1).unwrap();
    let sp=machine.cpu().read_reg(Reg::Sp);
    assert_eq!(machine.deliver_handset_pen(PenEvent{kind:PenKind::Down,x:0,y:0},1000).unwrap(),Outcome::Returned{code:1});
    assert_eq!(machine.cpu().read_u32(applet+4).unwrap(),0x50b);
    assert_eq!(machine.cpu().read_reg(Reg::Sp),sp);
    drop(machine);std::fs::remove_dir_all(work).unwrap();
}
