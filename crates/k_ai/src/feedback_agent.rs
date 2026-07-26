//! FeedbackAgent — subscreve CARD_ACTION (👍/👎) e alimenta SuccessEngine.
//! IDEA #149–#152: feedback loop → SleepCycle REPLAY → fine-tuning on-device.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use k_nano::{EVENT_BUS, slog_kai};
use crate::success_engine::SuccessEngine;

const MANIFEST: AgentManifest = AgentManifest {
    name: "FeedbackAgent",
    kind: AgentKind::System,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

/// Agent que coleta feedback do usuário a partir de eventos CARD_ACTION.
///
/// O payload de CARD_ACTION tem formato `"card_id:button_idx"`:
/// - `button_idx = 0` → 👍 (positivo)
/// - `button_idx = 1` → 👎 (negativo)
///
/// Os feedbacks são armazenados no `SuccessEngine` interno e exportados
/// para o SleepCycle durante a fase REPLAY.
pub struct FeedbackAgent {
    card_action_rx: event_bus::Receiver,
    engine: SuccessEngine,
}

impl FeedbackAgent {
    pub fn new() -> Self {
        Self {
            card_action_rx: EVENT_BUS.subscribe("CARD_ACTION"),
            engine: SuccessEngine::new(100),
        }
    }

    /// Acesso ao engine para coleta de amostras (SleepCycle).
    pub fn engine(&self) -> &SuccessEngine {
        &self.engine
    }

    /// Acesso mutável ao engine (reset, etc.).
    pub fn engine_mut(&mut self) -> &mut SuccessEngine {
        &mut self.engine
    }
}

impl Agent for FeedbackAgent {
    fn manifest(&self) -> &AgentManifest {
        &MANIFEST
    }

    fn tick(&mut self, tick: u64, _count: u64) -> AgentTickResult {
        while let Some(ev) = self.card_action_rx.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            // Payload: "card_id:button_idx"
            if let Some((card_id, btn_idx)) = text.split_once(':') {
                let positive = match btn_idx.trim() {
                    "0" => true,   // 👍
                    "1" => false,  // 👎
                    _ => {
                        slog_kai!("FEEDBACK", "warn", "unknown button_idx={} card={}", btn_idx, card_id);
                        continue;
                    }
                };
                // ponytail: card_id lookup of input/response not wired yet.
                // Upgrade: subscribe HERMES_RESPONSE + USER_INTENT, build CardStore mapping
                // card_id → (input, response) for richer feedback recording.
                let input = alloc::format!("card:{}", card_id);
                let response = if positive { "👍" } else { "👎" };
                self.engine.record(&input, response, positive, tick);

                let emoji = if positive { "👍" } else { "👎" };
                slog_kai!("FEEDBACK", "info", "{} card={}", emoji, card_id);
            }
        }
        AgentTickResult::Pending
    }
}
