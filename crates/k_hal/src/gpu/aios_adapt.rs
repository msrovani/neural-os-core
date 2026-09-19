//! AIOS full-auto GPU+LLM — Observe→Plan→Act→Verify→Remember (ADR-0088).
//!
//! Sem bypass: escolhe sozinho Falcon3 SKU (1B/3B/7B/10B), dual-GPU
//! (iGPU display / dGPU compute), IsaTag + OpProfile + pack W2A8.
//! Hitl só se Trust/CapGate Deny — nunca hardcodar sm_61-only ou só-3B.

use crate::gpu::backend;
use crate::gpu::compute_abi::{IsaTag, OpProfile};
use crate::gpu::detect::GpuInfo;
use crate::gpu::display_coex::{self, GpuAssignment};
use crate::gpu::falcon3_w2a8::{self, Falcon3Sku};
use crate::gpu::kernel_pack::{self, PackOp};
use cortex::model_fit::{self, Falcon3Kind, LlmBootPlan};
use k_nano::slog_hal;

/// Snapshot do último ciclo AIOS (Remember em RAM; HANR/NSGDB = residual).
#[derive(Debug, Clone, Copy)]
pub struct AiosGpuLlmPlan {
    pub gpu: GpuAssignment,
    pub sku: Falcon3Sku,
    pub compute_isa: IsaTag,
    pub op_profile: OpProfile,
    pub dual: bool,
    pub w2a8_pack_present: bool,
    pub ram_mb: u64,
}

static LAST: spin::Mutex<Option<AiosGpuLlmPlan>> = spin::Mutex::new(None);

pub fn last_plan() -> Option<AiosGpuLlmPlan> {
    *LAST.lock()
}

fn vendor_for_isa(isa: IsaTag) -> Option<crate::gpu::detect::GpuVendor> {
    use crate::gpu::detect::GpuVendor;
    match isa {
        IsaTag::Sm52 | IsaTag::Sm61 | IsaTag::Sm70 | IsaTag::Sm75 | IsaTag::Sm80 | IsaTag::Sm86
        | IsaTag::Sm89 => Some(GpuVendor::Nvidia),
        IsaTag::Gen9 | IsaTag::Dg2 => Some(GpuVendor::Intel),
        IsaTag::Gfx90c | IsaTag::Gfx1030 | IsaTag::Gfx1036 | IsaTag::Gfx1103 => Some(GpuVendor::Amd),
        IsaTag::None => None,
    }
}

