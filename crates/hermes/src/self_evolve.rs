//! Sprint 108 — Self-Evolving Agents engine.
//! Ciclo: observe → generate → verify → register → improve → reflect.
//! Integra skill_gen, skill_observer e skill_loader sem hardcode de intents.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use crate::skill_gen;
use crate::skill_loader::SkillLoader;
use crate::skill_observer;

/// Tópico EventBus para insights de meta-cognição.
pub const TOPIC_SELF_EVOLVE: &str = "SELF_EVOLVE";
/// Pedido de geração LLM de skill (payload = prompt SKILL.md).
pub const TOPIC_SKILL_GEN_REQUEST: &str = "SKILL_GEN_REQUEST";
/// Notificação "fonte mudou sob você" (swarm): payload "what:name".
pub const TOPIC_CHANGE: &str = "CHANGE_NOTIFY";

/// Publica CHANGE_NOTIFY quando uma skill foi criada/alterada por outra fonte.
pub fn publish_change(what: &str, name: &str) {
    let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: String::from(TOPIC_CHANGE),
        payload: alloc::format!("{}:{}", what, name).into_bytes(),
        token: event_bus::CapabilityToken::Legacy(1),
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyVerdict {
    Ok,
    Reject(&'static str),
}

#[derive(Debug, Clone)]
pub struct SkillOutcome {
    pub name: String,
    pub ok: bool,
    pub ticks: u64,
}

#[derive(Debug, Clone)]
pub struct EvolveInsight {
    pub tick: u64,
    pub kind: String,
    pub detail: String,
}

struct EvolveState {
    /// Contagem de intents normalizados observados.
    intent_hits: BTreeMap<String, u32>,
    /// Skills auto-geradas nesta sessão.
    generated: Vec<String>,
    /// Histórico de sucesso/falha por skill.
    outcomes: Vec<SkillOutcome>,
    /// Insights de reflexão (meta-cognição).
    insights: Vec<EvolveInsight>,
    /// Skills que falharam e precisam de melhoria.
    improve_queue: Vec<String>,
    /// Último ciclo de reflect.
    last_reflect_tick: u64,
}

impl EvolveState {
    const fn new() -> Self {
        EvolveState {
            intent_hits: BTreeMap::new(),
            generated: Vec::new(),
            outcomes: Vec::new(),
            insights: Vec::new(),
            improve_queue: Vec::new(),
            last_reflect_tick: 0,
        }
    }
}

static STATE: Mutex<EvolveState> = Mutex::new(EvolveState::new());
static GENERATED_COUNT: AtomicU32 = AtomicU32::new(0);
static VERIFIED_OK: AtomicU32 = AtomicU32::new(0);
static VERIFIED_REJECT: AtomicU32 = AtomicU32::new(0);
static IMPROVE_COUNT: AtomicU32 = AtomicU32::new(0);
static REFLECT_COUNT: AtomicU32 = AtomicU32::new(0);
static CYCLE_COUNT: AtomicU64 = AtomicU64::new(0);

/// Normaliza texto de intent para chave de padrão (lowercase, espaços colapsados, max 48).
pub fn normalize_intent(text: &str) -> String {
    let mut out = String::new();
    let mut prev_space = true;
    for c in text.chars().take(96) {
        let lc = c.to_ascii_lowercase();
        if lc.is_ascii_alphanumeric() {
            out.push(lc);
            prev_space = false;
        } else if !prev_space {
            out.push('_');
            prev_space = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.len() > 48 {
        out.truncate(48);
    }
    if out.is_empty() {
        String::from("unknown")
    } else {
        out
    }
}

/// Observa um intent/user text — alimenta skill_gen + contadores.
pub fn observe_intent(text: &str, tick: u64) {
    let key = normalize_intent(text);
    if key == "unknown" || key.len() < 3 {
        return;
    }
    let steps = ["parse_intent", "route_hermes", "execute_or_chat"];
    skill_gen::record_task(&key, text, &steps);
    skill_observer::watch_task(&key, &steps, tick);

    let mut st = STATE.lock();
    let hits = st.intent_hits.entry(key.clone()).or_insert(0);
    *hits += 1;
    let n = *hits;
    drop(st);

    if n == 3 || n == 5 || (n > 5 && n % 10 == 0) {
        k_nano::slog_hermes!("S108", "info", "pattern '{}' hits={} — candidato auto-skill", key, n);
    }
}

/// Registra resultado de execução de skill (para self-improvement).
pub fn record_outcome(name: &str, ok: bool, tick: u64) {
    crate::skill_market::record_outcome("self_evolve", name, tick, ok);
    let mut st = STATE.lock();
    st.outcomes.push(SkillOutcome {
        name: String::from(name),
        ok,
        ticks: tick,
    });
    if st.outcomes.len() > 256 {
        st.outcomes.drain(0..64);
    }
    if !ok {
        if !st.improve_queue.iter().any(|s| s == name) {
            st.improve_queue.push(String::from(name));
        }
        skill_observer::watch_correction(
            name,
            "execution_failed",
            "Regenerate or tighten skill steps",
            "Failed skills must enter improve queue",
            tick,
        );
    }
}

/// Verificação ESTRITA ADR-0052 — delega ao gate único de artifact
/// (schema 1, kind skill, name sanitizado, goal/contexto/acionaveis/tokens/
/// provenance/sandbox_status, 7 seções `## ` obrigatórias, injection patterns,
/// content_hash e assinatura Ed25519 via k_nano::identity::verify_trusted).
/// Auto-skills entram no fleet apenas com o contrato completo.
pub fn verify_skill_md(content: &str) -> VerifyVerdict {
    match crate::package_hub::verify_artifact_md(crate::package_hub::PackageKind::Skill, content) {
        Ok(()) => VerifyVerdict::Ok,
        Err(reason) => VerifyVerdict::Reject(reason),
    }
}

/// Gera SKILL.md a partir de padrão observado (sem LLM).
pub fn generate_from_pattern(name: &str) -> Option<String> {
    if let Some(md) = skill_gen::generate_skill(name) {
        return Some(md);
    }
    // Fallback: observation-based
    let steps = ["observe", "plan", "act", "verify"];
    Some(skill_observer::generate_skill_md(name, &steps))
}

/// Prompt LLM para geração de skill (Hermes publica em TOPIC_LLM_REQUEST).
/// Instrui o contrato ADR-0052 completo: frontmatter + 7 seções obrigatórias.
/// content_hash/signature são adicionados por sign_artifact_md no registro.
pub fn llm_skill_prompt(name: &str, description: &str) -> String {
    format!(
        "Crie uma skill para o Neural OS Hermes (SKILL.md).\n\
         Nome: {}\nDescricao: {}\n\
         Formato OBRIGATORIO (contrato ADR-0052 — nao omita campos):\n\
         ---\nschema: 1\nkind: skill\nname: <nome a-z 0-9 _ ->\ndescription: <descricao>\n\
         contexto: <contexto em 1 linha>\n\
         acionaveis: [\"on_demand\"]\nrequired_tokens: [1]\n\
         provenance: hermes_created\nsandbox_status: none\n---\n\n\
         ## Contexto\n<1 linha>\n\n\
         ## Goal\n<objetivo>\n\n\
         ## Acionaveis\n- on_demand\n\n\
         ## Workflow\n1. <passo>\n2. <passo>\n\n\
         ## Pre-Flight\n- [ ] <check>\n\n\
         ## Success Criteria\n- [ ] <criterio>\n\n\
         ## Failure Policy\n- <politica>\n\n\
         Gere APENAS o bloco da skill, sem comentario extra.",
        name, description
    )
}

/// Verifica + registra no SkillLoader. Retorna Ok(nome) ou Err(motivo).
/// Ordem: SIGN FIRST → verificação ESTRITA do conteúdo selado → register.
/// Fail-closed: se assinar falhar, sealed==raw e a verificação estrita rejeita.
pub fn verify_and_register(loader: &mut SkillLoader, content: &str) -> Result<String, &'static str> {
    let tick = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
    // Sign FIRST (session key) — hash+assinatura são parte do contrato verificado.
    let sealed = crate::package_hub::sign_artifact_md(content)
        .unwrap_or_else(|_| String::from(content));
    match verify_skill_md(&sealed) {
        VerifyVerdict::Ok => {}
        VerifyVerdict::Reject(reason) => {
            VERIFIED_REJECT.fetch_add(1, Ordering::Relaxed);
            k_nano::slog_hermes!("S108", "VERIFY", "REJECT reason={}", reason);
            crate::globals::AUDIT_TRAIL.lock().push(
                tick,
                "self_evolve",
                "verify_reject",
                reason.as_bytes(),
            );
            return Err(reason);
        }
    }
    let name = extract_name(&sealed).unwrap_or_else(|| String::from("unnamed"));
    loader.remove_skill(&name);
    match loader.register_skill(&sealed) {
        Ok(()) => {
            VERIFIED_OK.fetch_add(1, Ordering::Relaxed);
            GENERATED_COUNT.fetch_add(1, Ordering::Relaxed);
            let mut st = STATE.lock();
            if !st.generated.iter().any(|g| g == &name) {
                st.generated.push(name.clone());
            }
            crate::globals::AUDIT_TRAIL.lock().push(
                tick,
                "self_evolve",
                "verify_ok",
                name.as_bytes(),
            );
            k_nano::slog_hermes!("S108", "info", "skill '{}' verified+registered", name);
            publish_change("skill", &name);
            Ok(name)
        }
        Err(e) => {
            VERIFIED_REJECT.fetch_add(1, Ordering::Relaxed);
            Err(e)
        }
    }
}

fn extract_name(content: &str) -> Option<String> {
    for line in content.lines() {
        if let Some(v) = line.strip_prefix("name: ") {
            let n = v.trim();
            if !n.is_empty() {
                return Some(String::from(n));
            }
        }
    }
    None
}

/// Tenta auto-gerar skills a partir de padrões com ≥3 usos.
/// Retorna quantas skills novas foram registradas.
pub fn auto_generate_pending(loader: &mut SkillLoader) -> u32 {
    let mut created = 0u32;
    let candidates: Vec<(String, u32)> = {
        let st = STATE.lock();
        st.intent_hits
            .iter()
            .filter(|(_, &c)| c >= 3)
            .map(|(k, &c)| (k.clone(), c))
            .collect()
    };
    for (name, hits) in candidates {
        if loader.list_skills().iter().any(|(n, _, _)| n == &name) {
            continue;
        }
        if let Some(md) = skill_gen::maybe_auto_skill(&name).or_else(|| generate_from_pattern(&name))
        {
            match verify_and_register(loader, &md) {
                Ok(_) => {
                    created += 1;
                    k_nano::slog_hermes!("S108", "GEN", "auto-skill '{}' from {} hits", name, hits);
                }
                Err(e) => k_nano::slog_hermes!("S108", "GEN", "fail '{}': {}", name, e),
            }
        }
    }
    // Processar observações abertas (cron/review path)
    for obs in skill_observer::pending_observations() {
        if !obs.skill.starts_with("New skill candidate:") {
            continue;
        }
        let name = obs
            .skill
            .trim_start_matches("New skill candidate:")
            .trim();
        if name.is_empty() {
            continue;
        }
        if loader.list_skills().iter().any(|(n, _, _)| n == name) {
            skill_observer::mark_actioned(obs.number);
            continue;
        }
        if let Some(md) = generate_from_pattern(name) {
            if verify_and_register(loader, &md).is_ok() {
                created += 1;
                skill_observer::mark_actioned(obs.number);
            }
        }
    }
    created
}

/// Self-improvement: regenera skills na fila de falhas.
pub fn improve_failed(loader: &mut SkillLoader) -> u32 {
    let queue: Vec<String> = {
        let mut st = STATE.lock();
        let q = st.improve_queue.clone();
        st.improve_queue.clear();
        q
    };
    let mut n = 0u32;
    for name in queue {
        let md = skill_observer::generate_skill_md(
            &name,
            &[
                "diagnose_previous_failure",
                "apply_corrected_steps",
                "verify_output",
                "report_status",
            ],
        );
        // reforça description
        let md = md.replacen(
            "description: Auto-generated from observation",
            &format!("description: Improved after failure — {}", name),
            1,
        );
        match verify_and_register(loader, &md) {
            Ok(_) => {
                n += 1;
                IMPROVE_COUNT.fetch_add(1, Ordering::Relaxed);
                k_nano::slog_hermes!("S108", "IMPROVE", "skill '{}' regenerated", name);
            }
            Err(e) => k_nano::slog_hermes!("S108", "IMPROVE", "fail '{}': {}", name, e),
        }
    }
    n
}

/// Meta-cognição / reflexão: resume estado e publica insight.
pub fn reflect(tick: u64) -> String {
    let mut st = STATE.lock();
    st.last_reflect_tick = tick;
    let gen = st.generated.len();
    let intents = st.intent_hits.len();
    let outcomes = st.outcomes.len();
    let ok = st.outcomes.iter().filter(|o| o.ok).count();
    let fail = outcomes.saturating_sub(ok);
    let pending_improve = st.improve_queue.len();
    let detail = format!(
        "intents={} generated={} outcomes={}/{}ok fail={} improve_q={} verify_ok={} verify_rej={} improve_total={}",
        intents,
        gen,
        ok,
        outcomes,
        fail,
        pending_improve,
        VERIFIED_OK.load(Ordering::Relaxed),
        VERIFIED_REJECT.load(Ordering::Relaxed),
        IMPROVE_COUNT.load(Ordering::Relaxed),
    );
    st.insights.push(EvolveInsight {
        tick,
        kind: String::from("reflect"),
        detail: detail.clone(),
    });
    if st.insights.len() > 64 {
        st.insights.drain(0..16);
    }
    drop(st);
    REFLECT_COUNT.fetch_add(1, Ordering::Relaxed);
    CYCLE_COUNT.fetch_add(1, Ordering::Relaxed);
    k_nano::slog_hermes!("S108", "REFLECT", "{}", detail);
    detail
}

/// Um ciclo completo do engine (chamado pelo SelfEvolveAgent).
pub fn tick_cycle(loader: &mut SkillLoader, tick: u64) -> u32 {
    let mut work = 0u32;
    work += auto_generate_pending(loader);
    work += improve_failed(loader);
    // Reflect a cada ~2000 ticks de timer (caller decide); aqui só se pedido via flag
    let _ = tick;
    work
}

/// Status serial one-liner.
pub fn status_line() -> String {
    format!(
        "[S108] cycles={} gen={} verify_ok={} reject={} improve={} reflect={} patterns={}",
        CYCLE_COUNT.load(Ordering::Relaxed),
        GENERATED_COUNT.load(Ordering::Relaxed),
        VERIFIED_OK.load(Ordering::Relaxed),
        VERIFIED_REJECT.load(Ordering::Relaxed),
        IMPROVE_COUNT.load(Ordering::Relaxed),
        REFLECT_COUNT.load(Ordering::Relaxed),
        STATE.lock().intent_hits.len(),
    )
}

/// Últimos insights (para shell / consciousness).
pub fn recent_insights(max: usize) -> Vec<EvolveInsight> {
    let st = STATE.lock();
    let n = st.insights.len();
    let start = n.saturating_sub(max);
    st.insights[start..].to_vec()
}

/// Re-export helper: verification only (sem registrar).
pub fn verify_only(content: &str) -> VerifyVerdict {
    verify_skill_md(content)
}

/// Contadores para telemetria Consciousness.
pub fn counters() -> (u32, u32, u32, u32, u32) {
    (
        GENERATED_COUNT.load(Ordering::Relaxed),
        VERIFIED_OK.load(Ordering::Relaxed),
        VERIFIED_REJECT.load(Ordering::Relaxed),
        IMPROVE_COUNT.load(Ordering::Relaxed),
        REFLECT_COUNT.load(Ordering::Relaxed),
    )
}

#[cfg(test)]
mod tests {
    // no_std: sem testes std; verificação via QEMU serial.
}






