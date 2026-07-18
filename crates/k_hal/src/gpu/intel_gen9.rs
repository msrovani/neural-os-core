//! Degrau Intel Gen9 — GPGPU_WALKER (layout Mesa/i965) + MI_STORE fence (ADR-0050 P3).
//!
//! Opcode: GPGPU_WALKER = 0x7105; Gen8+ = 15 dwords.
//! Fence: MI_STORE_DWORD_IMM → sysmem.
//! Golden vector ainda exige EU kernel/zebin — sem isso FailDispatch honesto.
//! Em iGPU real: fence MI pode hit mesmo sem EU (RingAlive); golden separado.

use crate::gpu::compute_abi::vector_add_check;
use crate::gpu::detect::GpuInfo;
use crate::gpu::intel::{
    IntelRing, MI_BATCH_BUFFER_END, MI_NOOP, PIPELINE_SELECT,
};
use crate::gpu::intel_gtt::GgttPin;
use k_nano::dma::dma_alloc_coalesced;

const GPGPU_WALKER_OP: u32 = 0x7105;
const MEDIA_VFE_STATE: u32 = 0x7000;
const MEDIA_INTERFACE_DESCRIPTOR_LOAD: u32 = 0x7002;
/// i915: MI_STORE_DWORD_IMM_GEN4 = MI_INSTR(0x20, 2) → (0x20<<23)|2
const MI_STORE_DWORD_IMM: u32 = (0x20 << 23) | 2;
const PIPELINE_MEDIA: u32 = 0x1;
const FENCE_SPINS: u32 = 200_000;
const FENCE_PAYLOAD: u32 = 0xA11E_u32;

fn gpgpu_walker_header(dwords: u32) -> u32 {
    (GPGPU_WALKER_OP << 16) | (dwords.saturating_sub(2))
}

fn media_cmd(op: u32, dwords: u32) -> u32 {
    (op << 16) | (dwords.saturating_sub(2))
}

/// Só MI_STORE + BB_END — prova que o ring consome comandos (útil no lab iGPU).
pub unsafe fn probe_ring_alive(ring: &mut IntelRing, gtt: &mut GgttPin) -> bool {
    let Some(fence) = dma_alloc_coalesced(4096) else {
        return false;
    };
    core::ptr::write_volatile(fence.virt as *mut u32, 0);
    let _ = gtt.pin_sys(fence.phys, 1);
    ring.write(&[
        MI_STORE_DWORD_IMM,
        (fence.phys & 0xffff_ffff) as u32,
        (fence.phys >> 32) as u32,
        FENCE_PAYLOAD ^ 0x1111,
    ]);
    ring.write(&[MI_BATCH_BUFFER_END]);
    ring.submit();
    let mut hit = false;
    for _ in 0..FENCE_SPINS {
        if core::ptr::read_volatile(fence.virt as *const u32) == (FENCE_PAYLOAD ^ 0x1111) {
            hit = true;
            break;
        }
        if ring.wait_idle(10) {
            // HEAD moveu; fence pode ainda falhar se GGTT/phys errado
        }
        core::hint::spin_loop();
    }
    k_nano::slog_hal!("INTEL", "GEN9", "RingAlive={}", hit);
    let _keep = fence;
    hit
}

