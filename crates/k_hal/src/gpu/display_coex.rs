//! GPU + Display co-existência (#336 / ADR-0048–50 / IDEA #382).
//!
//! Dual-GPU (mesma máquina): **iGPU = display**, **dGPU = compute**.
//! Exemplos canônicos: Intel HD/UHD + NVIDIA Pascal; Intel iGPU + Arc;
//! AMD APU + dGPU. Falha de compute **nunca** reseta DisplayOwner / BARs
//! do display. Pack W2A8 / canary / QMD usam só o índice `compute`.

use alloc::string::String;
use crate::gpu::detect::{GpuInfo, GpuVendor};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuAssignment {
    /// iGPU display, dGPU compute (ideal dual-GPU na mesma máquina)
    IgpuDisplayDgpuCompute { display: usize, compute: usize },
    /// GPU unica — compute só se compute_candidate
    SingleGpu { index: usize, compute_ok: bool },
    /// So CPU
    CpuOnly,
}

impl GpuAssignment {
    pub fn is_dual(self) -> bool {
        matches!(self, GpuAssignment::IgpuDisplayDgpuCompute { .. })
    }

    pub fn display_index(&self) -> Option<usize> {
        match *self {
            GpuAssignment::IgpuDisplayDgpuCompute { display, .. } => Some(display),
            GpuAssignment::SingleGpu { index, .. } => Some(index),
            GpuAssignment::CpuOnly => None,
        }
    }

    pub fn compute_index(&self) -> Option<usize> {
        match *self {
            GpuAssignment::IgpuDisplayDgpuCompute { compute, .. } => Some(compute),
            GpuAssignment::SingleGpu { index, compute_ok } if compute_ok => Some(index),
            _ => None,
        }
    }
}

/// Rank: NVIDIA Pascal/Turing+ > AMD dGPU > Intel Arc > resto.
fn compute_rank(g: &GpuInfo) -> u32 {
    match g.vendor {
        GpuVendor::Nvidia => 300,
        GpuVendor::Amd if !g.is_integrated => 200,
        GpuVendor::Intel if !g.is_integrated => 150, // Arc / DG2
        GpuVendor::Intel => 50,                      // iGPU só se única
        _ => 0,
    }
}

/// dGPU em D3hot/D3cold não é candidato até wake (Observe honesto).
fn power_ok_for_compute(g: &GpuInfo) -> bool {
    // 0=D0, 1=D1, 2=D2, 3=D3hot, 4=D3cold (convenção pci_power_state)
    g.pci_dstate <= 2
}

fn best_display_idx(gpus: &[GpuInfo]) -> Option<usize> {
    // iGPU com display engine primeiro; senão qualquer has_display.
    gpus.iter()
        .position(|g| g.is_integrated && g.has_display_engine)
        .or_else(|| gpus.iter().position(|g| g.has_display_engine))
}

fn best_compute_idx(gpus: &[GpuInfo], display: Option<usize>) -> Option<usize> {
    // 1) dGPU compute_candidate em D0–D2, distinto do display
    let dgpu = gpus
        .iter()
        .enumerate()
        .filter(|(i, g)| {
            !g.is_integrated
                && g.compute_candidate
                && power_ok_for_compute(g)
                && display.map(|d| d != *i).unwrap_or(true)
        })
        .max_by_key(|(_, g)| (compute_rank(g), g.vram_size))
        .map(|(i, _)| i);
    if dgpu.is_some() {
        return dgpu;
    }
    // 2) dGPU em D3 — ainda escolhe (Act pode tentar wake); log via status
    let dgpu_asleep = gpus
        .iter()
        .enumerate()
        .filter(|(i, g)| {
            !g.is_integrated
                && g.compute_candidate
                && display.map(|d| d != *i).unwrap_or(true)
        })
        .max_by_key(|(_, g)| (compute_rank(g), g.vram_size))
        .map(|(i, _)| i);
    if dgpu_asleep.is_some() {
        return dgpu_asleep;
    }
    // 3) Sem dGPU: iGPU Gen9 pode ser compute (lab HD 620) — UMA contend
    gpus.iter()
        .enumerate()
        .filter(|(_, g)| g.compute_candidate && power_ok_for_compute(g))
        .max_by_key(|(_, g)| (compute_rank(g), g.vram_size))
        .map(|(i, _)| i)
}

/// Observe→Plan: escolhe DisplayOwner vs ComputeOwner (dual ou single).
pub fn plan_assignment(gpus: &[GpuInfo]) -> GpuAssignment {
    let display = best_display_idx(gpus);
    let compute = best_compute_idx(gpus, display);

    match (display, compute) {
        (Some(d), Some(c)) if d != c => {
            GpuAssignment::IgpuDisplayDgpuCompute {
                display: d,
                compute: c,
            }
        }
        (Some(i), Some(c)) if i == c => GpuAssignment::SingleGpu {
            index: i,
            compute_ok: gpus[i].compute_candidate,
        },
        (Some(i), None) => GpuAssignment::SingleGpu {
            index: i,
            compute_ok: false,
        },
        (None, Some(c)) => GpuAssignment::SingleGpu {
            index: c,
            compute_ok: true,
        },
        _ => GpuAssignment::CpuOnly,
    }
}

/// Resolve GPU de compute do plano (fonte única p/ pack/canary/W2A8).
pub fn compute_gpu<'a>(plan: &GpuAssignment, gpus: &'a [GpuInfo]) -> Option<&'a GpuInfo> {
    plan.compute_index().and_then(|i| gpus.get(i))
}

pub fn display_gpu<'a>(plan: &GpuAssignment, gpus: &'a [GpuInfo]) -> Option<&'a GpuInfo> {
    plan.display_index().and_then(|i| gpus.get(i))
}

