//! Traits for cortex components — enables mocking/testing (Item 12).
//!
//! These traits capture the public API of PlasticityController
//! so it can be mocked in tests or swapped for alternative implementations.

use alloc::vec::Vec;

/// Trait for plasticity control (growth/pruning of cellular regions).
/// Allows mocking in tests and alternative implementations.
pub trait PlasticityController: Send + Sync {
    /// Feed plasticity data for a region (entropy, error rate, activation).
    fn observe(&mut self, region: usize, entropy: f32, error: f32, activated: f64);

    /// Advance internal tick counter.
    fn tick_advance(&mut self);

    /// Check if a region should grow (high entropy + sufficient hits).
    fn should_grow(&self, region: usize) -> bool;

    /// Check if a region should be pruned (low activation + sufficient hits).
    fn should_prune(&self, region: usize) -> bool;

    /// Get entropy for a region.
    fn region_entropy(&self, region: usize) -> f32;

    /// Number of regions managed.
    fn num_regions(&self) -> usize;
}

impl PlasticityController for crate::evolution::PlasticityController {
    fn observe(&mut self, region: usize, entropy: f32, error: f32, activated: f64) {
        self.observe(region, entropy, error, activated)
    }

    fn tick_advance(&mut self) {
        self.tick_advance()
    }

    fn should_grow(&self, region: usize) -> bool {
        self.should_grow(region)
    }

    fn should_prune(&self, region: usize) -> bool {
        self.should_prune(region)
    }

    fn region_entropy(&self, region: usize) -> f32 {
        self.region_entropy(region)
    }

    fn num_regions(&self) -> usize {
        self.num_regions()
    }
}