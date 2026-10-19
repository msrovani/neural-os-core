//! Compute posture AIOS multi-vendor — Observe→Plan→Act→Verify→Remember.
//! Vendor-agnostic: NVIDIA/AMD/Intel; Ampere é um caso, não o produto.

use crate::gpu::backend;
use crate::gpu::bar_compute;
use crate::gpu::compute_abi::{BackendState, ComputeBackendKind, IsaTag};
use crate::gpu::detect::{GpuInfo, GpuVendor};
use crate::gpu::display_coex;
use crate::gpu::kernel_pack::{self, PackOp};
use crate::gpu::loader_vram;
use crate::gpu::vram_stream::{self, StreamStage};
use k_nano::slog_hal;

/// Degrau de residência / device planejado (não é Ready silício).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ComputePosture {
    Off = 0,
    MappedFull = 1,
    StreamsWindow = 2,
    DeviceReady = 3,
    DeviceAwait = 4,
    CpuLadder = 5,
}

impl ComputePosture {
    pub fn as_str(self) -> &'static str {
        match self {
            ComputePosture::Off => "off",
            ComputePosture::MappedFull => "mapped_full",
            ComputePosture::StreamsWindow => "streams_window",
            ComputePosture::DeviceReady => "device_ready",
            ComputePosture::DeviceAwait => "device_await",
            ComputePosture::CpuLadder => "cpu_ladder",
        }
    }
}

/// Por que o device path ainda não Ready (tipado — n/a ≠ 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DeviceAwaitReason {
    None = 0,
    NeedsAcr = 1,
    NeedsGsp = 2,
    NeedsPsp = 3,
    NeedsGuc = 4,
    NeedsSignedPack = 5,
    NeedsCanary = 6,
    NeedsFence = 7,
    Quarantined = 8,
    CpuOnlySandbox = 9,
    NoComputeGpu = 10,
}

