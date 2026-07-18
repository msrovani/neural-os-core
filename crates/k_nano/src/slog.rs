//! Log estruturado K²CHJ (ADR-0041 anéis).
//!
//! Formato canônico (tick vem do `serial_println` / `_print`):
//! ```text
//! [T+N] [Rn] [k-xxx] [Item] [subitem] - texto e dados
//! ```
//!
//! | Ring | Crate   | Const  |
//! |------|---------|--------|
//! | R0   | k-nano  | R0 / K_NANO |
//! | R1   | k-hal   | R1 / K_HAL  |
//! | R2   | k_ai    | R2 / K_AI   |
//! | R2   | cortex  | R2 / K_CORTEX |
//! | R3   | hermes  | R3 / K_HERMES |
//! | R3   | jarbas  | R3 / K_JARBAS |
//! | BIN  | neural-kernel | R0 / K_BIN (integração) |

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

/// Core structured log. Prefer crate helpers `slog_hal!` / `slog_hermes!` etc.
#[macro_export]
macro_rules! slog {
    ($ring:expr, $krate:expr, $item:expr, $sub:expr, $($arg:tt)*) => {
        $crate::serial_println!(
            "[{}] [{}] [{}] [{}] - {}",
            $ring,
            $krate,
            $item,
            $sub,
            format_args!($($arg)*)
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
