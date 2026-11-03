use std::path::PathBuf;
use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend},
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    storage::StoragePaths,
};

const APPLET: u32 = loader::HEAP_BASE + 0x10000;
const VTABLE: u32 = APPLET + 0x100;
const PCB: u32 = loader::STACK_BASE + 0x200;
const CONTEXT: u32 = PCB + 64;
const FUNCTION: u32 = loader::MODULE_BASE + 0x200;

struct Fixture {
    m: Machine<BackendPadrao>,
    work: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let work = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests")
            .join(format!(
                "app-close-callback-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&work).unwrap();
        let storage = StoragePaths::from_root(work.join("runtime"));
        storage.create_dirs().unwrap();
        // HandleEvent registra evt+1 no objeto: START=1, STOP=2.
        let words = [
            0xe3a00000u32,
            0xe12fff1e,
            0xe2811001,
            0xe5801004,
            0xe3a00001,
            0xe12fff1e,
        ];
        let image =
            ModImage::parse(words.into_iter().flat_map(u32::to_le_bytes).collect()).unwrap();
        let mut loaded = loader::load_with(&image, &[]).unwrap();
        loaded.mem.write_u32(APPLET, VTABLE).unwrap();
        loaded
            .mem
            .write_u32(VTABLE + 8, loader::MODULE_BASE + 8)
            .unwrap();
        let mut m =
            Machine::new_with_storage(BackendPadrao::new().unwrap(), loaded, &work, &storage, None);
        assert_eq!(m.run(1000).unwrap(), Outcome::Returned { code: 0 });
        assert_eq!(
            m.start_applet(APPLET, 0x12345678, 1000).unwrap(),
            Outcome::Returned { code: 1 }
        );
        let mut f = Self { m, work };
        f.words(
            FUNCTION,
            &[
                0xe5901000, 0xe2811001, 0xe5801000, 0xe59f2018, 0xe5922004, 0xe5802008, 0xe59f2010,
                0xe5922008, 0xe580200c, 0xe3a00000, 0xe12fff1e, APPLET, PCB,
            ],
        );
        f.m.cpu_mut().write_mem(PCB, &[0; 28]).unwrap();
        f.m.cpu_mut().write_mem(CONTEXT, &[0; 16]).unwrap();
        f.m.cpu_mut().write_u32(PCB + 16, FUNCTION).unwrap();
        f.m.cpu_mut().write_u32(PCB + 20, CONTEXT).unwrap();
        f
    }

    fn words(&mut self, at: u32, words: &[u32]) {
        self.m
            .cpu_mut()
            .write_mem(
                at,
                &words
                    .iter()
                    .flat_map(|word| word.to_le_bytes())
                    .collect::<Vec<_>>(),
            )
            .unwrap();
    }

    fn api(&mut self, name: &str, args: [u32; 4]) -> u32 {
        let slot = (0..256)
            .find(|&s| Interface::Shell.method(s) == Some(name))
            .unwrap();
        match self
            .m
            .call_guest(aee::encode(Interface::Shell, slot), args, 10000)
            .unwrap()
        {
            Outcome::Returned { code } => code,
            other => panic!("{name}: {other:?}"),
        }
    }

    fn register(&mut self, event: u32) {
        self.api(
            "RegisterSystemCallback",
            [loader::OBJECT_BASE, PCB, event, 0],
        );
    }

    fn read(&self, at: u32) -> u32 {
        self.m.cpu().read_u32(at).unwrap()
    }

    fn cancel(&mut self) {
        let cancel = self.read(PCB + 8);
        assert_ne!(cancel, 0);
        assert!(matches!(
            self.m.call_guest(cancel, [PCB, 0, 0, 0], 1000).unwrap(),
            Outcome::Returned { .. }
        ));
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.work);
    }
}

