//! ADR-0057 WS-C: camada de dispatch de compute (ComputeBackend).
//!
//! Choke point único de roteamento do matmul da LLM. Ordem de fallback honesta:
//! `NPU → GPU → CPU-SMP (P-cores) → AVX-512 → AVX2 → scalar`. Cada camada só entra se o
//! seu gate passou; nada é "fingido".
//!
//! GPU (`k_hal`) e NPU (`k_ai`) registram-se por fn-pointer porque dependem de
//! `cortex` (evita ciclo de dependência: `k_nano ← cortex ← {k_hal,k_ai}`).
//! Enquanto nenhum backend real registra (ex.: QEMU sem GPU/NPU), o dispatch
//! cai direto no caminho CPU/SMP.

use crate::tensor::{PackedTernaryTensor, Tensor};
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
#[cfg(feature = "p2p")]
use alloc::vec::Vec;
#[cfg(feature = "p2p")]
use k_nano::net::mesh::{
    attempt_allowed, peer_available, peer_state, refusal_backoff_ticks, record_peer_failure,
    PeerState, MAX_TASK_ATTEMPTS, REFUSAL_BACKOFF_BASE_TICKS,
};
#[cfg(feature = "p2p")]
use spin::Mutex;

/// Assinatura de um backend de matmul ternário (BitNet).
pub type TernaryFn = fn(&PackedTernaryTensor, &Tensor) -> Option<Tensor>;
/// ADR-0112 s421: lane VRAM por SEQUÊNCIA (layer-major) — o cortex informa o
/// índice absoluto da matriz na ordem canônica layer*7+[q,k,v,o,gate,up,down].
pub type VramSeqFn = fn(usize, &PackedTernaryTensor, &Tensor) -> Option<Tensor>;

// Slots de registro (0 = não registrado). fn-pointer cabe em usize no alvo.
static GPU_TERNARY: AtomicUsize = AtomicUsize::new(0);
static NPU_TERNARY: AtomicUsize = AtomicUsize::new(0);
/// ADR-0112: lane VRAM — pesos residentes na VRAM via BAR (qualquer vendor);
/// GEMV no host lendo pela aperture. Antes do GPU device (não exige firmware).
static VRAM_TERNARY: AtomicUsize = AtomicUsize::new(0);
static VRAM_ENABLED: AtomicBool = AtomicBool::new(false);

// Telemetria (ADR-0057 + ADR-0061): quantas ops cada anel tratou.
static N_NPU: AtomicU64 = AtomicU64::new(0);
static N_GPU: AtomicU64 = AtomicU64::new(0);
static N_VRAM: AtomicU64 = AtomicU64::new(0);
static N_SMP: AtomicU64 = AtomicU64::new(0);
static N_AVX512: AtomicU64 = AtomicU64::new(0);
static N_CPU: AtomicU64 = AtomicU64::new(0);
// ADR-0081 C1: ops despachadas para o mesh (Worker → Master)
static N_MESH: AtomicU64 = AtomicU64::new(0);
/// Orb: Thinking laranja enquanto round-trip MW/MR (FRAG) está em voo.
#[cfg(feature = "p2p")]
static MESH_MATMUL_BUSY: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// True se um matmul mesh (FRAG) está em curso — DisplayAgent → OrbSignals.thinking.
#[cfg(feature = "p2p")]
pub fn mesh_matmul_busy() -> bool {
    MESH_MATMUL_BUSY.load(Ordering::Acquire)
}
#[cfg(not(feature = "p2p"))]
pub fn mesh_matmul_busy() -> bool {
    false
}

/// Ring 0 (intent/router) — registrado por `k_ai` quando uma NPU fica pronta.
pub fn register_npu_ternary(f: TernaryFn) {
    NPU_TERNARY.store(f as usize, Ordering::Release);
    k_nano::slog_nano!("COMPUTE", "ok", "NPU ternary backend registrado (Ring0)");
}

/// Ring 1 (matmul pesado) — registrado por `k_hal` quando o canário GPU passa.
pub fn register_gpu_ternary(f: TernaryFn) {
    GPU_TERNARY.store(f as usize, Ordering::Release);
    k_nano::slog_nano!("COMPUTE", "ok", "GPU ternary backend registrado (Ring1)");
}

/// ADR-0112 — lane VRAM (BAR compute): registrado por `k_hal` quando o
/// canário de round-trip da aperture passa (init_stream_ring). GEMV no host
/// lendo pesos residentes na VRAM — libera RAM e prefetch overlap via PCIe.
pub fn register_vram_ternary_seq(f: VramSeqFn) {
    VRAM_TERNARY.store(f as usize, Ordering::Release);
    VRAM_ENABLED.store(true, Ordering::Release);
    k_nano::slog_nano!("COMPUTE", "ok", "VRAM ternary backend registrado (ADR-0112 BAR compute, seq)");
}

/// Lane VRAM pronto p/ servir ( registrado + pesos uploaded). Callers
/// usam isso p/ gate de rota honesto (HUD/logs) antes de tentar o lane.
pub fn vram_served() -> bool {
    VRAM_ENABLED.load(Ordering::Acquire)
}

/// Telemetria: ops tratadas pelo lane VRAM.
pub fn vram_ops() -> u64 {
    N_VRAM.load(Ordering::Relaxed)
}

#[inline]
fn call_slot(slot: usize, w: &PackedTernaryTensor, x: &Tensor) -> Option<Tensor> {
    if slot == 0 {
        return None;
    }
    // Safety: só armazenamos fn-pointers válidos via register_*.
    let f: TernaryFn = unsafe { core::mem::transmute::<usize, TernaryFn>(slot) };
    f(w, x)
}

