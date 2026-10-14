//! Traits for k_ai components — enables mocking/testing (Item 12).
//!
//! These traits capture the public API of ExpertLifecycleManager and BudgetManager
//! so they can be mocked in tests or swapped for alternative implementations.

use alloc::vec::Vec;
use alloc::string::String;
use core::sync::atomic::Ordering;

// Re-export concrete types from their original modules
pub use crate::expert_lifecycle::ExpertMetadata;
pub use crate::economy::CompressionTier;

/// Trait for expert lifecycle management.
/// Allows mocking in tests and alternative implementations.
pub trait ExpertLifecycle: Send + Sync {
    /// Register a new expert. Returns the assigned expert ID.
    fn register(&mut self, name: String, bits_per_weight: u8, current_tick: u64) -> u64;

    /// Record a hit (usage) for an expert with confidence feedback.
    fn record_hit(&mut self, id: u64, confidence: f32, current_tick: u64);

    /// Update the entropy metric for an expert.
    fn update_entropy(&mut self, id: u64, entropy: f32);

    /// Get candidate pairs for merging (low entropy, same quantization).
    fn candidates_for_merge(&self, current_tick: u64) -> Vec<(usize, usize)>;

    /// Get candidate indices for splitting (high entropy).
    fn candidates_for_split(&self, current_tick: u64) -> Vec<usize>;

    /// Get indices of stale experts (inactive + low hits).
    fn stale_experts(&self, current_tick: u64) -> Vec<usize>;

    /// Remove an expert by index.
    fn remove_expert(&mut self, idx: usize);

    /// Get expert metadata by ID.
    fn get(&self, id: u64) -> Option<&ExpertMetadata>;

    /// Total number of registered experts.
    fn count(&self) -> usize;

    /// Get all experts as a slice.
    fn all(&self) -> &[ExpertMetadata];
}

/// Trait for budget management (tokens, memory, compression).
/// Allows mocking in tests and alternative implementations.
pub trait BudgetManager: Send + Sync {
    /// Record N tokens consumed. Returns true if still within budget.
    fn record_tokens(&self, n: u64) -> bool;

    /// Check if budget exists for N tokens.
    fn has_budget_for(&self, n: u64) -> bool;

    /// Get current compression tier.
    fn compression_tier(&self) -> CompressionTier;

    /// Adapt compression tier based on inference load.
    fn adapt_compression(&self);

    /// Record an inference cycle.
    fn record_inference_cycle(&self);

    /// Reset token budget for new session.
    fn reset_session(&self);

    /// Current memory pressure (0.0 - 1.0).
    fn pressure(&self) -> f32;

    /// Try to allocate bytes in budget. Returns false if would exceed.
    fn allocate(&self, bytes: usize) -> bool;

    /// Deallocate bytes from budget.
    fn deallocate(&self, bytes: usize);

    /// Currently used bytes.
    fn used_bytes(&self) -> usize;

    /// Maximum budgeted bytes.
    fn max_bytes(&self) -> usize;

    /// Set system temperature (0.0 - 1.0).
    fn set_temperature(&mut self, t: f32);
}

impl ExpertLifecycle for crate::expert_lifecycle::ExpertLifecycleManager {
    fn register(&mut self, name: String, bits_per_weight: u8, current_tick: u64) -> u64 {
        self.register(name, bits_per_weight, current_tick)
    }

    fn record_hit(&mut self, id: u64, confidence: f32, current_tick: u64) {
        self.record_hit(id, confidence, current_tick)
    }

    fn update_entropy(&mut self, id: u64, entropy: f32) {
        self.update_entropy(id, entropy)
    }

    fn candidates_for_merge(&self, current_tick: u64) -> Vec<(usize, usize)> {
        self.candidates_for_merge(current_tick)
    }

    fn candidates_for_split(&self, current_tick: u64) -> Vec<usize> {
        self.candidates_for_split(current_tick)
    }

    fn stale_experts(&self, current_tick: u64) -> Vec<usize> {
        self.stale_experts(current_tick)
    }

    fn remove_expert(&mut self, idx: usize) {
        self.remove_expert(idx)
    }

    fn get(&self, id: u64) -> Option<&ExpertMetadata> {
        self.get(id)
    }

    fn count(&self) -> usize {
        self.count()
    }

    fn all(&self) -> &[ExpertMetadata] {
        self.all()
    }
}

impl BudgetManager for crate::economy::BudgetManager {
    fn record_tokens(&self, n: u64) -> bool {
        self.record_tokens(n)
    }

    fn has_budget_for(&self, n: u64) -> bool {
        self.has_budget_for(n)
    }

    fn compression_tier(&self) -> CompressionTier {
        self.compression_tier()
    }

    fn adapt_compression(&self) {
        self.adapt_compression()
    }

    fn record_inference_cycle(&self) {
        self.record_inference_cycle()
    }

    fn reset_session(&self) {
        self.reset_session()
    }

    fn pressure(&self) -> f32 {
        self.pressure()
    }

    fn allocate(&self, bytes: usize) -> bool {
        self.allocate(bytes)
    }

    fn deallocate(&self, bytes: usize) {
        self.deallocate(bytes)
    }

    fn used_bytes(&self) -> usize {
        self.used_bytes()
    }

    fn max_bytes(&self) -> usize {
        self.max_bytes()
    }

    fn set_temperature(&mut self, t: f32) {
        self.set_temperature(t)
    }
}