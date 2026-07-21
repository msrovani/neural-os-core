//! FlowTrigger — quando e como um agente acorda.
//! Inspirado nos decorators @start, @listen, @router do CrewAI.
//! Um agente com flow Start acorda no boot.
//! Um agente com flow Listen(topic) acorda quando o topico tem mensagem.
//! Um agente com flow Router(topic) acorda, le o payload, e roteia.

use alloc::vec::Vec;
use alloc::string::String;
use crate::FlowTrigger;

/// Resultado do roteamento: para qual agente/skill delegar
#[derive(Debug, Clone)]
pub struct RouteResult {
    pub target_agent: String,
    pub target_skill: String,
    pub payload: Vec<u8>,
}

/// Handler de roteamento: funcao que decide para onde vai o payload
pub type RouterFn = fn(topic: &str, payload: &[u8]) -> Option<RouteResult>;

/// Pool de roteadores registrados
pub struct RouterRegistry {
    routers: Vec<(&'static str, RouterFn)>,
}

impl RouterRegistry {
    pub fn new() -> Self {
        RouterRegistry { routers: Vec::new() }
    }

    pub fn register(&mut self, topic: &'static str, func: RouterFn) {
        self.routers.push((topic, func));
    }

    pub fn route(&self, topic: &str, payload: &[u8]) -> Option<RouteResult> {
        for (t, func) in &self.routers {
            if *t == topic {
                if let Some(result) = func(topic, payload) {
                    return Some(result);
                }
            }
        }
        None
    }
}

/// Determina se um agente com FlowTrigger deve ser pollado neste tick
pub fn should_poll_flow(flow: &FlowTrigger, tick: u64, last_poll: u64, has_event: bool) -> bool {
    match flow {
        FlowTrigger::Schedule(sched) => match sched {
            crate::ScheduleKind::Continuous => true,
            crate::ScheduleKind::PollEvery(n) => last_poll == 0 || tick - last_poll >= *n,
            crate::ScheduleKind::Oneshot => true,
            crate::ScheduleKind::EventDriven => has_event,
        },
        FlowTrigger::Start => last_poll == 0, // soh uma vez
        FlowTrigger::Listen(_) => has_event,
        FlowTrigger::Router(_) => has_event,
    }
}
