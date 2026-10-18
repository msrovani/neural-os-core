//! cortex::model — registro mínimo de modelos .bitnet v6 (ADR-0085 §7).
//! Fronteira de formato única: load_model_v6 despacha por model_type para
//! LLM / HWExpert. Router é formato posicional próprio (trinity) — separado.
//! ModelHub usa `register_bytes` como ponto único.
//!
//! Fase 1 (autonomia): ModelHeader lê todos os campos do header v6 em runtime.
//! Zero hardcoded — o sistema descobre hidden/layers/vocab/kv/q_dim sozinho.

use alloc::string::String;
use core::sync::atomic::{AtomicBool, Ordering};
use crate::cortex::{HwExpertV4Model, TransformerModel};

// ── ModelHeader: parse autônomo do v6 ──────────────────────────────────

/// Header parsed from a .bitnet v6 file — autonomo, zero hardcoded.
/// Reads all fields from the binary header at runtime.
#[derive(Debug, Clone, Copy)]
pub struct ModelHeader {
    pub hidden: usize,
    pub num_layers: usize,
    pub num_heads: usize,
    pub vocab: usize,
    pub max_seq: usize,
    pub intermediate: usize,
    pub kv_heads: usize,
    pub q_dim: usize,
    pub num_medusa: usize,
    pub tie: bool,
    pub feat: u8,
    pub embed_type: u8,
    pub file_size: usize,
}

impl ModelHeader {
    /// Params estimados: embed + unembed + per_layer * num_layers.
    /// Saturates to u64::MAX on overflow (never wraps).
    pub fn estimated_params(&self) -> u64 {
        let embed = self.hidden.checked_mul(self.vocab).unwrap_or(usize::MAX) as u64;
        let kv_dim = self.kv_heads.checked_mul(self.q_dim.checked_div(self.num_heads.max(1)).unwrap_or(0)).unwrap_or(usize::MAX);
        let ffn_group = self.intermediate.checked_mul(self.q_dim).unwrap_or(usize::MAX).checked_div(self.hidden.max(1)).unwrap_or(usize::MAX);
        let per_layer = (self.hidden.checked_mul(self.q_dim).unwrap_or(usize::MAX))
            .saturating_add(self.hidden.checked_mul(kv_dim).unwrap_or(usize::MAX).saturating_mul(2))
            .saturating_add(self.q_dim.checked_mul(self.hidden).unwrap_or(usize::MAX))
            .saturating_add(self.hidden.checked_mul(ffn_group).unwrap_or(usize::MAX).saturating_mul(2))
            .saturating_add(self.intermediate.checked_mul(self.q_dim).unwrap_or(usize::MAX)) as u64;
        let unembed = if !self.tie { embed } else { 0 };
        embed.saturating_add(per_layer.saturating_mul(self.num_layers as u64)).saturating_add(unembed)
    }

    /// Tamanho em MB (arredondado).
    pub fn file_size_mb(&self) -> u64 {
        (self.file_size as u64 / (1024 * 1024)).max(1)
    }
}

