//! HotSwap multi-vendor — 4 fases; MVP CpuEcho; vendors EngineAbsent.

mod backend;

pub use backend::{CpuEchoBackend, EngineKind, InferenceBackend, SwapError};

use parking_lot::Mutex;
use std::sync::Arc;
use tracing::{info, warn};

#[derive(Clone)]
pub struct HotSwapController {
    active: Arc<Mutex<Box<dyn InferenceBackend>>>,
    kind: Arc<Mutex<EngineKind>>,
}

impl HotSwapController {
    pub fn new_cpu() -> Self {
        Self {
            active: Arc::new(Mutex::new(Box::new(CpuEchoBackend))),
            kind: Arc::new(Mutex::new(EngineKind::CpuEcho)),
        }
    }

    pub fn kind(&self) -> EngineKind {
        *self.kind.lock()
    }

    pub fn infer(&self, prompt: &str) -> Result<String, SwapError> {
        self.active.lock().infer(prompt)
    }

    /// Protocolo 4 fases (scaffold): freeze→serialize KV→evict→link+warmup.
    pub fn swap_to(&self, kind: EngineKind) -> Result<(), SwapError> {
        info!(target: "nosd", "hotswap freeze/drain kind={:?}", kind);
        // Fase 1: freeze — fila externa deve parar submits (caller).
        info!(target: "nosd", "hotswap kv_serialize (stub session)");
        // Fase 2: KV serialize — stub path.
        info!(target: "nosd", "hotswap vram_eviction");
        // Fase 3: eviction — backend antigo drop.
        let backend: Box<dyn InferenceBackend> = match kind {
            EngineKind::CpuEcho => Box::new(CpuEchoBackend),
            EngineKind::LlamaGguf => {
                // Sem libllama no scaffold — EngineAbsent honesto.
                warn!(target: "nosd", "llama.cpp not linked; EngineAbsent");
                return Err(SwapError::EngineAbsent("llama.cpp"));
            }
            EngineKind::Exl2 | EngineKind::VllmCuda => {
                return Err(SwapError::EngineAbsent("nvidia backend"));
            }
            EngineKind::VllmRocm | EngineKind::LlamaHip => {
                return Err(SwapError::EngineAbsent("amd backend"));
            }
            EngineKind::Ipex | EngineKind::OpenVino | EngineKind::LlamaSycl => {
                return Err(SwapError::EngineAbsent("intel backend"));
            }
        };
        // Fase 4: dynamic link + warmup
        info!(target: "nosd", "hotswap warmup kind={:?}", kind);
        let _ = backend.infer("warmup");
        *self.active.lock() = backend;
        *self.kind.lock() = kind;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_infer() {
        let hs = HotSwapController::new_cpu();
        let out = hs.infer("oi").unwrap();
        assert!(out.contains("oi"));
    }

    #[test]
    fn vendor_absent() {
        let hs = HotSwapController::new_cpu();
        assert!(matches!(
            hs.swap_to(EngineKind::Exl2),
            Err(SwapError::EngineAbsent(_))
        ));
    }
}
