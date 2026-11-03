use zeebx::video::{gles, rasterizer::GlState};
use zeebx::save_state::{Guardavel, Leitor, Secoes};

#[test]
fn guest_color_readback_cannot_leave_presentation_on_the_previous_frame() {
    let mut gl = GlState::new(4, 4);
    let mut guest_bytes = Vec::new();
    let mut presented_words = Vec::new();
    gl.frame_rgb565(4, 4, &mut guest_bytes);
    gl.frame_rgb565_words(4, 4, &mut presented_words);

    gl.set_clear_color([0.0, 0.0, 1.0, 1.0]);
    gl.clear(gles::GL_COLOR_BUFFER_BIT);
    gl.frame_rgb565(4, 4, &mut guest_bytes);
    gl.frame_rgb565_words(4, 4, &mut presented_words);
    assert_eq!(presented_words, vec![0x001f; 16]);
    assert_eq!(guest_bytes, [0x1f, 0x00].repeat(16));
}

#[test]
fn independent_readers_receive_the_current_frame_in_either_order() {
    let mut gl = GlState::new(2, 2);
    let mut first = vec![0; 8];
    let mut second = vec![0; 8];
    gl.set_clear_color([1.0, 0.0, 0.0, 1.0]);
    gl.clear(gles::GL_COLOR_BUFFER_BIT);
    gl.frame_rgb565(2, 2, &mut first);
    gl.frame_rgb565(2, 2, &mut second);
    assert_eq!(first, [0x00, 0xf8].repeat(4));
    assert_eq!(second, first);

    let mut words = vec![0; 4];
    gl.set_clear_color([0.0, 1.0, 0.0, 1.0]);
    gl.clear(gles::GL_COLOR_BUFFER_BIT);
    gl.frame_rgb565_words(2, 2, &mut words);
    gl.frame_rgb565(2, 2, &mut first);
    assert_eq!(words, vec![0x07e0; 4]);
    assert_eq!(first, [0xe0, 0x07].repeat(4));
}

#[test]
fn caller_reusing_an_output_buffer_does_not_change_the_renderer_frame() {
    let mut gl = GlState::new(2, 2);
    gl.set_clear_color([1.0, 1.0, 1.0, 1.0]);
    gl.clear(gles::GL_COLOR_BUFFER_BIT);
    let mut bytes = Vec::new();
    gl.frame_rgb565(2, 2, &mut bytes);
    bytes.fill(0xab);
    gl.frame_rgb565(2, 2, &mut bytes);
    assert_eq!(bytes, vec![0xff; 8]);
}

#[test]
fn equal_area_exports_still_use_the_requested_geometry() {
    let mut gl = GlState::new(2, 2);
    let original = [0x00, 0xf8, 0xe0, 0x07, 0x1f, 0x00, 0xff, 0xff];
    gl.import_rgb565_changes(2, 2, &[0; 8], &original);
    let mut bytes = Vec::new();
    gl.frame_rgb565(4, 1, &mut bytes);
    assert_eq!(bytes, [0x00, 0xf8, 0x00, 0xf8, 0xe0, 0x07, 0xe0, 0x07]);
    gl.frame_rgb565(2, 2, &mut bytes);
    assert_eq!(bytes, original);
}

#[test]
fn changing_source_sampling_geometry_invalidates_a_clean_export() {
    let original = [0x00, 0xf8, 0xe0, 0x07, 0x1f, 0x00, 0xff, 0xff];
    for change in [0, 1, 2] {
        let mut gl = GlState::new(2, 2);
        gl.import_rgb565_changes(2, 2, &[0; 8], &original);
        let mut bytes = Vec::new();
        gl.frame_rgb565(2, 2, &mut bytes);
        match change {
            0 => gl.set_surface(1, 1),
            1 => gl.set_viewport(0, 0, 1, 1),
            _ => gl.define_reducao(2),
        }
        gl.frame_rgb565(2, 2, &mut bytes);
        assert_eq!(bytes, [0x00, 0xf8].repeat(4), "mudança {change}");
    }
}

#[test]
fn restoring_a_clean_frame_invalidates_the_derived_conversion() {
    let mut gl = GlState::new(2, 2);
    let mut bytes = Vec::new();
    gl.set_clear_color([1.0, 0.0, 0.0, 1.0]);
    gl.clear(gles::GL_COLOR_BUFFER_BIT);
    gl.frame_rgb565(2, 2, &mut bytes);
    let mut sections = Secoes::nova();
    gl.grava(&mut sections);
    let saved = sections.fecha();
    gl.set_clear_color([0.0, 0.0, 1.0, 1.0]);
    gl.clear(gles::GL_COLOR_BUFFER_BIT);
    gl.frame_rgb565(2, 2, &mut bytes);
    gl.restaura(&Leitor::abre(&saved).unwrap()).unwrap();
    gl.frame_rgb565(2, 2, &mut bytes);
    assert_eq!(bytes, [0x00, 0xf8].repeat(4));
}
