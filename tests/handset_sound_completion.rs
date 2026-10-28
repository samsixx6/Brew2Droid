use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend};
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};

const MARKER: u32 = loader::HEAP_BASE + 0x800;
const CALLBACK: u32 = loader::HEAP_BASE + 0x1000;
const COUNT: u32 = MARKER + 16;
fn machine() -> Machine<BackendPadrao> {
    let image =
        ModImage::parse([0xe3a00000u32.to_le_bytes(), 0xe12fff1eu32.to_le_bytes()].concat())
            .unwrap();
    let mut m = Machine::new(
        BackendPadrao::new().unwrap(),
        loader::load_with(&image, &[]).unwrap(),
        ".",
    );
    m.enable_handset_clock();
    assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
    // STMIA r0,{r0-r3}; mov r0,#0; bx lr: registra a ABI real do callback.
    m.cpu_mut()
        .write_mem(
            CALLBACK,
            &[
                0xe590c010u32,
                0xe28cc001,
                0xe580c010,
                0xe880000f,
                0xe3a00000,
                0xe12fff1e,
            ]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>(),
        )
        .unwrap();
    m
}
fn call(m: &mut Machine<BackendPadrao>, iface: Interface, slot: u32, args: [u32; 4]) {
    assert_eq!(
        m.call_guest(aee::encode(iface, slot), args, 1000).unwrap(),
        Outcome::Returned { code: 0 }
    );
}
fn marker(m: &Machine<BackendPadrao>) -> [u32; 4] {
    std::array::from_fn(|i| m.cpu().read_u32(MARKER + i as u32 * 4).unwrap())
}
fn check_completion(slot: u32, argument: u32) {
    for (class, iface) in [
        (0x0100100b, Interface::SoundLegacy),
        (0x01001056, Interface::Sound),
    ] {
        let mut m = machine();
        call(
            &mut m,
            Interface::Shell,
            2,
            [loader::OBJECT_BASE, class, MARKER, 0],
        );
        let sound = m.cpu().read_u32(MARKER).unwrap();
        assert_ne!(sound, 0);
        call(&mut m, iface, 2, [sound, CALLBACK, MARKER, 0]);
        {
            m.cpu_mut().write_u32(COUNT, 0).unwrap();
            m.cpu_mut().write_mem(MARKER, &[0xa5; 16]).unwrap();
            call(&mut m, iface, slot, [sound, argument, 0, 0]);
            assert_eq!(
                marker(&m),
                [0xa5a5a5a5; 4],
                "completion must wait for the guest callback turn"
            );
            let saved = m.grava_estado();
            m.restaura_estado(&saved).unwrap();
            let outcomes = m.deliver_signals(1000).unwrap();
            assert!(
                outcomes
                    .iter()
                    .all(|o| matches!(o, Outcome::Returned { .. })),
                "slot{slot}, class{class:#x}: {outcomes:?}"
            );
            assert_eq!(
                marker(&m),
                [MARKER, 0, 1, 0],
                "PFNSOUNDSTATUS(user,STATUS_CB,SUCCESS,0)"
            );
            assert_eq!(m.cpu().read_u32(COUNT).unwrap(), 1);
            assert!(
                m.deliver_signals(1000).unwrap().is_empty(),
                "exactly one completion"
            );
            assert_eq!(m.cpu().read_u32(COUNT).unwrap(), 1);
        }
        // Sem registro não existe callback e a operação continua disponível.
        call(&mut m, iface, 2, [sound, 0, 0, 0]);
        call(&mut m, iface, 5, [sound, 0, 0, 0]);
        assert!(m.deliver_signals(1000).unwrap().is_empty());
        for release in [false, true] {
            m.cpu_mut().write_u32(COUNT, 0).unwrap();
            call(&mut m, iface, 2, [sound, CALLBACK, MARKER, 0]);
            call(&mut m, iface, slot, [sound, argument, 0, 0]);
            if release {
                call(&mut m, iface, 1, [sound, 0, 0, 0]);
            } else {
                call(&mut m, iface, 2, [sound, 0, 0, 0]);
            }
            assert!(m.deliver_signals(1000).unwrap().is_empty());
            assert_eq!(
                m.cpu().read_u32(COUNT).unwrap(),
                0,
                "cancelled sound must not call the old guest context"
            );
        }
    }
}