/// Parse the v6/v7 header without loading the full model — lightweight.
/// Returns None if data is too short or not v6/v7 LLM.
/// v7 delta (ADR-0115 §2.2): tok BPB1 embutido + provenance após feat entram no size.
pub fn parse_model_header(data: &[u8]) -> Option<ModelHeader> {
    if data.len() < 52 { return None; }
    let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    if magic != 0xBE11BE11 { return None; }
    let version = u16::from_le_bytes([data[4], data[5]]);
    if (version != 6 && version != 7) || data[14] != 0 { return None; } // LLM only

    let hidden = u16::from_le_bytes([data[18], data[19]]) as usize;
    let num_layers = u16::from_le_bytes([data[20], data[21]]) as usize;
    let num_heads = u16::from_le_bytes([data[22], data[23]]) as usize;
    let vocab = u32::from_le_bytes([data[24], data[25], data[26], data[27]]) as usize;
    let max_seq = u16::from_le_bytes([data[28], data[29]]) as usize;
    let intermediate = u16::from_le_bytes([data[30], data[31]]) as usize;
    let kv_heads = u16::from_le_bytes([data[32], data[33]]) as usize;
    let q_dim = u16::from_le_bytes([data[34], data[35]]) as usize;
    let num_medusa = u32::from_le_bytes([data[36], data[37], data[38], data[39]]) as usize;
    let tie = &data[40..44] == b"TIED";
    let tok_len = u32::from_le_bytes([data[45], data[46], data[47], data[48]]) as usize;
    // v6: tok vazio (cap histórico); v7: tok BPB1 embutido (~3MB).
    let tok_cap = if version == 7 { 8 * 1024 * 1024 } else { 65536 };
    if tok_len > tok_cap { return None; }
    let mut hdr_end = 49usize.checked_add(tok_len)?.checked_add(3)?;
    if hdr_end > data.len() { return None; }
    let embed_type = *data.get(hdr_end.checked_sub(2)?)?;
    let feat = *data.get(hdr_end.checked_sub(1)?)?;
    // v7: provenance após feat (repo_len + repo + rev40 + conv_len + conv + git40 + 4B).
    if version == 7 {
        let rl = u16::from_le_bytes(*data.get(hdr_end..hdr_end.checked_add(2)?)?.first_chunk::<2>()?) as usize;
        if rl > 4096 { return None; }
        let mut p = hdr_end.checked_add(2)?.checked_add(rl)?.checked_add(40)?;
        if p > data.len() { return None; }
        let cl = u16::from_le_bytes(*data.get(p..p.checked_add(2)?)?.first_chunk::<2>()?) as usize;
        if cl > 4096 { return None; }
        p = p.checked_add(2)?.checked_add(cl)?.checked_add(40)?.checked_add(4)?;
        if p > data.len() { return None; }
        hdr_end = p;
    }

    // Calculate file size from header (same logic as the old v6_file_size)
    let kv_head_dim = q_dim.checked_div(num_heads.max(1))?;
    let k_dim = kv_heads.checked_mul(kv_head_dim)?;
    let ffn_group = intermediate.checked_mul(q_dim)?.checked_div(hidden.max(1))?;
    let down_out = q_dim;
    let mut size = hdr_end;
    size = size.checked_add(match embed_type {
        0 => hidden.checked_mul(vocab)?.checked_add(3)?.checked_div(4)?.checked_add(4)?,
        1 => hidden.checked_mul(vocab)?.checked_add(255)?.checked_div(256)?.checked_mul(210)?.checked_add(4)?,
        _ => hidden.checked_mul(vocab)?.checked_mul(2)?.checked_add(4)?,
    })?;
    let per_layer_norms = hidden.checked_mul(4)?.checked_mul(2)?
        .checked_add(if feat & 1 != 0 { hidden.checked_mul(4)? } else { 0 })?
        .checked_add(if feat & 2 != 0 { intermediate.checked_mul(4)? } else { 0 })?;
    let q_term = hidden.checked_mul(q_dim)?.checked_add(3)?.checked_div(4)?;
    let kv_term = hidden.checked_mul(k_dim)?.checked_add(3)?.checked_div(4)?;
    let o_term = q_dim.checked_mul(hidden)?.checked_add(3)?.checked_div(4)?;
    let ff_term = hidden.checked_mul(ffn_group)?.checked_add(3)?.checked_div(4)?;
    let down_term = intermediate.checked_mul(down_out)?.checked_add(3)?.checked_div(4)?;
    let per_layer_tern = q_term
        .checked_add(kv_term.checked_mul(2)?)?
        .checked_add(o_term)?
        .checked_add(ff_term.checked_mul(2)?)?
        .checked_add(down_term)?
        .checked_add(7usize.checked_mul(4)?)?;
    size = size.checked_add(num_layers.checked_mul(per_layer_norms.checked_add(per_layer_tern)?)?)?;
    size = size.checked_add(hidden.checked_mul(4)?)?; // rms_final
    if !tie {
        size = size.checked_add(if feat & 0x08 != 0 {
            hidden.checked_mul(vocab)?.checked_add(255)?.checked_div(256)?.checked_mul(210)?.checked_add(4)?
        } else {
            hidden.checked_mul(vocab)?.checked_add(3)?.checked_div(4)?.checked_add(4)?
        })?;
    }
    size = size.checked_add(num_medusa.checked_mul(hidden.checked_mul(vocab)?.checked_add(3)?.checked_div(4)?.checked_add(4)?)?)?;
    if feat & 4 != 0 { size = size.checked_add(4)?; }

    Some(ModelHeader { hidden, num_layers, num_heads, vocab, max_seq, intermediate,
        kv_heads, q_dim, num_medusa, tie, feat, embed_type, file_size: size })
}

