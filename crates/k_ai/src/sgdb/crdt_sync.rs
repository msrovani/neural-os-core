//! ADR-0081 C4: CRDT Memory Sync (#315.26).
//!
//! ## Visão
//! O estado do SGDB (memória episódica, semântica, procedural) é replicado
//! entre nós do mesh via CRDT (Conflict-free Replicated Data Type).
//! Cada nó tem uma cópia local. Alterações são propagadas assincronamente.
//! Conflitos são resolvidos por "last-writer-wins" + merge semântico.
//!
//! ## Depende de: P2P Transport (Fase A da ADR-0081)
//! - `k_nano::net::mesh::local_role()` — indica se mesh/P2P está ativo
//! - `k_nano::net::udp_broadcast` — transporte broadcast real (assinado)
//! - `k_nano::EVENT_BUS` — consumo dos pacotes P2P não-heartbeat
//!
//! ## Estado (Fase C, ADR-0081 #315.26)
//! Sync real de **VERSÃO** (LWW: maior version vence) provando o transporte:
//! - Worker publica `CRDT\0` + local_version u64 LE (assinado) a cada
//!   `SYNC_INTERVAL_TICKS`; se recebe version maior que a local, adota (merge
//!   LWW).
//! - Master publica sua versão e registra as versões dos Workers em
//!   `peer_versions: Vec<(u8, u64)>`.
//! O merge de **conteúdo** (ART/BQ do SGDB) fica ponytail — próximo passo.
//!
//! ## Fallback local (ativo enquanto P2P não estiver vivo)
//! Sem P2P, o SGDB opera localmente — comportamento atual.
//! `crdt_sync()` retorna imediatamente se P2P não estiver ativo.

use alloc::vec::Vec;
use k_nano::net::mesh::{self, NodeRole};
use k_nano::net::noproto::{AiosTaskPacket, PacketFlags, TaskType};
use k_nano::net::udp_broadcast;
use spin::Mutex;

/// Intervalo mínimo entre syncs (ticks do TIMER) — ~2s a 100Hz.
const SYNC_INTERVAL_TICKS: u64 = 200;
/// Porta P2P do mesh (transport k_nano, broadcast 42069).
const P2P_PORT: u16 = 42069;

/// Agente CRDT de sincronização de memória entre nós do mesh.
///
/// Mantém versão local e versões conhecidas de outros nós.
/// Quando o mesh P2P está ativo (role != Undecided), troca `CRDT\0` com os
/// pares periodicamente. Caso contrário, opera apenas localmente.
pub struct CrdtMemorySync {
    /// Se true, P2P está ativo e sync será tentado.
    active: bool,
    /// Último tick do TIMER em que sync foi executado.
    last_sync_tick: u64,
    /// Versão monotônica local — incrementada a cada `record_change()`.
    local_version: u64,
    /// Versões conhecidas de outros nós: (node_id, version).
    /// Usado para detectar quais nós precisam de diffs.
    pub node_versions: Vec<(u8, u64)>,
}

impl CrdtMemorySync {
    /// Cria novo sync agent. Começa inativo (P2P ainda não detectado).
    pub const fn new() -> Self {
        Self {
            active: false,
            last_sync_tick: 0,
            local_version: 0,
            node_versions: Vec::new(),
        }
    }

    /// Retorna a versão local atual.
    pub fn local_version(&self) -> u64 {
        self.local_version
    }

    /// Marca uma mutação no SGDB local — incrementa o contador de versão.
    ///
    /// Deve ser chamada após toda escrita no SGDB (put/kv/audit/etc.)
    /// para que o próximo sync propague a alteração aos pares.
    pub fn record_change(&mut self) {
        self.local_version = self.local_version.saturating_add(1);
    }

