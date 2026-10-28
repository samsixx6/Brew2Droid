use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend, Reg};
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

const SHELL: u32 = loader::OBJECT_BASE;
const BUFFER: u32 = loader::STACK_BASE + 0x100;

struct Fixture {
    machine: Machine<BackendPadrao>,
    work: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        Self::with_overlay(false)
    }
    fn with_overlay(overlay: bool) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let work = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests")
            .join(format!(
                "reported-contract-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&work).unwrap();
        let storage = StoragePaths::from_root(work.join("runtime"));
        storage.create_dirs().unwrap();
        let image = ModImage::parse(
            [0xe3a0_0000u32, 0xe12f_ff1e]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect(),
        )
        .unwrap();
        let content = if overlay {
            work.join("content")
        } else {
            work.clone()
        };
        std::fs::create_dir_all(&content).unwrap();
        let mut machine = Machine::new_with_storage(
            BackendPadrao::new().unwrap(),
            loader::load_with(&image, &[]).unwrap(),
            &content,
            &storage,
            overlay.then(|| work.join("overlay")),
        );
        assert_eq!(machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
        Self { machine, work }
    }

    fn call(&mut self, interface: Interface, name: &str, arguments: [u32; 4]) -> u32 {
        let slot = (0..256)
            .find(|slot| interface.method(*slot) == Some(name))
            .unwrap();
        match self
            .machine
            .call_guest(aee::encode(interface, slot), arguments, 1000)
            .unwrap()
        {
            Outcome::Returned { code } => code,
            outcome => panic!("{name}: {outcome:?}"),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let private_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("local-tests")
            .canonicalize()
            .unwrap();
        let actual = self.work.canonicalize().unwrap();
        assert!(actual.starts_with(private_root));
        std::fs::remove_dir_all(actual).unwrap();
    }
}

impl Fixture {
    fn object(&mut self, class: u32) -> u32 {
        assert_eq!(
            self.call(
                Interface::Shell,
                "CreateInstance",
                [SHELL, class, BUFFER, 0]
            ),
            0
        );
        self.machine.cpu().read_u32(BUFFER).unwrap()
    }
    fn source(&mut self, bytes: &[u8]) -> (u32, u32) {
        let util = self.object(0x01001011);
        self.machine
            .cpu_mut()
            .write_mem(BUFFER + 256, bytes)
            .unwrap();
        let sp = self.machine.cpu().read_reg(Reg::Sp);
        self.machine.cpu_mut().write_reg(Reg::Sp, sp - 32);
        self.machine.cpu_mut().write_u32(sp - 32, 0).unwrap();
        self.machine
            .cpu_mut()
            .write_u32(sp - 28, BUFFER + 128)
            .unwrap();
        assert_eq!(
            self.call(
                Interface::SourceUtil,
                "SourceFromMemory",
                [util, BUFFER + 256, bytes.len() as u32, 0]
            ),
            0
        );
        self.machine.cpu_mut().write_reg(Reg::Sp, sp);
        (util, self.machine.cpu().read_u32(BUFFER + 128).unwrap())
    }
    fn adapter(&mut self, util: u32, source: u32) -> u32 {
        assert_eq!(
            self.call(
                Interface::SourceUtil,
                "AStreamFromSource",
                [util, source, BUFFER + 128, 0]
            ),
            0
        );
        let stream = self.machine.cpu().read_u32(BUFFER + 128).unwrap();
        assert_ne!(
            stream, source,
            "ISource tem QueryInterface no slot de Readable"
        );
        stream
    }
    fn read(&mut self, stream: u32, expected: &[u8]) {
        let vtable = self.machine.cpu().read_u32(stream).unwrap();
        let read = self.machine.cpu().read_u32(vtable + 12).unwrap();
        assert_eq!(
            self.machine
                .call_guest(read, [stream, BUFFER + 512, expected.len() as u32, 0], 1000)
                .unwrap(),
            Outcome::Returned {
                code: expected.len() as u32
            }
        );
        let mut bytes = vec![0; expected.len()];
        self.machine
            .cpu()
            .read_mem(BUFFER + 512, &mut bytes)
            .unwrap();
        assert_eq!(bytes, expected);
    }
    fn readable(&mut self, stream: u32) -> (u32, u32) {
        let function = loader::MODULE_BASE + 0x40;
        let context = BUFFER + 768;
        self.machine
            .cpu_mut()
            .write_mem(
                function,
                &[0xe5901000u32, 0xe2811001, 0xe5801000, 0xe12fff1e]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        self.call(
            Interface::MemAStream,
            "Readable",
            [stream, function, context, 0],
        );
        assert_eq!(self.machine.cpu().read_u32(context).unwrap(), 0);
        (function, context)
    }
}

#[test]
fn adapter_uses_iastream_vtable_and_shares_source_cursor() {
    let mut f = Fixture::new();
    let (util, source) = f.source(b"abcdef");
    assert_eq!(
        f.call(Interface::Source, "Read", [source, BUFFER + 512, 1, 0]),
        1
    );
    let a = f.adapter(util, source);
    let b = f.adapter(util, source);
    f.read(a, b"bc");
    f.read(b, b"de");
    assert_eq!(
        f.call(Interface::Source, "Read", [source, BUFFER + 512, 2, 0]),
        1
    );
    assert_eq!(
        f.machine.cpu().read_u32(BUFFER + 512).unwrap() & 255,
        b'f' as u32
    );
    assert_eq!(
        f.call(Interface::MemAStream, "Read", [a, BUFFER + 512, 1, 0]),
        0
    );
    assert_eq!(f.call(Interface::MemAStream, "Read", [a, 0, 0, 0]), 0);
}

#[test]
fn adapter_retains_source_until_its_last_release() {
    let mut f = Fixture::new();
    let (util, source) = f.source(b"retained");
    let a = f.adapter(util, source);
    assert_eq!(f.call(Interface::Source, "Release", [source, 0, 0, 0]), 1);
    f.read(a, b"retained");
    assert_eq!(f.call(Interface::MemAStream, "Release", [a, 0, 0, 0]), 0);
    assert_eq!(
        f.call(Interface::Source, "Read", [source, BUFFER + 512, 1, 0]),
        2
    );
}

#[test]
fn adapter_rejects_invalid_arguments_and_clears_output() {
    let mut f = Fixture::new();
    let (util, source) = f.source(b"x");
    assert_eq!(
        f.call(
            Interface::SourceUtil,
            "AStreamFromSource",
            [util, source, 0, 0]
        ),
        2
    );
    for invalid in [0, util, 0x12345678] {
        f.machine
            .cpu_mut()
            .write_u32(BUFFER + 128, 0xdeadbeef)
            .unwrap();
        assert_eq!(
            f.call(
                Interface::SourceUtil,
                "AStreamFromSource",
                [util, invalid, BUFFER + 128, 0]
            ),
            2
        );
        assert_eq!(f.machine.cpu().read_u32(BUFFER + 128).unwrap(), 0);
    }
    assert_eq!(f.call(Interface::Source, "Release", [source, 0, 0, 0]), 0);
}

#[test]
fn adapter_feeds_real_gzip_to_unzip_and_consumes_source() {
    let mut f = Fixture::new();
    let (util, source) = f.source(&[
        31, 139, 8, 0, 0, 0, 0, 0, 2, 10, 203, 77, 205, 43, 45, 86, 72, 204, 75, 81, 72, 79, 204,
        77, 85, 72, 44, 46, 78, 45, 41, 230, 2, 0, 252, 155, 111, 54, 22, 0, 0, 0,
    ]);
    let a = f.adapter(util, source);
    let unzip = f.object(0x01001014);
    f.call(Interface::UnzipStream, "SetStream", [unzip, a, 0, 0]);
    assert_eq!(
        f.call(
            Interface::UnzipStream,
            "Read",
            [unzip, BUFFER + 512, 100, 0]
        ),
        22
    );
    let mut decoded = [0; 22];
    f.machine
        .cpu()
        .read_mem(BUFFER + 512, &mut decoded)
        .unwrap();
    assert_eq!(&decoded, b"menus and game assets\n");
    assert_eq!(
        f.call(Interface::MemAStream, "Read", [a, BUFFER + 512, 1, 0]),
        0
    );
}

#[test]
fn adapter_readable_is_deferred_cancelable_and_one_shot() {
    let mut f = Fixture::new();
    let (util, source) = f.source(b"x");
    let a = f.adapter(util, source);
    let (function, context) = f.readable(a);
    f.call(
        Interface::MemAStream,
        "Cancel",
        [a, function, context + 4, 0],
    );
    f.machine.deliver_signals(1000).unwrap();
    assert_eq!(f.machine.cpu().read_u32(context).unwrap(), 1);
    f.call(Interface::MemAStream, "Readable", [a, function, context, 0]);
    f.call(Interface::MemAStream, "Cancel", [a, function, context, 0]);
    f.machine.deliver_signals(1000).unwrap();
    assert_eq!(f.machine.cpu().read_u32(context).unwrap(), 1);
    f.call(Interface::MemAStream, "Readable", [a, function, context, 0]);
    f.call(Interface::MemAStream, "Readable", [a, function, context, 0]);
    f.machine.deliver_signals(1000).unwrap();
    f.machine.deliver_signals(1000).unwrap();
    assert_eq!(f.machine.cpu().read_u32(context).unwrap(), 2);
}

#[test]
fn releasing_adapter_cancels_pending_readable() {
    let mut f = Fixture::new();
    let (util, source) = f.source(b"x");
    let a = f.adapter(util, source);
    let (_, context) = f.readable(a);
    f.call(Interface::MemAStream, "Release", [a, 0, 0, 0]);
    f.machine.deliver_signals(1000).unwrap();
    assert_eq!(f.machine.cpu().read_u32(context).unwrap(), 0);
}

#[test]
fn adapter_cursor_reference_and_readable_survive_save_restore() {
    let mut f = Fixture::new();
    let (util, source) = f.source(b"abcdef");
    let a = f.adapter(util, source);
    f.read(a, b"ab");
    f.call(Interface::Source, "Release", [source, 0, 0, 0]);
    let (_, context) = f.readable(a);
    let state = f.machine.grava_estado();
    let mut restored = Fixture::new();
    restored.machine.restaura_estado(&state).unwrap();
    restored.read(a, b"cdef");
    restored.machine.deliver_signals(1000).unwrap();
    assert_eq!(restored.machine.cpu().read_u32(context).unwrap(), 1);
    assert_eq!(
        restored.call(Interface::MemAStream, "Release", [a, 0, 0, 0]),
        0
    );
    assert_eq!(
        restored.call(Interface::Source, "Read", [source, BUFFER + 512, 1, 0]),
        2
    );
}

#[test]
fn replacing_adapter_with_memory_cancels_callback_and_releases_source() {
    let mut f = Fixture::new();
    let (util, source) = f.source(b"abc");
    let a = f.adapter(util, source);
    f.call(Interface::Source, "Release", [source, 0, 0, 0]);
    let (_, context) = f.readable(a);
    f.machine.cpu_mut().write_mem(BUFFER + 512, b"xyz").unwrap();
    f.call(Interface::MemAStream, "SetEx", [a, BUFFER + 512, 3, 0]);
    f.machine.deliver_signals(1000).unwrap();
    assert_eq!(f.machine.cpu().read_u32(context).unwrap(), 0);
    f.read(a, b"xyz");
    assert_eq!(
        f.call(Interface::Source, "Read", [source, BUFFER + 512, 1, 0]),
        2
    );
}

#[test]
fn adapter_rejects_corrupt_saved_relationships() {
    let mut f = Fixture::new();
    let (util, source) = f.source(b"abc");
    let a = f.adapter(util, source);
    let bytes = f.machine.grava_estado();
    let state = zeebx::save_state::Leitor::abre(&bytes).unwrap();
    let row = state.u32s("tab.source_streams").unwrap();
    for broken in [
        vec![a, 0, row[2]],
        vec![a, source, 0],
        vec![util, source, row[2]],
        vec![a, source, row[2], a, source, row[2]],
    ] {
        let mut sections = zeebx::save_state::Secoes::nova();
        for name in state.nomes() {
            if name == "tab.source_streams" {
                sections.poe_u32s(name, broken.iter().copied());
            } else {
                sections.poe(name, state.secao(name).unwrap().to_vec());
            }
        }
        let mut restored = Fixture::new();
        assert!(restored.machine.restaura_estado(&sections.fecha()).is_err());
    }
}

#[test]
fn loading_older_state_clears_new_adapter_state() {
    let mut f = Fixture::new();
    let bytes = f.machine.grava_estado();
    let state = zeebx::save_state::Leitor::abre(&bytes).unwrap();
    let mut older = zeebx::save_state::Secoes::nova();
    for name in state
        .nomes()
        .into_iter()
        .filter(|n| *n != "tab.source_streams")
    {
        older.poe(name, state.secao(name).unwrap().to_vec());
    }
    let (util, source) = f.source(b"abc");
    f.adapter(util, source);
    f.machine.restaura_estado(&older.fecha()).unwrap();
    let bytes = f.machine.grava_estado();
    let state = zeebx::save_state::Leitor::abre(&bytes).unwrap();
    assert!(state.u32s("tab.source_streams").unwrap().is_empty());
}

#[test]
fn adapter_cancel_null_pair_cancels_all_readable_callbacks() {
    let mut f = Fixture::new();
    let (util, source) = f.source(b"x");
    let a = f.adapter(util, source);
    let (_, context) = f.readable(a);
    f.call(Interface::MemAStream, "Cancel", [a, 0, 0, 0]);
    f.machine.deliver_signals(1000).unwrap();
    assert_eq!(f.machine.cpu().read_u32(context).unwrap(), 0);
}

#[test]
fn peek_adapter_shares_cursor_and_rejects_corrupt_saved_position() {
    let mut f = Fixture::new();
    let (util, source) = f.source(b"abcdef");
    assert_eq!(
        f.call(
            Interface::SourceUtil,
            "PeekFromSource",
            [util, source, 100, BUFFER + 128]
        ),
        0
    );
    let peek = f.machine.cpu().read_u32(BUFFER + 128).unwrap();
    f.call(Interface::Peek, "Advance", [peek, 2, 0, 0]);
    let a = f.adapter(util, peek);
    f.read(a, b"cd");
    assert_eq!(
        f.call(Interface::Peek, "Read", [peek, BUFFER + 512, 1, 0]),
        1
    );
    f.read(a, b"f");
    let bytes = f.machine.grava_estado();
    let state = zeebx::save_state::Leitor::abre(&bytes).unwrap();
    let section = format!("peek.{peek}.meta");
    let mut meta = state.u32s(&section).unwrap();
    meta[0] = 1000;
    let mut broken = zeebx::save_state::Secoes::nova();
    for name in state.nomes() {
        if name == section {
            broken.poe_u32s(name, meta.iter().copied());
        } else {
            broken.poe(name, state.secao(name).unwrap().to_vec());
        }
    }
    let mut restored = Fixture::new();
    assert!(restored.machine.restaura_estado(&broken.fecha()).is_err());
}

#[test]
fn adapter_rejects_saved_callback_with_wrong_owner() {
    let mut f = Fixture::new();
    let (util, source) = f.source(b"abc");
    let a = f.adapter(util, source);
    f.readable(a);
    let bytes = f.machine.grava_estado();
    let state = zeebx::save_state::Leitor::abre(&bytes).unwrap();
    let mut agenda = state.u32s("agenda.callbacks").unwrap();
    assert_eq!(agenda.len(), 2);
    agenda[1] = 0;
    let mut broken = zeebx::save_state::Secoes::nova();
    for name in state.nomes() {
        if name == "agenda.callbacks" {
            broken.poe_u32s(name, agenda.iter().copied());
        } else {
            broken.poe(name, state.secao(name).unwrap().to_vec());
        }
    }
    let mut restored = Fixture::new();
    assert!(restored.machine.restaura_estado(&broken.fecha()).is_err());
}
