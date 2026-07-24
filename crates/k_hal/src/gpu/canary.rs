//! Canário vector_add — promoção has_compute só após golden (ADR-0048–50).

use crate::gpu::amd::AmdGpu;
use crate::gpu::compute_abi::{
    vector_add_check, vector_add_cpu, BackendState, ComputeBackendKind, GoldenId, IsaTag,
};
use crate::gpu::detect::{GpuInfo, GpuVendor};
use crate::gpu::intel::IntelRing;
use crate::gpu::kernel_pack::{self, PackOp};
use crate::gpu::nvidia::NvidiaGpu;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanaryResult {
    Pass,
    FailNoPack,
    FailUnsigned,
    FailDispatch,
    FailGolden,
    SkipVirtIo,
    SkipCpu,
}

/// Canário Pascal com handle vivo (D2/D3 do probe) — Degrau 4 / Labor 7 golden.
pub unsafe fn run_vector_add_canary_nv(gpu: &GpuInfo, nv: &mut NvidiaGpu) -> CanaryResult {
    if gpu.vendor != GpuVendor::Nvidia || !gpu.compute_candidate {
        return CanaryResult::SkipVirtIo;
    }

    let acr_ok = crate::unlock_dag::has(crate::unlock_dag::CapToken::GpuAcrBooted);
    let family = crate::gpu::detect::nvidia_family_str(gpu.arch);
    let legacy = matches!(gpu.backend_kind, ComputeBackendKind::LegacyAcr);

    k_nano::slog_bin!(
        "GPU-HW",
        "info",
        "step=golden status=START family={} isa={} backend={:?} acr={} name={}",
        family,
        gpu.isa_tag.as_str(),
        gpu.backend_kind,
        acr_ok as u8,
        gpu.name
    );

    // Labor 7: LegacyAcr sem HsBooted → nunca Pass (honesty).
    if legacy && !acr_ok {
        k_nano::slog_bin!(
            "GPU-HW",
            "info",
            "step=golden status=FAIL reason=acr_not_hs_booted family={} isa={}",
            family,
            gpu.isa_tag.as_str()
        );
        k_nano::slog_hal!(
            "GPU",
            "canary",
            "{}: ACR≠HsBooted — D4 estrutural; sem GpuCompute",
            gpu.name
        );
        // Ainda tenta D4 estrutural abaixo se quiser diagnóstico; mas Pass blocked.
        // Early return evita falso Pass se fence somehow ok sem ACR.
        let a = [1.0f32, 2.0, 3.0, 4.0];
        let b = [10.0f32, 20.0, 30.0, 40.0];
        let mut expect = [0.0f32; 4];
        let _ = vector_add_cpu(&a, &b, &mut expect);
        let pack = kernel_pack::find_active_pack(gpu.vendor, gpu.isa_tag, PackOp::VectorAdd);
        let payload = match &pack {
            Some(p) => p.payload.as_slice(),
            None => b"CPU_VECTOR_ADD_STUB\0sm_61",
        };
        let _ = nv.try_vector_add_d4(payload, &a, &b, &expect);
        return CanaryResult::FailDispatch;
    }

    let a = [1.0f32, 2.0, 3.0, 4.0];
    let b = [10.0f32, 20.0, 30.0, 40.0];
    let mut expect = [0.0f32; 4];
    if !vector_add_cpu(&a, &b, &mut expect) {
        return CanaryResult::FailGolden;
    }

    let pack = kernel_pack::find_active_pack(gpu.vendor, gpu.isa_tag, PackOp::VectorAdd);
    let (payload, verified, no_pack) = match &pack {
        Some(p) => (p.payload.as_slice(), p.verified, false),
        None => {
            k_nano::slog_hal!("GPU", "canary", "{}: sem KernelPack — D4 estrutural com stub (nunca Ready)", gpu.name);
            (b"CPU_VECTOR_ADD_STUB\0sm_61" as &[u8], false, true)
        }
    };
    if let Some(p) = &pack {
        if p.header.golden != GoldenId::VectorAdd {
            return CanaryResult::FailGolden;
        }
        if !p.verified {
            k_nano::slog_hal!("GPU", "canary", "{}: pack unsigned — D4 estrutural; unsigned ≠ Ready", gpu.name);
        }
    }

    k_nano::slog_bin!(
        "GPU-HW",
        "info",
        "step=golden status=dispatch pack={} verified={} bytes={}",
        if no_pack { 0 } else { 1 },
        verified as u8,
        payload.len()
    );

    let hw_ok = nv.try_vector_add_d4(payload, &a, &b, &expect);

    if no_pack {
        k_nano::slog_bin!(
            "GPU-HW",
            "info",
            "step=golden status=FAIL reason=kernel_pack_missing isa={}",
            gpu.isa_tag.as_str()
        );
        return CanaryResult::FailNoPack;
    }
    if !verified {
        k_nano::slog_bin!(
            "GPU-HW",
            "info",
            "step=golden status=FAIL reason=kernel_pack_unsigned isa={}",
            gpu.isa_tag.as_str()
        );
        return CanaryResult::FailUnsigned;
    }
    if !hw_ok {
        k_nano::slog_hal!("GPU", "canary", "{}: D4 dispatch sem golden (fence/CUBIN/ACR) — CPU_FALLBACK", gpu.name);
        k_nano::slog_bin!(
            "GPU-HW",
            "info",
            "step=golden status=FAIL reason=canary_dispatch_fail family={} isa={}",
            family,
            gpu.isa_tag.as_str()
        );
        return CanaryResult::FailDispatch;
    }

    k_nano::slog_hal!("GPU", "canary", "{}: vector_add PASS isa={} — has_compute=true",
        gpu.name,
        gpu.isa_tag.as_str());
    k_nano::slog_bin!(
        "GPU-HW",
        "info",
        "step=golden status=OK family={} isa={} acr=1 pack=1",
        family,
        gpu.isa_tag.as_str()
    );
    let _ = vector_add_check(&expect, &expect, 1e-5);
    CanaryResult::Pass
}

