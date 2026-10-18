//! Composição multi-função (pipelines) sobre o ModelHub + Trinity.
//! Modo B do plano Hub MoE: cadeias curtas fail-closed com slog por etapa.
//! DiskDiag/Security ficam skill-backed (sem nicho novo).

use alloc::string::String;
use core::sync::atomic::{AtomicU8, Ordering};

use crate::model_hub::{self, ModelSlot};

/// Telemetria HUD (lock-free): último route/pipe/fallback.
/// 0 = nenhum / n/a (nunca tratar como sucesso silencioso).
static LAST_ROUTE_KIND: AtomicU8 = AtomicU8::new(0);
static LAST_ROUTE_SLOT: AtomicU8 = AtomicU8::new(0);
static LAST_PIPE_STEPS: AtomicU8 = AtomicU8::new(0);
static LAST_FALLBACK: AtomicU8 = AtomicU8::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PipelineId {
    VisionQa = 1,
    Rag = 2,
    Heal = 3,
    Agentic = 4,
}

impl PipelineId {
    pub fn name(self) -> &'static str {
        match self {
            Self::VisionQa => "vision_qa",
            Self::Rag => "rag",
            Self::Heal => "heal",
            Self::Agentic => "agentic_pipe",
        }
    }
}

pub fn pipeline_from_name(name: &str) -> Option<PipelineId> {
    match name {
        "vision_qa" | "visionqa" | "vlm" => Some(PipelineId::VisionQa),
        "rag" | "retrieve" | "rerank_gen" => Some(PipelineId::Rag),
        "heal" | "self_heal" | "heal_code" => Some(PipelineId::Heal),
        "agentic_pipe" | "agent_pipe" => Some(PipelineId::Agentic),
        _ => None,
    }
}

/// Snapshot telemetria: (kind_tag, slot_tag, pipe_steps, fallback_tag). 0 = n/a.
pub fn telemetry() -> (u8, u8, u8, u8) {
    (
        LAST_ROUTE_KIND.load(Ordering::Relaxed),
        LAST_ROUTE_SLOT.load(Ordering::Relaxed),
        LAST_PIPE_STEPS.load(Ordering::Relaxed),
        LAST_FALLBACK.load(Ordering::Relaxed),
    )
}

fn tag_kind(name: &str) -> u8 {
    match name {
        "hw_identify" => 1,
        "rust_coder" => 2,
        "agent" | "orchestrator" | "agentic" => 3,
        "generator" | "generator_pro" | "generator_fast" | "tinystories" => 4,
        "disk_diag" => 5,
        "security" => 6,
        "speech_synth" => 7,
        "vision_qa" | "rag" | "heal" | "agentic_pipe" => 8,
        _ => 9,
    }
}

fn tag_slot(name: &str) -> u8 {
    match name {
        "active" => 1,
        "vision" => 2,
        "generator_pro" => 3,
        "reranker" => 4,
        "rust_coder" => 5,
        "hw_identify" => 6,
        "learner" => 7,
        "agent" => 8,
        "piper" => 9,
        "airllm" => 10, // degradacao adaptativa pos-MoE (ADR-0088), nao bypass
        _ => 0,
    }
}

fn tag_fallback(reason: &str) -> u8 {
    match reason {
        "direct" | "generator_select" => 0,
        "niche_miss" => 1,
        "pro_miss" => 2,
        "skill_backed" => 3,
        "outside_hub" => 4,
        "pipe_degrade" => 5,
        _ => 6,
    }
}

/// Registra última rota (kind → slot, motivo). Usado por `dispatch_expert`.
pub fn note_route(kind: &str, slot: &str, reason: &str) {
    LAST_ROUTE_KIND.store(tag_kind(kind), Ordering::Relaxed);
    LAST_ROUTE_SLOT.store(tag_slot(slot), Ordering::Relaxed);
    LAST_FALLBACK.store(tag_fallback(reason), Ordering::Relaxed);
    k_nano::slog_cortex!(
        "MOE",
        "ok",
        "route={}→{} reason={} (tele kind={} slot={} fb={})",
        kind,
        slot,
        reason,
        tag_kind(kind),
        tag_slot(slot),
        tag_fallback(reason)
    );
}

