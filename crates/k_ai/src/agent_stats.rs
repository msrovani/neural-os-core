//! Agent statistics for safety invariant I2 (agents alive).
//! Provides a snapshot of the current agent count from the global registry.

use core::sync::atomic::{AtomicUsize, Ordering};

/// Last known agent count, updated by the scheduler each tick.
static AGENT_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Update the agent count snapshot (called by scheduler each tick).
pub fn update_agent_count(count: usize) {
    AGENT_COUNT.store(count, Ordering::Relaxed);
}

/// Get the current agent count (last snapshot).
pub fn current_agent_count() -> usize {
    AGENT_COUNT.load(Ordering::Relaxed)
}
