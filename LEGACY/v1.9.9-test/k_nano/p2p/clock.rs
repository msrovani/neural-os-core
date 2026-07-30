//! Lamport Logical Clock for P2P Orchestration
//! 
//! Implements atomic logical clocks for ordering inference tasks across
//! distributed AIOS nodes without NTP/RTC dependencies.

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

/// Vector Clock for tracking causality across multiple nodes
/// 
/// Extends Lamport clocks to track per-node counters, enabling
/// detection of concurrent events and causal relationships.
#[derive(Debug, Clone)]
pub struct VectorClock {
    /// Array of counters, indexed by node ID
    counters: [u64; 16], // Support up to 16 nodes
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logical_clock_tick() {
        let clock = LogicalClock::new();
        assert_eq!(clock.get(), 0);

        let v1 = clock.tick();
        assert_eq!(v1, 1);
        assert_eq!(clock.get(), 1);

        let v2 = clock.tick();
        assert_eq!(v2, 2);
        assert_eq!(clock.get(), 2);
    }

    #[test]
    fn test_logical_clock_update() {
        let clock1 = LogicalClock::new();
        let clock2 = LogicalClock::new();

        clock1.tick();
        clock1.tick();
        assert_eq!(clock1.get(), 2);

        // Clock2 receives message with clock=2
        let new_clock = clock2.update(2);
        assert_eq!(new_clock, 3); // max(0, 2) + 1
        assert_eq!(clock2.get(), 3);
    }

    #[test]
    fn test_logical_clock_update_lower() {
        let clock1 = LogicalClock::new();
        let clock2 = LogicalClock::new();

        clock1.tick();
        clock1.tick();
        clock1.tick();
        assert_eq!(clock1.get(), 3);

        // Clock2 already ahead
        clock2.tick();
        clock2.tick();
        clock2.tick();
        clock2.tick();
        assert_eq!(clock2.get(), 4);

        // Clock2 receives message with clock=3 (lower than local)
        let new_clock = clock2.update(3);
        assert_eq!(new_clock, 5); // max(4, 3) + 1
    }

    #[test]
    fn test_vector_clock_tick() {
        let mut vc = VectorClock::new(0);
        assert_eq!(vc.get_local(), 0);

        let v1 = vc.tick();
        assert_eq!(v1, 1);
        assert_eq!(vc.get_local(), 1);

        let v2 = vc.tick();
        assert_eq!(v2, 2);
    }

    #[test]
    fn test_vector_clock_update() {
        let mut vc1 = VectorClock::new(0);
        let mut vc2 = VectorClock::new(1);

        vc1.tick();
        vc1.tick();
        assert_eq!(vc1.get_local(), 2);

        vc2.tick();
        assert_eq!(vc2.get_local(), 1);

        // VC2 receives VC1's clock
        vc2.update(&vc1);
        assert_eq!(vc2.get_local(), 2); // incremented after update
        assert_eq!(vc2.counters[0], 2); // received from node 0
    }

    #[test]
    fn test_vector_clock_happens_before() {
        let mut vc1 = VectorClock::new(0);
        let mut vc2 = VectorClock::new(1);

        vc1.tick();
        vc1.tick();

        // VC2 receives VC1
        vc2.update(&vc1);

        assert!(vc1.happens_before(&vc2));
        assert!(!vc2.happens_before(&vc1));
    }

    #[test]
    fn test_vector_clock_concurrent() {
        let mut vc1 = VectorClock::new(0);
        let mut vc2 = VectorClock::new(1);

        vc1.tick(); // Node 0: [1, 0, ...]
        vc2.tick(); // Node 1: [0, 1, ...]

        // Concurrent events
        assert!(vc1.is_concurrent(&vc2));
        assert!(!vc1.happens_before(&vc2));
        assert!(!vc2.happens_before(&vc1));
    }

    #[test]
    fn test_vector_clock_concurrent_after_update() {
        let mut vc1 = VectorClock::new(0);
        let mut vc2 = VectorClock::new(1);
        let mut vc3 = VectorClock::new(2);

        vc1.tick(); // [1, 0, 0, ...]
        vc2.tick(); // [0, 1, 0, ...]
        
        // VC3 receives from both
        vc3.update(&vc1);
        vc3.update(&vc2);

        // VC1 and VC2 are still concurrent
        assert!(vc1.is_concurrent(&vc2));
        // VC3 happened after both
        assert!(vc1.happens_before(&vc3));
        assert!(vc2.happens_before(&vc3));
    }
}
