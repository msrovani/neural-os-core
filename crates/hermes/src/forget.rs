//! s410m — Forget cognitivo HITL: registro de alvos pendentes de aprovação.
//!
//! O `ApprovalGate` é genérico (id → skill/agent/reason); a resolução de um
//! `/forget approve <id>` precisa saber QUAL memória apagar. Este módulo é o
//! registry mínimo (cap + evicção, runtime-hygiene s410d) que amarra o id do
//! gate ao alvo `(layer, key)` — e também aos conflitos pendentes de
//! resolução (`/conflicts resolve`).
//!
//! HITL: o delete só acontece com gate aprovado; o registry é estado efêmero
//! de coordenação — se o node reiniciar antes do approve, o pedido morre com
//! ele (fail-closed: nada é apagado sem decisão humana explícita).

use alloc::string::String;
use alloc::vec::Vec;
use k_ai::sgdb::MemoryLayer;
use spin::Mutex;

/// Cap anti-bloat (runtime-hygiene s410d): pedidos de forget não resolvidos
/// não acumulam sem teto; evicção FIFO (o mais velho é o mais provável de
/// ter sido abandonado).
const PENDING_CAP: usize = 32;

struct PendingForget {
    request_id: String,
    layer: MemoryLayer,
    key: String,
}

struct PendingConflict {
    request_id: String,
    conflict_id: String,
    winner_vid: String,
}

static PENDING_FORGETS: Mutex<Vec<PendingForget>> = Mutex::new(Vec::new());
static PENDING_CONFLICTS: Mutex<Vec<PendingConflict>> = Mutex::new(Vec::new());

fn push_capped<T>(v: &mut Vec<T>, item: T) {
    if v.len() >= PENDING_CAP {
        v.remove(0); // evicção FIFO
    }
    v.push(item);
}

/// Registra alvo de forget pendente para um request-id do gate.
pub fn register_pending(request_id: &str, layer: MemoryLayer, key: &str) {
    let mut g = PENDING_FORGETS.lock();
    push_capped(
        &mut g,
        PendingForget {
            request_id: String::from(request_id),
            layer,
            key: String::from(key),
        },
    );
}

/// Consulta (sem remover) o alvo pendente de um request-id.
pub fn pending_target(request_id: &str) -> Option<(MemoryLayer, String)> {
    let g = PENDING_FORGETS.lock();
    g.iter()
        .find(|p| p.request_id == request_id)
        .map(|p| (p.layer, p.key.clone()))
}

/// Limpa o registro pós-resolução (approve ou deny — o dado não é mais necessário).
pub fn clear_pending(request_id: &str) {
    PENDING_FORGETS.lock().retain(|p| p.request_id != request_id);
}

/// Registra resolução de conflito pendente (request-id do gate → decisão).
pub fn register_conflict_pending(request_id: &str, conflict_id: &str, winner_vid: &str) {
    let mut g = PENDING_CONFLICTS.lock();
    push_capped(
        &mut g,
        PendingConflict {
            request_id: String::from(request_id),
            conflict_id: String::from(conflict_id),
            winner_vid: String::from(winner_vid),
        },
    );
}

/// Consulta a decisão pendente de um request-id.
pub fn pending_conflict(request_id: &str) -> Option<(String, String)> {
    let g = PENDING_CONFLICTS.lock();
    g.iter()
        .find(|p| p.request_id == request_id)
        .map(|p| (p.conflict_id.clone(), p.winner_vid.clone()))
}

/// Limpa o registro pós-resolução.
pub fn clear_conflict_pending(request_id: &str) {
    PENDING_CONFLICTS.lock().retain(|p| p.request_id != request_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forget_pending_roundtrip_and_clear() {
        register_pending("42", MemoryLayer::L3EpisodicLong, "md-key");
        assert_eq!(
            pending_target("42"),
            Some((MemoryLayer::L3EpisodicLong, String::from("md-key")))
        );
        clear_pending("42");
        assert_eq!(pending_target("42"), None);
    }

    #[test]
    fn forget_pending_cap_fifo() {
        for i in 0..(PENDING_CAP + 8) {
            register_pending(&alloc::format!("r{}", i), MemoryLayer::L4Semantic, "k");
        }
        // O mais velho foi evictado (FIFO).
        assert_eq!(pending_target("r0"), None);
        assert!(pending_target(&alloc::format!("r{}", PENDING_CAP + 7)).is_some());
        for i in 0..(PENDING_CAP + 8) {
            clear_pending(&alloc::format!("r{}", i));
        }
        assert!(PENDING_FORGETS.lock().is_empty());
    }

    #[test]
    fn conflict_pending_roundtrip() {
        register_conflict_pending("7", "cfl-abc", "vid-2");
        assert_eq!(
            pending_conflict("7"),
            Some((String::from("cfl-abc"), String::from("vid-2")))
        );
        clear_conflict_pending("7");
        assert_eq!(pending_conflict("7"), None);
    }
}
