//! GPU Backend — conecta a GPU detectada com o pipeline de inferencia do Cortex.
//! Seleciona automaticamente: Intel ring > AMD PM4 > NVIDIA PFIFO > CPU fallback.

use crate::gpu::detect::{GpuInfo, GpuVendor};
use crate::gpu::intel::IntelRing;
use crate::tensor::Tensor;
use crate::serial_println;

pub enum GpuAccel {
    Intel(IntelRing),
    CpuOnly,
}

// Usar Mutex em vez de `static mut` para evitar UB de referencia a static mut
use spin::Mutex;
static CURRENT_BACKEND: Mutex<Option<GpuAccel>> = Mutex::new(None);

/// Inicializa o backend GPU baseado no hardware detectado
pub unsafe fn init_backend(gpus: &[GpuInfo]) {
    let pmoff = crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);

    for gpu in gpus {
        match gpu.vendor {
            GpuVendor::Intel => {
                if let Some(ring) = IntelRing::probe(gpu, pmoff) {
                    serial_println!("[GPU-BACKEND] Intel GPU ativo: {}", gpu.name);
                    *CURRENT_BACKEND.lock() = Some(GpuAccel::Intel(ring));
                    return; // prioridade 1: Intel ring
                }
            }
            GpuVendor::Amd => {
                serial_println!("[GPU-BACKEND] AMD detectado: {} (futuro)", gpu.name);
            }
            GpuVendor::Nvidia => {
                serial_println!("[GPU-BACKEND] NVIDIA detectado: {} (futuro)", gpu.name);
            }
            _ => {}
        }
    }
    serial_println!("[GPU-BACKEND] Sem GPU acelerada. Fallback CPU.");
    *CURRENT_BACKEND.lock() = Some(GpuAccel::CpuOnly);
}

/// Matmul acelerado por GPU (ou fallback CPU)
pub fn gpu_matmul(a: &Tensor, b: &Tensor) -> Option<Tensor> {
    let guard = CURRENT_BACKEND.lock();
    let result = match guard.as_ref() {
        Some(GpuAccel::Intel(_)) => None, // stub: Intel matmul retorna None → fallback CPU
        _ => None,
    };
    drop(guard);
    result.or_else(|| cpu_matmul(a, b))
}

fn cpu_matmul(a: &Tensor, b: &Tensor) -> Option<Tensor> {
    a.matmul(b)
}

/// Forward pass do modelo usando GPU (se disponivel)
pub fn gpu_forward(_model: &crate::cortex::TransformerModel, _tokens: &[u16]) -> Option<(Tensor, Tensor)> {
    None
}

/// Status do backend GPU para debug
pub fn gpu_status() -> &'static str {
    let guard = CURRENT_BACKEND.lock();
    match guard.as_ref() {
        Some(GpuAccel::Intel(_)) => "Intel iGPU ring buffer",
        Some(GpuAccel::CpuOnly) => "CPU fallback",
        None => "Nao inicializado",
    }
}
