//! cortex::model — registro mínimo de modelos .bitnet v6 (ADR-0085 §7).
//! Fronteira de formato única: load_model_v6 despacha por model_type para
//! LLM / HWExpert. Router é formato posicional próprio (trinity) — separado.
//! ModelHub usa `register_bytes` como ponto único.

use alloc::string::String;
use alloc::vec::Vec;

use crate::cortex::{HwExpertV4Model, TransformerModel};

/// Tipos de modelo reconhecidos (ADR-0085 §2: model_type).
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
/// Returns None em erro de parse ou versão desconhecida.
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
            // LLM v6: cortex::load_model_v6 (já despacha v6 estrito + legado)
            0 => crate::cortex::load_model_v6(data).map(ModelView::Llm),
            // HWExpert v6: loader próprio pendente; v5 legado via load_hwexpert_v5
            1 => {
                k_nano::slog_cortex!("MODEL", "warn",
                    "v6 hwexpert loader pendente (F1b) — tentando legado v5");
                crate::cortex::load_hwexpert_v5(data).map(ModelView::HwExpert)
            }
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
/// Usado pelo QEMU-loader para dimensionar o slice sem hardcode de tamanho
/// (o const v4 604MB truncava o 2B v6 de 792MB → OOM #PF). Returns None se
/// não for v6 LLM válido.
pub fn v6_file_size(data: &[u8]) -> Option<usize> {
    if data.len() < 52 {
        return None;
    }
    let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    if magic != 0xBE11BE11 {
        return None;
    }
    let version = u16::from_le_bytes([data[4], data[5]]);
    if version != 6 || data[14] != 0 {
        return None; // só LLM v6 (model_type=0)
    }
    let hidden = u16::from_le_bytes([data[18], data[19]]) as usize;
    let num_layers = u16::from_le_bytes([data[20], data[21]]) as usize;
    let num_heads = u16::from_le_bytes([data[22], data[23]]) as usize;
    let vocab = u32::from_le_bytes([data[24], data[25], data[26], data[27]]) as usize;
    let intermediate = u16::from_le_bytes([data[30], data[31]]) as usize;
    let kv_heads = u16::from_le_bytes([data[32], data[33]]) as usize;
    let q_dim = u16::from_le_bytes([data[34], data[35]]) as usize;
    let num_medusa = u32::from_le_bytes([data[36], data[37], data[38], data[39]]) as usize;
    let tie = &data[40..44] == b"TIED";
    let tok_len = u32::from_le_bytes([data[45], data[46], data[47], data[48]]) as usize;
    let hdr_end = 49 + tok_len + 3; // + act_type, embed_type, feat
    if hdr_end > data.len() {
        return None;
    }
    let embed_type = data[49 + tok_len + 1];
    let feat = data[49 + tok_len + 2];

    let kv_head_dim = q_dim / num_heads.max(1);
    let k_dim = kv_heads * kv_head_dim;
    let ffn_group = intermediate * q_dim / hidden.max(1);
    let down_out = q_dim;

    let mut size = hdr_end;

    // embed
    size += match embed_type {
        0 => (hidden * vocab + 3) / 4 + 4,          // ternary + scale
        1 => ((hidden * vocab + 255) / 256) * 210 + 4, // Q6_K + scale
        _ => hidden * vocab * 2 + 4,                // BF16 + scale
    };

    // layers
    let per_layer_norms = hidden * 4 * 2 // rms_attn + rms_ffn
        + if feat & 1 != 0 { hidden * 4 } else { 0 }
        + if feat & 2 != 0 { intermediate * 4 } else { 0 };
    let per_layer_tern = (hidden * q_dim + 3) / 4 // q
        + 2 * ((hidden * k_dim + 3) / 4)           // k + v
        + (q_dim * hidden + 3) / 4                 // o
        + 2 * ((hidden * ffn_group + 3) / 4)       // gate + up
        + (intermediate * down_out + 3) / 4        // down
        + 7 * 4;                                   // 7 scales
    size += num_layers * (per_layer_norms + per_layer_tern);

    // rms_final
    size += hidden * 4;

    // unembed (só se não tied)
    if !tie {
        size += (hidden * vocab + 3) / 4 + 4;
    }

    // medusa
    size += num_medusa * ((hidden * vocab + 3) / 4 + 4);

    // theta (feat bit2)
    if feat & 4 != 0 {
        size += 4;
    }

    Some(size)
}
