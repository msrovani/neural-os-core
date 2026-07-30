//! CFS Scheduler — Completely Fair Scheduler for agents (#335).
//! Substitui round-robin do AgentScheduler por vruntime-based fairness.

#![allow(dead_code)]

use core::sync::atomic::{AtomicU64, Ordering};

/// CFS Scheduler state for vruntime-based agent fairness.
/// Tracks total weight and minimum vruntime across all scheduled entities.
pub struct CfsScheduler {
    pub total_weight: u64,
    pub min_vruntime: u64,
}

impl CfsScheduler {
    pub const fn new() -> Self {
        CfsScheduler { total_weight: 0, min_vruntime: 0 }
    }

    /// Place a new entity (agent) with the given weight into the tree.
    /// Returns the initial vruntime for this entity.
    pub fn place_entity(&mut self, weight: u64) -> u64 {
        self.total_weight = self.total_weight.saturating_add(weight);
        // Base vruntime = current min + 1000/weight (ensures fairness for new entities)
        let base = self.min_vruntime.saturating_add(1000u64.saturating_div(weight.max(1)));
        base
    }

    /// Update scheduling state after an entity has run.
    /// Tracks the minimum vruntime seen.
    pub fn update(&mut self, vruntime: u64, _weight: u64) {
        if vruntime < self.min_vruntime {
            self.min_vruntime = vruntime;
        }
    }

    pub fn status(&self) -> alloc::string::String {
        alloc::format!("[CFS] {} weight, min_v={}", self.total_weight, self.min_vruntime)
    }
}

// ─── Global CFS instance for scheduler integration ───

/// Global atomic instance pointer for the CFS scheduler.
/// Set once during platform init; read by agent-core scheduler loop.
static CFS_PTR: AtomicU64 = AtomicU64::new(0);

/// Set the global CFS scheduler reference.
/// Called once during boot from the scheduler init.
pub fn set_global_cfs(cfs: &'static mut CfsScheduler) {
    CFS_PTR.store(cfs as *mut CfsScheduler as u64, Ordering::Release);
}

/// Access the global CFS scheduler, if initialized.
/// Returns `None` if not yet set.
pub fn with_global_cfs<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut CfsScheduler) -> R,
{
    let ptr = CFS_PTR.load(Ordering::Acquire);
    if ptr == 0 {
        return None;
    }
    let cfs = unsafe { &mut *(ptr as *mut CfsScheduler) };
    Some(f(cfs))
}
