//! Cognitive Bridge — Hermes orquestra Cortex+Jarbas com stack AI superior ao HANR.
//! Une: SOUL/PERSONA/MEMORY, BGE RAG, Trinity hint, LatentBus, session search,
//! IterationBudget, capability gating, memory nudges HITL.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU16, AtomicU32, Ordering};
use lazy_static::lazy_static;
use spin::Mutex;
use ticket_lock::TicketLock;

use crate::memory_store;
use k_nano::globals::EVENT_BUS;

pub const TOPIC_MEMORY_NUDGE: &str = "MEMORY_NUDGE";
pub const TOPIC_COG_STATUS: &str = "COG_STATUS";

/// Ponte de guarda do Hermes: detecta se a mensagem e sobre criacao de skill.
/// Se for, o Hermes DEVE garantir que o skill_writer esteja no contexto do LLM.
/// Usado no Chat handler do HermesAgent como pre-flight check.
pub fn is_skill_creation_request(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("cria") && lower.contains("skill")
        || lower.contains("create") && lower.contains("skill")
        || lower.contains("novo skill")
        || lower.contains("new skill")
        || lower.contains("/add_skill")
        || lower.contains("/learn")
        || lower.contains("registra") && lower.contains("skill")
        || lower.contains("register") && lower.contains("skill")
}

// ─── IterationBudget (HANR-class, bare-metal) ─────────────────────────────

static BUDGET_MAX: AtomicU16 = AtomicU16::new(12);
static BUDGET_LEFT: AtomicU16 = AtomicU16::new(12);
static BUDGET_GRACE: AtomicU16 = AtomicU16::new(0);
static BUDGET_EXHAUSTED: AtomicU32 = AtomicU32::new(0);

pub fn budget_reset() {
    BUDGET_LEFT.store(BUDGET_MAX.load(Ordering::Relaxed), Ordering::Relaxed);
    BUDGET_GRACE.store(0, Ordering::Relaxed);
}

pub fn budget_set_max(n: u16) {
    BUDGET_MAX.store(n.max(1).min(64), Ordering::Relaxed);
    budget_reset();
}

