//! ADR-0101 Onda 3 — composição Cognitive Runtime (gate + spec + kernel + H2O + SGDB seam).
//!
//! Não duplica generate: orquestra política à volta de `generate_speculative`.
//! SGDB continua **fora** do matmul (prompt/recall via hermes cognitive_bridge).

use alloc::string::String;

use crate::cortex::{generate_speculative, TransformerModel};
use crate::difficulty_gate::{self, ComputeTier};
use crate::structured_decode::StructuredDecoder;

/// Pacote de telemetria pós-generate (Onda 3 observabilidade).
#[derive(Clone, Debug)]
pub struct CognitiveReport {
    pub tier: ComputeTier,
    pub soft_stride: usize,
    pub max_gen: usize,
    pub shortlist_hits: u64,
    pub full_unembeds: u64,
    pub h2o_drops: u64,
}

static LAST_H2O_DROPS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

pub fn note_h2o_drops(n: usize) {
    LAST_H2O_DROPS.fetch_add(n as u64, core::sync::atomic::Ordering::Relaxed);
}

pub fn take_h2o_drops() -> u64 {
    LAST_H2O_DROPS.swap(0, core::sync::atomic::Ordering::Relaxed)
}

/// Entrada canónica Onda 3: classifica tier → aplica stride → generate → limpa override.
pub fn generate_with_policy(
    model: &TransformerModel,
    prompt: &str,
    decoder: Option<&mut StructuredDecoder>,
) -> (String, CognitiveReport) {
    let use_bpe = crate::bpe::is_loaded();
    let is_greeting = crate::bpe::prompt_is_greeting(prompt);
    let tier = difficulty_gate::classify(prompt, is_greeting, model.hidden);
    let stride = difficulty_gate::soft_stride_for(tier, model.hidden);
    let max_gen = difficulty_gate::max_gen_for(tier, model.hidden, use_bpe, is_greeting);
    difficulty_gate::apply_tier(tier, model.hidden);

    k_nano::slog_cortex!(
        "CogRT",
        "ok",
        "Onda3 generate tier={} stride={} max_gen_hint={}",
        tier.name(),
        stride,
        max_gen
    );

    let text = generate_speculative(model, prompt, decoder);

    difficulty_gate::clear_soft_stride_override();
    let (sl_hits, full_u) = crate::vocab_shortlist::telemetry();
    let report = CognitiveReport {
        tier,
        soft_stride: stride,
        max_gen,
        shortlist_hits: sl_hits,
        full_unembeds: full_u,
        h2o_drops: take_h2o_drops(),
    };
    k_nano::slog_cortex!(
        "CogRT",
        "ok",
        "done tier={} shortlist={} full_unembed={} h2o_drops={}",
        report.tier.name(),
        report.shortlist_hits,
        report.full_unembeds,
        report.h2o_drops
    );
    (text, report)
}

/// Status one-liner p/ `/cog` / boot.
pub fn status_line() -> String {
    let (sl, fu) = crate::vocab_shortlist::telemetry();
    alloc::format!(
        "CogRT ADR-0101: shortlist_hits={} full_unembed={} h2o_drops_session={} (SGDB=prompt-side)",
        sl,
        fu,
        LAST_H2O_DROPS.load(core::sync::atomic::Ordering::Relaxed)
    )
}
