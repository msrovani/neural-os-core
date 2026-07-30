//! Lamport Logical Clock for P2P Orchestration
//!
//! Implements atomic logical clocks for ordering inference tasks across
//! distributed AIOS nodes without NTP/RTC dependencies.
//!
//! NoProto packets carry LogicalClock timestamps for ordering.
//! Brain Mesh uses NoProto for node discovery broadcasts.
//! VectorClock enables causal consistency in distributed inference.

use core::sync::atomic::{AtomicU64, Ordering};

/// Lamport Logical Clock
///
/// Provides a monotonically increasing counter for ordering events
/// in distributed systems. Each tick increments the counter on send,
/// and updates to max(local, received) + 1 on receive.
#[derive(Debug)]
pub struct LogicalClock {
    counter: AtomicU64,
}

impl LogicalClock {
    /// Create a new logical clock initialized to 0
    #[must_use]
    pub const fn new() -> Self {
        Self {
            counter: AtomicU64::new(0),
        }
    }

    /// Get the current clock value
    #[must_use]
    pub fn get(&self) -> u64 {
        self.counter.load(Ordering::Acquire)
    }

    /// Increment the clock (called before sending a message)
    /// Returns the new clock value to include in the message
    pub fn tick(&self) -> u64 {
        self.counter.fetch_add(1, Ordering::AcqRel) + 1
    }

    /// Update the clock based on a received clock value
    /// Sets clock to max(local, received) + 1
    pub fn update(&self, received_clock: u64) -> u64 {
        let current = self.counter.load(Ordering::Acquire);
        let new_clock = current.max(received_clock) + 1;
        self.counter.store(new_clock, Ordering::Release);
        new_clock
    }

    /// Reset the clock to 0 (useful for testing)
    pub fn reset(&self) {
        self.counter.store(0, Ordering::Release);
    }
}

impl Default for LogicalClock {
    fn default() -> Self {
        Self::new()
    }
}

/// Static global logical clock for P2P mesh ordering.
pub static GLOBAL_LOGICAL_CLOCK: LogicalClock = LogicalClock::new();

/// Convenience: tick do logical clock global.
pub fn logical_tick() -> u64 {
    GLOBAL_LOGICAL_CLOCK.tick()
}

/// Vector Clock for tracking causality across multiple nodes
///
/// Extends Lamport clocks to track per-node counters, enabling
/// detection of concurrent events and causal relationships.
///
/// Supports up to 16 nodes for distributed inference mesh scenarios.
#[derive(Debug, Clone)]
pub struct VectorClock {
    /// Array of counters, indexed by node ID
    counters: [u64; 16],
    /// Local node ID
    local_id: u8,
}

impl VectorClock {
    /// Create a new vector clock for the given local node ID
    #[must_use]
    pub const fn new(local_id: u8) -> Self {
        Self {
            counters: [0; 16],
            local_id,
        }
    }

    /// Get the local node's clock value
    #[must_use]
    pub fn get_local(&self) -> u64 {
        self.counters[self.local_id as usize]
    }

    /// Increment the local clock (called before sending)
    pub fn tick(&mut self) -> u64 {
        self.counters[self.local_id as usize] += 1;
        self.counters[self.local_id as usize]
    }

    /// Update the vector clock based on a received vector clock
    /// For each node: new[i] = max(local[i], received[i])
    /// Then increment local clock
    pub fn update(&mut self, received: &VectorClock) {
        for i in 0..16 {
            self.counters[i] = self.counters[i].max(received.counters[i]);
        }
        self.counters[self.local_id as usize] += 1;
    }

    /// Check if this clock happened before another clock (causality)
    /// Returns true if for all i: self[i] <= other[i] and exists j: self[j] < other[j]
    #[must_use]
    pub fn happens_before(&self, other: &VectorClock) -> bool {
        let mut leq_all = true;
        let mut lt_some = false;

        for i in 0..16 {
            if self.counters[i] > other.counters[i] {
                leq_all = false;
            }
            if self.counters[i] < other.counters[i] {
                lt_some = true;
            }
        }

        leq_all && lt_some
    }

    /// Check if two clocks are concurrent (neither happens before the other)
    #[must_use]
    pub fn is_concurrent(&self, other: &VectorClock) -> bool {
        !self.happens_before(other) && !other.happens_before(self)
    }

    /// Get the raw counter array (for serialization)
    #[must_use]
    pub const fn as_slice(&self) -> &[u64; 16] {
        &self.counters
    }

    /// Create from raw counter array
    #[must_use]
    pub const fn from_slice(counters: [u64; 16], local_id: u8) -> Self {
        Self {
            counters,
            local_id,
        }
    }

    /// Reset all counters to 0
    pub fn reset(&mut self) {
        self.counters = [0; 16];
    }
}
