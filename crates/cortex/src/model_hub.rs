//! ModelHub (bin) — slots .bitnet multi-modelo; truth do boot (ADR multi-model).

use alloc::boxed::Box;
use alloc::string::String;
use core::sync::atomic::{AtomicU8, Ordering};
use spin::Mutex;

use crate::cortex::Model;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ModelSlot {
    Active = 0,
    Vision = 1,
    GeneratorPro = 2,
    Reranker = 3,
    RustCoder = 4,
    HwExpert = 5,
    Learner = 6,
    Agent = 7,
}

impl ModelSlot {
    pub fn name(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Vision => "vision",
            Self::GeneratorPro => "generator_pro",
            Self::Reranker => "reranker",
            Self::RustCoder => "rust_coder",
            Self::HwExpert => "hw_identify",
            Self::Learner => "learner",
            Self::Agent => "agent",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "active" | "current" | "generator" => Some(Self::Active),
            "vision" | "siglip" | "vit" | "encoder" => {
                Some(Self::Vision)
            }
            "generator_pro" | "pro" | "3b" | "bitnet3b" => Some(Self::GeneratorPro),
            "reranker" | "rerank" | "cross_encoder" | "bge_reranker" => Some(Self::Reranker),
            "rust_coder" | "rustcoder" => Some(Self::RustCoder),
            "hw_identify" | "hwexpert" => Some(Self::HwExpert),
            "learner" | "qwen05" | "qwen0.5b" => Some(Self::Learner),
            "agent" | "qwen3b" | "agentic" | "orchestrator" => Some(Self::Agent),
            _ => None,
        }
    }
}

const N_SLOTS: usize = 8;

struct HubInner {
    slots: [Option<Box<dyn Model>>; N_SLOTS],
}

static HUB: Mutex<HubInner> = Mutex::new(HubInner {
    slots: [None, None, None, None, None, None, None, None],
});
static SLOT_MASK: AtomicU8 = AtomicU8::new(0);

fn idx(slot: ModelSlot) -> usize {
    slot as usize
}

fn mark(slot: ModelSlot, filled: bool) {
    let bit = 1u8 << (slot as u8);
    if filled {
        SLOT_MASK.fetch_or(bit, Ordering::Release);
    } else {
        SLOT_MASK.fetch_and(!bit, Ordering::Release);
    }
}

pub fn slot_loaded(slot: ModelSlot) -> bool {
    (SLOT_MASK.load(Ordering::Acquire) & (1u8 << (slot as u8))) != 0
}

pub fn register_model(slot: ModelSlot, model: Box<dyn Model>) {
    // Active / RustCoder / HwExpert vivem nos Mutex legados — só marca flag.
    if matches!(
        slot,
        ModelSlot::Active | ModelSlot::RustCoder | ModelSlot::HwExpert
    ) {
        mark(slot, true);
        k_nano::slog_bin!("MODEL", "info", "hub mark slot={}", slot.name());
        return;
    }
    let i = idx(slot);
    HUB.lock().slots[i] = Some(model);
    mark(slot, true);
    k_nano::slog_bin!("MODEL", "info", "hub slot={} loaded", slot.name());
}

/// Ponto único de carga por bytes (ADR-0085 §7): load_model_v6 → ModelView
/// → armazenamento por kind. Active/RustCoder/HwExpert usam os Mutex legados.
pub fn register_bytes(slot: ModelSlot, data: &[u8]) -> bool {
    let view = match crate::model::load_model_v6(data) {
        Some(v) => v,
        None => {
            k_nano::slog_bin!("MODEL", "warn", "register_bytes: parse falhou slot={}", slot.name());
            return false;
        }
    };
    match view {
        crate::model::ModelView::Llm(m) => {
            let boxed: Box<dyn Model> = Box::new(m);
            if matches!(
                slot,
                ModelSlot::Active | ModelSlot::RustCoder | ModelSlot::HwExpert
            ) {
                match slot {
                    ModelSlot::Active => *crate::cortex::CURRENT_MODEL.lock() = Some(boxed),
                    ModelSlot::RustCoder => *crate::cortex::RUSTCODER_MODEL.lock() = Some(boxed),
                    _ => {}
                }
            } else {
                let i = idx(slot);
                HUB.lock().slots[i] = Some(boxed);
            }
            mark(slot, true);
            k_nano::slog_bin!("MODEL", "info", "register_bytes slot={} LLM v6 ok", slot.name());
            true
        }
        crate::model::ModelView::HwExpert(_m) => {
            // HWExpert v6: statics legados em k_ai (set_hwexpert_v4_model).
            mark(slot, true);
            k_nano::slog_bin!("MODEL", "info", "register_bytes slot={} HWExpert (legado v5)", slot.name());
            true
        }
    }
}

