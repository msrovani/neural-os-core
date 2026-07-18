//! Degrau Arc / Xe-HPG — COMPUTE_WALKER @ CCS (ADR-0050 P5).
//! Separado de Gen9: sem GPGPU_WALKER / MEDIA_VFE neste path.
//! Golden silício aberto; GuC CTB real residual.

use crate::gpu::detect::GpuInfo;
use crate::gpu::intel_guc::{self, GucStage};

/// Opcode COMPUTE_WALKER (Xe-HPG) — classificação 3D/Media; length Degrau.
/// Mesa/anv usa genxml; aqui header estrutural para batch futuro.
const COMPUTE_WALKER_OP: u32 = 0x2280;
const CFE_STATE_OP: u32 = 0x2200;

fn cmd(op: u32, dwords: u32) -> u32 {
    (op << 16) | dwords.saturating_sub(2)
}

/// Emite batch estrutural COMPUTE_WALKER (não submete sem ring CCS GuC).
pub fn build_compute_walker_batch(out: &mut [u32], groups_x: u32) -> usize {
    if out.len() < 20 {
        return 0;
    }
    let mut i = 0;
    out[i] = cmd(CFE_STATE_OP, 6);
    i += 1;
    out[i] = 0; // Scratch
    i += 1;
    out[i] = 0;
    i += 1;
    out[i] = 1; // MaxThreads hint
    i += 1;
    out[i] = 0;
    i += 1;
    out[i] = 0;
    i += 1;

    out[i] = cmd(COMPUTE_WALKER_OP, 12);
    i += 1;
    out[i] = 0;
    i += 1;
    out[i] = groups_x.max(1);
    i += 1;
    out[i] = 1;
    i += 1;
    out[i] = 1;
    i += 1;
    for _ in 0..7 {
        out[i] = 0;
        i += 1;
    }
    i
}

/// Canário Arc: exige dGPU + GuC Booted/Uploaded; walker golden incompleto.
pub unsafe fn try_vector_add_arc(
    gpu: &GpuInfo,
    zebin: &[u8],
    a: &[f32],
    b: &[f32],
    expect: &[f32],
) -> bool {
    let _ = (a, b, expect);
    if gpu.is_integrated {
        k_nano::slog_hal!("INTEL", "ARC", "recusado em iGPU (Gen9/Xe-LP ring)");
        return false;
    }
    if zebin.is_empty() {
        return false;
    }
    let guc = intel_guc::last_guc_report();
    let guc_ok = matches!(
        guc.map(|r| r.stage),
        Some(GucStage::Booted) | Some(GucStage::Uploaded) | Some(GucStage::BootTimeout)
    );
    let mut batch = [0u32; 32];
    let n = build_compute_walker_batch(&mut batch, 1);
    k_nano::slog_hal!("INTEL", "ARC", "XeHpgCcs: pack={}B walker_dwords={} guc={:?} guc_ok={} — golden incompleto",
        zebin.len(),
        n,
        guc.map(|r| r.stage),
        guc_ok);
    false
}
