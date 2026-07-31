//! ADR-0081 C3: Skill Sync between mesh nodes.
//!
//! ## Visão
//! Skills registradas no Master são sincronizadas para Workers do mesh.
//! Workers recebem skills atualizadas via NoProto packets. Skill nova no
//! Worker é promovida para o Master.
//!
//! ## Depende de: P2P Transport (Fase A da ADR-0081)
//! - udp_broadcast::send() para enviar skill manifests
//! - udp_broadcast::recv() para receber updates do Master
//! - mesh::local_role() para saber se é Master/Worker
//!
//! ## Fallback local (ativo enquanto P2P não estiver vivo)
//! Sem P2P, cada nó mantém seu próprio SkillRegistry — comportamento atual.
//! A função `sync_skills()` retorna imediatamente se P2P não estiver ativo.

use alloc::string::String;
use alloc::vec::Vec;
use k_nano::net::mesh::{self, NodeRole};
use k_nano::net::noproto::{AiosTaskPacket, TaskType, PacketFlags};
use k_nano::slog_hermes;
use spin::Mutex;
use ticket_lock::TicketLock;

/// Agente de sincronização de skills entre nós do mesh.
///
/// Gerencia a fila de skills pendentes e realiza a sync de acordo com
/// o papel do nó no mesh (Master push vs Worker promote).
pub struct SkillSync {
    /// true quando o transporte P2P está conectado
    active: bool,
    /// tick da última sincronização
    last_sync_tick: u64,
    /// nomes das skills pendentes de sincronização
    pending_skills: Vec<String>,
    /// nomes das skills já empurradas pelo Master (diff incremental)
    synced: Vec<String>,
}

impl SkillSync {
    /// Cria um novo `SkillSync` (inativo por padrão).
    pub const fn new() -> Self {
        Self {
            active: false,
            last_sync_tick: 0,
            pending_skills: Vec::new(),
            synced: Vec::new(),
        }
    }

    /// Marca P2P como ativo (chamado pela camada de transporte ao estabelecer link mesh).
    pub fn activate(&mut self) {
        self.active = true;
        slog_hermes!("SkillSync", "info", "P2P ativado — sync de skills habilitada");
    }

    /// Marca P2P como inativo (chamado pela camada de transporte ao desconectar).
    pub fn deactivate(&mut self) {
        self.active = false;
        slog_hermes!("SkillSync", "info", "P2P desativado — fallback local");
    }

    /// Retorna `true` se a sync P2P está ativa.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Executa o ciclo de sincronização.
    ///
    /// Se P2P estiver ativo, sincroniza skills pendentes com o mesh.
    /// Se P2P não estiver ativo, retorna imediatamente (fallback local-only).
    ///
    /// Chamado de `NetAgent::tick()` ou `HermesAgent::tick()`, recebendo o
    /// tick atual do scheduler.
    pub fn sync_skills(&mut self, tick: u64) {
        if !self.active {
            // Fallback local: no-op até o transporte P2P estar estabelecido
            return;
        }

        let role = mesh::local_role();

        // Throttle: sincroniza no máximo a cada 100 ticks (evita flooding)
        if tick.wrapping_sub(self.last_sync_tick) < 100 {
            return;
        }
        self.last_sync_tick = tick;

        match role {
            NodeRole::Master => {
                // Master push: diff do SkillRegistry e broadcast das skills
                // ainda não sincronizadas via NoProto (TaskType::Sync).
                let mut to_broadcast: Vec<(String, String)> = Vec::new();
                {
                    let reg = k_nano::SKILL_REGISTRY.lock();
                    for (entry, _pol) in reg.list_skills() {
                        // list_skills() devolve "name: desc"
                        let name = match entry.split_once(": ") {
                            Some((n, _d)) => n,
                            None => &entry[..],
                        };
                        if self.synced.iter().any(|s| s == name) {
                            continue;
                        }
                        let desc = match entry.split_once(": ") {
                            Some((_n, d)) => d,
                            None => "",
                        };
                        to_broadcast.push((String::from(name), String::from(desc)));
                    }
                }
                for (name, desc) in to_broadcast {
                    if self.broadcast_skill(&name, &desc) {
                        self.synced.push(name);
                    }
                }
            }
            NodeRole::Worker | NodeRole::Compute | NodeRole::Memory => {
                // Worker push: promove skill para o Master
                // ponytail: udp_broadcast::send(PROMOTE_SKILL) não implementado — log apenas
                if let Some(name) = self.pending_skills.first() {
                    slog_hermes!(
                        "SkillSync", "info",
                        "Worker: promovendo skill='{}' para Master (PROMOTE pendente)",
                        name
                    );
                }
            }
            NodeRole::Undecided => {
                // Nó ainda não faz parte do mesh — requeue para próxima sync
                if !self.pending_skills.is_empty() {
                    let name = self.pending_skills.remove(0);
                    self.pending_skills.push(name);
                }
            }
        }
    }

