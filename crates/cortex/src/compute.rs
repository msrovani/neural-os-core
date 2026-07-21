//! ADR-0057 WS-C: camada de dispatch de compute (ComputeBackend).
//!
//! Choke point único de roteamento do matmul da LLM. Ordem de fallback honesta:
//! `NPU → GPU → CPU-SMP (P-cores) → AVX2 → scalar`. Cada camada só entra se o
//! seu gate passou; nada é "fingido".
//!
//! GPU (`k_hal`) e NPU (`k_ai`) registram-se por fn-pointer porque dependem de
//! `cortex` (evita ciclo de dependência: `k_nano ← cortex ← {k_hal,k_ai}`).
//! Enquanto nenhum backend real registra (ex.: QEMU sem GPU/NPU), o dispatch
//! cai direto no caminho CPU/SMP.

use crate::tensor::{PackedTernaryTensor, Tensor};
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// Assinatura de um backend de matmul ternário (BitNet).
pub type TernaryFn = fn(&PackedTernaryTensor, &Tensor) -> Option<Tensor>;

// Slots de registro (0 = não registrado). fn-pointer cabe em usize no alvo.
static GPU_TERNARY: AtomicUsize = AtomicUsize::new(0);
static NPU_TERNARY: AtomicUsize = AtomicUsize::new(0);

// Telemetria (ADR-0057): quantas ops cada anel tratou.
static N_NPU: AtomicU64 = AtomicU64::new(0);
static N_GPU: AtomicU64 = AtomicU64::new(0);
static N_SMP: AtomicU64 = AtomicU64::new(0);
static N_CPU: AtomicU64 = AtomicU64::new(0);

/// Ring 0 (intent/router) — registrado por `k_ai` quando uma NPU fica pronta.
pub fn register_npu_ternary(f: TernaryFn) {
    NPU_TERNARY.store(f as usize, Ordering::Release);
    k_nano::slog_nano!("COMPUTE", "info", "NPU ternary backend registrado (Ring0)");
}

/// Ring 1 (matmul pesado) — registrado por `k_hal` quando o canário GPU passa.
pub fn register_gpu_ternary(f: TernaryFn) {
    GPU_TERNARY.store(f as usize, Ordering::Release);
    k_nano::slog_nano!("COMPUTE", "info", "GPU ternary backend registrado (Ring1)");
}

#[inline]
fn call_slot(slot: usize, w: &PackedTernaryTensor, x: &Tensor) -> Option<Tensor> {
    if slot == 0 {
        return None;
    }
    // Safety: só armazenamos fn-pointers válidos via register_*.
    let f: TernaryFn = unsafe { core::mem::transmute::<usize, TernaryFn>(slot) };
    f(w, x)
}

/// Roteia um matmul ternário. `Some` = tratado por acelerador/paralelo;
/// `None` = chamador segue no caminho AVX2/scalar existente.
pub fn dispatch_ternary(w: &PackedTernaryTensor, x: &Tensor) -> Option<Tensor> {
    let (k, n) = w.shape;
    let big = n >= 64 && k >= 64;

    // Ring 0 — NPU (router/intent, latência-crítico). Só se registrado.
    if let Some(r) = call_slot(NPU_TERNARY.load(Ordering::Acquire), w, x) {
        N_NPU.fetch_add(1, Ordering::Relaxed);
        return Some(r);
    }

    // Ring 1 — GPU (matmul pesado). Só se registrado e op grande.
    if big {
        if let Some(r) = call_slot(GPU_TERNARY.load(Ordering::Acquire), w, x) {
            N_GPU.fetch_add(1, Ordering::Relaxed);
            return Some(r);
        }
    }

    // Ring 1 fallback — P-cores (APs) via WS-B.
    if big
        && k_nano::platform_probe::allow_smp()
        && k_nano::smp::ap_entry_count() > 0
    {
        if let Some(r) = crate::parallel_matmul::parallel_ternary_matmul(w, x) {
            N_SMP.fetch_add(1, Ordering::Relaxed);
            return Some(r);
        }
    }

    // Ring 2 — CPU (AVX2/scalar): sinaliza fallback ao chamador.
    N_CPU.fetch_add(1, Ordering::Relaxed);
    None
}

/// (npu, gpu, smp, cpu) — contadores de dispatch para telemetria/serial.
pub fn dispatch_summary() -> (u64, u64, u64, u64) {
    (
        N_NPU.load(Ordering::Relaxed),
        N_GPU.load(Ordering::Relaxed),
        N_SMP.load(Ordering::Relaxed),
        N_CPU.load(Ordering::Relaxed),
    )
}

/// True se algum acelerador (NPU/GPU) está registrado.
pub fn accel_registered() -> bool {
    GPU_TERNARY.load(Ordering::Acquire) != 0 || NPU_TERNARY.load(Ordering::Acquire) != 0
}
