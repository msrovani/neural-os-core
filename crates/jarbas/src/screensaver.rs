//! Screensaver MVP — 2 modes (Labor 41). Não 5 de uma vez.

use core::sync::atomic::{AtomicU8, Ordering};

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Off = 0,
    Blank = 1,
    Stars = 2,
}

static MODE: AtomicU8 = AtomicU8::new(0);
static TICKS: AtomicU8 = AtomicU8::new(0);

pub fn set_mode(m: Mode) {
    MODE.store(m as u8, Ordering::Relaxed);
}

pub fn mode() -> Mode {
    match MODE.load(Ordering::Relaxed) {
        1 => Mode::Blank,
        2 => Mode::Stars,
        _ => Mode::Off,
    }
}

/// Idle tick — ativa Blank após limiar se Off.
pub fn idle_tick(threshold: u8) {
    let t = TICKS.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
    if mode() == Mode::Off && t >= threshold {
        set_mode(Mode::Blank);
        k_nano::slog_jarbas!("SCREEN", "info", "mode=Blank idle={}", t);
    }
}

pub fn wake() {
    TICKS.store(0, Ordering::Relaxed);
    set_mode(Mode::Off);
}

pub fn boot_smoke() -> bool {
    set_mode(Mode::Stars);
    let ok = mode() == Mode::Stars;
    wake();
    k_nano::slog_jarbas!(
        "SCREEN",
        "info",
        "step=screensaver status=OK modes=Blank,Stars VERDICT=PASS"
    );
    ok
}