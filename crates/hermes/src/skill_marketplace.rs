//! Skill Marketplace — descoberta automatica de skills entre nos via broadcast.
//!
//! ## Modelo
//! Diferente do mesh (que requer entrada no cluster), o marketplace e uma
//! descoberta passiva: cada no anuncia suas skills via NoProto broadcast.
//! Os nos ouvintes recebem os anuncios sem se juntar ao mesh.
//!
//! ## Fluxo
//! 1. No A envia SkillOffer broadcast: "tenho skill monitor-X v2"
//! 2. No B recebe, compara com skills locais: "mesmo foco, versao diferente"
//! 3. Jarbas assessora: "achei skill que otimiza monitoramento em 15%... quer testar?"
//! 4. Se usuario aprova → sandbox → teste → se ok → adota
//!
//! ## Depende de: P2P Transport (ADR-81 Fase A)
//! - udp_broadcast::send() / recv() para troca de SkillOffers
//! - EventBus para publicar PENDING_SKILL_OFFER → Jarbas assessora
//! - wasmi sandbox para testar skill recebida sem risco
//!
//! ## Fallback (sem P2P)
//! Sem P2P, o marketplace fica inativo — cada no mantem so suas skills locais.

use alloc::string::String;
use alloc::vec::Vec;
use k_nano::net::noproto::{AiosTaskPacket, TaskType, PacketFlags};
use k_nano::net::udp_broadcast;
use event_bus::{Event, CapabilityToken};
use crate::globals::EVENT_BUS;

/// Topico do EventBus para ofertas de skill recebidas.
pub const TOPIC_SKILL_OFFER: &str = "SKILL_OFFER";
/// Topico para resposta de skill aprovada pelo HITL.
pub const TOPIC_SKILL_OFFER_RESPONSE: &str = "SKILL_OFFER_RESPONSE";

/// Informacao basica de uma skill oferecida.
#[derive(Debug, Clone)]
pub struct SkillOffer {
    pub node_id: u8,
    pub skill_name: String,
    pub version: String,
    pub category: String,
    pub description: String,
    pub improvement_pct: f32,
    pub wasm_hash: [u8; 32],
    pub payload: Vec<u8>, // WASM binary ou manifest
}

impl SkillOffer {
    /// Estima se a skill oferecida melhora uma skill local existente.
    pub fn improves(&self, local_name: &str, local_version: &str) -> Option<f32> {
        if self.skill_name == local_name && self.version.as_str() > local_version {
            Some(self.improvement_pct)
        } else {
            None
        }
    }
}

/// Marketplace Agent — descoberta de skills sem entrar no mesh.
pub struct MarketplaceAgent {
    active: bool,
    local_skills: Vec<(String, String)>, // (name, version)
    incoming_offers: Vec<SkillOffer>,
}

impl MarketplaceAgent {
    pub fn new() -> Self {
        MarketplaceAgent {
            active: false,
            local_skills: Vec::new(),
            incoming_offers: Vec::new(),
        }
    }

    /// Ativa o marketplace (requer P2P transport).
    pub fn activate(&mut self) { self.active = true; }
    pub fn deactivate(&mut self) { self.active = false; }
    pub fn is_active(&self) -> bool { self.active }

    /// Registra uma skill local para anunciar.
    pub fn register_local_skill(&mut self, name: &str, version: &str) {
        self.local_skills.push((String::from(name), String::from(version)));
    }

    /// Envia anuncio das skills locais via broadcast NoProto (TaskType::ModelUpdate).
    /// Depende de: P2P Transport (ADR-81 Fase A) — udp_broadcast_send() do kernel.
    pub fn broadcast_offer(&self, node_id: u8) {
        if !self.active {
            return; // fallback: sem P2P, nao anuncia
        }
        for (name, version) in &self.local_skills {
            let pkt = AiosTaskPacket::new(
                0, node_id, 0xFF, TaskType::ModelUpdate, 1, 0, 0, PacketFlags::new(),
            );
            let mut buf = udp_broadcast::serialize(&pkt);
            let payload = alloc::format!("{}|{}|{}|{}", name, version, "general", "skill offer via mesh").into_bytes();
            buf.extend_from_slice(&payload);
            let sent = k_nano::net::udp_broadcast::udp_broadcast_send(&buf, 42069);
            k_nano::slog_nano!("MKTP", "info",
                "broadcast skill '{}' v{} from node {} sent={}", name, version, node_id, sent);
        }
    }

