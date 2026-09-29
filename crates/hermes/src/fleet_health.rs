//! FLEET_HEALTH — agregação de frota no Master (SESSION_417).
//!
//! Cada nó publica seu veredito de máquina consolidado via mesh (`MCH\0` +
//! JSON de `k_nano::sys_health::machine_verdict_json`, TX do hermes::sys_health
//! com cooldown 10 s). Este módulo é o lado RX **no Master**: consome os
//! `MCH\0` que chegam pelo tópico `P2P_PACKET`, guarda o último snapshot por
//! nó (CAP 16 + evicção FIFO — runtime hygiene SESSION_410) e agrega o
//! worst-of da frota (`fleet_worst`), publicando `FLEET_HEALTH` no EventBus.
//! O LLM do Master recebe um prompt único de frota quando o overall degrada
//! (política EscalationState única — anti-loop SESSION_410).

use core::sync::atomic::{AtomicU64, Ordering};

use event_bus::{CapabilityToken, Event};

use k_nano::sys_health::{
    fleet_worst, machine_verdict_json, parse_machine_json, MachineHealth, Verdict,
    NO_GO_PERSIST_MIN, R_GPU_QUARANTINE, R_NET_LINK_DOWN, R_NET_SLIP_DEGRADED,
    R_STORAGE_FLUSH_FAIL, R_STORAGE_NO_PERSIST,
};

use k_nano::EVENT_BUS;
use k_nano::slog_hermes;

/// Tópico do agregado de frota (consumido pelo HUD/LLM do Master).
pub const TOPIC_FLEET_HEALTH: &str = "FLEET_HEALTH";

/// Prefixo mesh do veredito de máquina (espelho de hermes::sys_health).
const MESH_PREFIX_MCH: &[u8] = b"MCH\0";

/// CAP de nós rastreados (mesmo teto do PEER_KEYS do mesh — 16 nós).
const FLEET_CAP: usize = 16;

/// Cooldown de escala de frota: 60 s (600 ticks @100 Hz) — incidentes de
/// frota são raros e o LLM do Master é o recurso mais caro do sistema.
pub const FLEET_ESCALATION_COOLDOWN_TICKS: u64 = 600;

// ── Contadores lock-free (observabilidade) ──────────────────────────────────
pub static FLEET_MCH_RECV: AtomicU64 = AtomicU64::new(0);
pub static FLEET_MCH_BADJSON: AtomicU64 = AtomicU64::new(0);
pub static FLEET_AGGREGATIONS: AtomicU64 = AtomicU64::new(0);
pub static FLEET_ESCALATIONS: AtomicU64 = AtomicU64::new(0);

// ── Estado: último snapshot por nó (CAP + evicção FIFO) ─────────────────────

struct NodeHealth {
    /// node_id do mesh (u8 — mesmo id do heartbeat).
    node: u8,
    /// JSON bruto do último MCH do nó.
    json: alloc::string::String,
    /// Tick da última recepção (evicção FIFO por staleness).
    last_seen: u64,
}

/// CAP fixo — array de slots `None` (sem Vec ilimitado; runtime hygiene
/// SESSION_410, zero nova dependência).
static NODES: spin::Mutex<[Option<NodeHealth>; FLEET_CAP]> = {
    use spin::Mutex;
    Mutex::new([const { None }; FLEET_CAP])
};

/// Tick do último aggregate (para o HUD saber a idade do agregado).
pub static LAST_FLEET_JSON: spin::Mutex<Option<alloc::string::String>> =
    spin::Mutex::new(None);

/// Política de escala da frota (única — EscalationState de k_nano).
static FLEET_ESCALATION: spin::Mutex<Option<k_nano::sys_health::EscalationState>> =
    spin::Mutex::new(None);

/// Recebe um payload `MCH\0` + JSON vindo do P2P_PACKET (parse puro —
/// host-testável). Atualiza o snapshot do nó.
fn on_machine_health(payload: &[u8], now: u64) {
    let Some(json) = core::str::from_utf8(&payload[MESH_PREFIX_MCH.len()..]).ok() else {
        FLEET_MCH_BADJSON.fetch_add(1, Ordering::Relaxed);
        return;
    };
    // Valida ANTES de armazenar (não guarda lixo): parse do contrato fixo.
    if parse_machine_json(json).is_none() {
        FLEET_MCH_BADJSON.fetch_add(1, Ordering::Relaxed);
        return;
    }
    let (node, _overall, _triplets, _reasons) = parse_machine_json(json).unwrap();
    FLEET_MCH_RECV.fetch_add(1, Ordering::Relaxed);
    let mut nodes = NODES.lock();
    // Update in place ou ocupa slot livre; cheio → evicção do mais velho.
    if let Some(slot) = nodes.iter_mut().flatten().find(|n| n.node == node) {
        slot.json = alloc::string::String::from(json);
        slot.last_seen = now;
        return;
    }
    let entry = NodeHealth {
        node,
        json: alloc::string::String::from(json),
        last_seen: now,
    };
    if let Some(free) = nodes.iter_mut().find(|s| s.is_none()) {
        *free = Some(entry);
        return;
    }
    // Evicção: o mais velho (menor last_seen).
    let oldest = nodes
        .iter()
        .enumerate()
        .filter_map(|(i, s)| s.as_ref().map(|n| (i, n.last_seen)))
        .min_by_key(|(_, t)| *t)
        .map(|(i, _)| i)
        .unwrap_or(0);
    nodes[oldest] = Some(entry);
}

