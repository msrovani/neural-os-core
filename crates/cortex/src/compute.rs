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
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
#[cfg(feature = "p2p")]
use alloc::vec::Vec;
#[cfg(feature = "p2p")]
use spin::Mutex;

/// Assinatura de um backend de matmul ternário (BitNet).
pub type TernaryFn = fn(&PackedTernaryTensor, &Tensor) -> Option<Tensor>;

// Slots de registro (0 = não registrado). fn-pointer cabe em usize no alvo.
static GPU_TERNARY: AtomicUsize = AtomicUsize::new(0);
static NPU_TERNARY: AtomicUsize = AtomicUsize::new(0);

// Telemetria (ADR-0057 + ADR-0061): quantas ops cada anel tratou.
static N_NPU: AtomicU64 = AtomicU64::new(0);
static N_GPU: AtomicU64 = AtomicU64::new(0);
static N_SMP: AtomicU64 = AtomicU64::new(0);
static N_AVX512: AtomicU64 = AtomicU64::new(0);
static N_CPU: AtomicU64 = AtomicU64::new(0);
// ADR-0081 C1: ops despachadas para o mesh (Worker → Master)
static N_MESH: AtomicU64 = AtomicU64::new(0);

/// Ring 0 (intent/router) — registrado por `k_ai` quando uma NPU fica pronta.
pub fn register_npu_ternary(f: TernaryFn) {
    NPU_TERNARY.store(f as usize, Ordering::Release);
    k_nano::slog_nano!("COMPUTE", "info", "NPU ternary backend registrado (Ring0)");
}

/// Ring 1 (matmul pesado) — registrado por `k_hal` quando o canário GPU passa.
pub fn register_gpu_ternary(f: TernaryFn) {
    GPU_TERNARY.store(f as usize, Ordering::Release);
    k_nano::slog_nano!("COMPUTE", "info", "GPU ternary backend registrado (Ring1)");
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

    // ADR-0081 C1: Mesh-aware dispatch.
    // Se o no local e Worker, despacha o matmul para o Master via P2P real
    // (payload "MW\0..." → resposta "MR\0..."). SESSION_235: implementado.
    #[cfg(feature = "p2p")]
    {
        let role = k_nano::net::mesh::local_role();
        if role == k_nano::net::mesh::NodeRole::Worker {
            N_MESH.fetch_add(1, Ordering::Relaxed);
            // SESSION_235: matmul ternário distribuído — Worker serializa w+x,
            // envia para o Master, espera síncrona (~200 TIMER_TICKS) a resposta.
            // ponytail: síncrono + gate MTU 1200B — assíncrono/fragmentação = futuro.
            if let Some(t) = mesh_matmul_worker(w, x) {
                return Some(t);
            }
            // Timeout/sem resposta → cai no fallback local (CPU/scalar).
        }
    }

    // Ring 0 — NPU (router/intent, latência-crítico). Só se registrado.
    if let Some(r) = call_slot(NPU_TERNARY.load(Ordering::Acquire), w, x) {
        N_NPU.fetch_add(1, Ordering::Relaxed);
        return Some(r);
    }

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
// (send_fragmented/recv_fragmented) — sem gate de tamanho aqui.

/// Serializa w+x num request "MW\0". Sem gate de tamanho — payloads grandes
/// são fragmentados pelo transporte (SESSION_237).
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

/// Worker side: envia request "MW\0" e espera síncrona pela resposta "MR\0".
/// Timeout ~200 TIMER_TICKS (~2s a 100Hz). Retorna `None` em timeout/falha →
/// o dispatch cai no fallback local. Pacotes que não são a nossa resposta são
/// descartados (não re-injetados no RX do mesh).
#[cfg(feature = "p2p")]
fn mesh_matmul_worker(w: &PackedTernaryTensor, x: &Tensor) -> Option<Tensor> {
    let payload = serialize_mesh_request(w, x)?;
    let node_id = k_nano::net::mesh::node_id();
    let pkt = k_nano::net::noproto::AiosTaskPacket::new(
        0, node_id, 0xFF, k_nano::net::noproto::TaskType::Inference,
        1, 0, 0, k_nano::net::noproto::PacketFlags::new(),
    );
    let mut buf = k_nano::net::udp_broadcast::serialize(&pkt);
    buf.extend_from_slice(&payload);
    // Fase A (SESSION_236): assinado — o RX fail-closed dropa não-assinados.
    let Some(signed) = k_nano::net::udp_broadcast::sign_packet(&buf) else {
        return None; // fail-closed: sem sessão não assina
    };
    // SESSION_237: fragmenta se > 1200B (o blob assinado é dividido; o
    // receptor reassembla antes do verify_packet).
    let ok = k_nano::net::udp_broadcast::send_fragmented(&signed, 42069);
    k_nano::slog_cortex!(
        "MESH", "info",
        "matmul request node={} size={} sent={}", node_id, payload.len(), ok
    );
    if !ok {
        return None;
    }

    // Espera síncrona com timeout real por TIMER_TICKS (não iteração cega —
    // o scheduler pode não rodar durante a espera).
    let start = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
    loop {
        let now = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        if now.wrapping_sub(start) >= 200 {
            break; // timeout — fallback local
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
                // usam tiered (HMAC em Relativized, Ed25519 em Full).
                let Some(pk) = k_nano::net::mesh::peer_public_key(sender) else { continue };
                let Some(valid) = k_nano::net::udp_broadcast::verify_packet_tiered(&rx, &pk) else {
                    continue; // autenticação inválida — DROP
                };
                if valid.len() <= k_nano::net::noproto::PACKET_HEADER_SIZE {
                    continue;
                }
                let resp = &valid[k_nano::net::noproto::PACKET_HEADER_SIZE..];
                if let Some(t) = deserialize_mesh_response(resp) {
                    k_nano::slog_cortex!(
                        "MESH", "info",
                        "matmul resposta node={} ok shape={:?}", node_id, t.shape
                    );
                    return Some(t);
                }
            }
            // Não é a nossa resposta — DROP (não re-injetar no RX do mesh).
        }
        core::hint::spin_loop();
    }
    k_nano::slog_cortex!("MESH", "info", "matmul timeout node={} - fallback local", node_id);
    None
}

