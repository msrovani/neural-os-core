//! Degrau KIQ / MEC — GFX9–GFX10 PM4 + doorbell por gen (ADR-0049 P2/P3).
//! Nenhum offset KIQ neste módulo para path MES (GFX11+).

use crate::gpu::amd_discovery::AmdIpId;
use crate::gpu::compute_abi::vector_add_check;
use k_nano::dma::dma_alloc_coalesced;

/// PACKET3 header: type=3, opcode, count (dwords after header - 1? amdgpu: n = count)
fn packet3(op: u32, count: u32) -> u32 {
    (3u32 << 30) | ((op & 0xff) << 8) | (count & 0x3fff)
}

const PACKET3_NOP: u32 = 0x10;
const PACKET3_DISPATCH_DIRECT: u32 = 0x15;
const PACKET3_EVENT_WRITE_EOP: u32 = 0x47;
const FENCE_PAYLOAD: u32 = 0xA11D_u32;
const FENCE_SPINS: u32 = 150_000;

/// Doorbell index/offset por GC major — tabela Degrau (≠ BAR+0x1B0 genérico).
pub fn doorbell_offset(ip: &AmdIpId) -> u32 {
    match ip.gfx_major {
        9 => 0x0000_01A0,  // GFX9 KiQ-ish
        10 => 0x0000_01B0, // RDNA1/2 — histórico; ainda por-gen
        _ => 0x0000_0000,  // MES path não usa
    }
}

/// Emite DISPATCH_DIRECT + EVENT_WRITE_EOP estrutural; fence em sysmem.
/// Sem MEC FW + MQD reais → FenceTimeout esperado.
pub unsafe fn dispatch_vector_add_kiq(
    mmio: u64,
    ip: &AmdIpId,
    hsaco: &[u8],
    a: &[f32],
    b: &[f32],
    expect: &[f32],
) -> bool {
    if ip.has_mes || !ip.has_kiq {
        k_nano::slog_hal!("AMD", "KIQ", "recusado (use MES path) GC={}.{}", ip.gfx_major, ip.gfx_minor);
        return false;
    }
    if a.len() != b.len() || a.len() != expect.len() || a.is_empty() {
        return false;
    }
    let n = a.len();
    let stub = hsaco.starts_with(b"CPU_VECTOR_ADD_STUB");

    let Some(vecs) = dma_alloc_coalesced(4096) else {
        return false;
    };
    let Some(fence) = dma_alloc_coalesced(4096) else {
        return false;
    };
    let Some(ring) = dma_alloc_coalesced(4096) else {
        return false;
    };

    {
        let base = vecs.virt as *mut f32;
        for i in 0..n {
            core::ptr::write_volatile(base.add(i), a[i]);
            core::ptr::write_volatile(base.add(n + i), b[i]);
            core::ptr::write_volatile(base.add(2 * n + i), 0.0f32);
        }
    }
    core::ptr::write_volatile(fence.virt as *mut u32, 0);

    // PM4 batch na ring page (não kick real sem CP/MEC).
    let r = ring.virt as *mut u32;
    let mut i = 0usize;
    // NOP pad
    r.add(i).write_volatile(packet3(PACKET3_NOP, 0));
    i += 1;
    // DISPATCH_DIRECT: dim_x, dim_y, dim_z, initiator
    r.add(i).write_volatile(packet3(PACKET3_DISPATCH_DIRECT, 3));
    i += 1;
    r.add(i).write_volatile(1); // X
    i += 1;
    r.add(i).write_volatile(1);
    i += 1;
    r.add(i).write_volatile(1);
    i += 1;
    r.add(i).write_volatile(0); // initiator
    i += 1;
    // EVENT_WRITE_EOP → fence phys (simplificado; layout EOP real é maior)
    r.add(i).write_volatile(packet3(PACKET3_EVENT_WRITE_EOP, 4));
    i += 1;
    r.add(i).write_volatile(0); // event
    i += 1;
    r.add(i).write_volatile((fence.phys & 0xffff_ffff) as u32);
    i += 1;
    r.add(i).write_volatile((fence.phys >> 32) as u32);
    i += 1;
    r.add(i).write_volatile(FENCE_PAYLOAD);
    i += 1;
    r.add(i).write_volatile(0);
    let _pm4_dwords = i;

    let db = doorbell_offset(ip);
    if db != 0 {
        // Kick estrutural — sem MQD mapeado não move CP.
        core::ptr::write_volatile((mmio + db as u64) as *mut u32, 1);
    }

    k_nano::slog_hal!("AMD", "KIQ", "GC={}.{} pack={}B stub={} doorbell={:#x} pm4={}dw — poll fence",
        ip.gfx_major, ip.gfx_minor, hsaco.len(), stub, db, _pm4_dwords);

    let mut hit = false;
    for _ in 0..FENCE_SPINS {
        if core::ptr::read_volatile(fence.virt as *const u32) == FENCE_PAYLOAD {
            hit = true;
            break;
        }
        core::hint::spin_loop();
    }

    if !hit {
        k_nano::slog_hal!("AMD", "KIQ", "FenceTimeout (esperado sem MEC/MQD)");
        let _keep = (vecs, fence, ring);
        return false;
    }

    let mut got = [0.0f32; 64];
    if n > got.len() {
        let _keep = (vecs, fence, ring);
        return false;
    }
    let c = (vecs.virt as *const f32).add(2 * n);
    for i in 0..n {
        got[i] = core::ptr::read_volatile(c.add(i));
    }
    let pass = vector_add_check(&got[..n], expect, 1e-5);
    let _keep = (vecs, fence, ring);
    k_nano::slog_hal!("AMD", "KIQ", "fence OK golden={} (HSACO EU residual se mismatch)", pass);
    pass
}
