//! SESSION_350 — Heap AIOS: Observe → Plan → Act → Verify → Remember.
//!
//! Piso fail-closed (Tensor checked_mul / max_seq clamp) permanece.
//! Este módulo é a **política dinâmica** sobre orçamento real de bump heap
//! (`window − used`), não o HUD “2168MB RAM ample”.

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::difficulty_gate::{self, ComputeTier};

/// Seam Remember (hermes/k_ai registam no boot) — zero dep cortex→hermes.
pub type RememberFn = fn(host_note: &str);
static REMEMBER_FN: spin::Mutex<Option<RememberFn>> = spin::Mutex::new(None);

static LAST_PLAN_CTX: AtomicUsize = AtomicUsize::new(0);
static LAST_PLAN_TIER: AtomicUsize = AtomicUsize::new(0); // 0 cheap 1 normal 2 full 3 escalate
static LAST_VERIFY_OK: AtomicBool = AtomicBool::new(true);
static DEGRADE_STREAK: AtomicUsize = AtomicUsize::new(0);

pub fn register_remember(f: RememberFn) {
    *REMEMBER_FN.lock() = Some(f);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeapPlanKind {
    Ok,
    Degrade,
    Escalate,
}

#[derive(Clone, Copy, Debug)]
pub struct HeapPlan {
    pub kind: HeapPlanKind,
    pub tier: ComputeTier,
    pub ctx_cap: usize,
    pub max_gen: usize,
    pub soft_stride: usize,
    pub force_slim: bool,
    pub headroom_mb: usize,
    pub pressure: u8,
}

fn tier_idx(t: ComputeTier) -> usize {
    match t {
        ComputeTier::Cheap => 0,
        ComputeTier::Normal => 1,
        ComputeTier::Full => 2,
    }
}

/// Observe + Plan a partir do tier heurístico e do heap real.
pub fn plan_for(
    base: ComputeTier,
    hidden: usize,
    use_bpe: bool,
    is_greeting: bool,
) -> HeapPlan {
    let obs = k_nano::allocator::heap_observe();
    let mut tier = base;
    let mut kind = HeapPlanKind::Ok;
    let mut force_slim = hidden >= 2048;

    // Plan: degradar por pressão / headroom.
    if obs.pressure >= 2 || obs.headroom_mb < 128 {
        tier = ComputeTier::Cheap;
        kind = HeapPlanKind::Degrade;
        force_slim = true;
    } else if obs.pressure >= 1 || obs.headroom_mb < 384 {
        tier = match base {
            ComputeTier::Full => ComputeTier::Normal,
            ComputeTier::Normal => ComputeTier::Cheap,
            ComputeTier::Cheap => ComputeTier::Cheap,
        };
        if tier != base {
            kind = HeapPlanKind::Degrade;
        }
        force_slim = true;
    }

    // Escalate: headroom insuficiente até para Cheap decode (1 token residual).
    if hidden >= 2048 && obs.headroom_mb < 48 {
        kind = HeapPlanKind::Escalate;
        tier = ComputeTier::Cheap;
        force_slim = true;
    }

    let soft_stride = difficulty_gate::soft_stride_for(tier, hidden);
    // Ctx: Full 512 → Normal 256 → Cheap 64 (heavy); escalate 32.
    let ctx_cap = if hidden < 2048 {
        64
    } else {
        match (kind, tier) {
            (HeapPlanKind::Escalate, _) => 32,
            (_, ComputeTier::Cheap) => 64,
            (_, ComputeTier::Normal) => 256,
            (_, ComputeTier::Full) => 512,
        }
    };
    let max_gen = difficulty_gate::max_gen_for(tier, hidden, use_bpe, is_greeting);

    HeapPlan {
        kind,
        tier,
        ctx_cap,
        max_gen,
        soft_stride,
        force_slim,
        headroom_mb: obs.headroom_mb,
        pressure: obs.pressure,
    }
}

/// Act: aplica overrides no difficulty_gate + grava plano.
pub fn apply_plan(plan: HeapPlan, hidden: usize) {
    LAST_PLAN_CTX.store(plan.ctx_cap, Ordering::Release);
    LAST_PLAN_TIER.store(
        match plan.kind {
            HeapPlanKind::Escalate => 3,
            _ => tier_idx(plan.tier),
        },
        Ordering::Release,
    );
    difficulty_gate::apply_tier(plan.tier, hidden);
    difficulty_gate::set_soft_stride_override(plan.soft_stride);
    if plan.kind == HeapPlanKind::Escalate {
        // max_gen mínimo — job curto ou escalate msg.
        difficulty_gate::set_force_max_gen(plan.max_gen.min(2).max(1));
    }
    k_nano::slog_cortex!(
        "HeapAIOS",
        if plan.kind == HeapPlanKind::Escalate {
            "warn"
        } else {
            "ok"
        },
        "plan={} tier={} ctx={} max_gen={} stride={} slim={} headroom={}MB pressure={}",
        match plan.kind {
            HeapPlanKind::Ok => "ok",
            HeapPlanKind::Degrade => "degrade",
            HeapPlanKind::Escalate => "escalate",
        },
        plan.tier.name(),
        plan.ctx_cap,
        plan.max_gen,
        plan.soft_stride,
        plan.force_slim as u8,
        plan.headroom_mb,
        plan.pressure
    );
    if plan.kind == HeapPlanKind::Degrade {
        DEGRADE_STREAK.fetch_add(1, Ordering::Relaxed);
        k_nano::allocator::clear_critical_pressure();
    } else if plan.kind == HeapPlanKind::Ok {
        DEGRADE_STREAK.store(0, Ordering::Relaxed);
    }
    // Observe→EventBus fora do grow.
    k_nano::allocator::publish_heap_pressure_if_due();
}

pub fn last_ctx_cap() -> usize {
    let c = LAST_PLAN_CTX.load(Ordering::Acquire);
    if c == 0 {
        512
    } else {
        c
    }
}

pub fn clear_job_overrides() {
    difficulty_gate::clear_soft_stride_override();
    difficulty_gate::set_force_max_gen(0);
}

/// Verify pós-job: sem OOM (pressure não subiu a critical durante) + tok sample.
pub fn verify_job(completed: bool, toks: u64, us: u64) {
    let obs = k_nano::allocator::heap_observe();
    let ok = completed && obs.pressure < 2;
    LAST_VERIFY_OK.store(ok, Ordering::Release);
    let tps_milli = if toks > 0 && us > 0 {
        toks.saturating_mul(1_000_000) / us
    } else {
        0
    };
    k_nano::slog_cortex!(
        "HeapAIOS",
        if ok { "ok" } else { "warn" },
        "verify completed={} pressure={} milli_tok_s≈{} degrade_streak={}",
        completed as u8,
        obs.pressure,
        tps_milli,
        DEGRADE_STREAK.load(Ordering::Relaxed)
    );
    if !ok || plan_was_degraded() {
        remember_host(ok, toks, us);
    }
}

fn plan_was_degraded() -> bool {
    let ctx = LAST_PLAN_CTX.load(Ordering::Acquire);
    let streak = DEGRADE_STREAK.load(Ordering::Relaxed);
    let tier = LAST_PLAN_TIER.load(Ordering::Acquire);
    streak > 0 || tier == 3 || (ctx > 0 && ctx <= 64)
}

fn remember_host(ok: bool, toks: u64, us: u64) {
    let obs = k_nano::allocator::heap_observe();
    let ctx = last_ctx_cap();
    let tier = LAST_PLAN_TIER.load(Ordering::Acquire);
    let note = alloc::format!(
        "heap_aios host window={}MB used={}MB headroom={}MB ctx≤{} tier_idx={} verify_ok={} toks={} us={} refuse_n={}",
        obs.window_mb,
        obs.used_mb,
        obs.headroom_mb,
        ctx,
        tier,
        ok as u8,
        toks,
        us,
        obs.refuse_count
    );
    if let Some(f) = *REMEMBER_FN.lock() {
        f(&note);
    } else {
        k_nano::slog_cortex!("HeapAIOS", "ok", "remember(local): {}", note);
    }
}

/// Mensagem HITL quando Escalate.
pub fn escalate_message(plan: &HeapPlan) -> alloc::string::String {
    alloc::format!(
        "[heap escalate] headroom={}MB pressure={} — ctx≤{} insuficient; /approve ou feche apps. HITL.",
        plan.headroom_mb,
        plan.pressure,
        plan.ctx_cap
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escalate_message_mentions_hitl() {
        let p = HeapPlan {
            kind: HeapPlanKind::Escalate,
            tier: ComputeTier::Cheap,
            ctx_cap: 32,
            max_gen: 2,
            soft_stride: 3,
            force_slim: true,
            headroom_mb: 20,
            pressure: 2,
        };
        let m = escalate_message(&p);
        assert!(m.contains("HITL") || m.contains("escalate"));
    }
}
