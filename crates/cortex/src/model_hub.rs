//! ModelHub (bin) — slots .bitnet multi-modelo; truth do boot (ADR multi-model).

use alloc::boxed::Box;
use alloc::string::String;
use core::sync::atomic::{AtomicU64, AtomicU8, Ordering};
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

/// Política de residência (hub MoE usável): tiny no boot; mid/heavy via LOO.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResidencyTier {
    /// Carrega no boot se blob no FAT (HwExpert ~1MB).
    AlwaysTiny,
    /// Load-on-first-use; evict sob pressão.
    LooMid,
    /// LOO + evict; Pro só sob rota/select.
    LooHeavy,
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

    pub fn residency_tier(self) -> ResidencyTier {
        match self {
            Self::HwExpert => ResidencyTier::AlwaysTiny,
            // Learner: LOO mid por default; boot eager só se treino contínuo (bin).
            Self::Learner | Self::RustCoder | Self::Agent | Self::Reranker | Self::Vision => {
                ResidencyTier::LooMid
            }
            Self::Active | Self::GeneratorPro => ResidencyTier::LooHeavy,
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "active" | "current" | "generator" | "falcon3" | "falcon3b" | "falcon3-3b" | "3b"
            | "tiiuae" | "daily" | "lab" => Some(Self::Active),
            "vision" | "siglip" | "vit" | "encoder" => {
                Some(Self::Vision)
            }
            "generator_pro" | "pro" | "falcon7b" | "falcon3-7b" | "7b" | "falcon3-10b" | "10b"
            | "falcon10b" | "large" => Some(Self::GeneratorPro),
            "reranker" | "rerank" | "cross_encoder" | "bge_reranker" => Some(Self::Reranker),
            "rust_coder" | "rustcoder" => Some(Self::RustCoder),
            "hw_identify" | "hwexpert" => Some(Self::HwExpert),
            "learner" | "qwen05" | "qwen0.5b" | "falcon3-1b" | "falcon1b" | "1b" | "tiny" => {
                Some(Self::Learner)
            }
            "agent" | "qwen3b" | "agentic" | "orchestrator" => Some(Self::Agent),
            _ => None,
        }
    }
}

/// Slots AlwaysTiny — o boot deve residentar estes após `register_slot_loader`.
pub fn always_tiny_slots() -> &'static [ModelSlot] {
    &[ModelSlot::HwExpert]
}

const N_SLOTS: usize = 8;

struct HubInner {
    slots: [Option<Box<dyn Model>>; N_SLOTS],
}

static HUB: Mutex<HubInner> = Mutex::new(HubInner {
    slots: [None, None, None, None, None, None, None, None],
});
static SLOT_MASK: AtomicU8 = AtomicU8::new(0);

/// s482 Passo 1: footprint **medido** do blob por slot, em bytes. 0 = sem blob.
///
/// Antes disto, `slot_footprint_mb` lia UM header global (`loaded_model_header`, do
/// último carregado), então 3 slots respondiam pelo tamanho de outro e 3 mais caíam
/// em `_ => None` → `slot_too_tight(None) == false` → "sempre cabe". Medir por slot
/// mata as duas classes de mentira com um array.
static SLOT_FOOTPRINT: [AtomicU64; 8] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];

/// Grava o tamanho real do blob carregado neste slot (chamado em `register_bytes`).
pub fn note_footprint(slot: ModelSlot, bytes: u64) {
    SLOT_FOOTPRINT[idx(slot)].store(bytes, Ordering::Release);
}

