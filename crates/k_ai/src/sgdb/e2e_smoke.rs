//! ADR-0063 — smoke e2e memória: L1 put → checkpoint → prune → remount → get.

use super::engine::{init_global, with_engine};
use super::layers::ensure_ready;
use super::memory_doc::{MemoryDoc, MemoryLayer};
use super::store::{checkpoint_working, get_doc, prune_working_ram};

const E2E_KEY: &str = "e2e_ckpt";
const E2E_MARKER: &[u8] = b"e2e_l1_ckpt_v1";

/// Gate E1 honesty: L1 sobrevive checkpoint + remount Tickv.
pub fn memory_checkpoint_e2e_smoke() -> bool {
    if !k_nano::storage::is_ready() {
        return false;
    }
    ensure_ready();
    let put_ok = with_engine(|e| {
        let d = MemoryDoc::new(MemoryLayer::L1Working, E2E_KEY, E2E_MARKER.to_vec());
        e.put(d).is_ok() && e.ram_l0l1_len() > 0
    })
    .unwrap_or(false);
    if !put_ok {
        return false;
    }
    if checkpoint_working().is_err() {
        return false;
    }
    let _ = prune_working_ram();
    // Remount Tickv (simula reboot parcial)
    {
        *k_nano::storage::TICKV.lock() = None;
        let mut g = k_nano::storage::TICKV.lock();
        let kv = g.get_or_insert_with(k_nano::storage::TickvLite::new);
        if kv.mount().is_err() {
            return false;
        }
    }
    // Engine fresco + rebuild a partir do Tickv
    init_global(1);
    ensure_ready();
    let _ = with_engine(|e| e.rebuild_indices_from_tickv());
    match get_doc(MemoryLayer::L1Working, E2E_KEY) {
        Ok(Some(doc)) => doc.payload.as_slice() == E2E_MARKER,
        _ => false,
    }
}