/// Roteia um matmul ternário. `Some` = tratado por acelerador/paralelo;
/// `None` = chamador segue no caminho AVX-512/AVX2/scalar existente.
pub fn dispatch_ternary(w: &PackedTernaryTensor, x: &Tensor) -> Option<Tensor> {
    let (k, n) = w.shape;
    let big = n >= 64 && k >= 64;

    // ADR-0081 C1: Mesh-aware dispatch — protocolo MW→Master→MR (não Worker→Compute).
    // Memory/Compute/Worker (qualquer não-Master) despacham para o Master quando
    // há peer na malha. Gate antigo "só Worker + Compute peer" matava o path
    // assim que ROLE\0 aplicava Memory/Compute (self-test nunca FRAG-ava).
    #[cfg(feature = "p2p")]
    {
        let role = k_nano::net::mesh::local_role();
        let can_send = matches!(
            role,
            k_nano::net::mesh::NodeRole::Worker
                | k_nano::net::mesh::NodeRole::Memory
                | k_nano::net::mesh::NodeRole::Compute
        );
        if can_send {
            // s458: fonte única de disponibilidade — `peer_available(id)`
            // substitui o node_count() ad-hoc (telemetria dizia peer ativo
            // enquanto o compute não achava peer disponível; o circuit breaker
            // era ignorado → retries eternos alimentando o OOM). Gate ANTES de
            // N_MESH/serialize — skip não aloca nada.
            if let Some(target) = mesh_first_peer_id() {
                let st = peer_state(target);
                if !peer_available(target) {
                    // Divergência visível: estado explícito no slog. Skip ≠
                    // failure — NÃO marca peer failure (o circuit breaker já
                    // falou; s364).
                    k_nano::slog_cortex!(
                        "MESH", "warn",
                        "dispatch skip peer={} state={} - fallback local",
                        target,
                        peer_state_label(st)
                    );
                } else if k_nano::memory::refuse_heavy_frag() {
                    // Fall through — ternary_matmul recusa local big sob pressure.
                } else {
                    N_MESH.fetch_add(1, Ordering::Relaxed);
                    MESH_MATMUL_BUSY.store(true, Ordering::Release);
                    let got = mesh_matmul_worker(w, x, target);
                    MESH_MATMUL_BUSY.store(false, Ordering::Release);
                    if let Some(t) = got {
                        return Some(t);
                    }
                    // Falha já registrada por tentativa DENTRO do worker
                    // (record_peer_failure(target)) — sem dupla contagem aqui.
                }
            }
        }
    }

    // Ring 0 — NPU (router/intent, latência-crítico). Só se registrado.
    if let Some(r) = call_slot(NPU_TERNARY.load(Ordering::Acquire), w, x) {
        N_NPU.fetch_add(1, Ordering::Relaxed);
        return Some(r);
    }

    // Ring 1 — VRAM (ADR-0112 BAR compute): MOVIDO p/ dispatch_vram_seq
    // (s421) — o lane é POR SEQUÊNCIA (layer*7+slot) e só o apply_one_layer
    // conhece a posição; shape não resolve q/k/v/o (mesma (h,h)). Aqui não
    // há como identificar a matriz sem risco de peso errado.
    let _ = big;

    // Ring 1 — GPU (matmul pesado). Só se registrado e op grande.
    if big {
        if let Some(r) = call_slot(GPU_TERNARY.load(Ordering::Acquire), w, x) {
            N_GPU.fetch_add(1, Ordering::Relaxed);
            return Some(r);
        }
    }

    // Ring 1 fallback — P-cores (APs) via WS-B. Só se os APs forem workers
    // vivos (`ap_pollable`, WS-F); senão o `parallel_*` degrada e caímos em CPU.
    if big
        && k_nano::platform_probe::allow_smp()
        && k_nano::smp::ap_pollable()
        && k_nano::smp::ap_entry_count() > 0
    {
        if let Some(r) = crate::parallel_matmul::parallel_ternary_matmul(w, x) {
            N_SMP.fetch_add(1, Ordering::Relaxed);
            return Some(r);
        }
    }

    // Ring 2 — AVX-512 (ADR-0061): antes de AVX2, se FeatureGate permite.
    if big && k_nano::platform_probe::allow_avx512() {
        if let Some(r) = crate::bitnet_avx512::ternary_matmul_avx512(w, x) {
            N_AVX512.fetch_add(1, Ordering::Relaxed);
            return Some(r);
        }
    }

    // Ring 2 — CPU (AVX2/scalar): sinaliza fallback ao chamador.
    N_CPU.fetch_add(1, Ordering::Relaxed);
    None
}

/// ADR-0112 s421 — tentativa do lane VRAM por sequência (layer-major).
/// Chamado do apply_one_layer com slot = layer*7+[q,k,v,o,gate,up,down].
/// Só entra se o lane está registrado (`vram_served`) e a op é big;
/// `None` honesto → caller segue a escada CPU (nunca peso errado).
pub fn dispatch_vram_seq(slot: usize, w: &PackedTernaryTensor, x: &Tensor) -> Option<Tensor> {
    if !vram_served() {
        return None;
    }
    let (k, n) = w.shape;
    if !(n >= 64 && k >= 64) {
        return None;
    }
    let slot_fn = VRAM_TERNARY.load(Ordering::Acquire);
    if slot_fn == 0 {
        return None;
    }
    // Safety: só armazenamos fn-pointers válidos via register_*.
    let f: VramSeqFn = unsafe { core::mem::transmute::<usize, VramSeqFn>(slot_fn) };
    let r = f(slot, w, x);
    if r.is_some() {
        N_VRAM.fetch_add(1, Ordering::Relaxed);
    }
    r
}

/// (npu, gpu, smp, avx512, cpu) — contadores de dispatch para telemetria/serial.
pub fn dispatch_summary() -> (u64, u64, u64, u64, u64) {
    (
        N_NPU.load(Ordering::Relaxed),
        N_GPU.load(Ordering::Relaxed),
        N_SMP.load(Ordering::Relaxed),
        N_AVX512.load(Ordering::Relaxed),
        N_CPU.load(Ordering::Relaxed),
    )
}

/// True se algum acelerador (NPU/GPU) está registrado.
pub fn accel_registered() -> bool {
    GPU_TERNARY.load(Ordering::Acquire) != 0 || NPU_TERNARY.load(Ordering::Acquire) != 0
}

#[cfg(test)]
mod vram_seq_tests {
    use super::*;

    /// Contrato: sem registro do k_hal, o lane seq está fechado e devolve
    /// None em qualquer slot — e NÃO conta N_VRAM (op não foi servida).
    #[test]
    fn vram_seq_fechado_sem_registro() {
        assert!(!vram_served());
        let w = PackedTernaryTensor { shape: (128, 128), packed_data: alloc::vec![0u8; 4096] };
        let x = Tensor::zero((1, 128));
        let before = vram_ops();
        for slot in 0..7 {
            assert!(dispatch_vram_seq(slot, &w, &x).is_none(), "slot {slot}");
        }
        assert_eq!(vram_ops(), before, "op não servida não conta");
    }
}

/// Lane D-accel: leitores p/ rótulo de rota (sem expor fn-pointers).
pub(crate) fn npu_registered() -> bool {
    NPU_TERNARY.load(Ordering::Acquire) != 0
}

/// Lane D-accel: leitores p/ rótulo de rota (sem expor fn-pointers).
pub(crate) fn gpu_registered() -> bool {
    GPU_TERNARY.load(Ordering::Acquire) != 0
}

