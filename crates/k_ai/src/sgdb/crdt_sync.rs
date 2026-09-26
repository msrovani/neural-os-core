//! ADR-0081 C4: CRDT Memory Sync (#315.26).
//!
//! ## Visão
//! O estado do SGDB (memória episódica, semântica, procedural) é replicado
//! entre nós do mesh via CRDT (Conflict-free Replicated Data Type).
//! Cada nó tem uma cópia local. Alterações são propagadas assincronamente.
//! Conflitos são resolvidos pela **política CRDT única** do neural-sgdb
//! (`merge_remote_nsgdb`: MergePolicy::for_layer + happens-before + conflito
//! preservado) — a MESMA política do RX de memória do `mesh_knowledge`
//! (s410l: um só veredicto, um só código de merge nos dois caminhos).
//!
//! ## Depende de: P2P Transport (Fase A da ADR-0081)
//! - `k_nano::net::mesh::local_role()` — indica se mesh/P2P está ativo
//! - `k_nano::net::udp_broadcast` — transporte broadcast real (assinado)
//! - `k_nano::EVENT_BUS` — consumo dos pacotes P2P não-heartbeat
//!
//! ## Estado (Fase C, ADR-0081 #315.26 + s410l)
//! Sync de **VERSÃO** (LWW do envelope `CRDT\0` + u64 LE) provando o
//! transporte + **blob de conteúdo com frames NMD1**:
//! - Worker publica `CRDT\0` + local_version u64 LE + blob (assinado) a cada
//!   `SYNC_INTERVAL_TICKS`.
//! - Master registra as versões dos Workers em `peer_versions: Vec<(u8, u64)>`.
//! - Blob: frames NMD1 concatenados (docs tipados do NSGDB — exportados do
//!   motor). O RX decodifica cada frame e aplica via `merge_remote_nsgdb`
//!   (política por camada; sem local = Applied; stale = ignorado).
//!   Blob opaco legado (não-NMD1) = merge LWW do envelope (fallback s385) —
//!   era o estado antes do s410l; nós velhos convergem para o novo formato
//!   no próximo TX (o RX atualiza o blob local com o payload recebido).
//!
//! ## Fallback local (ativo enquanto P2P não estiver vivo)
//! Sem P2P, o SGDB opera localmente — comportamento atual.
//! `crdt_sync()` retorna imediatamente se P2P não estiver ativo.

use alloc::vec::Vec;
use super::memory_doc::MemoryDoc;
use k_nano::net::mesh::{self, NodeRole};
use k_nano::net::noproto::{AiosTaskPacket, PacketFlags, TaskType};
use k_nano::net::udp_broadcast;
use spin::Mutex;

/// Intervalo mínimo entre syncs (ticks do TIMER) — ~2s a 100Hz.
const SYNC_INTERVAL_TICKS: u64 = 200;
/// Porta P2P do mesh (transport k_nano, broadcast 42069).
const P2P_PORT: u16 = 42069;
/// s410l: teto de docs NMD1 exportados por TX (memória L2/L3/L4 mais
/// recente do scan). Fragmentação do transporte (mesh_send_large/send_fragmented)
/// paga o resto; o teto só limita o tamanho do envelope por sync.
const TX_MAX_DOCS: usize = 32;

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
    pub node_versions: Vec<(u8, u64)>,
    /// Último blob local (merge T-048); vazio = só versão.
    local_blob: Vec<u8>,
}

