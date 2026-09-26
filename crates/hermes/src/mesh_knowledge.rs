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

/// FNV-1a 64 — dedup por conteúdo (anti-bloat TX/RX do learner/mesh).
fn fnv1a64(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

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
    // Anti-bloat TX: só difunde o par quando o CONTEÚDO muda. O clock.tick()
    // por TX fazia o doc recebido sempre dominar o local no dedup RX
    // (clock_dominates) e o put_doc repetido do mesmo payload crescia o Tickv
    // linear (GC suspenso no mount — K33[28]) até OOM fatal do intent_router
    // nos workers do mesh 6 (SESSION 407/408: T+56k com 1G, T+111k com 2G).
    static LAST_TX_HASH: [AtomicU64; 8] = [
        AtomicU64::new(0),
        AtomicU64::new(0),
        AtomicU64::new(0),
        AtomicU64::new(0),
        AtomicU64::new(0),
        AtomicU64::new(0),
        AtomicU64::new(0),
        AtomicU64::new(0),
    ];
    for (i, pair) in pairs.iter().rev().take(8).enumerate() {
        let mut payload = Vec::with_capacity(pair.input.len() + pair.output.len() + 1);
        payload.extend_from_slice(pair.input.as_bytes());
        payload.push(0);
        payload.extend_from_slice(pair.output.as_bytes());
        let h = fnv1a64(&payload);
        if LAST_TX_HASH[i].load(Ordering::Relaxed) == h {
            continue;
        }
        let mut doc = MemoryDoc::new(
            MemoryLayer::L4Semantic,
            &alloc::format!("learner/mesh/{}", i),
            payload,
        );
        doc.clock.tick(mesh::node_id());
        if broadcast_memory_doc(&doc) {
            LAST_TX_HASH[i].store(h, Ordering::Relaxed);
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
///
/// **s410k — RX em batch:** os docs `MEM\0` do drain são coletados e os blobs
/// vencedores do merge CRDT (L2+, fora da arena RAM) aplicados em UMA operação
/// de storage (`put_many_raw` → TickvLite `put_batch` = 1 lock + GC adiado)
/// no fim do drain — o re-broadcast de N docs não paga N× (lock + maybe_gc).
pub fn poll_p2p() {
    subscribe_p2p();
    // Batch do drain: (storage_key, NMD1 blob) para o put_many_raw final.
    let mut pending_keys: alloc::vec::Vec<alloc::string::String> = Vec::new();
    let mut pending_blobs: alloc::vec::Vec<Vec<u8>> = Vec::new();
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
            on_memory_doc(
                &pkt,
                &payload[PREFIX_MEM.len()..],
                &mut pending_keys,
                &mut pending_blobs,
            );
        } else if payload.starts_with(PREFIX_SOUL) {
            on_persona("SOUL", &payload[PREFIX_SOUL.len()..], pkt.source_id);
        } else if payload.starts_with(PREFIX_PERS) {
            on_persona("PERSONA", &payload[PREFIX_PERS.len()..], pkt.source_id);
        }
    }
    // s410k: flush do batch — UMA operação de storage para todos os blobs
    // vencedores do drain (merge Applied preserva o wire NMD1 como recebido;
    // é o mesmo byte que o put_doc individual gravaria).
    if !pending_keys.is_empty() {
        let refs: alloc::vec::Vec<(&str, &[u8])> = pending_keys
            .iter()
            .zip(pending_blobs.iter())
            .map(|(k, v)| (k.as_str(), v.as_slice()))
            .collect();
        match k_ai::sgdb::nsgdb_bridge::put_many_raw_nsgdb(&refs) {
            Ok(n) if n == refs.len() => {
                slog_hermes!(
                    "MeshKnowledge", "ok",
                    "RX MEM batch aplicada n={} (1 op storage — s410k)",
                    n
                );
            }
            Ok(n) => {
                slog_hermes!(
                    "MeshKnowledge", "warn",
                    "RX MEM batch parcial: {} de {} (storage fail no meio)",
                    n,
                    refs.len()
                );
            }
            Err(e) => {
                slog_hermes!("MeshKnowledge", "warn", "RX MEM batch FAIL: {}", e);
            }
        }
    }
}

/// MERGE por VectorClock: para a mesma (layer, key), só aplica se o clock do
/// doc recebido domina o local — count do node_id do remetente maior, ou key
/// inexistente localmente.
///
/// Aplica via merge CRDT policy-aware do neural-sgdb (`merge_remote_nsgdb`,
/// s410f) — decisão por camada + happens-before + conflito preservado.
/// **s410k:** o blob NMD1 vencedor (veredicto Applied) NÃO é gravado aqui
/// individualmente: vai para o batch do drain (`pending_keys`/`pending_blobs`)
/// e é aplicado em UMA operação de storage no fim do `poll_p2p`
/// (`put_many_raw` — 1 lock TICKV + GC adiado). L0/L1 ficam na arena RAM do
/// motor (o batch cru reforça a fonte da verdade para fast-mount/rebuild).
fn on_memory_doc(
    pkt: &AiosTaskPacket,
    body: &[u8],
    pending_keys: &mut alloc::vec::Vec<alloc::string::String>,
    pending_blobs: &mut alloc::vec::Vec<Vec<u8>>,
) {
    let mut doc = match MemoryDoc::decode(body) {
        Ok(d) => d,
        Err(e) => {
            slog_hermes!("MeshKnowledge", "warn", "RX MEM decode fail: {}", e);
            return;
        }
    };
    // Anti-bloat RX: re-broadcast periódico do MESMO payload não re-entra no
    // Tickv — put repetido da mesma (key, payload) era o vazamento linear que
    // OOM'd os workers do mesh 6 (188x 'RX MEM aplicada' da mesma key em T+111k).
    static LAST_RX_HASH: [AtomicU64; 8] = [
        AtomicU64::new(0),
        AtomicU64::new(0),
        AtomicU64::new(0),
        AtomicU64::new(0),
        AtomicU64::new(0),
        AtomicU64::new(0),
        AtomicU64::new(0),
        AtomicU64::new(0),
    ];
    let h = fnv1a64(&doc.payload);
    let slot = (fnv1a64(doc.key.as_bytes()) % 8) as usize;
    if LAST_RX_HASH[slot].load(Ordering::Relaxed) == h {
        slog_hermes!(
            "MeshKnowledge", "info",
            "RX MEM skip (conteudo igual — anti-bloat) layer={} key='{}' node={}",
            doc.layer.as_str(), doc.key, pkt.source_id
        );
        return;
    }
    let layer = doc.layer.as_str();
    let key = doc.key.clone();
    // s410f: merge CRDT policy-aware do neural-sgdb 1.2.x no lugar do
    // put_doc cego + clock_dominates manual (pre-check removido — o merge
    // policy já decide: MergePolicy::for_layer por camada, L0/L1 nunca
    // adotam; L4 causal-LWW; L2/L3 multi-value; conflito PRESERVADO).
    use k_ai::sgdb::nsgdb_bridge::NsMergeVerdict;
    match k_ai::sgdb::nsgdb_bridge::merge_remote_nsgdb(
        doc.layer,
        &doc.key,
        core::mem::take(&mut doc.payload),
        doc.clock.clone(),
    ) {
        NsMergeVerdict::Applied => {
            MEMORY_DOCS_SYNCED.fetch_add(1, Ordering::Relaxed);
            LAST_RX_HASH[slot].store(h, Ordering::Relaxed);
            // s410k: blob vencedor vai para o batch do drain (1 op de storage
            // para todos; o merge já decidiu e atualizou os índices).
            pending_keys.push(alloc::format!("md/{}/{}", layer, key));
            pending_blobs.push(body.to_vec());
            slog_hermes!(
                "MeshKnowledge", "info",
                "RX MEM aceita (merge) layer={} key='{}' node={} (batch)",
                layer, key, pkt.source_id
            );
        }
        NsMergeVerdict::Duplicate | NsMergeVerdict::Stale => {
            // Sem regressão / eco: registra hash p/ anti-bloat e ignora.
            LAST_RX_HASH[slot].store(h, Ordering::Relaxed);
            slog_hermes!(
                "MeshKnowledge", "info",
                "RX MEM merge={} layer={} key='{}' (sem regressão)",
                if matches!(NsMergeVerdict::Duplicate, NsMergeVerdict::Duplicate) { "dup" } else { "stale" },
                layer, key
            );
        }
        NsMergeVerdict::Conflict => {
            // Conflito preservado no NSGDB (ConflictRecord) — camada
            // cognitiva decide depois; NUNCA overwrite silencioso (ADR-0081).
            LAST_RX_HASH[slot].store(h, Ordering::Relaxed);
            slog_hermes!(
                "MeshKnowledge", "warn",
                "RX MEM CONFLICT preservado layer={} key='{}' node={}",
                layer, key, pkt.source_id
            );
        }
        NsMergeVerdict::Rejected => {
            slog_hermes!(
                "MeshKnowledge", "info",
                "RX MEM rejeitada (layer local-only) layer={} key='{}'",
                layer, key
            );
        }
    }
}

/// O clock recebido domina o local se o count do node_id do remetente no clock
/// recebido for maior que no clock local (comparação por nó, não total).
// s410f: clock_dominates/clock_count removidos — a decisão causal virou
// responsabilidade do merge policy-aware do neural-sgdb 1.2.x
// (merge_remote_nsgdb: happens-before + MergePolicy::for_layer).

fn clock_count(vc: &VectorClock, node: u8) -> u64 {
    for i in 0..8 {
        if vc.nodes[i] == node {
            return vc.counts[i];
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// s410k: o batch do drain RX aplica N blobs vencedores com UMA operação
    /// de storage (`put_many_raw` → TickvLite `put_batch`) — e a fonte da
    /// verdade fica byte-exata com o wire NMD1 (get_blob = decode igual).
    #[test]
    fn mesh_rx_batch_applies_blobs_with_one_storage_op() {
        // Storage limpa (RamFlash) + motor NSGDB.
        *k_nano::storage::TICKV.lock() = None;
        *k_nano::storage::FLASH.lock() = None;
        k_nano::storage::install_ram_flash(256 * 1024);
        {
            let mut g = k_nano::storage::TICKV.lock();
            g.get_or_insert_with(k_nano::storage::TickvLite::new)
                .mount()
                .expect("mount");
        }
        k_ai::sgdb::nsgdb_bridge::nsgdb_init();

        // 4 docs remotos (L2 — MergePolicy multi-value; sem local = Applied).
        let mut pending_keys: alloc::vec::Vec<alloc::string::String> = Vec::new();
        let mut pending_blobs: alloc::vec::Vec<Vec<u8>> = Vec::new();
        for i in 0..4u32 {
            let mut doc = MemoryDoc::new(
                MemoryLayer::L2EpisodicShort,
                &alloc::format!("batch/rx/{}", i),
                alloc::format!("payload-{}", i).into_bytes(),
            );
            doc.clock.tick(9); // nó remoto (≠ local)
            let enc = doc.encode();
            let mut body = Vec::with_capacity(PREFIX_MEM.len() + enc.len());
            body.extend_from_slice(PREFIX_MEM);
            body.extend_from_slice(&enc);
            // pkt fake (só source_id é usado no path Applied)
            let pkt = AiosTaskPacket::default();
            on_memory_doc(&pkt, &body[PREFIX_MEM.len()..], &mut pending_keys, &mut pending_blobs);
        }
        assert_eq!(pending_keys.len(), 4, "4 Applied deviam entrar no batch");

        // Flush do batch — 1 op de storage.
        let refs: alloc::vec::Vec<(&str, &[u8])> = pending_keys
            .iter()
            .zip(pending_blobs.iter())
            .map(|(k, v)| (k.as_str(), v.as_slice()))
            .collect();
        let n = k_ai::sgdb::nsgdb_bridge::put_many_raw_nsgdb(&refs).expect("batch");
        assert_eq!(n, 4);

        // Fonte da verdade byte-exata: get_blob do Tickv == NMD1 do wire.
        for i in 0..4u32 {
            let sk = alloc::format!("md/L2/batch/rx/{}", i);
            let raw = k_nano::storage::get_blob(&sk).expect("blob no tickv");
            let dec = MemoryDoc::decode(&raw).expect("NMD1 válido");
            assert_eq!(dec.payload, alloc::format!("payload-{}", i).into_bytes());
        }
        // Índices derivados (merge) também enxergam.
        let hits = k_ai::sgdb::nsgdb_bridge::scan_prefix_nsgdb("md/L2/batch/rx/");
        assert!(hits.len() >= 4);

        // Anti-bloat intacto: mesmo payload re-decodificado → skip (hash igual
        // → não re-entra no batch).
        let mut pending2: alloc::vec::Vec<alloc::string::String> = Vec::new();
        let mut blobs2: alloc::vec::Vec<Vec<u8>> = Vec::new();
        let mut doc = MemoryDoc::new(
            MemoryLayer::L2EpisodicShort,
            "batch/rx/0",
            b"payload-0".to_vec(),
        );
        doc.clock.tick(9);
        let enc = doc.encode();
        let pkt = AiosTaskPacket::default();
        on_memory_doc(&pkt, &enc, &mut pending2, &mut blobs2);
        assert!(pending2.is_empty(), "duplicata não deve re-entrar no batch");

        *k_nano::storage::TICKV.lock() = None;
        *k_nano::storage::FLASH.lock() = None;
    }
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