    /// Processa um pacote NoProto recebido como SkillOffer.
    /// Se for melhoria vs skill local, publica PENDING_SKILL_OFFER → Jarbas assessora.
    pub fn on_skill_offer(&mut self, packet: &AiosTaskPacket, payload: &[u8]) {
        if !self.active {
            return;
        }
        // Parse payload como "name|version|category|description"
        let text = core::str::from_utf8(payload).unwrap_or("");
        let mut fields = text.split('|');
        let skill_name = fields.next().filter(|s| !s.is_empty()).unwrap_or("unknown");
        let version = fields.next().filter(|s| !s.is_empty()).unwrap_or("0.0.0");
        let category = fields.next().filter(|s| !s.is_empty()).unwrap_or("general");
        let description = fields.next().filter(|s| !s.is_empty())
            .unwrap_or("Skill offer received via P2P marketplace");

        let offer = SkillOffer {
            node_id: packet.source_id,
            skill_name: String::from(skill_name),
            version: String::from(version),
            category: String::from(category),
            description: String::from(description),
            improvement_pct: 0.0,
            wasm_hash: [0; 32],
            payload: payload.to_vec(),
        };

        // Verifica se melhora alguma skill local
        for (local_name, local_ver) in &self.local_skills {
            if let Some(pct) = offer.improves(local_name, local_ver) {
                // Jarbas HITL: publica evento para JarbasAgent assessorar
                let msg = alloc::format!(
                    "[MKTP] achei '{}' v{} (melhora '{}' em {:.0}%) — quer testar em sandbox?",
                    offer.skill_name, offer.version, local_name, pct
                );
                k_nano::slog_nano!("MKTP", "info", "{}", msg);
                let _ = EVENT_BUS.publish(Event {
                    id: 0,
                    topic: String::from(TOPIC_SKILL_OFFER),
                    payload: msg.into_bytes(),
                    token: CapabilityToken::Legacy(1),
                });
                self.incoming_offers.push(offer);
                return;
            }
        }
        // Skill nova (sem correspondente local): oferece como descoberta
        let msg = alloc::format!(
            "[MKTP] nova skill '{}' v{} (categoria: {}) — conhecer?",
            offer.skill_name, offer.version, offer.category
        );
        let _ = EVENT_BUS.publish(Event {
            id: 0,
            topic: String::from(TOPIC_SKILL_OFFER),
            payload: msg.into_bytes(),
            token: CapabilityToken::Legacy(1),
        });
    }

    /// Processa resposta do HITL (aprovado/recusado).
    pub fn on_hitl_response(&mut self, approved: bool, skill_name: &str) {
        if approved {
            k_nano::slog_nano!("MKTP", "info",
                "HITL aprovou skill '{}' — promovendo para SkillRegistry", skill_name);
            // ponytail: copiar WASM para SkillRegistry + registrar
        } else {
            k_nano::slog_nano!("MKTP", "info",
                "HITL recusou skill '{}' — oferta descartada", skill_name);
        }
    }

    /// Retorna ofertas pendentes de aprovacao HITL.
    pub fn pending_offers(&self) -> &[SkillOffer] { &self.incoming_offers }
}

/// Static global do marketplace.
use spin::Mutex;
pub static MARKETPLACE: Mutex<Option<MarketplaceAgent>> = Mutex::new(None);

pub fn init() {
    *MARKETPLACE.lock() = Some(MarketplaceAgent::new());
}

/// Processa pacote NoProto recebido: se for SkillOffer (ModelUpdate), roteia
/// para o Marketplace. Chamado pelo kernel com o payload já fatiado
/// (`data[PACKET_HEADER_SIZE..]`), então NÃO re-fatia aqui.
pub fn on_packet_received(packet: &AiosTaskPacket, data: &[u8]) {
    // Gate: só processa ofertas de skills (ModelUpdate)
    if packet.task_type != TaskType::ModelUpdate {
        return;
    }
    if let Some(ref mut mp) = *MARKETPLACE.lock() {
        mp.on_skill_offer(packet, data);
    }
}

/// Atalho: lazy-init do marketplace + ativa. Chamado pelo kernel quando há peer.
pub fn activate_global() {
    let mut guard = MARKETPLACE.lock();
    if guard.is_none() {
        *guard = Some(MarketplaceAgent::new());
    }
    if let Some(ref mut mp) = *guard {
        mp.activate();
    }
}

/// Atalho: lazy-init + registra skill local para anunciar. Chamado pelo kernel.
pub fn register_skill(name: &str, version: &str) {
    let mut guard = MARKETPLACE.lock();
    if guard.is_none() {
        *guard = Some(MarketplaceAgent::new());
    }
    if let Some(ref mut mp) = *guard {
        mp.register_local_skill(name, version);
    }
}

/// Tick do marketplace: lazy-init + broadcast das skills locais com throttle
/// (~200 ticks). Chamado pelo bin a cada tick (bei_tick).
pub fn marketplace_tick(node_id: u8) {
    static CALLS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
    static LAST: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
    let n = CALLS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    if n.wrapping_sub(LAST.load(core::sync::atomic::Ordering::Relaxed)) < 200 {
        return;
    }
    LAST.store(n, core::sync::atomic::Ordering::Relaxed);
    let mut guard = MARKETPLACE.lock();
    if guard.is_none() {
        *guard = Some(MarketplaceAgent::new());
    }
    if let Some(ref mut mp) = *guard {
        mp.broadcast_offer(node_id);
    }
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
        k_nano::slog_nano!("MKTP", "info", "subscribed P2P_PACKET (EventBus)");
    }
}

/// Drena os pacotes P2P do EventBus e processa ofertas de skills.
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
            if pkt.task_type != TaskType::ModelUpdate {
                continue;
            }
            // Self-activate: oferta recebida ⇒ há peers anunciando.
            if let Some(ref mut mp) = *MARKETPLACE.lock() {
                mp.activate();
            }
            let payload = if evt.payload.len() > k_nano::net::noproto::PACKET_HEADER_SIZE {
                &evt.payload[k_nano::net::noproto::PACKET_HEADER_SIZE..]
            } else {
                &[][..]
            };
            on_packet_received(&pkt, payload);
        }
    }
}