/// Resultado de `probe_llm_file_size`.
/// O boot lê o loader em fatias: 64 B não cobrem o tokenizer embutido do v7.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LlmSizeProbe {
    /// Magic ausente, versão fora de 6/7, ou model_type diferente de LLM.
    NotLlm,
    /// Prefixo curto demais. `bytes` é o mínimo para a próxima decisão.
    Need(usize),
    /// Tamanho total do arquivo, o mesmo de `parse_model_header`.
    Ready(usize),
}

/// Descobre o tamanho de um LLM v6/v7 sem exigir o arquivo inteiro no buffer.
/// `Need` pede mais prefixo (tok_len ou provenance). `NotLlm` não é erro de
/// header: expert/router e lixo caem aqui para o caller escolher outro parser.
pub fn probe_llm_file_size(buf: &[u8]) -> LlmSizeProbe {
    if buf.len() < 4 {
        return LlmSizeProbe::Need(52);
    }
    let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if magic != 0xBE11BE11 {
        return LlmSizeProbe::NotLlm;
    }
    if buf.len() < 16 {
        return LlmSizeProbe::Need(52);
    }
    let version = u16::from_le_bytes([buf[4], buf[5]]);
    if version != 6 && version != 7 {
        return LlmSizeProbe::NotLlm;
    }
    if buf.len() < 49 {
        return LlmSizeProbe::Need(52);
    }
    if buf[14] != 0 {
        return LlmSizeProbe::NotLlm;
    }
    let tok_len = u32::from_le_bytes([buf[45], buf[46], buf[47], buf[48]]) as usize;
    let tok_cap = if version == 7 { 8 * 1024 * 1024 } else { 65536 };
    if tok_len > tok_cap {
        return LlmSizeProbe::NotLlm;
    }
    let mut need = match 49usize.checked_add(tok_len).and_then(|n| n.checked_add(3)) {
        Some(n) => n,
        None => return LlmSizeProbe::NotLlm,
    };
    if version == 7 {
        if buf.len() < need.saturating_add(2) {
            return LlmSizeProbe::Need(need.saturating_add(2));
        }
        let rl = u16::from_le_bytes([buf[need], buf[need + 1]]) as usize;
        if rl > 4096 {
            return LlmSizeProbe::NotLlm;
        }
        let p = match need
            .checked_add(2)
            .and_then(|n| n.checked_add(rl))
            .and_then(|n| n.checked_add(40))
        {
            Some(n) => n,
            None => return LlmSizeProbe::NotLlm,
        };
        if buf.len() < p.saturating_add(2) {
            return LlmSizeProbe::Need(p.saturating_add(2));
        }
        let cl = u16::from_le_bytes([buf[p], buf[p + 1]]) as usize;
        if cl > 4096 {
            return LlmSizeProbe::NotLlm;
        }
        need = match p
            .checked_add(2)
            .and_then(|n| n.checked_add(cl))
            .and_then(|n| n.checked_add(40))
            .and_then(|n| n.checked_add(4))
        {
            Some(n) => n,
            None => return LlmSizeProbe::NotLlm,
        };
    }
    if buf.len() < need {
        return LlmSizeProbe::Need(need);
    }
    match parse_model_header(buf) {
        Some(h) => LlmSizeProbe::Ready(h.file_size),
        None => LlmSizeProbe::NotLlm,
    }
}

