//! Agent.xpu — prefill/decode split entre CPU e GPU.
//! CPU: prefill (forward do prompt), GPU: decode (1 token/vez via KV cache).
//! Referência: arXiv 2506.24045.

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
}

fn now_ticks() -> u64 {
    k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64
}

impl XpuEngine {
    pub fn new(config: XpuConfig) -> Self {
        XpuEngine { config, prefill_ticks: 0, decode_ticks: 0, total_tokens: 0 }
    }

    /// Prefill: CPU forward do prompt completo com KV cache
    pub fn prefill(&mut self, model: &TransformerModel, prompt: &[u32], cache: &mut KvCache, tick_start: u64) {
        if prompt.is_empty() { return; }
        let (_logits, _hidden) = model.forward_with_kv(prompt, cache);
        self.prefill_ticks += now_ticks().wrapping_sub(tick_start);
        k_nano::slog_hal!("XPU", "prefill", "{} tokens", prompt.len());
    }

    /// Decode: gera 1 token (sempre CPU por enquanto — GPU é stub futuro)
    pub fn decode(&mut self, model: &TransformerModel, ctx: &[u32], _cache: &mut KvCache, tick_start: u64) -> u32 {
        let token = model.generate_next(ctx);
        self.total_tokens = self.total_tokens.wrapping_add(1);
        self.decode_ticks += now_ticks().wrapping_sub(tick_start);
        token
    }

    /// Geração completa: prefill + N steps decode (CPU sempre)
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
        k_nano::slog_hal!("XPU", "info", "{} tokens em {} ticks", output.len() - prompt.len(), elapsed);
        output
    }

    pub fn stats(&self) -> alloc::string::String {
        alloc::format!("XPU: {} prefill, {} decode ticks, {} tokens, GPU={}",
            self.prefill_ticks, self.decode_ticks, self.total_tokens, self.config.use_gpu_decode)
    }
}
