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
//! - `k_nano::p2p::noproto` — serialização NoProto dos diffs (quando disponível)
//!
//! ## Fallback local (ativo enquanto P2P não estiver vivo)
//! Sem P2P, o SGDB opera localmente — comportamento atual.
//! `crdt_sync()` retorna imediatamente se P2P não estiver ativo.

use alloc::vec::Vec;
use k_nano::net::mesh::NodeRole;

/// Intervalo mínimo entre syncs (em ticks do scheduler) — ~2s a 100Hz.
const SYNC_INTERVAL_TICKS: u64 = 200;

/// Agente CRDT de sincronização de memória entre nós do mesh.
///
/// Mantém versão local e versões conhecidas de outros nós.
/// Quando o mesh P2P está ativo (role != Undecided), tenta trocar
/// diffs periodicamente. Caso contrário, opera apenas localmente.
pub struct CrdtMemorySync {
    /// Se true, P2P está ativo e sync será tentado.
    active: bool,
    /// Último tick em que sync foi tentado.
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
    /// - Se P2P está ativo mas `SYNC_INTERVAL_TICKS` não passou: retorna sem ação.
    /// - Se P2P ativo e intervalo venceu: executa sync (atualmente stub de log).
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
            self.last_sync_tick = tick;
            k_nano::slog_kai!(
                "CRDT", "sync",
                "P2P mesh ativo (role={:?}) → sync iniciado (v={})",
                role, self.local_version,
            );
            // Primeiro sync imediato
            self.sync_exchange();
            return;
        }

        // Rate-limit: só sync se intervalo mínimo passou
        if tick < self.last_sync_tick.saturating_add(SYNC_INTERVAL_TICKS) {
            return;
        }
        self.last_sync_tick = tick;

        self.sync_exchange();
    }

    /// Troca de diffs com os peers — implementação real requer udp_broadcast.
    ///
    /// ## Stub atual
    /// Apenas loga o estado local e o número de nós conhecidos. A troca real
    /// de diffs será implementada quando o transporte P2P (udp_broadcast)
    /// estiver disponível (ADR-0081 Fase A).
    ///
    /// ## Plano
    /// 1. Serializar diffs locais via NoProto (k_nano::p2p::noproto)
    /// 2. Enviar via udp_broadcast::send()
    /// 3. Receber diffs via udp_broadcast::recv()
    /// 4. Aplicar merge LWW nos MemoryDocs do SGDB
    /// 5. Atualizar node_versions com os pares recebidos
    fn sync_exchange(&mut self) {
        let peer_count = self.node_versions.len();

        // ponytail: sync real quando udp_broadcast estiver pronto
        k_nano::slog_kai!(
            "CRDT", "sync",
            "sync_cycle: local_v={} peers={} — NOP (transport stub, ADR-0081 Fase A pendente)",
            self.local_version, peer_count,
        );
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
