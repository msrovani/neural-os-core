//! ADR-0063 — smoke e2e memória: L1 put → checkpoint → prune → remount → get.
//!
//! **s410h — motor único:** exercita o NSGDB externo (`neural-sgdb` via
//! `nsgdb_bridge`): put tipado → checkpoint (flush RAM L0/L1 → Tickv) →
//! prune → remount Tickv + reset NSGDB (fast-mount/rebuild) → get.
//! O `AiosDatabaseEngine` interno não existe mais.

use super::memory_doc::{MemoryDoc, MemoryLayer};
use super::nsgdb_bridge::{nsgdb_is_ready, nsgdb_reset, put_doc_nsgdb, ram_len_nsgdb};
use super::store::{checkpoint_working, get_doc, prune_working_ram};

const E2E_KEY: &str = "e2e_ckpt";
const E2E_MARKER: &[u8] = b"e2e_l1_ckpt_v1";

/// Gate E1 honesty: L1 sobrevive checkpoint + remount Tickv.
pub fn memory_checkpoint_e2e_smoke() -> bool {
    if !k_nano::storage::is_ready() {
        return false;
    }
    if !nsgdb_is_ready() {
        return false;
    }
    // Put tipado no motor único (L1 → arena RAM do NSGDB)
    let doc = MemoryDoc::new(MemoryLayer::L1Working, E2E_KEY, E2E_MARKER.to_vec());
    if put_doc_nsgdb(doc).is_err() {
        return false;
    }
    if ram_len_nsgdb() == 0 {
        return false;
    }
    if checkpoint_working().is_err() {
        return false;
    }
    if prune_working_ram() == 0 {
        return false;
    }
    // Remount Tickv (simula reboot parcial)
    {
        *k_nano::storage::TICKV.lock() = None;
        let mut g = k_nano::storage::TICKV.lock();
        let kv = g.get_or_insert_with(k_nano::storage::TickvLite::new);
        if kv.mount().is_err() {
            return false;
        }
    }
    // Motor fresco: NSGDB reaberto a partir do Tickv (fast-mount snapshot ou rebuild)
    let _ = nsgdb_reset();
    match get_doc(MemoryLayer::L1Working, E2E_KEY) {
        Ok(Some(doc)) => doc.payload.as_slice() == E2E_MARKER,
        _ => false,
    }
}