/// Consome P2P_PACKET (chamado pelo bin no mesmo loop do skill_sync::poll_p2p).
pub fn poll_p2p(now: u64) {
    static RECV: spin::Mutex<Option<event_bus::Receiver>> = spin::Mutex::new(None);
    let mut guard = RECV.lock();
    if guard.is_none() {
        *guard = Some(EVENT_BUS.subscribe(k_nano::net::mesh::TOPIC_P2P_PACKET));
        slog_hermes!("FleetHealth", "info", "subscribed P2P_PACKET");
    }
    loop {
        let Some(evt) = guard.as_ref().and_then(|r| r.try_receive()) else {
            break;
        };
        if evt.topic != k_nano::net::mesh::TOPIC_P2P_PACKET {
            continue;
        }
        let Some(pkt) = k_nano::net::udp_broadcast::parse(&evt.payload) else {
            continue;
        };
        if pkt.task_type != k_nano::net::noproto::TaskType::Sync {
            continue;
        }
        let payload = if evt.payload.len() > k_nano::net::noproto::PACKET_HEADER_SIZE {
            &evt.payload[k_nano::net::noproto::PACKET_HEADER_SIZE..]
        } else {
            continue;
        };
        if payload.starts_with(MESH_PREFIX_MCH) {
            on_machine_health(payload, now);
        }
    }
}

