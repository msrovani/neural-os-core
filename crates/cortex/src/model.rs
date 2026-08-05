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