pub fn assignment_status(assignment: &GpuAssignment, gpus: &[GpuInfo]) -> alloc::string::String {
    match assignment {
        GpuAssignment::IgpuDisplayDgpuCompute { display, compute } => {
            let dn = gpus.get(*display).map_or("?", |g| g.name);
            let cn = gpus.get(*compute).map_or("?", |g| g.name);
            let disa = gpus.get(*display).map(|g| g.isa_tag.as_str()).unwrap_or("?");
            let cisa = gpus.get(*compute).map(|g| g.isa_tag.as_str()).unwrap_or("?");
            let dstate = gpus.get(*compute).map(|g| g.pci_dstate).unwrap_or(0);
            let note = match gpus.get(*compute).map(|g| g.vendor) {
                Some(GpuVendor::Nvidia) => "NVIDIA dGPU compute",
                Some(GpuVendor::Amd) => "AMD dGPU compute",
                Some(GpuVendor::Intel) => "Intel Arc dGPU compute (iGPU display)",
                _ => "dGPU compute",
            };
            alloc::format!(
                "[GPU-PLAN] DUAL Display: {} (isa={}) | Compute: {} (isa={} D={}; {}; W2A8 packs → compute ISA)",
                dn, disa, cn, cisa, dstate, note
            )
        }
        GpuAssignment::SingleGpu { index, compute_ok } => {
            let g = gpus.get(*index);
            let uma = g.map(|x| x.is_integrated).unwrap_or(false);
            alloc::format!(
                "[GPU-PLAN] Unica: {} isa={} compute_cand={}{}",
                g.map_or("?", |x| x.name),
                g.map(|x| x.isa_tag.as_str()).unwrap_or("?"),
                compute_ok,
                if *compute_ok && uma {
                    " (UMA: display+AI contend — último recurso)"
                } else {
                    ""
                }
            )
        }
        GpuAssignment::CpuOnly => String::from("[GPU-PLAN] CPU-only"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::compute_abi::{ComputeBackendKind, IsaTag};
    use crate::gpu::detect::GpuArch;

    fn stub(
        vendor: GpuVendor,
        name: &'static str,
        integrated: bool,
        isa: IsaTag,
        vram: u64,
        dstate: u8,
    ) -> GpuInfo {
        GpuInfo {
            vendor,
            arch: match vendor {
                GpuVendor::Nvidia => GpuArch::NvidiaPascal,
                GpuVendor::Intel if integrated => GpuArch::IntelGen9,
                GpuVendor::Intel => GpuArch::IntelXe,
                GpuVendor::Amd if integrated => GpuArch::AmdRdna2,
                GpuVendor::Amd => GpuArch::AmdRdna2,
                _ => GpuArch::Unknown,
            },
            device_id: 0,
            bar0: 0x1000,
            bar2: 0,
            vram_size: vram,
            has_display_engine: true,
            has_compute: false,
            is_integrated: integrated,
            pci_bus: 0,
            pci_dev: 0,
            pci_fn: 0,
            pci_dstate: dstate,
            name,
            backend_kind: ComputeBackendKind::LegacyAcr,
            isa_tag: isa,
            compute_candidate: true,
        }
    }

    #[test]
    fn dual_igpu_intel_plus_nvidia_dgpu() {
        let gpus = [
            stub(GpuVendor::Intel, "HD 620", true, IsaTag::Gen9, 0, 0),
            stub(
                GpuVendor::Nvidia,
                "GTX 1050",
                false,
                IsaTag::Sm61,
                2 * 1024 * 1024 * 1024,
                0,
            ),
        ];
        let plan = plan_assignment(&gpus);
        assert!(plan.is_dual());
        assert_eq!(plan.display_index(), Some(0));
        assert_eq!(plan.compute_index(), Some(1));
        let cg = compute_gpu(&plan, &gpus).unwrap();
        assert_eq!(cg.isa_tag, IsaTag::Sm61);
        assert!(!cg.is_integrated);
        // Pack W2A8 NÃO deve usar Gen9 da iGPU
        assert_ne!(cg.isa_tag, IsaTag::Gen9);
    }

    #[test]
    fn dual_prefers_nvidia_over_arc_when_both_dgpu() {
        let gpus = [
            stub(GpuVendor::Intel, "UHD", true, IsaTag::Gen9, 0, 0),
            stub(GpuVendor::Intel, "Arc A370M", false, IsaTag::Dg2, 4 << 30, 0),
            stub(GpuVendor::Nvidia, "GTX 1050", false, IsaTag::Sm61, 2 << 30, 0),
        ];
        let plan = plan_assignment(&gpus);
        assert!(plan.is_dual());
        assert_eq!(compute_gpu(&plan, &gpus).unwrap().vendor, GpuVendor::Nvidia);
    }

    #[test]
    fn single_igpu_only_uma_fallback() {
        let gpus = [stub(GpuVendor::Intel, "HD 620", true, IsaTag::Gen9, 0, 0)];
        let plan = plan_assignment(&gpus);
        assert!(!plan.is_dual());
        assert!(matches!(
            plan,
            GpuAssignment::SingleGpu {
                index: 0,
                compute_ok: true
            }
        ));
    }

    #[test]
    fn dgpu_d3_still_selected_for_wake() {
        let gpus = [
            stub(GpuVendor::Intel, "UHD", true, IsaTag::Gen9, 0, 0),
            stub(GpuVendor::Nvidia, "GTX 1050", false, IsaTag::Sm61, 2 << 30, 3),
        ];
        let plan = plan_assignment(&gpus);
        assert!(plan.is_dual());
        assert_eq!(plan.compute_index(), Some(1));
        assert_eq!(gpus[1].pci_dstate, 3);
    }
}
