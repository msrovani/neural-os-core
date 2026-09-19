//! ADR-0106 — Contrato `Decision<T>`: confiança Q8, margem, abstenção, fonte.
//!
//! Forma System One (Choice/Score/Noul) **sem** vendor externo. Soft-float:
//! fronteira em u8/Q8; f32 só onde o softmax Trinity já existe.

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use ticket_lock::TicketLock;

pub const DECISION_CONTRACT_VERSION: u16 = 1;

/// Q8 — 0..=255 (SESSION_336/346: sem f32 novo no hot path).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Confidence(pub u8);

impl Confidence {
    pub const ZERO: Self = Self(0);
    pub const MAX: Self = Self(255);

    pub fn from_f32(p: f32) -> Self {
        if p <= 0.0 {
            return Self(0);
        }
        if p >= 1.0 {
            return Self(255);
        }
        Self((p * 255.0) as u8)
    }

    pub fn as_f32(self) -> f32 {
        self.0 as f32 / 255.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecisionSource {
    Neural,
    Heuristic,
    Cache,
    Fallback,
}

impl DecisionSource {
    pub fn name(self) -> &'static str {
        match self {
            DecisionSource::Neural => "neural",
            DecisionSource::Heuristic => "heuristic",
            DecisionSource::Cache => "cache",
            DecisionSource::Fallback => "fallback",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbstainReason {
    LowConfidence,
    Tie,
    NoOptions,
    ScorerAbsent,
}

impl AbstainReason {
    pub fn name(self) -> &'static str {
        match self {
            AbstainReason::LowConfidence => "low_conf",
            AbstainReason::Tie => "tie",
            AbstainReason::NoOptions => "no_options",
            AbstainReason::ScorerAbsent => "scorer_absent",
        }
    }
}

/// Distribuição heapless Q8 (N fixo no call-site via array).
#[derive(Clone, Copy, Debug)]
pub struct Q8Dist<const N: usize> {
    pub bins: [u8; N],
}

impl<const N: usize> Q8Dist<N> {
    pub const ZERO: Self = Self { bins: [0; N] };

    /// Normaliza pesos u16 → soma ≈ 255 (resto no argmax).
    pub fn from_weights(weights: &[u16; N]) -> Self {
        let sum: u32 = weights.iter().map(|&w| w as u32).sum();
        if sum == 0 {
            return Self::ZERO;
        }
        let mut bins = [0u8; N];
        let mut allocated: u32 = 0;
        let mut best_i = 0usize;
        let mut best_w = 0u16;
        for i in 0..N {
            let w = weights[i];
            if w > best_w {
                best_w = w;
                best_i = i;
            }
            let v = ((w as u32) * 255) / sum;
            bins[i] = v as u8;
            allocated += v;
        }
        if allocated < 255 {
            bins[best_i] = bins[best_i].saturating_add((255 - allocated) as u8);
        }
        Self { bins }
    }

    pub fn sum(&self) -> u32 {
        self.bins.iter().map(|&b| b as u32).sum()
    }

    pub fn argmax(&self) -> Option<(usize, u8)> {
        let mut best_i = 0usize;
        let mut best = 0u8;
        let mut second = 0u8;
        for (i, &b) in self.bins.iter().enumerate() {
            if b > best {
                second = best;
                best = b;
                best_i = i;
            } else if b > second {
                second = b;
            }
        }
        if best == 0 {
            None
        } else {
            let _ = second;
            Some((best_i, best))
        }
    }

    pub fn margin(&self) -> u8 {
        let mut best = 0u8;
        let mut second = 0u8;
        for &b in &self.bins {
            if b > best {
                second = best;
                best = b;
            } else if b > second {
                second = b;
            }
        }
        best.saturating_sub(second)
    }
}

/// T = enum Copy (conjunto FECHADO). `choice=None` = abstenção.
#[derive(Clone, Copy, Debug)]
pub struct Decision<T: Copy, const N: usize> {
    pub choice: Option<T>,
    pub confidence: Confidence,
    pub margin: Confidence,
    pub source: DecisionSource,
    pub abstain: Option<AbstainReason>,
    pub dist: Q8Dist<N>,
}

impl<T: Copy, const N: usize> Decision<T, N> {
    pub fn abstain(reason: AbstainReason, source: DecisionSource) -> Self {
        Self {
            choice: None,
            confidence: Confidence::ZERO,
            margin: Confidence::ZERO,
            source,
            abstain: Some(reason),
            dist: Q8Dist::ZERO,
        }
    }

    pub fn from_dist(
        options: &[T; N],
        dist: Q8Dist<N>,
        source: DecisionSource,
        theta_conf: u8,
        theta_margin: u8,
    ) -> Self {
        let margin = Confidence(dist.margin());
        match dist.argmax() {
            None => Self::abstain(AbstainReason::NoOptions, source),
            Some((i, conf_u8)) => {
                let confidence = Confidence(conf_u8);
                if conf_u8 < theta_conf {
                    return Self {
                        choice: None,
                        confidence,
                        margin,
                        source,
                        abstain: Some(AbstainReason::LowConfidence),
                        dist,
                    };
                }
                if margin.0 < theta_margin {
                    return Self {
                        choice: None,
                        confidence,
                        margin,
                        source,
                        abstain: Some(AbstainReason::Tie),
                        dist,
                    };
                }
                Self {
                    choice: Some(options[i]),
                    confidence,
                    margin,
                    source,
                    abstain: None,
                    dist,
                }
            }
        }
    }

    pub fn slog_fields(&self, site: &str, choice_name: &str) -> () {
        let abs = self.abstain.map(|a| a.name()).unwrap_or("-");
        k_nano::slog_cortex!(
            "Decide",
            "ok",
            "site={} source={} conf={} margin={} choice={} abstain={} ver={}",
            site,
            self.source.name(),
            self.confidence.0,
            self.margin.0,
            choice_name,
            abs,
            DECISION_CONTRACT_VERSION
        );
    }
}

// ── Noul (P(yes) Q8) ────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug)]
pub struct Noul {
    pub p_yes: Confidence,
    pub source: DecisionSource,
    pub abstain: Option<AbstainReason>,
}

impl Noul {
    pub fn from_p_yes(p: f32, source: DecisionSource) -> Self {
        Self {
            p_yes: Confidence::from_f32(p),
            source,
            abstain: None,
        }
    }

    pub fn absent() -> Self {
        Self {
            p_yes: Confidence::ZERO,
            source: DecisionSource::Fallback,
            abstain: Some(AbstainReason::ScorerAbsent),
        }
    }

    /// true se P(yes) ≥ theta; None se abster.
    pub fn decide(&self, theta: u8) -> Option<bool> {
        if self.abstain.is_some() {
            return None;
        }
        // Incerteza perto de 0.5 → abster
        let p = self.p_yes.0;
        if p > 100 && p < 155 {
            return None;
        }
        Some(p >= theta)
    }
}

// ── Score ordinal (0..L-1) ──────────────────────────────────────────────

pub type ScoreDecision<const L: usize> = Decision<u8, L>;

// ── Reliableza / telemetria ─────────────────────────────────────────────

const MAX_SITES: usize = 16;
const SITE_KEY_LEN: usize = 24;

#[derive(Clone, Copy)]
struct SiteStats {
    key: [u8; SITE_KEY_LEN],
    key_len: u8,
    auto_ok: u32,
    auto_bad: u32,
    abstain: u32,
    escalate: u32,
    trusted: bool,
}

impl SiteStats {
    const EMPTY: Self = Self {
        key: [0; SITE_KEY_LEN],
        key_len: 0,
        auto_ok: 0,
        auto_bad: 0,
        abstain: 0,
        escalate: 0,
        trusted: false,
    };

    fn name(&self) -> &str {
        core::str::from_utf8(&self.key[..self.key_len as usize]).unwrap_or("?")
    }
}

struct ReliabilityStore {
    sites: [SiteStats; MAX_SITES],
    n: usize,
}

impl ReliabilityStore {
    const fn new() -> Self {
        Self {
            sites: [SiteStats::EMPTY; MAX_SITES],
            n: 0,
        }
    }

    fn find_or_insert(&mut self, site: &str) -> Option<usize> {
        let bytes = site.as_bytes();
        let len = bytes.len().min(SITE_KEY_LEN);
        for i in 0..self.n {
            let s = &self.sites[i];
            if s.key_len as usize == len && &s.key[..len] == &bytes[..len] {
                return Some(i);
            }
        }
        if self.n >= MAX_SITES {
            return None;
        }
        let i = self.n;
        self.sites[i].key[..len].copy_from_slice(&bytes[..len]);
        self.sites[i].key_len = len as u8;
        self.n += 1;
        Some(i)
    }
}

static RELIABILITY: TicketLock<ReliabilityStore> = TicketLock::new(ReliabilityStore::new());
static PERSIST_DIRTY: AtomicBool = AtomicBool::new(false);
static TOTAL_AUTO: AtomicU64 = AtomicU64::new(0);
static TOTAL_ABSTAIN: AtomicU64 = AtomicU64::new(0);
static TOTAL_ESCALATE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutcomeKind {
    AutoOk,
    AutoBad,
    Abstain,
    Escalate,
}

/// Registra resultado verificado (HITL / skill success) — D0/D4.
pub fn note_outcome(site: &str, kind: OutcomeKind) {
    match kind {
        OutcomeKind::AutoOk => {
            TOTAL_AUTO.fetch_add(1, Ordering::Relaxed);
        }
        OutcomeKind::Abstain => {
            TOTAL_ABSTAIN.fetch_add(1, Ordering::Relaxed);
        }
        OutcomeKind::Escalate => {
            TOTAL_ESCALATE.fetch_add(1, Ordering::Relaxed);
        }
        OutcomeKind::AutoBad => {}
    }
    let mut g = RELIABILITY.lock();
    if let Some(i) = g.find_or_insert(site) {
        let s = &mut g.sites[i];
        match kind {
            OutcomeKind::AutoOk => s.auto_ok = s.auto_ok.saturating_add(1),
            OutcomeKind::AutoBad => s.auto_bad = s.auto_bad.saturating_add(1),
            OutcomeKind::Abstain => s.abstain = s.abstain.saturating_add(1),
            OutcomeKind::Escalate => s.escalate = s.escalate.saturating_add(1),
        }
        let n = s.auto_ok.saturating_add(s.auto_bad);
        if n >= 8 {
            let ok_rate = (s.auto_ok as u64 * 100) / (n as u64);
            s.trusted = ok_rate >= 70;
        }
        PERSIST_DIRTY.store(true, Ordering::Release);
    }
}

/// θ conservador se `trusted=false` (ADR-0106 §0.4).
pub fn theta_for_site(site: &str, default_auto: u8, default_review: u8) -> (u8, u8, bool) {
    let g = RELIABILITY.lock();
    let bytes = site.as_bytes();
    let len = bytes.len().min(SITE_KEY_LEN);
    for i in 0..g.n {
        let s = &g.sites[i];
        if s.key_len as usize == len && &s.key[..len] == &bytes[..len] {
            if s.trusted {
                return (default_auto, default_review, true);
            }
            // conservador: sobe barra (leve — D0 ainda sem diagrama)
            return (
                default_auto.saturating_add(10).min(230),
                default_review.saturating_add(8).min(220),
                false,
            );
        }
    }
    (
        default_auto.saturating_add(12).min(235),
        default_review.saturating_add(10).min(225),
        false,
    )
}

pub fn counters() -> (u64, u64, u64) {
    (
        TOTAL_AUTO.load(Ordering::Relaxed),
        TOTAL_ABSTAIN.load(Ordering::Relaxed),
        TOTAL_ESCALATE.load(Ordering::Relaxed),
    )
}

pub fn status_line() -> alloc::string::String {
    let (a, ab, e) = counters();
    let n_sites = RELIABILITY.lock().n;
    alloc::format!(
        "[Decide] ver={} auto={} abstain={} escalate={} sites={} dirty={}",
        DECISION_CONTRACT_VERSION,
        a,
        ab,
        e,
        n_sites,
        PERSIST_DIRTY.load(Ordering::Acquire) as u32
    )
}

/// Snapshot JSONL das estatísticas por sítio (seed D4 / train_router).
pub fn export_labels_jsonl() -> alloc::string::String {
    let g = RELIABILITY.lock();
    let mut out = alloc::string::String::new();
    out.push_str(&alloc::format!(
        "{{\"type\":\"header\",\"contract\":{},\"provenance\":\"runtime_reliability\"}}\n",
        DECISION_CONTRACT_VERSION
    ));
    for i in 0..g.n {
        let s = &g.sites[i];
        out.push_str(&alloc::format!(
            "{{\"type\":\"site\",\"k\":\"{}\",\"ok\":{},\"bad\":{},\"abs\":{},\"esc\":{},\"trusted\":{},\"provenance\":\"hitl_or_auto\"}}\n",
            s.name(),
            s.auto_ok,
            s.auto_bad,
            s.abstain,
            s.escalate,
            s.trusted as u32
        ));
    }
    drop(g);
    // Exemplos utterance→expert (D4)
    let ring = LABEL_RING.lock();
    let n = ring.n.min(LABEL_CAP);
    let start = if ring.n > LABEL_CAP {
        ring.head
    } else {
        0
    };
    for i in 0..n {
        let idx = (start + i) % LABEL_CAP;
        let e = &ring.buf[idx];
        if e.text_len == 0 {
            continue;
        }
        let text = core::str::from_utf8(&e.text[..e.text_len as usize]).unwrap_or("");
        let expert = match e.expert {
            0 => "generator",
            1 => "hw_control",
            2 => "hw_identify",
            3 => "rust_coder",
            4 => "disk_diag",
            5 => "security",
            6 => "speech_synth",
            _ => "generator",
        };
        let prov = if e.hitl { "hitl" } else { "weak_auto" };
        // Escape mínimo de aspas
        let safe = text.replace('"', "'");
        out.push_str(&alloc::format!(
            "{{\"type\":\"example\",\"text\":\"{}\",\"expert\":\"{}\",\"provenance\":\"{}\"}}\n",
            safe,
            expert,
            prov
        ));
    }
    out
}

const LABEL_CAP: usize = 32;
const LABEL_TEXT: usize = 64;

#[derive(Clone, Copy)]
struct LabeledUtterance {
    text: [u8; LABEL_TEXT],
    text_len: u8,
    expert: u8,
    hitl: bool,
}

impl LabeledUtterance {
    const EMPTY: Self = Self {
        text: [0; LABEL_TEXT],
        text_len: 0,
        expert: 0,
        hitl: false,
    };
}

struct LabelRing {
    buf: [LabeledUtterance; LABEL_CAP],
    head: usize,
    n: usize,
}

impl LabelRing {
    const fn new() -> Self {
        Self {
            buf: [LabeledUtterance::EMPTY; LABEL_CAP],
            head: 0,
            n: 0,
        }
    }
}

static LABEL_RING: TicketLock<LabelRing> = TicketLock::new(LabelRing::new());

/// Grava utterance→expert (0..6 Trinity). `hitl=true` = correção operador.
pub fn note_labeled_utterance(text: &str, expert: u8, hitl: bool) {
    let expert = expert.min(6);
    let bytes = text.as_bytes();
    let len = bytes.len().min(LABEL_TEXT);
    let mut g = LABEL_RING.lock();
    let slot = g.head;
    g.buf[slot].text[..len].copy_from_slice(&bytes[..len]);
    g.buf[slot].text_len = len as u8;
    g.buf[slot].expert = expert;
    g.buf[slot].hitl = hitl;
    g.head = (g.head + 1) % LABEL_CAP;
    g.n = g.n.saturating_add(1);
    PERSIST_DIRTY.store(true, Ordering::Release);
}

pub fn labeled_count() -> usize {
    LABEL_RING.lock().n.min(LABEL_CAP)
}

/// Postura para HUD: 0=ok 1=warn 2=fail 3=n/a (mesmo código do pill Hub).
pub fn hub_posture_sev() -> u8 {
    let (auto, abs, esc) = counters();
    let total = auto.saturating_add(abs).saturating_add(esc);
    if total == 0 {
        return 3; // n/a
    }
    // escalate alto → fail; abstain dominante → warn; senão ok
    if esc * 2 > total {
        2
    } else if abs * 2 > total {
        1
    } else {
        0
    }
}

pub fn hub_posture_line() -> alloc::string::String {
    let (a, ab, e) = counters();
    let n = labeled_count();
    alloc::format!("a{} x{} e{} L{}", a, ab, e, n)
}

/// Persistência best-effort (Tickv). Sem backend = no-op honesto.
pub fn persist_reliability() -> bool {
    if !PERSIST_DIRTY.swap(false, Ordering::AcqRel) {
        return false;
    }
    let snap = {
        let g = RELIABILITY.lock();
        let mut out = alloc::string::String::from("{\"v\":1,\"sites\":[");
        for i in 0..g.n {
            let s = &g.sites[i];
            if i > 0 {
                out.push(',');
            }
            out.push_str(&alloc::format!(
                "{{\"k\":\"{}\",\"ok\":{},\"bad\":{},\"abs\":{},\"esc\":{},\"t\":{}}}",
                s.name(),
                s.auto_ok,
                s.auto_bad,
                s.abstain,
                s.escalate,
                s.trusted as u32
            ));
        }
        out.push_str("]}");
        out
    };
    match k_nano::storage::tickv::put_blob("ai/decisions/reliability", snap.as_bytes()) {
        Ok(()) => {
            k_nano::slog_cortex!("Decide", "ok", "persisted reliability {}B", snap.len());
            true
        }
        Err(e) => {
            PERSIST_DIRTY.store(true, Ordering::Release);
            k_nano::slog_cortex!("Decide", "warn", "persist skip: {}", e);
            false
        }
    }
}

pub fn hydrate_reliability() {
    match k_nano::storage::tickv::get_blob("ai/decisions/reliability") {
        Ok(data) => {
            // Parse mínimo: só loga presença; contadores runtime reiniciam
            // (JSON full parser = residual D4). Honesty: hydrated bytes.
            k_nano::slog_cortex!(
                "Decide",
                "ok",
                "hydrate reliability {}B (counters cold; ADR-0106 D0)",
                data.len()
            );
        }
        Err(_) => {
            k_nano::slog_cortex!("Decide", "ok", "hydrate none (fresh)");
        }
    }
}

/// θ pin HITL (clamp 64..=250) — bin/shell.
static THETA_PIN_AUTO: AtomicU32 = AtomicU32::new(0); // 0 = unset
static THETA_PIN_REVIEW: AtomicU32 = AtomicU32::new(0);

pub fn pin_theta(auto: u8, review: u8) {
    let a = auto.clamp(64, 250);
    let r = review.clamp(48, 240).min(a.saturating_sub(5));
    THETA_PIN_AUTO.store(a as u32, Ordering::Release);
    THETA_PIN_REVIEW.store(r as u32, Ordering::Release);
    k_nano::slog_cortex!("Decide", "ok", "theta pin auto={} review={}", a, r);
}

pub fn pinned_theta() -> Option<(u8, u8)> {
    let a = THETA_PIN_AUTO.load(Ordering::Acquire);
    if a == 0 {
        None
    } else {
        Some((a as u8, THETA_PIN_REVIEW.load(Ordering::Acquire) as u8))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn q8_dist_sums_near_255() {
        let w = [10u16, 20, 5];
        let d = Q8Dist::from_weights(&w);
        let s = d.sum();
        assert!(s >= 254 && s <= 255, "sum={}", s);
    }

    #[test]
    fn abstain_on_low_conf() {
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        enum T {
            A,
            B,
        }
        let opts = [T::A, T::B];
        let dist = Q8Dist::from_weights(&[3u16, 2]);
        let d = Decision::from_dist(&opts, dist, DecisionSource::Heuristic, 200, 10);
        assert!(d.choice.is_none());
        assert_eq!(d.abstain, Some(AbstainReason::LowConfidence));
    }

    #[test]
    fn pick_on_high_conf() {
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        enum T {
            A,
            B,
        }
        let opts = [T::A, T::B];
        let dist = Q8Dist::from_weights(&[90u16, 10]);
        let d = Decision::from_dist(&opts, dist, DecisionSource::Heuristic, 100, 20);
        assert_eq!(d.choice, Some(T::A));
        assert!(d.abstain.is_none());
    }

    #[test]
    fn noul_uncertain_abstains() {
        let n = Noul::from_p_yes(0.5, DecisionSource::Heuristic);
        assert!(n.decide(128).is_none());
    }

    #[test]
    fn contract_version_is_one() {
        assert_eq!(DECISION_CONTRACT_VERSION, 1);
    }
}
