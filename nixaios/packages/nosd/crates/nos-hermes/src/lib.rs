//! Hermes agentic mínimo — Observe→Plan→Act→Remember via CapGate.

use nos_bei::BeiRuntime;
use nos_bus::EventBus;
use nos_core::{CapBits, CapGate, ModelSlot};
use nos_cortex::{HotSwapController, ModelHub};
use nos_memory::{MemoryRecord, NosMemory};
use serde_json::json;
use thiserror::Error;
use tracing::info;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum HermesError {
    #[error("cap: {0}")]
    Cap(&'static str),
    #[error("infer: {0}")]
    Infer(String),
    #[error("memory: {0}")]
    Memory(String),
}

pub struct HermesEngine {
    pub hub: ModelHub,
    pub hotswap: HotSwapController,
    pub memory: NosMemory,
    pub bei: BeiRuntime,
    pub bus: EventBus,
    pub cap_gen: u64,
}

impl HermesEngine {
    pub fn chat(&self, gate: &CapGate, prompt: &str) -> Result<String, HermesError> {
        gate.allow(CapBits::INFER, self.cap_gen)
            .map_err(HermesError::Cap)?;
        let _ = self.bei.tick();
        // Observe
        self.bus.publish("HERMES_OBSERVE", json!({ "prompt": prompt }));
        // Plan: Active slot LOO
        if !self.hub.ensure_loaded(ModelSlot::Active) {
            info!(target: "nosd", "hermes Active miss — cpu-echo fallback");
        }
        // Act
        let reply = self
            .hotswap
            .infer(prompt)
            .map_err(|e| HermesError::Infer(e.to_string()))?;
        // Remember (nsgdb doctrine)
        if gate.allow(CapBits::MEMORY, self.cap_gen).is_ok() {
            let key = format!("md/L2/chat/{}", Uuid::new_v4());
            self.memory
                .remember(MemoryRecord {
                    key,
                    text: format!("user: {prompt} | assistant: {reply}"),
                    scope: "session/nosd".into(),
                    entities: vec!["mom/episodic".into()],
                })
                .map_err(|e| HermesError::Memory(e.to_string()))?;
        }
        self.bus.publish("HERMES_RESPONSE", json!({ "text": reply }));
        Ok(reply)
    }

    pub fn recall_context(&self, query: &str) -> Result<Vec<String>, HermesError> {
        let hits = self
            .memory
            .recall_lexical(query, 5)
            .map_err(|e| HermesError::Memory(e.to_string()))?;
        Ok(hits.into_iter().map(|h| h.text).collect())
    }
}