/// Footprint medido do slot em MB. `None` = nenhum blob medido (nunca inferido).
pub fn measured_slot_mb(slot: ModelSlot) -> Option<u64> {
    let b = SLOT_FOOTPRINT[idx(slot)].load(Ordering::Acquire);
    if b == 0 {
        None
    } else {
        Some(b / (1024 * 1024))
    }
}

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
    // Honesty s388: Active/RustCoder/HwExpert vivem nos Mutex legados — NÃO drop.
    match slot {
        ModelSlot::Active => {
            crate::cortex::set_model(model);
            k_nano::slog_bin!("MODEL", "ok", "hub register Active → CURRENT_MODEL");
        }
        ModelSlot::RustCoder => {
            crate::cortex::set_rustcoder_model(model);
            k_nano::slog_bin!("MODEL", "ok", "hub register RustCoder → static");
        }
        ModelSlot::HwExpert => {
            crate::cortex::set_hwexpert_model(model);
            k_nano::slog_bin!("MODEL", "ok", "hub register HwExpert → static");
        }
        other => {
            let i = idx(other);
            HUB.lock().slots[i] = Some(model);
            mark(other, true);
            k_nano::slog_bin!("MODEL", "ok", "hub slot={} loaded", other.name());
        }
    }
}

/// Ponto único de carga por bytes (ADR-0085 §7): load_model_v6 → ModelView
/// → armazenamento por kind. Active/RustCoder/HwExpert usam os Mutex legados.
pub fn register_bytes(slot: ModelSlot, data: &[u8]) -> bool {
    // Honesty s387: Observe header ANTES do load - ADR-0101/AIOS (SKU sync k_hal).
    crate::model::note_header_from_bytes(data);
    // s482 Passo 1: footprint REAL do blob neste slot (medido, nao inferred).
    // `slot_footprint_mb` lia UM header global (do ultimo carregado), entao 3 slots
    // respondiam pelo tamanho de outro. Ver nota_footprint / slot_footprint_mb.
    note_footprint(slot, data.len() as u64);
    // 1. Tenta v6 .bitnet
    let view = match crate::model::load_model_v6(data) {
        Some(v) => Some(v),
        None => {
            // 2. Tenta GGUF — cria GgufBackedModel e seta como CURRENT_MODEL
            if let Ok(file) = crate::gguf::load_gguf(data) {
                k_nano::slog_bin!("MODEL", "ok", "register_bytes slot={} GGUF auto-config", slot.name());
                let gguf_model = crate::gguf::GgufBackedModel::new(file);
                crate::gguf::note_gguf_header(&gguf_model);
                let boxed: Box<dyn Model> = Box::new(gguf_model);
                // Honesty s387: Active → set_model (MODEL_LOADED + dim); Pro ≠ CURRENT.
                if matches!(slot, ModelSlot::Active) {
                    crate::cortex::set_model(boxed);
                } else {
                    let i = idx(slot);
                    HUB.lock().slots[i] = Some(boxed);
                    mark(slot, true);
                }
                k_nano::slog_bin!("MODEL", "ok", "GGUF slot={} registered", slot.name());
                return true;
            }
            None
        }
    };
    let view = match view {
        Some(v) => v,
        None => {
            // Fail-closed: header observado no início não pode sobreviver a load falho.
            crate::model::clear_model_header();
            k_nano::slog_bin!("MODEL", "warn", "register_bytes: formato desconhecido slot={}", slot.name());
            return false;
        }
    };
    match view {
        crate::model::ModelView::Llm(m) => {
            let boxed: Box<dyn Model> = Box::new(m);
            match slot {
                // Honesty s388: set_model = MODEL_LOADED + EMBED_DIM + slog AI_READY
                ModelSlot::Active => {
                    crate::cortex::set_model(boxed);
                    mark(slot, true);
                }
                ModelSlot::RustCoder => {
                    crate::cortex::set_rustcoder_model(boxed);
                }
                ModelSlot::HwExpert => {
                    // LLM bytes no slot HwExpert ≠ HWExpert v4 — não mentir loaded.
                    k_nano::slog_bin!(
                        "MODEL",
                        "warn",
                        "register_bytes: LLM no slot hw_identify — redireciona Active"
                    );
                    crate::cortex::set_model(boxed);
                    mark(ModelSlot::Active, true);
                    // O blob foi para Active, não para hw_identify: mede nos dois
                    // senão o footprint de Active fica stale (s482).
                    note_footprint(ModelSlot::Active, data.len() as u64);
                }
                other => {
                    let i = idx(other);
                    HUB.lock().slots[i] = Some(boxed);
                    mark(other, true);
                }
            }
            k_nano::slog_bin!("MODEL", "ok", "register_bytes slot={} LLM v6 ok", slot.name());
            true
        }
        crate::model::ModelView::HwExpert(m) => {
            // SESSION_359: antes mark(true) descartava `_m` → hub mentia "loaded"
            crate::cortex::set_hwexpert_v4_model(m);
            mark(slot, true);
            k_nano::slog_bin!("MODEL", "ok", "register_bytes slot={} HWExpert v6 → static", slot.name());
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

/// Gera a partir do slot — Active/RustCoder leem os Mutex legados (não o array HUB).
/// s483 Passo 2b — load-on-first-use.
///
/// O loader vive no bin (precisa de ATA/FAT32); a crate so tem a seam. Mesmo
/// padrao da bridge TLS (SESSION_241): tipo aqui, `register_*` aqui, o boot
/// registra com cast explicito, o consumidor chama a API daqui.
pub type SlotLoader = fn(ModelSlot) -> bool;

/// 0 = nenhum loader registrado. Guardamos o ponteiro como `usize` para nao
/// precisar de `Option<fn>` (nao Copy) dentro de um static.
static SLOT_LOADER: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

/// Chamado pelo boot com `try_hub_slot_fat` (ou equivalente).
pub fn register_slot_loader(f: SlotLoader) {
    SLOT_LOADER.store(f as usize, Ordering::Release);
}

/// Dispara o loader registrado uma vez, sob demanda.
fn try_load_on_demand(slot: ModelSlot) -> bool {
    let p = SLOT_LOADER.load(Ordering::Acquire);
    if p == 0 {
        return false;
    }
    // SAFETY: p veio de `register_slot_loader`, que so grava ponteiros do
    // mesmo tipo. 0 e verificado acima.
    let f: SlotLoader = unsafe { core::mem::transmute(p) };
    f(slot)
}

/// Garante o slot carregado, chamando o loader uma unica vez sob demanda.
/// Nao pega NENHUM lock: o loader usa `HUB` e `TicketLock` nao e reentrante
/// (s438) — carregar dentro do lock seria deadlock.
pub fn ensure_slot_loaded(slot: ModelSlot) -> bool {
    if slot_real(slot) {
        return true;
    }
    try_load_on_demand(slot)
}

/// s484 Passo 3: primitivo de EVICT explicito — libera o modelo do slot e
/// limpa mask + footprint (o estado volta a "nunca medido", honesto).
///
/// Direcao do plano @oracle: Active <-> (Pro|Agent), 1 evict por vez. O
/// contrario (recarregar o Active em runtime) nao tem caminho hoje — o Active
/// so carrega no boot — entao a direcao unica implementada e: sob pressao,
/// carregar Pro|Agent custa o Active (maior residente). Evict e DEGRADACAO:
/// sempre `warn`, nunca `ok` (mesma regra do fallback s480).
pub fn evict_slot(slot: ModelSlot) -> bool {
    match slot {
        // Active vive no CURRENT_MODEL (honesty s388) — clear_model ja faz
        // None + MODEL_LOADED=false + EMBED_DIM=0 + header + mark_active(false).
        ModelSlot::Active => {
            if !slot_loaded(ModelSlot::Active) {
                return false;
            }
            crate::cortex::clear_model();
            note_footprint(ModelSlot::Active, 0);
            // O alias Pro (mark_pro_alias) apontava para o Active — sem blob
            // Pro ele vira mentira no hub_status ("alias" de um morto).
            if !hub_has_blob(ModelSlot::GeneratorPro) {
                mark(ModelSlot::GeneratorPro, false);
            }
            k_nano::slog_bin!("MODEL", "warn", "hub EVICT slot=active (CURRENT_MODEL liberado)");
            true
        }
        // Experts legados (RustCoder/HwExpert) vivem em statics dedicados com
        // consumers proprios (MoE routing) — sem evict (YAGNI, fora do plano).
        ModelSlot::RustCoder | ModelSlot::HwExpert => false,
        other => {
            if !hub_has_blob(other) {
                return false;
            }
            let freed_mb = measured_slot_mb(other);
            let taken = HUB.lock().slots[idx(other)].take();
            drop(taken); // libera os tensores do heap AGORA
            mark(other, false);
            note_footprint(other, 0);
            k_nano::slog_bin!(
                "MODEL",
                "warn",
                "hub EVICT slot={} freed~{}MB",
                other.name(),
                freed_mb.unwrap_or(0)
            );
            true
        }
    }
}

pub fn generate_from_slot(slot: ModelSlot, prompt: &str) -> Option<String> {
    // s483 Passo 2b: primeiro consumidor dispara o load.
    let _ = ensure_slot_loaded(slot);
    match slot {
        ModelSlot::Active => {
            let guard = crate::cortex::CURRENT_MODEL.lock();
            guard.as_ref().map(|m| m.generate(prompt))
        }
        ModelSlot::RustCoder => {
            let guard = crate::cortex::RUSTCODER_MODEL.lock();
            guard.as_ref().map(|m| m.generate(prompt))
        }
        ModelSlot::HwExpert => {
            // Mutex legado (LLM) OU v4 multi-head (predict) — nunca HUB array.
            {
                let guard = crate::cortex::HWEXPERT_MODEL.lock();
                if let Some(m) = guard.as_ref() {
                    return Some(m.generate(prompt));
                }
            }
            try_hwexpert_v4_text(prompt)
        }
        other => {
            let hub = HUB.lock();
            hub.slots[idx(other)].as_ref().map(|m| m.generate(prompt))
        }
    }
}

/// HwExpert v4: extrai vid:did do prompt (ou injeta 1º PCI real) e formata saída usável.
fn try_hwexpert_v4_text(prompt: &str) -> Option<String> {
    if !crate::cortex::hwexpert_v4_is_loaded() {
        return None;
    }
    let (vid, did, injected) = match parse_pci_ids(prompt) {
        Some((v, d)) => (v, d, false),
        None => {
            let (v, d) = first_pci_vid_did()?;
            k_nano::slog_cortex!(
                "HWEXPERT",
                "ok",
                "hw_identify inject vid={:04x} did={:04x} (inventario PCI)",
                v,
                d
            );
            (v, d, true)
        }
    };
    let pred = crate::cortex::hwexpert_v4_predict(vid, did)?;
    let family = hw_family_name(pred.family_id);
    let agent = hw_agent_name(pred.agent_id);
    let inj = if injected { " inject=1" } else { "" };
    Some(alloc::format!(
        "hw_identify vid={:04x} did={:04x} family={} ({}) fw={} agent={} ({}) caps={:#x} next={}{}",
        vid,
        did,
        pred.family_id,
        family,
        pred.fw_id,
        pred.agent_id,
        agent,
        pred.caps_bits,
        pred.next_action,
        inj
    ))
}

fn hw_family_name(id: u8) -> &'static str {
    match id {
        0 => "Unknown",
        1 => "IntelE1000",
        2 => "VirtioNet",
        3 => "RealtekEth",
        4 => "IntelIwlWifi",
        5 => "RealtekWifi",
        6 => "AtherosWifi",
        7 => "BroadcomWifi",
        8 => "NvidiaGpu",
        9 => "IntelI915",
        10 => "AmdGpu",
        11 => "QemuVga",
        12 => "VirtioGpu",
        13 => "UsbHostXhci",
        14 => "IntelHda",
        15 => "StorageAta",
        16 => "PciBridge",
        _ => "Other",
    }
}

fn hw_agent_name(id: u8) -> &'static str {
    match id {
        0 => "HwBridgeAgent",
        1 => "NetAgent",
        2 => "WifiAgent",
        3 => "DisplayAgent",
        4 => "GpuBackend",
        5 => "UsbDriverAgent",
        6 => "HdaAudioAgent",
        7 => "DiskAgent",
        8 => "PlatformAgent",
        _ => "PlatformAgent",
    }
}

/// Primeiro dispositivo PCI válido do inventário (HW real / QEMU).
fn first_pci_vid_did() -> Option<(u16, u16)> {
    let mut found: Option<(u16, u16)> = None;
    unsafe {
        k_nano::pci::scan_pci_cb(|_bus, _slot, _func, vid, did| {
            if found.is_none() && vid != 0xffff && vid != 0 {
                found = Some((vid, did));
                true // stop
            } else {
                false
            }
        });
    }
    found
}

fn parse_pci_ids(prompt: &str) -> Option<(u16, u16)> {
    // Aceita "8086:2723", "0x8086:0x2723", "vid=8086 did=2723".
    let bytes = prompt.as_bytes();
    let mut i = 0;
    while i + 9 <= bytes.len() {
        if let Some((vid, next)) = parse_hex_u16_at(bytes, i) {
            let mut j = next;
            while j < bytes.len() && (bytes[j] == b':' || bytes[j] == b' ' || bytes[j] == b'=') {
                j += 1;
            }
            if let Some((did, _)) = parse_hex_u16_at(bytes, j) {
                return Some((vid, did));
            }
        }
        i += 1;
    }
    None
}

fn parse_hex_u16_at(bytes: &[u8], mut i: usize) -> Option<(u16, usize)> {
    if i + 2 <= bytes.len() && bytes[i] == b'0' && (bytes[i + 1] == b'x' || bytes[i + 1] == b'X') {
        i += 2;
    }
    let start = i;
    let mut val: u16 = 0;
    let mut n = 0usize;
    while i < bytes.len() && n < 4 {
        let c = bytes[i];
        let d = match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            b'A'..=b'F' => c - b'A' + 10,
            _ => break,
        };
        val = (val << 4) | (d as u16);
        n += 1;
        i += 1;
    }
    if n == 4 {
        Some((val, i))
    } else {
        let _ = start;
        None
    }
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
    let out = generate_from_slot(slot, prompt);
    // SESSION_359: limpa ponteiro residual se generate não consumiu (slot vazio / path sem take)
    let _ = crate::cortex::DECODER_CELL.take();
    out
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
        || contains_ci(prompt, "por qué")
        || contains_ci(prompt, "why ")
        || contains_ci(prompt, "architect")
        || contains_ci(prompt, "desenhe")
        || contains_ci(prompt, "design")
        || contains_ci(prompt, "diseño")
        || contains_ci(prompt, "implemente")
        || contains_ci(prompt, "implement")
        || (prompt.len() > 80 && (
            contains_ci(prompt, "como ")
            || contains_ci(prompt, "how ")
            || contains_ci(prompt, "cómo ")
        ))
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

/// Presença REAL de um modelo no slot — s480.
///
/// `slot_loaded` sozinho MENTE para GeneratorPro: `mark_pro_alias(true)`
/// (chamado em `main.rs:5041` quando o Active tem embed_dim >= 2048, o que o
/// 3B tem: 3072) marca Pro como carregado sem existir blob Pro. `hub_status`
/// já desambiguava isso com a tag "alias"; aqui a decisão é programática.
///
/// Os 5 slots do HUB só são reais com blob em `HUB`. Active/RustCoder/HwExpert
/// vivem nos Mutex legados (`register_model`), então `slot_loaded` é a
/// verdade para eles.
pub fn slot_real(slot: ModelSlot) -> bool {
    match slot {
        ModelSlot::Vision
        | ModelSlot::GeneratorPro
        | ModelSlot::Reranker
        | ModelSlot::Agent
        | ModelSlot::Learner => hub_has_blob(slot),
        ModelSlot::Active | ModelSlot::RustCoder | ModelSlot::HwExpert => slot_loaded(slot),
    }
}

fn pick_fit_fallback(preferred: ModelSlot, reason: &str) -> ModelSlot {
    // Honesty s388: Vision ≠ gerador — só Pro/Agent/Reranker/Active.
    let order = [
        ModelSlot::GeneratorPro,
        ModelSlot::Agent,
        ModelSlot::Reranker,
        ModelSlot::Active,
    ];
    for s in order {
        if slot_real(s) && fit_ok(s) {
            if s != preferred {
                // s480: fallback é DEGRADAÇÃO, não sucesso — sai como warn.
                // A lição do próprio repo: "sucesso com sev fail = dmesg mentiroso".
                k_nano::slog_bin!(
                    "FIT",
                    "warn",
                    "niche={} -> fallback={} reason={} (usando generico)",
                    preferred.name(),
                    s.name(),
                    reason
                );
            }
            return s;
        }
    }
    k_nano::slog_bin!(
        "FIT",
        "warn",
        "niche={} sem fallback carregado reason={} -> Active (pode estar vazio)",
        preferred.name(),
        reason
    );
    ModelSlot::Active
}

fn maybe_fit(slot: ModelSlot) -> ModelSlot {
    // s480: AUSÊNCIA também é motivo de fallback, não só aperto de RAM.
    // Antes, um nicho ausente com RAM folgada passava por `fit_ok` e era
    // devolvido — o consumidor recebia um slot vazio. Agora ausência e
    // too_tight caem no mesmo `pick_fit_fallback` (decisão não duplicada).
    let present = slot_real(slot);
    if present && fit_ok(slot) {
        slot
    } else {
        pick_fit_fallback(slot, if present { "too_tight" } else { "absent" })
    }
}

pub fn select_generator_slot(prompt: &str) -> ModelSlot {
    // Complexo: preferir Pro. s487 ovo-galinha — NÃO exigir hub_has_blob a priori:
    // generate_from_slot → ensure_slot_loaded dispara LOO; se o load falhar,
    // dispatch_expert (cortex) cai no Active com warn. Antes, Pro ausente com
    // RAM folgada nunca era escolhido → LOO do Pro morto por construção.
    if is_complex_conversation(prompt) {
        if slot_real(ModelSlot::GeneratorPro) {
            return maybe_fit(ModelSlot::GeneratorPro);
        }
        k_nano::slog_bin!(
            "FIT",
            "ok",
            "niche=generator_pro absent -> try LOO (complex prompt)"
        );
        return ModelSlot::GeneratorPro;
    }
    if slot_loaded(ModelSlot::Active) {
        return maybe_fit(ModelSlot::Active);
    }
    // Vision encoder não é gerador de texto — não entra no select_generator
    if slot_real(ModelSlot::GeneratorPro) {
        return maybe_fit(ModelSlot::GeneratorPro);
    }
    // Active vazio: tentar Pro via LOO (mesmo padrão do complexo).
    if fit_ok(ModelSlot::GeneratorPro) {
        k_nano::slog_bin!(
            "FIT",
            "ok",
            "niche=generator_pro absent -> try LOO (Active empty)"
        );
        return ModelSlot::GeneratorPro;
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
        // Honesty s388: Pro pode ser alias do Active (7B no CURRENT) sem blob hub.
        let tag = if slot == ModelSlot::GeneratorPro {
            if hub_has_blob(slot) {
                "1"
            } else if slot_loaded(slot) {
                "alias"
            } else {
                "0"
            }
        } else if slot_loaded(slot) {
            "1"
        } else {
            "0"
        };
        s.push_str(tag);
    }
    s
}

/// FAT 8.3 candidatos por slot (ordem de preferência).
pub fn fat_names_for(slot: ModelSlot) -> &'static [&'static str] {
    match slot {
        // target1 canônico (.v6 primeiro); fallback .BIN legado
        ModelSlot::Reranker => &["RERANKER.v6", "RERANKER.BIN", "RERANK.BITNET", "RERANK.BIN"],
        ModelSlot::Vision => &["VISION.v6", "VISION.BIN", "SIGLIP.BIN", "VIT.BIN"],
        ModelSlot::GeneratorPro => &[
            "PRO.v6",
            "PRO.BIN",
            "FALCON7B.v6",
            "FALCON7B.BIN",
            "PRO.GGUF",
            "FALCN7B.GGUF",
            "FALCON10.v6",
            "FALCON10.BIN",
            "FALCN10.GGUF",
            "BITNET3B.BIN",
            "LLAMA8B.BIN",
        ],
        ModelSlot::RustCoder => &["RUSTCDR3.v6", "RUSTCDR3.BIN", "RUSTCDR2.BIN", "RUSTCDR.BITNET", "RUSTCDR.BIN"],
        ModelSlot::HwExpert => &[
            "HWEXPRT.v6",
            "HWEXPRT4.BIN",
            "HWEXPRT4.bin",
            "HW_EXPERT.BITNET",
            "HWEXPRT.BIN",
            "HWEXPERT.BIN",
            "HW_EXPERT.BIN",
        ],
        ModelSlot::Learner => &[
            "FALCON1B.v6",
            "FALCON1B.BIN",
            "F1B.v6",
            "FALCN1B.GGUF",
            "LEARNER.v6",
            "LEARNER.BIN",
            "QWEEN05.BIN",
            "QWEN05B.BIN",
        ],
        ModelSlot::Agent => &["FALCON3B.BIN", "FALCN3B.GGUF", "AGENT.v6", "AGENT.BIN", "QWEN3B.BIN", "QWEN.BIN"],
        ModelSlot::Active => &[
            "FALCON3.V6",
            "FALCON3B.v6",
            "FALCON3B.BIN",
            "FALCN3B.GGUF",
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

#[cfg(test)]
mod evict_tests {
    use super::*;
    use alloc::string::String;

    struct DummyModel;
    impl crate::cortex::Model for DummyModel {
        fn generate(&self, _prompt: &str) -> String {
            String::new()
        }
        fn embed_dim(&self) -> usize {
            1
        }
        fn vocab_size(&self) -> u32 {
            1
        }
        fn max_seq(&self) -> usize {
            1
        }
    }

    #[test]
    fn residency_tiers_match_plan() {
        assert_eq!(
            ModelSlot::HwExpert.residency_tier(),
            ResidencyTier::AlwaysTiny
        );
        assert_eq!(
            ModelSlot::RustCoder.residency_tier(),
            ResidencyTier::LooMid
        );
        assert_eq!(
            ModelSlot::Agent.residency_tier(),
            ResidencyTier::LooMid
        );
        assert_eq!(
            ModelSlot::Active.residency_tier(),
            ResidencyTier::LooHeavy
        );
        assert_eq!(
            ModelSlot::GeneratorPro.residency_tier(),
            ResidencyTier::LooHeavy
        );
        assert_eq!(always_tiny_slots(), &[ModelSlot::HwExpert]);
    }

    #[test]
    fn select_complex_prefers_pro_even_absent() {
        // s487 ovo-galinha: Pro ausente ainda e escolhido p/ disparar LOO.
        let _ = evict_slot(ModelSlot::GeneratorPro);
        assert!(
            !slot_real(ModelSlot::GeneratorPro),
            "precondicao: Pro sem blob"
        );
        let slot = select_generator_slot(
            "explique detalhadamente a arquitetura do neural-os e compare com linux",
        );
        assert_eq!(
            slot,
            ModelSlot::GeneratorPro,
            "complexo + Pro ausente deve devolver Pro (LOO), nao Active"
        );
    }

    #[test]
    fn evict_slot_libera_blob_e_limpa_estado() {
        // Slot Agent: nenhum outro teste toca o HUB[7] (grep em cortex/src).
        let slot = ModelSlot::Agent;
        register_model(slot, alloc::boxed::Box::new(DummyModel));
        assert!(slot_real(slot), "dummy deve estar real antes do evict");

        assert!(evict_slot(slot), "evict de slot com blob = true");
        assert!(!slot_real(slot), "pos-evict o slot nao e mais real");
        assert_eq!(measured_slot_mb(slot), None, "footprint volta a None");
        // Evict de slot vazio = false (nada a liberar, nao finge sucesso).
        assert!(!evict_slot(slot));
        // Experts legados: sem evict por design.
        assert!(!evict_slot(ModelSlot::RustCoder));
        assert!(!evict_slot(ModelSlot::HwExpert));
    }
}