/// Emite walker Gen8+ (15 dwords) + store fence.
pub unsafe fn dispatch_vector_add_gen9(
    ring: &mut IntelRing,
    gtt: &mut GgttPin,
    zebin: &[u8],
    a: &[f32],
    b: &[f32],
    expect: &[f32],
) -> bool {
    if ring.gen > 11 {
        k_nano::slog_hal!("INTEL", "GEN9", "recusado gen={} (use GuC/Xe path)", ring.gen);
        return false;
    }
    if a.len() != b.len() || a.len() != expect.len() || a.is_empty() {
        return false;
    }
    let stub = zebin.starts_with(b"CPU_VECTOR_ADD_STUB");
    let n = a.len();

    let _ring_alive = probe_ring_alive(ring, gtt);

    let Some(vecs) = dma_alloc_coalesced(4096) else {
        return false;
    };
    let Some(fence) = dma_alloc_coalesced(4096) else {
        return false;
    };
    let Some(iface) = dma_alloc_coalesced(4096) else {
        return false;
    };

    {
        let base = vecs.virt as *mut u8;
        for i in 0..n {
            core::ptr::write_volatile((base as *mut f32).add(i), a[i]);
            core::ptr::write_volatile((base as *mut f32).add(n + i), b[i]);
            core::ptr::write_volatile((base as *mut f32).add(2 * n + i), 0.0f32);
        }
    }
    core::ptr::write_volatile(fence.virt as *mut u32, 0);
    core::ptr::write_bytes(iface.virt as *mut u8, 0, 256);

    let Some(_v_gtt) = gtt.pin_sys(vecs.phys, 1) else {
        return false;
    };
    let Some(fence_gtt) = gtt.pin_sys(fence.phys, 1) else {
        return false;
    };
    let Some(iface_gtt) = gtt.pin_sys(iface.phys, 1) else {
        return false;
    };

    let vfe = [
        media_cmd(MEDIA_VFE_STATE, 9),
        0,
        0,
        (1 << 16) | 16,
        0,
        0,
        0,
        0,
        0,
    ];
    ring.write(&vfe);

    let idl = [
        media_cmd(MEDIA_INTERFACE_DESCRIPTOR_LOAD, 4),
        1,
        (iface_gtt & 0xffff_ffff) as u32,
        (iface_gtt >> 32) as u32,
    ];
    ring.write(&idl);

    ring.write(&[PIPELINE_SELECT | PIPELINE_MEDIA]);

    let simd16 = 1u32 << 30;
    let walker = [
        gpgpu_walker_header(15),
        0,
        0,
        0,
        simd16 | 0,
        0,
        0,
        1,
        0,
        0,
        1,
        0,
        1,
        0xffff_ffff,
        0xffff_ffff,
    ];
    ring.write(&walker);

    ring.write(&[
        MI_STORE_DWORD_IMM,
        (fence.phys & 0xffff_ffff) as u32,
        (fence.phys >> 32) as u32,
        FENCE_PAYLOAD,
    ]);
    ring.write(&[MI_NOOP, MI_BATCH_BUFFER_END]);
    ring.submit();

    k_nano::slog_hal!("INTEL", "GEN9", "walker+store pack={}B fence_gtt={:#x} stub={}",
        zebin.len(),
        fence_gtt,
        stub);

    let mut hit = false;
    let mut ring_idle = false;
    for _ in 0..FENCE_SPINS {
        if core::ptr::read_volatile(fence.virt as *const u32) == FENCE_PAYLOAD {
            hit = true;
            break;
        }
        if ring.wait_idle(10) {
            ring_idle = true;
        }
        core::hint::spin_loop();
    }

    k_nano::slog_hal!("INTEL", "GEN9", "fence_hit={} ring_idle={} (golden exige EU/zebin)", hit, ring_idle);

    if !hit {
        let _keep = (vecs, fence, iface);
        return false;
    }

    let mut got = [0.0f32; 64];
    if n > got.len() {
        let _keep = (vecs, fence, iface);
        return false;
    }
    let c_ptr = (vecs.virt as *const f32).add(2 * n);
    for i in 0..n {
        got[i] = core::ptr::read_volatile(c_ptr.add(i));
    }
    let pass = vector_add_check(&got[..n], expect, 1e-5);
    let _keep = (vecs, fence, iface);
    if pass {
        k_nano::slog_hal!("INTEL", "GEN9", "GOLDEN PASS n={}", n);
        true
    } else {
        k_nano::slog_hal!("INTEL", "GEN9", "fence OK mas GoldenMismatch (sem EU kernel)");
        false
    }
}

pub unsafe fn try_vector_add_gen9_standalone(
    gpu: &GpuInfo,
    zebin: &[u8],
    _a: &[f32],
    _b: &[f32],
    _expect: &[f32],
) -> bool {
    if !gpu.is_integrated
        && matches!(
            gpu.arch,
            crate::gpu::detect::GpuArch::IntelXe | crate::gpu::detect::GpuArch::IntelXe2
        )
    {
        k_nano::slog_hal!("INTEL", "GEN9", "standalone recusado para Arc dGPU");
        return false;
    }
    k_nano::slog_hal!("INTEL", "GEN9", "standalone: precisa ring+gtt vivos (pack {}B)", zebin.len());
    false
}