// ─── ADR-0081 item 4: matmul ternário distribuído Worker→Master ────────────
// Protocolo binário (sem dep de serialização externa), porta P2P 42069:
//
// REQUEST (Worker→Master):  task_type=Inference, payload =
//   b"MW\0" | w.shape.0 u32 LE | w.shape.1 u32 LE | w.packed_data
//         | x.shape.0 u32 LE | x.shape.1 u32 LE | x.data (f32 LE × N)
//
// RESPONSE (Master→Worker): task_type=Inference, dest_id=node_id do Worker,
//   payload = b"MR\0" | shape.0 u32 LE | shape.1 u32 LE | data (f32 LE × N)
//
// SESSION_237: payloads grandes são fragmentados pelo transporte
// (send_fragmented/recv_fragmented). s457: o teto do wire é
// FRAG_MAX_PARTS(64) × FRAG_MAX_CHUNK(1000) = 64.000 B — o RX dropa
// total_len > 64.000 em silêncio. Preflight abaixo rejeita o job ANTES de
// qualquer Vec (serialize copiaria MBs e a recusa viria depois).

/// Teto REAL do wire FRAG: FRAG_MAX_PARTS(64) × FRAG_MAX_CHUNK(1000) = 64.000 B
/// (constantes privadas em `k_nano/src/net/udp_broadcast.rs`). Espelho local —
/// atualizar os dois juntos se um mudar.
const MESH_FRAG_WIRE_LIMIT: usize = 64_000;

/// Tamanho no wire de um pacote assinado/selado: header do pacote
/// (PACKET_HEADER_SIZE) + payload + tag. Tag máxima = Ed25519 64B (HMAC 32B /
/// AEAD 16B são menores) — estimativa = teto superior honesto p/ o gate.
#[cfg(feature = "p2p")]
fn mesh_signed_wire_size(payload_len: usize) -> Option<usize> {
    k_nano::net::noproto::PACKET_HEADER_SIZE
        .checked_add(payload_len)?
        .checked_add(k_nano::identity::SIGNATURE_LEN)
}

/// Estima o payload "MW\0" SEM alloc — aritmética checked, byte-exato com
/// `serialize_mesh_request` (3 magic + 4+4 shape w + packed + 4+4 shape x +
/// x.data × 4 f32 = 19 + packed + xbytes). w packed bytes derivam da SHAPE (o
/// Master parseia wbytes da shape, não do packed_data — tensor inconsistente
/// seria mis-parseado lá; recusa local → fallback correto).
/// `None` = overflow/inconsistente.
#[cfg(feature = "p2p")]
fn estimate_mesh_request_payload_size(w: &PackedTernaryTensor, x: &Tensor) -> Option<usize> {
    let (k, n) = w.shape;
    let wbytes = k.checked_mul(n)?.checked_add(3)? / 4;
    if wbytes != w.packed_data.len() {
        return None;
    }
    let xbytes = x.data.len().checked_mul(4)?;
    19usize
        .checked_add(wbytes)?
        .checked_add(xbytes)
}

/// Estima o request "MW\0" completo no wire (header + payload + assinatura).
#[cfg(feature = "p2p")]
fn estimate_mesh_request_wire_size(w: &PackedTernaryTensor, x: &Tensor) -> Option<usize> {
    mesh_signed_wire_size(estimate_mesh_request_payload_size(w, x)?)
}

/// Estima o payload "MR\0" de resposta SEM alloc — byte-exato com a
/// serialização em `handle_mesh_request` (3 magic + 8 shape + data × 4 f32).
#[cfg(feature = "p2p")]
fn estimate_mesh_response_payload_size(rows: usize, cols: usize) -> Option<usize> {
    rows.checked_mul(cols)?
        .checked_mul(4)
        .and_then(|d| 11usize.checked_add(d))
}

/// Estima a resposta "MR\0" completa no wire (header + payload + tag).
#[cfg(feature = "p2p")]
fn estimate_mesh_response_wire_size(rows: usize, cols: usize) -> Option<usize> {
    mesh_signed_wire_size(estimate_mesh_response_payload_size(rows, cols)?)
}

/// Gate puro do preflight FRAG (sem statics de RAM — testável): `None` =
/// passa; `Some(reason)` = recusa honesta com o motivo p/ o slog.
#[cfg(feature = "p2p")]
fn mesh_frag_gate(est_wire: usize, frag_budget: usize) -> Option<&'static str> {
    if est_wire > MESH_FRAG_WIRE_LIMIT {
        return Some("wire_limit");
    }
    if est_wire > frag_budget {
        return Some("frag_budget");
    }
    None
}

/// Budget de reassembly FRAG do nó (RAM), p/ o gate puro acima.
#[cfg(feature = "p2p")]
fn mesh_frag_budget() -> usize {
    k_nano::memory::frag_reassembly_budget_bytes(
        k_nano::memory::TOTAL_RAM_MB.load(core::sync::atomic::Ordering::Relaxed),
    )
}

/// Serializa w+x num request "MW\0". Caller DEVE rodar o preflight
/// (`mesh_frag_gate` + estimativas) antes — sem gate aqui.
#[cfg(feature = "p2p")]
fn serialize_mesh_request(w: &PackedTernaryTensor, x: &Tensor) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(1200);
    out.extend_from_slice(b"MW\0");
    out.extend_from_slice(&(w.shape.0 as u32).to_le_bytes());
    out.extend_from_slice(&(w.shape.1 as u32).to_le_bytes());
    out.extend_from_slice(&w.packed_data);
    out.extend_from_slice(&(x.shape.0 as u32).to_le_bytes());
    out.extend_from_slice(&(x.shape.1 as u32).to_le_bytes());
    for v in &x.data {
        out.extend_from_slice(&v.to_le_bytes());
    }
    Some(out)
}

/// Deserializa resposta "MR\0" num Tensor.
#[cfg(feature = "p2p")]
fn deserialize_mesh_response(data: &[u8]) -> Option<Tensor> {
    // "MR\0" + rows u32 LE + cols u32 LE + data f32 LE
    if data.len() < 11 || &data[0..3] != b"MR\0" {
        return None;
    }
    let rows = u32::from_le_bytes([data[3], data[4], data[5], data[6]]) as usize;
    let cols = u32::from_le_bytes([data[7], data[8], data[9], data[10]]) as usize;
    let n = rows.checked_mul(cols)?;
    let mut d = Vec::with_capacity(n);
    let mut off = 11;
    for _ in 0..n {
        let b = data.get(off..off + 4)?;
        d.push(f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        off += 4;
    }
    Some(Tensor { shape: (rows, cols), data: d })
}

/// M4: id do alvo do circuit breaker = primeiro peer online (nunca o
/// broadcast 0xFF — que poluía PEER_HEALTH com um pseudo-peer).
#[cfg(feature = "p2p")]
fn mesh_first_peer_id() -> Option<u8> {
    k_nano::net::mesh::MESH_ENGINE
        .lock()
        .as_ref()
        .and_then(|eng| eng.online_nodes().next().map(|n| n.capabilities.node_id[0]))
}

/// s458: rótulo legível do estado do peer p/ o slog de divergência
/// (telemetria-vs-compute visível: available/degraded/unavailable/unknown).
#[cfg(feature = "p2p")]
fn peer_state_label(st: Option<PeerState>) -> &'static str {
    match st {
        Some(PeerState::Available) => "available",
        Some(PeerState::Degraded) => "degraded",
        Some(PeerState::Unavailable) => "unavailable",
        None => "unknown",
    }
}

