//! Log estruturado K³CHJ (ADR-0041 anéis + ADR-0092 severidade + ADR-0103 homes).
//!
//! Formato canónico (tick vem do `serial::_print`):
//! ```text
//! [T+N] [Rn] [k-xxx] [src] [sev] - home=<crate::mod> | texto
//! ```
//!
//! `sev` ∈ {ok, warn, fail, trace}. Desconhecido → `trace` (mudo na consola).
//!
//! **Contrato para IA (grep):**
//! - `[src]` = subsistema estável (`Net`, `USB`, `SMP`, `FAT32`, `BOOT`)
//! - `home=` = módulo onde a verdade vive pós-emagreçer (ADR-0103)
//! - `facade=` = re-export fino (opcional) quando o bin/crate espelha
//! - `ref=ADR-NNNN` = decisão arquitectural quando o evento é normativo
//!
//! Exemplo:
//! `[T+12] [R1] [k-hal] [USB] [ok] - home=k_hal::usb::hub_msc | MSC bringup OK port=5`

use core::sync::atomic::{AtomicU64, AtomicU8, Ordering};

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
            // Canónicos + aliases pós-s321 (evita TRACE mudo por "info"/subsistema no slot sev).
            // SESSION_372: callers históricos põem o *nome do driver* no slot sev
            // (`slog_nano!("Net","e1000",...)`) — desconhecido = TRACE = dmesg cego (ADR-0092).
            "ok" | "OK" | "pass" | "PASS" | "ready" | "bound" | "grant" | "refresh"
            | "info" | "INFO"
            | "e1000" | "i225" | "ahci" | "ata" | "nvme" | "NVMe" | "virtio" | "VIRTIO"
            | "MBR" | "GPT" | "FAT32" | "ACPI" | "HEAP" | "MEM" | "P2P" | "PCI"
            | "IDT" | "GDT" | "PIC" | "INSTALL" | "SYS-INST" | "SLIP" | "USB"
            | "SMP" | "Net" | "Disk"
            // SESSION_373 k_hal: probe/init/intel no slot sev (BCS Ring OK era TRACE).
            | "init" | "intel" | "INTEL" | "NVIDIA" | "AMD" | "BLIT" | "NKP"
            | "ATH10K" | "IWL" | "FE" | "cursor" | "bind" | "probe" | "kvdma"
            | "MATMUL" | "GPU" | "AUD" | "NPU" | "canary"
            | "BAR" | "bar" | "BACKEND" | "GEN9" | "GUC" | "D2" | "D3" | "D4"
            | "CE" | "KIQ" | "MES" | "GGTT" | "gtt" | "BCS" | "bench" | "XQUEUE"
            | "VRAM" | "SASOS" | "ACR" | "WIFI" | "Wifi" | "DB" | "vga" | "XPU"
            | "SZ" | "RING" | "ring" | "nvidia" | "COMPUTE" | "Cap" => Sev::Ok,
            "warn" | "WARN" | "warning" | "degraded" | "skip" | "absent" | "msc" => Sev::Warn,
            "fail" | "FAIL" | "error" | "panic" | "err" => Sev::Fail,
            "trace" | "TRACE" | "debug" | "ckpt" | "mmIO" | "mmio" => Sev::Trace,
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

#[cfg(test)]
mod channel_a_tests {
    use super::*;

    /// ADR-0092: ok/warn/fail vão a ficheiro; trace só com boot-trace.
    #[test]
    fn file_gate_matches_adr0092() {
        assert!(file_allows(Sev::Ok));
        assert!(file_allows(Sev::Warn));
        assert!(file_allows(Sev::Fail));
        assert_eq!(file_allows(Sev::Trace), cfg!(feature = "boot-trace"));
    }
}

