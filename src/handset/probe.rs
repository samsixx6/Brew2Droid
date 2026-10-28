//! A sonda comprova execução nativa sem confundir isso com compatibilidade de um jogo.

use std::path::Path;

use crate::cpu::mem::GuestMemory;
use crate::cpu::{BackendPadrao, CpuBackend, CpuError, RETURN_MAGIC, Reg, StopReason};

#[derive(Debug)]
pub struct CpuProbeReport {
    pub arm_result: u32,
    pub thumb_result: u32,
    pub unmapped_fault_addr: u32,
    pub memory_protection_passed: bool,
    pub instructions: u64,
}

fn execute(code: &[u8], thumb: bool, r1: u32) -> Result<(StopReason, u32, u64), CpuError> {
    let mut memory = GuestMemory::new();
    let mut page = vec![0; 4096];
    page[..code.len()].copy_from_slice(code);
    memory
        .map("handset-probe", 0x1000, page, true)
        .map_err(|error| CpuError(error.to_string()))?;
    let mut cpu = BackendPadrao::new()?;
    cpu.reset(&memory)?;
    cpu.write_reg(Reg::Lr, RETURN_MAGIC);
    cpu.write_reg(Reg::R1, r1);
    let outcome = cpu.run(if thumb { 0x1001 } else { 0x1000 }, 256)?;
    Ok((outcome, cpu.read_reg(Reg::R0), cpu.instructions()))
}

