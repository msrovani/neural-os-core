//! MessageBus — IPC ponto-a-ponto entre agentes (ADR-0068 / Labor 9).
//! Complementa EventBus (pub/sub). CapToken deny se inválido.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use ticket_lock::TicketLock;

use crate::capability::CapabilityToken;
use crate::channel::BoundedChannel;

/// Identificador de agente (estático no MVP).
pub type AgentId = &'static str;

pub const DEFAULT_MAILBOX_CAP: usize = 32;

/// Envelope ponto-a-ponto.
#[derive(Clone)]
pub struct Envelope {
    pub from: AgentId,
    pub to: AgentId,
    pub kind: u32,
    pub payload: Vec<u8>,
    pub token: CapabilityToken,
}

/// Barramento de mailboxes por `AgentId`.
pub struct MessageBus {
    mailboxes: TicketLock<BTreeMap<String, BoundedChannel<Envelope>>>,
    default_cap: usize,
}

impl MessageBus {
    pub fn new() -> Self {
        Self {
            mailboxes: TicketLock::new(BTreeMap::new()),
            default_cap: DEFAULT_MAILBOX_CAP,
        }
    }

    pub fn with_capacity(default_cap: usize) -> Self {
        Self {
            mailboxes: TicketLock::new(BTreeMap::new()),
            default_cap: default_cap.max(1),
        }
    }

    /// Garante mailbox do agente (cria se ausente).
    pub fn open_mailbox(&self, agent: AgentId) {
        let key = String::from(agent);
        let mut map = self.mailboxes.lock();
        map.entry(key)
            .or_insert_with(|| BoundedChannel::new(self.default_cap));
    }

    /// Envia para mailbox de `to`. Deny se token inválido.
    pub fn send(&self, env: Envelope) -> Result<(), &'static str> {
        if !env.token.is_valid() {
            return Err("ipc_token_invalid");
        }
        let to = env.to;
        self.open_mailbox(to);
        let map = self.mailboxes.lock();
        let ch = map
            .get(to)
            .ok_or("ipc_mailbox_missing")?;
        ch.send(env)
    }

    /// Helper: monta envelope e envia.
    pub fn send_to(
        &self,
        from: AgentId,
        to: AgentId,
        kind: u32,
        payload: Vec<u8>,
        token: CapabilityToken,
    ) -> Result<(), &'static str> {
        self.send(Envelope {
            from,
            to,
            kind,
            payload,
            token,
        })
    }

    pub fn try_recv(&self, agent: AgentId) -> Option<Envelope> {
        let map = self.mailboxes.lock();
        map.get(agent).and_then(|ch| ch.try_recv())
    }

    /// Drena até `max` mensagens (ordem FIFO).
    pub fn mailbox_drain(&self, agent: AgentId, max: usize) -> Vec<Envelope> {
        let mut out = Vec::new();
        for _ in 0..max {
            match self.try_recv(agent) {
                Some(e) => out.push(e),
                None => break,
            }
        }
        out
    }

    pub fn pending(&self, agent: AgentId) -> usize {
        let map = self.mailboxes.lock();
        map.get(agent).map(|ch| ch.len()).unwrap_or(0)
    }
}

impl Default for MessageBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Smoke A→B roundtrip. Sem I/O; para boot.
pub fn self_test() -> bool {
    let bus = MessageBus::with_capacity(4);
    bus.open_mailbox("cortex");
    bus.open_mailbox("rustcoder");

    if bus
        .send_to(
            "cortex",
            "rustcoder",
            1,
            b"ping".to_vec(),
            CapabilityToken::Legacy(0),
        )
        .is_ok()
    {
        return false; // token 0 deve deny
    }

    if bus
        .send_to(
            "cortex",
            "rustcoder",
            1,
            b"ping".to_vec(),
            CapabilityToken::Legacy(1),
        )
        .is_err()
    {
        return false;
    }

    match bus.try_recv("rustcoder") {
        Some(e) if e.from == "cortex" && e.payload.as_slice() == b"ping" && e.kind == 1 => {}
        _ => return false,
    }

    if bus.try_recv("rustcoder").is_some() {
        return false;
    }

    // Reply
    if bus
        .send_to(
            "rustcoder",
            "cortex",
            2,
            b"pong".to_vec(),
            CapabilityToken::Legacy(1),
        )
        .is_err()
    {
        return false;
    }
    matches!(
        bus.try_recv("cortex"),
        Some(e) if e.from == "rustcoder" && e.payload.as_slice() == b"pong"
    )
}