/// s483 Passo 0 (plano s481 Passo 2/4): validação entre read e load.
/// v6/v7 LLM: o header é autodescritivo — o tamanho declarado TEM que bater
/// com o blob lido (s480: fantasma de tamanho = recusa, nunca registra).
/// v1..=5 legacy: sem tamanho declarado no header — só magic (o warn de
/// migração já vem do load_model_v6). v6/v7 com model_type != 0 (expert/
/// router) não é LLM: recusa — o HwExpert tem path dedicado
/// (load_hwexpert_v6) e parsear como LLM é fantasma.
pub fn validate_bitnet_blob(data: &[u8]) -> Result<(), &'static str> {
    if data.len() < 16 {
        return Err("too_short");
    }
    let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    if magic != 0xBE11BE11 {
        return Err("bad_magic");
    }
    let version = u16::from_le_bytes([data[4], data[5]]);
    match version {
        1..=5 => Ok(()),
        6 | 7 => {
            if data[14] != 0 {
                return Err("not_llm");
            }
            match probe_llm_file_size(data) {
                LlmSizeProbe::Ready(n) if n == data.len() => Ok(()),
                LlmSizeProbe::Ready(_) => Err("size_mismatch"),
                LlmSizeProbe::Need(_) => Err("truncated_header"),
                LlmSizeProbe::NotLlm => Err("bad_header"),
            }
        }
        _ => Err("unsupported_version"),
    }
}

/// Validação por slot do ModelHub (LOO / metal). HwExpert e Vision/Reranker
/// não passam pelo gate LLM-only de `validate_bitnet_blob`.
pub fn validate_for_slot(
    slot: crate::model_hub::ModelSlot,
    data: &[u8],
) -> Result<(), &'static str> {
    use crate::model_hub::ModelSlot;
    if data.len() < 16 {
        return Err("too_short");
    }
    let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    if magic != 0xBE11BE11 {
        return Err("bad_magic");
    }
    let version = u16::from_le_bytes([data[4], data[5]]);
    match slot {
        ModelSlot::HwExpert => match version {
            1..=5 => Ok(()), // legado; loader decide
            6 | 7 => {
                // model_type: 0=LLM, 1=HwExpert, 2=router — só 1 é coerente aqui.
                if data[14] == 1 {
                    Ok(())
                } else if data[14] == 0 {
                    Err("llm_in_hwexpert_slot")
                } else {
                    Err("not_hwexpert")
                }
            }
            _ => Err("unsupported_version"),
        },
        ModelSlot::Vision | ModelSlot::Reranker => match version {
            1..=5 => Ok(()),
            6 | 7 => {
                // Encoder/rerank podem ser type 0 (LLM shell) ou legado; não exigir probe
                // de tamanho LLM (artefatos v5/v6 variam). Magic+versão bastam.
                Ok(())
            }
            _ => Err("unsupported_version"),
        },
        ModelSlot::Active
        | ModelSlot::GeneratorPro
        | ModelSlot::Agent
        | ModelSlot::RustCoder
        | ModelSlot::Learner => validate_bitnet_blob(data),
    }
}

// ── Static: header do modelo carregado ─────────────────────────────────

static HEADER_LOADED: AtomicBool = AtomicBool::new(false);
// L4: sem `static mut` — ModelHeader é Copy/plain-data; leitura/escrita via
// spin::Mutex (barreira Acquire/Release explícita, sem UB de data race).
static LOADED_HEADER: spin::Mutex<ModelHeader> = spin::Mutex::new(ModelHeader {
    hidden: 0, num_layers: 0, num_heads: 0, vocab: 0, max_seq: 0,
    intermediate: 0, kv_heads: 0, q_dim: 0, num_medusa: 0,
    tie: false, feat: 0, embed_type: 0, file_size: 0,
});

/// Store the header of the loaded model (called from set_model or load path).
/// AIOS: dispara sync de SKU Falcon3 (1B/3B/7B/10B) no lado k_hal via hook.
pub fn set_model_header(h: ModelHeader) {
    *LOADED_HEADER.lock() = h;
    HEADER_LOADED.store(true, Ordering::Release);
    let f = MODEL_HEADER_HOOK.load(Ordering::Acquire);
    if f != 0 {
        unsafe {
            let cb: fn() = core::mem::transmute(f);
            cb();
        }
    }
}

/// Honesty s387: clear quando CURRENT_MODEL cai — senão header órfão mente.
pub fn clear_model_header() {
    HEADER_LOADED.store(false, Ordering::Release);
}

/// Aplica o mesmo clamp de `load_llm_v6` (SESSION_349 OOM) ao header Observe.
pub fn header_with_runtime_clamp(mut h: ModelHeader) -> ModelHeader {
    if h.hidden >= 2048 && h.max_seq > 4096 {
        h.max_seq = 4096;
    }
    h
}