impl CrdtMemorySync {
    /// Cria novo sync agent. Começa inativo (P2P ainda não detectado).
    pub const fn new() -> Self {
        Self {
            active: false,
            last_sync_tick: 0,
            local_version: 0,
            node_versions: Vec::new(),
            local_blob: Vec::new(),
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
                k_nano::slog_kai!("CRDT", "warn", "P2P mesh offline → fallback local (v={})", self.local_version);
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
        // s410l: blob de conteúdo = frames NMD1 do NSGDB (política única).
        // O blob opaco legado (merge LWW do envelope) só é usado se o export
        // vier vazio (NSGDB down) — nunca publicar vazio quebraria o fallback.
        if self.local_blob.is_empty() {
            let exported = super::nsgdb_bridge::export_nmd1_frames_nsgdb(TX_MAX_DOCS);
            if !exported.is_empty() {
                self.local_blob = exported;
            }
        }
        let my_id = mesh::node_id();
        // ADR-0081 follow-up: clock monotônico único por fonte (anti-replay).
        let pkt = AiosTaskPacket::new(mesh::next_data_clock(), my_id, 0xFF, TaskType::Inference, 1, 0, 0, PacketFlags::new());
        let mut buf = udp_broadcast::serialize(&pkt);
        buf.extend_from_slice(b"CRDT\0");
        buf.extend_from_slice(&self.local_version.to_le_bytes());
        buf.extend_from_slice(&self.local_blob);
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
            let Some((v, blob)) = super::crdt_merge::parse_crdt_body(payload) else {
                continue;
            };
            match role {
                NodeRole::Master => {
                    self.upsert_peer_version(pkt.source_id, v);
                    // s410l: Master também APLICA conteúdo (antes só version —
                    // o conteúdo só fluía Master→Worker). Mesma política única.
                    self.apply_blob_to_sgdb(blob, pkt.source_id);
                    k_nano::slog_kai!(
                        "CRDT", "info",
                        "peer node={} v={} peers={}", pkt.source_id, v, self.node_versions.len()
                    );
                }
                NodeRole::Worker => {
                    let m = super::crdt_merge::merge_lww(
                        self.local_version,
                        &self.local_blob,
                        v,
                        blob,
                    );
                    match m.kind {
                        super::crdt_merge::MergeKind::KeepLocal => {}
                        super::crdt_merge::MergeKind::AdoptRemote => {
                            k_nano::slog_kai!(
                                "CRDT", "info",
                                "sync local_v={} -> remote_v={} adopted",
                                self.local_version, v
                            );
                            self.local_version = m.version;
                            self.local_blob = m.payload;
                        }
                        super::crdt_merge::MergeKind::ConflictBoth => {
                            k_nano::slog_kai!(
                                "CRDT", "warn",
                                "CONFLICT v={} both sides kept (no silent overwrite)",
                                m.version
                            );
                            self.local_blob = m.payload;
                        }
                    }
                    // s410l: política CRDT ÚNICA — os docs NMD1 do blob (do
                    // lado remoto, que é o conteúdo novo adotado) passam pelo
                    // merge policy-aware do neural-sgdb, o mesmo do
                    // mesh_knowledge. Idempotente: Stale/Duplicate não regravam.
                    self.apply_blob_to_sgdb(blob, pkt.source_id);
                }
                _ => {}
            }
        }
    }

