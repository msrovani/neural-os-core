//! Proof-gated mutations via ruvix-proof (ADR-0076).
//! 3-tier proof: Reflex <100ns, Standard <100μs, Deep <10ms.
//! Integrates with CapGate for capability-gated verification.

use core::sync::atomic::Ordering;
use ruvix_proof::{ProofEngine, ProofEngineConfig, ProofError, ProofToken, ProofTier, ProofVerifier, VerificationResult};

/// Proof gate — wraps ruvix-proof engine + verifier with kernel integration.
pub struct ProofGate {
    engine: ProofEngine,
    verifier: ProofVerifier,
}

impl ProofGate {
    pub fn new() -> Self {
        let config = ProofEngineConfig::default();
        Self {
            engine: ProofEngine::new(config),
            verifier: ProofVerifier::new(),
        }
    }

    /// Coarse monotonic time in nanoseconds (TIMER_TICKS × 1_000_000).
    // ponytail: not wall-clock; do not use for precise timing.
    fn now_ns() -> u64 {
        let ticks = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
        ticks.saturating_mul(1_000_000)
    }

    /// Generate a proof token for a mutation at the given tier.
    pub fn create_proof(
        &mut self,
        mutation_hash: &[u8; 32],
        tier: ProofTier,
    ) -> Result<ProofToken, ProofError> {
        let now = Self::now_ns();
        self.engine.generate_for_tier(mutation_hash, tier, now)
    }

    /// Verify a proof token before allowing a mutation.
    /// Returns `Ok` with the verification result on success, `Err` on denial.
    pub fn verify(
        &mut self,
        token: &ProofToken,
        mutation_hash: &[u8; 32],
    ) -> Result<VerificationResult, ProofError> {
        let now = Self::now_ns();
        self.verifier.verify(token, mutation_hash, now)
    }

    /// Access the underlying engine (for advanced configuration).
    pub fn engine_mut(&mut self) -> &mut ProofEngine {
        &mut self.engine
    }

    /// Access the underlying verifier (for advanced configuration).
    pub fn verifier_mut(&mut self) -> &mut ProofVerifier {
        &mut self.verifier
    }
}

impl Default for ProofGate {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_verify_reflex_proof() {
        let mut gate = ProofGate::new();
        let hash = [42u8; 32];
        let token = gate.create_proof(&hash, ProofTier::Reflex).unwrap();
        let result = gate.verify(&token, &hash).unwrap();
        assert_eq!(result.tier, ProofTier::Reflex);
    }

    #[test]
    fn test_verify_hash_mismatch_rejected() {
        let mut gate = ProofGate::new();
        let hash = [42u8; 32];
        let wrong_hash = [0u8; 32];
        let token = gate.create_proof(&hash, ProofTier::Reflex).unwrap();
        let err = gate.verify(&token, &wrong_hash).unwrap_err();
        assert!(matches!(err, ProofError::HashMismatch { .. }));
    }

    #[test]
    fn test_create_standard_proof() {
        let mut gate = ProofGate::new();
        let hash = [0xABu8; 32];
        let token = gate.create_proof(&hash, ProofTier::Standard).unwrap();
        assert_eq!(token.tier, ProofTier::Standard);
        assert_eq!(token.mutation_hash, hash);
    }

    #[test]
    fn test_create_deep_proof() {
        let mut gate = ProofGate::new();
        let hash = [0xCDu8; 32];
        let token = gate.create_proof(&hash, ProofTier::Deep).unwrap();
        assert_eq!(token.tier, ProofTier::Deep);
    }

    #[test]
    fn test_nonce_uniqueness() {
        let mut gate = ProofGate::new();
        let hash = [99u8; 32];
        let t1 = gate.create_proof(&hash, ProofTier::Reflex).unwrap();
        let t2 = gate.create_proof(&hash, ProofTier::Reflex).unwrap();
        assert_ne!(t1.nonce, t2.nonce);
    }

    #[test]
    fn test_verify_rejects_expired_token() {
        // Create a token with a time far in the past using engine directly
        let mut gate = ProofGate::new();
        let hash = [1u8; 32];

        // Use generate_for_tier with a very old timestamp
        let token = gate
            .engine_mut()
            .generate_for_tier(&hash, ProofTier::Reflex, 1)
            .unwrap();

        // Verify at a far-future `now` — explicit, because on host
        // `ProofGate::now_ns()` reads TIMER_TICKS which never advances (0),
        // so the token would look not-yet-expired.
        let err = gate
            .verifier_mut()
            .verify(&token, &hash, 1_000_000_000)
            .unwrap_err();
        assert!(matches!(err, ProofError::Expired { .. }));
    }
}
