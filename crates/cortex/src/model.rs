//! cortex::model — registro mínimo de modelos .bitnet v6 (ADR-0085 §7).
//! Fronteira de formato única: load_model_v6 despacha por model_type para
//! LLM / HWExpert. Router é formato posicional próprio (trinity) — separado.
//! ModelHub usa `register_bytes` como ponto único.
//!
//! Fase 1 (autonomia): ModelHeader lê todos os campos do header v6 em runtime.
//! Zero hardcoded — o sistema descobre hidden/layers/vocab/kv/q_dim sozinho.

use alloc::string::String;
use core::sync::atomic::{AtomicBool, Ordering};
use crate::cortex::{HwExpertV4Model, TransformerModel};

// ── ModelHeader: parse autônomo do v6 ──────────────────────────────────

/// Header parsed from a .bitnet v6 file — autonomo, zero hardcoded.
/// Reads all fields from the binary header at runtime.
#[derive(Debug, Clone, Copy)]
pub struct ModelHeader {
    pub hidden: usize,
    pub num_layers: usize,
    pub num_heads: usize,
    pub vocab: usize,
    pub max_seq: usize,
    pub intermediate: usize,
    pub kv_heads: usize,
    pub q_dim: usize,
    pub num_medusa: usize,
    pub tie: bool,
    pub feat: u8,
    pub embed_type: u8,
    pub file_size: usize,
}

impl ModelHeader {
    /// Params estimados: embed + unembed + per_layer * num_layers.
    pub fn estimated_params(&self) -> u64 {
        let embed = (self.hidden * self.vocab) as u64;
        let kv_dim = self.kv_heads * (self.q_dim / self.num_heads.max(1));
        let ffn_group = self.intermediate * self.q_dim / self.hidden.max(1);
        let per_layer = (self.hidden * self.q_dim       // q
            + self.hidden * kv_dim * 2                   // k + v
            + self.q_dim * self.hidden                   // o
            + self.hidden * ffn_group * 2                 // gate + up
            + self.intermediate * self.q_dim) as u64;    // down
        let unembed = if !self.tie { embed } else { 0 };
        embed + per_layer * self.num_layers as u64 + unembed
    }

    /// Tamanho em MB (arredondado).
    pub fn file_size_mb(&self) -> u64 {
        (self.file_size as u64 / (1024 * 1024)).max(1)
    }
}

/// Parse the v6 header without loading the full model — lightweight.
/// Returns None if data is too short or not v6 LLM.
pub fn parse_model_header(data: &[u8]) -> Option<ModelHeader> {
    if data.len() < 52 { return None; }
    let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    if magic != 0xBE11BE11 { return None; }
    let version = u16::from_le_bytes([data[4], data[5]]);
    if version != 6 || data[14] != 0 { return None; } // LLM only

    let hidden = u16::from_le_bytes([data[18], data[19]]) as usize;
    let num_layers = u16::from_le_bytes([data[20], data[21]]) as usize;
    let num_heads = u16::from_le_bytes([data[22], data[23]]) as usize;
    let vocab = u32::from_le_bytes([data[24], data[25], data[26], data[27]]) as usize;
    let max_seq = u16::from_le_bytes([data[28], data[29]]) as usize;
    let intermediate = u16::from_le_bytes([data[30], data[31]]) as usize;
    let kv_heads = u16::from_le_bytes([data[32], data[33]]) as usize;
    let q_dim = u16::from_le_bytes([data[34], data[35]]) as usize;
    let num_medusa = u32::from_le_bytes([data[36], data[37], data[38], data[39]]) as usize;
    let tie = &data[40..44] == b"TIED";
    let tok_len = u32::from_le_bytes([data[45], data[46], data[47], data[48]]) as usize;
    let hdr_end = 49 + tok_len + 3;
    if hdr_end > data.len() { return None; }
    let embed_type = data[49 + tok_len + 1];
    let feat = data[49 + tok_len + 2];

    // Calculate file size from header (same logic as the old v6_file_size)
    let kv_head_dim = q_dim / num_heads.max(1);
    let k_dim = kv_heads * kv_head_dim;
    let ffn_group = intermediate * q_dim / hidden.max(1);
    let down_out = q_dim;
    let mut size = hdr_end;
    size += match embed_type {
        0 => (hidden * vocab + 3) / 4 + 4,
        1 => ((hidden * vocab + 255) / 256) * 210 + 4,
        _ => hidden * vocab * 2 + 4,
    };
    let per_layer_norms = hidden * 4 * 2
        + if feat & 1 != 0 { hidden * 4 } else { 0 }
        + if feat & 2 != 0 { intermediate * 4 } else { 0 };
    let per_layer_tern = (hidden * q_dim + 3) / 4
        + 2 * ((hidden * k_dim + 3) / 4)
        + (q_dim * hidden + 3) / 4
        + 2 * ((hidden * ffn_group + 3) / 4)
        + (intermediate * down_out + 3) / 4
        + 7 * 4;
    size += num_layers * (per_layer_norms + per_layer_tern);
    size += hidden * 4; // rms_final
    if !tie { size += (hidden * vocab + 3) / 4 + 4; }
    size += num_medusa * ((hidden * vocab + 3) / 4 + 4);
    if feat & 4 != 0 { size += 4; }

    Some(ModelHeader { hidden, num_layers, num_heads, vocab, max_seq, intermediate,
        kv_heads, q_dim, num_medusa, tie, feat, embed_type, file_size: size })
}