impl DeviceAwaitReason {
    pub fn as_str(self) -> &'static str {
        match self {
            DeviceAwaitReason::None => "none",
            DeviceAwaitReason::NeedsAcr => "needs_acr",
            DeviceAwaitReason::NeedsGsp => "needs_gsp",
            DeviceAwaitReason::NeedsPsp => "needs_psp",
            DeviceAwaitReason::NeedsGuc => "needs_guc",
            DeviceAwaitReason::NeedsSignedPack => "needs_signed_pack",
            DeviceAwaitReason::NeedsCanary => "needs_canary",
            DeviceAwaitReason::NeedsFence => "needs_fence",
            DeviceAwaitReason::Quarantined => "quarantined",
            DeviceAwaitReason::CpuOnlySandbox => "cpu_only_sandbox",
            DeviceAwaitReason::NoComputeGpu => "no_compute_gpu",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ComputePostureSnap {
    pub posture: ComputePosture,
    pub await_reason: DeviceAwaitReason,
    pub vendor: GpuVendor,
    pub isa: IsaTag,
    pub backend_kind: ComputeBackendKind,
    pub aperture_mb: u64,
    pub need_mb: u64,
    pub pack_present: bool,
    pub pack_verified: bool,
    pub weights_uploaded: bool,
    pub ap_pollable: bool,
}

static LAST: spin::Mutex<Option<ComputePostureSnap>> = spin::Mutex::new(None);

pub fn last_posture() -> Option<ComputePostureSnap> {
    *LAST.lock()
}

/// Observe silício + modelo → Plan posture (sem Act de unlock pesado).
/// `gpus` vazio → usa `backend::compute_isa_tag` / vendor (pós-model load).
pub fn plan_posture(gpus: &[GpuInfo]) -> ComputePostureSnap {
    let (vendor, isa, backend_kind, has_compute_gpu) = if !gpus.is_empty() {
        let assign = display_coex::plan_assignment(gpus);
        match display_coex::compute_gpu(&assign, gpus) {
            Some(g) => (g.vendor, g.isa_tag, g.backend_kind, true),
            None => (GpuVendor::Unknown, IsaTag::None, ComputeBackendKind::CpuFallback, false),
        }
    } else if let Some(isa) = backend::compute_isa_tag() {
        let vendor = backend::compute_vendor().unwrap_or(GpuVendor::Unknown);
        let kind = match vendor {
            GpuVendor::Nvidia => match isa {
                IsaTag::Sm52 | IsaTag::Sm61 | IsaTag::Sm70 => ComputeBackendKind::LegacyAcr,
                _ => ComputeBackendKind::Gsp,
            },
            GpuVendor::Amd => ComputeBackendKind::KiQ,
            GpuVendor::Intel => ComputeBackendKind::Gen9Ring,
            _ => ComputeBackendKind::CpuFallback,
        };
        (vendor, isa, kind, true)
    } else {
        (GpuVendor::Unknown, IsaTag::None, ComputeBackendKind::CpuFallback, false)
    };

    let aperture = crate::gpu::vram::vram_status_aperture_bytes().unwrap_or(0);
    let aperture_mb = aperture / (1024 * 1024);
    let need_bytes = cortex::model::loaded_model_header()
        .and_then(|h| loader_vram::layers_total_bytes(&h))
        .unwrap_or(0);
    let need_mb = need_bytes / (1024 * 1024);

    let pack_present = if isa != IsaTag::None {
        kernel_pack::pack_present_on_fat(isa, PackOp::BitLinearW2A8)
    } else {
        false
    };
    let pack_verified = match vendor {
        GpuVendor::Unknown | GpuVendor::VirtIo => false,
        v => kernel_pack::find_active_pack(v, isa, PackOp::BitLinearW2A8)
            .map(|p| p.verified && !kernel_pack::is_cpu_stub_pack(&p))
            .unwrap_or(false),
    };

    let st = backend::compute_state();
    let stage = vram_stream::stage();
    let uploaded = bar_compute::weights_uploaded();
    let streaming = matches!(stage, StreamStage::StreamsW2a8);
    let ap_pollable = k_nano::smp::ap_pollable();

    let (posture, await_reason) = decide(
        has_compute_gpu,
        stage,
        aperture,
        need_bytes,
        uploaded,
        streaming,
        st,
        pack_verified,
        backend_kind,
    );

    let snap = ComputePostureSnap {
        posture,
        await_reason,
        vendor,
        isa,
        backend_kind,
        aperture_mb,
        need_mb,
        pack_present,
        pack_verified,
        weights_uploaded: uploaded,
        ap_pollable,
    };
    *LAST.lock() = Some(snap);
    snap
}

fn decide(
    has_compute_gpu: bool,
    stage: StreamStage,
    aperture: u64,
    need: u64,
    uploaded: bool,
    streaming: bool,
    st: BackendState,
    pack_verified: bool,
    backend_kind: ComputeBackendKind,
) -> (ComputePosture, DeviceAwaitReason) {
    if !has_compute_gpu {
        return (ComputePosture::CpuLadder, DeviceAwaitReason::NoComputeGpu);
    }
    if matches!(st, BackendState::CpuOnly) {
        // Sandbox / VirtIO — BAR pode existir; device nunca.
        if stage == StreamStage::Off || aperture == 0 {
            return (ComputePosture::CpuLadder, DeviceAwaitReason::CpuOnlySandbox);
        }
    }
    if matches!(st, BackendState::Quarantine) {
        // Device off; BAR ainda pode servir.
        if streaming {
            return (ComputePosture::StreamsWindow, DeviceAwaitReason::Quarantined);
        }
        if uploaded && need > 0 && need <= aperture {
            return (ComputePosture::MappedFull, DeviceAwaitReason::Quarantined);
        }
        if stage != StreamStage::Off && aperture > 0 && need > aperture {
            return (ComputePosture::StreamsWindow, DeviceAwaitReason::Quarantined);
        }
        return (ComputePosture::CpuLadder, DeviceAwaitReason::Quarantined);
    }
    if matches!(st, BackendState::Ready) && pack_verified {
        // Fence device ainda AWAITING na prática — posture DeviceReady só se
        // algum dia gpu_ternary servir; até lá DeviceAwait NeedsFence.
        return (
            ComputePosture::DeviceAwait,
            DeviceAwaitReason::NeedsFence,
        );
    }

    let reason = unlock_reason(backend_kind, pack_verified, st);
    if streaming || stage == StreamStage::StreamsW2a8 {
        return (ComputePosture::StreamsWindow, reason);
    }
    if stage == StreamStage::Off {
        return (ComputePosture::Off, reason);
    }
    // Mapped / ComputeDevice
    if uploaded && (need == 0 || need <= aperture) {
        (ComputePosture::MappedFull, reason)
    } else if need > aperture && aperture > 0 {
        (ComputePosture::StreamsWindow, reason)
    } else if uploaded {
        (ComputePosture::MappedFull, reason)
    } else {
        (ComputePosture::CpuLadder, reason)
    }
}

fn unlock_reason(
    kind: ComputeBackendKind,
    pack_verified: bool,
    st: BackendState,
) -> DeviceAwaitReason {
    if matches!(st, BackendState::Ready) && !pack_verified {
        return DeviceAwaitReason::NeedsSignedPack;
    }
    if !matches!(st, BackendState::Ready | BackendState::BringingUp) {
        match kind {
            ComputeBackendKind::LegacyAcr => DeviceAwaitReason::NeedsAcr,
            ComputeBackendKind::Gsp => DeviceAwaitReason::NeedsGsp,
            ComputeBackendKind::KiQ | ComputeBackendKind::Mes => DeviceAwaitReason::NeedsPsp,
            ComputeBackendKind::Gen9Ring | ComputeBackendKind::XeHpgCcs => {
                DeviceAwaitReason::NeedsGuc
            }
            _ => DeviceAwaitReason::NeedsCanary,
        }
    } else if !pack_verified {
        DeviceAwaitReason::NeedsSignedPack
    } else {
        DeviceAwaitReason::NeedsFence
    }
}

/// Act leve: slog + remember Tickv; unlock pesado permanece nos backends.
pub fn act_and_remember(snap: &ComputePostureSnap) {
    slog_hal!(
        "AIOS",
        "ok",
        "compute_posture={} await={} vendor={:?} isa={} backend={:?} aperture={}MB need={}MB pack={}/{} uploaded={} ap={}",
        snap.posture.as_str(),
        snap.await_reason.as_str(),
        snap.vendor,
        snap.isa.as_str(),
        snap.backend_kind,
        snap.aperture_mb,
        snap.need_mb,
        snap.pack_present as u8,
        snap.pack_verified as u8,
        snap.weights_uploaded as u8,
        snap.ap_pollable as u8
    );
    if k_nano::storage::tickv::is_ready() {
        let line = alloc::format!(
            "{}|{}|{}|{:?}",
            snap.posture.as_str(),
            snap.await_reason.as_str(),
            snap.isa.as_str(),
            snap.vendor
        );
        let _ = k_nano::storage::tickv::put_blob("hw/compute/posture", line.as_bytes());
    }
}

/// Ciclo completo pós-GPU init / pós-model load.
pub fn refresh(gpus: &[GpuInfo]) -> ComputePostureSnap {
    let snap = plan_posture(gpus);
    act_and_remember(&snap);
    snap
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn await_reason_gsp_for_ampere_backend() {
        let r = unlock_reason(
            ComputeBackendKind::Gsp,
            false,
            BackendState::Quarantine,
        );
        assert_eq!(r, DeviceAwaitReason::NeedsGsp);
    }

    #[test]
    fn await_reason_acr_pascal() {
        let r = unlock_reason(
            ComputeBackendKind::LegacyAcr,
            false,
            BackendState::Probed,
        );
        assert_eq!(r, DeviceAwaitReason::NeedsAcr);
    }

    #[test]
    fn streams_when_need_gt_aperture() {
        let (p, _) = decide(
            true,
            StreamStage::Mapped,
            256 * 1024 * 1024,
            700 * 1024 * 1024,
            false,
            false,
            BackendState::Quarantine,
            false,
            ComputeBackendKind::Gsp,
        );
        assert_eq!(p, ComputePosture::StreamsWindow);
    }
}
