//! Status LLM leve — o painel Hermes overlay foi removido (SESSION_261).
//!
//! Mantém um flag atômico para o HUD (barra de status) saber se o Cortex
//! está gerando. Sem buffer de texto legado.

use core::sync::atomic::{AtomicBool, Ordering};

static LLM_BUSY: AtomicBool = AtomicBool::new(false);

/// Marca se o Cortex está gerando (HUD). Também lê `cortex::infer_in_flight`.
pub fn set_llm_busy(busy: bool) {
    LLM_BUSY.store(busy, Ordering::Relaxed);
}

pub fn llm_busy() -> bool {
    LLM_BUSY.load(Ordering::Relaxed) || cortex::cortex::infer_in_flight()
}
