//! HotSwap multi-vendor — 4 fases; MVP CpuEcho + LlamaGguf(cli); vendors EngineAbsent.

mod backend;
mod llama_cli;

pub use backend::{CpuEchoBackend, EngineKind, InferenceBackend, SwapError};
pub use llama_cli::{find_llama_cli, LlamaCliBackend};

use parking_lot::Mutex;
use std::path::PathBuf;
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

    pub fn engine_name(&self) -> &'static str {
        self.active.lock().name()
    }

    pub fn infer(&self, prompt: &str) -> Result<String, SwapError> {
        self.active.lock().infer(prompt)
    }

    /// Tenta anexar GGUF via `llama-cli`. Falha → mantém CpuEcho (caller decide log).
    pub fn try_attach_llama(&self, model: PathBuf) -> Result<(), SwapError> {
        let backend = LlamaCliBackend::try_new(model)?;
        info!(target: "nosd", "hotswap attach {}", backend.name());
        *self.active.lock() = Box::new(backend);
        *self.kind.lock() = EngineKind::LlamaGguf;
        Ok(())
    }

    /// Protocolo 4 fases (scaffold): freeze→serialize KV→evict→link+warmup.
    pub fn swap_to(&self, kind: EngineKind) -> Result<(), SwapError> {
        info!(target: "nosd", "hotswap freeze/drain kind={:?}", kind);
        info!(target: "nosd", "hotswap kv_serialize (stub session)");
        info!(target: "nosd", "hotswap vram_eviction");
        let backend: Box<dyn InferenceBackend> = match kind {
            EngineKind::CpuEcho => Box::new(CpuEchoBackend),
            EngineKind::LlamaGguf => {
                warn!(target: "nosd", "swap_to(LlamaGguf) requires try_attach_llama(path)");
                return Err(SwapError::EngineAbsent("llama.cpp path required"));
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
        assert_eq!(hs.engine_name(), "cpu-echo");
    }

    #[test]
    fn vendor_absent() {
        let hs = HotSwapController::new_cpu();
        assert!(matches!(
            hs.swap_to(EngineKind::Exl2),
            Err(SwapError::EngineAbsent(_))
        ));
    }

    #[test]
    fn attach_missing_model() {
        let hs = HotSwapController::new_cpu();
        let r = hs.try_attach_llama(PathBuf::from("/no/such/model.gguf"));
        assert!(r.is_err());
        assert_eq!(hs.kind(), EngineKind::CpuEcho);
    }
}
