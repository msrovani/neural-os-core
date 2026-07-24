//! Clipboard + notify toast bridge (Labor 38). EventBus toast via slog + ring.
//! Cosmético — wire NotificationGate sem novo framework.

use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

const CLIP_MAX: usize = 4096;
const TOAST_MAX: usize = 8;

static CLIP: Mutex<String> = Mutex::new(String::new());
static TOASTS: Mutex<Vec<String>> = Mutex::new(Vec::new());

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
    let mut t = TOASTS.lock();
    if t.len() >= TOAST_MAX {
        t.remove(0);
    }
    t.push(String::from(msg));
    k_nano::slog_bin!("NOTIFY", "info", "toast={}", msg);
}

pub fn toast_pop() -> Option<String> {
    let mut t = TOASTS.lock();
    if t.is_empty() {
        None
    } else {
        Some(t.remove(0))
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