/// Consome 1 ciclo. Se esgotar, concede 1 grace. Depois força done.
pub fn budget_tick() -> BudgetVerdict {
    let left = BUDGET_LEFT.load(Ordering::Relaxed);
    if left > 0 {
        BUDGET_LEFT.fetch_sub(1, Ordering::Relaxed);
        return BudgetVerdict::Continue;
    }
    if BUDGET_GRACE.swap(1, Ordering::Relaxed) == 0 {
        return BudgetVerdict::Grace;
    }
    BUDGET_EXHAUSTED.fetch_add(1, Ordering::Relaxed);
    BudgetVerdict::Exhausted
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetVerdict {
    Continue,
    Grace,
    Exhausted,
}

pub fn budget_status() -> String {
    format!(
        "[BUDGET] left={}/{} grace_used={} exhausted={}",
        BUDGET_LEFT.load(Ordering::Relaxed),
        BUDGET_MAX.load(Ordering::Relaxed),
        BUDGET_GRACE.load(Ordering::Relaxed),
        BUDGET_EXHAUSTED.load(Ordering::Relaxed)
    )
}

// ─── Session search (FTS5-lite: ring + substring) ─────────────────────────

const SESSION_CAP: usize = 48;

struct SessionEntry {
    tick: u64,
    role: String,
    text: String,
}

struct SessionLog {
    entries: Vec<SessionEntry>,
}

impl SessionLog {
    const fn new() -> Self {
        SessionLog {
            entries: Vec::new(),
        }
    }
}

lazy_static! {
    static ref SESSION: Mutex<SessionLog> = Mutex::new(SessionLog::new());
    static ref NUDGE_QUEUE: TicketLock<Vec<String>> = TicketLock::new(Vec::new());
}

pub fn session_record(role: &str, text: &str, tick: u64) {
    let mut log = SESSION.lock();
    log.entries.push(SessionEntry {
        tick,
        role: String::from(role),
        text: memory_store::clamp_public(text, 240),
    });
    if log.entries.len() > SESSION_CAP {
        let drain = log.entries.len() - SESSION_CAP;
        log.entries.drain(0..drain);
    }
    // Persistência leve: append SESSION.log
    let mut prev = crate::globals::read_vfs("/mnt/neural/SESSION.log").unwrap_or_default();
    if prev.len() > 16_000 {
        prev = prev[prev.len() - 12_000..].to_vec();
    }
    let line = format!("[{}] {}: {}\n", tick, role, memory_store::clamp_public(text, 120));
    prev.extend_from_slice(line.as_bytes());
    let _ = crate::globals::write_vfs("/mnt/neural/SESSION.log", &prev);
}

pub fn session_search(query: &str, top_k: usize) -> String {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return String::from("[SESSION] search <query>");
    }
    let log = SESSION.lock();
    let mut hits: Vec<&SessionEntry> = log
        .entries
        .iter()
        .filter(|e| e.text.to_ascii_lowercase().contains(&q) || e.role.contains(&q))
        .collect();
    hits.reverse();
    hits.truncate(top_k.max(1).min(16));
    if hits.is_empty() {
        // Fallback BGE se disponível
        let sem = k_ai::memory_systems::semantic_search(query, top_k.min(5));
        if sem.is_empty() {
            return format!("[SESSION] no hits for '{}'", q);
        }
        let mut s = format!("[SESSION+BGE] '{}'\n", q);
        for (label, score) in sem {
            s.push_str(&format!("  {:.2} {}\n", score, label));
        }
        return s;
    }
    let mut s = format!("[SESSION] {} hit(s) for '{}'\n", hits.len(), q);
    for e in hits {
        s.push_str(&format!("  @{} [{}] {}\n", e.tick, e.role, e.text));
    }
    s
}

// ─── Memory nudge (HANR closed-loop, superior: HITL Jarbas) ───────────────

pub fn propose_memory_nudge(fact: &str) {
    let fact = fact.trim();
    if fact.is_empty() {
        return;
    }
    {
        let mut q = NUDGE_QUEUE.lock();
        if q.iter().any(|f| f == fact) {
            return;
        }
        q.push(String::from(fact));
        if q.len() > 16 {
            q.remove(0);
        }
    }
    let msg = format!(
        "[MEMORY NUDGE] Persistir?\n- {}\n/remember {}   ou ignore\n",
        fact, fact
    );
    let _ = EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: String::from(TOPIC_MEMORY_NUDGE),
        payload: msg.as_bytes().to_vec(),
        token: event_bus::CapabilityToken::Legacy(1),
    });
    // Superfície: Jarbas via HERMES_RESPONSE
    let _ = EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: String::from(crate::hermes::TOPIC_HERMES_RESPONSE),
        payload: msg.into_bytes(),
        token: event_bus::CapabilityToken::Legacy(1),
    });
}

pub fn reflect_and_nudge(tick: u64) -> String {
    // Extrai candidatos do session log recente
    let log = SESSION.lock();
    let mut candidates = Vec::new();
    for e in log.entries.iter().rev().take(8) {
        if e.role == "user" && e.text.len() > 12 {
            let lower = e.text.to_ascii_lowercase();
            if lower.contains("meu ")
                || lower.contains("prefiro")
                || lower.contains("sempre")
                || lower.contains("nunca")
                || lower.contains("chamo")
            {
                candidates.push(e.text.clone());
            }
        }
    }
    drop(log);
    if let Some(c) = candidates.first() {
        propose_memory_nudge(c);
        format!("[REFLECT] nudge proposed @{}: {}", tick, memory_store::clamp_public(c, 60))
    } else {
        format!("[REFLECT] no nudge @{}", tick)
    }
}

