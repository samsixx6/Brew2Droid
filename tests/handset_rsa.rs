use std::path::PathBuf;

use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend, Reg};
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

const OUT: u32 = loader::STACK_BASE + 0x100;
const INPUT: u32 = loader::STACK_BASE + 0x1000;
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
                "rsa-service-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            ));
        std::fs::create_dir_all(&work).unwrap();
        let storage = StoragePaths::from_root(work.join("runtime"));
        storage.create_dirs().unwrap();
        let mut code = vec![0u8; 0x100];
        for (offset, words) in [
            (0, vec![0xe3a0_0000u32, 0xe12f_ff1e]),
            (0x20, vec![0xe3a0_1001, 0xe580_1000, 0xe12f_ff1e]),
            // O callback volta a enfileirar ModExp no mesmo objeto.
            (
                0x80,
                vec![
                    0xe92d_4010,
                    0xe1a0_4000,
                    0xe594_1000,
                    0xe281_1001,
                    0xe584_1000,
                    0xe24d_d010,
                    0xe594_1018,
                    0xe58d_1000,
                    0xe594_101c,
                    0xe58d_1004,
                    0xe594_1020,
                    0xe58d_1008,
                    0xe594_c004,
                    0xe594_0008,
                    0xe594_100c,
                    0xe594_2010,
                    0xe594_3014,
                    0xe12f_ff3c,
                    0xe28d_d010,
                    0xe8bd_8010,
                ],
            ),
            (
                0x40,
                vec![
                    0xe3a0_10c8,
                    0xe251_1001,
                    0x1aff_fffd,
                    0xe3a0_1001,
                    0xe580_1000,
                    0xe12f_ff1e,
                ],
            ),
        ] {
            for (i, word) in words.into_iter().enumerate() {
                code[offset + i * 4..offset + i * 4 + 4].copy_from_slice(&word.to_le_bytes());
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

include!("fixtures/handset_rsa_vectors.rs");
const MOD: u32 = INPUT;
const EXP: u32 = INPUT + 0x400;
const SIG: u32 = INPUT + 0x800;
const DIGEST: u32 = INPUT + 0xc00;
const RESULT: u32 = INPUT + 0x1000;
const CB: u32 = INPUT + 0x1040;
const MARKER: u32 = INPUT + 0x1080;
const PTR: u32 = INPUT + 0x10c0;
const LENGTH: u32 = INPUT + 0x10c4;

impl Harness {
    fn rsa(&mut self, object: u32, slot: u32, args: &[u32]) -> u32 {
        let table = self.machine.cpu().read_u32(object).unwrap();
        let function = self.machine.cpu().read_u32(table + slot * 4).unwrap();
        let original_sp = self.machine.cpu().read_reg(Reg::Sp);
        let sp = original_sp - 64;
        self.machine.cpu_mut().write_reg(Reg::Sp, sp);
        let mut regs = [object, 0, 0, 0];
        for (index, arg) in args.iter().enumerate() {
            if index < 3 {
                regs[index + 1] = *arg;
            } else {
                self.machine
                    .cpu_mut()
                    .write_u32(sp + (index as u32 - 3) * 4, *arg)
                    .unwrap();
            }
        }
        let out = self.machine.call_guest(function, regs, 10000).unwrap();
        self.machine.cpu_mut().write_reg(Reg::Sp, original_sp);
        match out {
            Outcome::Returned { code } => code,
            other => panic!("RSA {slot}: {other:?}"),
        }
    }
    fn key(&mut self, object: u32, modulus: &[u8], exponent: &[u8]) -> u32 {
        self.machine.cpu_mut().write_mem(MOD, modulus).unwrap();
        self.machine.cpu_mut().write_mem(EXP, exponent).unwrap();
        self.rsa(
            object,
            2,
            &[MOD, modulus.len() as u32, EXP, exponent.len() as u32],
        )
    }
    fn verify(&mut self, object: u32, signature: &[u8], digest: &[u8], kind: u32) {
        self.machine.cpu_mut().write_mem(SIG, signature).unwrap();
        self.machine.cpu_mut().write_mem(DIGEST, digest).unwrap();
        self.machine
            .cpu_mut()
            .write_u32(RESULT, 0xdeadbeef)
            .unwrap();
        self.machine.cpu_mut().write_u32(MARKER, 0).unwrap();
        self.machine.cpu_mut().write_mem(CB, &[0; 28]).unwrap();
        self.machine
            .cpu_mut()
            .write_u32(CB + 16, loader::MODULE_BASE + 0x20)
            .unwrap();
        self.machine.cpu_mut().write_u32(CB + 20, MARKER).unwrap();
        self.rsa(
            object,
            7,
            &[
                SIG,
                signature.len() as u32,
                DIGEST,
                digest.len() as u32,
                kind,
                RESULT,
                CB,
            ],
        );
        assert_eq!(
            self.machine.cpu().read_u32(RESULT).unwrap(),
            0xdeadbeef,
            "a operação deve completar no laço de eventos, depois de quem pediu retornar"
        );
        assert_eq!(self.machine.cpu().read_u32(MARKER).unwrap(), 0);
    }
}

#[test]
fn rsa_raw_modexp_has_exact_big_endian_result_and_internal_buffer() {
    let mut h = Harness::new();
    let rsa = h.create(0x0100101a);
    assert_eq!(h.key(rsa, &[0x0c, 0xa1], &[0x11]), 0);
    h.machine.cpu_mut().write_mem(SIG, &[0, 65]).unwrap();
    assert_eq!(h.rsa(rsa, 3, &[SIG, 2, PTR, LENGTH, 0]), 0);
    assert_eq!(h.machine.cpu().read_u32(LENGTH).unwrap(), 2);
    let p = h.machine.cpu().read_u32(PTR).unwrap();
    assert_ne!(p, 0);
    let mut out = [0; 2];
    h.machine.cpu().read_mem(p, &mut out).unwrap();
    assert_eq!(out, [0x0a, 0xe6]);
    assert_eq!(h.key(rsa, N, E), 0);
    h.machine.cpu_mut().write_mem(SIG, SIG_SHA1).unwrap();
    assert_eq!(h.rsa(rsa, 3, &[SIG, 128, PTR, LENGTH, 0]), 0);
    let pointer = h.machine.cpu().read_u32(PTR).unwrap();
    let saved = h.machine.grava_estado();
    h.machine.restaura_estado(&saved).unwrap();
    let mut encoded = vec![0; 128];
    h.machine.cpu().read_mem(pointer, &mut encoded).unwrap();
    assert_eq!(encoded, ENCODED_SHA1);
}

#[test]
fn rsa_sha1_and_md5_verify_real_signatures_deferred_and_preserve_keys_in_save() {
    let mut h = Harness::new();
    let rsa = h.create(0x0100101a);
    assert_eq!(h.key(rsa, N, E), 0);
    // Init copia a chave; os buffers do chamador não são propriedade do objeto.
    h.machine.cpu_mut().write_mem(MOD, &[0; 128]).unwrap();
    for (signature, digest, kind) in [(SIG_SHA1, SHA1, 3), (SIG_MD5, MD5, 2)] {
        h.verify(rsa, signature, digest, kind);
        let state = h.machine.grava_estado();
        h.machine.restaura_estado(&state).unwrap();
        assert!(!h.machine.is_idle());
        assert_eq!(
            h.machine.next_guest_work_delay(),
            Some(std::time::Duration::ZERO)
        );
        h.machine.deliver_signals(10000).unwrap();
        assert_eq!(h.machine.cpu().read_u32(RESULT).unwrap(), 0);
        assert_eq!(h.machine.cpu().read_u32(MARKER).unwrap(), 1);
        h.machine.cpu_mut().write_u32(MARKER, 0).unwrap();
        h.machine.deliver_signals(10000).unwrap();
        assert_eq!(h.machine.cpu().read_u32(MARKER).unwrap(), 0);
    }
}

#[test]
fn rsa_rejects_corrupt_signatures_digests_padding_and_algorithm_mismatch() {
    let mut h = Harness::new();
    let rsa = h.create(0x0100101a);
    assert_eq!(h.key(rsa, N, E), 0);
    let mut bad_sig = SIG_SHA1.to_vec();
    bad_sig[17] ^= 1;
    let mut bad_digest = SHA1.to_vec();
    bad_digest[0] ^= 1;
    for (sig, digest, kind) in [
        (bad_sig.as_slice(), SHA1, 3),
        (SIG_SHA1, bad_digest.as_slice(), 3),
        (SIG_BAD_PADDING, SHA1, 3),
        (SIG_MD5, SHA1, 3),
        (SIG_SHA1, SHA1, 8),
    ] {
        h.verify(rsa, sig, digest, kind);
        h.machine.deliver_signals(10000).unwrap();
        assert_ne!(h.machine.cpu().read_u32(RESULT).unwrap(), 0);
        assert_eq!(h.machine.cpu().read_u32(MARKER).unwrap(), 1);
    }
}

#[test]
fn rsa_cancel_and_release_prevent_late_result_and_notification() {
    for release in [false, true] {
        let mut h = Harness::new();
        let rsa = h.create(0x0100101a);
        assert_eq!(h.key(rsa, N, E), 0);
        h.verify(rsa, SIG_SHA1, SHA1, 3);
        if release {
            h.rsa(rsa, 1, &[]);
        } else {
            let cancel = h.machine.cpu().read_u32(CB + 8).unwrap();
            assert_ne!(cancel, 0);
            assert_eq!(
                h.machine.call_guest(cancel, [CB, 0, 0, 0], 1000).unwrap(),
                Outcome::Returned { code: 0 }
            );
        }
        h.machine.deliver_signals(10000).unwrap();
        assert_eq!(h.machine.cpu().read_u32(RESULT).unwrap(), 0xdeadbeef);
        assert_eq!(h.machine.cpu().read_u32(MARKER).unwrap(), 0);
    }
}

#[test]
fn rsa_invalid_key_and_input_size_leave_output_guards_untouched() {
    let mut h = Harness::new();
    let rsa = h.create(0x0100101a);
    assert_eq!(h.rsa(rsa, 2, &[0, 0, 0, 0]), 0x603);
    assert_eq!(h.key(rsa, &[0x0c, 0xa1], &[0x11]), 0);
    h.machine.cpu_mut().write_u32(PTR, 0x12345678).unwrap();
    h.machine.cpu_mut().write_u32(LENGTH, 0x87654321).unwrap();
    h.machine.cpu_mut().write_mem(SIG, &[0x0c, 0xa1]).unwrap();
    for len in [1, 2, 0xffffffff] {
        assert_eq!(h.rsa(rsa, 3, &[SIG, len, PTR, LENGTH, 0]), 0x609);
        assert_eq!(h.machine.cpu().read_u32(PTR).unwrap(), 0x12345678);
        assert_eq!(h.machine.cpu().read_u32(LENGTH).unwrap(), 0x87654321);
    }
}

#[test]
fn rsa_modexp_async_writes_owned_output_only_on_completion() {
    let mut h = Harness::new();
    let rsa = h.create(0x0100101a);
    assert_eq!(h.key(rsa, &[0x0c, 0xa1], &[17]), 0);
    h.machine.cpu_mut().write_mem(SIG, &[0, 65]).unwrap();
    h.machine.cpu_mut().write_mem(CB, &[0; 28]).unwrap();
    h.machine
        .cpu_mut()
        .write_u32(CB + 16, loader::MODULE_BASE + 0x20)
        .unwrap();
    h.machine.cpu_mut().write_u32(CB + 20, MARKER).unwrap();
    h.machine.cpu_mut().write_u32(RESULT, 0xdeadbeef).unwrap();
    h.rsa(rsa, 4, &[SIG, 2, PTR, LENGTH, RESULT, CB]);
    assert_eq!(h.machine.cpu().read_u32(RESULT).unwrap(), 0xdeadbeef);
    h.machine.deliver_signals(10000).unwrap();
    assert_eq!(h.machine.cpu().read_u32(RESULT).unwrap(), 0);
    let pointer = h.machine.cpu().read_u32(PTR).unwrap();
    let mut data = [0; 2];
    h.machine.cpu().read_mem(pointer, &mut data).unwrap();
    assert_eq!(data, [0x0a, 0xe6]);
}

#[test]
fn rsa_callback_budget_and_reused_callback_keep_one_completion() {
    let mut h = Harness::new();
    let a = h.create(0x0100101a);
    let b = h.create(0x0100101a);
    assert_eq!(h.key(a, N, E), 0);
    assert_eq!(h.key(b, N, E), 0);
    h.verify(a, SIG_SHA1, SHA1, 3);
    h.verify(b, SIG_MD5, MD5, 2);
    h.machine.deliver_signals(10000).unwrap();
    assert_eq!(h.machine.cpu().read_u32(RESULT).unwrap(), 0xdeadbeef);
    h.machine
        .cpu_mut()
        .write_u32(CB + 16, loader::MODULE_BASE + 0x40)
        .unwrap();
    assert_eq!(
        h.machine.deliver_signals(16).unwrap(),
        vec![Outcome::Budget]
    );
    assert_eq!(h.machine.cpu().read_u32(RESULT).unwrap(), 0);
    let saved = h.machine.grava_estado();
    h.machine.restaura_estado(&saved).unwrap();
    for _ in 0..128 {
        h.machine.advance(16).unwrap();
        if h.machine.handset_event_ready() {
            break;
        }
    }
    assert_eq!(h.machine.cpu().read_u32(MARKER).unwrap(), 1);
    h.machine.cpu_mut().write_u32(MARKER, 0).unwrap();
    h.machine.cpu_mut().write_u32(OUTPUT, 0xa5a5a5a5).unwrap();
    h.machine.deliver_signals(10000).unwrap();
    assert_eq!(h.machine.cpu().read_u32(MARKER).unwrap(), 0);
}

#[test]
fn current_net_class_creates_existing_offline_manager_instead_of_null() {
    let mut h = Harness::new();
    let net = h.create(0x0100102e);
    assert_eq!(
        h.rsa(net, 5, &[1]),
        0,
        "OpenSocket offline devolve ponteiro nulo"
    );
    assert_eq!(h.rsa(net, 4, &[]), 0x216, "GetLastError descreve NETDOWN");
    h.machine.cpu_mut().write_mem(OUTPUT, &[0xa5; 36]).unwrap();
    assert_eq!(h.rsa(net, 6, &[OUTPUT]), 4);
    let mut data = [0; 36];
    h.machine.cpu().read_mem(OUTPUT, &mut data).unwrap();
    assert_eq!(&data[..32], &[0; 32]);
    assert_eq!(&data[32..], &[0xa5; 4]);
}

#[test]
fn rsa_requeued_callback_cannot_starve_another_object_even_after_restore() {
    for restore in [false, true] {
        let mut h = Harness::new();
        let a = h.create(0x0100101a);
        let b = h.create(0x0100101a);
        assert!(a < b);
        assert_eq!(h.key(a, &[0x0c, 0xa1], &[17]), 0);
        assert_eq!(h.key(b, &[0x0c, 0xa1], &[17]), 0);
        h.machine.cpu_mut().write_mem(SIG, &[0, 65]).unwrap();
        let cb_b = CB + 0x200;
        let result_b = RESULT + 0x200;
        let marker_b = MARKER + 0x200;
        let table = h.machine.cpu().read_u32(a).unwrap();
        let modexp = h.machine.cpu().read_u32(table + 4 * 4).unwrap();
        for (i, value) in [0, modexp, a, SIG, 2, PTR, LENGTH, RESULT, CB]
            .into_iter()
            .enumerate()
        {
            h.machine
                .cpu_mut()
                .write_u32(OUTPUT + i as u32 * 4, value)
                .unwrap();
        }
        for (cb, notify, context, result) in [
            (CB, loader::MODULE_BASE + 0x80, OUTPUT, RESULT),
            (cb_b, loader::MODULE_BASE + 0x20, marker_b, result_b),
        ] {
            h.machine.cpu_mut().write_mem(cb, &[0; 28]).unwrap();
            h.machine.cpu_mut().write_u32(cb + 16, notify).unwrap();
            h.machine.cpu_mut().write_u32(cb + 20, context).unwrap();
            h.machine.cpu_mut().write_u32(result, 0xdeadbeef).unwrap();
        }
        h.rsa(a, 4, &[SIG, 2, PTR, LENGTH, RESULT, CB]);
        h.rsa(b, 4, &[SIG, 2, PTR, LENGTH, result_b, cb_b]);
        h.machine.deliver_signals(10000).unwrap();
        assert_eq!(h.machine.cpu().read_u32(OUTPUT).unwrap(), 1);
        assert_eq!(h.machine.cpu().read_u32(result_b).unwrap(), 0xdeadbeef);
        if restore {
            let saved = h.machine.grava_estado();
            h.machine.restaura_estado(&saved).unwrap();
        }
        h.machine.deliver_signals(10000).unwrap();
        assert_eq!(
            h.machine.cpu().read_u32(marker_b).unwrap(),
            1,
            "o segundo objeto precisa concluir na volta seguinte"
        );
        assert_eq!(h.machine.cpu().read_u32(result_b).unwrap(), 0);
        assert_eq!(h.machine.cpu().read_u32(OUTPUT).unwrap(), 1);
    }
}
