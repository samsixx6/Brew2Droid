//! Executada uma vez no contexto real, antes de o egui criar qualquer estado GL.

use glow::HasContext;

pub unsafe fn triangle(gl: &glow::Context) -> Result<[u8; 4], String> {
    let mut shaders = Vec::new();
    let (mut program, mut texture, mut framebuffer, mut vao) = (None, None, None, None);
    let result = (|| unsafe {
        for (kind, source) in [
            (
                glow::VERTEX_SHADER,
                "#version 300 es\nconst vec2 p[3] = vec2[3](vec2(-1,-1),vec2(3,-1),vec2(-1,3));\nvoid main(){gl_Position=vec4(p[gl_VertexID],0,1);}",
            ),
            (
                glow::FRAGMENT_SHADER,
                "#version 300 es\nprecision highp float;\nout vec4 color;\nvoid main(){color=vec4(64.0/255.0,128.0/255.0,192.0/255.0,1);}",
            ),
        ] {
            let shader = gl.create_shader(kind)?;
            shaders.push(shader);
            gl.shader_source(shader, source);
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                return Err(format!(
                    "GLES probe shader: {}",
                    gl.get_shader_info_log(shader)
                ));
            }
        }
        let linked = gl.create_program()?;
        program = Some(linked);
        for &shader in &shaders {
            gl.attach_shader(linked, shader);
        }
        gl.link_program(linked);
        if !gl.get_program_link_status(linked) {
            return Err(format!(
                "GLES probe program: {}",
                gl.get_program_info_log(linked)
            ));
        }
        let target = gl.create_texture()?;
        texture = Some(target);
        gl.bind_texture(glow::TEXTURE_2D, texture);
        gl.tex_storage_2d(glow::TEXTURE_2D, 1, glow::RGBA8, 4, 4);
        let fbo = gl.create_framebuffer()?;
        framebuffer = Some(fbo);
        gl.bind_framebuffer(glow::FRAMEBUFFER, framebuffer);
        gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D,
            texture,
            0,
        );
        if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
            return Err("GLES probe framebuffer is incomplete".into());
        }
        vao = Some(gl.create_vertex_array()?);
        gl.bind_vertex_array(vao);
        gl.use_program(program);
        gl.viewport(0, 0, 4, 4);
        gl.clear_color(0.0, 0.0, 0.0, 0.0);
        gl.clear(glow::COLOR_BUFFER_BIT);
        gl.draw_arrays(glow::TRIANGLES, 0, 3);
        let mut rgba = [0; 4];
        gl.read_pixels(
            2,
            2,
            1,
            1,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelPackData::Slice(Some(&mut rgba)),
        );
        let error = gl.get_error();
        if error != glow::NO_ERROR {
            return Err(format!("GLES probe error: {error:#x}"));
        }
        if !rgba
            .into_iter()
            .zip([64u8, 128, 192, 255])
            .all(|(a, b)| a.abs_diff(b) <= 2)
        {
            return Err(format!("GLES triangle readback failed: {rgba:?}"));
        }
        Ok(rgba)
    })();
    // A leitura síncrona é só desta sonda de 1 pixel; não entra no caminho dos quadros do jogo.
    unsafe {
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        gl.bind_texture(glow::TEXTURE_2D, None);
        gl.bind_vertex_array(None);
        gl.use_program(None);
        if let Some(value) = vao {
            gl.delete_vertex_array(value);
        }
        if let Some(value) = framebuffer {
            gl.delete_framebuffer(value);
        }
        if let Some(value) = texture {
            gl.delete_texture(value);
        }
        if let Some(value) = program {
            gl.delete_program(value);
        }
        for shader in shaders {
            gl.delete_shader(shader);
        }
    }
    result
}