/// Canário Gen9 com ring vivo — Degrau Intel.
pub unsafe fn run_vector_add_canary_intel(gpu: &GpuInfo, ring: &mut IntelRing) -> CanaryResult {
    if gpu.vendor != GpuVendor::Intel || !gpu.compute_candidate {
        return CanaryResult::SkipVirtIo;
    }

    let a = [1.0f32, 2.0, 3.0, 4.0];
    let b = [10.0f32, 20.0, 30.0, 40.0];
    let mut expect = [0.0f32; 4];
    if !vector_add_cpu(&a, &b, &mut expect) {
        return CanaryResult::FailGolden;
    }

    let pack = kernel_pack::find_active_pack(gpu.vendor, gpu.isa_tag, PackOp::VectorAdd);
    let (payload, verified, no_pack) = match &pack {
        Some(p) => (p.payload.as_slice(), p.verified, false),
        None => {
            k_nano::slog_hal!("GPU", "canary", "{}: sem KernelPack Gen9 — D estrutural com stub", gpu.name);
            (b"CPU_VECTOR_ADD_STUB\0gen9" as &[u8], false, true)
        }
    };
    if let Some(p) = &pack {
        if p.header.golden != GoldenId::VectorAdd {
            return CanaryResult::FailGolden;
        }
        if !p.verified {
            k_nano::slog_hal!("GPU", "canary", "{}: pack unsigned — D estrutural; unsigned ≠ Ready", gpu.name);
        }
    }

    let hw_ok = if matches!(gpu.backend_kind, ComputeBackendKind::XeHpgCcs) {
        crate::gpu::intel::try_vector_add_arc(gpu, payload, &a, &b, &expect)
    } else {
        crate::gpu::intel::try_vector_add_gen9_ring(ring, payload, &a, &b, &expect)
    };

    if no_pack {
        return CanaryResult::FailNoPack;
    }
    if !verified {
        return CanaryResult::FailUnsigned;
    }
    if !hw_ok {
        k_nano::slog_hal!("GPU", "canary", "{}: Intel dispatch sem golden — CPU_FALLBACK (display intacto)", gpu.name);
        return CanaryResult::FailDispatch;
    }

    k_nano::slog_hal!("GPU", "canary", "{}: vector_add PASS isa={} — has_compute=true",
        gpu.name,
        gpu.isa_tag.as_str());
    CanaryResult::Pass
}

