//! ADR-0081 C3 — Conhecimento via mesh: memórias SGDB (NMD1), persona coletiva
//! (SOUL.md/PERSONA.md) e contadores de diagnóstico.
//!
//! Consome o mesmo tópico P2P_PACKET do skill_sync (subscribe lazy próprio +
//! dreno try_receive). Self-activate no 1º pacote válido; TX gated por
//! `is_active()` (ativado junto com o skill_sync via `mark_active()` — o bin
//! chama `skill_sync::activate_global()` quando há ≥1 peer).
//!
//! ## Wire (sem editar o bin)
//! `skill_sync::poll_p2p()` (chamado a cada tick pelo bin) repassa para
//! `crate::mesh_knowledge::poll_p2p()` — cada módulo tem subscribe próprio
//! (EventBus dá fila por assinante).

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use k_ai::sgdb::{MemoryDoc, MemoryLayer, VectorClock};
use k_nano::net::mesh;
use k_nano::net::noproto::{AiosTaskPacket, TaskType};
use k_nano::slog_hermes;
use spin::Mutex;

/// Prefixos de payload (não colidem com PK/ROLE/PROMOTE/CHK/CAP do mesh).
const PREFIX_MEM: &[u8] = b"MEM\0";
const PREFIX_SOUL: &[u8] = b"SOUL\0";
const PREFIX_PERS: &[u8] = b"PERS\0";

/// Ativo após o 1º pacote válido OU quando o skill_sync ativa (peer presente).
static ACTIVE: AtomicBool = AtomicBool::new(false);
/// Memórias (MemoryDoc NMD1) aplicadas do mesh — diagnóstico.
static MEMORY_DOCS_SYNCED: AtomicU64 = AtomicU64::new(0);
/// Syncs de persona (SOUL/PERSONA) aplicadas do mesh — diagnóstico.
static PERSONA_SYNCS: AtomicU64 = AtomicU64::new(0);

/// `true` quando o mesh de conhecimento está ativo (gate dos hooks TX).
pub fn is_active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

/// Marca o mesh como ativo. Chamado por `skill_sync::activate()` (o bin ativa
/// quando há ≥1 peer) e self-activate no 1º pacote RX.
pub(crate) fn mark_active() {
    ACTIVE.store(true, Ordering::Relaxed);
}

// ─── TX ────────────────────────────────────────────────────────────────────

/// (a) Broadcast de um MemoryDoc: payload `"MEM\0" + NMD1` via mesh_send_large
/// (chunking automático no k_nano para payloads > 1100 bytes). Best-effort.
pub fn broadcast_memory_doc(doc: &MemoryDoc) -> bool {
    let enc = doc.encode();
    let mut payload = Vec::with_capacity(PREFIX_MEM.len() + enc.len());
    payload.extend_from_slice(PREFIX_MEM);
    payload.extend_from_slice(&enc);
    let ok = mesh::mesh_send_large(&payload);
    slog_hermes!(
        "MeshKnowledge", "info",
        "TX MemoryDoc layer={} key='{}' bytes={} ok={}",
        doc.layer.as_str(), doc.key, payload.len(), ok
    );
    ok
}

/// TX helper: monta MemoryDoc L3 episódica (key = timestamp sortable) a partir
/// de um fato e faz broadcast. Chamado por `memory_store::remember()`.
pub fn broadcast_fact(fact: &str) -> bool {
    if !is_active() {
        return false;
    }
    let ts = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
    let mut doc = MemoryDoc::new(
        MemoryLayer::L3EpisodicLong,
        &MemoryDoc::sortable_ts_key(ts),
        fact.as_bytes().to_vec(),
    );
    doc.clock.tick(mesh::node_id());
    broadcast_memory_doc(&doc)
}

/// (c) Broadcast de SOUL.md + PERSONA.md (`"SOUL\0{...}"` / `"PERS\0{...}"`).
/// Chamado best-effort em `memory_store::ensure_defaults()` e após
/// `write_soul`/`write_persona`. Anti-loop fica no RX (diff-check).
pub fn broadcast_persona() -> bool {
    if !is_active() {
        return false;
    }
    let mut ok = true;
    let soul = crate::memory_store::read_soul();
    if !soul.trim().is_empty() {
        let mut p = Vec::with_capacity(PREFIX_SOUL.len() + soul.len());
        p.extend_from_slice(PREFIX_SOUL);
        p.extend_from_slice(soul.as_bytes());
        ok = mesh::mesh_send_large(&p) && ok;
    }
    let pers = crate::memory_store::read_persona();
    if !pers.trim().is_empty() {
        let mut p = Vec::with_capacity(PREFIX_PERS.len() + pers.len());
        p.extend_from_slice(PREFIX_PERS);
        p.extend_from_slice(pers.as_bytes());
        ok = mesh::mesh_send_large(&p) && ok;
    }
    ok
}