pub fn mark_active(filled: bool) {
    mark(ModelSlot::Active, filled);
}

pub fn mark_slot(slot: ModelSlot, filled: bool) {
    mark(slot, filled);
}

/// Active grande (2B/3B) também conta como Pro para select_generator_slot.
pub fn mark_pro_alias(filled: bool) {
    mark(ModelSlot::GeneratorPro, filled);
}

/// Clona referência lógica: move para slot sem dropar Active se for o mesmo ptr — use register.
pub fn generate_from_slot(slot: ModelSlot, prompt: &str) -> Option<String> {
    let hub = HUB.lock();
    hub.slots[idx(slot)].as_ref().map(|m| m.generate(prompt))
}

/// Generate with structured decoding from a specific slot.
/// Masks logits at each token step to enforce the output grammar.
pub fn generate_structured_from_slot(
    slot: ModelSlot,
    prompt: &str,
    grammar: crate::structured_decode::OutputGrammar,
) -> Option<String> {
    let mut dec = crate::structured_decode::StructuredDecoder::new(grammar.into());
    crate::cortex::DECODER_CELL.set(&mut dec as *mut crate::structured_decode::StructuredDecoder);
    let hub = HUB.lock();
    hub.slots[idx(slot)].as_ref().map(|m| m.generate(prompt))
}

pub fn is_complex_conversation(prompt: &str) -> bool {
    if prompt.len() > 160 {
        return true;
    }
    contains_ci(prompt, "detalhad")
        || contains_ci(prompt, "analis")
        || contains_ci(prompt, "explain")
        || contains_ci(prompt, "compare")
        || contains_ci(prompt, "porque")
        || contains_ci(prompt, "why ")
        || contains_ci(prompt, "architect")
        || (prompt.len() > 80 && (contains_ci(prompt, "como ") || contains_ci(prompt, "how ")))
}

fn contains_ci(hay: &str, needle: &str) -> bool {
    let h = hay.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() || h.len() < n.len() {
        return false;
    }
    'outer: for i in 0..=(h.len() - n.len()) {
        for j in 0..n.len() {
            if h[i + j].to_ascii_lowercase() != n[j].to_ascii_lowercase() {
                continue 'outer;
            }
        }
        return true;
    }
    false
}

fn hub_has_blob(slot: ModelSlot) -> bool {
    matches!(
        slot,
        ModelSlot::Vision
            | ModelSlot::GeneratorPro
            | ModelSlot::Reranker
            | ModelSlot::Agent
            | ModelSlot::Learner
    ) && HUB.lock().slots[idx(slot)].is_some()
}

fn fit_ok(slot: ModelSlot) -> bool {
    !crate::model_fit::slot_too_tight(slot.name())
}

fn pick_fit_fallback(preferred: ModelSlot) -> ModelSlot {
    let order = [
        ModelSlot::GeneratorPro,
        ModelSlot::Agent,
        ModelSlot::Vision,
        ModelSlot::Reranker,
        ModelSlot::Active,
    ];
    for s in order {
        let loaded = if s == ModelSlot::Active {
            slot_loaded(s)
        } else {
            hub_has_blob(s) || slot_loaded(s)
        };
        if loaded && fit_ok(s) {
            if s != preferred {
                k_nano::slog_bin!(
                    "FIT",
                    "info",
                    "escalate slot={} → {} reason=too_tight",
                    preferred.name(),
                    s.name()
                );
            }
            return s;
        }
    }
    k_nano::slog_bin!(
        "FIT",
        "info",
        "escalate slot={} reason=too_tight (no Good+ fallback)",
        preferred.name()
    );
    if hub_has_blob(ModelSlot::Vision) {
        ModelSlot::Vision
    } else {
        ModelSlot::Active
    }
}

fn maybe_fit(slot: ModelSlot) -> ModelSlot {
    if fit_ok(slot) {
        slot
    } else {
        pick_fit_fallback(slot)
    }
}