/// Emite linha slog. TRACE não vai à consola (default); ficheiro só com `boot-trace`.
pub fn emit(ring: &str, krate: &str, item: &str, sub: &str, args: core::fmt::Arguments) {
    let sev = Sev::from_sub(sub);
    // L20 (onda 4 / ADR-0092): token de severidade DESCONHECIDO caía em Trace
    // mudo — dmesg cego. Emite UMA linha warn rate-limited (a cada ~300 ticks)
    // nomeando o token para o dev mapear em `Sev::from_sub`. A linha original
    // continua com a severidade classificada (Trace) — não promove a mensagem.
    if sev == Sev::Trace
        && !matches!(sub, "trace" | "TRACE" | "debug" | "ckpt" | "mmIO" | "mmio")
    {
        static LAST_UNK_LOG: AtomicU64 = AtomicU64::new(0);
        let n = UNK_SUB_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
        let now = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
        let last = LAST_UNK_LOG.load(Ordering::Relaxed);
        if last == 0 || now.wrapping_sub(last) >= 300 {
            LAST_UNK_LOG.store(now, Ordering::Relaxed);
            crate::serial::emit_tagged(
                "slog", "slog", "slog", "warn",
                format_args!("sev desconhecida '{}' — total={} (mapear em Sev::from_sub)", sub, n),
                true, true,
            );
        }
    }
    let to_console = console_allows(sev);
    let to_file = file_allows(sev);
    if !to_console && !to_file {
        return;
    }
    crate::serial::emit_tagged(ring, krate, item, sev.as_str(), args, to_console, to_file);
}

/// Contagem total de linhas slog com severidade desconhecida (diagnóstico).
static UNK_SUB_COUNT: AtomicU64 = AtomicU64::new(0);

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

/// slog com `home=` explícito (ADR-0103) — preferir em bring-up / facades.
#[macro_export]
macro_rules! slog_home {
    ($ring:expr, $krate:expr, $item:expr, $sub:expr, $home:expr, $($arg:tt)*) => {
        $crate::slog::emit(
            $ring,
            $krate,
            $item,
            $sub,
            format_args!("home={} | {}", $home, format_args!($($arg)*)),
        )
    };
}

#[macro_export]
macro_rules! slog_nano_home {
    ($item:expr, $sub:expr, $home:expr, $($arg:tt)*) => {
        $crate::slog_home!(
            $crate::slog::R0,
            $crate::slog::K_NANO,
            $item,
            $sub,
            $home,
            $($arg)*
        )
    };
}

#[macro_export]
macro_rules! slog_hal_home {
    ($item:expr, $sub:expr, $home:expr, $($arg:tt)*) => {
        $crate::slog_home!(
            $crate::slog::R1,
            $crate::slog::K_HAL,
            $item,
            $sub,
            $home,
            $($arg)*
        )
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
        // "info" é alias de ok (s321: k_hal usava info → TRACE mudo).
        assert!(console_allows(Sev::from_sub("info")));
        // SESSION_372: driver name no slot sev → Ok (antes TRACE escondia Reset OK).
        assert!(console_allows(Sev::from_sub("e1000")));
        assert_eq!(Sev::from_sub("ok"), Sev::Ok);
    }

    #[test]
    fn unknown_sub_is_trace() {
        assert_eq!(Sev::from_sub("ckpt"), Sev::Trace);
        assert_eq!(Sev::from_sub("msg"), Sev::Trace);
        assert!(!file_allows(Sev::Trace) || cfg!(feature = "boot-trace"));
    }

    #[test]
    fn migration_aliases_visible() {
        assert_eq!(Sev::from_sub("info"), Sev::Ok);
        assert_eq!(Sev::from_sub("bound"), Sev::Ok);
        assert_eq!(Sev::from_sub("e1000"), Sev::Ok);
        assert_eq!(Sev::from_sub("ahci"), Sev::Ok);
        assert_eq!(Sev::from_sub("intel"), Sev::Ok);
        assert_eq!(Sev::from_sub("init"), Sev::Ok);
        assert_eq!(Sev::from_sub("canary"), Sev::Ok);
        assert_eq!(Sev::from_sub("BAR"), Sev::Ok);
        assert_eq!(Sev::from_sub("GEN9"), Sev::Ok);
        assert_eq!(Sev::from_sub("D4"), Sev::Ok);
        assert_eq!(Sev::from_sub("err"), Sev::Fail);
        assert_eq!(Sev::from_sub("msc"), Sev::Warn);
        assert_eq!(Sev::from_sub("skip"), Sev::Warn);
    }
}