// ─── Memória coletiva do learner (k_ai::self_learning) ─────────────────────

/// (d) Difunde os pares aprendidos pelo SelfLearningAgent como MemoryDocs
/// L4Semantic (`learner/mesh/{i}`, payload `input\0output`) — memória coletiva:
/// o RX de `"MEM\0"` de outro nó aplica via `put_doc` (que cobre L0–L7, incl.
/// L4 — sem filtro de layer no on_memory_doc). Throttled ~500 ticks.
///
/// Lê os pares via `collector.snapshot()` (API pública do singleton) — o getter
/// `learned_pairs()` da lane paralela ainda não existe; quando chegar, trocar a
/// leitura sem mudar o resto (sem conflito de compilação).
pub fn broadcast_learner_memory() -> bool {
    if !is_active() {
        return false;
    }
    static LAST_BROADCAST: AtomicU64 = AtomicU64::new(0);
    let now = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
    let last = LAST_BROADCAST.load(Ordering::Relaxed);
    if last != 0 && now.wrapping_sub(last) < 500 {
        return false;
    }
    LAST_BROADCAST.store(now, Ordering::Relaxed);
    if k_ai::self_learning::learned_pairs_count() == 0 {
        return false;
    }
    let pairs = k_ai::self_learning::learner_global()
        .lock()
        .as_ref()
        .map(|a| a.collector.snapshot())
        .unwrap_or_default();
    let mut n = 0usize;
    for (i, pair) in pairs.iter().rev().take(8).enumerate() {
        let mut payload = Vec::with_capacity(pair.input.len() + pair.output.len() + 1);
        payload.extend_from_slice(pair.input.as_bytes());
        payload.push(0);
        payload.extend_from_slice(pair.output.as_bytes());
        let mut doc = MemoryDoc::new(
            MemoryLayer::L4Semantic,
            &alloc::format!("learner/mesh/{}", i),
            payload,
        );
        doc.clock.tick(mesh::node_id());
        if broadcast_memory_doc(&doc) {
            n += 1;
        }
    }
    if n > 0 {
        slog_hermes!(
            "MeshKnowledge", "info",
            "TX {} pares do learner como L4 (memória coletiva)",
            n
        );
    }
    n > 0
}

// ─── Contadores de diagnóstico ─────────────────────────────────────────────

pub fn memory_docs_synced() -> u64 {
    MEMORY_DOCS_SYNCED.load(Ordering::Relaxed)
}

pub fn persona_syncs() -> u64 {
    PERSONA_SYNCS.load(Ordering::Relaxed)
}

// ─── RX (EventBus P2P_PACKET — padrão skill_sync::subscribe_p2p) ───────────

static RECV: Mutex<Option<event_bus::Receiver>> = Mutex::new(None);

/// Inscreve no tópico P2P_PACKET do EventBus (idempotente).
pub fn subscribe_p2p() {
    let mut recv = RECV.lock();
    if recv.is_none() {
        *recv = Some(k_nano::EVENT_BUS.subscribe(k_nano::net::mesh::TOPIC_P2P_PACKET));
        slog_hermes!("MeshKnowledge", "info", "subscribed P2P_PACKET (EventBus)");
    }
}

/// Drena os pacotes P2P do EventBus e aplica conhecimento (memórias/persona).
/// Self-activate no primeiro pacote válido. Chamado pelo bin via
/// `skill_sync::poll_p2p()`.
pub fn poll_p2p() {
    subscribe_p2p();
    loop {
        let evt = RECV.lock().as_ref().and_then(|r| r.try_receive());
        let Some(evt) = evt else { break };
        if evt.topic != k_nano::net::mesh::TOPIC_P2P_PACKET {
            continue;
        }
        let Some(pkt) = k_nano::net::udp_broadcast::parse(&evt.payload) else {
            continue;
        };
        if pkt.task_type != TaskType::Sync {
            continue;
        }
        mark_active();
        let payload = if evt.payload.len() > k_nano::net::noproto::PACKET_HEADER_SIZE {
            &evt.payload[k_nano::net::noproto::PACKET_HEADER_SIZE..]
        } else {
            &[][..]
        };
        if payload.starts_with(PREFIX_MEM) {
            on_memory_doc(&pkt, &payload[PREFIX_MEM.len()..]);
        } else if payload.starts_with(PREFIX_SOUL) {
            on_persona("SOUL", &payload[PREFIX_SOUL.len()..], pkt.source_id);
        } else if payload.starts_with(PREFIX_PERS) {
            on_persona("PERSONA", &payload[PREFIX_PERS.len()..], pkt.source_id);
        }
    }
}