fn note_pipe(id: PipelineId, steps: u8, degrade: bool) {
    LAST_PIPE_STEPS.store(steps, Ordering::Relaxed);
    LAST_ROUTE_KIND.store(tag_kind(id.name()), Ordering::Relaxed);
    if degrade {
        LAST_FALLBACK.store(tag_fallback("pipe_degrade"), Ordering::Relaxed);
    } else {
        LAST_FALLBACK.store(0, Ordering::Relaxed);
    }
    if degrade {
        k_nano::slog_cortex!("MOE", "warn", "pipe={} steps={} degrade=1", id.name(), steps);
    } else {
        k_nano::slog_cortex!("MOE", "ok", "pipe={} steps={} degrade=0", id.name(), steps);
    }
}

/// Executa um pipeline curto. Cada etapa: `ensure_slot_loaded` + fallback honesto.
pub fn compose_pipeline(id: PipelineId, prompt: &str) -> String {
    match id {
        PipelineId::VisionQa => pipe_vision_qa(prompt),
        PipelineId::Rag => pipe_rag(prompt),
        PipelineId::Heal => pipe_heal(prompt),
        PipelineId::Agentic => pipe_agentic(prompt),
    }
}

fn pipe_vision_qa(prompt: &str) -> String {
    // Vision encode (se blob) → Active/Pro gera. Vision ≠ gerador (honesty s388).
    let mut steps = 0u8;
    let mut ctx = String::new();
    if model_hub::ensure_slot_loaded(ModelSlot::Vision) && model_hub::slot_real(ModelSlot::Vision) {
        steps += 1;
        ctx.push_str("[vision:encoder_ready] ");
        k_nano::slog_cortex!("MOE", "ok", "pipe=vision_qa step=vision ok");
    } else {
        k_nano::slog_cortex!("MOE", "warn", "pipe=vision_qa step=vision miss");
    }
    let gen_prompt = alloc::format!("{}{}", ctx, prompt);
    let slot = model_hub::select_generator_slot(prompt);
    steps += 1;
    if let Some(out) = model_hub::generate_from_slot(slot, &gen_prompt) {
        note_pipe(PipelineId::VisionQa, steps, ctx.is_empty());
        note_route("vision_qa", slot.name(), if ctx.is_empty() { "pipe_degrade" } else { "direct" });
        return out;
    }
    note_pipe(PipelineId::VisionQa, steps, true);
    note_route("vision_qa", "active", "niche_miss");
    degrade_active(prompt)
}

fn pipe_rag(prompt: &str) -> String {
    // BGE fora do hub; Reranker → Active. Sem BGE wire aqui = degrade honesto.
    let mut steps = 0u8;
    let mut ctx = String::from("[rag:embed=bge_external_or_pseudo] ");
    steps += 1;
    if model_hub::ensure_slot_loaded(ModelSlot::Reranker) && model_hub::slot_real(ModelSlot::Reranker)
    {
        steps += 1;
        ctx.push_str("[reranker:ready] ");
        k_nano::slog_cortex!("MOE", "ok", "pipe=rag step=reranker ok");
    } else {
        k_nano::slog_cortex!("MOE", "warn", "pipe=rag step=reranker miss");
    }
    let gen_prompt = alloc::format!("{}{}", ctx, prompt);
    steps += 1;
    if let Some(out) = model_hub::generate_from_slot(ModelSlot::Active, &gen_prompt) {
        let degrade = !model_hub::slot_real(ModelSlot::Reranker);
        note_pipe(PipelineId::Rag, steps, degrade);
        note_route("rag", "active", if degrade { "pipe_degrade" } else { "direct" });
        return out;
    }
    note_pipe(PipelineId::Rag, steps, true);
    note_route("rag", "active", "niche_miss");
    degrade_active(prompt)
}