    /// s410l: aplica frames NMD1 do blob via `merge_remote_nsgdb` — política
    /// CRDT ÚNICA com o mesh_knowledge (MergePolicy::for_layer + happens-before
    /// + conflito preservado). Blob opaco (não-NMD1, wire legado) = no-op aqui:
    /// continua tratado pelo merge LWW do envelope acima (nós velhos convergem
    /// para o novo formato no próximo TX).
    fn apply_blob_to_sgdb(&mut self, blob: &[u8], source: u8) {
        let mut applied = 0u64;
        let mut skipped = 0u64;
        for frame in super::nsgdb_bridge::iterate_nmd1_frames(blob) {
            let Ok(doc) = MemoryDoc::decode(frame) else {
                skipped += 1;
                continue;
            };
            use super::nsgdb_bridge::NsMergeVerdict;
            let verdict = super::nsgdb_bridge::merge_remote_nsgdb(
                doc.layer,
                &doc.key,
                doc.payload.clone(),
                doc.clock.clone(),
            );
            match verdict {
                NsMergeVerdict::Applied => applied += 1,
                NsMergeVerdict::Duplicate | NsMergeVerdict::Stale | NsMergeVerdict::Conflict => {
                    skipped += 1
                }
                NsMergeVerdict::Rejected => {}
            }
        }
        if applied > 0 {
            k_nano::slog_kai!(
                "CRDT", "ok",
                "blob NMD1 aplicado via merge_remote: applied={} skipped={} node={}",
                applied, skipped, source
            );
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

/// Marca mutação local no CRDT global (versão monotônica).
/// Chamado por put_kv/put_doc — sem isto local_version fica 0 e o mesh não propaga.
pub fn crdt_record_change_global() {
    let mut guard = CRDT_GLOBAL.lock();
    if guard.is_none() {
        *guard = Some(CrdtMemorySync::new());
    }
    if let Some(ref mut sync) = *guard {
        sync.record_change();
    }
}

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

/// Lock de teste (statics TICKV/FLASH/NSGDB globais — padrão SESSION_346/368).
#[cfg(test)]
pub(crate) fn crdt_sync_tests_lock() -> spin::MutexGuard<'static, ()> {
    static TEST_LOCK: spin::Mutex<()> = spin::Mutex::new(());
    TEST_LOCK.lock()
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

#[cfg(test)]
mod tests {
    use super::*;
    use super::crdt_sync_tests_lock;
    use crate::sgdb::memory_doc::MemoryLayer::L2EpisodicShort as L2;

    /// Storage limpa (RamFlash) + NSGDB — padrão dos testes de interop.
    fn reset_and_mount() {
        *k_nano::storage::TICKV.lock() = None;
        *k_nano::storage::FLASH.lock() = None;
        k_nano::storage::install_ram_flash(256 * 1024);
        {
            let mut g = k_nano::storage::TICKV.lock();
            g.get_or_insert_with(k_nano::storage::TickvLite::new)
                .mount()
                .expect("mount");
        }
        k_nano::storage::set_gc_suspended(false);
        super::super::nsgdb_bridge::nsgdb_init();
    }

    /// s410l: blob com frames NMD1 é aplicado via merge_remote (política
    /// única) — sem local = Applied; re-aplicação = Duplicate (idempotente);
    /// rejeita frame corrompido sem panic.
    #[test]
    fn crdt_rx_applies_nmd1_frames_via_merge_remote() {
        let _g = crdt_sync_tests_lock();
        reset_and_mount();
        let mut sync = CrdtMemorySync::new();

        // Monta blob: 2 docs L2 válidos (NMD1 com clock do nó remoto 9) +
        // cauda de lixo truncado (simula corte do frame — o iterador é
        // fail-stop: aplica o prefixo válido e para no lixo).
        let mut blob = Vec::new();
        for i in 0..2u32 {
            let mut doc = MemoryDoc::new(
                L2,
                &alloc::format!("crdt/rx/{}", i),
                alloc::format!("payload-{}", i).into_bytes(),
            );
            doc.clock.tick(9);
            let nmd1 = doc.encode();
            blob.extend_from_slice(&(nmd1.len() as u32).to_le_bytes());
            blob.extend_from_slice(&nmd1);
        }
        blob.extend_from_slice(&[0xFF, 0x00, 0x12]); // lixo truncado no fim

        sync.apply_blob_to_sgdb(&blob, 9);

        // Docs aplicados na storage (fonte da verdade) com clock do remoto.
        for i in 0..2u32 {
            let sk = alloc::format!("md/L2/crdt/rx/{}", i);
            let raw = k_nano::storage::get_blob(&sk).expect("doc no tickv");
            let dec = MemoryDoc::decode(&raw).expect("NMD1 válido");
            assert_eq!(dec.payload, alloc::format!("payload-{}", i).into_bytes());
        }

        // Idempotência da política única: re-aplicar o MESMO blob = nada novo
        // (Duplicate/Stale) — a storage não cresce (anti-bloat do RX).
        let len_before = {
            let g = k_nano::storage::TICKV.lock();
            g.as_ref().map(|kv| kv.append_off()).unwrap_or(0)
        };
        sync.apply_blob_to_sgdb(&blob, 9);
        let len_after = {
            let g = k_nano::storage::TICKV.lock();
            g.as_ref().map(|kv| kv.append_off()).unwrap_or(0)
        };
        assert_eq!(len_before, len_after, "re-aplicação não deve crescer o volume");

        *k_nano::storage::TICKV.lock() = None;
        *k_nano::storage::FLASH.lock() = None;
    }

    /// s410l: blob opaco (não-NMD1, wire legado) não passa pelo merge policy —
    /// é tratado só pelo merge LWW do envelope (compat s385).
    #[test]
    fn crdt_rx_opaque_blob_is_lww_envelope_only() {
        let _g = crdt_sync_tests_lock();
        let mut sync = CrdtMemorySync::new();
        // apply_blob_to_sgdb com blob sem frames válidos = no-op silencioso.
        sync.apply_blob_to_sgdb(b"blob-opaco-legado", 5);
        sync.apply_blob_to_sgdb(&[], 5);
        assert_eq!(sync.local_version, 0);
    }
}
