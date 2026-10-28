use std::io::Write;
use std::path::PathBuf;

use zeebx::brew::aee::{self, Interface};
use zeebx::cpu::{BackendPadrao, CpuBackend};
use zeebx::loader::{self, modfile::ModImage};
use zeebx::machine::{Machine, Outcome};
use zeebx::storage::StoragePaths;

const OUT: u32 = loader::STACK_BASE + 0x100;
const INPUT: u32 = loader::STACK_BASE + 0x1000;
const OUTPUT: u32 = loader::STACK_BASE + 0x40000;
const TEXT: &[u8] = b"synthetic archive index: graphics/title.pcx; music/title.mid; end";

struct Harness {
    machine: Machine<BackendPadrao>,
    work: PathBuf,
    source: u32,
    unzip: u32,
}

impl Harness {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let work = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local-tests").join(format!(
            "unzip-stream-test-{}-{}", std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        std::fs::create_dir_all(&work).unwrap();
        let storage = StoragePaths::from_root(work.join("runtime"));
        storage.create_dirs().unwrap();
        let image = ModImage::parse([0xe3a0_0000u32, 0xe12f_ff1e].into_iter()
            .flat_map(u32::to_le_bytes).collect()).unwrap();
        let module = loader::load_with(&image, &[]).unwrap();
        let mut result = Self {
            machine: Machine::new_with_storage(BackendPadrao::new().unwrap(), module, &work, &storage, None),
            work, source: 0, unzip: 0,
        };
        assert_eq!(result.machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
        result.source = result.create(0x0100_100c);
        result.unzip = result.create(0x0100_1014);
        result
    }

    fn call(&mut self, interface: Interface, name: &str, args: [u32; 4]) -> u32 {
        let slot = (0..256).find(|slot| interface.method(*slot) == Some(name)).unwrap();
        match self.machine.call_guest(aee::encode(interface, slot), args, 1000).unwrap() {
            Outcome::Returned { code } => code,
            outcome => panic!("{name}: {outcome:?}"),
        }
    }

    fn create(&mut self, class: u32) -> u32 {
        assert_eq!(self.call(Interface::Shell, "CreateInstance", [loader::OBJECT_BASE, class, OUT, 0]), 0);
        let object = self.machine.cpu().read_u32(OUT).unwrap();
        assert_ne!(object, 0);
        object
    }

    fn feed(&mut self, bytes: &[u8]) {
        self.machine.cpu_mut().write_mem(INPUT, bytes).unwrap();
        self.call(Interface::MemAStream, "SetEx", [self.source, INPUT, bytes.len() as u32, 0]);
        self.call(Interface::UnzipStream, "SetStream", [self.unzip, self.source, 0, 0]);
    }

    fn read(&mut self, want: u32) -> Vec<u8> {
        let got = self.call(Interface::UnzipStream, "Read", [self.unzip, OUTPUT, want, 0]);
        let mut output = vec![0; got as usize];
        self.machine.cpu().read_mem(OUTPUT, &mut output).unwrap();
        output
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        assert!(self.work.canonicalize().unwrap().starts_with(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).canonicalize().unwrap().join("local-tests"),
        ));
        std::fs::remove_dir_all(&self.work).unwrap();
    }
}

fn zlib() -> Vec<u8> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(TEXT).unwrap();
    encoder.finish().unwrap()
}

#[test]
fn unzip_accepts_only_the_missing_adler_after_a_complete_zlib_deflate_payload() {
    let mut wrapped = zlib();
    assert_eq!(&wrapped[..2], &[0x78, 0x9c]);
    wrapped.truncate(wrapped.len() - 4);
    let mut h = Harness::new();
    h.feed(&wrapped);
    assert_eq!(h.read(7), &TEXT[..7]);
    // O estado existente guarda a saída e o cursor; o novo envelope não pede
    // outro formato de save nem pode repetir o início após restaurar.
    let saved = h.machine.grava_estado();
    h.machine.restaura_estado(&saved).unwrap();
    assert_eq!(h.read(1000), &TEXT[7..]);
    assert!(h.read(1000).is_empty());
}

#[test]
fn unzip_keeps_valid_zlib_raw_deflate_and_gzip_readable() {
    let mut raw = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    raw.write_all(TEXT).unwrap();
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(TEXT).unwrap();
    let mut h = Harness::new();
    for encoded in [zlib(), raw.finish().unwrap(), gzip.finish().unwrap()] {
        h.feed(&encoded);
        assert_eq!(h.read(1000), TEXT);
        assert!(h.read(1000).is_empty());
    }
}