// ── Static: header do modelo carregado ─────────────────────────────────

static HEADER_LOADED: AtomicBool = AtomicBool::new(false);
static mut LOADED_HEADER: ModelHeader = ModelHeader {
    hidden: 0, num_layers: 0, num_heads: 0, vocab: 0, max_seq: 0,
    intermediate: 0, kv_heads: 0, q_dim: 0, num_medusa: 0,
    tie: false, feat: 0, embed_type: 0, file_size: 0,
};

/// Store the header of the loaded model (called from set_model or load path).
pub fn set_model_header(h: ModelHeader) {
    unsafe { LOADED_HEADER = h; }
    HEADER_LOADED.store(true, Ordering::Release);
}

/// Get the header of the currently loaded model, if any.
pub fn loaded_model_header() -> Option<ModelHeader> {
    if HEADER_LOADED.load(Ordering::Acquire) {
        unsafe { Some(LOADED_HEADER) }
    } else {
        None
    }
}

/// Human-readable model info from header (zero hardcoded). Used by n3_cortex_gate.
pub fn model_info() -> String {
    if let Some(h) = loaded_model_header() {
        alloc::format!(
            "Falcon3 hidden={} layers={} heads={} kv={} vocab={} intermediate={} max_seq={} file={}MB",
            h.hidden, h.num_layers, h.num_heads, h.kv_heads, h.vocab, h.intermediate, h.max_seq, h.file_size_mb()
        )
    } else {
        String::from("no model loaded")
    }
}

// ── Tipos de modelo ────────────────────────────────────────────────────

/// Tipos de modelo reconhecidos (ADR-0085 S2: model_type).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ModelKind {
    Llm = 0,
    HwExpert = 1,
}

/// Visão de um modelo carregado — despacho por model_type do header v6.
pub enum ModelView {
    Llm(TransformerModel),
    HwExpert(HwExpertV4Model),
}

impl ModelView {
    pub fn kind(&self) -> ModelKind {
        match self {
            ModelView::Llm(_) => ModelKind::Llm,
            ModelView::HwExpert(_) => ModelKind::HwExpert,
        }
    }

    pub fn as_llm(&self) -> Option<&TransformerModel> {
        match self {
            ModelView::Llm(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_hwexpert(&self) -> Option<&HwExpertV4Model> {
        match self {
            ModelView::HwExpert(m) => Some(m),
            _ => None,
        }
    }
}

/// Carrega modelo .bitnet v6 (estrito) com fallback legado v3/v4/v5 (WARN).
pub fn load_model_v6(data: &[u8]) -> Option<ModelView> {
    if data.len() < 16 {
        return None;
    }
    let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    if magic != 0xBE11BE11 {
        return None;
    }
    let version = u16::from_le_bytes([data[4], data[5]]);
    match version {
        6 => match data[14] {
            0 => crate::cortex::load_model_v6(data).map(ModelView::Llm),
            1 => crate::cortex::load_hwexpert_v6(data).map(ModelView::HwExpert),
            _ => {
                k_nano::slog_cortex!("MODEL", "warn", "v6 model_type={} desconhecido", data[14]);
                None
            }
        },
        3..=5 => {
            k_nano::slog_cortex!("MODEL", "warn",
                "legacy v{} format — use migrate_bitnet_v6.py", version);
            crate::cortex::load_model(data).map(ModelView::Llm)
        }
        _ => None,
    }
}

/// Tamanho total do arquivo .bitnet v6 LLM a partir do header (autodescritivo).
/// Delega a parse_model_header — zero hardcoded.
pub fn v6_file_size(data: &[u8]) -> Option<usize> {
    parse_model_header(data).map(|h| h.file_size)
}