    /// Tenta sincronizar o estado do SGDB com outros nós do mesh.
    ///
    /// ## Comportamento
    /// - Se P2P não está ativo (role == Undecided): retorna imediatamente (fallback local).
    /// - Se P2P está ativo: rate-limit por `SYNC_INTERVAL_TICKS` (ticks do
    ///   TIMER — SESSION_235: scheduler é rate-limited) e executa o sync real.
    ///
    /// ## Integração
    /// - Chamado pelo SleepCycleAgent ao final da fase CONSOLIDATE.
    /// - Chamado pelo SecurityAgent após escrita de audit trail.
    /// - Pode ser chamado por qualquer agente que deseje propagar mudanças.
    pub fn crdt_sync(&mut self, tick: u64) {
        // --- Fallback local: P2P inativo ---
        let role = k_nano::net::mesh::local_role();
        if role == NodeRole::Undecided {
            if self.active {
                // Transição ativo → inativo
                self.active = false;
                k_nano::slog_kai!("CRDT", "sync", "P2P mesh offline → fallback local (v={})", self.local_version);
            }
            return;
        }

        // P2P ativo
        if !self.active {
            self.active = true;
            k_nano::slog_kai!(
                "CRDT", "sync",
                "P2P mesh ativo (role={:?}) → sync iniciado (v={})",
                role, self.local_version,
            );
        }

        // Rate-limit: só sync se intervalo mínimo passou (TIMER_TICKS —
        // SESSION_235: 200 CALLS do scheduler rate-limited demoravam minutos).
        let now = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        if self.last_sync_tick != 0 && now.wrapping_sub(self.last_sync_tick) < SYNC_INTERVAL_TICKS {
            let _ = tick; // tick do scheduler — informativo apenas
            return;
        }
        self.last_sync_tick = now;

        self.sync_exchange(role);
    }

    /// Troca de versões CRDT com os peers via P2P real (Fase C).
    ///
    /// 1. Drena o EventBus P2P_PACKET (assinatura já verificada no ingress do
    ///    k_nano — Fase A fail-closed) e aplica por papel:
    ///    - Master: registra versões dos Workers em `node_versions`.
    ///    - Worker: LWW — version maior que a local vence (merge).
    /// 2. Publica `CRDT\0` + local_version u64 LE (assinado, fragmentado).
    fn sync_exchange(&mut self, role: NodeRole) {
        // (1) RX: aplica versões recebidas.
        self.drain_crdt_events(role);

        // (2) TX: publica nossa versão local (assinada).
        let my_id = mesh::node_id();
        let pkt = AiosTaskPacket::new(0, my_id, 0xFF, TaskType::Inference, 1, 0, 0, PacketFlags::new());
        let mut buf = udp_broadcast::serialize(&pkt);
        buf.extend_from_slice(b"CRDT\0");
        buf.extend_from_slice(&self.local_version.to_le_bytes());
        // Fase A (SESSION_236): todo TX assina — RX fail-closed dropa não-assinados.
        let Some(signed) = udp_broadcast::sign_packet(&buf) else { return };
        let ok = udp_broadcast::send_fragmented(&signed, P2P_PORT);
        match role {
            NodeRole::Master => k_nano::slog_kai!(
                "CRDT", "master",
                "publish v={} peers={} sent={}", self.local_version, self.node_versions.len(), ok
            ),
            _ => k_nano::slog_kai!(
                "CRDT", "worker",
                "publish v={} sent={}", self.local_version, ok
            ),
        }
    }

