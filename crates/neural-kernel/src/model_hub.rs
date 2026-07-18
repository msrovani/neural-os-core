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
    GeneratorFast = 1,
    GeneratorPro = 2,
    TinyStories = 3,
    RustCoder = 4,
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
            "generator_fast" | "fast" | "850m" | "bitnet850" => Some(Self::GeneratorFast),
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
        ModelSlot::GeneratorFast | ModelSlot::GeneratorPro | ModelSlot::TinyStories
    ) && HUB.lock().slots[idx(slot)].is_some()
}

pub fn select_generator_slot(prompt: &str) -> ModelSlot {
    if wants_tinystories(prompt) && hub_has_blob(ModelSlot::TinyStories) {
        return ModelSlot::TinyStories;
    }
    // Complexo: blob Pro separado → Pro; senão Active (2B/3B no CURRENT).
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

pub fn slot_from_bitnet_bytes(len: usize) -> ModelSlot {
    const MB: usize = 1024 * 1024;
    if len < 20 * MB {
        ModelSlot::TinyStories
    } else if len < 200 * MB {
        ModelSlot::GeneratorFast
    } else {
        ModelSlot::GeneratorPro
    }
}

/// FAT 8.3 candidatos por slot (ordem de preferência).
pub fn fat_names_for(slot: ModelSlot) -> &'static [&'static str] {
    match slot {
        ModelSlot::TinyStories => &["TINYSTOR.BIN", "TINY.BIN", "STORIES.BIN"],
        ModelSlot::GeneratorFast => &["BITNET850.BIN", "BITN850.BIN", "MICRO.BIN", "MICRO.BITNET"],
        ModelSlot::GeneratorPro => &["BITNET3B.BIN", "BITN3B.BIN", "BITNET2B.BIN"],
        ModelSlot::RustCoder => &["RUSTCDR3.BIN", "RUSTCDR2.BIN", "RUSTCDR.BITNET", "RUSTCDR.BIN"],
        ModelSlot::HwExpert => &["HWEXPRT.BIN", "HWEXPERT.BIN"],
        ModelSlot::Active => &["BITNET2B.BIN", "BITNET.BIN", "MICRO.BITNET", "MICRO.BIN"],
    }
}