/// Parse + store a partir dos bytes do artefato (v6). No-op se não for LLM v6.
pub fn note_header_from_bytes(data: &[u8]) {
    if let Some(h) = parse_model_header(data) {
        set_model_header(header_with_runtime_clamp(h));
    }
}

static MODEL_HEADER_HOOK: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);

/// k_hal registra `aios_adapt::adapt_on_model_loaded` no boot (zero ciclo crate).
pub fn register_model_header_hook(f: fn()) {
    MODEL_HEADER_HOOK.store(f as usize, Ordering::Release);
}

/// Get the header of the currently loaded model, if any.
pub fn loaded_model_header() -> Option<ModelHeader> {
    if HEADER_LOADED.load(Ordering::Acquire) {
        Some(*LOADED_HEADER.lock())
    } else {
        None
    }
}

/// Human-readable model info from header (zero hardcoded). Used by n3_cortex_gate.
pub fn model_info() -> String {
    if let Some(h) = loaded_model_header() {
        alloc::format!(
            "Falcon3 hidden={} layers={} heads={} kv={} vocab={} intermediate={} max_seq={} file={}MB",
            h.hidden, h.num_layers, h.num_heads, h.kv_heads, h.vocab, h.intermediate, h.max_seq, h.file_size_mb()
        )
    } else {
        String::from("no model loaded")
    }
}

// ── Tipos de modelo ────────────────────────────────────────────────────

