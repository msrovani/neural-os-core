//! N1.1 — Telemetria honesta de assets (LLM, BGE, Piper, FW GPU).
//! Estados: Loaded | Absent | Failed. Sem SUCCESS falso.

use core::sync::atomic::{AtomicU8, Ordering};
/// Estado de carga de um asset de boot/runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LoadStatus {
    /// Nunca tentado ou confirmado ausente no media.
    Absent = 0,
    /// Carregado e utilizável.
    Loaded = 1,
    /// Tentativa falhou (magic inválido, OOM, I/O, etc).
    Failed = 2,
}

impl LoadStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            LoadStatus::Absent => "ABSENT",
            LoadStatus::Loaded => "LOADED",
            LoadStatus::Failed => "FAILED",
        }
    }

    fn from_u8(v: u8) -> Self {
        match v {
            1 => LoadStatus::Loaded,
            2 => LoadStatus::Failed,
            _ => LoadStatus::Absent,
        }
    }
}

/// Assets rastreados pelo banner `[STATUS]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    /// Modelo LLM principal (BitNet 2B / BITNET.BIN).
    Llm,
    /// Embedding BGE (BGE.BIN).
    Bge,
    /// Piper TTS neural.
    Piper,
    /// Firmware GPU (ex.: NVIDIA ACR) — Loaded só se blob+VID ok.
    FwGpu,
}

static LLM: AtomicU8 = AtomicU8::new(0);
static BGE: AtomicU8 = AtomicU8::new(0);
static PIPER: AtomicU8 = AtomicU8::new(0);
static FW_GPU: AtomicU8 = AtomicU8::new(0);

fn slot(kind: AssetKind) -> &'static AtomicU8 {
    match kind {
        AssetKind::Llm => &LLM,
        AssetKind::Bge => &BGE,
        AssetKind::Piper => &PIPER,
        AssetKind::FwGpu => &FW_GPU,
    }
}

pub fn get(kind: AssetKind) -> LoadStatus {
    LoadStatus::from_u8(slot(kind).load(Ordering::Relaxed))
}

pub fn set(kind: AssetKind, status: LoadStatus) {
    slot(kind).store(status as u8, Ordering::Relaxed);
}

/// Só sobe para Loaded/Failed a partir de Absent; Loaded não rebaixa para Absent.
pub fn set_if_upgrade(kind: AssetKind, status: LoadStatus) {
    let cur = get(kind);
    if cur == LoadStatus::Loaded && status != LoadStatus::Loaded {
        return;
    }
    set(kind, status);
}

/// Banner serial coerente com LLM-TEST / FAT load.
pub fn print_status_banner() {
    crate::slog_bin!("Status", "ok", "llm={} bge={} piper={} fw_gpu={}",
        get(AssetKind::Llm).as_str(),
        get(AssetKind::Bge).as_str(),
        get(AssetKind::Piper).as_str(),
        get(AssetKind::FwGpu).as_str(),);
}
