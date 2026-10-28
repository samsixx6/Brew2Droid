//! Contratos reais de GL, compartilhados pela suíte e pela bancada Android.
use std::sync::Arc;
use glow::HasContext;
use zeebx::video::{gpu::GpuState, gles, rasterizer::{Rasterizador, Vertex, UV_PADRAO}};

fn vertex(x: f32, y: f32, color: [f32; 4]) -> Vertex {
    Vertex { position: [x, y, 0.0, 1.0], color, uv: UV_PADRAO, uv1: UV_PADRAO,
        normal: [0.0, 0.0, 1.0], fog: 1.0 }
}

pub fn scope(gl: Arc<glow::Context>) {
    let mut r = GpuState::novo(16, 16, Some(gl.clone())).unwrap();
    r.clear(gles::GL_COLOR_BUFFER_BIT);
    r.comeca_volta();
    let red = [1.0, 0.0, 0.0, 1.0];
    r.draw(gles::GL_TRIANGLES, &[
        vertex(-1.0, -1.0, red), vertex(1.0, -1.0, red), vertex(0.0, 1.0, red)]);
    r.descarrega_o_desenho();
    assert_ne!(unsafe { gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING) }, 0,
        "a volta trocou o framebuffer entre lotes");
    Rasterizador::devolve_o_contexto(&r);
    assert_eq!(unsafe { gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING) }, 0,
        "o fim da volta deixou o framebuffer do jogo ligado");
    let pixels = r.read_rect(0, 0, 16, 16);
    assert!(pixels.iter().any(|p| p[0] == 255 && p[1] == 0 && p[2] == 0));

    // A janela pode deixar outros objetos e testes ligados antes de a próxima volta.
    unsafe {
        gl.use_program(None);
        gl.bind_vertex_array(None);
        gl.enable(glow::SCISSOR_TEST);
        gl.scissor(0, 0, 1, 1);
    }
    r.retoma_o_contexto();
    r.comeca_volta();
    r.set_color_mask([false, true, false, true]);
    r.set_clear_color([0.0, 0.0, 0.0, 1.0]);
    r.clear(gles::GL_COLOR_BUFFER_BIT);
    // Clear abre máscaras por fora do espelho. A mesma máscara do desenho deve ser
    // reaplicada mesmo dentro da volta, preservando só o verde do triângulo branco.
    let white = [1.0; 4];
    r.draw(gles::GL_TRIANGLES, &[
        vertex(-1.0, -1.0, white), vertex(1.0, -1.0, white), vertex(0.0, 1.0, white)]);
    r.descarrega_o_desenho();
    Rasterizador::devolve_o_contexto(&r);
    let pixels = r.read_rect(0, 0, 16, 16);
    assert!(pixels.iter().any(|p| p[1] == 255), "o segundo lote não desenhou");
    assert!(pixels.iter().all(|p| p[0] == 0 && p[2] == 0), "clear deixou máscaras fora do espelho");
    r.retoma_o_contexto();
    r.draw(gles::GL_TRIANGLES, &[
        vertex(-1.0, -1.0, white), vertex(1.0, -1.0, white), vertex(0.0, 1.0, white)]);
    r.descarrega_o_desenho();
    assert_eq!(unsafe { gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING) }, 0,
        "o desenho avulso herdou o agrupamento da volta anterior");
    assert_eq!(unsafe { gl.get_error() }, glow::NO_ERROR);
}

pub fn ring(gl: Arc<glow::Context>) {
    const COLORS: [[u8; 4]; 4] = [[255,0,0,255], [0,255,0,255], [0,0,255,255], [255,255,0,255]];
    let mut r = GpuState::novo(80, 4, Some(gl.clone())).unwrap();
    r.clear(gles::GL_COLOR_BUFFER_BIT);
    r.comeca_volta();
    for x in 0..80 {
        let left = x as f32 / 40.0 - 1.0;
        let right = (x + 1) as f32 / 40.0 - 1.0;
        let color = COLORS[x % 4].map(|n| n as f32 / 255.0);
        let quad = [(left,-1.0),(right,-1.0),(right,1.0),(left,-1.0),(right,1.0),(left,1.0)]
            .map(|(x,y)| vertex(x,y,color));
        // 144.000 vértices passam por um anel de 65.536: duas voltas, com os
        // primeiros desenhos ainda pendentes na placa quando o armazenamento muda.
        let vertices: Vec<_> = (0..300).flat_map(|_| quad).collect();
        r.draw(gles::GL_TRIANGLES, &vertices);
        r.descarrega_o_desenho();
    }
    Rasterizador::devolve_o_contexto(&r);
    let pixels = r.read_rect(0, 0, 80, 4);
    for (i, pixel) in pixels.iter().enumerate() {
        assert_eq!(*pixel, COLORS[(i % 80) % 4], "faixa {i}: upload sobrescreveu lote anterior");
    }
    assert_eq!(unsafe { gl.get_error() }, glow::NO_ERROR);
}