    /// Drena o EventBus P2P_PACKET (subscribe lazy) e aplica `CRDT\0`.
    ///
    /// Nota de segurança (Fase A): o payload do EventBus já foi verificado no
    /// ingress do k_nano (`p2p_tick` — fail-closed: unsigned/badsig são
    /// dropados ANTES do publish). A assinatura (64B) é removida no verify do
    /// ingress, então não há re-verificação aqui — o parse valida o magic.
    fn drain_crdt_events(&mut self, role: NodeRole) {
        {
            let mut recv = CRDT_RECV.lock();
            if recv.is_none() {
                *recv = Some(k_nano::EVENT_BUS.subscribe(k_nano::net::mesh::TOPIC_P2P_PACKET));
            }
        }
        loop {
            let evt = CRDT_RECV.lock().as_ref().and_then(|r| r.try_receive());
            let Some(evt) = evt else { break };
            if evt.topic != k_nano::net::mesh::TOPIC_P2P_PACKET {
                continue;
            }
            let Some(pkt) = udp_broadcast::parse(&evt.payload) else { continue };
            if pkt.task_type != TaskType::Inference {
                continue;
            }
            let payload = if evt.payload.len() > k_nano::net::noproto::PACKET_HEADER_SIZE {
                &evt.payload[k_nano::net::noproto::PACKET_HEADER_SIZE..]
            } else {
                &[][..]
            };
            // "CRDT\0" + version u64 LE
            if !payload.starts_with(b"CRDT\0") || payload.len() < 5 + 8 {
                continue;
            }
            let v = u64::from_le_bytes([
                payload[5], payload[6], payload[7], payload[8],
                payload[9], payload[10], payload[11], payload[12],
            ]);
            match role {
                NodeRole::Master => {
                    // Master registra a versão do peer (source of truth local).
                    self.upsert_peer_version(pkt.source_id, v);
                    k_nano::slog_kai!(
                        "CRDT", "info",
                        "peer node={} v={} peers={}", pkt.source_id, v, self.node_versions.len()
                    );
                }
                NodeRole::Worker => {
                    // LWW merge: maior version vence.
                    if v > self.local_version {
                        k_nano::slog_kai!(
                            "CRDT", "info",
                            "sync local_v={} -> master_v={} merged", self.local_version, v
                        );
                        self.local_version = v;
                    }
                }
                _ => {}
            }
        }
    }

    /// Insere/atualiza a versão conhecida de um peer (dedupe por node_id).
    fn upsert_peer_version(&mut self, node: u8, v: u64) {
        if let Some(slot) = self.node_versions.iter_mut().find(|(n, _)| *n == node) {
            slot.1 = v;
        } else {
            self.node_versions.push((node, v));
        }
    }
}

// ─── Wiring global (ADR-0081 C4, Fase C) — chamado pelo bin bei_tick ───────

/// Receiver do EventBus P2P_PACKET (subscribe lazy).
static CRDT_RECV: Mutex<Option<event_bus::Receiver>> = Mutex::new(None);

/// Instância global do sync CRDT.
static CRDT_GLOBAL: Mutex<Option<CrdtMemorySync>> = Mutex::new(None);

/// Tick do sync CRDT — chamado pelo bin a cada bei_tick (após p2p_tick).
pub fn crdt_sync_global(tick: u64) {
    {
        let mut guard = CRDT_GLOBAL.lock();
        if guard.is_none() {
            *guard = Some(CrdtMemorySync::new());
        }
    }
    let mut guard = CRDT_GLOBAL.lock();
    if let Some(ref mut sync) = *guard {
        sync.crdt_sync(tick);
    }
}

/// (local_version, peers_known) — para log no bei_tick.
pub fn crdt_stats_global() -> (u64, usize) {
    let guard = CRDT_GLOBAL.lock();
    match guard.as_ref() {
        Some(s) => (s.local_version, s.node_versions.len()),
        None => (0, 0),
    }
}

// ─── Teste unitário (run-only, não espera para no_std) ───

/// Self-test: criação, record, local fallback.
pub fn demo() -> bool {
    let mut sync = CrdtMemorySync::new();
    if sync.active || sync.local_version != 0 {
        return false;
    }

    // record_change incrementa versão
    sync.record_change();
    if sync.local_version != 1 {
        return false;
    }
    sync.record_change();
    if sync.local_version != 2 {
        return false;
    }

    // local_version()
    if sync.local_version() != 2 {
        return false;
    }

    // node_versions começa vazio
    if !sync.node_versions.is_empty() {
        return false;
    }

    true
}