pub fn select_generator_slot(prompt: &str) -> ModelSlot {
    // Complexo: blob Pro separado → Pro; senão Active (2B/3B no CURRENT).
    if is_complex_conversation(prompt) {
        if hub_has_blob(ModelSlot::GeneratorPro) {
            return maybe_fit(ModelSlot::GeneratorPro);
        }
        if slot_loaded(ModelSlot::Active) {
            return maybe_fit(ModelSlot::Active);
        }
    }
    if slot_loaded(ModelSlot::Active) {
        return maybe_fit(ModelSlot::Active);
    }
    // Vision encoder não é gerador de texto — não entra no select_generator
    if hub_has_blob(ModelSlot::GeneratorPro) {
        return maybe_fit(ModelSlot::GeneratorPro);
    }
    if hub_has_blob(ModelSlot::Reranker) {
        return maybe_fit(ModelSlot::Reranker);
    }
    ModelSlot::Active
}

pub fn hub_status() -> String {
    let mut s = String::from("ModelHub:");
    for slot in [
        ModelSlot::Active,
        ModelSlot::Vision,
        ModelSlot::GeneratorPro,
        ModelSlot::Reranker,
        ModelSlot::RustCoder,
        ModelSlot::HwExpert,
        ModelSlot::Learner,
        ModelSlot::Agent,
    ] {
        s.push(' ');
        s.push_str(slot.name());
        s.push('=');
        s.push_str(if slot_loaded(slot) { "1" } else { "0" });
    }
    s
}

pub fn slot_from_bitnet_bytes(len: usize) -> ModelSlot {
    const MB: usize = 1024 * 1024;
    if len < 20 * MB {
        ModelSlot::Reranker
    } else if len < 200 * MB {
        ModelSlot::Learner
    } else if len < 450 * MB {
        ModelSlot::Vision
    } else if len < 1100 * MB {
        // Modelos <= 1100MB (medido via v6_file_size) → Agent
        ModelSlot::Agent
    } else {
        // Modelos > 1100MB (medido via v6_file_size) → GeneratorPro
        ModelSlot::GeneratorPro
    }
}

/// FAT 8.3 candidatos por slot (ordem de preferência).
pub fn fat_names_for(slot: ModelSlot) -> &'static [&'static str] {
    match slot {
        // target1 canônico (.v6 primeiro); fallback .BIN legado
        ModelSlot::Reranker => &["RERANKER.v6", "RERANKER.BIN", "RERANK.BITNET", "RERANK.BIN"],
        ModelSlot::Vision => &["VISION.v6", "VISION.BIN", "SIGLIP.BIN", "VIT.BIN"],
        ModelSlot::GeneratorPro => &[
            "PRO.v6",       // Modelo PRO (tamanho detectado do header v6)
            "PRO.BIN",      // Modelo PRO legado
            "FALCON7B.v6",  // alias alternativo
            "FALCON7B.BIN",
            "BITNET3B.BIN",
            "BITN3B.BIN",
            "LLAMA8B.BIN",
            "BITNET2B.BIN",
        ],
        ModelSlot::RustCoder => &["RUSTCDR3.v6", "RUSTCDR3.BIN", "RUSTCDR2.BIN", "RUSTCDR.BITNET", "RUSTCDR.BIN"],
        ModelSlot::HwExpert => &["HWEXPRT.v6", "HWEXPRT.BIN", "HWEXPERT.BIN", "HWEXPRT4.BIN", "HWEXPRT4.bin"],
        ModelSlot::Learner => &["LEARNER.v6", "LEARNER.BIN", "QWEEN05.BIN", "QWEN05B.BIN"],
        ModelSlot::Agent => &[
            "FALCON3.V6",
            "FALCON3.BIN",
            "AGENT.v6",
            "AGENT.BIN",
            "QWEN3B.BIN",
            "QWEN.BIN",
        ],
        ModelSlot::Active => &[
            // Falcon3 3B é o preset principal (hidden 3072 L22 H12 kv4 vocab 131072 silu rope 1000042 tie false)
            "FALCON3.V6",
            "FALCON3.BIN",
            "BITNET2B.v6",
            "BITNET2B.BIN",
            "BITNET13.BIN",
            "BITNET850.BIN",
            "BITNET3B.BIN",
            "BITNET.BIN",
            "MICRO.BITNET",
            "MICRO.BIN",
            "LLAMA8B.BIN",
        ],
    }
}