// ─── Capability gating (requires / fallback) ──────────────────────────────

pub fn caps_available() -> Vec<&'static str> {
    let mut c = Vec::from(["eventbus", "skills", "package_hub", "trust"]);
    if k_nano::identity::session_ready() {
        c.push("session_sign");
    }
    // Net: heurística — se netdiag não panic
    c.push("net_optional");
    c.push("wasm");
    c.push("voice_optional");
    c.push("display");
    c
}

/// Retorna true se skill pode aparecer no L0 (requires satisfeitos).
pub fn skill_visible(body: &str) -> bool {
    let requires = extract_list_field(body, "capabilities")
        .or_else(|| extract_list_field(body, "requires"));
    let Some(reqs) = requires else {
        return true;
    };
    let avail = caps_available();
    for r in reqs {
        let r = r.to_ascii_lowercase();
        if r.is_empty() {
            continue;
        }
        if !avail.iter().any(|a| *a == r || a.starts_with(r.as_str())) {
            // fallback_for: se declarado, ainda mostra
            if extract_list_field(body, "fallback_for").is_some() {
                return true;
            }
            return false;
        }
    }
    true
}

fn extract_list_field(body: &str, key: &str) -> Option<Vec<String>> {
    let prefix = format!("{}:", key);
    for line in body.lines() {
        if let Some(v) = line.trim().strip_prefix(&prefix) {
            let inner = v
                .trim()
                .trim_start_matches('[')
                .trim_end_matches(']')
                .trim_matches('"');
            let list: Vec<String> = inner
                .split(',')
                .map(|s| String::from(s.trim().trim_matches('"')))
                .filter(|s| !s.is_empty())
                .collect();
            return Some(list);
        }
    }
    None
}

// ─── Direcionamento Trinity→Trust→Skill/LLM (superior ao HANR tool-soup) ───

/// Ação de roteamento após Trinity + Trust.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteKind {
    /// SpeechSynth — não passa pelo LLM
    Tts,
    /// Skill nativa preferida pelo expert MoE
    ExpertSkill,
    /// Cortex.think já resolveu skill estruturada
    Structured,
    /// Generator / chat → LLM (exige Trust)
    Llm,
    /// Token não-boot: HITL Escalate para llm_generate
    EscalateLlm,
    /// Trust negou llm_generate
    DenyTrust,
}

#[derive(Debug, Clone)]
pub struct RouteDecision {
    pub expert: &'static str,
    pub kind: RouteKind,
    pub skill: Option<&'static str>,
    pub emotion: &'static str,
    pub moe_loaded: bool,
    pub reason: String,
    /// Id ApprovalGate quando kind == EscalateLlm
    pub approval_id: Option<u64>,
}

/// Emotion lite (sem dependência Jarbas — evita ciclo hermes↔jarbas).
pub fn emotion_hint(text: &str) -> &'static str {
    let l = text.to_ascii_lowercase();
    if l.contains("obrigad")
        || l.contains("feliz")
        || l.contains("otimo")
        || l.contains("ótimo")
        || l.contains("ador")
    {
        "joy"
    } else if l.contains("raiva") || l.contains("irritad") || l.contains("odei") {
        "anger"
    } else if l.contains("trist") || l.contains("pena") {
        "sadness"
    } else if l.contains("medo") || l.contains("perigo") {
        "fear"
    } else if l.contains('?') {
        "curious"
    } else {
        "neutral"
    }
}

/// Skill nativa preferida pelo expert Trinity (antes de cair no LLM).
/// Sem fallback genérico para system_status — skills reais disk_diag/security.
pub fn prefer_expert_skill(expert: &str) -> Option<&'static [&'static str]> {
    match expert {
        "hw_control" => Some(&["audio_set_volume"]),
        "hw_identify" => Some(&["hw_identify", "hardware_info"]),
        "disk_diag" => Some(&["disk_diag"]),
        "security" => Some(&["security"]),
        "rust_coder" => None, // código → LLM + Trinity hint no prompt
        "speech_synth" => None,
        "generator" => None,
        _ => None,
    }
}