/// MERGE por VectorClock: para a mesma (layer, key), só aplica se o clock do
/// doc recebido domina o local — count do node_id do remetente maior, ou key
/// inexistente localmente.
///
/// Aplica via API pública `k_ai::sgdb::put_doc` (store.rs) — o put público por
/// MemoryDoc que persiste e indexa sob a storage_key canônica `md/{layer}/{key}`
/// (a mesma que `get_doc` lê). Decisão documentada: como `put_doc` cobre todas
/// as camadas L0–L7, NÃO foi preciso o fallback `remember_fact` só para L3.
fn on_memory_doc(pkt: &AiosTaskPacket, body: &[u8]) {
    let doc = match MemoryDoc::decode(body) {
        Ok(d) => d,
        Err(e) => {
            slog_hermes!("MeshKnowledge", "warn", "RX MEM decode fail: {}", e);
            return;
        }
    };
    let apply = match k_ai::sgdb::get_doc(doc.layer, &doc.key) {
        Ok(None) => true, // key inexistente → aplica
        Ok(Some(local)) => clock_dominates(&doc.clock, &local.clock, pkt.source_id),
        Err(_) => true, // engine indisponível → deixa o put decidir
    };
    if !apply {
        slog_hermes!(
            "MeshKnowledge", "info",
            "RX MEM drop (clock dominado) layer={} key='{}' node={}",
            doc.layer.as_str(), doc.key, pkt.source_id
        );
        return;
    }
    let layer = doc.layer.as_str();
    let key = doc.key.clone();
    match k_ai::sgdb::put_doc(doc) {
        Ok(_) => {
            MEMORY_DOCS_SYNCED.fetch_add(1, Ordering::Relaxed);
            slog_hermes!(
                "MeshKnowledge", "info",
                "RX MEM aplicada layer={} key='{}' node={}",
                layer, key, pkt.source_id
            );
        }
        Err(e) => slog_hermes!("MeshKnowledge", "warn", "RX MEM put FAIL: {}", e),
    }
}

/// O clock recebido domina o local se o count do node_id do remetente no clock
/// recebido for maior que no clock local (comparação por nó, não total).
fn clock_dominates(remote: &VectorClock, local: &VectorClock, sender: u8) -> bool {
    clock_count(remote, sender) > clock_count(local, sender)
}

fn clock_count(vc: &VectorClock, node: u8) -> u64 {
    for i in 0..8 {
        if vc.nodes[i] == node {
            return vc.counts[i];
        }
    }
    0
}

/// Persona coletiva (L8): aplica SOUL/PERSONA de qualquer peer (memória
/// coletiva), mas só se o conteúdo difere do atual (anti-loop). Loga source_id.
fn on_persona(kind: &str, body: &[u8], source: u8) {
    let content = match core::str::from_utf8(body) {
        Ok(s) => s,
        Err(_) => return,
    };
    if content.trim().is_empty() {
        return;
    }
    let current = if kind == "SOUL" {
        crate::memory_store::read_soul()
    } else {
        crate::memory_store::read_persona()
    };
    if current == content {
        slog_hermes!(
            "MeshKnowledge", "info",
            "RX {} igual ao local — skip (anti-loop) node={}",
            kind, source
        );
        return;
    }
    let res = if kind == "SOUL" {
        crate::memory_store::write_soul(content)
    } else {
        crate::memory_store::write_persona(content)
    };
    match res {
        Ok(()) => {
            PERSONA_SYNCS.fetch_add(1, Ordering::Relaxed);
            slog_hermes!(
                "MeshKnowledge", "info",
                "RX {} aplicada node={} bytes={}",
                kind, source, content.len()
            );
        }
        Err(e) => slog_hermes!("MeshKnowledge", "warn", "RX {} write FAIL: {}", kind, e),
    }
}