/// Worker side: envia request "MW\0" e espera síncrona pela resposta "MR\0".
/// Timeout ~200 TIMER_TICKS (~2s a 100Hz). Retorna `None` em timeout/falha →
/// o dispatch cai no fallback local. Pacotes que não são a nossa resposta são
/// descartados (não re-injetados no RX do mesh).
///
/// s458: retry loop bounded — gate `attempt_allowed` antes de cada retry,
/// `refusal_backoff_ticks` (determinístico por seed=node) entre tentativas,
/// `record_peer_failure(target)` em cada recusa (send fail/timeout). O
/// preflight M1 fica FORA do loop (job oversized nunca entra no retry) e o
/// serialize é POR tentativa — nenhum Vec retido entre retries.
#[cfg(feature = "p2p")]
fn mesh_matmul_worker(w: &PackedTernaryTensor, x: &Tensor, target: u8) -> Option<Tensor> {
    // AIOS: nó frugal não inicia FRAG — fallback local (Observe→Act, sem OOM).
    if k_nano::memory::refuse_heavy_frag() {
        k_nano::slog_cortex!(
            "MESH", "warn",
            "matmul mesh skip DEGRADED RAM={}MB",
            k_nano::memory::TOTAL_RAM_MB.load(core::sync::atomic::Ordering::Relaxed)
        );
        return None;
    }
    // s457 PREFLIGHT (M1 — preservado exato): estima request E resposta no
    // wire ANTES de qualquer Vec — o gate antigo (`can_afford_frag`) rodava
    // DEPOIS do serialize, que já copiara MBs p/ um Vec (churn de alloc
    // alimentando o OOM do CellNetwork::sleep_cycle). Estimativa checked,
    // zero alloc; recusa honesta com motivo + tamanhos. FORA do retry loop —
    // cancel-before-serialize: job oversized nunca entra no retry.
    let req_est = match estimate_mesh_request_wire_size(w, x) {
        Some(e) => e,
        None => {
            k_nano::slog_cortex!(
                "MESH", "warn",
                "matmul mesh skip estimate_overflow w={:?} x={:?}", w.shape, x.shape
            );
            return None;
        }
    };
    // A resposta (k×m f32) passa pelo mesmo teto do wire — round-trip com
    // resposta > limite nunca retorna (RX dropa silencioso) → skip ≠ timeout.
    let resp_est = match estimate_mesh_response_wire_size(w.shape.0, x.shape.1) {
        Some(e) => e,
        None => {
            k_nano::slog_cortex!(
                "MESH", "warn",
                "matmul mesh skip estimate_overflow resp w={:?} x={:?}", w.shape, x.shape
            );
            return None;
        }
    };
    let budget = mesh_frag_budget();
    if let Some(reason) = mesh_frag_gate(req_est, budget) {
        k_nano::slog_cortex!(
            "MESH", "warn",
            "matmul mesh skip req reason={} est={} wire={} budget={} w={:?} x={:?}",
            reason, req_est, MESH_FRAG_WIRE_LIMIT, budget, w.shape, x.shape
        );
        return None;
    }
    if let Some(reason) = mesh_frag_gate(resp_est, budget) {
        k_nano::slog_cortex!(
            "MESH", "warn",
            "matmul mesh skip resp reason={} est={} wire={} budget={} w={:?} x={:?}",
            reason, resp_est, MESH_FRAG_WIRE_LIMIT, budget, w.shape, x.shape
        );
        return None;
    }
    let node_id = k_nano::net::mesh::node_id();
    // s458: retry loop bounded — gate `attempt_allowed` no topo (antes de
    // cada retry), backoff entre tentativas, `record_peer_failure(target)` em
    // cada recusa. Teto: 3 tentativas × timeout 200 ticks + backoffs 50/100;
    // após 3 falhas o circuit breaker marca Unavailable e os dispatches
    // seguintes skipam (peer_available=false).
    let mut attempts: u32 = 0;
    loop {
        if !attempt_allowed(attempts, MAX_TASK_ATTEMPTS) {
            k_nano::slog_cortex!(
                "MESH", "warn",
                "matmul attempts esgotados node={} target={} attempts={} - fallback local",
                node_id, target, attempts
            );
            return None;
        }
        if attempts > 0 {
            // Backoff entre tentativas — delay determinístico por seed (node).
            let delay = refusal_backoff_ticks(attempts, REFUSAL_BACKOFF_BASE_TICKS, u64::from(node_id));
            k_nano::slog_cortex!(
                "MESH", "warn",
                "matmul retry target={} attempt={} backoff={} ticks",
                target, attempts, delay
            );
            mesh_backoff_wait(delay);
        }
        // Serialize POR tentativa — cancel-before-serialize: nenhum Vec
        // retido entre retries (payload/buf/signed dropam no fim da iteração
        // ou no return; o preflight acima já recusou antes de alloc).
        let payload = serialize_mesh_request(w, x)?;
        // ADR-0081 follow-up: clock monotônico único por fonte (anti-replay de
        // dados exige clk estritamente crescente; clock=0 seria dropado).
        let pkt = k_nano::net::noproto::AiosTaskPacket::new(
            k_nano::net::mesh::next_data_clock(), node_id, 0xFF, k_nano::net::noproto::TaskType::Inference,
            1, 0, 0, k_nano::net::noproto::PacketFlags::new(),
        );
        let mut buf = k_nano::net::udp_broadcast::serialize(&pkt);
        buf.extend_from_slice(&payload);
        // Fase A (SESSION_236): assinado — o RX fail-closed dropa não-assinados.
        let Some(signed) = k_nano::net::udp_broadcast::sign_packet(&buf) else {
            return None; // fail-closed: sem sessão não assina
        };
        // Phase 3: usa unicast para payloads grandes (porta 42070) em vez de
        // broadcast — reduz colisões. Fallback para broadcast se unicast falhar.
        let ok = k_nano::net::udp_broadcast::send_fragmented(&signed, 42069);
        k_nano::slog_cortex!(
            "MESH", "warn",
            "matmul request node={} target={} size={} sent={} attempt={}/{}",
            node_id, target, payload.len(), ok, attempts + 1, MAX_TASK_ATTEMPTS
        );
        if !ok {
            // Recusa do transporte → record_peer_failure + attempts += 1
            // (contrato do lane); o gate no topo decide retry/stop.
            record_peer_failure(target);
            attempts += 1;
            continue;
        }

        // Espera síncrona com timeout real por TIMER_TICKS (não iteração cega —
        // o scheduler pode não rodar durante a espera).
        let start = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        loop {
            let now = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
            if now.wrapping_sub(start) >= 200 {
                break; // timeout — recusa → retry/stop no topo do loop
            }
            // SESSION_237: recv_fragmented reassembla a resposta "MR" (ou devolve
            // pacotes ≤1200B direto). O blob completo volta para o parse/verify.
            while let Some(rx) = k_nano::net::udp_broadcast::recv_fragmented(42069) {
                let Some(p) = k_nano::net::udp_broadcast::parse(&rx) else { continue };
                // Copia campos do packed struct (E0793: sem refs a campos packed).
                let d = p.dest_id;
                let tt = p.task_type as u8;
                let sender = p.source_id;
                if tt == 1 && d == node_id && rx.len() > k_nano::net::noproto::PACKET_HEADER_SIZE {
                    // Fase A (SESSION_236): só aceita resposta do Master — verifica
                    // contra a pk vinculada na tabela TOFU do mesh. ADR-0081: dados
                    // usam tiered (HMAC em Relativized, Ed25519 em Full) e, no
                    // Tier F (Full), o seam `verify_or_open_tiered` ABRE o AEAD
                    // (X25519 DH + ChaCha20-Poly1305) de respostas seladas MR\0.
                    let Some(pk) = k_nano::net::mesh::peer_public_key(sender) else { continue };
                    let Some(valid) = k_nano::net::udp_broadcast::verify_or_open_tiered(&rx, &pk) else {
                        continue; // autenticação inválida / AEAD open falhou — DROP
                    };
                    if valid.len() <= k_nano::net::noproto::PACKET_HEADER_SIZE {
                        continue;
                    }
                    let resp = &valid[k_nano::net::noproto::PACKET_HEADER_SIZE..];
                    if let Some(t) = deserialize_mesh_response(resp) {
                        // Phase 3: registra sucesso no circuit breaker.
                        let rtt = (k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64)
                            .wrapping_sub(start);
                        k_nano::net::mesh::record_peer_success(sender, rtt);
                        k_nano::slog_cortex!(
                            "MESH", "ok",
                            "matmul resposta node={} ok shape={:?} rtt={} attempt={}",
                            node_id, t.shape, rtt, attempts + 1
                        );
                        return Some(t);
                    }
                }
                // Não é a nossa resposta — DROP (não re-injetar no RX do mesh).
            }
            // M4: yield em vez de spin puro — espera avanço por TSC (~1ms/iter)
            // sem encravar o loop no poll do NIC.
            k_nano::tsc::sleep_us(1_000);
        }
        // Timeout → recusa: record_peer_failure(target) + attempts += 1
        // (contrato do lane); o gate no topo decide retry/stop.
        record_peer_failure(target);
        attempts += 1;
    }
}

