//! GPU Backend Bridge — jarbas FE wrapper for k_hal GPU BE.
//! Re-exports k_hal GPU functions and provides safe init/fallback logic.

use alloc::string::String;

// Re-export k_hal GPU backend functions
pub use k_hal::gpu::backend::{
    gpu_matmul, gpu_status, job_ring_info,
    adr0047_compute_gate, compute_state,
};
pub use k_hal::gpu::compute_abi::BackendState;
pub use k_hal::gpu::vram::{vram_alloc, vram_free, vram_status, vram_usage};
pub use k_hal::gpu::blit::{blit_2d, fill_rect_2d, blit_ready};
pub use k_hal::gpu::canary::{run_vector_add_canary, CanaryResult, state_after};
pub use k_hal::gpu::detect::{GpuInfo, GpuVendor};
pub use k_hal::gpu::display_coex::{plan_assignment, GpuAssignment};
pub use k_hal::gpu::firmware::{preload_blob, load_firmware_file, has_named_blob};

/// Initialize GPU backend — verifies compute backend is ready (not CpuOnly).
/// Called from DisplayAgent::tick() on first tick.
/// Ready = canário CE/vector_add, NÃO KernelPack/matmul no device (SESSION_274).
pub fn init_gpu_backend() -> Result<(), &'static str> {
    // k_hal GPU init happens in Phase 5 (DriverInit) via PlatformAgent
    let state = compute_state();
    match state {
        BackendState::Ready => Ok(()),
        BackendState::CpuOnly => Err("GPU backend not initialized (CpuOnly)"),
        BackendState::Quarantine => Err("GPU backend in quarantine (canary failed)"),
        BackendState::BringingUp => Err("GPU backend still bringing up"),
        BackendState::Probed => Err("GPU backend only probed (firmware/canary pending)"),
    }
}

/// Safe wrapper for gpu_matmul. NOTA (SESSION_274): enquanto o kernel W2A8
/// no device é Layer S, o resultado vem do CPU fallback interno do backend —
/// a telemetria (work_queue) já registra isso honestamente.
pub fn try_gpu_matmul(a: &cortex::tensor::Tensor, b: &cortex::tensor::Tensor) -> Option<cortex::tensor::Tensor> {
    // Check compute state before attempting GPU path
    if compute_state() != BackendState::Ready {
        return None;
    }
    gpu_matmul(a, b)
}

/// Safe wrapper for VRAM allocation with OOM handling.
/// Returns Some(phys_addr) on success, None on OOM (falls back to CPU).
pub fn try_vram_alloc(size: usize) -> Option<u64> {
    if compute_state() != BackendState::Ready {
        return None;
    }
    vram_alloc(size)
}

/// Safe wrapper for blit_2d with CPU fallback.
/// Returns true on success (GPU or CPU).
pub fn try_blit_2d(src_pa: u64, dst_pa: u64, w: u32, h: u32, bpp: u32) -> bool {
    blit_2d(src_pa, dst_pa, w, h, bpp)
}

/// Safe wrapper for fill_rect_2d.
/// Returns true on success.
pub fn try_fill_rect_2d(dst_pa: u64, w: u32, h: u32, bpp: u32, color: u32) -> bool {
    fill_rect_2d(dst_pa, w, h, bpp, color)
}

/// Get GPU status string for display in gauges/dock.
/// Returns formatted string with vendor, VRAM, compute state.
pub fn gpu_status_string() -> String {
    gpu_status()
}

/// Get VRAM status string for display.
/// Returns formatted string with used/free/total MB.
pub fn vram_status_string() -> String {
    vram_status()
}

/// Check if GPU compute is ready (canary passed).
pub fn is_gpu_compute_ready() -> bool {
    compute_state() == BackendState::Ready
}

/// Check if blit acceleration is available.
pub fn is_blit_ready() -> bool {
    blit_ready()
}

/// Get job ring info for debugging.
pub fn job_ring_info_string() -> String {
    job_ring_info()
}

/// Get ADR-0047 compute gate status.
pub fn compute_gate_status() -> &'static str {
    adr0047_compute_gate()
}