//! GPU + Display co-existencia (#336).
//! iGPU (Intel) para display, dGPU (NVIDIA) para compute.
//! Time-sharing se apenas 1 GPU disponivel.

use alloc::string::String;
use crate::gpu::detect::GpuInfo;

#[derive(Debug)]
pub enum GpuAssignment {
    /// iGPU display, dGPU compute (ideal)
    IgpuDisplayDgpuCompute { display: usize, compute: usize },
    /// GPU unica faz tudo
    SingleGpu(usize),
    /// So CPU
    CpuOnly,
}

pub fn plan_assignment(gpus: &[GpuInfo]) -> GpuAssignment {
    let igpu = gpus.iter().position(|g| g.is_integrated && g.has_display_engine);
    let dgpu = gpus.iter().position(|g| !g.is_integrated && g.has_compute);

    match (igpu, dgpu) {
        (Some(d), Some(c)) if d != c => GpuAssignment::IgpuDisplayDgpuCompute { display: d, compute: c },
        (Some(i), _) => GpuAssignment::SingleGpu(i),
        (_, Some(c)) => GpuAssignment::SingleGpu(c),
        _ => GpuAssignment::CpuOnly,
    }

    // Nota: Na GTX 1050 + Intel HD 630, o plano ideal e:
    //   display -> Intel HD 630 (iGPU)
    //   compute -> GTX 1050 (dGPU, via NVIDIA PFIFO + PUSH_BUFFER)
}

pub fn assignment_status(assignment: &GpuAssignment, gpus: &[GpuInfo]) -> alloc::string::String {
    match assignment {
        GpuAssignment::IgpuDisplayDgpuCompute { display, compute } => {
            alloc::format!("[GPU-PLAN] Display: {} | Compute: {}",
                gpus.get(*display).map_or("?", |g| g.name),
                gpus.get(*compute).map_or("?", |g| g.name))
        }
        GpuAssignment::SingleGpu(i) => {
            alloc::format!("[GPU-PLAN] Unica: {}", gpus.get(*i).map_or("?", |g| g.name))
        }
        GpuAssignment::CpuOnly => String::from("[GPU-PLAN] CPU-only"),
    }
}