/// Master side: processa request "MW\0" e retorna resposta "MR\0" serializada.
#[cfg(feature = "p2p")]
pub fn handle_mesh_request(payload: &[u8]) -> Option<Vec<u8>> {
    // "MW\0" + k u32 + n u32 + packed + rows u32 + cols u32 + data f32 LE
    if payload.len() < 19 || &payload[0..3] != b"MW\0" {
        return None;
    }
    let k = u32::from_le_bytes([payload[3], payload[4], payload[5], payload[6]]) as usize;
    let n = u32::from_le_bytes([payload[7], payload[8], payload[9], payload[10]]) as usize;
    // 2-bit packing (4 pesos/byte) — ceil div por 4.
    let wbytes = k.checked_mul(n)?.checked_add(3)? / 4;
    let mut off = 11;
    let packed = payload.get(off..off + wbytes)?.to_vec();
    off += wbytes;
    let rows = u32::from_le_bytes([payload[off], payload[off + 1], payload[off + 2], payload[off + 3]]) as usize;
    let cols = u32::from_le_bytes([payload[off + 4], payload[off + 5], payload[off + 6], payload[off + 7]]) as usize;
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
/// publica). Só responde se `local_role() == Master`.
#[cfg(feature = "p2p")]
pub fn poll_mesh_requests() {
    {
        let mut recv = MESH_RECV.lock();
        if recv.is_none() {
            *recv = Some(k_nano::EVENT_BUS.subscribe(k_nano::net::mesh::TOPIC_P2P_PACKET));
        }
    }
    loop {
        let evt = MESH_RECV.lock().as_ref().and_then(|r| r.try_receive());
        let Some(evt) = evt else { break };
        if evt.topic != k_nano::net::mesh::TOPIC_P2P_PACKET {
            continue;
        }
        let Some(pkt) = k_nano::net::udp_broadcast::parse(&evt.payload) else { continue };
        if pkt.task_type != k_nano::net::noproto::TaskType::Inference {
            continue;
        }
        // SESSION_235: responde MW mesmo se Undecided — o request só chega
        // a quem recebeu o broadcast (o Worker não recebe o próprio TX); sob
        // TCG o Master pode ainda não ter eleito (Undecided) quando o request
        // chega, e o gate "só Master" fazia o Worker dar timeout.
        let payload = if evt.payload.len() > k_nano::net::noproto::PACKET_HEADER_SIZE {
            &evt.payload[k_nano::net::noproto::PACKET_HEADER_SIZE..]
        } else {
            &[][..]
        };
        if !payload.starts_with(b"MW\0") {
            continue;
        }
        let req_src = pkt.source_id;
        let Some(resp) = handle_mesh_request(payload) else { continue };

        // Resposta "MR\0" — dest_id = node_id do Worker (filtro lógico no
        // receptor; o transporte é broadcast).
        let my_id = k_nano::net::mesh::node_id();
        let rpkt = k_nano::net::noproto::AiosTaskPacket::new(
            0, my_id, req_src, k_nano::net::noproto::TaskType::Inference,
            1, 0, 0, k_nano::net::noproto::PacketFlags::new(),
        );
        let mut buf = k_nano::net::udp_broadcast::serialize(&rpkt);
        buf.extend_from_slice(&resp);
        // Fase A (SESSION_236): assinado — o Worker só aceita MR verificado
        // contra a pk vinculada do remetente.
        let Some(signed) = k_nano::net::udp_broadcast::sign_packet(&buf) else {
            k_nano::slog_cortex!("MESH", "info", "matmul resposta node={} sem sessao - skip", req_src);
            continue;
        };
        // SESSION_237: resposta grande (ex: matmul 64x64) fragmentada.
        let ok = k_nano::net::udp_broadcast::send_fragmented(&signed, 42069);
        k_nano::slog_cortex!(
            "MESH", "info",
            "matmul resposta node={} sent={}", req_src, ok
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
    match dispatch_ternary(&w, &x) {
        Some(r) => k_nano::slog_cortex!(
            "MESH", "info",
            "self-test node={} shape=({}, {}) primeiro={:.1} (mesh dispatch)",
            my_id, r.shape.0, r.shape.1, r.data.first().copied().unwrap_or(0.0)
        ),
        None => k_nano::slog_cortex!(
            "MESH", "info",
            "self-test node={} fallback local (timeout/MTU/sem Master)", my_id
        ),
    }
}
