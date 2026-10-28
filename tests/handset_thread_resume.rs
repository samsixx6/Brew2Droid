use std::path::PathBuf;

use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend, Reg};
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

const OUT: u32 = loader::STACK_BASE + 0x100;
const OUTPUT: u32 = loader::STACK_BASE + 0x40000;

struct Harness {
    machine: Machine<BackendPadrao>,
    work: PathBuf,
}

impl Harness {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let work = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests")
            .join(format!(
                "thread-resume-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            ));
        std::fs::create_dir_all(&work).unwrap();
        let storage = StoragePaths::from_root(work.join("runtime"));
        storage.create_dirs().unwrap();
        let mut code = vec![0u8; 0x100];
        for (offset, words) in [
            (0, vec![0xe3a0_0000u32, 0xe12f_ff1e]),
            // O orçamento corta o laço com um frame ainda vivo na pilha da thread.
            (
                0x60,
                vec![
                    0xe92d_4070,
                    0xe1a0_4000,
                    0xe1a0_5001,
                    0xe3a0_60c8,
                    0xe256_6001,
                    0x1aff_fffd,
                    0xe594_3000,
                    0xe593_c028,
                    0xe1a0_0004,
                    0xe12f_ff3c,
                    0xe3a0_0055,
                    0xe585_0000,
                    0xe8bd_8070,
                ],
            ),
            (
                0xa0,
                vec![
                    0xe3a0_10c8,
                    0xe251_1001,
                    0x1aff_fffd,
                    0xe3a0_0055,
                    0xe12f_ff1e,
                ],
            ),
            // A thread cede uma vez; ao retomar grava uma marca no contexto.
            (
                0x20,
                vec![
                    0xe92d_4070,
                    0xe1a0_4000,
                    0xe1a0_5001,
                    0xe594_3000,
                    0xe593_c028,
                    0xe1a0_e00f,
                    0xe12f_ff1c,
                    0xe3a0_0055,
                    0xe585_0000,
                    0xe8bd_8070,
                ],
            ),
        ] {
            for (index, word) in words.into_iter().enumerate() {
                code[offset + index * 4..offset + index * 4 + 4]
                    .copy_from_slice(&word.to_le_bytes());
            }
        }
        let image = ModImage::parse(code).unwrap();
        let module = loader::load_with(&image, &[]).unwrap();
        let mut result = Self {
            machine: Machine::new_with_storage(
                BackendPadrao::new().unwrap(),
                module,
                &work,
                &storage,
                None,
            ),
            work,
        };
        assert_eq!(
            result.machine.run(1000).unwrap(),
            Outcome::Returned { code: 0 }
        );

        result
    }

    fn call(&mut self, interface: Interface, name: &str, args: [u32; 4]) -> u32 {
        let slot = (0..256)
            .find(|slot| interface.method(*slot) == Some(name))
            .unwrap();
        match self
            .machine
            .call_guest(aee::encode(interface, slot), args, 1000)
            .unwrap()
        {
            Outcome::Returned { code } => code,
            outcome => panic!("{name}: {outcome:?}"),
        }
    }

    fn create(&mut self, class: u32) -> u32 {
        assert_eq!(
            self.call(
                Interface::Shell,
                "CreateInstance",
                [loader::OBJECT_BASE, class, OUT, 0]
            ),
            0
        );
        let object = self.machine.cpu().read_u32(OUT).unwrap();
        assert_ne!(object, 0);
        object
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        assert!(
            self.work.canonicalize().unwrap().starts_with(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .canonicalize()
                    .unwrap()
                    .join("local-tests"),
            )
        );
        std::fs::remove_dir_all(&self.work).unwrap();
    }
}

#[test]
fn initialized_thread_resume_callback_works_with_settimerex_cancel_and_save() {
    for restore in [false, true] {
        let mut h = Harness::new();
        let thread = h.create(0x0100_1017);
        assert_eq!(
            h.call(
                Interface::Thread,
                "Start",
                [thread, 0x1000, loader::MODULE_BASE + 0x20, OUTPUT]
            ),
            0
        );
        h.machine.advance(10000).unwrap();
        assert_eq!(h.machine.cpu().read_u32(OUTPUT).unwrap(), 0);
        let cb = h.call(Interface::Thread, "GetResumeCBK", [thread, 0, 0, 0]);
        assert_ne!(cb, 0);
        assert_ne!(
            h.machine.cpu().read_u32(cb + 16).unwrap(),
            0,
            "GetResumeCBK deve devolver um callback inicializado, utilizável por SetTimerEx"
        );
        assert_eq!(
            h.call(
                Interface::Shell,
                "SetTimer",
                [loader::OBJECT_BASE, 0, cb, cb]
            ),
            0
        );
        h.call(
            Interface::Shell,
            "CancelTimer",
            [loader::OBJECT_BASE, cb, cb, 0],
        );
        h.machine.advance(10000).unwrap();
        assert_eq!(h.machine.cpu().read_u32(OUTPUT).unwrap(), 0);
        h.call(
            Interface::Shell,
            "SetTimer",
            [loader::OBJECT_BASE, 0, cb, cb],
        );
        if restore {
            let saved = h.machine.grava_estado();
            h.machine.restaura_estado(&saved).unwrap();
        }
        for _ in 0..4 {
            h.machine.advance(10000).unwrap();
        }
        assert_eq!(h.machine.cpu().read_u32(OUTPUT).unwrap(), 0x55);
    }
}

#[test]
fn thread_resume_callback_can_be_invoked_directly_and_keeps_thread_identity() {
    let mut h = Harness::new();
    let mut callbacks = Vec::new();
    for index in 0..2 {
        let thread = h.create(0x0100_1017);
        h.call(
            Interface::Thread,
            "Start",
            [
                thread,
                0x1000,
                loader::MODULE_BASE + 0x20,
                OUTPUT + index * 4,
            ],
        );
        h.machine.advance(10000).unwrap();
        let cb = h.call(Interface::Thread, "GetResumeCBK", [thread, 0, 0, 0]);
        assert_eq!(
            h.call(Interface::Thread, "GetResumeCBK", [thread, 0, 0, 0]),
            cb
        );
        callbacks.push(cb);
    }
    assert_ne!(callbacks[0], callbacks[1]);
    for index in 0..2 {
        let cb = callbacks[index];
        let function = h.machine.cpu().read_u32(cb + 16).unwrap();
        let data = h.machine.cpu().read_u32(cb + 20).unwrap();
        assert_ne!(function, 0);
        assert_eq!(
            h.machine
                .call_guest(function, [data, 0, 0, 0], 10000)
                .unwrap(),
            Outcome::Returned { code: 0 }
        );
        h.machine.advance(10000).unwrap();
        assert_eq!(
            h.machine.cpu().read_u32(OUTPUT + index as u32 * 4).unwrap(),
            0x55
        );
        if index == 0 {
            assert_eq!(h.machine.cpu().read_u32(OUTPUT + 4).unwrap(), 0);
        }
    }
}

#[test]
fn final_thread_release_cancels_resume_timer_and_makes_saved_notifier_harmless() {
    let mut h = Harness::new();
    let thread = h.create(0x0100_1017);
    let cb = h.call(Interface::Thread, "GetResumeCBK", [thread, 0, 0, 0]);
    let function = h.machine.cpu().read_u32(cb + 16).unwrap();
    let data = h.machine.cpu().read_u32(cb + 20).unwrap();
    h.call(
        Interface::Shell,
        "SetTimer",
        [loader::OBJECT_BASE, 1000, cb, cb],
    );
    assert_eq!(h.call(Interface::Thread, "Release", [thread, 0, 0, 0]), 0);
    assert_eq!(
        h.call(
            Interface::Shell,
            "GetTimerExpiration",
            [loader::OBJECT_BASE, cb, cb, 0]
        ),
        0,
        "a liberação não pode deixar um timer apontar para o callback desalocado"
    );
    assert_eq!(
        h.machine
            .call_guest(function, [data, 0, 0, 0], 1000)
            .unwrap(),
        Outcome::Returned { code: 0 }
    );
    for _ in 0..3 {
        h.machine.advance(1000).unwrap();
    }
    assert!(h.machine.swallowed_faults().is_empty());
}

#[test]
fn budgeted_thread_restores_outer_stack_and_keeps_owner_through_save_and_suspend() {
    for restore in [false, true] {
        let mut h = Harness::new();
        let root_sp = h.machine.cpu().read_reg(Reg::Sp);
        let thread = h.create(0x01001017);
        h.call(
            Interface::Thread,
            "Start",
            [thread, 0x1000, loader::MODULE_BASE + 0x60, OUTPUT],
        );
        assert_eq!(h.machine.advance(16).unwrap(), vec![Outcome::Budget]);
        if restore {
            let saved = h.machine.grava_estado();
            h.machine.restaura_estado(&saved).unwrap();
        }
        for _ in 0..128 {
            h.machine.advance(16).unwrap();
            if h.machine.handset_event_ready() {
                break;
            }
        }
        assert!(h.machine.handset_event_ready());
        assert_eq!(
            h.machine.cpu().read_reg(Reg::Sp),
            root_sp,
            "um callback posterior não pode usar a pilha de uma thread suspensa"
        );
        let cb = h.call(Interface::Thread, "GetResumeCBK", [thread, 0, 0, 0]);
        h.call(Interface::Shell, "Resume", [loader::OBJECT_BASE, cb, 0, 0]);
        h.machine.advance(10000).unwrap();
        assert_eq!(h.machine.cpu().read_u32(OUTPUT).unwrap(), 0x55);
        assert_eq!(h.machine.cpu().read_reg(Reg::Sp), root_sp);
    }
}

#[test]
fn budgeted_thread_return_completes_join_after_resume() {
    let mut h = Harness::new();
    let thread = h.create(0x01001017);
    h.call(
        Interface::Thread,
        "Start",
        [thread, 0x1000, loader::MODULE_BASE + 0xa0, OUTPUT],
    );
    h.machine.cpu_mut().write_u32(OUTPUT, 0xdeadbeef).unwrap();
    h.call(Interface::Thread, "Join", [thread, 0, OUTPUT, 0]);
    assert_eq!(h.machine.advance(16).unwrap(), vec![Outcome::Budget]);
    for _ in 0..128 {
        h.machine.advance(16).unwrap();
        if h.machine.handset_event_ready() {
            break;
        }
    }
    assert_eq!(h.machine.cpu().read_u32(OUTPUT).unwrap(), 0x55);
}