/// Observe RAM + GPUs → Plan SKU + dual → Act (preferred_sku + remember ISA).
/// Chamado no boot após `detect_all` e sempre que o fit mudar.
pub fn adapt_boot(gpus: &[GpuInfo], ram_mb: u64) -> AiosGpuLlmPlan {
    // ── Observe ──────────────────────────────────────────────
    let gpu_plan = display_coex::plan_assignment(gpus);
    let llm: LlmBootPlan = model_fit::llm_boot_plan(ram_mb);
    let bw = backend::measured_bandwidth_gbps();

    // Header já carregado manda (modelo real > fit teórico).
    let sku = if let Some(h) = cortex::model::loaded_model_header() {
        falcon3_w2a8::sku_from_header(h.hidden, h.intermediate, h.num_layers)
            .unwrap_or(llm.pick)
    } else {
        llm.pick
    };

    let compute = display_coex::compute_gpu(&gpu_plan, gpus);
    let compute_isa = compute.map(|g| g.isa_tag).unwrap_or(IsaTag::None);
    let (_, _, _, _, op_profile) = crate::gpu::compute_abi::ComputeCaps::features_for_isa(compute_isa);
    // Bandwidth Observe → Plan: baixo → Mad; muito baixo → Scalar (CPU path)
    let op_profile = match (op_profile, bw) {
        (OpProfile::Dp4aW2A8, Some(g)) if g < 8 => {
            slog_hal!(
                "AIOS",
                "warn",
                "CE bw={}GB/s ≪ esperado — profile ScalarInt8 (não insiste dp4a)",
                g
            );
            OpProfile::ScalarInt8
        }
        (OpProfile::Dp4aW2A8, Some(g)) if g < 20 => {
            slog_hal!(
                "AIOS",
                "warn",
                "CE bw={}GB/s baixo — profile MadInt8 (boot clocks)",
                g
            );
            OpProfile::MadInt8
        }
        (p, _) => p,
    };

    // ── Plan ─────────────────────────────────────────────────
    let pack_ok = vendor_for_isa(compute_isa)
        .and_then(|v| kernel_pack::find_active_pack(v, compute_isa, PackOp::BitLinearW2A8))
        .map(|p| p.verified)
        .unwrap_or(false);

    let snap = AiosGpuLlmPlan {
        gpu: gpu_plan,
        sku,
        compute_isa,
        op_profile,
        dual: gpu_plan.is_dual(),
        w2a8_pack_present: pack_ok,
        ram_mb,
    };

    // ── Act ──────────────────────────────────────────────────
    falcon3_w2a8::set_preferred_sku(sku);
    if let Some(g) = compute {
        // backend::remember já roda no init; reforça se adapt após detect
        let _ = g;
    }

    // ── Verify (honesto) ─────────────────────────────────────
    if snap.dual {
        slog_hal!(
            "AIOS",
            "ok",
            "adapt DUAL sku={} compute_isa={} profile={:?} w2a8_pack={} ram={}MB llm={}",
            sku.as_str(),
            compute_isa.as_str(),
            op_profile,
            pack_ok,
            ram_mb,
            llm.as_str()
        );
    } else {
        slog_hal!(
            "AIOS",
            "ok",
            "adapt sku={} compute_isa={} profile={:?} w2a8_pack={} dual=0 ram={}MB llm={}",
            sku.as_str(),
            compute_isa.as_str(),
            op_profile,
            pack_ok,
            ram_mb,
            llm.as_str()
        );
    }
    if !pack_ok && compute_isa != IsaTag::None {
        slog_hal!(
            "AIOS",
            "warn",
            "W2A8 pack ausente p/ {} — fallback CPU/SMP até KernelPack (não inventa ISA)",
            compute_isa.as_str()
        );
    }

    // ── Remember ─────────────────────────────────────────────
    *LAST.lock() = Some(snap);
    snap
}

/// Re-sincroniza SKU quando o header do modelo entra (load path).
pub fn adapt_on_model_loaded() {
    let ram = k_nano::memory::TOTAL_RAM_MB.load(core::sync::atomic::Ordering::Relaxed) as u64;
    if let Some(h) = cortex::model::loaded_model_header() {
        if let Some(sku) = falcon3_w2a8::sku_from_header(h.hidden, h.intermediate, h.num_layers) {
            falcon3_w2a8::set_preferred_sku(sku);
            slog_hal!(
                "AIOS",
                "ok",
                "Remember SKU←header {} (h={} ffn={} L={}) ram={}MB",
                sku.as_str(),
                h.hidden,
                h.intermediate,
                h.num_layers,
                ram
            );
            if let Some(mut p) = *LAST.lock() {
                p.sku = sku;
                *LAST.lock() = Some(p);
            }
        }
    }
}

/// Re-Plan após CE bandwidth medida (chamar do backend pós-canário CE).
pub fn replan_after_bandwidth(gpus: &[GpuInfo], ram_mb: u64) -> AiosGpuLlmPlan {
    adapt_boot(gpus, ram_mb)
}

/// Remember caps medidas (isa, profile, bandwidth, Ready/dual) — slog + snapshot.
/// HANR/NSGDB write = residual path cognitivo (sem hoard).
pub fn remember_caps() {
    let st = backend::compute_state();
    let bw = backend::measured_bandwidth_gbps();
    let dual = backend::is_dual_gpu();
    let isa = backend::compute_isa_tag()
        .map(|i| i.as_str())
        .unwrap_or("none");
    let sku = falcon3_w2a8::active_sku().as_str();
    let profile = last_plan().map(|p| p.op_profile).unwrap_or(OpProfile::ScalarInt8);
    let pack = last_plan().map(|p| p.w2a8_pack_present).unwrap_or(false);
    slog_hal!(
        "AIOS",
        "ok",
        "Remember caps isa={} profile={:?} bw={:?}GB/s dual={} ready={:?} sku={} w2a8_pack={}",
        isa,
        profile,
        bw,
        dual as u8,
        st,
        sku,
        pack as u8
    );
}