#[test]
fn unzip_rejects_incomplete_deflate_invalid_headers_and_present_bad_checksums() {
    let encoded = zlib();
    let mut fixtures = Vec::new();
    // Retirar de um a três bytes do Adler não é o envelope sem Adler.
    for missing in 1..=3 { fixtures.push(encoded[..encoded.len() - missing].to_vec()); }
    // Cinco ou mais bytes removidos já cortam o DEFLATE, mesmo quando houve saída parcial.
    for missing in 5..=8 { fixtures.push(encoded[..encoded.len() - missing].to_vec()); }
    let mut checksum = encoded.clone();
    *checksum.last_mut().unwrap() ^= 1;
    fixtures.push(checksum);
    let mut header = encoded[..encoded.len() - 4].to_vec();
    header[1] ^= 1; // FCHECK inválido.
    fixtures.push(header);
    let mut dictionary = encoded[..encoded.len() - 4].to_vec();
    dictionary[1] = 0x20; // cabeçalho válido com FDICT, sem dicionário fornecido.
    fixtures.push(dictionary);
    let mut raw = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    raw.write_all(TEXT).unwrap();
    let mut truncated_raw = raw.finish().unwrap();
    truncated_raw.pop();
    fixtures.push(truncated_raw);
    let mut h = Harness::new();
    for (index, bytes) in fixtures.iter().enumerate() {
        h.feed(bytes);
        h.machine.cpu_mut().write_mem(OUTPUT, &[0xa5; 32]).unwrap();
        assert!(h.read(1000).is_empty(), "entrada inválida {index} entregou saída parcial");
        let mut untouched = [0; 32];
        h.machine.cpu().read_mem(OUTPUT, &mut untouched).unwrap();
        assert_eq!(untouched, [0xa5; 32]);
    }
}

#[test]
fn unzip_caps_checksumless_expansion_at_128_mib_before_exposing_any_output() {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    let block = [0; 64 * 1024];
    for _ in 0..2048 { encoder.write_all(&block).unwrap(); }
    encoder.write_all(&[0]).unwrap();
    let mut encoded = encoder.finish().unwrap();
    encoded.truncate(encoded.len() - 4);
    let mut h = Harness::new();
    h.feed(&encoded);
    assert!(h.read(1000).is_empty());
}


fn gzip_fixture() -> (Vec<u8>, Vec<u8>) {
    let mut seed = 0x1234_5678u32;
    let plain: Vec<u8> = (0..8192).map(|_| {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed as u8
    }).collect();
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(&plain).unwrap();
    (encoder.finish().unwrap(), plain)
}

#[test]
fn unzip_reads_gzip_header_from_a_compressed_prefix_and_preserves_cursor() {
    let (encoded, plain) = gzip_fixture();
    assert!(encoded.len() > 512);
    let mut h = Harness::new();
    h.feed(&encoded[..512]);
    assert!(h.read(0).is_empty());
    assert_eq!(h.read(64), plain[..64]);
    let saved = h.machine.grava_estado();
    h.machine.restaura_estado(&saved).unwrap();
    assert_eq!(h.read(64), plain[64..128]);
    let remainder = h.read(8192);
    assert_eq!(remainder, plain[128..128 + remainder.len()]);
    assert!(!remainder.is_empty());
    // O prefixo acabou antes do trailer: nao pode anunciar EOF validado.
    assert_eq!(h.call(Interface::UnzipStream, "Read", [h.unzip, OUTPUT, 1, 0]), u32::MAX);
    // SetStream descarta o estado parcial e permite ler o arquivo completo.
    h.feed(&encoded);
    assert_eq!(h.read(10000), plain);
    assert!(h.read(1).is_empty());
}

#[test]
fn unzip_rejects_complete_gzip_with_bad_crc_size_or_header_before_exposing_output() {
    let (encoded, _) = gzip_fixture();
    let mut fixtures = Vec::new();
    for offset in [encoded.len() - 8, encoded.len() - 4, 2] {
        let mut corrupt = encoded.clone();
        corrupt[offset] ^= 1;
        fixtures.push(corrupt);
    }
    let mut h = Harness::new();
    for encoded in fixtures {
        h.feed(&encoded);
        h.machine.cpu_mut().write_mem(OUTPUT, &[0xa5; 64]).unwrap();
        assert!(h.read(64).is_empty());
        let mut untouched = [0; 64];
        h.machine.cpu().read_mem(OUTPUT, &mut untouched).unwrap();
        assert_eq!(untouched, [0xa5; 64]);
    }
}
