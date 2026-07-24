//! Virtual consoles Ctrl+Alt+F1–F6 MVP (Labor 40).

use core::sync::atomic::{AtomicU8, Ordering};

const N: u8 = 6;
static ACTIVE: AtomicU8 = AtomicU8::new(0);

pub fn active() -> u8 {
    ACTIVE.load(Ordering::Relaxed)
}

pub fn switch(n: u8) -> bool {
    if n >= N {
        return false;
    }
    ACTIVE.store(n, Ordering::Relaxed);
    k_nano::slog_jarbas!("VCON", "info", "switch=F{} VERDICT=PARTIAL", n + 1);
    true
}

/// Scancode path: Ctrl+Alt+F1..F6 (F1=0x3B … F6=0x40) — caller passa índice 0..5.
pub fn on_ctrl_alt_fn(fn_index: u8) -> bool {
    switch(fn_index)
}

pub fn boot_smoke() -> bool {
    let ok = switch(0) && switch(1) && switch(0);
    k_nano::slog_jarbas!(
        "VCON",
        "info",
        "step=vconsole status=OK n={} VERDICT=PARTIAL reason=switch_mvp",
        N
    );
    ok
}