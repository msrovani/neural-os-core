//! MhiScheduler — stub. Unique brain is `k_nano::mhi::mhi_tick`.
//! Logging-only scanner deleted to avoid dual policy.

/// Kept so other crates compiling against this symbol do not break.
pub fn mhi_scheduler_tick(_tick: u64) {
    // unique brain is mhi_tick; logging-only scanner deleted to avoid dual policy
}