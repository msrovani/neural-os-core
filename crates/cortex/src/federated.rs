//! F4 — Aprendizado federado mínimo do router Trinity (FedAvg em espaço binário).
//!
//! Escopo mínimo: só os pesos do router MoE (HIDDEN×num_experts = 384 i8 p/ 6
//! experts), trocados entre nós do mesh como deltas XOR vs o seed base (LCG
//! seed 42, idêntico ao `trinity::generate_router_weights`). Modelos grandes
//! (LLM/experts BitNet) ficam fora — futuro.
//!
//! Protocolo (prefixos espelham skill_sync — lane paralela):
//! - `"FED\0"`   + node_id(1B) + delta(u8) — Worker/Compute/Memory → Master.
//! - `"FEDW\0"`  + num_experts(u8) + pesos i8 raw — Master → todos (absoluto).
//!
//! RX via EventBus (tópico `P2P_PACKET`, subscribe lazy — padrão skill_sync).
//! Transporte: `k_nano::net::mesh::mesh_send_large` (assinado, chunking >1100B).

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use k_nano::net::mesh::{self, NodeRole};
use k_nano::net::noproto::{PACKET_HEADER_SIZE, TaskType};

use crate::r3;
use crate::trinity::{ROUTER_HIDDEN, ROUTER_MAX_EXPERTS};

const PREFIX_FED: &[u8] = b"FED\0";
const PREFIX_FEDW: &[u8] = b"FEDW\0";

/// Throttle entre ações federadas (espelha o heartbeat ~110 ticks; 500 é
/// folgado — o SleepCycle roda a cada 1000 ticks).
const FED_THROTTLE_TICKS: u64 = 500;

// ─── Estado local (ponytail: globals com lock — 1 escritor (SleepCycle) +
// leitores ocasionais; upgrade p/ SPSC se throughput exigir) ────────────────

/// Receptor lazy do EventBus (tópico P2P_PACKET).
static RECV: Mutex<Option<event_bus::Receiver>> = Mutex::new(None);

/// Deltas acumulados de peers: (node_id, delta). Substitui entrada do mesmo nó
/// (keep latest). Limpo pelo Master ao fundir.
static FED_DELTAS: Mutex<Vec<(u8, Vec<u8>)>> = Mutex::new(Vec::new());

/// Pesos fundidos aguardando aplicação no router vivo (TRINITY do hermes) —
/// drenado pelo driver após fed_tick/poll_p2p.
static FED_PENDING_LIVE: Mutex<Option<Vec<i8>>> = Mutex::new(None);

/// Último tick de ação federada (throttle).
static LAST_FED_ACTION: AtomicU64 = AtomicU64::new(0);

/// Contador de rodadas federadas (diagnóstico).
static FED_ROUNDS: AtomicU64 = AtomicU64::new(0);

/// Rodadas federadas concluídas (deltas enviados / merges aplicados).
pub fn fed_rounds() -> u64 {
    FED_ROUNDS.load(Ordering::Relaxed)
}

// ─── Lógica pura (sem rede/HW) ─────────────────────────────────────────────

/// Seed base idêntico ao `trinity::generate_router_weights` (LCG seed 42).
/// Retorna Vec<i8> ternário {-1,0,1} em ordem row-major HIDDEN×num_experts.
///
/// Conversão: o trinity gera direto no PackedTernaryTensor (2 bits/valor,
/// LSB-first); a sequência LCG dos draws dos bytes packed é a mesma dos
/// valores unpacked — espelhamos aqui a ordem de draws (primeiro os 99×64
/// draws da embedding table, depois 1 draw por peso). p/ 6 experts: 384 i8.
pub fn seed_router_weights(num_experts: usize) -> Vec<i8> {
    let mut seed: u32 = 42;
    // Embedding table primeiro (VOCAB_SIZE é pub em crate::cortex — não magic number).
    for _ in 0..((crate::cortex::VOCAB_SIZE as usize) * ROUTER_HIDDEN) {
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
    }
    let mut w = Vec::with_capacity(ROUTER_HIDDEN * num_experts);
    for _ in 0..(ROUTER_HIDDEN * num_experts) {
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        let r = (seed % 3) as i8;
        w.push(if r == 2 { -1i8 } else { r });
    }
    w
}

