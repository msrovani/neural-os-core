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
use k_nano::slog_hermes;
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
}

impl SkillSync {
    /// Cria um novo `SkillSync` (inativo por padrão).
    pub const fn new() -> Self {
        Self {
            active: false,
            last_sync_tick: 0,
            pending_skills: Vec::new(),
        }
    }

    /// Marca P2P como ativo (chamado pela camada de transporte ao estabelecer link mesh).
    pub fn activate(&mut self) {
        self.active = true;
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

        // Drena uma skill pendente por ciclo
        if self.pending_skills.is_empty() {
            return;
        }

        let name = self.pending_skills.remove(0);

        match role {
            NodeRole::Master => {
                // Master push: serializa skill e broadcast para Workers
                // ponytail: udp_broadcast::send(manifest) não implementado — log apenas
                slog_hermes!(
                    "SkillSync", "info",
                    "Master: sync skill='{}' para Workers (broadcast pendente)",
                    name
                );
            }
            NodeRole::Worker | NodeRole::Compute | NodeRole::Memory => {
                // Worker push: promove skill para o Master
                // ponytail: udp_broadcast::send(PROMOTE_SKILL) não implementado — log apenas
                slog_hermes!(
                    "SkillSync", "info",
                    "Worker: promovendo skill='{}' para Master (PROMOTE pendente)",
                    name
                );
            }
            NodeRole::Undecided => {
                // Nó ainda não faz parte do mesh — requeue para próxima sync
                self.pending_skills.push(name);
            }
        }
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