/// Consolida e publica o agregado de frota se há ≥1 snapshot válido.
/// Retorna o JSON do agregado (para testes).
pub fn aggregate(now: u64) -> Option<alloc::string::String> {
    let nodes = NODES.lock();
    let live: alloc::vec::Vec<&NodeHealth> = nodes.iter().flatten().collect();
    if live.is_empty() {
        return None;
    }
    let machines: alloc::vec::Vec<MachineHealth> = live
        .iter()
        .filter_map(|n| {
            let (node, _o, triplets, reasons) = parse_machine_json(&n.json)?;
            // Reconstrói domínios do contrato fixo (sys/audio).
            let mut domains = alloc::vec::Vec::new();
            for dom in ["sys", "audio"] {
                let fields: alloc::vec::Vec<(&'static str, Verdict)> = triplets
                    .iter()
                    .filter(|(_, d, _)| *d == dom)
                    .map(|(f, _, v)| (*f, *v))
                    .collect();
                if fields.is_empty() {
                    continue;
                }
                // Razões globais do snapshot (contrato: razões são do nó inteiro).
                let dom_reasons: alloc::vec::Vec<&'static str> = reasons
                    .iter()
                    .filter_map(|r| {
                        const KNOWN: [&str; 9] = [
                            R_NET_LINK_DOWN,
                            R_NET_SLIP_DEGRADED,
                            R_STORAGE_NO_PERSIST,
                            R_STORAGE_FLUSH_FAIL,
                            R_GPU_QUARANTINE,
                            "SD_PLAY_DMA_STUCK",
                            "SD_CAPTURE_LPIB_STALE",
                            "PLAYBACK_DROPPED",
                            "PLAYBACK_PATH_ABSENT",
                        ];
                        KNOWN.iter().copied().find(|k| k == &r.as_str())
                    })
                    .collect();
                domains.push((dom, k_nano::sys_health::DomainVerdicts { fields, reasons: dom_reasons }));
            }
            Some(MachineHealth { node, domains })
        })
        .collect();
    if machines.is_empty() {
        return None;
    }
    let fleet = fleet_worst(&machines);
    let json = machine_verdict_json(&fleet);
    *LAST_FLEET_JSON.lock() = Some(json.clone());
    FLEET_AGGREGATIONS.fetch_add(1, Ordering::Relaxed);
    let _ = EVENT_BUS.publish(Event {
        id: 0,
        topic: alloc::string::String::from(TOPIC_FLEET_HEALTH),
        payload: json.clone().into_bytes(),
        token: CapabilityToken::Legacy(1),
    });
    // Escala de frota (política única): NO_GO persistente → 1 prompt ao LLM.
    let worst = fleet.overall();
    let reason: Option<&'static str> = fleet.all_reasons().first().copied();
    {
        let mut esc = FLEET_ESCALATION.lock();
        let st = esc.get_or_insert_with(k_nano::sys_health::EscalationState::default);
        if let Some(r) = st.observe(worst, reason) {
            FLEET_ESCALATIONS.fetch_add(1, Ordering::Relaxed);
            let prompt = alloc::format!(
                "FLEET_HEALTH consolidada (worst-of de {} nós): {} em NO_GO (razão: {}). \
Diagnostique a causa raiz na frota e proponha correção. Snapshot: {}",
                machines.len(),
                "frota",
                r,
                json
            );
            let _ = EVENT_BUS.publish(Event {
                id: 0,
                topic: alloc::string::String::from(crate::hermes::TOPIC_USER_INTENT),
                payload: prompt.into_bytes(),
                token: CapabilityToken::Legacy(1),
            });
            k_nano::slog_hermes!(
                "FleetHealth", "ok",
                "escalate LLM frota reason={} nodes={} escalations={}",
                r, machines.len(), FLEET_ESCALATIONS.load(Ordering::Relaxed)
            );
        }
    }
    Some(json)
}

/// JSON do último agregado (HUD do Master lê sem re-parse do bus).
pub fn last_fleet_json() -> Option<alloc::string::String> {
    LAST_FLEET_JSON.lock().clone()
}

/// Número de nós com snapshot vivo (telemetria).
pub fn tracked_nodes() -> usize {
    NODES.lock().iter().flatten().count()
}

/// Serializa testes (statics globais compartilhados — lição SESSION_346:
/// `boot_observe` falha em run paralelo).
#[cfg(test)]
static TEST_LOCK: spin::Mutex<()> = spin::Mutex::new(());

/// Limpa estado global (usado só por testes).
#[cfg(test)]
fn reset_state() {
    NODES.lock().iter_mut().for_each(|s| *s = None);
    *LAST_FLEET_JSON.lock() = None;
    *FLEET_ESCALATION.lock() = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON_OK: &str = "{\"node\":3,\"overall\":\"GO\",\"sys\":{\"net\":\"GO\",\"storage\":\"GO\",\"gpu\":\"UNKNOWN\"},\"audio\":{\"playback\":\"GO\",\"capture\":\"GO\"},\"reasons\":[]}";
    const JSON_NOGO: &str = "{\"node\":7,\"overall\":\"NO_GO\",\"sys\":{\"net\":\"NO_GO\",\"storage\":\"GO\",\"gpu\":\"UNKNOWN\"},\"audio\":{\"playback\":\"GO\",\"capture\":\"GO\"},\"reasons\":[\"NET_LINK_DOWN\"]}";

    fn mch(json: &str) -> alloc::vec::Vec<u8> {
        let mut p = alloc::vec::Vec::new();
        p.extend_from_slice(MESH_PREFIX_MCH);
        p.extend_from_slice(json.as_bytes());
        p
    }

    #[test]
    fn mch_valido_arma_snapshot_e_agrega() {
        let _g = TEST_LOCK.lock();
        reset_state();
        on_machine_health(&mch(JSON_OK), 100);
        on_machine_health(&mch(JSON_NOGO), 110);
        assert_eq!(tracked_nodes(), 2);
        let agg = aggregate(120).unwrap();
        assert!(agg.contains("\"node\":0"));
        // worst-of: net = NO_GO do nó 7.
        assert!(agg.contains("\"net\":\"NO_GO\""));
        assert!(agg.contains("NET_LINK_DOWN"));
    }

    #[test]
    fn mch_malformado_nao_entra() {
        let _g = TEST_LOCK.lock();
        reset_state();
        let before = FLEET_MCH_BADJSON.load(Ordering::Relaxed);
        on_machine_health(b"MCH\0{lixo", 100);
        assert_eq!(FLEET_MCH_BADJSON.load(Ordering::Relaxed), before + 1);
        assert_eq!(tracked_nodes(), 0);
    }

    #[test]
    fn update_no_mesmo_no_substitui() {
        let _g = TEST_LOCK.lock();
        reset_state();
        on_machine_health(&mch(JSON_OK), 100);
        on_machine_health(&mch(JSON_OK), 200);
        assert_eq!(tracked_nodes(), 1);
    }

    #[test]
    fn frota_so_go_nao_escala() {
        let _g = TEST_LOCK.lock();
        reset_state();
        let before = FLEET_ESCALATIONS.load(Ordering::Relaxed);
        on_machine_health(&mch(JSON_OK), 100);
        for t in 0..(NO_GO_PERSIST_MIN as u64 + 10) {
            aggregate(200 + t * 100);
        }
        // snapshot não muda → depois da primeira observação, streak cresce mas
        // GO zera... GO não zera aqui porque aggregate não repõe — o snapshot
        // permanece GO, então nunca passa do piso (streak só conta NO_GO).
        assert_eq!(FLEET_ESCALATIONS.load(Ordering::Relaxed), before);
    }
}