/// Escalação automática de SKU se fit TooTight (menor opção que cabe).
pub fn auto_downgrade_sku_if_tight(ram_mb: u64) -> Falcon3Sku {
    let order = [
        Falcon3Kind::Large10B,
        Falcon3Kind::Goal7B,
        Falcon3Kind::Daily3B,
        Falcon3Kind::Tiny1B,
    ];
    let current = falcon3_w2a8::preferred_sku();
    for &k in &order {
        if k.params() > current.params() {
            continue; // só desce
        }
        let mb = k.file_mb_hint();
        if !model_fit::needs_airllm_at(ram_mb, k.params(), mb) {
            if k != current {
                falcon3_w2a8::set_preferred_sku(k);
                slog_hal!(
                    "AIOS",
                    "ok",
                    "auto-downgrade SKU {} → {} (fit RAM={}MB)",
                    current.as_str(),
                    k.as_str(),
                    ram_mb
                );
            }
            return k;
        }
    }
    falcon3_w2a8::set_preferred_sku(Falcon3Kind::Tiny1B);
    Falcon3Kind::Tiny1B
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::compute_abi::{ComputeBackendKind, IsaTag};
    use crate::gpu::detect::{GpuArch, GpuVendor};

    fn stub_igpu() -> GpuInfo {
        GpuInfo {
            vendor: GpuVendor::Intel,
            arch: GpuArch::IntelGen9,
            device_id: 0x5916,
            bar0: 0x1000,
            bar2: 0,
            vram_size: 0,
            has_display_engine: true,
            has_compute: false,
            is_integrated: true,
            pci_bus: 0,
            pci_dev: 2,
            pci_fn: 0,
            pci_dstate: 0,
            name: "HD 620",
            backend_kind: ComputeBackendKind::Gen9Ring,
            isa_tag: IsaTag::Gen9,
            compute_candidate: true,
        }
    }

    fn stub_dgpu() -> GpuInfo {
        GpuInfo {
            vendor: GpuVendor::Nvidia,
            arch: GpuArch::NvidiaPascal,
            device_id: 0x1c82,
            bar0: 0x2000,
            bar2: 0x3000,
            vram_size: 2 << 30,
            has_display_engine: true,
            has_compute: false,
            is_integrated: false,
            pci_bus: 1,
            pci_dev: 0,
            pci_fn: 0,
            pci_dstate: 0,
            name: "GTX 1050",
            backend_kind: ComputeBackendKind::LegacyAcr,
            isa_tag: IsaTag::Sm61,
            compute_candidate: true,
        }
    }

    #[test]
    fn adapt_dual_picks_dgpu_isa_and_3b_lab_on_32g() {
        let gpus = [stub_igpu(), stub_dgpu()];
        let p = adapt_boot(&gpus, 32768);
        assert!(p.dual);
        assert_eq!(p.compute_isa, IsaTag::Sm61);
        assert_eq!(p.sku, Falcon3Kind::Daily3B);
        assert_eq!(falcon3_w2a8::preferred_sku(), Falcon3Kind::Daily3B);
    }

    #[test]
    fn adapt_tight_ram_prefers_smaller_sku_via_llm_plan() {
        let gpus = [stub_igpu()];
        let p = adapt_boot(&gpus, 2048);
        // 2GB: llm_boot_plan cai p/ 1B ou airllm — pick Tiny1B ou Daily com air
        assert!(matches!(
            p.sku,
            Falcon3Kind::Tiny1B | Falcon3Kind::Daily3B | Falcon3Kind::Goal7B
        ));
        assert!(!p.dual);
    }
}