/// TTL curto para boot console (token 1). Aprovação HITL usa TTL maior.
pub const LLM_BOOT_TTL: u64 = 300;
pub const LLM_APPROVED_TTL: u64 = 1800;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmGate {
    Allow,
    /// Pediu HITL Escalate — id da ApprovalGate
    Escalate(u64),
    Deny,
}

/// Trust no path LLM — superior ao HANR: token não-boot exige Escalate.
pub fn llm_gate(token: u64, now: u64) -> LlmGate {
    {
        let tc = crate::globals::TRUST_CACHE.lock();
        if tc.is_trusted(token, "llm_generate", now)
            || tc.is_trusted_agent(token, "cortex", "llm_generate", now)
        {
            return LlmGate::Allow;
        }
    }
    // Só o console boot (Legacy 1) auto-cacheia — TTL curto, sem session_ready bypass
    if token == 1 {
        let mut tc = crate::globals::TRUST_CACHE.lock();
        tc.check_or_cache(token, "llm_generate", now, LLM_BOOT_TTL);
        tc.check_or_cache_agent(token, "cortex", "llm_generate", now, LLM_BOOT_TTL);
        return LlmGate::Allow;
    }
    // Não-boot: Escalate HITL (Jarbas/terminal). session_ready ≠ trust LLM.
    let id = crate::globals::APPROVAL_GATE.lock().request(
        "llm_generate",
        "cortex",
        "LLM generate: token nao-boot requer /approve",
        crate::approval::ApprovalLevel::Escalate,
    );
    LlmGate::Escalate(id)
}

/// Compat: true só se Allow imediato (sem abrir HITL).
pub fn llm_allowed(token: u64, now: u64) -> Result<(), &'static str> {
    match llm_gate(token, now) {
        LlmGate::Allow => Ok(()),
        LlmGate::Escalate(_) => Err("Trust: llm_generate pending HITL — /approve <id>"),
        LlmGate::Deny => Err("Trust deny llm_generate"),
    }
}

/// Após /approve de llm_generate — concede trust com TTL aprovado.
pub fn grant_llm_after_approve(token: u64, now: u64) {
    let mut tc = crate::globals::TRUST_CACHE.lock();
    tc.check_or_cache(token, "llm_generate", now, LLM_APPROVED_TTL);
    tc.check_or_cache_agent(token, "cortex", "llm_generate", now, LLM_APPROVED_TTL);
    tc.trust_allow(token, "llm_generate", now);
    tc.trust_allow_agent(token, "cortex", "llm_generate", now);
}

/// Classifica uma vez na Trinity e encaminha a decisão para o roteador.
pub fn route_user_intent(
    msg: &str,
    token: u64,
    now: u64,
    structured_skill: Option<&'static str>,
) -> RouteDecision {
    let (expert, moe_loaded) = {
        let t = crate::globals::TRINITY.lock();
        let e = t.classify_intent(msg);
        (e.name, t.moe_router_loaded())
    };
    route_classified_user_intent(
        msg,
        token,
        now,
        expert,
        moe_loaded,
        structured_skill,
    )
}

