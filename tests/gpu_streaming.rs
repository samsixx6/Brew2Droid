#![cfg(feature = "gpu")]
#[path = "support/gpu_streaming.rs"]
mod contract;

#[test]
fn o_contexto_volta_ao_frontend_so_no_fim_da_volta() {
    let Ok(context) = zeebx::video::contexto::Contexto::novo() else { return; };
    contract::scope(context.gl.clone());
}

#[test]
fn o_anel_conserva_lotes_apos_duas_voltas() {
    let Ok(context) = zeebx::video::contexto::Contexto::novo() else { return; };
    contract::ring(context.gl.clone());
}