/// FedAvg em espaço binário por majority vote, posição a posição.
/// Cada delta (XOR vs seed) reconstrói os pesos candidatos de um nó
/// (cand[i] = seed[i] ^ delta[i], ambos i8 tratados como u8 — {-1,0,1} ↔
/// {0xFF,0x00,0x01}); para cada índice, o valor {-1,0,1} com mais votos
/// vence; empate (ou nenhum voto) mantém o seed.
pub fn merge_router_deltas(seed: &[i8], deltas: &[Vec<u8>]) -> Vec<i8> {
    let n = seed.len();
    let mut out = seed.to_vec();
    if n == 0 || deltas.is_empty() {
        return out;
    }
    for i in 0..n {
        let mut votes = [0i32; 3]; // índice: 0→-1, 1→0, 2→+1
        let mut voted = false;
        for d in deltas.iter() {
            if i >= d.len() {
                continue;
            }
            let cand = (seed[i] as u8) ^ d[i];
            let v = cand as i8; // volta ao ternário {-1,0,1}
            votes[((v + 1).clamp(0, 2)) as usize] += 1;
            voted = true;
        }
        if !voted {
            continue;
        }
        let mut best_idx = 1usize;
        let mut best_count = 0i32;
        for (idx, &c) in votes.iter().enumerate() {
            if c > best_count {
                best_count = c;
                best_idx = idx;
            }
        }
        // Empate entre líderes → mantém o seed.
        let tied = votes.iter().filter(|&&c| c == best_count).count() > 1;
        if !tied {
            out[i] = best_idx as i8 - 1;
        }
    }
    out
}

/// Aplica pesos fundidos: valida, persiste no seam `TRAINED_ROUTER` (r3 —
/// marca changed, consumido/limpo pelo broadcast) e enfileira a aplicação no
/// router vivo (o driver hermes drena via `pending_live_weights` e chama
/// `trinity::set_router_weights`). A conversão i8 → PackedTernaryTensor é a
/// mesma do trinity (2 bits/valor, LSB-first — byte-idêntico ao seed).
pub fn apply_router_weights(weights: &[i8]) -> bool {
    if weights.is_empty() || weights.len() % ROUTER_HIDDEN != 0 {
        return false;
    }
    let n_exp = weights.len() / ROUTER_HIDDEN;
    if n_exp > ROUTER_MAX_EXPERTS {
        return false;
    }
    r3::store_trained_router(weights);
    *FED_PENDING_LIVE.lock() = Some(weights.to_vec());
    k_nano::slog_cortex!(
        "FED", "info",
        "router fundido aplicado: {}x{} ({} i8)", ROUTER_HIDDEN, n_exp, weights.len()
    );
    true
}

/// Drena os pesos fundidos que aguardam aplicação no router vivo.
pub fn pending_live_weights() -> Option<Vec<i8>> {
    FED_PENDING_LIVE.lock().take()
}

// ─── Exchange via mesh (TX + RX) ───────────────────────────────────────────

/// Se há conhecimento novo (`trained_router_changed`), computa o delta XOR vs
/// o seed (determinístico) e transmite `"FED\0" + node_id(1B) + delta`.
/// Limpa a flag só em envio bem-sucedido (retry no próximo throttle).
pub fn broadcast_router_delta() -> bool {
    if !r3::trained_router_changed() {
        return false;
    }
    let Some(trained) = r3::trained_router_weights() else { return false };
    let n_exp = trained.len() / ROUTER_HIDDEN;
    if n_exp == 0 || trained.len() != n_exp * ROUTER_HIDDEN || n_exp > ROUTER_MAX_EXPERTS {
        return false;
    }
    let seed = seed_router_weights(n_exp);
    let delta = r3::router_delta_vs_seed(&seed, &trained);
    let mut payload = Vec::with_capacity(PREFIX_FED.len() + 1 + delta.len());
    payload.extend_from_slice(PREFIX_FED);
    payload.push(mesh::node_id());
    payload.extend_from_slice(&delta);
    let ok = mesh::mesh_send_large(&payload);
    if ok {
        r3::clear_trained_router_changed();
        FED_ROUNDS.fetch_add(1, Ordering::Relaxed);
        k_nano::slog_cortex!(
            "FED", "info",
            "TX delta node={} bytes={}", mesh::node_id(), delta.len()
        );
    }
    ok
}