/// Roteia uma classificação Trinity já calculada.
///
/// O bin usa esta entrada para reaproveitar o expert e o trace R3, evitando
/// classificar o mesmo texto duas vezes. Intents estruturados do Cortex têm
/// precedência sobre heurísticas de expert.
pub fn route_classified_user_intent(
    msg: &str,
    token: u64,
    now: u64,
    expert: &'static str,
    moe_loaded: bool,
    structured_skill: Option<&'static str>,
) -> RouteDecision {
    let emotion = emotion_hint(msg);

    if let Some(skill) = structured_skill {
        return RouteDecision {
            expert,
            kind: RouteKind::Structured,
            skill: Some(skill),
            emotion,
            moe_loaded,
            reason: format!("Cortex.think→structured '{}' (antes do expert)", skill),
            approval_id: None,
        };
    }

    if expert == "speech_synth" {
        return RouteDecision {
            expert,
            kind: RouteKind::Tts,
            skill: None,
            emotion,
            moe_loaded,
            reason: String::from("Trinity→TTS (sem LLM)"),
            approval_id: None,
        };
    }

    // Prefer skill nativa do expert se existir no registry
    if let Some(cands) = prefer_expert_skill(expert) {
        let reg = crate::globals::SKILL_REGISTRY.lock();
        for &sk in cands {
            if reg.has_skill(sk) {
                return RouteDecision {
                    expert,
                    kind: RouteKind::ExpertSkill,
                    skill: Some(sk),
                    emotion,
                    moe_loaded,
                    reason: format!("Trinity→expert_skill '{}' (antes do LLM)", sk),
                    approval_id: None,
                };
            }
        }
    }

    // LLM path — Trust gate (boot TTL curto; não-boot → Escalate HITL)
    match llm_gate(token, now) {
        LlmGate::Allow => RouteDecision {
            expert,
            kind: RouteKind::Llm,
            skill: None,
            emotion,
            moe_loaded,
            reason: String::from("Trinity→LLM (Trust ok)"),
            approval_id: None,
        },
        LlmGate::Escalate(id) => RouteDecision {
            expert,
            kind: RouteKind::EscalateLlm,
            skill: None,
            emotion,
            moe_loaded,
            reason: format!(
                "Trinity→LLM Escalate #{} — /approve {} (token nao-boot)",
                id, id
            ),
            approval_id: Some(id),
        },
        LlmGate::Deny => RouteDecision {
            expert,
            kind: RouteKind::DenyTrust,
            skill: None,
            emotion,
            moe_loaded,
            reason: String::from("Trust deny llm_generate"),
            approval_id: None,
        },
    }
}

static LAST_ROUTE: Mutex<Option<String>> = Mutex::new(None);

pub fn note_route(d: &RouteDecision) {
    *LAST_ROUTE.lock() = Some(format!(
        "{}→{:?} moe={} emo={}",
        d.expert, d.kind, d.moe_loaded, d.emotion
    ));
}

// ─── Prompt enriquecido Cortex (superior ao HANR context dump) ─────────────

pub fn cortex_system_prompt(user_intent: &str) -> String {
    let mut s = String::with_capacity(3500);
    s.push_str("[NEURAL-OS COGNITIVE CONTEXT — K²CHJ]\n");
    s.push_str(&memory_store::prompt_slice());
    s.push('\n');

    // Emotion + PERSONA reminder (Jarbas aplica tom; Cortex não imita PERSONA)
    let emo = emotion_hint(user_intent);
    s.push_str(&format!(
        "[AFFECT] user_emotion={} | PERSONA=Jarbas-only (não orquestrar por tom)\n",
        emo
    ));

    // RAG context: BQ L4 + texto formatado (embedding BGE ou pseudo sobre SGDB real)
    let mut recall_path = "rag";
    let (q_emb, emb_path) = k_ai::memory_systems::embed_or_pseudo(user_intent);
    let rag = k_ai::sgdb::rag_context(&q_emb, 5);
    if !rag.is_empty() {
        s.push_str(&rag);
        s.push('\n');
    } else {
        recall_path = "no_rag";
    }
    k_nano::slog_bin!("sgdb", "recall", "path={} emb={}", recall_path, emb_path);

    // ADR-0063 F6: working/episodic MemoryDoc (L1/L2)
    let sgdb = k_ai::sgdb::prompt_slice(400);
    if !sgdb.is_empty() {
        s.push_str(&sgdb);
        s.push('\n');
    }

    // Trinity hint + MoE status
    {
        let trinity = crate::globals::TRINITY.lock();
        let expert = trinity.classify_intent(user_intent);
        s.push_str(&format!(
            "[TRINITY] route→{} ({}) moe_router={}\n",
            expert.name,
            expert.description,
            if trinity.moe_router_loaded() {
                "LOADED"
            } else {
                "keyword+R3"
            }
        ));
    }

    // Skills L0 gated
    s.push_str(&memory_store::skills_l0_gated());
    s.push('\n');

    // Latent thought snip (se houver drain recente)
    if let Some(thought) = last_latent_snip() {
        s.push_str(&format!("[LATENT] {}\n", thought));
    }

    if let Some(r) = LAST_ROUTE.lock().clone() {
        s.push_str(&format!("[LAST-ROUTE] {}\n", r));
    }

    s.push_str(&budget_status());
    s.push('\n');
    s.push_str(
        "Instruções: responda em PT-BR, preciso e curto. Use skills só se L0 indicar. \
         Se precisar HITL diga /approve. Não invente HW. Trust/CapGate já filtraram o path.\n",
    );
    // Truncate soft
    if s.len() > 3200 {
        s.truncate(3190);
        s.push_str("…\n");
    }
    s
}

