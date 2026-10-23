//! Hermes — Observe→Plan→Act→Verify→Remember + MoE S1/S2 (manifesto NixAIOS).

use nos_bei::BeiRuntime;
use nos_bus::EventBus;
use nos_core::{CapBits, CapGate, ModelSlot};
use nos_cortex::{HotSwapController, ModelHub, RouteDecision, SystemLane, route_intent};
use nos_factory::{AppFactory, ArtifactDraft, FactoryPath};
use nos_memory::{MemoryRecord, NosMemory};
use serde_json::json;
use thiserror::Error;
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum HermesError {
    #[error("cap: {0}")]
    Cap(&'static str),
    #[error("infer: {0}")]
    Infer(String),
    #[error("memory: {0}")]
    Memory(String),
    #[error("factory: {0}")]
    Factory(String),
}

pub struct HermesEngine {
    pub hub: ModelHub,
    pub hotswap: HotSwapController,
    pub memory: NosMemory,
    pub bei: BeiRuntime,
    pub bus: EventBus,
    pub factory: AppFactory,
    pub cap_gen: u64,
}

impl HermesEngine {
    /// Ciclo cognitivo chat com roteamento MoE.
    pub fn chat(&self, gate: &CapGate, prompt: &str) -> Result<String, HermesError> {
        gate.allow(CapBits::INFER, self.cap_gen)
            .map_err(HermesError::Cap)?;
        let bei_budget = self
            .bei
            .tick()
            .map(|s| s.budget_remaining)
            .unwrap_or(0);
        let decision = route_intent(prompt);

        // Observe
        self.bus.publish(
            "HERMES_OBSERVE",
            json!({
                "prompt_len": prompt.len(),
                "slot": decision.slot.name(),
                "lane": lane_name(decision.lane),
                "reason": decision.reason,
                "bei_budget": bei_budget,
            }),
        );

        // Plan — LOO no slot preferido; fallback Active se niche vazio
        let slot = self.plan_slot(decision);
        if decision.lane == SystemLane::S2Deep {
            // Honesty: offload async ainda não tem fila dedicada — marca e segue sync
            // para não mentir "fire-and-forget concluído".
            self.bus.publish(
                "HERMES_S2_OFFLOAD",
                json!({ "slot": slot.name(), "mode": "sync_marked" }),
            );
            warn!(
                target: "nosd",
                "hermes S2Deep sync_marked (async queue = residual) slot={}",
                slot.name()
            );
        }

        // Act
        info!(
            target: "nosd",
            "hermes act engine={} slot={} lane={} reason={}",
            self.hotswap.engine_name(),
            slot.name(),
            lane_name(decision.lane),
            decision.reason
        );
        let reply = self
            .hotswap
            .infer(prompt)
            .map_err(|e| HermesError::Infer(e.to_string()))?;

        // Verify (mínimo: não vazio)
        if reply.trim().is_empty() {
            return Err(HermesError::Infer("empty reply".into()));
        }

        // Remember
        if gate.allow(CapBits::MEMORY, self.cap_gen).is_ok() {
            let key = format!("md/L2/chat/{}", Uuid::new_v4());
            self.memory
                .remember(MemoryRecord {
                    key,
                    text: format!("user: {prompt} | assistant: {reply}"),
                    scope: "session/nosd".into(),
                    entities: vec![
                        "mom/episodic".into(),
                        format!("slot/{}", slot.name()),
                        format!("lane/{}", lane_name(decision.lane)),
                    ],
                })
                .map_err(|e| HermesError::Memory(e.to_string()))?;
        }
        self.bus.publish(
            "HERMES_RESPONSE",
            json!({
                "text_len": reply.len(),
                "slot": slot.name(),
                "lane": lane_name(decision.lane),
            }),
        );
        Ok(reply)
    }

    fn plan_slot(&self, decision: RouteDecision) -> ModelSlot {
        if self.hub.ensure_loaded(decision.slot) {
            return decision.slot;
        }
        if decision.slot != ModelSlot::Active && self.hub.ensure_loaded(ModelSlot::Active) {
            info!(
                target: "nosd",
                "hermes slot {} miss → Active",
                decision.slot.name()
            );
            return ModelSlot::Active;
        }
        info!(target: "nosd", "hermes hub empty — engine may be cpu-echo");
        decision.slot
    }

    pub fn recall_context(&self, query: &str) -> Result<Vec<String>, HermesError> {
        let hits = self
            .memory
            .recall_lexical(query, 5)
            .map_err(|e| HermesError::Memory(e.to_string()))?;
        Ok(hits.into_iter().map(|h| h.text).collect())
    }

    /// Explain route without infer (telemetria / HUD).
    pub fn explain_route(&self, prompt: &str) -> RouteDecision {
        route_intent(prompt)
    }

    /// Factory A: validate + promote WASM frugal sob CapGate AGENT_SPAWN.
    pub fn spawn_wasm_agent(
        &self,
        gate: &CapGate,
        name: &str,
        wasm: &[u8],
    ) -> Result<serde_json::Value, HermesError> {
        gate.allow(CapBits::AGENT_SPAWN, self.cap_gen)
            .map_err(HermesError::Cap)?;
        let draft = ArtifactDraft {
            name: name.to_string(),
            wasm: wasm.to_vec(),
            path: FactoryPath::AWasmi,
        };
        let caps = CapBits::AGENT_SPAWN;
        let out = self
            .factory
            .validate(&draft, caps, self.cap_gen)
            .map_err(|e| HermesError::Factory(e.to_string()))?;
        let promoted = self
            .factory
            .promote(&draft, caps, self.cap_gen, true)
            .map_err(|e| HermesError::Factory(e.to_string()))?;
        self.bus.publish(
            "HERMES_AGENT_SPAWN",
            json!({ "name": name, "hash": format!("{:016x}", promoted.content_hash) }),
        );
        Ok(json!({
            "name": promoted.name,
            "content_hash": format!("{:016x}", promoted.content_hash),
            "validate_i32": out,
            "path": "A",
        }))
    }

    /// Tick de liveness (I1) — BEI + score bus; não bloqueia.
    pub fn cognitive_tick(&self) -> serde_json::Value {
        let bei_budget = self
            .bei
            .tick()
            .map(|s| s.budget_remaining)
            .unwrap_or(0);
        let payload = json!({
            "bei_budget": bei_budget,
            "hub_loaded": self.hub.loaded_count(),
            "engine": self.hotswap.engine_name(),
        });
        self.bus.publish("HERMES_TICK", payload.clone());
        payload
    }
}

fn lane_name(l: SystemLane) -> &'static str {
    match l {
        SystemLane::S1Local => "s1",
        SystemLane::S2Deep => "s2",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nos_cortex::HotSwapController;
    use nos_memory::NosMemory;
    use tempfile::{TempDir, tempdir};

    struct TestEng {
        _dir: TempDir,
        eng: HermesEngine,
    }

    fn eng() -> TestEng {
        let dir = tempdir().unwrap();
        let eng = HermesEngine {
            hub: ModelHub::new(),
            hotswap: HotSwapController::new_cpu(),
            memory: NosMemory::open(dir.path()).unwrap(),
            bei: BeiRuntime::new(1000),
            bus: EventBus::new(16),
            factory: AppFactory::new(),
            cap_gen: 1,
        };
        TestEng { _dir: dir, eng }
    }

    #[test]
    fn chat_s1() {
        let t = eng();
        let g = CapGate::new(CapBits::ALL, 1);
        let r = t.eng.chat(&g, "oi").unwrap();
        assert!(!r.is_empty());
    }

    #[test]
    fn spawn_agent() {
        let t = eng();
        let g = CapGate::new(CapBits::ALL, 1);
        let v = t.eng.spawn_wasm_agent(&g, "echo", &[]).unwrap();
        assert_eq!(v["path"], "A");
    }
}
