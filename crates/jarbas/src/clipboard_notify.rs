//! Clipboard + notify toast bridge (Labor 38). EventBus toast via slog + ring.
//! Cosmético — wire NotificationGate sem novo framework.

use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;
use core::sync::atomic::Ordering;

const CLIP_MAX: usize = 4096;
const TOAST_MAX: usize = 8;
const TOAST_TTL_TICKS: usize = 120; // ~2s at 60 FPS

static CLIP: Mutex<String> = Mutex::new(String::new());
static TOASTS: Mutex<Vec<(String, usize)>> = Mutex::new(Vec::new()); // (msg, expiry_tick)

/// EventBus topic for toast notifications
pub const TOPIC_TOAST: &str = "TOAST";

pub fn clipboard_set(text: &str) {
    let mut c = CLIP.lock();
    c.clear();
    let n = text.len().min(CLIP_MAX);
    c.push_str(&text[..n]);
}

pub fn clipboard_get() -> String {
    CLIP.lock().clone()
}

pub fn toast_push(msg: &str) {
    let now = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed);
    let mut t = TOASTS.lock();
    if t.len() >= TOAST_MAX {
        t.remove(0);
    }
    t.push((String::from(msg), now + TOAST_TTL_TICKS));
    k_nano::slog_bin!("NOTIFY", "info", "toast={}", msg);
}

/// Returns toasts that haven't expired yet
pub fn toast_get_active(now: usize) -> Vec<String> {
    let t = TOASTS.lock();
    t.iter()
        .filter(|(_, exp)| *exp > now)
        .map(|(msg, _)| msg.clone())
        .collect()
}

pub fn toast_pop() -> Option<String> {
    let mut t = TOASTS.lock();
    if t.is_empty() {
        None
    } else {
        Some(t.remove(0).0)
    }
}

pub fn boot_smoke() -> bool {
    clipboard_set("neural-os");
    let g = clipboard_get();
    toast_push("L38 notify smoke");
    let _ = toast_pop();
    let ok = g == "neural-os";
    k_nano::slog_bin!(
        "NOTIFY",
        "info",
        "step=clip_notify status={} VERDICT={}",
        if ok { "OK" } else { "FAIL" },
        if ok { "PASS" } else { "FAIL" }
    );
    ok
}