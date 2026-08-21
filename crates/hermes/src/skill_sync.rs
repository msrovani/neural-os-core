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
        if self.active {
            return;
        }
        self.active = true;
        // ADR-0081 C3: conhecimento via mesh ativa junto (peer presente).
        crate::mesh_knowledge::mark_active();
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

        // Master só empurra após TOFU settle (peer já tem nossa pk).
        if role == NodeRole::Master && !mesh::tofu_settled() {
            return;
        }

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
                // Worker push: promove skill local para o Master via NoProto Sync
                // payload "PROMOTE\0{name}\0{desc}". SESSION_235: era log-only.
                if !self.pending_skills.is_empty() {
                    let name = self.pending_skills.remove(0);
                    // Desc do SkillRegistry canônico ("name: desc").
                    let desc = {
                        let reg = k_nano::SKILL_REGISTRY.lock();
                        let mut d = String::new();
                        for (entry, _pol) in reg.list_skills() {
                            if let Some((n, desc)) = entry.split_once(": ") {
                                if n == name {
                                    d = String::from(desc);
                                    break;
                                }
                            }
                        }
                        d
                    };
                    let ok = self.broadcast_promote(&name, &desc);
                    slog_hermes!(
                        "SkillSync", "info",
                        "Worker: promote skill='{}' broadcast={}", name, ok
                    );
                    if !ok {
                        // Sem envio (ex. sem NIC) — re-tenta na próxima sync.
                        self.pending_skills.push(name);
                    }
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

    /// Master push: serializa "name\0desc" (ou "SKILL\0name\0desc\0body" quando
    /// o corpo SKILL.md está disponível — ADR-0081 C3) num NoProto TaskType::Sync
    /// e faz broadcast UDP na porta P2P (42069) via transporte k_nano (R0).
    /// Fase A (SESSION_236): assinado — o RX fail-closed dropa não-assinados.
    /// Retorna `true` se o envio foi ok.
    fn broadcast_skill(&mut self, name: &str, desc: &str) -> bool {
        // ID único por instância (último octeto do IP) — SESSION_234.
        let node_id = k_nano::net::mesh::node_id();
        // ADR-0081 follow-up: clock monotônico único por fonte (anti-replay).
        let pkt = AiosTaskPacket::new(
            k_nano::net::mesh::next_data_clock(), node_id, 0xFF, TaskType::Sync, 1, 0, 0, PacketFlags::new(),
        );
        let mut buf = k_nano::net::udp_broadcast::serialize(&pkt);
        let payload = match skill_body(name) {
            // Corpo disponível → formato novo com SKILL.md (compat: RX antigo
            // ignora desconhecido; RX novo aplica o corpo real).
            Some(body) => alloc::format!("SKILL\0{}\0{}\0{}", name, desc, body).into_bytes(),
            // Sem corpo → formato legado "name\0desc".
            None => alloc::format!("{}\0{}", name, desc).into_bytes(),
        };
        buf.extend_from_slice(&payload);
        let Some(signed) = k_nano::net::udp_broadcast::sign_packet(&buf) else {
            slog_hermes!("SkillSync", "info", "Master: push skill='{}' sem sessao - skip", name);
            return false;
        };
        let ok = k_nano::net::udp_broadcast::udp_broadcast_send(&signed, 42069);
        slog_hermes!(
            "SkillSync", "info",
            "Master: push skill='{}' broadcast={} bytes={}", name, ok, payload.len()
        );
        ok
    }

    /// Worker push: serializa "PROMOTE\0{name}\0{desc}" num NoProto Sync e faz
    /// broadcast UDP na porta P2P (42069). O Master aplica via on_packet_received.
    /// Fase A (SESSION_236): assinado — o RX fail-closed dropa não-assinados.
    fn broadcast_promote(&self, name: &str, desc: &str) -> bool {
        let node_id = k_nano::net::mesh::node_id();
        // ADR-0081 follow-up: clock monotônico único por fonte (anti-replay).
        let pkt = AiosTaskPacket::new(
            k_nano::net::mesh::next_data_clock(), node_id, 0xFF, TaskType::Sync, 1, 0, 0, PacketFlags::new(),
        );
        let mut buf = k_nano::net::udp_broadcast::serialize(&pkt);
        let payload = alloc::format!("PROMOTE\0{}\0{}", name, desc).into_bytes();
        buf.extend_from_slice(&payload);
        let Some(signed) = k_nano::net::udp_broadcast::sign_packet(&buf) else {
            return false;
        };
        k_nano::net::udp_broadcast::udp_broadcast_send(&signed, 42069)
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

/// Busca o corpo (SKILL.md) de uma skill para o push com corpo (ADR-0081 C3).
/// Ordem: PACKAGE_HUB → SKILL_STORAGE → registry. O registry (skill-registry)
/// não expõe o corpo (só manifest name/description) — se nada achar, retorna
/// None e o Master mantém o formato legado "name\0desc".
fn skill_body(name: &str) -> Option<String> {
    {
        let hub = crate::package_hub::PACKAGE_HUB.lock();
        if let Some(p) = hub.get(crate::package_hub::PackageKind::Skill, name) {
            if !p.body.trim().is_empty() {
                return Some(p.body.clone());
            }
        }
    }
    {
        let storage = crate::globals::SKILL_STORAGE.lock();
        for s in &storage.skills {
            if s.name == name {
                return Some(s.to_skill_md());
            }
        }
    }
    None
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

/// Limpa a lista `synced` para o Master re-empurrar skills após TOFU settle
/// (skills enviadas antes do peer vincular pk eram dropadas e marcadas synced).
pub fn clear_synced_for_resync() {
    let mut sync = SKILL_SYNC.lock();
    sync.synced.clear();
    slog_hermes!(
        "SkillSync", "info",
        "synced limpo — re-push apos TOFU settle"
    );
}

/// GOAL3 mesh smoke: Master registra UMA skill nova pós-settle (não nativa).
/// Worker ainda não a tem → deve logar `aplicada do Master`. Só Master; 1x.
pub fn register_mesh_g3_probe_on_master() {
    if mesh::local_role() != NodeRole::Master {
        return;
    }
    const NAME: &str = "mesh_g3_probe";
    const DESC: &str = "GOAL3 SkillSync probe (Master-only post-TOFU)";
    {
        let mut reg = k_nano::SKILL_REGISTRY.lock();
        if reg.has_skill(NAME) {
            return;
        }
        reg.register(alloc::boxed::Box::new(skill_registry::DynamicSkill::new(
            NAME,
            DESC,
            "mesh_g3_probe body — smoke SkillSync apply",
        )));
    }
    // Garante push no próximo sync_skills (e no re-push pós clear_synced).
    register_skill_for_sync(NAME);
    slog_hermes!(
        "SkillSync",
        "info",
        "Master: skill '{}' registrada pos-TOFU (GOAL3 probe)",
        NAME
    );
}

/// Recebe um pacote NoProto do mesh e aplica skill enviada pelo Master.
/// Função livre chamada pelo kernel (dono único do RX P2P) com o payload
/// já fatiado (`data[PACKET_HEADER_SIZE..]`), formato "name\0desc".
///
/// SESSION_235: payload "PROMOTE\0{name}\0{desc}" (Worker → Master) é tratado
/// ANTES do fluxo normal de push — só o Master aplica.
pub fn on_packet_received(pkt: &AiosTaskPacket, data: &[u8]) {
    if pkt.task_type != TaskType::Sync || data.is_empty() {
        return;
    }

    // ── Guards de protocolos paralelos (não-skills) ──
    // MEM\0/SOUL\0/PERS\0 = conhecimento (mesh_knowledge, subscribe próprio);
    // FED\0/FEDW\0 = aprendizado federado (cortex::federated). Sem o guard o
    // parse genérico "name\0desc" registraria skills-lixo (ex. "FED").
    if data.starts_with(b"MEM\0")
        || data.starts_with(b"SOUL\0")
        || data.starts_with(b"PERS\0")
        || data.starts_with(b"FED\0")
        || data.starts_with(b"FEDW\0")
    {
        return;
    }

    // ── SKILL\0name\0desc\0body — Master push com corpo (ADR-0081 C3) ──
    // Corpo real do SKILL.md em vez de "synced from mesh master".
    if data.starts_with(b"SKILL\0") {
        let mut parts = data[6..].splitn(3, |&b| b == 0);
        let name = match parts.next().and_then(|s| core::str::from_utf8(s).ok()) {
            Some(n) if !n.is_empty() => n,
            _ => return,
        };
        let desc = parts
            .next()
            .map(|s| core::str::from_utf8(s).unwrap_or(""))
            .unwrap_or("");
        let body = parts
            .next()
            .map(|s| core::str::from_utf8(s).unwrap_or(""))
            .unwrap_or("");

        let mut reg = k_nano::SKILL_REGISTRY.lock();
        if reg.has_skill(name) {
            slog_hermes!("SkillSync", "info", "Worker: skill '{}' ja existe (SKILL\\0)", name);
            return;
        }
        reg.register(alloc::boxed::Box::new(skill_registry::DynamicSkill::new(
            name, desc, body,
        )));
        crate::self_evolve::publish_change("mesh", name);
        slog_hermes!(
            "SkillSync", "info",
            "Worker: skill '{}' aplicada do Master (SKILL\\0, {} bytes)", name, body.len()
        );
        return;
    }

    // ── PROMOTE\0name\0desc — Worker → Master ──
    if data.starts_with(b"PROMOTE\0") {
        // Só o Master aplica promotes (payload é para ele).
        if mesh::local_role() != NodeRole::Master {
            return;
        }
        let mut parts = data[8..].splitn(2, |&b| b == 0);
        let name = match parts.next().and_then(|s| core::str::from_utf8(s).ok()) {
            Some(n) if !n.is_empty() => n,
            _ => return,
        };
        let desc = parts
            .next()
            .map(|s| core::str::from_utf8(s).unwrap_or(""))
            .unwrap_or("");

        let mut reg = k_nano::SKILL_REGISTRY.lock();
        if reg.has_skill(name) {
            slog_hermes!("SkillSync", "info", "Master: skill '{}' ja existe (promote ignorado)", name);
            return;
        }
        reg.register(alloc::boxed::Box::new(
            skill_registry::DynamicSkill::new(name, desc, "promoted from mesh worker"),
        ));
        slog_hermes!(
            "SkillSync", "info",
            "Master: skill '{}' promovida do Worker node={}", name, pkt.source_id
        );
        return;
    }

    // ── Push normal do Master: "name\0desc" ──
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
    // ADR-0081 C3: conhecimento via mesh (memórias SGDB + persona coletiva).
    // Cada módulo tem subscribe próprio (EventBus = fila por assinante); o bin
    // só chama `skill_sync::poll_p2p()` por tick — repassa sem editar o bin.
    crate::mesh_knowledge::poll_p2p();
}