fn verify_memory_protection() -> Result<u64, CpuError> {
    // O mesmo código roda na sonda host e no APK: confirma o emissor real do aparelho.
    let mut page = vec![0; 4096];
    for (index, word) in [0xe591_0000u32, 0xe581_2000, 0xe12f_ff1e].into_iter().enumerate() {
        page[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    page[0x100..0x104].copy_from_slice(&0xe3a0_3001u32.to_le_bytes()); // mov r3,#1
    page[0x104..0x108].copy_from_slice(&0xe12f_ff1eu32.to_le_bytes());
    let mut memory = GuestMemory::new();
    memory.map("protected-probe", 0x1000, page, true).map_err(|e| CpuError(e.to_string()))?;
    let mut ro = vec![0; 4096];
    ro[..4].copy_from_slice(&123u32.to_le_bytes());
    memory.map("readonly-probe", 0x4000, ro, false).map_err(|e| CpuError(e.to_string()))?;
    memory.map_zeroed("watch-probe", 0x6000, 4096).map_err(|e| CpuError(e.to_string()))?;
    // Dois bytes da página seguinte existem, mas a leitura de quatro bytes cruza o fim real.
    memory.map_zeroed("partial-probe", 0x8000, 4098).map_err(|e| CpuError(e.to_string()))?;
    let mut cpu = BackendPadrao::new()?;
    cpu.reset(&memory)?;
    cpu.write_reg(Reg::Lr, RETURN_MAGIC);
    if cpu.run(0x1100, 256)? != StopReason::Returned || cpu.read_reg(Reg::R3) != 1 {
        return Err(CpuError("Memory-protection probe could not compile its test routine".into()));
    }
    cpu.watch_dirty(7, 0x1100, 4)?;
    cpu.take_dirty(7);
    cpu.liga_contagem_de_memoria(true);
    cpu.write_reg(Reg::R1, 0x1100);
    cpu.write_reg(Reg::R2, 0xe3a0_3002); // mov r3,#2, escrito pelo guest
    if cpu.run(0x1000, 256)? != StopReason::Returned
        || cpu.read_reg(Reg::R0) != 0xe3a0_3001 || !cpu.take_dirty(7)
        || cpu.run(0x1100, 256)? != StopReason::Returned || cpu.read_reg(Reg::R3) != 2
        || cpu.relato_de_memoria().is_some_and(|c| c.leituras != 0 || c.escritas != 1 || c.escritas_em_codigo != 1) {
        return Err(CpuError("Protected code read/write, dirty watch or JIT invalidation failed".into()));
    }
    cpu.liga_contagem_de_memoria(true);
    cpu.write_reg(Reg::R1, 0x4000);
    if cpu.run(0x1000, 256)? != (StopReason::MemoryFault { addr: 0x4000, pc: 0x1004 })
        || cpu.read_reg(Reg::R0) != 123 || cpu.read_u32(0x4000)? != 123
        || cpu.relato_de_memoria().is_some_and(|c| c.leituras != 0 || c.escritas != 1) {
        return Err(CpuError("Read-only memory probe did not preserve the precise write fault".into()));
    }
    cpu.write_reg(Reg::R1, 0x8fff);
    if cpu.run(0x1000, 256)? != (StopReason::MemoryFault { addr: 0x8fff, pc: 0x1000 }) {
        return Err(CpuError("Cross-page read probe bypassed the real mapped boundary".into()));
    }
    // Esta página nunca foi executada: a vigia deve proteger a escrita por conta própria.
    cpu.watch_dirty(8, 0x6000, 4)?;
    cpu.take_dirty(8);
    cpu.liga_contagem_de_memoria(true);
    cpu.write_reg(Reg::R1, 0x6000);
    if cpu.run(0x1000, 256)? != StopReason::Returned || !cpu.take_dirty(8)
        || cpu.run(0x1000, 256)? != StopReason::Returned || !cpu.take_dirty(8)
        || cpu.relato_de_memoria().is_some_and(|c| c.leituras != 0 || c.escritas != 2) {
        return Err(CpuError("Clean data watch did not intercept and rearm guest writes".into()));
    }
    let mut instructions = cpu.instructions();

    let mut ordered_memory = GuestMemory::new();
    let lda = [0xe191_0c9fu32, 0xe12f_ff1e].into_iter().flat_map(u32::to_le_bytes).collect();
    ordered_memory.map("lda-probe", 0x10000, lda, true).map_err(|e| CpuError(e.to_string()))?;
    let mut misaligned = vec![0; 0x2000];
    misaligned[0xffe..0x1002].copy_from_slice(&123u32.to_le_bytes());
    ordered_memory.map("misaligned-ro", 0x1002, misaligned, false).map_err(|e| CpuError(e.to_string()))?;
    let mut aligned = vec![0; 4096];
    aligned[2..6].copy_from_slice(&456u32.to_le_bytes());
    ordered_memory.map("aligned-ro", 0x4000, aligned, false).map_err(|e| CpuError(e.to_string()))?;
    cpu.reset(&ordered_memory)?;
    cpu.liga_contagem_de_memoria(true);
    cpu.write_reg(Reg::R1, 0x2000);
    cpu.write_reg(Reg::Lr, RETURN_MAGIC);
    if cpu.run(0x10000, 256)? != StopReason::Returned || cpu.read_reg(Reg::R0) != 123
        || cpu.relato_de_memoria().is_some_and(|c| c.leituras != 1) {
        return Err(CpuError("Ordered read did not handle the misaligned host page mapping".into()));
    }
    cpu.liga_contagem_de_memoria(true);
    cpu.write_reg(Reg::R1, 0x4002);
    if cpu.run(0x10000, 256)? != StopReason::Returned || cpu.read_reg(Reg::R0) != 456
        || (cfg!(target_arch = "aarch64") && cpu.relato_de_memoria().is_some_and(|c| c.leituras != 1)) {
        return Err(CpuError("Ordered guest-misaligned read did not use the safe ARM64 fallback".into()));
    }
    instructions += cpu.instructions();

    // O primeiro desvio para a callback fica além dos 32 KiB aceitos por TB(N)Z.
    let mut long_memory = GuestMemory::new();
    let words = std::iter::repeat_n(0xe581_0000u32, 2048).chain([0xe12f_ff1e]);
    long_memory.map("long-store-probe", 0x10000, words.flat_map(u32::to_le_bytes).collect(), true)
        .map_err(|e| CpuError(e.to_string()))?;
    long_memory.map_zeroed("long-store-data", 0x4000, 4096).map_err(|e| CpuError(e.to_string()))?;
    cpu.reset(&long_memory)?;
    cpu.watch_dirty(8, 0x4000, 4)?;
    cpu.take_dirty(8);
    cpu.liga_contagem_de_memoria(true);
    cpu.write_reg(Reg::R0, 123);
    cpu.write_reg(Reg::R1, 0x4000);
    cpu.write_reg(Reg::Lr, RETURN_MAGIC);
    if cpu.run(0x10000, 4096)? != StopReason::Returned || cpu.read_u32(0x4000)? != 123 || !cpu.take_dirty(8)
        || cpu.relato_de_memoria().is_some_and(|c| c.escritas != 1) {
        return Err(CpuError("Long store block did not preserve its deferred write fallback".into()));
    }
    cpu.liga_contagem_de_memoria(false);
    Ok(instructions + cpu.instructions())
}

pub fn run_cpu_probe() -> Result<CpuProbeReport, CpuError> {
    // Instruções literais: MOV r0,#42; BX lr. Não há interpretador de teste no caminho.
    let arm = [0xe3a0_002au32.to_le_bytes(), 0xe12f_ff1eu32.to_le_bytes()].concat();
    let (arm_stop, arm_result, arm_instructions) = execute(&arm, false, 0)?;
    if arm_stop != StopReason::Returned || arm_result != 42 {
        return Err(CpuError(format!(
            "ARM probe failed: {arm_stop:?}, r0={arm_result}"
        )));
    }
    // Thumb: LSLS r0,r1,#2; BX lr. A troca de estado também faz parte da prova.
    let thumb = [0x0088u16.to_le_bytes(), 0x4770u16.to_le_bytes()].concat();
    let (thumb_stop, thumb_result, thumb_instructions) = execute(&thumb, true, 2)?;
    if thumb_stop != StopReason::Returned || thumb_result != 8 {
        return Err(CpuError(format!(
            "Thumb probe failed: {thumb_stop:?}, r0={thumb_result}"
        )));
    }
    let fault = [0xe591_0000u32.to_le_bytes(), 0xe12f_ff1eu32.to_le_bytes()].concat();
    let (fault_stop, _, fault_instructions) = execute(&fault, false, 0x3000)?;
    let StopReason::MemoryFault { addr: 0x3000, .. } = fault_stop else {
        return Err(CpuError(format!(
            "Unmapped-memory probe failed: {fault_stop:?}"
        )));
    };
    let protection_instructions = verify_memory_protection()?;
    Ok(CpuProbeReport {
        arm_result,
        thumb_result,
        unmapped_fault_addr: 0x3000,
        memory_protection_passed: true,
        instructions: arm_instructions + thumb_instructions + fault_instructions + protection_instructions,
    })
}

pub fn write_capability_report(
    path: &Path,
    native_page_size: usize,
    vendor: &str,
    renderer: &str,
    version: &str,
    triangle_rgba: [u8; 4],
    cpu: &CpuProbeReport,
) -> std::io::Result<()> {
    let triangle_passed = triangle_rgba
        .into_iter()
        .zip([64u8, 128, 192, 255])
        .all(|(actual, expected)| actual.abs_diff(expected) <= 2);
    let report = serde_json::json!({
        "schema_version": 1,
        "host_arch": std::env::consts::ARCH,
        "cpu_backend": if cfg!(any(target_arch = "wasm32", target_os = "ios")) { "interpreter" } else { "dynarmic" },
        "native_page_size_bytes": native_page_size,
        "graphics": {
            "vendor": vendor, "renderer": renderer, "version": version,
            "triangle_readback_rgba": triangle_rgba, "triangle_probe_passed": triangle_passed,
        },
        "cpu_probe": {
            "arm_result": cpu.arm_result,
            "thumb_result": cpu.thumb_result,
            "unmapped_fault_addr": cpu.unmapped_fault_addr,
            "memory_protection_passed": cpu.memory_protection_passed,
            "instructions": cpu.instructions,
        },
        "game_compatibility_tested": false,
    });
    let bytes = serde_json::to_vec_pretty(&report).map_err(std::io::Error::other)?;
    std::fs::write(path, bytes)
}