static LAST_LATENT: Mutex<Option<String>> = Mutex::new(None);

pub fn note_latent(text: &str) {
    *LAST_LATENT.lock() = Some(memory_store::clamp_public(text, 80));
}

fn last_latent_snip() -> Option<String> {
    LAST_LATENT.lock().clone()
}

pub fn after_exchange(user: &str, response: &str, tick: u64) {
    session_record("user", user, tick);
    session_record("assistant", response, tick);
    // ADR-0063: texto + embedding com chave temporal (RAG via SGDB TicKV+NoProto+BQ)
    let (emb_u, path_u) = k_ai::memory_systems::embed_or_pseudo(user);
    let (emb_a, path_a) = k_ai::memory_systems::embed_or_pseudo(response);
    k_ai::sgdb::remember_exchange_full(user, response, &emb_u, &emb_a, tick);
    k_nano::slog_bin!("sgdb", "emb", "user={} asst={}", path_u, path_a);
    // Audit flush periódico (cada exchange — compacto)
    crate::globals::AUDIT_TRAIL.lock().flush_to_sgdb();
    // BGE paralelo (embeddings residual)
    k_ai::memory_systems::index_embedding("user", user);
    k_ai::memory_systems::index_embedding("assistant", response);
    budget_reset();
}

/// Extract recent (user, assistant) Q&A pairs from the session log for DREAM replay.
/// Returns at most `max_pairs` pairs by matching consecutive user→assistant entries.
pub fn extract_qa_pairs(max_pairs: usize) -> Vec<(String, String)> {
    let log = SESSION.lock();
    let mut pairs = Vec::new();
    let mut i = log.entries.len();
    while i >= 2 && pairs.len() < max_pairs {
        i -= 1;
        if log.entries[i].role == "assistant" && i > 0 && log.entries[i - 1].role == "user" {
            let user = log.entries[i - 1].text.clone();
            let asst = log.entries[i].text.clone();
            pairs.push((user, asst));
            if i > 0 { i -= 1; } // skip the user entry we just consumed
        }
    }
    pairs
}

/// Returns the number of entries in the session log (IDEA #314e confidence tracking).
pub fn session_len() -> u64 {
    SESSION.lock().entries.len() as u64
}

pub fn status_line() -> String {
    format!(
        "{} | session_n={} | nudges={} | route={} | {}",
        budget_status(),
        SESSION.lock().entries.len(),
        NUDGE_QUEUE.lock().len(),
        LAST_ROUTE
            .lock()
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("-"),
        k_ai::memory_systems::bge_status()
    )
}






