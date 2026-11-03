use std::path::PathBuf;
use zeebx::{
    brew::aee::{self, Interface},
    cpu::{BackendPadrao, CpuBackend, Reg},
    loader::{self, modfile::ModImage},
    machine::{Machine, Outcome},
    storage::StoragePaths,
};

const SCRATCH: u32 = loader::STACK_BASE + 0x100;

struct Fixture {
    machine: Machine<BackendPadrao>,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local-tests").join(format!(
            "startup-images-{}-{}", std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        std::fs::create_dir_all(&root).unwrap();
        let storage = StoragePaths::from_root(root.join("runtime"));
        storage.create_dirs().unwrap();
        let image = ModImage::parse(
            [0xe3a0_0000u32, 0xe12f_ff1e].into_iter().flat_map(u32::to_le_bytes).collect(),
        ).unwrap();
        let mut machine = Machine::new_with_storage(
            BackendPadrao::new().unwrap(), loader::load_with(&image, &[]).unwrap(),
            &root, &storage, None,
        );
        assert_eq!(machine.run(1000).unwrap(), Outcome::Returned { code: 0 });
        Self { machine, root }
    }

    fn call(&mut self, iface: Interface, name: &str, args: [u32; 4]) -> u32 {
        let slot = (0..256).find(|slot| iface.method(*slot) == Some(name)).unwrap();
        match self.machine.call_guest(aee::encode(iface, slot), args, 1000).unwrap() {
            Outcome::Returned { code } => code,
            other => panic!("{name}: {other:?}"),
        }
    }

    fn resource(&mut self, bytes: &[u8]) -> u32 {
        std::fs::write(self.root.join("preview.bmp"), bytes).unwrap();
        self.machine.cpu_mut().write_mem(SCRATCH, b"preview.bmp\0").unwrap();
        let image = self.call(Interface::Shell, "LoadResObject", [loader::OBJECT_BASE, SCRATCH, 0, 0]);
        assert_ne!(image, 0);
        image
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let root = self.root.canonicalize().unwrap();
        assert!(root.starts_with(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local-tests").canonicalize().unwrap()));
        std::fs::remove_dir_all(root).unwrap();
    }
}

fn preview_bmp() -> Vec<u8> {
    let mut bmp = vec![0; 62];
    bmp[..2].copy_from_slice(b"BM");
    for (at, value) in [(2, 62u32), (10, 54), (14, 40), (18, 2), (22, 1)] {
        bmp[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
    bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
    bmp[54..62].copy_from_slice(&[255, 0, 255, 0, 0, 255, 0, 0]);
    bmp
}

#[test]
fn legacy_image_oldmask_preserves_background_and_copy_restores_magenta() {
    let mut f = Fixture::new();
    let image = f.resource(&preview_bmp());
    // Nemesis usa SetParm(IPARM_ROP, OLDMASK, 0) no IImage do preview.
    // A mesma imagem volta a COPY para provar que a chave não foi apagada ao decodificar.
    for (rop, x, expected) in [(4, 0, 0), (7, 2, 0), (2, 4, 0xf81f)] {
        f.call(Interface::Image, "SetParm", [image, 3, rop, 0]);
        f.call(Interface::Image, "Draw", [image, x, 0, 0]);
        assert_eq!(f.machine.screen().get_pixel(x as i32, 0), expected, "IPARM_ROP={rop}");
        assert_eq!(f.machine.screen().get_pixel(x as i32 + 1, 0), 0xf800);
    }
}

#[test]
fn native_bitmap_pixel_probe_does_not_mistake_header_padding_for_a_row() {
    let mut f = Fixture::new();
    // Penoa mede o formato nativo com 32 pixels pretos seguidos de um marcador.
    // Um prefixo com uma linha falsa de zeros muda a profundidade inferida pelo jogo.
    let mut bmp = vec![0; 54 + 40 * 3];
    bmp[..2].copy_from_slice(b"BM");
    let size = bmp.len() as u32;
    for (at, value) in [(2, size), (10, 54), (14, 40), (18, 40), (22, 1)] {
        bmp[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
    bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
    bmp[54 + 32 * 3..].fill(255);
    let input = f.call(Interface::Helpers, "malloc", [size, 0, 0, 0]);
    f.machine.cpu_mut().write_mem(input, &bmp).unwrap();
    let native = f.call(Interface::Helpers, "SetupNativeImage", [0x01004001, input, 0, 0]);
    assert_ne!(native, 0);
    let mut bytes = [0; 256];
    f.machine.cpu().read_mem(native, &mut bytes).unwrap();
    let (mut at, mut zeros) = (0, 0);
    while at < bytes.len() && zeros < 32 {
        zeros = if bytes[at] == 0 { zeros + 1 } else { 0 };
        at += 1;
    }
    assert_eq!(zeros, 32);
    while at < bytes.len() && bytes[at] == 0 {
        zeros += 1;
        at += 1;
    }
    let bytes_per_pixel = zeros / 32;
    let inferred_pixels = native + (at - bytes_per_pixel * 32) as u32;
    assert_eq!(bytes_per_pixel, 2, "o teste do próprio guest precisa identificar RGB565");
    assert_eq!(inferred_pixels, f.machine.cpu().read_u32(native + 8).unwrap());
}

fn saf_rgb332() -> Vec<u8> {
    // Fixture própria: tabela DEFLATE canônica externa, quatro literais e fim.
    // Os literais são vermelho, verde, azul e magenta; não há arte de um jogo.
    vec![
        0x53,0x41,0x46,0x00,0x02,0x00,0x6f,0x07,0x00,0x02,0x00,0x15,
        0x01,0x06,0x02,0x02,0x08,0x04,0x00,0x00,0x02,0x02,0x01,0x00,
        0x03,0x03,0x01,0x00,0x07,0x0a,0x02,0x01,0x90,0x0b,0x00,0x2a,
        0x00,0x38,0x80,0x00,0x00,0x00,0x40,0x00,0x84,0x01,0x00,0x00,
        0x06,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,
        0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,
        0xc0,0x0c,0x00,0x00,0x00,0x05,0x04,0x00,0x05,0x00,
        0x02,0x02,0x01,0xe3,0x00,0x08,0x78,0x9c,0x5d,0x73,0x04,0xc1,
        0x01,0xe3,0x06,0x00,0x09,0xe3,0x01,0x00,0x05,0x00,0x00,0x00,
        0x00,0x00,0x08,0x00,0x00,
    ]
}

fn bci_indexed() -> Vec<u8> {
    // Fixture própria, três colunas e duas linhas. O quarto byte de cada linha
    // é enchimento, deliberadamente fora da paleta para revelar passo incorreto.
    let mut bytes = vec![0; 80];
    bytes[..4].copy_from_slice(b"BCI\0");
    for (at, value) in [(4, 99u32), (8, 68), (68, 19)] {
        bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    for (at, value) in [
        (12, 1u16), (14, 3), (16, 2), (18, 1), (20, 1), (22, 1),
        (26, 8), (28, 1), (30, 4), (48, 150), (50, 1), (56, 1),
        (72, 3), (74, 2), (76, 1), (78, 1),
    ] {
        bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }
    bytes[32..48].copy_from_slice(&[
        0,0,255,0, 0,255,0,0, 255,0,0,0, 255,0,255,0,
    ]);
    // zlib, bloco DEFLATE sem compressão, oito índices e Adler32.
    bytes.extend_from_slice(&[
        120,1,1,8,0,247,255,0,1,2,255,3,2,1,255,6,41,2,8,
    ]);
    bytes
}

#[test]
fn bci_static_palette_rows_preserve_color_and_skip_alignment_bytes() {
    let decoded = zeebx::video::icon::decode(&bci_indexed()).unwrap();
    assert_eq!((decoded.width, decoded.height), (3, 2));
    assert_eq!(decoded.rgba, [
        255,0,0,255, 0,255,0,255, 0,0,255,255,
        255,0,255,255, 0,0,255,255, 0,255,0,255,
    ]);
    // O BCI Authoring Tool pode deixar bytes antigos depois do tamanho declarado.
    let mut untrimmed = bci_indexed();
    untrimmed.extend_from_slice(&[0xfe, 0x12]);
    assert_eq!(zeebx::video::icon::decode(&untrimmed).unwrap(), decoded);
    let mut f = Fixture::new();
    let image = f.resource(&bci_indexed());
    f.call(Interface::Image, "SetParm", [image, 3, 4, 0]);
    f.call(Interface::Image, "Draw", [image, 0, 0, 0]);
    assert_eq!(f.machine.screen().get_pixel(0, 0), 0xf800);
    assert_eq!(f.machine.screen().get_pixel(0, 1), 0);
    assert_eq!(f.machine.screen().get_pixel(1, 1), 0x001f);
}

#[test]
fn bci_rejects_truncation_corruption_unbounded_rasters_and_animation() {
    let original = bci_indexed();
    for end in 0..original.len() {
        let mut truncated = original[..end].to_vec();
        if end >= 8 { truncated[4..8].copy_from_slice(&(end as u32).to_le_bytes()); }
        assert!(zeebx::video::icon::decode(&truncated).is_err(), "corte {end}");
    }
    let mut damaged = original.clone();
    *damaged.last_mut().unwrap() ^= 1;
    assert!(zeebx::video::icon::decode(&damaged).is_err());
    let mut bad_index = original.clone();
    bad_index[87] = 4;
    // Adler32 atualizado: o fluxo é válido, mas o primeiro índice não existe.
    bad_index[95..99].copy_from_slice(&[6,73,2,12]);
    assert!(zeebx::video::icon::decode(&bad_index).is_err());
    let mut trailing = original.clone();
    trailing.push(0);
    trailing[4..8].copy_from_slice(&100u32.to_le_bytes());
    trailing[68..72].copy_from_slice(&20u32.to_le_bytes());
    assert!(zeebx::video::icon::decode(&trailing).is_err());
    for (at, value) in [
        (14, 0u16), (16, 65535), (20, 2), (22, 2), (26, 4),
        (30, 257), (56, 2), (60, 1), (72, 65535), (74, 65535), (78, 2),
    ] {
        let mut bytes = original.clone();
        bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
        assert!(zeebx::video::icon::decode(&bytes).is_err(), "campo {at}");
    }
    // A altura maior exigiria mais bytes que o DEFLATE entrega; a menor faria
    // o mesmo fluxo ultrapassar o raster declarado. Ambos devem ser recusados.
    for height in [1u16, 3] {
        let mut bytes = original.clone();
        bytes[16..18].copy_from_slice(&height.to_le_bytes());
        bytes[74..76].copy_from_slice(&height.to_le_bytes());
        assert!(zeebx::video::icon::decode(&bytes).is_err());
    }
    let mut enormous = original;
    for at in [14, 16, 72, 74] {
        enormous[at..at + 2].copy_from_slice(&65535u16.to_le_bytes());
    }
    assert!(zeebx::video::icon::decode(&enormous).is_err());
}

#[test]
fn saf_external_dynamic_table_decodes_the_actual_rgb332_pixels() {
    let decoded = zeebx::video::icon::decode(&saf_rgb332()).unwrap();
    assert_eq!((decoded.width, decoded.height), (2, 2));
    assert_eq!(decoded.rgba, [
        255,0,0,255, 0,255,0,255, 0,0,255,255, 255,0,255,255,
    ]);
}

#[test]
fn saf_resource_uses_image_oldmask_without_destroying_the_color_key() {
    let mut f = Fixture::new();
    let image = f.resource(&saf_rgb332());
    f.call(Interface::Image, "SetParm", [image, 3, 4, 0]);
    f.call(Interface::Image, "Draw", [image, 0, 0, 0]);
    assert_eq!(f.machine.screen().get_pixel(0, 0), 0xf800);
    assert_eq!(f.machine.screen().get_pixel(1, 0), 0x07e0);
    assert_eq!(f.machine.screen().get_pixel(0, 1), 0x001f);
    assert_eq!(f.machine.screen().get_pixel(1, 1), 0);
    f.call(Interface::Image, "SetParm", [image, 3, 2, 0]);
    f.call(Interface::Image, "Draw", [image, 0, 0, 0]);
    assert_eq!(f.machine.screen().get_pixel(1, 1), 0xf81f);
}

#[test]
fn saf_frame_transparency_uses_its_declared_index_and_background() {
    let mut bytes = saf_rgb332();
    bytes[85] = 224; // vermelho é a chave deste objeto
    bytes[99] = 3; // azul é o fundo deste quadro
    bytes[105] = 2; // composição SAF com a chave do objeto
    let decoded = zeebx::video::icon::decode(&bytes).unwrap();
    assert_eq!(decoded.rgba, [
        0,0,255,255, 0,255,0,255, 0,0,255,255, 255,0,255,255,
    ]);
}

fn saf_two_objects() -> Vec<u8> {
    let original = saf_rgb332();
    let mut bytes = original[..96].to_vec();
    bytes[14] = 4;
    bytes[22] = 2;
    let mut second = original[78..96].to_vec();
    second[3] = 1;
    bytes.extend_from_slice(&second);
    // Um quadro, dois objetos. O magenta do segundo é transparente e conserva
    // o fundo preto; o do primeiro é copiado como qualquer outra cor.
    bytes.extend_from_slice(&[
        6,0,16, 0,2,
        0,5,0,0,0,0,0,
        0,5,1,0,2,2,0,
        8,0,0,
    ]);
    let size = bytes.len() as u16;
    bytes[5..7].copy_from_slice(&size.to_be_bytes());
    bytes
}

#[test]
fn saf_static_frame_composes_two_objects_in_order_with_independent_placement() {
    let decoded = zeebx::video::icon::decode(&saf_two_objects()).unwrap();
    assert_eq!((decoded.width, decoded.height), (4, 2));
    assert_eq!(decoded.rgba, [
        255,0,0,255, 0,255,0,255, 255,0,0,255, 0,255,0,255,
        0,0,255,255, 255,0,255,255, 0,0,255,255, 0,0,0,255,
    ]);
}

#[test]
fn saf_object_table_rejects_duplicate_ids_missing_references_and_wrong_counts() {
    let original = saf_two_objects();
    for (offset, value) in [(99, 0), (128, 2), (22, 3), (22, 129)] {
        let mut damaged = original.clone();
        damaged[offset] = value;
        assert!(zeebx::video::icon::decode(&damaged).is_err(), "tabela inválida em {offset}");
    }
}

#[test]
fn saf_rejects_truncation_bad_checksum_and_output_past_the_declared_raster() {
    let original = saf_rgb332();
    for end in 0..original.len() {
        let mut truncated = original[..end].to_vec();
        if end >= 7 {
            // O tamanho externo continua consistente: os limites internos de
            // cada comando e do DEFLATE também precisam detectar a truncagem.
            truncated[5..7].copy_from_slice(&(end as u16).to_be_bytes());
        }
        assert!(zeebx::video::icon::decode(&truncated).is_err(), "truncado em {end}");
    }
    for (offset, value) in [(95, 0), (82, 1), (83, 1), (26, 2), (34, 255),
        (101, 33), (5, 1), (29, 9), (23, 32)] {
        let mut damaged = original.clone();
        damaged[offset] = value;
        assert!(zeebx::video::icon::decode(&damaged).is_err(), "campo inválido em {offset}");
    }
}

const OEM_DESCRIPTOR_PAIRS: [(&str, &str); 3] = [
    ("0ce4c727fad23abd0588a65b1b42de2b338cdbb972d34f431424ca7d5ed9c372",
     "23ee170d578cc4d8edc993dcb2ed10766b1189e72d6ebbc6f248f115451c443c"),
    ("9fe4954e114fcbeeb9c16452c0108ea8536b5a02b09b2c131c861113c4537f4d",
     "dcd418e20007ce90254fb1b3f520d295aa8b18a163f5635cc48d586389f6e604"),
    ("f693a0ff75f98efe034c88f36e0dd24ec6b7302190b7dbc34d6a4914e53a07e8",
     "8149b5e92bf6a9c51730acaa271e698ea068778b35e98106d0069f4904c42e95"),
];

impl Fixture {
    fn oem_descriptor(&mut self) -> u32 {
        let pointer = self.call(Interface::Helpers, "malloc", [24, 0, 0, 0]);
        assert_ne!(pointer, 0);
        for (offset, value) in [(0, 2), (4, 3), (8, pointer + 12)] {
            self.machine.cpu_mut().write_u32(pointer + offset, value).unwrap();
        }
        let pixels: Vec<u8> = [0xf800u16, 0x07e0, 0x001f, 0xffff, 0x0000, 0xf81f]
            .into_iter().flat_map(u16::to_le_bytes).collect();
        self.machine.cpu_mut().write_mem(pointer + 12, &pixels).unwrap();
        pointer
    }

    fn oem_blit(&mut self, pointer: u32) {
        let old_sp = self.machine.cpu().read_reg(Reg::Sp);
        let sp = old_sp - 32;
        self.machine.cpu_mut().write_reg(Reg::Sp, sp);
        for (i, value) in [2, pointer, 1, 0, 2].into_iter().enumerate() {
            self.machine.cpu_mut().write_u32(sp + i as u32 * 4, value).unwrap();
        }
        self.call(Interface::Display, "BitBlt", [loader::OBJECT_BASE + 16, 1, 1, 2]);
        self.machine.cpu_mut().write_reg(Reg::Sp, old_sp);
    }
}

#[test]
fn selected_oem_descriptor_blits_rgb565_with_source_stride_and_origin() {
    // Três módulos fornecidos constroem {altura32, largura32, pixels=self+12}.
    // Não é o IDIB moderno nem o raster sem cabeçalho do outro contrato OEM.
    for (module, mif) in OEM_DESCRIPTOR_PAIRS {
        let mut f = Fixture::new();
        f.machine.set_handset_runtime_identity(module, mif);
        let pointer = f.oem_descriptor();
        f.oem_blit(pointer);
        assert_eq!(f.machine.screen().get_pixel(1, 1), 0x07e0);
        assert_eq!(f.machine.screen().get_pixel(2, 1), 0x001f);
        assert_eq!(f.machine.screen().get_pixel(1, 2), 0);
        assert_eq!(f.machine.screen().get_pixel(2, 2), 0xf81f);
    }
}

#[test]
fn oem_descriptor_requires_both_verified_hashes() {
    for (module, mif) in OEM_DESCRIPTOR_PAIRS {
        for identity in [(module, "00"), ("00", mif), ("00", "00")] {
            let mut f = Fixture::new();
            f.machine.set_handset_runtime_identity(identity.0, identity.1);
            let pointer = f.oem_descriptor();
            f.oem_blit(pointer);
            assert_eq!(f.machine.screen().get_pixel(1, 1), 0);
        }
    }
}

#[test]
fn selected_oem_descriptor_rejects_invalid_dimensions_pointer_and_allocation() {
    for (offset, value) in [(0, 0), (0, u32::MAX), (4, 1025), (4, 1024), (8, 0)] {
        let mut f = Fixture::new();
        let (module, mif) = OEM_DESCRIPTOR_PAIRS[0];
        f.machine.set_handset_runtime_identity(module, mif);
        let pointer = f.oem_descriptor();
        f.machine.cpu_mut().write_u32(pointer + offset, value).unwrap();
        f.oem_blit(pointer);
        assert_eq!(f.machine.screen().get_pixel(1, 1), 0, "campo {offset}={value}");
    }
}

const PADDED_NATIVE_PAIR: (&str, &str) = (
    "3786a3f4af6841f489a279e2575dc64282afb47cb1b3f39ad4d70d8cb17043a7",
    "bda94290d1941ec2ea2d1f731568fd080ab700636515eb1e7f294e199a290a5f",
);

impl Fixture {
    fn small_native(&mut self) -> u32 {
        let mut bmp = preview_bmp();
        bmp.resize(70, 0);
        bmp[2..6].copy_from_slice(&70u32.to_le_bytes());
        bmp[22..26].copy_from_slice(&2u32.to_le_bytes());
        // BMP de baixo para cima: azul/magenta; vermelho/verde.
        bmp[54..].copy_from_slice(&[255,0,0,255,0,255,0,0, 0,0,255,0,255,0,0,0]);
        let input = self.call(Interface::Helpers, "malloc", [70,0,0,0]);
        self.machine.cpu_mut().write_mem(input, &bmp).unwrap();
        let native = self.call(Interface::Helpers, "SetupNativeImage", [0x01004001,input,0,0]);
        assert_ne!(native, 0);
        native
    }

    fn copy_native(&mut self, native: u32, y: u32) {
        let old_sp = self.machine.cpu().read_reg(Reg::Sp);
        let sp = old_sp - 32;
        self.machine.cpu_mut().write_reg(Reg::Sp, sp);
        for (i, value) in [2,native,0,0,2].into_iter().enumerate() {
            self.machine.cpu_mut().write_u32(sp + i as u32 * 4, value).unwrap();
        }
        self.call(Interface::Display, "BitBlt", [loader::OBJECT_BASE+16,0,y,2]);
        self.machine.cpu_mut().write_reg(Reg::Sp, old_sp);
    }

    fn guest_writes_cached_native_pixels(&mut self, native: u32) {
        // ADD base,#52; quatro STRH reais exercitam a vigia do backend.
        // Uma injeção de bytes pelo host não representa uma escrita do guest.
        let code = loader::MODULE_BASE + 0x100;
        let instructions = [0xe2800034u32,0xe1c010b0,0xe1c020b2,0xe1c030b4,
            0xe3a01000,0xe1c010b6,0xe3a00000,0xe12fff1e];
        self.machine.cpu_mut().write_mem(code,
            &instructions.into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>()).unwrap();
        assert_eq!(self.machine.call_guest(code,[native,0x07e0,0x001f,0xffff],1000).unwrap(),
            Outcome::Returned {code:0});
    }
}

#[test]
fn native_padding_profile_keeps_public_idib_and_cached_pixel_edits_contiguous() {
    let mut f = Fixture::new();
    f.machine.set_handset_runtime_identity(PADDED_NATIVE_PAIR.0, PADDED_NATIVE_PAIR.1);
    let native = f.small_native();
    assert_eq!(f.machine.cpu().read_u32(native+8).unwrap(), native+52);
    let mut fields = [0; 16];
    f.machine.cpu().read_mem(native+20, &mut fields).unwrap();
    assert_eq!(&fields[..6], &[2,0,2,0,4,0]);
    assert_eq!(fields[8], 16);
    let mut padding = [0xff; 16];
    f.machine.cpu().read_mem(native+36, &mut padding).unwrap();
    assert_eq!(padding, [0; 16]);
    f.copy_native(native, 0);
    assert_eq!(f.machine.screen().get_pixel(0,0), 0xf800);
    assert_eq!(f.machine.screen().get_pixel(1,1), 0xf81f);
    let guard = f.call(Interface::Helpers, "malloc", [8,0,0,0]);
    f.machine.cpu_mut().write_mem(guard, &[0xa5;8]).unwrap();
    // O guest usa o deslocamento52 guardado, em vez de consultar pBits.
    f.guest_writes_cached_native_pixels(native);
    f.copy_native(native, 3);
    assert_eq!(f.machine.screen().get_pixel(0,3), 0x07e0);
    assert_eq!(f.machine.screen().get_pixel(1,3), 0x001f);
    assert_eq!(f.machine.screen().get_pixel(0,4), 0xffff);
    assert_eq!(f.machine.cpu().read_u32(guard).unwrap(), 0xa5a5a5a5);
    assert_eq!(f.call(Interface::Bitmap,"QueryInterface",[native,0x01001045,SCRATCH+512,0]),0);
    assert_eq!(f.machine.cpu().read_u32(SCRATCH+512).unwrap(), native);
    assert_eq!(f.machine.cpu().read_u32(native+8).unwrap(), native+52);
}

#[test]
fn native_padding_profile_requires_both_hashes_and_preserves_default36() {
    for (module,mif) in [(PADDED_NATIVE_PAIR.0,"00"),("00",PADDED_NATIVE_PAIR.1),("00","00")] {
        let mut f = Fixture::new();
        f.machine.set_handset_runtime_identity(module,mif);
        let native = f.small_native();
        assert_eq!(f.machine.cpu().read_u32(native+8).unwrap(), native+36);
    }
}

#[test]
fn native_padding_profile_roundtrips_and_rejects_saved_pixels_outside_the_owned_block() {
    let mut f = Fixture::new();
    f.machine.set_handset_runtime_identity(PADDED_NATIVE_PAIR.0,PADDED_NATIVE_PAIR.1);
    let native = f.small_native();
    assert_eq!(f.machine.cpu().read_u32(native+8).unwrap(), native+52);
    f.guest_writes_cached_native_pixels(native);
    f.copy_native(native, 0);
    let state = f.machine.grava_estado();
    let mut restored = Fixture::new();
    restored.machine.set_handset_runtime_identity(PADDED_NATIVE_PAIR.0,PADDED_NATIVE_PAIR.1);
    restored.machine.restaura_estado(&state).unwrap();
    restored.copy_native(native, 3);
    assert_eq!(restored.machine.screen().get_pixel(0,3), 0x07e0);
    assert_eq!(restored.machine.screen().get_pixel(1,3), 0x001f);
    let parsed = zeebx::save_state::Leitor::abre(&state).unwrap();
    let mut buffers = parsed.pares("tab.dib_buffers").unwrap();
    buffers.iter_mut().find(|(bitmap,_)| *bitmap==native).unwrap().1 = native+64;
    let mut damaged = zeebx::save_state::Secoes::nova();
    for name in parsed.nomes() {
        if name=="tab.dib_buffers" {damaged.poe_mapa(name,buffers.iter().copied());}
        else {damaged.poe(name,parsed.secao(name).unwrap().to_vec());}
    }
    let mut rejected = Fixture::new();
    rejected.machine.set_handset_runtime_identity(PADDED_NATIVE_PAIR.0,PADDED_NATIVE_PAIR.1);
    assert!(rejected.machine.restaura_estado(&damaged.fecha()).is_err(),
        "prefixo legado64 não autoriza ler pixels além da alocação inteira");
}

#[test]
fn native_padding_profile_preserves_valid_cache_layouts_and_ignores_malformed_data() {
    let mut cases = Vec::new();
    for offset in [36u32,52,64] {
        let mut bytes = offset.to_le_bytes().to_vec();
        bytes.extend_from_slice(&[16,0,3]);
        cases.push((bytes,offset));
    }
    for offset in [0u32,35,37,51,53,u32::MAX] {
        let mut bytes = offset.to_le_bytes().to_vec();
        bytes.extend_from_slice(&[16,0,3]);
        cases.push((bytes,52));
    }
    for bytes in [vec![36,0,0,0,16,0],vec![36,0,0,0,16,0,3,0],
        vec![36,0,0,0,8,0,3],vec![36,0,0,0,16,0,4]] {
        cases.push((bytes,52));
    }
    for (cache,expected) in cases {
        let mut f = Fixture::new();
        std::fs::write(f.root.join("lcdinfo.dat"),&cache).unwrap();
        f.machine.set_handset_runtime_identity(PADDED_NATIVE_PAIR.0,PADDED_NATIVE_PAIR.1);
        let native = f.small_native();
        assert_eq!(f.machine.cpu().read_u32(native+8).unwrap(),native+expected,
            "cache {cache:?}");
        assert_eq!(std::fs::read(f.root.join("lcdinfo.dat")).unwrap(),cache);
    }
    let mut f = Fixture::new();
    std::fs::write(f.root.join("lcdinfo.dat"),[64,0,0,0,16,0,3]).unwrap();
    f.machine.set_handset_runtime_identity("00",PADDED_NATIVE_PAIR.1);
    let native = f.small_native();
    assert_eq!(f.machine.cpu().read_u32(native+8).unwrap(),native+36);
}

#[test]
fn native_padding_profile_saved_raster_must_fit_after_the_public_header() {
    let mut f = Fixture::new();
    let native = f.small_native();
    let state = f.machine.grava_estado();
    let parsed = zeebx::save_state::Leitor::abre(&state).unwrap();
    let mut heap = parsed.pares("heap.live").unwrap();
    // Mantém os 36 bytes públicos, mas faltam quatro bytes do raster 2×2.
    heap.iter_mut().find(|(pointer,_)| *pointer==native).unwrap().1 = 40;
    let mut damaged = zeebx::save_state::Secoes::nova();
    for name in parsed.nomes() {
        if name=="heap.live" {damaged.poe_mapa(name,heap.iter().copied());}
        else {damaged.poe(name,parsed.secao(name).unwrap().to_vec());}
    }
    let mut restored = Fixture::new();
    assert!(restored.machine.restaura_estado(&damaged.fecha()).is_err());
}

#[test]
fn saf_object_decompression_flag_is_separate_from_its_seven_bit_id() {
    for id in [0u8,127] {
        let mut bytes = saf_rgb332();
        bytes[81] = 0x80 | id;
        bytes[103] = id;
        let decoded = zeebx::video::icon::decode(&bytes).unwrap();
        assert_eq!((decoded.width,decoded.height),(2,2));
        assert_eq!(decoded.rgba,[
            255,0,0,255, 0,255,0,255, 0,0,255,255, 255,0,255,255,
        ]);
    }
    // O sinalizador não cria um ID adicional: objetos 0 e 0x80 são duplicados.
    let mut duplicate = saf_two_objects();
    duplicate[99] = 0x80;
    assert!(zeebx::video::icon::decode(&duplicate).is_err());
}
