//! Degrau MES — GFX11+ (RDNA3/4). Sem offsets KIQ (ADR-0049 P5).
//! uni_mes / MES_KIQ residual; batch estrutural separado.

use crate::gpu::amd_discovery::AmdIpId;
use crate::gpu::firmware;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MesStage {
    BlobsMissing,
    PresentNotLoaded,
    LoadTimeout,
}

/// Doorbell MES — distinto de KIQ (tabela Degrau).
pub fn mes_doorbell_offset(ip: &AmdIpId) -> u32 {
    match ip.gfx_major {
        11 => 0x0000_00A0,
        12 => 0x0000_00B0,
        _ => 0,
    }
}

pub fn check_mes_blobs(ip: &AmdIpId) -> MesStage {
    if ip.gfx_major < 11 {
        return MesStage::BlobsMissing;
    }
    let names = [
        "gc_11_5_0_mes_2.bin",
        "gc_11_5_0_mes1.bin",
        "gc_11_5_0_mec.bin",
    ];
    let mut any = false;
    for n in &names {
        if firmware::has_named_blob(n) || firmware::load_firmware_file(n).is_some() {
            any = true;
            break;
        }
    }
    if any {
        MesStage::PresentNotLoaded
    } else {
        MesStage::BlobsMissing
    }
}

/// Canário MES: nunca usa amd_kiq::doorbell_offset.
pub unsafe fn dispatch_vector_add_mes(
    mmio: u64,
    ip: &AmdIpId,
    hsaco: &[u8],
    _a: &[f32],
    _b: &[f32],
    _expect: &[f32],
) -> bool {
    if !ip.has_mes || ip.gfx_major < 11 {
        k_nano::slog_hal!("AMD", "MES", "recusado GC={}.{} (use KIQ)", ip.gfx_major, ip.gfx_minor);
        return false;
    }
    let stage = check_mes_blobs(ip);
    let db = mes_doorbell_offset(ip);
    k_nano::slog_hal!("AMD", "MES", "GC={}.{} pack={}B mes_fw={:?} doorbell={:#x} — golden incompleto (uni_mes residual)",
        ip.gfx_major,
        ip.gfx_minor,
        hsaco.len(),
        stage,
        db);
    if db != 0 {
        // Não escrever doorbell MES sem ring/MQD — só log.
        let _ = mmio;
    }
    false
}