#[test]
fn app_close_callback_is_deferred_until_closing_and_runs_after_stop_once() {
    let mut f = Fixture::new();
    f.register(4);
    assert_ne!(f.read(PCB + 8), 0);
    assert_eq!(f.read(CONTEXT), 0, "registrar não fecha o applet");
    f.api("CloseApplet", [loader::OBJECT_BASE, 0, 0, 0]);
    assert!(f.m.pediu_para_fechar());
    assert_eq!(
        f.read(CONTEXT),
        0,
        "CloseApplet apenas solicita o fechamento"
    );
    assert_eq!(f.read(APPLET + 4), 1);
    f.m.encerra_applet().unwrap();
    assert_eq!(f.read(CONTEXT), 1);
    assert_eq!(
        f.read(CONTEXT + 8),
        2,
        "limpeza do applet terminou antes do callback"
    );
    assert_eq!(
        f.read(CONTEXT + 12),
        0,
        "identidade retirada antes da chamada"
    );
    assert_eq!(f.read(PCB + 8), 0);
    assert_eq!(f.read(PCB + 12), 0);
    f.m.encerra_applet().unwrap();
    assert_eq!(f.read(CONTEXT), 1, "registro one-shot já consumido");
}

#[test]
fn cancelled_and_replaced_app_close_callbacks_do_not_run() {
    for replace in [false, true] {
        let mut f = Fixture::new();
        f.register(4);
        if replace {
            f.register(3);
        } else {
            f.cancel();
        }
        f.m.encerra_applet().unwrap();
        assert_eq!(f.read(CONTEXT), 0);
    }
}

#[test]
fn app_close_callback_can_reregister_without_recursive_delivery() {
    let mut f = Fixture::new();
    let slot = (0..256)
        .find(|&s| Interface::Shell.method(s) == Some("RegisterSystemCallback"))
        .unwrap();
    // Incrementa o contexto e rearma o mesmo AEECallback durante a entrega one-shot.
    f.words(
        FUNCTION,
        &[
            0xe92d4010,
            0xe1a04000,
            0xe5941000,
            0xe2811001,
            0xe5841000,
            0xe59f0014,
            0xe59f1014,
            0xe3a02004,
            0xe59fc010,
            0xe12fff3c,
            0xe3a00000,
            0xe8bd8010,
            loader::OBJECT_BASE,
            PCB,
            aee::encode(Interface::Shell, slot),
        ],
    );
    f.register(4);
    f.m.encerra_applet().unwrap();
    assert_eq!(f.read(CONTEXT), 1);
    assert_ne!(
        f.read(PCB + 8),
        0,
        "novo registro fica pendente e cancelável"
    );
    f.m.advance(1000).unwrap();
    assert_eq!(f.read(CONTEXT), 1);
    f.cancel();
    f.m.encerra_applet().unwrap();
    assert_eq!(f.read(CONTEXT), 1);
}

#[test]
fn app_close_registration_survives_restore_and_absent_old_section_stays_empty() {
    let mut f = Fixture::new();
    f.register(4);
    let saved = f.m.grava_estado();
    f.m.encerra_applet().unwrap();
    assert_eq!(f.read(CONTEXT), 1);
    let mut restored = Fixture::new();
    restored.m.restaura_estado(&saved).unwrap();
    restored.m.encerra_applet().unwrap();
    assert_eq!(restored.read(CONTEXT), 1);
    let reader = zeebx::save_state::Leitor::abre(&saved).unwrap();
    let old = zeebx::save_state::escreve(
        &reader
            .nomes()
            .into_iter()
            .filter(|name| *name != "agenda.systemcallbacks")
            .map(|name| (name.to_owned(), reader.secao(name).unwrap().to_vec()))
            .collect::<Vec<_>>(),
    );
    restored.m.restaura_estado(&old).unwrap();
    restored.m.encerra_applet().unwrap();
    assert_eq!(restored.read(CONTEXT), 0);
}