    /// Master push: serializa "name\0desc" num NoProto TaskType::Sync e faz
    /// broadcast UDP na porta P2P (42069) via transporte k_nano (R0).
    /// Retorna `true` se o envio foi ok.
    fn broadcast_skill(&mut self, name: &str, desc: &str) -> bool {
        let node_id = mesh::local_role() as u8;
        let pkt = AiosTaskPacket::new(
            0, node_id, 0xFF, TaskType::Sync, 1, 0, 0, PacketFlags::new(),
        );
        let mut buf = k_nano::net::udp_broadcast::serialize(&pkt);
        let payload = alloc::format!("{}\0{}", name, desc).into_bytes();
        buf.extend_from_slice(&payload);
        let ok = k_nano::net::udp_broadcast::udp_broadcast_send(&buf, 42069);
        slog_hermes!(
            "SkillSync", "info",
            "Master: push skill='{}' broadcast={}", name, ok
        );
        ok
    }

    /// Marca uma skill para sincronização no próximo ciclo.
    ///
    /// Chamado após `SkillRegistry::register(name)`.
    pub fn register_for_sync(&mut self, skill_name: &str) {
        self.pending_skills.push(String::from(skill_name));
    }

    /// Número de skills pendentes de sincronização.
    pub fn pending_count(&self) -> usize {
        self.pending_skills.len()
    }
}

// ─── Singleton global ───

lazy_static::lazy_static! {
    /// Instância global do SkillSync, acessível de qualquer lugar no crate hermes.
    pub static ref SKILL_SYNC: TicketLock<SkillSync> = TicketLock::new(SkillSync::new());
}

/// Atalho: marca uma skill para sync no próximo ciclo via singleton global.
/// Chamado após `SkillRegistry::register()`.
pub fn register_skill_for_sync(skill_name: &str) {
    SKILL_SYNC.lock().register_for_sync(skill_name);
}

/// Atalho: executa sync_skills no singleton global.
pub fn sync_skills(tick: u64) {
    SKILL_SYNC.lock().sync_skills(tick);
}

/// Atalho: retorna o número de skills pendentes no singleton global.
pub fn pending_sync_count() -> usize {
    SKILL_SYNC.lock().pending_count()
}

/// Atalho: marca o transporte P2P como ativo no singleton global.
/// Chamado pelo kernel quando há pelo menos um peer no mesh.
pub fn activate_global() {
    SKILL_SYNC.lock().activate();
}

/// Recebe um pacote NoProto do mesh e aplica skill enviada pelo Master.
/// Função livre chamada pelo kernel (dono único do RX P2P) com o payload
/// já fatiado (`data[PACKET_HEADER_SIZE..]`), formato "name\0desc".
pub fn on_packet_received(pkt: &AiosTaskPacket, data: &[u8]) {
    if pkt.task_type != TaskType::Sync || data.is_empty() {
        return;
    }
    // Parse payload como "name\0desc"
    let name = match data.iter().position(|&b| b == 0) {
        Some(i) => match core::str::from_utf8(&data[..i]) {
            Ok(s) => s,
            Err(_) => return,
        },
        None => return,
    };
    if name.is_empty() {
        return;
    }
    let desc = core::str::from_utf8(&data[name.len() + 1..]).unwrap_or("");

    let mut reg = k_nano::SKILL_REGISTRY.lock();
    if reg.has_skill(name) {
        slog_hermes!("SkillSync", "info", "Worker: skill '{}' ja existe", name);
        return;
    }
    reg.register(alloc::boxed::Box::new(
        skill_registry::DynamicSkill::new(name, desc, "synced from mesh master"),
    ));
    slog_hermes!("SkillSync", "info", "Worker: skill '{}' aplicada do Master", name);
}

// ─── Consumo via EventBus (SESSION_234) ───────────────────────────────────
// k_nano publica pacotes P2P não-heartbeat no tópico "P2P_PACKET". O bin
// chama `poll_p2p()` a cada tick (bei_tick) — subscribe lazy na 1ª chamada.

static RECV: Mutex<Option<event_bus::Receiver>> = Mutex::new(None);

/// Inscreve no tópico P2P_PACKET do EventBus (idempotente).
pub fn subscribe_p2p() {
    let mut recv = RECV.lock();
    if recv.is_none() {
        *recv = Some(k_nano::EVENT_BUS.subscribe(k_nano::net::mesh::TOPIC_P2P_PACKET));
        slog_hermes!("SkillSync", "info", "subscribed P2P_PACKET (EventBus)");
    }
}

/// Drena os pacotes P2P do EventBus e aplica skills enviadas pelo Master.
/// Self-activate no primeiro pacote válido.
pub fn poll_p2p() {
    subscribe_p2p();
    loop {
        let evt = RECV.lock().as_ref().and_then(|r| r.try_receive());
        let Some(evt) = evt else { break };
        if evt.topic != k_nano::net::mesh::TOPIC_P2P_PACKET {
            continue;
        }
        if let Some(pkt) = k_nano::net::udp_broadcast::parse(&evt.payload) {
            if pkt.task_type != TaskType::Sync {
                continue;
            }
            SKILL_SYNC.lock().activate();
            let payload = if evt.payload.len() > k_nano::net::noproto::PACKET_HEADER_SIZE {
                &evt.payload[k_nano::net::noproto::PACKET_HEADER_SIZE..]
            } else {
                &[][..]
            };
            on_packet_received(&pkt, payload);
        }
    }
}
