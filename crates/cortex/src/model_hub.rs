//! ModelHub — slots nomeados para múltiplos .bitnet sem quebrar CURRENT_MODEL.
//! TinyStories (smoke) · generator_fast (850M) · generator_pro (3B) · experts.

use alloc::boxed::Box;
use alloc::string::String;
use core::sync::atomic::{AtomicU8, Ordering};
use spin::Mutex;

use crate::cortex::Model;

/// Slot lógico de modelo BitNet / expert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ModelSlot {
    /// Alias do ativo (compat `set_model` / CURRENT_MODEL).
    Active = 0,
    /// Fallback rápido (~850M ou MICRO).
    GeneratorFast = 1,
    /// Generator "pro" (BitNet b1.58 3B / 2B grande).
    GeneratorPro = 2,
    /// Dev/test/smoke TinyStories 1M–15M.
    TinyStories = 3,
    /// Expert código (RustCoder 2B/3B substitui o pequeno).
    RustCoder = 4,
    /// Expert HW identify.
    HwExpert = 5,
}

impl ModelSlot {
    pub fn name(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::GeneratorFast => "generator_fast",
            Self::GeneratorPro => "generator_pro",
            Self::TinyStories => "tinystories",
            Self::RustCoder => "rust_coder",
            Self::HwExpert => "hw_identify",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "active" | "current" | "generator" => Some(Self::Active),
            "generator_fast" | "fast" | "850m" | "bitnet850" | "1p3" | "1.3b" | "xl" => {
                Some(Self::GeneratorFast)
            }
            "generator_pro" | "pro" | "3b" | "bitnet3b" => Some(Self::GeneratorPro),
            "tinystories" | "tiny" | "smoke" => Some(Self::TinyStories),
            "rust_coder" | "rustcoder" => Some(Self::RustCoder),
            "hw_identify" | "hwexpert" => Some(Self::HwExpert),
            _ => None,
        }
    }
}

const N_SLOTS: usize = 6;

struct HubInner {
    slots: [Option<Box<dyn Model>>; N_SLOTS],
}

static HUB: Mutex<HubInner> = Mutex::new(HubInner {
    slots: [None, None, None, None, None, None],
});

/// Máscara de slots ocupados (bit i = slot i).
static SLOT_MASK: AtomicU8 = AtomicU8::new(0);

fn idx(slot: ModelSlot) -> usize {
    slot as usize
}

pub fn mark(slot: ModelSlot, filled: bool) {
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

/// Registra modelo em slot. Não remove CURRENT externo — caller sincroniza Active.
pub fn register_model(slot: ModelSlot, model: Box<dyn Model>) {
    if matches!(
        slot,
        ModelSlot::Active | ModelSlot::RustCoder | ModelSlot::HwExpert
    ) {
        mark(slot, true);
        return;
    }
    let i = idx(slot);
    let mut hub = HUB.lock();
    hub.slots[i] = Some(model);
    mark(slot, true);
    k_nano::slog_cortex!(
        "MODEL",
        "info",
        "hub slot={} loaded",
        slot.name()
    );
}

pub fn take_model(slot: ModelSlot) -> Option<Box<dyn Model>> {
    let mut hub = HUB.lock();
    let m = hub.slots[idx(slot)].take();
    if m.is_some() {
        mark(slot, false);
    }
    m
}

/// Gera com slot específico; None se vazio.
pub fn generate_from_slot(slot: ModelSlot, prompt: &str) -> Option<String> {
    let hub = HUB.lock();
    hub.slots[idx(slot)].as_ref().map(|m| m.generate(prompt))
}

/// Heurística: conversation complexa → prefer Pro.
pub fn is_complex_conversation(prompt: &str) -> bool {
    if prompt.len() > 160 {
        return true;
    }
    let lower = prompt.as_bytes();
    // ASCII-ish contains without alloc
    contains_ci(prompt, "detalhad")
        || contains_ci(prompt, "analis")
        || contains_ci(prompt, "explain")
        || contains_ci(prompt, "compare")
        || contains_ci(prompt, "porque")
        || contains_ci(prompt, "why ")
        || contains_ci(prompt, "architect")
        || (lower.len() > 80 && (contains_ci(prompt, "como ") || contains_ci(prompt, "how ")))
}

/// Smoke / TinyStories intent.
pub fn wants_tinystories(prompt: &str) -> bool {
    contains_ci(prompt, "tinystories")
        || contains_ci(prompt, "[smoke]")
        || contains_ci(prompt, "smoke test")
        || contains_ci(prompt, "storytime")
        || contains_ci(prompt, "once upon")
        || contains_ci(prompt, "conta uma historia")
}

fn contains_ci(hay: &str, needle: &str) -> bool {
    let h = hay.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() || h.len() < n.len() {
        return false;
    }
    'outer: for i in 0..=(h.len() - n.len()) {
        for j in 0..n.len() {
            let a = h[i + j].to_ascii_lowercase();
            let b = n[j].to_ascii_lowercase();
            if a != b {
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
        ModelSlot::GeneratorFast | ModelSlot::GeneratorPro | ModelSlot::TinyStories
    ) && HUB.lock().slots[idx(slot)].is_some()
}

/// Escolhe slot generator por intent (sem alterar Trinity expert name).
pub fn select_generator_slot(prompt: &str) -> ModelSlot {
    if wants_tinystories(prompt) && hub_has_blob(ModelSlot::TinyStories) {
        return ModelSlot::TinyStories;
    }
    if is_complex_conversation(prompt) {
        if hub_has_blob(ModelSlot::GeneratorPro) {
            return ModelSlot::GeneratorPro;
        }
        if slot_loaded(ModelSlot::Active) {
            return ModelSlot::Active;
        }
    }
    if slot_loaded(ModelSlot::Active) {
        return ModelSlot::Active;
    }
    if hub_has_blob(ModelSlot::GeneratorFast) {
        return ModelSlot::GeneratorFast;
    }
    if hub_has_blob(ModelSlot::GeneratorPro) {
        return ModelSlot::GeneratorPro;
    }
    if hub_has_blob(ModelSlot::TinyStories) {
        return ModelSlot::TinyStories;
    }
    ModelSlot::Active
}

/// Relatório curto para logs/gates.
pub fn hub_status() -> String {
    let mut s = String::from("ModelHub:");
    for slot in [
        ModelSlot::Active,
        ModelSlot::GeneratorFast,
        ModelSlot::GeneratorPro,
        ModelSlot::TinyStories,
        ModelSlot::RustCoder,
        ModelSlot::HwExpert,
    ] {
        s.push(' ');
        s.push_str(slot.name());
        s.push('=');
        s.push_str(if slot_loaded(slot) { "1" } else { "0" });
    }
    s
}

/// Inferir slot a partir do tamanho do blob .bitnet (heurística boot).
pub fn slot_from_bitnet_bytes(len: usize) -> ModelSlot {
    const MB: usize = 1024 * 1024;
    if len < 20 * MB {
        ModelSlot::TinyStories
    } else if len < 450 * MB {
        // 850M (~174MB) e 1.3B/xl (~250–400MB)
        ModelSlot::GeneratorFast
    } else {
        ModelSlot::GeneratorPro
    }
}