/// Inscreve no tópico P2P_PACKET do EventBus (idempotente) — padrão skill_sync.
fn subscribe_p2p() {
    let mut recv = RECV.lock();
    if recv.is_none() {
        *recv = Some(k_nano::EVENT_BUS.subscribe(mesh::TOPIC_P2P_PACKET));
        k_nano::slog_cortex!("FED", "info", "subscribed P2P_PACKET (EventBus)");
    }
}

/// Drena os pacotes P2P do EventBus:
/// - `"FED\0"`  → decodifica delta e acumula no buffer de peers.
/// - `"FEDW\0"` → aplica pesos fundidos do Master (apply_router_weights).
pub fn poll_p2p() {
    subscribe_p2p();
    loop {
        let evt = RECV.lock().as_ref().and_then(|r| r.try_receive());
        let Some(evt) = evt else { break };
        if evt.topic != mesh::TOPIC_P2P_PACKET {
            continue;
        }
        let Some(pkt) = k_nano::net::udp_broadcast::parse(&evt.payload) else { continue };
        if pkt.task_type != TaskType::Sync {
            continue;
        }
        let payload = if evt.payload.len() > PACKET_HEADER_SIZE {
            &evt.payload[PACKET_HEADER_SIZE..]
        } else {
            &[][..]
        };
        if payload.starts_with(PREFIX_FED) {
            let rest = &payload[PREFIX_FED.len()..];
            if rest.len() <= 1 {
                continue;
            }
            let sid = rest[0];
            let delta = rest[1..].to_vec();
            // Valida tamanho (HIDDEN × n, 1..=ROUTER_MAX_EXPERTS experts).
            if delta.len() < ROUTER_HIDDEN
                || delta.len() % ROUTER_HIDDEN != 0
                || delta.len() / ROUTER_HIDDEN > ROUTER_MAX_EXPERTS
            {
                continue;
            }
            if sid == mesh::node_id() {
                continue; // eco do próprio broadcast
            }
            let mut buf = FED_DELTAS.lock();
            match buf.iter_mut().find(|(n, _)| *n == sid) {
                Some(slot) => slot.1 = delta, // keep latest do mesmo nó
                None => buf.push((sid, delta)),
            }
            k_nano::slog_cortex!(
                "FED", "info",
                "RX delta node={} bytes={} peers={}", sid, rest[1..].len(), buf.len()
            );
        } else if payload.starts_with(PREFIX_FEDW) {
            // C2 (oracle): só Worker/Compute/Memory aplica FEDW — o Master é
            // a FONTE (funde e difunde); rejeitar FEDW de não-Master do wire.
            if mesh::local_role() == NodeRole::Master {
                continue;
            }
            let rest = &payload[PREFIX_FEDW.len()..];
            if rest.len() <= 1 {
                continue;
            }
            let n_exp = rest[0] as usize;
            let weights = &rest[1..];
            if n_exp == 0 || n_exp > ROUTER_MAX_EXPERTS || weights.len() != ROUTER_HIDDEN * n_exp {
                continue;
            }
            let w: Vec<i8> = weights.iter().map(|&b| b as i8).collect();
            // C2 (oracle): valida valores ternários {-1,0,1} — bytes lixo
            // zerariam o router de toda a frota (encode clamp para 0).
            if w.iter().any(|&x| x != -1 && x != 0 && x != 1) {
                k_nano::slog_cortex!("FED", "warn", "RX FEDW descartado: valores nao-ternarios");
                continue;
            }
            if apply_router_weights(&w) {
                k_nano::slog_cortex!(
                    "FED", "info",
                    "RX FEDW master: {}x{} pesos aplicados", ROUTER_HIDDEN, n_exp
                );
            }
        }
    }
}