#[test]
fn app_close_does_not_overwrite_a_pending_guest_continuation() {
    let mut f = Fixture::new();
    f.register(4);
    let busy = FUNCTION + 0x100;
    f.words(busy, &[0xeafffffe]);
    assert_eq!(f.m.call_guest(busy, [0; 4], 8).unwrap(), Outcome::Budget);
    assert!(!f.m.handset_event_ready());
    assert!(f.m.encerra_applet().is_err());
    assert!(!f.m.handset_event_ready());
    assert_eq!(f.read(CONTEXT), 0);
    assert_ne!(f.read(PCB + 8), 0);
    assert_eq!(f.read(APPLET + 4), 1);
}

#[test]
fn failed_stop_handler_does_not_deliver_app_close_cleanup() {
    let mut f = Fixture::new();
    f.register(4);
    let failing_stop = FUNCTION + 0x100;
    // Falha real de memória no STOP; não substituir a execução interrompida por limpeza.
    f.words(failing_stop, &[0xe3a00000, 0xe5801000, 0xe12fff1e]);
    f.m.cpu_mut().write_u32(VTABLE + 8, failing_stop).unwrap();
    assert!(f.m.encerra_applet().is_err());
    assert_eq!(f.read(CONTEXT), 0);
    assert_ne!(f.read(PCB + 8), 0);
}

#[test]
fn stop_receives_a_writable_boolean_initialized_to_true_and_releases_it_after_return() {
    let mut f = Fixture::new();
    let stop = FUNCTION + 0x100;
    // Só STOP lê o parâmetro boolean*: guarda seu endereço e o valor inicial.
    f.words(stop, &[0xe5803008, 0xe5d31000, 0xe580100c, 0xe3a00001, 0xe12fff1e]);
    f.m.cpu_mut().write_u32(VTABLE + 8, stop).unwrap();
    let used = f.m.heap_used();
    f.api("CloseApplet", [loader::OBJECT_BASE, 0, 0, 0]);
    assert!(f.m.encerra_applet().is_ok(), "dwParam de STOP deve apontar para boolean válido");
    assert_ne!(f.read(APPLET + 8), 0);
    assert_eq!(f.read(APPLET + 12), 1, "não alterar o valor significa aceitar o fechamento");
    assert_eq!(f.m.heap_used(), used, "parâmetro temporário é devolvido depois do retorno");
}

#[test]
fn stop_can_choose_background_without_delivering_app_closed_or_repeating_close_intent() {
    let mut f = Fixture::new();
    f.register(4);
    let stop = FUNCTION + 0x100;
    // O SDK permite escrever FALSE em *dwParam para continuar no background.
    f.words(stop, &[0xe3a01000, 0xe5c31000, 0xe3a00001, 0xe12fff1e]);
    f.m.cpu_mut().write_u32(VTABLE + 8, stop).unwrap();
    let used = f.m.heap_used();
    f.api("CloseApplet", [loader::OBJECT_BASE, 0, 0, 0]);
    assert!(f.m.encerra_applet().is_ok());
    assert!(!f.m.pediu_para_fechar(), "pedido tratado não pode repetir STOP no próximo poll");
    assert_eq!(f.read(CONTEXT), 0, "background não é APP_CLOSED");
    assert_ne!(f.read(PCB + 8), 0, "limpeza continua registrada para o fechamento real");
    assert_eq!(f.m.heap_used(), used);
}

#[test]
fn unfinished_stop_keeps_its_boolean_live_and_does_not_deliver_app_closed() {
    let mut f = Fixture::new();
    f.register(4);
    let stop = FUNCTION + 0x100;
    f.words(stop, &[0xe5803008, 0xeafffffe]);
    f.m.cpu_mut().write_u32(VTABLE + 8, stop).unwrap();
    let used = f.m.heap_used();
    assert!(f.m.encerra_applet().is_err(), "STOP sem retorno não confirma fechamento");
    let parameter = f.read(APPLET + 8);
    assert_ne!(parameter, 0);
    assert_eq!(f.m.cpu().read_u32(parameter).unwrap() & 0xff, 1);
    assert!(f.m.heap_used() > used, "não reutilizar a memória de uma chamada sem retorno");
    assert_eq!(f.read(CONTEXT), 0);
    assert_ne!(f.read(PCB + 8), 0);
}
