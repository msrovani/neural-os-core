//! BEI tick fino — budget + affect (PORT ADR-0060; não substitui LLM).

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::debug;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffectSnapshot {
    pub valence: i8,
    pub arousal: i8,
    pub budget_remaining: u32,
}

#[derive(Clone)]
pub struct BeiRuntime {
    inner: Arc<Mutex<BeiState>>,
}

struct BeiState {
    budget: u32,
    valence: i8,
    arousal: i8,
    ticks: u64,
}

impl Default for BeiRuntime {
    fn default() -> Self {
        Self::new(1000)
    }
}

impl BeiRuntime {
    pub fn new(budget: u32) -> Self {
        Self {
            inner: Arc::new(Mutex::new(BeiState {
                budget,
                valence: 0,
                arousal: 0,
                ticks: 0,
            })),
        }
    }

    /// Tick contínuo com gate de budget (fail-closed quando 0).
    pub fn tick(&self) -> Option<AffectSnapshot> {
        let mut g = self.inner.lock();
        if g.budget == 0 {
            debug!(target: "nosd", "bei tick skip budget=0");
            return None;
        }
        g.budget -= 1;
        g.ticks += 1;
        if g.ticks % 8 == 0 {
            g.arousal = g.arousal.saturating_sub(1);
        }
        Some(AffectSnapshot {
            valence: g.valence,
            arousal: g.arousal,
            budget_remaining: g.budget,
        })
    }

    pub fn note_event(&self, delta_valence: i8, cost: u32) {
        let mut g = self.inner.lock();
        g.valence = g.valence.saturating_add(delta_valence);
        g.arousal = g.arousal.saturating_add(1);
        g.budget = g.budget.saturating_sub(cost);
    }

    pub fn snapshot(&self) -> AffectSnapshot {
        let g = self.inner.lock();
        AffectSnapshot {
            valence: g.valence,
            arousal: g.arousal,
            budget_remaining: g.budget,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_exhausts() {
        let b = BeiRuntime::new(2);
        assert!(b.tick().is_some());
        assert!(b.tick().is_some());
        assert!(b.tick().is_none());
    }
}