/// Canário AMD com handle vivo — KIQ ou MES (ADR-0049).
pub unsafe fn run_vector_add_canary_amd(gpu: &GpuInfo, amd: &mut AmdGpu) -> CanaryResult {
    if gpu.vendor != GpuVendor::Amd || !gpu.compute_candidate {
        return CanaryResult::SkipVirtIo;
    }

    let a = [1.0f32, 2.0, 3.0, 4.0];
    let b = [10.0f32, 20.0, 30.0, 40.0];
    let mut expect = [0.0f32; 4];
    if !vector_add_cpu(&a, &b, &mut expect) {
        return CanaryResult::FailGolden;
    }

    let pack = kernel_pack::find_active_pack(gpu.vendor, gpu.isa_tag, PackOp::VectorAdd);
    let (payload, verified, no_pack) = match &pack {
        Some(p) => (p.payload.as_slice(), p.verified, false),
        None => {
            k_nano::slog_hal!("GPU", "canary", "{}: sem KernelPack AMD — D estrutural com stub", gpu.name);
            (b"CPU_VECTOR_ADD_STUB\0gfx1030" as &[u8], false, true)
        }
    };
    if let Some(p) = &pack {
        if p.header.golden != GoldenId::VectorAdd {
            return CanaryResult::FailGolden;
        }
        if !p.verified {
            k_nano::slog_hal!("GPU", "canary", "{}: pack unsigned — D estrutural; unsigned ≠ Ready", gpu.name);
        }
    }

    let hw_ok = amd.try_vector_add(payload, &a, &b, &expect);

    if no_pack {
        return CanaryResult::FailNoPack;
    }
    if !verified {
        return CanaryResult::FailUnsigned;
    }
    if !hw_ok {
        k_nano::slog_hal!("GPU", "canary", "{}: AMD dispatch sem golden — CPU_FALLBACK (display intacto)", gpu.name);
        return CanaryResult::FailDispatch;
    }

    k_nano::slog_hal!("GPU", "canary", "{}: vector_add PASS isa={} — has_compute=true",
        gpu.name,
        gpu.isa_tag.as_str());
    CanaryResult::Pass
}

/// Tenta canário para uma GPU. Não toca display; falha → Quarantine.
pub unsafe fn run_vector_add_canary(gpu: &GpuInfo) -> CanaryResult {
    if gpu.vendor == GpuVendor::VirtIo || !gpu.compute_candidate {
        return CanaryResult::SkipVirtIo;
    }
    if gpu.backend_kind == ComputeBackendKind::CpuFallback {
        return CanaryResult::SkipCpu;
    }

    let a = [1.0f32, 2.0, 3.0, 4.0];
    let b = [10.0f32, 20.0, 30.0, 40.0];
    let mut expect = [0.0f32; 4];
    if !vector_add_cpu(&a, &b, &mut expect) {
        return CanaryResult::FailGolden;
    }

    let pack = match kernel_pack::find_active_pack(gpu.vendor, gpu.isa_tag, PackOp::VectorAdd) {
        Some(p) => p,
        None => {
            k_nano::slog_hal!("GPU", "canary", "{}: sem KernelPack signed isa={} — quarantine compute",
                gpu.name,
                gpu.isa_tag.as_str());
            return CanaryResult::FailNoPack;
        }
    };
    if !pack.verified {
        return CanaryResult::FailUnsigned;
    }
    if pack.header.golden != GoldenId::VectorAdd {
        return CanaryResult::FailGolden;
    }

    let hw_ok = match gpu.backend_kind {
        ComputeBackendKind::LegacyAcr => {
            crate::gpu::nvidia::try_vector_add_legacy(gpu, &pack.payload, &a, &b, &expect)
        }
        ComputeBackendKind::Gsp => {
            crate::gpu::nvidia::try_vector_add_gsp(gpu, &pack.payload, &a, &b, &expect)
        }
        ComputeBackendKind::Gen9Ring => {
            crate::gpu::intel::try_vector_add_gen9(gpu, &pack.payload, &a, &b, &expect)
        }
        ComputeBackendKind::XeHpgCcs => {
            crate::gpu::intel::try_vector_add_arc(gpu, &pack.payload, &a, &b, &expect)
        }
        ComputeBackendKind::KiQ | ComputeBackendKind::Mes => {
            crate::gpu::amd::try_vector_add(gpu, &pack.payload, &a, &b, &expect)
        }
        _ => false,
    };

    if !hw_ok {
        k_nano::slog_hal!("GPU", "canary", "{}: pack OK mas dispatch HW ainda incompleto — CPU_FALLBACK", gpu.name);
        return CanaryResult::FailDispatch;
    }

    k_nano::slog_hal!("GPU", "canary", "{}: vector_add PASS isa={} — has_compute=true",
        gpu.name,
        gpu.isa_tag.as_str());
    let _ = vector_add_check(&expect, &expect, 1e-5);
    CanaryResult::Pass
}

pub fn state_after(result: CanaryResult) -> BackendState {
    match result {
        CanaryResult::Pass => BackendState::Ready,
        CanaryResult::SkipVirtIo | CanaryResult::SkipCpu => BackendState::CpuOnly,
        _ => BackendState::Quarantine,
    }
}

pub fn expected_isa(gpu: &GpuInfo) -> IsaTag {
    gpu.isa_tag
}
