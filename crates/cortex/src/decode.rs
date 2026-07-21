//! ADR-0057 WS-G (#412) — Structured decoding (grammar/JSON/FSM constrained).
//!
//! Restringe os logits a um conjunto de tokens permitidos **antes** do
//! argmax/sample. Default: sem máscara → **comportamento idêntico** ao
//! `argmax_row` (zero regressão). O Hermes pode instalar uma máscara por
//! passo para forçar saída estruturada (ex.: JSON), depois `clear_allow_mask`.
//!
//! Nota de validação: as demais otimizações WS-G que alteram a numérica do
//! forward (Medusa #140, FlashAttention #414, PagedAttention #413, huge pages
//! #92/#93, burn-flex #333, codebook VQ #169/#170) exigem **validação com
//! modelo carregado + geração** (não disponível no VM de setup sem modelos) e
//! ficam como residual WS-G no ADR-0057/TODO — não implementadas "às cegas".

use crate::tensor::Tensor;
use alloc::vec::Vec;
use spin::Mutex;

/// Máscara de tokens permitidos. `None` = sem restrição (identidade).
static ALLOW_MASK: Mutex<Option<Vec<bool>>> = Mutex::new(None);

/// Instala a máscara de permitidos para o próximo passo de decode.
pub fn set_allow_mask(mask: Vec<bool>) {
    *ALLOW_MASK.lock() = Some(mask);
}

/// Remove a restrição (volta ao decode livre).
pub fn clear_allow_mask() {
    *ALLOW_MASK.lock() = None;
}

pub fn is_constrained() -> bool {
    ALLOW_MASK.lock().is_some()
}

/// argmax do `row` respeitando a máscara de permitidos (se houver).
/// Sem máscara: equivalente a `argmax_row` (mesmo desempate = primeiro máximo).
pub fn argmax_constrained(logits: &Tensor, row: usize) -> u16 {
    let cols = logits.shape.1;
    let start = row * cols;
    let guard = ALLOW_MASK.lock();
    let mut best = 0u16;
    let mut best_val = f32::NEG_INFINITY;
    let mut found = false;
    for j in 0..cols {
        if let Some(ref m) = *guard {
            // Token bloqueado pela gramática → ignora.
            if j >= m.len() || !m[j] {
                continue;
            }
        }
        let v = logits.data[start + j];
        if !found || v > best_val {
            best_val = v;
            best = j as u16;
            found = true;
        }
    }
    best
}

/// Self-test determinístico (sem modelo): prova a equivalência no-op e a
/// restrição. Retorna true se PASS. Chamado uma vez no boot (evidência WS-G).
pub fn self_test() -> bool {
    // logits = [0.1, 0.9, 0.5, 0.3] → argmax livre = 1
    let t = match Tensor::from_row_major((1, 4), alloc::vec![0.1f32, 0.9, 0.5, 0.3]) {
        Some(t) => t,
        None => return false,
    };
    clear_allow_mask();
    let free = argmax_constrained(&t, 0);
    // Restringe a {0, 2}: melhor permitido = 2 (0.5 > 0.1).
    set_allow_mask(alloc::vec![true, false, true, false]);
    let constrained = argmax_constrained(&t, 0);
    clear_allow_mask();
    let ok = free == 1 && constrained == 2;
    if ok {
        k_nano::slog_cortex!("DECODE", "info", "structured-decode self-test PASS (free=1 constrained=2)");
    } else {
        k_nano::slog_cortex!(
            "DECODE",
            "warn",
            "structured-decode self-test FAIL (free={} constrained={})",
            free,
            constrained
        );
    }
    ok
}
