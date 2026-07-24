//! Firewall stateful MVP — allow/deny smoltcp path (Labor 57).

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Deny,
}

struct Rule {
    /// 0 = any
    dst_port: u16,
    verdict: Verdict,
}

static ENABLED: AtomicBool = AtomicBool::new(true);
static ALLOWED: AtomicU32 = AtomicU32::new(0);
static DENIED: AtomicU32 = AtomicU32::new(0);
static RULES: Mutex<[Option<Rule>; 8]> = Mutex::new([const { None }; 8]);

pub fn clear_rules() {
    let mut g = RULES.lock();
    for s in g.iter_mut() {
        *s = None;
    }
}

pub fn add_rule(dst_port: u16, verdict: Verdict) -> bool {
    let mut g = RULES.lock();
    for s in g.iter_mut() {
        if s.is_none() {
            *s = Some(Rule { dst_port, verdict });
            return true;
        }
    }
    false
}

pub fn check(dst_port: u16) -> Verdict {
    if !ENABLED.load(Ordering::Relaxed) {
        return Verdict::Allow;
    }
    let g = RULES.lock();
    for s in g.iter() {
        if let Some(r) = s {
            if r.dst_port == 0 || r.dst_port == dst_port {
                match r.verdict {
                    Verdict::Allow => {
                        ALLOWED.fetch_add(1, Ordering::Relaxed);
                        return Verdict::Allow;
                    }
                    Verdict::Deny => {
                        DENIED.fetch_add(1, Ordering::Relaxed);
                        return Verdict::Deny;
                    }
                }
            }
        }
    }
    ALLOWED.fetch_add(1, Ordering::Relaxed);
    Verdict::Allow
}

pub fn boot_smoke() -> bool {
    clear_rules();
    let _ = add_rule(23, Verdict::Deny);
    let _ = add_rule(80, Verdict::Allow);
    let ok = check(23) == Verdict::Deny && check(80) == Verdict::Allow;
    crate::slog_nano!(
        "FW",
        "info",
        "step=stateful status={} deny={} allow={} VERDICT={}",
        if ok { "OK" } else { "FAIL" },
        DENIED.load(Ordering::Relaxed),
        ALLOWED.load(Ordering::Relaxed),
        if ok { "PASS" } else { "FAIL" }
    );
    ok
}