#[test]
fn sound_device_completes_asynchronously_after_save_restore() {
    check_completion(5, 0);
}

#[test]
fn sound_volume_completes_asynchronously_after_save_restore() {
    check_completion(12, 73);
}

#[test]
fn sound_callback_can_cancel_the_remaining_completions_in_its_batch() {
    for (class, iface) in [
        (0x0100100b, Interface::SoundLegacy),
        (0x01001056, Interface::Sound),
    ] {
        for cancel_slot in [1, 2] {
            let mut m = machine();
            call(
                &mut m,
                Interface::Shell,
                2,
                [loader::OBJECT_BASE, class, MARKER, 0],
            );
            let sound = m.cpu().read_u32(MARKER).unwrap();
            m.cpu_mut().write_u32(MARKER, sound).unwrap();
            m.cpu_mut()
                .write_u32(MARKER + 4, aee::encode(iface, cancel_slot))
                .unwrap();
            m.cpu_mut().write_u32(COUNT, 0).unwrap();
            // O primeiro aviso cancela o dono ou seu registro dentro do próprio callback.
            let code = [
                0xe92d4010u32,
                0xe1a04000,
                0xe594c010,
                0xe28cc001,
                0xe584c010,
                0xe5940000,
                0xe594c004,
                0xe3a01000,
                0xe3a02000,
                0xe3a03000,
                0xe12fff3c,
                0xe3a00000,
                0xe8bd8010,
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
            call(&mut m, iface, 2, [sound, CALLBACK, MARKER, 0]);
            call(&mut m, iface, 5, [sound, 0, 0, 0]);
            call(&mut m, iface, 12, [sound, 73, 0, 0]);
            let outcomes = m.deliver_signals(1000).unwrap();
            assert!(
                outcomes
                    .iter()
                    .all(|o| matches!(o, Outcome::Returned { .. })),
                "{outcomes:?}"
            );
            assert_eq!(
                m.cpu().read_u32(COUNT).unwrap(),
                1,
                "release/unregister in first completion must cancel second, slot{cancel_slot}"
            );
            assert!(m.deliver_signals(1000).unwrap().is_empty());
            assert_eq!(m.cpu().read_u32(COUNT).unwrap(), 1);
        }
    }
}

#[test]
fn sound_completions_keep_fifo_order_across_guest_callback_turns() {
    let mut m = machine();
    let iface = Interface::SoundLegacy;
    call(
        &mut m,
        Interface::Shell,
        2,
        [loader::OBJECT_BASE, 0x0100100b, MARKER, 0],
    );
    let sound = m.cpu().read_u32(MARKER).unwrap();
    m.cpu_mut().write_u32(COUNT, 0).unwrap();
    call(&mut m, iface, 2, [sound, CALLBACK, MARKER, 0]);
    call(&mut m, iface, 5, [sound, 0, 0, 0]);
    call(&mut m, iface, 13, [sound, 0, 0, 0]);
    assert_eq!(m.deliver_signals(1000).unwrap().len(), 1);
    assert_eq!(marker(&m), [MARKER, 0, 1, 0]);
    assert_eq!(m.cpu().read_u32(COUNT).unwrap(), 1);
    assert_eq!(m.deliver_signals(1000).unwrap().len(), 1);
    let result = marker(&m);
    assert_eq!(result[..3], [MARKER, 1, 1]);
    assert_eq!(m.cpu().read_u32(result[3]).unwrap(), 100);
    assert_eq!(m.cpu().read_u32(COUNT).unwrap(), 2);
}
