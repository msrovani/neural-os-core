//! Storage posture AIOS (s493 / NeuralFS.md §14 / ADR-0088).
//! Observe-only: inventário de data FS + backend de BOOT.LOG — sem I/O pesado.
//! Telemetria: slog `STORAGE posture data_fs=… bootlog=…` (n/a ≠ 0).

use core::sync::atomic::{AtomicU8, Ordering};

/// Data payload filesystem detected (or none).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum DataFs {
    /// Ainda não observado neste boot.
    Unknown = 0,
    /// Partição NeuralFS (MBR 0x7F / GPT NeuralFS) montável ou magic visto.
    NeuralFs = 1,
    /// FAT32/exFAT de dados (legado DEGRADED).
    Fat32 = 2,
    /// Nenhuma partição de dados reconhecida.
    None = 3,
}

/// Onde o canal DEV BOOT.LOG deve gravar / gravou.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BootLogBackend {
    Unknown = 0,
    Esp = 1,
    FatData = 2,
    NeuralFsLogs = 3,
    None = 4,
}

static DATA_FS: AtomicU8 = AtomicU8::new(DataFs::Unknown as u8);
static BOOTLOG_BE: AtomicU8 = AtomicU8::new(BootLogBackend::Unknown as u8);
static NRFS_MOUNTED: AtomicU8 = AtomicU8::new(0);
static POSTURE_LOGGED: AtomicU8 = AtomicU8::new(0);

impl DataFs {
    pub fn as_str(self) -> &'static str {
        match self {
            DataFs::Unknown => "unknown",
            DataFs::NeuralFs => "neuralfs",
            DataFs::Fat32 => "fat32",
            DataFs::None => "none",
        }
    }

    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => DataFs::NeuralFs,
            2 => DataFs::Fat32,
            3 => DataFs::None,
            _ => DataFs::Unknown,
        }
    }

    /// FAT data = DEGRADED; NeuralFS = canônico.
    pub fn is_degraded(self) -> bool {
        matches!(self, DataFs::Fat32)
    }
}

impl BootLogBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            BootLogBackend::Unknown => "unknown",
            BootLogBackend::Esp => "esp",
            BootLogBackend::FatData => "fat_data",
            BootLogBackend::NeuralFsLogs => "nrfs_logs",
            BootLogBackend::None => "none",
        }
    }

    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => BootLogBackend::Esp,
            2 => BootLogBackend::FatData,
            3 => BootLogBackend::NeuralFsLogs,
            4 => BootLogBackend::None,
            _ => BootLogBackend::Unknown,
        }
    }
}

pub fn note_data_fs(fs: DataFs) {
    DATA_FS.store(fs as u8, Ordering::Release);
}

pub fn data_fs() -> DataFs {
    DataFs::from_u8(DATA_FS.load(Ordering::Acquire))
}

pub fn note_bootlog_backend(be: BootLogBackend) {
    BOOTLOG_BE.store(be as u8, Ordering::Release);
}

pub fn bootlog_backend() -> BootLogBackend {
    BootLogBackend::from_u8(BOOTLOG_BE.load(Ordering::Acquire))
}

pub fn note_nrfs_mounted(ok: bool) {
    NRFS_MOUNTED.store(if ok { 1 } else { 0 }, Ordering::Release);
    if ok {
        note_data_fs(DataFs::NeuralFs);
    }
}

pub fn nrfs_mounted() -> bool {
    NRFS_MOUNTED.load(Ordering::Acquire) != 0
}

/// Prefer ESP for DEV BOOT.LOG when data is NeuralFS or unknown-canonical.
pub fn bootlog_prefer_esp() -> bool {
    matches!(data_fs(), DataFs::NeuralFs | DataFs::Unknown | DataFs::None)
}

/// One-shot slog for boot Observe (safe to call multiple times).
pub fn slog_posture_once() {
    if POSTURE_LOGGED.swap(1, Ordering::AcqRel) != 0 {
        return;
    }
    let dfs = data_fs();
    let sev = if dfs.is_degraded() { "warn" } else { "ok" };
    crate::slog_bin!(
        "STORAGE",
        sev,
        "posture data_fs={} degraded={} nrfs_mounted={} bootlog={} (NeuralFS.md §14)",
        dfs.as_str(),
        dfs.is_degraded(),
        nrfs_mounted(),
        bootlog_backend().as_str()
    );
}

static FAT_FALLBACK_LOGGED: AtomicU8 = AtomicU8::new(0);

/// Hub LOO caiu em FAT — marca DEGRADED + telemetria (não silêncio).
pub fn observe_fat_fallback_once() {
    if data_fs() != DataFs::NeuralFs {
        note_data_fs(DataFs::Fat32);
    }
    if FAT_FALLBACK_LOGGED.swap(1, Ordering::AcqRel) != 0 {
        return;
    }
    crate::slog_bin!(
        "STORAGE",
        "warn",
        "storage_verdict=DEGRADED fat_fallback=1 (remember NeuralFS.md §14)"
    );
    slog_posture_once();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn degraded_only_fat32() {
        assert!(DataFs::Fat32.is_degraded());
        assert!(!DataFs::NeuralFs.is_degraded());
        assert!(!DataFs::None.is_degraded());
    }

    #[test]
    fn prefer_esp_when_nrfs() {
        note_data_fs(DataFs::NeuralFs);
        assert!(bootlog_prefer_esp());
        note_data_fs(DataFs::Fat32);
        assert!(!bootlog_prefer_esp());
    }
}