fn pipe_heal(prompt: &str) -> String {
    // HwIdentify (opcional) → RustCoder patch → (opcional) Pro review.
    let mut steps = 0u8;
    let mut ctx = String::new();
    if let Some(hw) = model_hub::generate_from_slot(
        ModelSlot::HwExpert,
        &alloc::format!("identifique hardware {}", prompt),
    ) {
        steps += 1;
        ctx.push_str(&hw);
        ctx.push(' ');
        k_nano::slog_cortex!("MOE", "ok", "pipe=heal step=hw_identify ok");
    } else {
        k_nano::slog_cortex!("MOE", "warn", "pipe=heal step=hw_identify miss");
    }
    let code_prompt = alloc::format!(
        "{{\"role\":\"system\",\"content\":\"Gere apenas codigo Rust valido (heal/patch).\"}}\n{}{}\n",
        ctx,
        prompt
    );
    if let Some(code) = model_hub::generate_from_slot(ModelSlot::RustCoder, &code_prompt) {
        steps += 1;
        k_nano::slog_cortex!("MOE", "ok", "pipe=heal step=rust_coder ok");
        // Review opcional via Pro (LOO); miss = devolve o patch.
        if model_hub::is_complex_conversation(prompt) {
            let review = alloc::format!("revise o patch:\n{}\ncontexto: {}", code, prompt);
            if let Some(rev) = model_hub::generate_from_slot(ModelSlot::GeneratorPro, &review) {
                steps += 1;
                note_pipe(PipelineId::Heal, steps, false);
                note_route("heal", "generator_pro", "direct");
                return rev;
            }
            k_nano::slog_cortex!("MOE", "warn", "pipe=heal step=pro miss (keep rust_coder)");
        }
        note_pipe(PipelineId::Heal, steps, ctx.is_empty());
        note_route("heal", "rust_coder", if ctx.is_empty() { "pipe_degrade" } else { "direct" });
        return code;
    }
    k_nano::slog_cortex!("MOE", "warn", "pipe=heal step=rust_coder miss → Active");
    note_pipe(PipelineId::Heal, steps, true);
    note_route("heal", "active", "niche_miss");
    degrade_active(prompt)
}

fn pipe_agentic(prompt: &str) -> String {
    // Agent planeja → RustCoder/Active executa → Agent verifica (best-effort).
    let mut steps = 0u8;
    let plan = match model_hub::generate_from_slot(ModelSlot::Agent, prompt) {
        Some(p) => {
            steps += 1;
            k_nano::slog_cortex!("MOE", "ok", "pipe=agentic step=plan ok");
            p
        }
        None => {
            k_nano::slog_cortex!("MOE", "warn", "pipe=agentic step=plan miss → Active");
            note_pipe(PipelineId::Agentic, 0, true);
            note_route("agentic_pipe", "active", "niche_miss");
            return degrade_active(prompt);
        }
    };
    let exec_prompt = alloc::format!("execute o plano:\n{}\npedido: {}", plan, prompt);
    let exec = if let Some(c) = model_hub::generate_from_slot(ModelSlot::RustCoder, &exec_prompt) {
        steps += 1;
        k_nano::slog_cortex!("MOE", "ok", "pipe=agentic step=rust_coder ok");
        c
    } else if let Some(a) = model_hub::generate_from_slot(ModelSlot::Active, &exec_prompt) {
        steps += 1;
        k_nano::slog_cortex!("MOE", "warn", "pipe=agentic step=exec via Active");
        a
    } else {
        note_pipe(PipelineId::Agentic, steps, true);
        note_route("agentic_pipe", "agent", "pipe_degrade");
        return plan;
    };
    let verify_prompt = alloc::format!("verifique:\nplano:\n{}\nresultado:\n{}", plan, exec);
    if let Some(v) = model_hub::generate_from_slot(ModelSlot::Agent, &verify_prompt) {
        steps += 1;
        note_pipe(PipelineId::Agentic, steps, false);
        note_route("agentic_pipe", "agent", "direct");
        return v;
    }
    note_pipe(PipelineId::Agentic, steps, true);
    note_route("agentic_pipe", "agent", "pipe_degrade");
    exec
}

fn degrade_active(prompt: &str) -> String {
    if let Some(out) = model_hub::generate_from_slot(ModelSlot::Active, prompt) {
        out
    } else {
        String::from("[MOE] niche miss e Active vazio")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_names_roundtrip() {
        assert_eq!(pipeline_from_name("vision_qa"), Some(PipelineId::VisionQa));
        assert_eq!(pipeline_from_name("rag"), Some(PipelineId::Rag));
        assert_eq!(pipeline_from_name("heal"), Some(PipelineId::Heal));
        assert_eq!(pipeline_from_name("agentic_pipe"), Some(PipelineId::Agentic));
        assert!(pipeline_from_name("generator").is_none());
    }

    #[test]
    fn note_route_updates_telemetry() {
        note_route("rust_coder", "rust_coder", "direct");
        let (k, s, _p, f) = telemetry();
        assert_eq!(k, 2);
        assert_eq!(s, 5);
        assert_eq!(f, 0);
        note_route("disk_diag", "active", "skill_backed");
        let (k2, s2, _, f2) = telemetry();
        assert_eq!(k2, 5);
        assert_eq!(s2, 1);
        assert_eq!(f2, 3);
    }
}
