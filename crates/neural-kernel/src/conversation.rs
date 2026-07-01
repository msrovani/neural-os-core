use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;

const MAX_EVENTS: usize = 256;
/// Curated memory budget: quanto contexto é SEMPRE carregado (Anatomy gap)
const CURATED_MEMORY_BUDGET: usize = 4096; // ~4KB de contexto sempre disponível

#[derive(Clone, Debug)]
pub enum EventKind {
    UserInput,
    HermesResponse,
    SkillExecuted,
    SystemEvent,
    ContextCompacted,
    KernelError,
}

#[derive(Clone, Debug)]
pub struct ConversationEvent {
    pub kind: EventKind,
    pub payload: Vec<u8>,
    pub timestamp: u64,
}

pub struct EventLog {
    events: VecDeque<ConversationEvent>,
    next_id: u64,
}

impl EventLog {
    pub const fn new() -> Self {
        EventLog {
            events: VecDeque::new(),
            next_id: 0,
        }
    }

    pub fn push(&mut self, kind: EventKind, payload: Vec<u8>, timestamp: u64) {
        if self.events.len() >= MAX_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(ConversationEvent {
            kind,
            payload,
            timestamp,
        });
        self.next_id += 1;
    }

    pub fn iter(&self) -> impl Iterator<Item = &ConversationEvent> {
        self.events.iter()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }

    pub fn last_n(&self, n: usize) -> Vec<&ConversationEvent> {
        self.events.iter().rev().take(n).collect()
    }

    pub fn events_since(&self, timestamp: u64) -> Vec<&ConversationEvent> {
        self.events.iter().filter(|e| e.timestamp >= timestamp).collect()
    }

    pub fn summarize(&self) -> String {
        let total = self.events.len();
        let user_count = self.events.iter().filter(|e| matches!(e.kind, EventKind::UserInput)).count();
        let resp_count = self.events.iter().filter(|e| matches!(e.kind, EventKind::HermesResponse)).count();
        let skill_count = self.events.iter().filter(|e| matches!(e.kind, EventKind::SkillExecuted)).count();
        alloc::format!(
            "{} eventos ({} input, {} resposta, {} skill) — ultimo @ tick {}",
            total, user_count, resp_count, skill_count,
            self.events.back().map_or(0, |e| e.timestamp),
        )
    }

    /// Curated context: retorna sempre ≤ CURATED_MEMORY_BUDGET bytes
    /// Prioriza: últimos inputs + respostas recentes, descarta o resto
    pub fn curated_context(&self) -> String {
        let mut ctx = String::new();
        // Sempre inclui os últimos 3 exchanges
        for ev in self.events.iter().rev().take(6) {
            let prefix = match ev.kind {
                EventKind::UserInput => "User: ",
                EventKind::HermesResponse => "Hermes: ",
                _ => "",
            };
            if !prefix.is_empty() {
                if let Ok(text) = core::str::from_utf8(&ev.payload) {
                    ctx.push_str(prefix);
                    ctx.push_str(text);
                    ctx.push('\n');
                }
            }
            if ctx.len() > CURATED_MEMORY_BUDGET { break; }
        }
        ctx.truncate(CURATED_MEMORY_BUDGET);
        ctx
    }
}
