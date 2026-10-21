//! App Factory path A — validate wasmi + CapGate + promote (ADR-0059).

use nos_core::CapBits;
use nos_wasm::{FrugalRuntime, WasmError, tiny_identity_wasm};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::info;

#[derive(Debug, Error)]
pub enum FactoryError {
    #[error("path B/C EngineAbsent until Linux isolation VERIFIED")]
    PathBcAbsent,
    #[error("wasm: {0}")]
    Wasm(#[from] WasmError),
    #[error("promote denied")]
    PromoteDenied,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FactoryPath {
    AWasmi,
    BCranelift,
    CNative,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactDraft {
    pub name: String,
    pub wasm: Vec<u8>,
    pub path: FactoryPath,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotedArtifact {
    pub name: String,
    pub content_hash: u64,
}

pub struct AppFactory {
    rt: FrugalRuntime,
}

impl Default for AppFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl AppFactory {
    pub fn new() -> Self {
        Self {
            rt: FrugalRuntime::new(),
        }
    }

    pub fn validate(&self, draft: &ArtifactDraft, caps: CapBits, gen: u64) -> Result<i32, FactoryError> {
        match draft.path {
            FactoryPath::AWasmi => {
                let code = if draft.wasm.is_empty() {
                    tiny_identity_wasm()
                } else {
                    draft.wasm.clone()
                };
                Ok(self.rt.run_i32(&code, caps, gen, 7)?)
            }
            FactoryPath::BCranelift | FactoryPath::CNative => Err(FactoryError::PathBcAbsent),
        }
    }

    pub fn promote(
        &self,
        draft: &ArtifactDraft,
        caps: CapBits,
        gen: u64,
        allow_promote: bool,
    ) -> Result<PromotedArtifact, FactoryError> {
        if !allow_promote {
            return Err(FactoryError::PromoteDenied);
        }
        let _ = self.validate(draft, caps, gen)?;
        let owned = if draft.wasm.is_empty() {
            tiny_identity_wasm()
        } else {
            draft.wasm.clone()
        };
        let hash = fnv1a64(&owned);
        info!(target: "nosd", "factory promote name={} hash={:016x}", draft.name, hash);
        Ok(PromotedArtifact {
            name: draft.name.clone(),
            content_hash: hash,
        })
    }
}

fn fnv1a64(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in data {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_a_ok() {
        let f = AppFactory::new();
        let d = ArtifactDraft {
            name: "echo".into(),
            wasm: vec![],
            path: FactoryPath::AWasmi,
        };
        assert_eq!(f.validate(&d, CapBits::ALL, 1).unwrap(), 7);
        assert!(f.promote(&d, CapBits::ALL, 1, true).is_ok());
    }

    #[test]
    fn path_b_absent() {
        let f = AppFactory::new();
        let d = ArtifactDraft {
            name: "x".into(),
            wasm: vec![],
            path: FactoryPath::BCranelift,
        };
        assert!(matches!(
            f.validate(&d, CapBits::ALL, 1),
            Err(FactoryError::PathBcAbsent)
        ));
    }
}
