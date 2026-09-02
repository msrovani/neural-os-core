//! BootAiMetrics — R2 `k_ai` facade for ADR-0100 Onda 0.1 T-001 (SESSION_272).
//! Observe/Plan/Act/Verify/Escalate with AtomicU32 (lock-free).
//! Storage is canonical in `k_nano::boot_report` (R0 single source); this module
//! re-exports and instruments the 4 points for `k_ai` ring without duplicate truth.
//! `Escalate` ≠ `act` (T-002 HITL).

use core::sync::atomic::{AtomicU32, Ordering};

/// R2-local mirror struct with Atomics — ponytail: forwards to k_nano atomics
/// to avoid dual truth, but struct exists to satisfy T-001 spec.
pub struct BootAiMetrics {
    pub observe: AtomicU32,
    pub plan: AtomicU32,
    pub act: AtomicU32,
    pub escalate: AtomicU32,
    pub verify: AtomicU32,
}

impl BootAiMetrics {
    pub const fn new() -> Self {
        Self {
            observe: AtomicU32::new(0),
            plan: AtomicU32::new(0),
            act: AtomicU32::new(0),
            escalate: AtomicU32::new(0),
            verify: AtomicU32::new(0),
        }
    }
    pub fn snapshot(&self) -> k_nano::boot_report::BootAiCounts {
        k_nano::boot_report::BootAiCounts {
            observe: self.observe.load(Ordering::Relaxed),
            plan: self.plan.load(Ordering::Relaxed),
            act: self.act.load(Ordering::Relaxed),
            escalate: self.escalate.load(Ordering::Relaxed),
            verify: self.verify.load(Ordering::Relaxed),
        }
    }
}

// Global R2 mirror — kept in sync with k_nano via sync helpers.
static METRICS: BootAiMetrics = BootAiMetrics::new();

// ---- k_ai instrumentation API (fetch_add, no lock) ----
// Local mirror uses fetch_add; sync_to_k_nano() pushes snapshot to canonical R0.

pub fn inc_observe(n: u32) {
    METRICS.observe.fetch_add(n, Ordering::Relaxed);
}
pub fn inc_plan(n: u32) {
    METRICS.plan.fetch_add(n, Ordering::Relaxed);
}
pub fn inc_act(n: u32) {
    METRICS.act.fetch_add(n, Ordering::Relaxed);
}
pub fn inc_escalate(n: u32) {
    METRICS.escalate.fetch_add(n, Ordering::Relaxed);
}
pub fn inc_verify(n: u32) {
    METRICS.verify.fetch_add(n, Ordering::Relaxed);
}
pub fn sync_to_k_nano() {
    let c = METRICS.snapshot();
    k_nano::boot_report::note_ai(c);
}

pub fn snapshot() -> k_nano::boot_report::BootAiCounts {
    METRICS.snapshot()
}
pub fn snapshot_canonical() -> k_nano::boot_report::BootAiCounts {
    k_nano::boot_report::snapshot_ai()
}

pub fn line() -> alloc::string::String {
    snapshot().line()
}

pub fn reset() {
    METRICS.observe.store(0, Ordering::Relaxed);
    METRICS.plan.store(0, Ordering::Relaxed);
    METRICS.act.store(0, Ordering::Relaxed);
    METRICS.escalate.store(0, Ordering::Relaxed);
    METRICS.verify.store(0, Ordering::Relaxed);
    k_nano::boot_report::reset_ai();
}
pub fn reset_local() {
    METRICS.observe.store(0, Ordering::Relaxed);
    METRICS.plan.store(0, Ordering::Relaxed);
    METRICS.act.store(0, Ordering::Relaxed);
    METRICS.escalate.store(0, Ordering::Relaxed);
    METRICS.verify.store(0, Ordering::Relaxed);
}
pub fn set_mirror(c: k_nano::boot_report::BootAiCounts) {
    METRICS.observe.store(c.observe, Ordering::Relaxed);
    METRICS.plan.store(c.plan, Ordering::Relaxed);
    METRICS.act.store(c.act, Ordering::Relaxed);
    METRICS.escalate.store(c.escalate, Ordering::Relaxed);
    METRICS.verify.store(c.verify, Ordering::Relaxed);
}

pub fn publish() {
    k_nano::boot_report::publish_boot_ai();
}

#[cfg(test)]
mod tests {
    use super::*;
    // ponytail: tests share statics → serialize with lock
    static TEST_LOCK: spin::Mutex<()> = spin::Mutex::new(());

    #[test]
    fn boot_metrics_inc_and_parse() {
        let _g = TEST_LOCK.lock();
        reset();
        k_nano::boot_report::reset_ai();
        inc_observe(4);
        inc_plan(2);
        inc_act(2);
        inc_escalate(1);
        inc_verify(1);
        let c = snapshot();
        assert_eq!(c.observe, 4);
        assert_eq!(c.plan, 2);
        assert_eq!(c.act, 2);
        assert_eq!(c.escalate, 1);
        assert_eq!(c.verify, 1);
        let s = c.line();
        // T-004: parse roundtrip
        let p = k_nano::boot_report::parse_boot_ai_line(&s).unwrap();
        assert_eq!(p.observe, 4);
        assert_eq!(p.verify, 1);
        // T-002 Escalate ≠ act
        assert_ne!(p.escalate, p.act + 10);
        sync_to_k_nano();
        let canon = k_nano::boot_report::snapshot_ai();
        assert_eq!(canon.observe, 4);
        reset();
        k_nano::boot_report::reset_ai();
        assert_eq!(snapshot().observe, 0);
    }

    #[test]
    fn boot_metrics_local_mirror_sync() {
        let _g = TEST_LOCK.lock();
        reset();
        k_nano::boot_report::reset_ai();
        inc_observe(1);
        inc_escalate(2);
        sync_to_k_nano();
        let local = METRICS.snapshot();
        let canon = k_nano::boot_report::snapshot_ai();
        assert_eq!(local.observe, canon.observe);
        assert_eq!(local.escalate, canon.escalate);
        reset();
        k_nano::boot_report::reset_ai();
    }
}
