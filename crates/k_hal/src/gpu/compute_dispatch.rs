//! ADR-0057 WS-D — Ponte GPU → dispatcher de compute do `cortex`.
//!
//! Conecta o backend GPU (`k_hal`) ao choke point de matmul da LLM
//! (`cortex::compute`). **Regra de honestidade:** só registra o backend GPU
//! quando o canário `vector_add` passou em silício (`BackendState::Ready`) — em
//! QEMU/VirtIO (CpuOnly) nada é registrado e o dispatcher usa CPU/SMP.
//!
//! O kernel ternário no device (BitLinearW2A8, IDEA #330) é **Layer S / HW**:
//! exige KernelPack assinado (CUBIN/HSACO/zebin) + golden em silício. Enquanto
//! ausente, `gpu_ternary` retorna `None` (fallback honesto), mesmo com o
//! backend `Ready`.

use crate::gpu::backend::compute_state;
use crate::gpu::compute_abi::BackendState;
use cortex::tensor::{PackedTernaryTensor, Tensor};
use k_nano::slog_hal;

/// Backend ternário GPU. LAYER-S: sem KernelPack W2A8 assinado carregado, não
/// há kernel real → `None` (o dispatcher cai em CPU-SMP/AVX2/scalar).
fn gpu_ternary(_w: &PackedTernaryTensor, _x: &Tensor) -> Option<Tensor> {
    // TODO(Layer S / HW): despachar QMD/PM4/GPGPU_WALKER do kernel W2A8 e ler o
    // resultado da VRAM. Requer canário PASS + KernelPack assinado (ADR-0048/49/50).
    None
}

/// Chamado no fim do bring-up GPU. Só registra se o compute está `Ready`
/// (canário passou). Idempotente/honesto em QEMU (nunca `Ready`).
pub fn register_compute_if_ready() {
    if compute_state() == BackendState::Ready {
        cortex::compute::register_gpu_ternary(gpu_ternary);
        slog_hal!(
            "COMPUTE",
            "info",
            "GPU Ready — ternary registrado; W2A8 kernel = Layer S/HW pendente"
        );
    } else {
        slog_hal!(
            "COMPUTE",
            "info",
            "GPU nao-Ready ({:?}) — dispatcher usa CPU/SMP",
            compute_state()
        );
    }
}
