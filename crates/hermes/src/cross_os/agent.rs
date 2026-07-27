//! CrossOsAgent — agente inteligente de descoberta e execucao de skills.
//! Continuous agent que monitora USER_INTENT e coordena o ciclo:
//! ANALISAR -> BUSCAR -> EXECUTAR -> APRENDER -> EVOLUIR

use event_bus::Receiver;
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use alloc::string::String;
use alloc::vec::Vec;
use k_nano::EVENT_BUS;
use crate::cross_os::intent::{CrossOsIntent, IntentCategory, IntentResult};
use crate::cross_os::discoverer::{CrossOsDiscoverer, DiscoverResult};

const CROSSOS_MANIFEST: AgentManifest = AgentManifest {
    name: "cross_os",
    kind: AgentKind::Router,
    schedule: ScheduleKind::EventDriven,
    auto_start: true,
    persist: true,
};

#[derive(Debug, Clone)]
struct UserNeed {
    intent_key: String,
    category: IntentCategory,
    times_used: u32,
    last_solution: String,
    proposed_upgrade: bool,
}

pub struct CrossOsAgent {
    receiver: Receiver,
    needs: Vec<UserNeed>,
}

impl CrossOsAgent {
    pub fn new() -> Self {
        Self {
            receiver: EVENT_BUS.subscribe("USER_INTENT"),
            needs: Vec::new(),
        }
    }

    fn process(&mut self, text: &str, tick: u64) {
        // 1. ANALISAR intencao
        let intent = CrossOsIntent::analyze(text);
        let intent_key = Self::key_for_intent(&intent);

        // 2. Verificar historico + BUSCAR
        let need_idx = self.needs.iter().position(|n| n.intent_key == intent_key);
        let times_used = need_idx.map(|i| self.needs[i].times_used).unwrap_or(0);

        if times_used == 0 {
            // Primeira vez: busca solucoes e sugere
            let discover = CrossOsDiscoverer::discover(&intent);

            if discover.has_wasm {
                let best = discover.best.as_ref().unwrap();
                Self::notify_user(&alloc::format!(
                    "Encontrei skill WASM '{}' para '{}'. Deseja instalar? (detalhes: {})",
                    best.name, text, best.description
                ));
            } else {
                Self::notify_user(&alloc::format!(
                    "Nao encontrei skill WASM para '{}'. Vou tentar executar via legacy (JAIL).",
                    text
                ));
            }

            self.needs.push(UserNeed {
                intent_key: intent_key.clone(),
                category: intent.category,
                times_used: 1,
                last_solution: Self::solution_name(&discover),
                proposed_upgrade: false,
            });
        } else {
            // Ja viu antes: sugere melhoria ou executa direto
            let need = &mut self.needs[need_idx.unwrap()];
            need.times_used += 1;

            if need.times_used >= 3 && !need.proposed_upgrade {
                // Terceira vez: propoe criar skill WASM dedicada
                need.proposed_upgrade = true;
                Self::notify_user(&alloc::format!(
                    "Percebi que voce usa '{}' frequentemente ({}x). \
                     Quer que eu crie uma skill WASM dedicada? Posso gerar via IA.",
                    text, need.times_used
                ));
            } else if need.times_used >= 2 && !need.proposed_upgrade {
                // Segunda vez: pergunta se quer migrar para WASM
                let discover = CrossOsDiscoverer::discover(&intent);
                if discover.has_wasm && need.last_solution != "wasm" {
                    Self::notify_user(&alloc::format!(
                        "Você usou '{}' via {}. Encontrei uma skill WASM agora. Quer migrar?",
                        text, need.last_solution
                    ));
                }
            } else {
                // Ja sabe o que fazer: executa direto
                k_nano::slog_bin!("CROSS-OS", "info",
                    "need={} use={} solution={}",
                    need.intent_key, need.times_used, need.last_solution);
            }
        }
    }

    fn key_for_intent(intent: &IntentResult) -> String {
        alloc::format!("{:?}", intent.category)
    }

    fn solution_name(discover: &DiscoverResult) -> String {
        if discover.has_wasm {
            "wasm"
        } else {
            "legacy"
        }.into()
    }

    fn notify_user(msg: &str) {
        let _ = EVENT_BUS.publish(event_bus::Event {
            id: 0,
            topic: alloc::string::String::from("HERMES_RESPONSE"),
            payload: msg.as_bytes().to_vec(),
            token: event_bus::CapabilityToken::Legacy(1),
        });
    }
}

impl Agent for CrossOsAgent {
    fn manifest(&self) -> &AgentManifest { &CROSSOS_MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        while let Some(ev) = self.receiver.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            if !text.is_empty() {
                self.process(text, _tick);
            }
        }
        AgentTickResult::Pending
    }
}