/// s458: espera o backoff entre tentativas de retry (ticks de TIMER_TICKS,
/// mesma unidade do timeout de resposta). Yield por TSC (~1ms/iter) — mesmo
/// padrão do wait de resposta (M4), sem encravar o scheduler.
#[cfg(feature = "p2p")]
fn mesh_backoff_wait(delay_ticks: u64) {
    let start = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
    loop {
        let now = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        if now.wrapping_sub(start) >= delay_ticks {
            break;
        }
        k_nano::tsc::sleep_us(1_000);
    }
}

/// Master side: processa request "MW\0" e retorna resposta "MR\0" serializada.
/// SESSION_360 fechou só o TX frugal; lab 6-node 1G #GP aqui no RX (s364) —
/// mesma `mesh_frag_pressure` / `can_afford_frag` antes de qualquer alloc.
#[cfg(feature = "p2p")]
pub fn handle_mesh_request(payload: &[u8]) -> Option<Vec<u8>> {
    // "MW\0" + k u32 + n u32 + packed + rows u32 + cols u32 + data f32 LE
    if payload.len() < 19 || &payload[0..3] != b"MW\0" {
        return None;
    }
    // AIOS: nó frugal não serve FRAG — Observe→Act, skip honesto (sem #GP).
    if k_nano::memory::refuse_heavy_frag() {
        return None;
    }
    // Resposta MR + FRAG reassembly ≥ payload; margem p/ header/sign.
    if !k_nano::memory::can_afford_frag(payload.len().saturating_add(256)) {
        return None;
    }
    let k = u32::from_le_bytes([payload[3], payload[4], payload[5], payload[6]]) as usize;
    let n = u32::from_le_bytes([payload[7], payload[8], payload[9], payload[10]]) as usize;
    // 2-bit packing (4 pesos/byte) — ceil div por 4.
    let wbytes = k.checked_mul(n)?.checked_add(3)? / 4;
    // s457 PREFLIGHT resposta: rows/cols vivem após o packed — leitura por
    // índice (bounds-checked, sem alloc) p/ estimar a resposta "MR" ANTES do
    // to_vec/with_capacity/compute. Resposta > teto nunca sai (RX do Worker
    // dropa silencioso) — recusa honesta aqui, sem gastar RAM com o parse.
    let off_rc = 11usize.checked_add(wbytes)?;
    if off_rc.checked_add(8)? > payload.len() {
        return None;
    }
    let rows = u32::from_le_bytes([
        payload[off_rc], payload[off_rc + 1], payload[off_rc + 2], payload[off_rc + 3],
    ]) as usize;
    let cols = u32::from_le_bytes([
        payload[off_rc + 4], payload[off_rc + 5], payload[off_rc + 6], payload[off_rc + 7],
    ]) as usize;
    let resp_est = match estimate_mesh_response_wire_size(rows, cols) {
        Some(e) => e,
        None => {
            k_nano::slog_cortex!(
                "MESH", "warn",
                "matmul serve skip estimate_overflow resp=({},{})", rows, cols
            );
            return None;
        }
    };
    let budget = mesh_frag_budget();
    if let Some(reason) = mesh_frag_gate(resp_est, budget) {
        k_nano::slog_cortex!(
            "MESH", "warn",
            "matmul serve skip resp reason={} est={} wire={} budget={} shape=({},{})",
            reason, resp_est, MESH_FRAG_WIRE_LIMIT, budget, rows, cols
        );
        return None;
    }
    let mut off = 11;
    let packed = payload.get(off..off + wbytes)?.to_vec();
    off += wbytes;
    // rows/cols já lidos no preflight (bounds-checked) acima.
    off += 8;
    let xlen = rows.checked_mul(cols)?;
    let mut xdata = Vec::with_capacity(xlen);
    for _ in 0..xlen {
        let b = payload.get(off..off + 4)?;
        xdata.push(f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        off += 4;
    }

    let w = PackedTernaryTensor { shape: (k, n), packed_data: packed };
    let x = Tensor { shape: (rows, cols), data: xdata };
    let r = crate::bitnet_avx2::ternary_matmul_adaptive(&w, &x)?;

    // Serializa resposta "MR\0" + shape + data f32 LE.
    let mut out = Vec::with_capacity(11 + r.data.len() * 4);
    out.extend_from_slice(b"MR\0");
    out.extend_from_slice(&(r.shape.0 as u32).to_le_bytes());
    out.extend_from_slice(&(r.shape.1 as u32).to_le_bytes());
    for v in &r.data {
        out.extend_from_slice(&v.to_le_bytes());
    }
    Some(out)
}

// ─── Consumo via EventBus (Master side, SESSION_235) ───────────────────────
// O request "MW" do Worker chega no k_nano p2p_tick como não-heartbeat →
// publicado no EventBus "P2P_PACKET". O bin chama `poll_mesh_requests()` a
// cada tick (bei_tick) — o Master responde com "MR". Subscribe lazy.

#[cfg(feature = "p2p")]
static MESH_RECV: Mutex<Option<event_bus::Receiver>> = Mutex::new(None);

/// Drena os pacotes P2P do EventBus e responde requests "MW\0" (Master side).
/// Chamado pelo bin a cada tick (bei_tick), depois do k_nano p2p_tick (que
/// publica). Só Master|Undecided e sem pressão de RAM (s364 fecha RX 1G).
#[cfg(feature = "p2p")]
pub fn poll_mesh_requests() {
    {
        let mut recv = MESH_RECV.lock();
        if recv.is_none() {
            *recv = Some(k_nano::EVENT_BUS.subscribe(k_nano::net::mesh::TOPIC_P2P_PACKET));
        }
    }
    // Drain sempre (não encher o bus), mas só Master/Undecided processa MW.
    // Memory/Compute/Worker: broadcast MW não é pra eles — servir #GP em 1G.
    let role = k_nano::net::mesh::local_role();
    let may_serve = matches!(
        role,
        k_nano::net::mesh::NodeRole::Master | k_nano::net::mesh::NodeRole::Undecided
    );
    let frugal = k_nano::memory::mesh_frag_pressure();
    loop {
        let evt = MESH_RECV.lock().as_ref().and_then(|r| r.try_receive());
        let Some(evt) = evt else { break };
        if evt.topic != k_nano::net::mesh::TOPIC_P2P_PACKET {
            continue;
        }
        if !may_serve || frugal {
            continue;
        }
        let Some(pkt) = k_nano::net::udp_broadcast::parse(&evt.payload) else { continue };
        if pkt.task_type != k_nano::net::noproto::TaskType::Inference {
            continue;
        }
        // SESSION_235: Undecided ainda responde (eleição tardia sob TCG).
        // s364: Worker/Memory/Compute NÃO — evita dual-MR + #GP em 1G.
        let payload = if evt.payload.len() > k_nano::net::noproto::PACKET_HEADER_SIZE {
            &evt.payload[k_nano::net::noproto::PACKET_HEADER_SIZE..]
        } else {
            &[][..]
        };
        if !payload.starts_with(b"MW\0") {
            continue;
        }
        let req_src = pkt.source_id;
        let Some(resp) = handle_mesh_request(payload) else {
            k_nano::slog_cortex!(
                "MESH", "warn",
                "matmul serve skip node={} (frag pressure/budget)",
                req_src
            );
            continue;
        };

        // Resposta "MR\0" — dest_id = node_id do Worker (filtro lógico no
        // receptor; o transporte é broadcast).
        let my_id = k_nano::net::mesh::node_id();
        // ADR-0081 follow-up: clock monotônico único por fonte (a resposta
        // MR também passa pelo anti-replay do RX no Worker).
        let rpkt = k_nano::net::noproto::AiosTaskPacket::new(
            k_nano::net::mesh::next_data_clock(), my_id, req_src, k_nano::net::noproto::TaskType::Inference,
            1, 0, 0, k_nano::net::noproto::PacketFlags::new(),
        );
        let mut buf = k_nano::net::udp_broadcast::serialize(&rpkt);
        buf.extend_from_slice(&resp);
        // ADR-0081 Tier F: resposta MR\0 é ponto-a-ponto (dest=req_src) →
        // SEAL com AEAD quando Tier Full + pk do Worker conhecida; senão cai
        // em sign_packet_tiered (HMAC Relativized / Ed25519 Full). O Worker só
        // aceita MR verificado/aberto contra a pk vinculada do remetente.
        let Some(signed) = k_nano::net::udp_broadcast::seal_packet_tiered(&buf, req_src) else {
            k_nano::slog_cortex!("MESH", "warn", "matmul resposta node={} sem sessao - skip", req_src);
            continue;
        };
        // SESSION_237: resposta grande (ex: matmul 64x64) fragmentada.
        let ok = k_nano::net::udp_broadcast::send_fragmented(&signed, 42069);
        k_nano::slog_cortex!(
            "MESH", "ok",
            "matmul resposta node={} sent={} bytes={}", req_src, ok, signed.len()
        );
    }
}

/// Self-test do matmul distribuído (Worker → Master → Worker).
/// SESSION_235: o DIAG de matmul do boot roda ANTES da eleição (role=Undecided)
/// — o caminho P2P nunca era exercitado. Chamado 1x pelo bei_tick quando o nó
/// é Worker com peer. SESSION_237: shape 64×64 (w 1KB + x 16KB ≈ 17.5KB) —
/// EXERCITA a fragmentação MTU (≈18 fragmentos FRAG\0), não mais o caso
/// ≤1200B direto.
#[cfg(feature = "p2p")]
pub fn mesh_matmul_self_test() {
    use crate::tensor::PackedTernaryTensor;
    // w: 64×64 ternário (padrão alternado +1/-1) → packed 1024 bytes.
    let n = 64 * 64;
    let mut wdata = Vec::with_capacity(n);
    for i in 0..n {
        wdata.push(if i % 2 == 0 { 1i8 } else { -1i8 });
    }
    let w = PackedTernaryTensor {
        shape: (64, 64),
        packed_data: PackedTernaryTensor::pack_weights(&wdata),
    };
    // x: 64×64 f32 (rampa 0..4095) → 16KB.
    let mut xdata = Vec::with_capacity(n);
    for i in 0..n {
        xdata.push(i as f32);
    }
    let x = crate::tensor::Tensor::from_row_major((64, 64), xdata)
        .unwrap_or_else(|| crate::tensor::Tensor::zero((64, 64)));
    let my_id = k_nano::net::mesh::node_id();
    let before = N_MESH.load(Ordering::Relaxed);
    match dispatch_ternary(&w, &x) {
        Some(r) => {
            let via_mesh = N_MESH.load(Ordering::Relaxed) > before;
            k_nano::slog_cortex!(
                "MESH", "ok",
                "self-test node={} shape=({}, {}) primeiro={:.1} ({})",
                my_id,
                r.shape.0,
                r.shape.1,
                r.data.first().copied().unwrap_or(0.0),
                if via_mesh { "mesh FRAG" } else { "local accel" }
            );
        }
        None => k_nano::slog_cortex!(
            "MESH", "warn",
            "self-test node={} fallback local (timeout/MTU/sem Master)", my_id
        ),
    }
}

#[cfg(all(test, feature = "p2p"))]
mod mesh_preflight_tests {
    use super::*;

    fn mk_w(k: usize, n: usize) -> PackedTernaryTensor {
        PackedTernaryTensor { shape: (k, n), packed_data: alloc::vec![0u8; (k * n + 3) / 4] }
    }

    /// Estimativa byte-exata com o serialize real (payload e wire completo).
    #[test]
    fn estimate_request_bate_serialize_byte_exact() {
        let w = mk_w(64, 64);
        let x = Tensor::zero((64, 64));
        let payload = serialize_mesh_request(&w, &x).unwrap();
        assert_eq!(
            estimate_mesh_request_payload_size(&w, &x),
            Some(payload.len()),
            "payload estimate ≠ serialize"
        );
        assert_eq!(
            estimate_mesh_request_wire_size(&w, &x),
            Some(
                payload.len()
                    + k_nano::net::noproto::PACKET_HEADER_SIZE
                    + k_nano::identity::SIGNATURE_LEN
            ),
            "wire estimate ≠ header+payload+assinatura"
        );
    }

    /// Resposta: estimativa byte-exata com a serialização "MR\0" do Master.
    #[test]
    fn estimate_resposta_bate_serialize_byte_exact() {
        let (rows, cols) = (64usize, 64usize);
        let mut out = Vec::new();
        out.extend_from_slice(b"MR\0");
        out.extend_from_slice(&(rows as u32).to_le_bytes());
        out.extend_from_slice(&(cols as u32).to_le_bytes());
        for i in 0..rows * cols {
            out.extend_from_slice(&(i as f32).to_le_bytes());
        }
        assert_eq!(estimate_mesh_response_payload_size(rows, cols), Some(out.len()));
        assert_eq!(
            estimate_mesh_response_wire_size(rows, cols),
            Some(
                out.len()
                    + k_nano::net::noproto::PACKET_HEADER_SIZE
                    + k_nano::identity::SIGNATURE_LEN
            )
        );
    }

    /// Fronteira do teto do wire: 64.000 passa; 64.001 recusa por wire_limit.
    /// Budget decide abaixo do teto do wire.
    #[test]
    fn fronteira_64k() {
        assert_eq!(mesh_frag_gate(MESH_FRAG_WIRE_LIMIT, usize::MAX), None);
        assert_eq!(
            mesh_frag_gate(MESH_FRAG_WIRE_LIMIT + 1, usize::MAX),
            Some("wire_limit")
        );
        assert_eq!(mesh_frag_gate(1000, 999), Some("frag_budget"));
        assert_eq!(mesh_frag_gate(1000, 1000), None);
    }

    /// Job oversized é recusado SÓ com a estimativa (o gate decide antes do
    /// serialize — o Vec real só é construído aqui p/ provar byte-exatidão).
    #[test]
    fn oversized_recusado_pre_serialize() {
        // w 512×512 packed = 64KB + x 512×512 f32 = 1MB → est ≫ 64.000.
        let w = mk_w(512, 512);
        let x = Tensor::zero((512, 512));
        let est = estimate_mesh_request_wire_size(&w, &x).unwrap();
        assert!(est > MESH_FRAG_WIRE_LIMIT);
        assert_eq!(mesh_frag_gate(est, usize::MAX), Some("wire_limit"));
        // Resposta 512×512 f32 = 1MB também estoura o teto.
        let resp_est = estimate_mesh_response_wire_size(512, 512).unwrap();
        assert!(resp_est > MESH_FRAG_WIRE_LIMIT);
        // Serialize real confirma a estimativa (recusa veio ANTES do Vec).
        let payload = serialize_mesh_request(&w, &x).unwrap();
        assert_eq!(
            payload.len()
                + k_nano::net::noproto::PACKET_HEADER_SIZE
                + k_nano::identity::SIGNATURE_LEN,
            est
        );
    }

    /// Fronteira exata no teto: shapes montadas p/ est == 64.000 passam.
    #[test]
    fn request_na_fronteira_exata_64k() {
        // wire = ovh + (hdr + wbytes + 4*xlen); ovh = header pacote + assinatura.
        let ovh = k_nano::net::noproto::PACKET_HEADER_SIZE + k_nano::identity::SIGNATURE_LEN;
        let hdr = 19usize; // "MW\0" + 2 shapes u32 (byte-exato c/ serialize)
        let xlen = 1000usize;
        let wbytes = MESH_FRAG_WIRE_LIMIT - ovh - hdr - xlen * 4;
        // k*n = wbytes*4 (≡0 mod 4 → ceil div == wbytes); fatoração k=4.
        let w = mk_w(4, wbytes);
        let x = Tensor::zero((xlen, 1));
        let est = estimate_mesh_request_wire_size(&w, &x).unwrap();
        assert_eq!(est, MESH_FRAG_WIRE_LIMIT, "est deve cravar 64.000");
        assert_eq!(mesh_frag_gate(est, usize::MAX), None);
    }

    /// Aritmética checked: shapes absurdas → estimate `None` (o caller recusa
    /// com estimate_overflow em vez de wrap silencioso). Tensor com
    /// packed_data ≠ ceil(k*n/4) também é recusado (o Master parseia wbytes
    /// da shape — inconsistência viraria lixo lá).
    #[test]
    fn overflow_e_inconsistencia_viram_none() {
        let big = usize::MAX / 2;
        // k*n transborda usize → None.
        let w_ovf = PackedTernaryTensor { shape: (big, big), packed_data: alloc::vec![0u8; 1] };
        let x = Tensor::zero((1, 1));
        assert_eq!(estimate_mesh_request_payload_size(&w_ovf, &x), None);
        assert_eq!(estimate_mesh_request_wire_size(&w_ovf, &x), None);
        // Shape consistente mas packed_data ≠ ceil(k*n/4) → None.
        let w_bad = PackedTernaryTensor { shape: (4, 4), packed_data: alloc::vec![0u8; 1] };
        assert_eq!(estimate_mesh_request_payload_size(&w_bad, &x), None);
        // rows*cols*4 transborda (u32::MAX² cabe em usize, ×4 não) → None.
        let umax = u32::MAX as usize;
        assert_eq!(estimate_mesh_response_payload_size(umax, umax), None);
        assert_eq!(estimate_mesh_response_wire_size(umax, umax), None);
    }
}

// ─── s458: wiring peer_available + retry bounded (LOG AGENTES (4).txt P1) ───
// Testes do contrato do lane de wiring: gate único `peer_available` antes do
// dispatch, limite de tentativas, backoff determinístico, sem retenção de
// buffers entre recusas. Statics de health compartilhados → rodar com
// `--test-threads=1` (lição SESSION_346).
#[cfg(all(test, feature = "p2p"))]
mod mesh_retry_tests {
    use super::*;
    use k_nano::net::mesh::{
        attempt_allowed, backoff_delay_ticks, peer_available, peer_state, peer_health,
        record_peer_failure, record_peer_success, refusal_backoff_ticks, PeerState,
        MAX_TASK_ATTEMPTS, REFUSAL_BACKOFF_BASE_TICKS, REFUSAL_BACKOFF_MAX_TICKS,
    };

    /// Gate único `peer_available`: 3 recusas (circuit breaker) → Unavailable
    /// → dispatch fecha. O peer AINDA existe na tabela de health (telemetria
    /// o vê) — divergência telemetria-vs-compute visível, resolvida pelo gate
    /// único (checado ANTES de N_MESH/serialize no dispatch → sem serialize).
    #[test]
    fn peer_unavailable_fecha_dispatch() {
        // Sucesso cria entrada Available (reachable, 0 falhas, atividade agora).
        record_peer_success(42, 100);
        assert_eq!(peer_state(42), Some(PeerState::Available));
        assert!(peer_available(42));
        // 3 recusas → circuit breaker → Unavailable.
        for _ in 0..3 {
            record_peer_failure(42);
        }
        assert_eq!(peer_state(42), Some(PeerState::Unavailable));
        assert!(!peer_available(42), "gate único deve fechar o dispatch");
        // Divergência: o peer ainda existe p/ a telemetria (health table),
        // mas o compute não despacha — exatamente o LOG AGENTES (4).txt P1.
        assert!(peer_health(42).is_some());
    }

    /// Limite de tentativas para os retries: MAX_TASK_ATTEMPTS sends e o gate
    /// `attempt_allowed` fecha (stop-condition do topo do loop do worker).
    #[test]
    fn attempt_limit_para_retries() {
        assert!(attempt_allowed(0, MAX_TASK_ATTEMPTS));
        assert!(attempt_allowed(1, MAX_TASK_ATTEMPTS));
        assert!(attempt_allowed(2, MAX_TASK_ATTEMPTS));
        assert!(!attempt_allowed(3, MAX_TASK_ATTEMPTS), "3 recusas = stop");
        assert!(!attempt_allowed(5, MAX_TASK_ATTEMPTS));
        // Contabilidade do loop do worker (mesmo contrato): exatamente MAX
        // sends, depois o gate para — sem retries eternos.
        let mut attempts = 0u32;
        let mut sends = 0u32;
        while attempt_allowed(attempts, MAX_TASK_ATTEMPTS) {
            sends += 1;
            attempts += 1; // recusa (sem rede no host)
        }
        assert_eq!(sends, MAX_TASK_ATTEMPTS);
        assert!(!attempt_allowed(attempts, MAX_TASK_ATTEMPTS));
    }

    /// Backoff determinístico por seed: mesmo (attempt, base, seed) → mesmo
    /// delay; jitter só subtrai (≤25%); cresce entre tentativas; capado.
    #[test]
    fn backoff_deterministico_seeded() {
        let base = REFUSAL_BACKOFF_BASE_TICKS;
        // Determinismo: mesma seed → mesmo valor.
        assert_eq!(
            refusal_backoff_ticks(1, base, 7),
            refusal_backoff_ticks(1, base, 7)
        );
        // Jitter só subtrai: delay ∈ [pure - pure/4, pure].
        for attempt in 1..4u32 {
            let pure = backoff_delay_ticks(attempt, base);
            let d = refusal_backoff_ticks(attempt, base, 42);
            assert!(d <= pure, "jitter subtrai");
            assert!(d >= pure - pure / 4, "jitter ≤ 25%");
        }
        // Crescimento entre tentativas (2× domina o jitter de 25%).
        let d1 = refusal_backoff_ticks(1, base, 42);
        let d2 = refusal_backoff_ticks(2, base, 42);
        assert!(d2 > d1, "backoff cresce entre tentativas");
        // Cap: delay puro crava o teto; jittered fica em [teto - teto/4, teto]
        // (jitter só subtrai — o cap é preservado como teto superior).
        assert_eq!(backoff_delay_ticks(6, base), REFUSAL_BACKOFF_MAX_TICKS);
        let dcap = refusal_backoff_ticks(31, base, 42);
        assert!(dcap <= REFUSAL_BACKOFF_MAX_TICKS);
        assert!(dcap >= REFUSAL_BACKOFF_MAX_TICKS - REFUSAL_BACKOFF_MAX_TICKS / 4);
    }

    /// Buffers não retidos entre tentativas: serialize é POR tentativa (puro
    /// de (w,x), sem estado acumulado) e o preflight M1 recusa oversized
    /// ANTES de qualquer Vec — o retry loop não reintroduz retenção.
    #[test]
    fn buffers_nao_retidos_entre_tentativas() {
        let w = PackedTernaryTensor { shape: (64, 64), packed_data: alloc::vec![0u8; 1024] };
        let x = Tensor::zero((64, 64));
        // Serialize puro: mesma entrada → mesmos bytes (cada retry serializa
        // fresco e dropa — nada retido entre recusas).
        let p1 = serialize_mesh_request(&w, &x).unwrap();
        let p2 = serialize_mesh_request(&w, &x).unwrap();
        assert_eq!(p1, p2, "serialize por tentativa é puro (sem retenção)");
        // Preflight M1 fora do loop: oversized recusado com estimativa
        // zero-alloc (o Vec real só é construído aqui p/ provar o gate).
        let big_w = PackedTernaryTensor { shape: (512, 512), packed_data: alloc::vec![0u8; 65536] };
        let big_x = Tensor::zero((512, 512));
        let est = estimate_mesh_request_wire_size(&big_w, &big_x).unwrap();
        assert_eq!(mesh_frag_gate(est, usize::MAX), Some("wire_limit"));
    }
}
