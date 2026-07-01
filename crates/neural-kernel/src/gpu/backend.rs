//! GPU Backend — conecta a GPU detectada com o pipeline de inferencia do Cortex.
//! Seleciona automaticamente: Intel ring > AMD PM4 > NVIDIA PFIFO > CPU fallback.

use crate::gpu::detect::{GpuInfo, GpuVendor, GpuArch};
use crate::gpu::intel::IntelRing;
use crate::tensor::Tensor;
use crate::serial_println;

pub enum GpuAccel {
    Intel(IntelRing),
    // Amd(Pm4Ring),  // futuro
    // Nvidia(PfifoFalcon),  // futuro
    CpuOnly,
}

static mut CURRENT_BACKEND: Option<GpuAccel> = None;

/// Inicializa o backend GPU baseado no hardware detectado
pub unsafe fn init_backend(gpus: &[GpuInfo]) {
    let pmoff = crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);

    for gpu in gpus {
        match gpu.vendor {
            GpuVendor::Intel => {
                if let Some(ring) = IntelRing::probe(gpu, pmoff) {
                    serial_println!("[GPU-BACKEND] Intel iGPU ativo: {}", gpu.name);
                    CURRENT_BACKEND = Some(GpuAccel::Intel(ring));
                    return;
                }
            }
            GpuVendor::Amd => {
                // AMD: futuro via PM4
                serial_println!("[GPU-BACKEND] AMD detectado: {} (futuro)", gpu.name);
            }
            GpuVendor::Nvidia => {
                // NVIDIA: futuro via PFIFO + FALCON
                serial_println!("[GPU-BACKEND] NVIDIA detectado: {} (futuro)", gpu.name);
            }
            _ => {}
        }
    }
    serial_println!("[GPU-BACKEND] Sem GPU acelerada. Fallback CPU.");
    CURRENT_BACKEND = Some(GpuAccel::CpuOnly);
}

/// Matmul acelerado por GPU (ou fallback CPU)
pub fn gpu_matmul(a: &Tensor, b: &Tensor) -> Option<Tensor> {
    let backend = unsafe { &CURRENT_BACKEND };
    match backend {
        Some(GpuAccel::Intel(ref ring)) => ring.gpu_matmul(a, b).or_else(|| cpu_matmul(a, b)),
        _ => cpu_matmul(a, b),
    }
}

fn cpu_matmul(a: &Tensor, b: &Tensor) -> Option<Tensor> {
    a.matmul(b) // fallback existente
}

/// Forward pass do modelo usando GPU (se disponivel)
pub fn gpu_forward(_model: &crate::cortex::TransformerModel, _tokens: &[u16]) -> Option<(Tensor, Tensor)> {
    // Stub: em producao, carregaria pesos na VRAM + executaria shaders
    None
}

/// Status do backend GPU para debug
pub fn gpu_status() -> &'static str {
    match unsafe { &CURRENT_BACKEND } {
        Some(GpuAccel::Intel(_)) => "Intel iGPU ring buffer",
        Some(GpuAccel::CpuOnly) => "CPU fallback",
        None => "Nao inicializado",
    }
}
