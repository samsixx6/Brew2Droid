// DIBs restaurados preservam pixels em chamadas aninhadas e as vigias de outros donos.
#![allow(dead_code)]
include!("common/modern_ui_fixture.rs");

fn native_u16(f: &F, address: u32) -> u16 {
    let mut bytes = [0; 2];
    f.m.cpu().read_mem(address, &mut bytes).unwrap();
    u16::from_le_bytes(bytes)
}

fn native_layout(f: &F, bitmap: u32) -> (u32, u32, u32, u32) {
    let pixels = f.read(bitmap + 8);
    let width = u32::from(native_u16(f, bitmap + 20));
    let height = u32::from(native_u16(f, bitmap + 22));
    let pitch = u32::from(native_u16(f, bitmap + 24));
    assert_ne!(pixels, 0);
    assert_eq!(pitch, (width * 2).div_ceil(4) * 4);
    (pixels, width, height, pitch)
}

fn widget_background_after_fresh_restore(class: u32) {
    let mut source = F::new();
    let widget = source.create(class);
    source.put(OUT, 9);
    source.put(OUT + 4, 9);
    assert_eq!(source.call(widget, 7, [OUT, 0, 0]), 0);
    assert_eq!(source.call(widget, 3, [0x801, 0x130, 0xff0000ff]), 1);
    let bitmap = source.bitmap();
    assert_eq!(source.call(bitmap, 2, [0x0101e443, OUT, 0]), 0);
    let canvas = source.read(OUT);
    let state = source.m.grava_estado();

    let mut restored = F::new();
    restored.m.restaura_estado(&state).unwrap();
    let (pixels, _, _, pitch) = native_layout(&restored, bitmap);
    let native_pixel = pixels + 9 * pitch + 8 * 2;
    assert_eq!(restored.m.screen().get_pixel(8, 9), 0);
    assert_eq!(native_u16(&restored, native_pixel), 0);
    let before = restored.m.screen().escritas();
    assert_eq!(restored.call(widget, 10, [canvas, 4, 5]), 0);
    assert_eq!(
        restored.m.screen().get_pixel(8, 9),
        31,
        "fundo azul deve sobreviver a chamadas aninhadas de IHFont para texto vazio"
    );
    assert_eq!(native_u16(&restored, native_pixel), 31);
    assert_eq!(restored.m.screen().escritas() - before, 81);
}

#[test]
fn fresh_restore_static_background_and_native_pixels_survive_nested_font() {
    widget_background_after_fresh_restore(0x010551e0);
}

#[test]
fn fresh_restore_button_background_is_a_non_font_control() {
    widget_background_after_fresh_restore(0x01053c8b);
}

fn exposed_compatible(f: &mut F, device: u32, width: u32, height: u32) -> u32 {
    assert_eq!(f.call(device, 13, [OUT, width, height]), 0);
    let bitmap = f.read(OUT);
    assert_eq!(f.call(bitmap, 2, [0x01001045, OUT, 0]), 0);
    assert_eq!(f.read(OUT), bitmap);
    bitmap
}

#[test]
fn restore_replaces_dib_watch_ids_and_padded_ranges_but_preserves_other_watches() {
    let mut source = F::new();
    let source_device = source.bitmap();
    let source_bitmap = exposed_compatible(&mut source, source_device, 7, 3);
    let (source_pixels, _, _, source_pitch) = native_layout(&source, source_bitmap);
    assert_eq!(source_pitch, 16, "largura ímpar inclui padding nativo");
    let state = source.m.grava_estado();

    let mut used = F::new();
    let old_device = used.bitmap();
    let old_bitmap = exposed_compatible(&mut used, old_device, 13, 5);
    let removed_bitmap = exposed_compatible(&mut used, old_device, 3, 2);
    assert_eq!(old_bitmap, source_bitmap, "mesmo id precisa receber uma faixa menor");
    assert_ne!(removed_bitmap, source_bitmap);
    let (old_pixels, _, old_height, old_pitch) = native_layout(&used, old_bitmap);
    assert!(old_pitch * old_height > source_pitch * 3);
    assert_eq!(old_pixels, source_pixels);
    used.m.cpu_mut().take_dirty(old_bitmap);
    used.m.cpu_mut().take_dirty(removed_bitmap);

    // EGL reserva id0; uma restauração de DIB não pode apagar vigias de outro dono.
    let unrelated_address = OUT + 4096;
    used.m.cpu_mut().watch_dirty(0, unrelated_address, 4).unwrap();
    assert!(used.m.cpu_mut().take_dirty(0));
    assert!(!used.m.cpu_mut().take_dirty(0));

    used.m.restaura_estado(&state).unwrap();
    let (pixels, width, height, pitch) = native_layout(&used, source_bitmap);
    assert_eq!((pixels, width, height, pitch), (source_pixels, 7, 3, 16));

    assert!(!used.m.cpu_mut().take_dirty(0), "vigia alheia deve permanecer registrada e limpa");
    used.m.cpu_mut().marca_sujo(unrelated_address, 4);
    assert!(used.m.cpu_mut().take_dirty(0));
    assert!(!used.m.cpu_mut().take_dirty(0));

    // Id sem vigia usa o fallback conservador true a cada consulta.
    assert_eq!(
        [used.m.cpu_mut().take_dirty(removed_bitmap), used.m.cpu_mut().take_dirty(removed_bitmap)],
        [true, true],
        "id presente apenas na máquina anterior deve perder sua vigia de DIB"
    );

    used.m.cpu_mut().take_dirty(source_bitmap);
    assert!(!used.m.cpu_mut().take_dirty(source_bitmap));
    used.m.cpu_mut().marca_sujo(pixels + pitch * height - 1, 1);
    assert!(used.m.cpu_mut().take_dirty(source_bitmap), "último byte de padding pertence ao raster");
    assert!(!used.m.cpu_mut().take_dirty(source_bitmap));
    used.m.cpu_mut().marca_sujo(pixels + pitch * height, 1);
    assert!(!used.m.cpu_mut().take_dirty(source_bitmap), "a faixa restaurada termina antes da antiga");
}