/// Orquestração federada por papel (chamado pelo SleepCycle, best-effort):
/// - Master: com ≥1 delta de peer acumulado (throttle ~500 ticks), funde com
///   majority vote, aplica, e broadcast `"FEDW\0"` dos pesos fundidos.
/// - Worker/Compute/Memory: broadcast do próprio delta (throttle ~500 ticks).
/// Retorna true se algo foi trocado (diagnóstico).
pub fn fed_tick(role: NodeRole, node_count: u8) -> bool {
    let now = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
    match role {
        NodeRole::Master => {
            let deltas = {
                let mut buf = FED_DELTAS.lock();
                if buf.is_empty() {
                    return false;
                }
                if now.saturating_sub(LAST_FED_ACTION.load(Ordering::Relaxed)) < FED_THROTTLE_TICKS {
                    return false;
                }
                core::mem::take(&mut *buf)
            };
            let n_exp = deltas[0].1.len() / ROUTER_HIDDEN;
            if n_exp == 0 || n_exp > ROUTER_MAX_EXPERTS {
                return false;
            }
            let ds: Vec<Vec<u8>> = deltas
                .iter()
                .filter(|(_, d)| d.len() == n_exp * ROUTER_HIDDEN)
                .map(|(_, d)| d.clone())
                .collect();
            if ds.is_empty() {
                return false;
            }
            let seed = seed_router_weights(n_exp);
            let merged = merge_router_deltas(&seed, &ds);
            if apply_router_weights(&merged) {
                LAST_FED_ACTION.store(now, Ordering::Relaxed);
                FED_ROUNDS.fetch_add(1, Ordering::Relaxed);
                let mut payload = Vec::with_capacity(PREFIX_FEDW.len() + 1 + merged.len());
                payload.extend_from_slice(PREFIX_FEDW);
                payload.push(n_exp as u8);
                for &w in &merged {
                    payload.push(w as u8);
                }
                let sent = mesh::mesh_send_large(&payload);
                k_nano::slog_cortex!(
                    "FED", "info",
                    "Master: merge {} deltas de {} nos → FEDW {}B sent={}",
                    ds.len(), node_count, merged.len(), sent
                );
                return true; // aplicou localmente (broadcast pode falhar sem sessão)
            }
            false
        }
        NodeRole::Worker | NodeRole::Compute | NodeRole::Memory => {
            if now.saturating_sub(LAST_FED_ACTION.load(Ordering::Relaxed)) < FED_THROTTLE_TICKS {
                return false;
            }
            LAST_FED_ACTION.store(now, Ordering::Relaxed);
            broadcast_router_delta()
        }
        NodeRole::Undecided => false,
    }
}

// ─── Self-test puro (sem rede/HW) ──────────────────────────────────────────

/// Pure: seed(6 experts) → 2 deltas sintéticos → merge → verifica tamanho e
/// majority vote (empate mantém seed). Loga PASS/FAIL.
pub fn federated_self_test() -> bool {
    let n_exp = 6;
    let seed = seed_router_weights(n_exp);
    let ok_size = seed.len() == ROUTER_HIDDEN * n_exp && seed.len() == 384;

    // Nó A e nó B concordam em mudar o índice 0 para `expect0` (≠ seed[0]),
    // e discordam no índice 1 (A → -1, B → 0). Majority: índice 0 = expect0;
    // índice 1 = empate 3-vias → mantém seed.
    let expect0 = if seed[0] == 1 { -1i8 } else { 1i8 };
    let mut a = seed.clone();
    a[0] = expect0;
    a[1] = -1;
    let mut b = seed.clone();
    b[0] = expect0;
    b[1] = 0;

    let delta_a = r3::router_delta_vs_seed(&seed, &a);
    let delta_b = r3::router_delta_vs_seed(&seed, &b);
    let merged = merge_router_deltas(&seed, &[delta_a, delta_b]);

    let ok_vote0 = merged[0] == expect0;
    let ok_tie1 = merged[1] == seed[1];
    let ok = ok_size && merged.len() == seed.len() && ok_vote0 && ok_tie1;
    if ok {
        k_nano::slog_cortex!(
            "FED", "info",
            "federated self-test PASS (seed={}B merged={}B)", seed.len(), merged.len()
        );
    } else {
        k_nano::slog_cortex!(
            "FED", "warn",
            "federated self-test FAIL (seed={}B merged={}B v0={} tie1={})",
            seed.len(), merged.len(), ok_vote0, ok_tie1
        );
    }
    ok
}
