//! Log estruturado K³CHJ (ADR-0041 anéis + ADR-0092 severidade).
//!
//! Formato canónico (tick vem do `serial::_print`):
//! ```text
//! [T+N] [Rn] [k-xxx] [src] [sev] - texto
//! ```
//!
//! `sev` ∈ {ok, warn, fail, trace}. Desconhecido → `trace` (mudo na consola).

use core::sync::atomic::{AtomicU8, Ordering};

/// Privilege / consciousness ring tag.
pub const R0: &str = "R0";
pub const R1: &str = "R1";
pub const R2: &str = "R2";
pub const R3: &str = "R3";

/// Canonical crate tags (grep-friendly).
pub const K_NANO: &str = "k-nano";
pub const K_HAL: &str = "k-hal";
pub const K_AI: &str = "k-ai";
pub const K_CORTEX: &str = "cortex";
pub const K_HERMES: &str = "hermes";
pub const K_JARBAS: &str = "jarbas";
pub const K_BIN: &str = "nk";

/// 0 = trace visível na consola; 1 = ok+; 2 = warn+; 3 = só fail.
pub const CONSOLE_TRACE: u8 = 0;
pub const CONSOLE_OK: u8 = 1;
pub const CONSOLE_WARN: u8 = 2;
pub const CONSOLE_FAIL: u8 = 3;

static CONSOLE_MIN_SEV: AtomicU8 = AtomicU8::new(CONSOLE_OK);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Sev {
    Trace = 0,
    Ok = 1,
    Warn = 2,
    Fail = 3,
}

impl Sev {
    pub fn as_str(self) -> &'static str {
        match self {
            Sev::Trace => "trace",
            Sev::Ok => "ok",
            Sev::Warn => "warn",
            Sev::Fail => "fail",
        }
    }

    pub fn from_sub(sub: &str) -> Self {
        match sub {
            "ok" | "OK" | "pass" | "PASS" => Sev::Ok,
            "warn" | "WARN" | "warning" | "degraded" => Sev::Warn,
            "fail" | "FAIL" | "error" | "panic" => Sev::Fail,
            "trace" | "TRACE" | "debug" | "ckpt" => Sev::Trace,
            _ => Sev::Trace,
        }
    }
}

pub fn set_console_min_sev(min: u8) {
    CONSOLE_MIN_SEV.store(min.min(CONSOLE_FAIL), Ordering::Relaxed);
}

pub fn console_min_sev() -> u8 {
    CONSOLE_MIN_SEV.load(Ordering::Relaxed)
}

pub fn console_allows(sev: Sev) -> bool {
    (sev as u8) >= CONSOLE_MIN_SEV.load(Ordering::Relaxed)
}

pub fn file_allows_trace() -> bool {
    cfg!(feature = "boot-trace")
}

pub fn file_allows(sev: Sev) -> bool {
    sev != Sev::Trace || file_allows_trace()
}

/// Emite linha slog. TRACE não vai à consola (default); ficheiro só com `boot-trace`.
pub fn emit(ring: &str, krate: &str, item: &str, sub: &str, args: core::fmt::Arguments) {
    let sev = Sev::from_sub(sub);
    let to_console = console_allows(sev);
    let to_file = file_allows(sev);
    if !to_console && !to_file {
        return;
    }
    crate::serial::emit_tagged(ring, krate, item, sev.as_str(), args, to_console, to_file);
}

/// Core structured log. Prefer crate helpers `slog_hal!` / `slog_hermes!` etc.
#[macro_export]
macro_rules! slog {
    ($ring:expr, $krate:expr, $item:expr, $sub:expr, $($arg:tt)*) => {
        $crate::slog::emit(
            $ring,
            $krate,
            $item,
            $sub,
            format_args!($($arg)*),
        )
    };
}

#[macro_export]
macro_rules! slog_nano {
    ($item:expr, $sub:expr, $($arg:tt)*) => {
        $crate::slog!($crate::slog::R0, $crate::slog::K_NANO, $item, $sub, $($arg)*)
    };
}

#[macro_export]
macro_rules! slog_hal {
    ($item:expr, $sub:expr, $($arg:tt)*) => {
        $crate::slog!($crate::slog::R1, $crate::slog::K_HAL, $item, $sub, $($arg)*)
    };
}

#[macro_export]
macro_rules! slog_kai {
    ($item:expr, $sub:expr, $($arg:tt)*) => {
        $crate::slog!($crate::slog::R2, $crate::slog::K_AI, $item, $sub, $($arg)*)
    };
}

#[macro_export]
macro_rules! slog_cortex {
    ($item:expr, $sub:expr, $($arg:tt)*) => {
        $crate::slog!($crate::slog::R2, $crate::slog::K_CORTEX, $item, $sub, $($arg)*)
    };
}

#[macro_export]
macro_rules! slog_hermes {
    ($item:expr, $sub:expr, $($arg:tt)*) => {
        $crate::slog!($crate::slog::R3, $crate::slog::K_HERMES, $item, $sub, $($arg)*)
    };
}

#[macro_export]
macro_rules! slog_jarbas {
    ($item:expr, $sub:expr, $($arg:tt)*) => {
        $crate::slog!($crate::slog::R3, $crate::slog::K_JARBAS, $item, $sub, $($arg)*)
    };
}

#[macro_export]
macro_rules! slog_bin {
    ($item:expr, $sub:expr, $($arg:tt)*) => {
        $crate::slog!($crate::slog::R0, $crate::slog::K_BIN, $item, $sub, $($arg)*)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fail_passes_default_console_filter() {
        set_console_min_sev(CONSOLE_OK);
        assert!(console_allows(Sev::Fail));
        assert!(console_allows(Sev::Ok));
        assert!(console_allows(Sev::Warn));
    }

    #[test]
    fn trace_hidden_on_default_console() {
        set_console_min_sev(CONSOLE_OK);
        assert!(!console_allows(Sev::Trace));
        assert!(!console_allows(Sev::from_sub("info")));
        assert!(!console_allows(Sev::from_sub("e1000")));
        assert_eq!(Sev::from_sub("ok"), Sev::Ok);
    }

    #[test]
    fn unknown_sub_is_trace() {
        assert_eq!(Sev::from_sub("ckpt"), Sev::Trace);
        assert_eq!(Sev::from_sub("msg"), Sev::Trace);
        assert!(!file_allows(Sev::Trace) || cfg!(feature = "boot-trace"));
    }
}