/// Tipos de modelo reconhecidos (ADR-0085 S2: model_type).
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
            0 => crate::cortex::load_model_v6(data).map(ModelView::Llm),
            1 => crate::cortex::load_hwexpert_v6(data).map(ModelView::HwExpert),
            _ => {
                k_nano::slog_cortex!("MODEL", "warn", "v6 model_type={} desconhecido", data[14]);
                None
            }
        },
        7 => match data[14] {
            0 => crate::cortex::load_model_v7(data).map(ModelView::Llm),
            _ => {
                k_nano::slog_cortex!("MODEL", "warn", "v7 model_type={} não suportado", data[14]);
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
/// Delega a parse_model_header — zero hardcoded.
pub fn v6_file_size(data: &[u8]) -> Option<usize> {
    parse_model_header(data).map(|h| h.file_size)
}

// ── T-064 (ADR-0100 Onda 7): PPL por unigramas ──────────────────────────

/// Limiar fixo de aceitação: PPL < 50 = "aceitável" (texto coerente).
pub const PPL_ACCEPT_THRESHOLD: u32 = 50;

/// Estimativa de perplexidade por modelo unigrama (frequentista, com
/// suavização add-one sobre o vocabulário observado no próprio texto).
/// Retorna ceil(ppl) como u32; texto vazio/single-token → u32::MAX.
/// ponytail: heurística barata sem LLM — upgrade path = PPL real via modelo.
pub fn ppl_estimate(text: &str) -> u32 {
    let mut counts: alloc::vec::Vec<(u64, u32)> = alloc::vec::Vec::new();
    let mut total: u64 = 0;
    for w in text.split(|c: char| c.is_whitespace()) {
        if w.is_empty() { continue; }
        total += 1;
        // hash FNV-1a 64 da palavra (normalizada lowercase ASCII)
        let mut h: u64 = 0xcbf29ce484222325;
        for b in w.bytes() { h = (h ^ (b | 0x20) as u64).wrapping_mul(0x100000001b3); }
        match counts.iter_mut().find(|(k, _)| *k == h) {
            Some((_, c)) => *c += 1,
            None => counts.push((h, 1)),
        }
    }
    let vocab = counts.len() as u64;
    if total < 2 || vocab == 0 { return u32::MAX; }
    // ppl = exp( (1/total) * Σ -log( (c+1) / (total+vocab) ) )
    let denom = (total + vocab) as f32;
    let mut sum = 0.0f32;
    for (_, c) in &counts {
        let p = (*c as f32 + 1.0) / denom;
        sum -= unsafe { libm::logf(p) } * (*c as f32);
    }
    let ppl = unsafe { libm::expf(sum / total as f32) };
    if ppl.is_nan() || ppl < 0.0 { return u32::MAX; }
    (ppl as u64).saturating_add(1).min(u32::MAX as u64) as u32
}

/// Gate fixo do T-064: true se o texto é "aceitável" (PPL < 50).
pub fn ppl_acceptable(text: &str) -> bool {
    ppl_estimate(text) < PPL_ACCEPT_THRESHOLD
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ppl_repeated_text_is_low() {
        // texto altamente repetitivo → perplexidade baixa
        assert!(ppl_acceptable("ola ola ola ola ola ola ola ola mundo mundo mundo mundo"));
    }

    #[test]
    fn ppl_distinct_tokens_is_high() {
        // 60 tokens distintos: ppl ≈ vocab+1 → acima do threshold
        let t: alloc::string::String = (0..60u32)
            .map(|i| alloc::format!("tok{} ", i)).collect();
        assert!(!ppl_acceptable(&t));
    }

    #[test]
    fn ppl_empty_is_max() {
        assert_eq!(ppl_estimate(""), u32::MAX);
    }

    /// v7 sintético: version-gate + provenance skip no size math.
    /// Header mínimo (hidden=8, L=1, vocab=16, tok vazio, feat=0x04 theta).
    /// Nunca include_bytes de modelo real (690MB+).
    fn synth_header(version: u16, provenance: bool) -> alloc::vec::Vec<u8> {
        let mut b = alloc::vec::Vec::new();
        let u16le = |b: &mut alloc::vec::Vec<u8>, v: u16| b.extend_from_slice(&v.to_le_bytes());
        let u32le = |b: &mut alloc::vec::Vec<u8>, v: u32| b.extend_from_slice(&v.to_le_bytes());
        b.extend_from_slice(&0xBE11BE11u32.to_le_bytes());
        u16le(&mut b, version);
        b.extend_from_slice(&0u64.to_le_bytes()); // num_params
        b.push(0); // model_type LLM
        b.extend_from_slice(&[0, 0, 0]); // reserved
        u16le(&mut b, 8); // hidden
        u16le(&mut b, 1); // layers
        u16le(&mut b, 2); // heads
        u32le(&mut b, 16); // vocab
        u16le(&mut b, 64); // max_seq
        u16le(&mut b, 16); // intermediate
        u16le(&mut b, 1); // kv_heads
        u16le(&mut b, 8); // q_dim
        u32le(&mut b, 0); // medusa
        b.extend_from_slice(&[0, 0, 0, 0]); // tie_flag
        b.push(0); // tok_type
        u32le(&mut b, 0); // tok_len
        b.push(0); // act
        b.push(0); // embed_type ternary
        b.push(0x04); // feat theta
        if provenance {
            u16le(&mut b, 1); // repo_len
            b.push(b'r'); // repo
            b.extend_from_slice(&[b'v'; 40]); // rev
            u16le(&mut b, 0); // conv_len
            b.extend_from_slice(&[b'g'; 40]); // git
            b.extend_from_slice(&[1, 1, 0, 0]); // codes
        }
        b
    }

    #[test]
    fn v7_provenance_skip_and_version_gate() {
        // ver=8: parse None + load None (refuse explícito, nunca misparse).
        let mut bad = synth_header(6, false);
        bad[4] = 8; bad[5] = 0;
        assert!(parse_model_header(&bad).is_none());
        assert!(load_model_v6(&bad).is_none());
        // v6 sem provenance: baseline.
        let v6 = synth_header(6, false);
        let h6 = parse_model_header(&v6).expect("v6 header");
        assert_eq!(h6.hidden, 8);
        // v7 com provenance (repo=1B): size = v6 + 89B exatos.
        let v7 = synth_header(7, true);
        let h7 = parse_model_header(&v7).expect("v7 header");
        assert_eq!(h7.hidden, 8);
        assert_eq!(h7.file_size - h6.file_size, 2 + 1 + 40 + 2 + 0 + 40 + 4);
        // v7 sem provenance mas truncado no corpo: load None (fail-closed).
        assert!(load_model_v6(&v7).is_none());
        // 64 B não cobrem o tok embutido: o probe pede o resto, não mente h=0.
        let mut with_tok = synth_header(7, true);
        with_tok[45] = 100;
        with_tok[46] = 0;
        with_tok[47] = 0;
        with_tok[48] = 0;
        assert_eq!(
            probe_llm_file_size(&with_tok[..64.min(with_tok.len())]),
            LlmSizeProbe::Need(49 + 100 + 3 + 2)
        );
        assert!(matches!(probe_llm_file_size(&v7[..64]), LlmSizeProbe::Need(_)));
        assert_eq!(probe_llm_file_size(&v7), LlmSizeProbe::Ready(h7.file_size));
        assert_eq!(probe_llm_file_size(&v6), LlmSizeProbe::Ready(h6.file_size));
        assert_eq!(probe_llm_file_size(&[0, 1, 2, 3]), LlmSizeProbe::NotLlm);
        // v6 com tok_len acima do cap histórico: None (comportamento intacto).
        let mut bigtok = synth_header(6, false);
        bigtok[45] = 0x70; bigtok[46] = 0x11; bigtok[47] = 0x01; bigtok[48] = 0x00; // 70000
        assert!(parse_model_header(&bigtok).is_none());
    }

    #[test]
    fn validate_bitnet_blob_gates() {
        // v6 válido: tamanho declarado == blob → Ok.
        let v6 = synth_header(6, false);
        let h6 = parse_model_header(&v6).expect("v6 header");
        let mut blob = v6.clone();
        blob.resize(h6.file_size, 0);
        assert_eq!(validate_bitnet_blob(&blob), Ok(()));
        // Truncado: declarado != lido → recusa (fantasma de tamanho, s480).
        blob.truncate(h6.file_size - 1);
        assert_eq!(validate_bitnet_blob(&blob), Err("size_mismatch"));
        // Magic errado → recusa.
        let mut junk = alloc::vec![0u8; 32];
        junk[0] = 0x11; junk[1] = 0xBE;
        assert_eq!(validate_bitnet_blob(&junk), Err("bad_magic"));
        // Curto (<16) → recusa.
        assert_eq!(validate_bitnet_blob(&[0xBE, 0x11, 0xBE, 0x11]), Err("too_short"));
        // v5 legacy: sem tamanho declarado — só magic → Ok (warn vem do loader).
        let mut v5 = alloc::vec![0u8; 32];
        v5[..4].copy_from_slice(&0xBE11BE11u32.to_le_bytes());
        v5[4] = 5; v5[5] = 0;
        assert_eq!(validate_bitnet_blob(&v5), Ok(()));
        // v8 → versão não suportada.
        let mut v8 = synth_header(6, false);
        v8[4] = 8; v8[5] = 0;
        assert_eq!(validate_bitnet_blob(&v8), Err("unsupported_version"));
        // model_type != 0 (expert/router) → não é LLM (path dedicado).
        let mut expert = synth_header(6, false);
        expert[14] = 1;
        assert_eq!(validate_bitnet_blob(&expert), Err("not_llm"));
    }

    #[test]
    fn validate_for_slot_hwexpert_vs_llm() {
        use crate::model_hub::ModelSlot;
        let mut expert = synth_header(6, false);
        expert[14] = 1;
        assert_eq!(
            validate_for_slot(ModelSlot::HwExpert, &expert),
            Ok(())
        );
        assert_eq!(
            validate_for_slot(ModelSlot::Active, &expert),
            Err("not_llm")
        );
        let v6 = synth_header(6, false);
        let h6 = parse_model_header(&v6).expect("v6");
        let mut llm = v6.clone();
        llm.resize(h6.file_size, 0);
        assert_eq!(validate_for_slot(ModelSlot::Active, &llm), Ok(()));
        assert_eq!(
            validate_for_slot(ModelSlot::HwExpert, &llm),
            Err("llm_in_hwexpert_slot")
        );
        let mut v5 = alloc::vec![0u8; 32];
        v5[..4].copy_from_slice(&0xBE11BE11u32.to_le_bytes());
        v5[4] = 5;
        v5[5] = 0;
        assert_eq!(validate_for_slot(ModelSlot::Vision, &v5), Ok(()));
        assert_eq!(validate_for_slot(ModelSlot::Reranker, &v5), Ok(()));
    }
}
