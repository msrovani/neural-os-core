//! GPU + Display co-existência (#336 / ADR-0048–50).
//! Política: iGPU = display; dGPU = compute (NVIDIA > AMD > Arc).
//! Falha de compute nunca reseta DisplayOwner / BARs do display.

use alloc::string::String;
use crate::gpu::detect::{GpuInfo, GpuVendor};

#[derive(Debug, Clone, Copy)]
pub enum GpuAssignment {
    /// iGPU display, dGPU compute (ideal dual-GPU)
    IgpuDisplayDgpuCompute { display: usize, compute: usize },
    /// GPU unica — compute só se compute_candidate
    SingleGpu { index: usize, compute_ok: bool },
    /// So CPU
    CpuOnly,
}

impl GpuAssignment {
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

fn best_display_idx(gpus: &[GpuInfo]) -> Option<usize> {
    // iGPU com display engine primeiro; senão qualquer has_display.
    gpus.iter()
        .position(|g| g.is_integrated && g.has_display_engine)
        .or_else(|| gpus.iter().position(|g| g.has_display_engine))
}

fn best_compute_idx(gpus: &[GpuInfo], display: Option<usize>) -> Option<usize> {
    // 1) dGPU compute_candidate (nunca a mesma da display se for iGPU-only role)
    let dgpu = gpus
        .iter()
        .enumerate()
        .filter(|(i, g)| {
            !g.is_integrated
                && g.compute_candidate
                && display.map(|d| d != *i).unwrap_or(true)
        })
        .max_by_key(|(_, g)| (compute_rank(g), g.vram_size))
        .map(|(i, _)| i);
    if dgpu.is_some() {
        return dgpu;
    }
    // 2) Sem dGPU: iGPU Gen9 pode ser compute (lab HD 620) — UMA contend, último recurso
    gpus.iter()
        .enumerate()
        .filter(|(_, g)| g.compute_candidate)
        .max_by_key(|(_, g)| (compute_rank(g), g.vram_size))
        .map(|(i, _)| i)
}

pub fn plan_assignment(gpus: &[GpuInfo]) -> GpuAssignment {
    let display = best_display_idx(gpus);
    let compute = best_compute_idx(gpus, display);

    match (display, compute) {
        (Some(d), Some(c)) if d != c => {
            serial_plan_dual(gpus, d, c);
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

fn serial_plan_dual(gpus: &[GpuInfo], d: usize, c: usize) {
    let _ = (gpus, d, c);
    // log via assignment_status
}

pub fn assignment_status(assignment: &GpuAssignment, gpus: &[GpuInfo]) -> alloc::string::String {
    match assignment {
        GpuAssignment::IgpuDisplayDgpuCompute { display, compute } => {
            let dn = gpus.get(*display).map_or("?", |g| g.name);
            let cn = gpus.get(*compute).map_or("?", |g| g.name);
            let note = match gpus.get(*compute).map(|g| g.vendor) {
                Some(GpuVendor::Nvidia) => "NVIDIA dGPU compute",
                Some(GpuVendor::Amd) => "AMD dGPU compute",
                Some(GpuVendor::Intel) => "Intel Arc dGPU compute (iGPU display)",
                _ => "dGPU compute",
            };
            alloc::format!(
                "[GPU-PLAN] Display: {} | Compute: {} ({}; has_compute após canário)",
                dn, cn, note
            )
        }
        GpuAssignment::SingleGpu { index, compute_ok } => {
            let g = gpus.get(*index);
            let uma = g.map(|x| x.is_integrated).unwrap_or(false);
            alloc::format!(
                "[GPU-PLAN] Unica: {} compute_cand={}{}",
                g.map_or("?", |x| x.name),
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
