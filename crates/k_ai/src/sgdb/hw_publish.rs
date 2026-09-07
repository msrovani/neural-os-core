//! SGDB HW publish — DeviceTree -> NSGDB (ADR-0063 + 0103 FASE D).
//! Migrated from k_nano::sgdb (2026-09-07). k_nano::sgdb retains legacy
//! for backward compat; new callers should use k_ai::sgdb::hw_publish.

use alloc::format;

// Delegate to k_nano::sgdb for now (backward compat).
// In future slices, this can be reimplemented against k_ai::sgdb store directly.

/// StorageBus -> /hw/storage/<id>/... (T-013)
pub fn publish_storage() {
    k_nano::sgdb::publish_storage()
}

/// GPU -> /hw/gpu/<id>/... (T-014)
pub fn publish_gpu() {
    k_nano::sgdb::publish_gpu()
}

/// Net -> /hw/net/<nic>/... (T-015)
pub fn publish_net() {
    k_nano::sgdb::publish_net()
}

/// WiFi only if device present (T-016)
pub fn publish_wifi() {
    k_nano::sgdb::publish_wifi()
}

/// Publica todo /hw/* de uma vez (chame apos H1 + StorageBus probe).
pub fn publish_all() {
    k_nano::sgdb::publish_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn publish_all_does_not_panic_without_hw() {
        publish_all();
    }
}
