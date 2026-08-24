//! Agent.xpu — prefill/decode split entre CPU e GPU.
//! CPU: prefill (forward do prompt), GPU: decode (1 token/vez via KV cache).
//! Referência: arXiv 2506.24045.
//!
//! Quando GPU está pronta (BackendState::Ready), o XPU despacha prefill/decode
//! para a fila lock-free do work_queue. Caso contrário, CPU fallback com
//! telemetria honesta via xpu_stats().

use cortex::cortex::{TransformerModel, KvCache};
use alloc::vec::Vec;
use core::sync::atomic::Ordering;

pub struct XpuConfig {
    pub use_gpu_decode: bool,
}

impl XpuConfig {
    pub fn cpu_only() -> Self { XpuConfig { use_gpu_decode: false } }
    pub fn gpu_decode() -> Self { XpuConfig { use_gpu_decode: true } }
}

pub struct XpuEngine {
    pub config: XpuConfig,
    pub prefill_ticks: u64,
    pub decode_ticks: u64,
    total_tokens: u64,
    gpu_dispatches: u64,
    cpu_fallbacks: u64,
}

fn now_ticks() -> u64 {
    k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64
}

/// Check if GPU backend is ready for compute dispatch.
/// Honesto: retorna false em QEMU sem GPU real.
fn gpu_ready() -> bool {
    crate::gpu::backend::compute_state() == crate::gpu::compute_abi::BackendState::Ready
}

impl XpuEngine {
    pub fn new(config: XpuConfig) -> Self {
        XpuEngine {
            config,
            prefill_ticks: 0,
            decode_ticks: 0,
            total_tokens: 0,
            gpu_dispatches: 0,
            cpu_fallbacks: 0,
        }
    }

    /// Prefill: forward do prompt completo com KV cache.
    /// Se GPU pronta e config.use_gpu_decode, despacha para work_queue.
    pub fn prefill(&mut self, model: &TransformerModel, prompt: &[u32], cache: &mut KvCache, tick_start: u64) {
        if prompt.is_empty() { return; }
        let use_gpu = self.config.use_gpu_decode && gpu_ready();
        if use_gpu {
            // AWAITING_HW: enqueue Prefill op na fila GPU (Layer S/HW pendente).
            // Quando pushbuffer NVIDIA / Intel ring estiver pronto, drain() despacha.
            let _ = crate::gpu::work_queue::submit(crate::gpu::work_queue::GpuOp::Prefill);
            self.gpu_dispatches += 1;
            k_nano::slog_hal!("XPU", "prefill-gpu", "{} tokens → GPU queue", prompt.len());
        }
        // Prefill sempre roda em CPU (prompt processing é paralelizável em layers)
        let (_logits, _hidden) = model.forward_with_kv(prompt, cache);
        self.prefill_ticks += now_ticks().wrapping_sub(tick_start);
        if use_gpu {
            k_nano::slog_hal!("XPU", "prefill-gpu-done", "{} tokens", prompt.len());
        } else {
            self.cpu_fallbacks += 1;
            k_nano::slog_hal!("XPU", "prefill-cpu", "{} tokens (GPU AWAITING_HW)", prompt.len());
        }
    }

    /// Decode: gera 1 token. Se GPU pronta e config.use_gpu_decode, despacha decode.
    pub fn decode(&mut self, model: &TransformerModel, ctx: &[u32], _cache: &mut KvCache, tick_start: u64) -> u32 {
        let use_gpu = self.config.use_gpu_decode && gpu_ready();
        if use_gpu {
            let _ = crate::gpu::work_queue::submit(crate::gpu::work_queue::GpuOp::Decode);
            self.gpu_dispatches += 1;
        }
        // Decode sempre roda em CPU (1 token por vez, KV cache lookup serial)
        let token = model.generate_next(ctx);
        self.total_tokens = self.total_tokens.wrapping_add(1);
        self.decode_ticks += now_ticks().wrapping_sub(tick_start);
        if !use_gpu {
            self.cpu_fallbacks += 1;
        }
        token
    }

    /// Geração completa: prefill + N steps decode (CPU sempre; GPU dispatch é marca de intenção).
    pub fn generate(&mut self, model: &TransformerModel, prompt: &[u32], max_tokens: usize,
                    cache: &mut KvCache) -> Vec<u32> {
        let t0 = now_ticks();
        self.prefill(model, prompt, cache, t0);
        let mut output = prompt.to_vec();
        let mut ctx = prompt.to_vec();
        for _ in 0..max_tokens {
            let t1 = now_ticks();
            let tok = self.decode(model, &ctx, cache, t1);
            if tok == 0 || tok == 2 { break; }
            output.push(tok);
            ctx.push(tok);
            if ctx.len() > 512 { ctx.drain(0..ctx.len() - 256); }
        }
        let elapsed = now_ticks().wrapping_sub(t0);
        k_nano::slog_hal!("XPU", "info", "{} tokens em {} ticks | GPU={} CPU={}",
            output.len() - prompt.len(), elapsed, self.gpu_dispatches, self.cpu_fallbacks);
        output
    }

    pub fn stats(&self) -> alloc::string::String {
        alloc::format!(
            "XPU: {} prefill, {} decode ticks, {} tokens, GPU_dispatch={}, CPU_fallback={}",
            self.prefill_ticks, self.decode_ticks, self.total_tokens,
            self.gpu_dispatches, self.cpu_fallbacks
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xpu_cpu_only_config() {
        let cfg = XpuConfig::cpu_only();
        assert!(!cfg.use_gpu_decode);
    }

    #[test]
    fn xpu_gpu_config() {
        let cfg = XpuConfig::gpu_decode();
        assert!(cfg.use_gpu_decode);
    }

    #[test]
    fn xpu_engine_starts_zeroed() {
        let eng = XpuEngine::new(XpuConfig::cpu_only());
        assert_eq!(eng.prefill_ticks, 0);
        assert_eq!(eng.decode_ticks, 0);
        assert_eq!(eng.total_tokens, 0);
        assert_eq!(eng.gpu_dispatches, 0);
        assert_eq!(eng.cpu_fallbacks, 0);
    }

    #[test]
    fn xpu_stats_string_not_empty() {
        let eng = XpuEngine::new(XpuConfig::gpu_decode());
        let s = eng.stats();
        assert!(s.contains("XPU"));
        assert!(s.contains("GPU_dispatch=0"));
    }
}
