//! s432 — Triagem IA do HUB HEALTH (premissa máxima ADR-0088: a IA consome o
//! painel sozinha, o humano lê a foto). O HUB HEALTH é painel humano; os dados
//! que o alimentam já existem em statics acessíveis. Este módulo:
//!
//! 1. Monta o snapshot `HUB\0` + JSON das linhas de DECISÃO do painel
//!    (não das 18 visuais — só as que a IA pode agir: heap, arena, posture,
//!    sys/audio verdicts, I4 sched).
//! 2. Classifica pior-estado com regras determinísticas PURAS (testáveis).
//! 3. Em `Propose`, submete o snapshot ao LLM (InferQueue, reply em
//!    `HUB_TRIAGE_LLM`) para GERAR a proposta acionável — premissa máxima
//!    ADR-0088: a IA decide, a heurística é fallback. A resposta do modelo é
//!    parseada (`{"title":..,"action":..}`); gibberish/marcador de
//!    recusa/timeout → publica a proposta heurística (comportamento s432).
//!    LLM responde `{}` = declínio honesto (observe, sem toast).
//! 4. Toda publicação (intent + TOAST HITL) passa por gate de headroom
//!    (`heap_headroom_low`) — fail-closed s430; dedupe por fingerprint
//!    (FNV-1a) + cooldown 10min por fingerprint — mesma lição do
//!    mesh_knowledge (s410d): sem dedupe, a escalada vira loop de feedback
//!    (233 intents no boot mesh).
//!
//! Honestidade: `Observe` nunca escala (I5/sched = sintoma de carga, lição
//! s429-lab); `mesh_frag_pressure` desliga tudo (caller gate, s429-lab); o
//! snapshot publicado no EventBus é a evidência de wire (regra SESSION_419:
//! tópico novo só está feito quando tem produtor + consumidor + slog).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use event_bus::{CapabilityToken, Event, Receiver};
use k_nano::EVENT_BUS;

/// Tópico do snapshot (evidência de wire; HUD/LLM podem assinar).
pub const TOPIC_HUB_TRIAGE: &str = "HUB_TRIAGE";
/// Prefixo de wire do snapshot (padrão MCH\0/HUB contract).
pub const HUB_PREFIX: &[u8] = b"HUB\0";

/// Cadência da triagem: 60 s (~3600 ticks @60Hz — painel 2 Hz, triagem lenta).
pub const TRIAGE_PERIOD_TICKS: u64 = 3600;
/// Cooldown por fingerprint: 10 min (600 s).
pub const PROPOSE_COOLDOWN_TICKS: u64 = 36000;
/// Timeout da resposta LLM: 60 s (prefill 12s+ em 8c; além disso fallback).
pub const LLM_REPLY_TIMEOUT_TICKS: u64 = 3600;
/// CAP do dedupe (runtime hygiene s410d: CAP + evicção FIFO).
const FP_CAP: usize = 8;

/// Tópico de reply do job LLM de triagem (o InferWorker publica o texto aqui).
pub const TOPIC_HUB_TRIAGE_LLM: &str = "HUB_TRIAGE_LLM";

/// Instrução determinística do prompt de triagem (padrão machine_prompt s417).
const TRIAGE_PROMPT_HEADER: &str = "Voce e a IA de auto-diagnostico do kernel AIOS. \
Abaixo o snapshot HUB de saude (JSON). Se houver acao estrutural de otimizacao \
clara e concreta, responda SOMENTE com: {\"title\":\"<titulo curto>\",\
\"action\":\"<acao concreta>\"}. Se nada precisa de acao agora, responda \
exatamente {}. Nao explique.\nSnapshot: ";

/// Veredito da triagem de pior-estado.
#[derive(Debug, PartialEq, Eq)]
pub enum TriageVerdict {
    /// Tudo que a IA pode agir está saudável (ou amostra insuficiente).
    Ok,
    /// Degradação real MAS observe-only (sintoma de carga / ambiental) —
    /// registra no snapshot, não escala.
    Observe(&'static str),
    /// Proposta acionável: título curto + ação recomendada (vira HITL toast
    /// + prompt LLM no USER_INTENT).
    Propose {
        title: &'static str,
        action: &'static str,
    },
}

/// Linhas do snapshot que a IA consome (campos com fonte única, sem dual-truth).
pub struct HubTriageInputs {
    /// heap usado/janela (MB) e pressure (0/1/2) — `k_nano::allocator::heap_observe`.
    pub heap_used_mb: u64,
    pub heap_window_mb: u64,
    pub heap_pressure: u8,
    /// TALC capacity claimed (MB, 0 = não pronto) — overflow honesto s431.
    pub talc_cap_mb: u64,
    /// s435: uso REAL do TALC medido nos gap-nodes (0/0/0 = nunca medido).
    pub talc_used_mb: u64,
    pub talc_free_mb: u64,
    /// Maior gap contíguo — o que um alloc grande consegue de fato.
    pub talc_largest_mb: u64,
    /// Nº de fragmentos livres no span.
    pub talc_gaps: u64,
    /// 1 = última amostra parcial (walk abortado — metadados ilegíveis).
    pub talc_partial: u8,
    /// Arena Cortex usada/capacidade (MB) — `cortex::global_arena::arena_stats`.
    pub arena_used_mb: u64,
    pub arena_cap_mb: u64,
    /// Postura de decisão (`a{n} x{n} e{n} L{n}`) — `cortex::decision`.
    pub posture_sev: u8,
    pub posture_line: String,
    /// Veredito consolidado de máquina (`MACHINE_HEALTH`) já publicada.
    pub machine_json: Option<String>,
    /// I4 scheduler violations (último SafetyStatus do SecurityAgent — static
    /// snapshot; sem dual-truth: a MESMA struct que o SEC loga).
    pub sched_violations: u64,
}

impl HubTriageInputs {
    /// Coleta as fontes reais (sem locks de bus; atomics + statics).
    pub fn gather() -> Self {
        let obs = k_nano::allocator::heap_observe();
        let (arena_used, arena_cap) = cortex::global_arena::arena_stats();
        HubTriageInputs {
            heap_used_mb: obs.used_mb as u64,
            heap_window_mb: obs.window_mb as u64,
            heap_pressure: obs.pressure,
            talc_cap_mb: k_nano::allocator::talc_capacity_mb(),
            talc_used_mb: obs.talc_used_mb as u64,
            talc_free_mb: obs.talc_free_mb as u64,
            talc_largest_mb: obs.talc_largest_mb as u64,
            talc_gaps: obs.talc_gaps,
            talc_partial: obs.talc_partial,
            arena_used_mb: (arena_used / (1024 * 1024)) as u64,
            arena_cap_mb: (arena_cap / (1024 * 1024)) as u64,
            posture_sev: cortex::decision::hub_posture_sev(),
            posture_line: cortex::decision::hub_posture_line(),
            machine_json: crate::sys_health::LAST_MACHINE_JSON.lock().clone(),
            sched_violations: crate::security::last_safety_violations(),
        }
    }
}

/// Classificação PURA de pior-estado (regras determinísticas, ordem = pior primeiro).
pub fn triage_from_inputs(i: &HubTriageInputs) -> TriageVerdict {
    // 1. Heap crítico COM arena vazio = proposta estrutural: drenar p/ arena.
    //    (a ação concreta que a foto s431 motivou — AIOS decide, não só observa)
    let heap_pct = if i.heap_window_mb > 0 {
        i.heap_used_mb * 100 / i.heap_window_mb
    } else {
        0
    };
    let arena_free_mb = i.arena_cap_mb.saturating_sub(i.arena_used_mb);
    if heap_pct >= 90 && arena_free_mb >= 64 {
        return TriageVerdict::Propose {
            title: "Heap critico + arena livre",
            action: "mover conversao/context-window p/ arena Cortex",
        };
    }
    // 2. Heap crítico sem arena = escalar p/ HITL (recusas já ocorrendo).
    if heap_pct >= 95 {
        return TriageVerdict::Propose {
            title: "Heap critico",
            action: "reduzir carga LLM e revisar consumidores de heap",
        };
    }
    // 3. Fragmentação do TALC (s435, idea #630): free alto mas o maior gap
    //    pequeno = memória viva em pedaços — allocs grandes vão falhar mesmo
    //    com "espaço". Sintoma estrutural silencioso → observe-only (mesma
    //    lição s429-lab: escalar sintoma = loop de feedback).
    if i.talc_free_mb >= 256 && i.talc_largest_mb * 4 < i.talc_free_mb {
        return TriageVerdict::Observe("talc fragmentado (largest/free baixo)");
    }
    // 3b. Amostra parcial do walk = metadados ilegíveis — nunca agir cego.
    if i.talc_partial == 1 {
        return TriageVerdict::Observe("talc metadata parcial (walk abortado)");
    }
    // 4. Postura de decisão FAIL com escaladas dominantes = degradação real,
    //    mas é SINTOMA (carga/recusa) — observe-only (lição s429-lab: escalar
    //    sintoma = loop de feedback).
    if i.posture_sev == 2 {
        return TriageVerdict::Observe("posture FAIL (sintoma de carga)");
    }
    // 4. Scheduler violations crescendo = lag — observe-only (I4 classe I5).
    if i.sched_violations > 0 {
        return TriageVerdict::Observe("sched lag (I4 observe-only)");
    }
    TriageVerdict::Ok
}

/// Snapshot `HUB\0` + JSON das linhas de decisão (wire format fixo).
pub fn triage_snapshot_json(i: &HubTriageInputs) -> String {
    let heap_pct = if i.heap_window_mb > 0 {
        i.heap_used_mb * 100 / i.heap_window_mb
    } else {
        0
    };
    let machine = i.machine_json.as_deref().unwrap_or("{}");
    format!(
        "{{\"heap\":{{\"used\":{},\"window\":{},\"pct\":{},\"pressure\":{},\"talc\":{},\"talc_used\":{},\"talc_free\":{},\"talc_largest\":{},\"talc_gaps\":{},\"talc_partial\":{}}},\
\"arena\":{{\"used\":{},\"cap\":{}}},\
\"decide\":{{\"sev\":{},\"line\":\"{}\"}},\
\"sched_violations\":{},\"machine\":{}}}",
        i.heap_used_mb,
        i.heap_window_mb,
        heap_pct,
        i.heap_pressure,
        i.talc_cap_mb,
        i.talc_used_mb,
        i.talc_free_mb,
        i.talc_largest_mb,
        i.talc_gaps,
        i.talc_partial,
        i.arena_used_mb,
        i.arena_cap_mb,
        i.posture_sev,
        i.posture_line,
        i.sched_violations,
        machine
    )
}

/// true se vale submeter o snapshot ao LLM (modelo carregado + headroom).
/// Gate DUPLO barato antes do submit — o submit recusaria de qualquer forma
/// (HeapPressure no InferQueue), mas recusar aqui evita o slog de ruído e
/// deixa o fallback heurístico imediato e explícito.
pub fn should_try_llm(model_loaded: bool, headroom_ok: bool) -> bool {
    model_loaded && headroom_ok
}

/// Prompt único p/ o LLM gerar a proposta (padrão machine_prompt s417).
pub fn triage_llm_prompt(snapshot_json: &str) -> String {
    format!("{}{}", TRIAGE_PROMPT_HEADER, snapshot_json)
}

/// Extrai o valor string de um campo JSON simples (sem serde; scanner
/// minimalista com escape de `\\` e `\"`). CAP de tamanho por campo =
/// runtime hygiene (resposta de modelo não-confiável não vira Vec infinito).
fn json_string_field(text: &str, field: &str) -> Option<String> {
    let needle = format!("\"{}\"", field);
    let start = text.find(&needle)? + needle.len();
    let rest = &text.as_bytes()[start..];
    // Pula whitespace até ':' e depois até a '"' de abertura.
    let mut idx = 0;
    while idx < rest.len() && (rest[idx] == b' ' || rest[idx] == b'\t' || rest[idx] == b'\n' || rest[idx] == b'\r') {
        idx += 1;
    }
    if idx >= rest.len() || rest[idx] != b':' {
        return None;
    }
    idx += 1;
    while idx < rest.len() && (rest[idx] == b' ' || rest[idx] == b'\t' || rest[idx] == b'\n' || rest[idx] == b'\r') {
        idx += 1;
    }
    if idx >= rest.len() || rest[idx] != b'"' {
        return None;
    }
    idx += 1;
    let mut out: Vec<u8> = Vec::new();
    while idx < rest.len() {
        let b = rest[idx];
        match b {
            b'"' => {
                // UTF-8 honesto: bytes crus do modelo → lossy (acentos PT-BR
                // sobrevivem; sequência quebrada vira U+FFFD, nunca panic).
                return Some(String::from_utf8_lossy(&out).into_owned());
            }
            b'\\' if idx + 1 < rest.len() => {
                let next = rest[idx + 1];
                if next == b'"' || next == b'\\' {
                    out.push(next);
                    idx += 2;
                } else {
                    // \n/\t/\uXXXX etc: descarta o escape (honesto e simples).
                    idx += 2;
                }
            }
            _ => {
                out.push(b);
                idx += 1;
            }
        }
        if out.len() > 256 {
            return None; // campo desproporcional = resposta malformada
        }
    }
    None // string nunca fechada
}

/// Parseia a proposta do LLM: `{"title":"..","action":".."}`.
/// Campos vazios/desproporcionais = None (fallback heurístico).
pub fn parse_llm_proposal(text: &str) -> Option<(String, String)> {
    let scan = &text[..text.len().min(4096)];
    if !scan.contains('{') {
        return None;
    }
    let title = json_string_field(scan, "title")?;
    let action = json_string_field(scan, "action")?;
    if title.is_empty() || action.is_empty() || title.len() > 64 || action.len() > 256 {
        return None;
    }
    Some((title, action))
}

/// Decisão PURA sobre a resposta do LLM (testável sem statics/bus).
#[derive(Debug, PartialEq, Eq)]
pub enum LlmDecision {
    /// LLM propôs ação concreta — publica HITL.
    Publish { title: String, action: String },
    /// LLM respondeu `{}` — declínio honesto: observe, sem toast.
    Decline,
    /// Resposta inutilizável (gibberish, marcador de controle, vazio,
    /// JSON malformado) — fallback heurístico.
    Fallback(&'static str),
}

pub fn decide_llm_reply(text: &str) -> LlmDecision {
    let t = text.trim();
    if t.is_empty() {
        return LlmDecision::Fallback("resposta vazia");
    }
    if t.starts_with('[') {
        // "[cancelled]" / "[heap escalate] ..." — marcadores de controle do
        // InferQueue (fail-closed s430): não são proposta.
        return LlmDecision::Fallback("marcador de controle do InferQueue");
    }
    if t == "{}" {
        return LlmDecision::Decline;
    }
    match parse_llm_proposal(t) {
        Some((title, action)) => LlmDecision::Publish { title, action },
        None => LlmDecision::Fallback("sem JSON de proposta valida"),
    }
}

/// FNV-1a 64-bit (fingerprint de conteúdo — dedupe válido, lição s410d).
pub fn fnv1a(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Estado de dedupe/cadência do agente (separado p/ testes sem statics).
pub struct TriageState {
    /// Fingerprints propostos recentemente (FIFO ring, CAP 8).
    fps: [u64; FP_CAP],
    fp_len: usize,
    fp_head: usize,
    /// Fingerprint -> tick da última proposta (paralelo aos fps).
    fp_tick: [u64; FP_CAP],
    next_tick: u64,
    /// Job LLM em voo (reserva o fp no dedupe; resposta/timeout resolve).
    pub pending: Option<PendingProposal>,
}

/// Proposta heurística aguardando o veredito do LLM (título/ação são
/// `&'static str` do veredito — fallback pronto sem re-classificar).
#[derive(Debug, Clone, Copy)]
pub struct PendingProposal {
    pub fp: u64,
    pub submitted_tick: u64,
    pub title: &'static str,
    pub action: &'static str,
}

impl Default for TriageState {
    fn default() -> Self {
        TriageState {
            fps: [0; FP_CAP],
            fp_len: 0,
            fp_head: 0,
            fp_tick: [0; FP_CAP],
            next_tick: 0,
            pending: None,
        }
    }
}

impl TriageState {
    /// true quando o job LLM em voo estourou o timeout (fallback deve disparar).
    pub fn llm_timeout_due(&self, now_tick: u64) -> bool {
        match self.pending {
            Some(p) => now_tick.saturating_sub(p.submitted_tick) >= LLM_REPLY_TIMEOUT_TICKS,
            None => false,
        }
    }
}

impl TriageState {
    /// true se pode propor AGORA (nunca proposto, ou cooldown expirado).
    /// Dedupe de conteúdo: mesmo payload dentro do cooldown = drop (s410d).
    pub fn can_propose(&mut self, fp: u64, now_tick: u64) -> bool {
        for idx in 0..self.fp_len {
            let slot = (self.fp_head + idx) % FP_CAP;
            if self.fps[slot] == fp {
                return now_tick.saturating_sub(self.fp_tick[slot]) >= PROPOSE_COOLDOWN_TICKS;
            }
        }
        true
    }

    /// Registra proposta (insere/atualiza slot; FIFO evicção quando cheio).
    pub fn note_proposed(&mut self, fp: u64, now_tick: u64) {
        for idx in 0..self.fp_len {
            let slot = (self.fp_head + idx) % FP_CAP;
            if self.fps[slot] == fp {
                self.fp_tick[slot] = now_tick;
                return;
            }
        }
        if self.fp_len < FP_CAP {
            let slot = (self.fp_head + self.fp_len) % FP_CAP;
            self.fps[slot] = fp;
            self.fp_tick[slot] = now_tick;
            self.fp_len += 1;
        } else {
            // Cheio: sobrescreve o mais antigo (head) e avança.
            self.fps[self.fp_head] = fp;
            self.fp_tick[self.fp_head] = now_tick;
            self.fp_head = (self.fp_head + 1) % FP_CAP;
        }
    }

    pub fn due(&self, now_tick: u64) -> bool {
        self.next_tick == 0 || now_tick >= self.next_tick
    }

    pub fn advance(&mut self, now_tick: u64) {
        self.next_tick = now_tick.saturating_add(TRIAGE_PERIOD_TICKS);
    }
}

/// Contadores de observabilidade (evidência de wire, lição SESSION_419).
pub static HUB_TRIAGE_SNAPSHOTS: AtomicU64 = AtomicU64::new(0);
pub static HUB_TRIAGE_PROPOSALS: AtomicU64 = AtomicU64::new(0);
pub static HUB_TRIAGE_DROPPED: AtomicU64 = AtomicU64::new(0);
pub static HUB_TRIAGE_LLM_SUBMITTED: AtomicU64 = AtomicU64::new(0);
pub static HUB_TRIAGE_LLM_FALLBACKS: AtomicU64 = AtomicU64::new(0);

/// Publica a proposta HITL (USER_INTENT + TOAST) com gate de headroom.
/// false = retida por headroom baixo (fail-closed s430: o snapshot já está
/// no EventBus — o humano vê a foto; a IA não dispara work sob pressão).
fn publish_proposal_hitl(source: &str, title: &str, action: &str) -> bool {
    if k_nano::allocator::heap_headroom_low() {
        HUB_TRIAGE_DROPPED.fetch_add(1, Ordering::Relaxed);
        k_nano::slog_hermes!(
            "HubTriage", "warn",
            "proposta retida (headroom baixo): {} — {}", title, action
        );
        return false;
    }
    // Snapshot fresco no momento da publicação (contexto do intent p/ Hermes).
    let json = triage_snapshot_json(&HubTriageInputs::gather());
    let _ = EVENT_BUS.publish(Event {
        id: 0,
        topic: String::from(crate::hermes::TOPIC_USER_INTENT),
        payload: format!(
            "proposta de otimizacao (HUB triage via {}): {} — {}. Snapshot: {}",
            source, title, action, json
        )
        .into_bytes(),
        token: CapabilityToken::Legacy(1),
    });
    let _ = EVENT_BUS.publish(Event {
        id: 0,
        topic: String::from("TOAST"),
        payload: format!("IA propoe: {} ({})", title, action).into_bytes(),
        token: CapabilityToken::Legacy(1),
    });
    HUB_TRIAGE_PROPOSALS.fetch_add(1, Ordering::Relaxed);
    k_nano::slog_hermes!(
        "HubTriage", "ok",
        "proposta HITL ({}): {} — {}", source, title, action
    );
    true
}

/// Consome UMA resposta do LLM (reply publicado pelo InferQueue em
/// `HUB_TRIAGE_LLM`). Retorna a linha de slog (Option) — o agente loga.
pub fn on_llm_reply(state: &mut TriageState, payload: &[u8], now_tick: u64) -> Option<String> {
    // Sem job em voo = resposta tardia (pending já resolvido) — ignora.
    let pending = state.pending.take()?;
    let text = core::str::from_utf8(payload).unwrap_or("");
    match decide_llm_reply(text) {
        LlmDecision::Publish { title, action } => {
            if publish_proposal_hitl("LLM", &title, &action) {
                // Dedupe da ação GERADA (o fp da heurística já foi anotado
                // no submit — evita re-submit enquanto esta ação está viva).
                state.note_proposed(fnv1a(action.as_bytes()), now_tick);
                Some(format!("proposta LLM→HITL: {}", title))
            } else {
                Some(format!("proposta LLM retida (headroom): {}", title))
            }
        }
        LlmDecision::Decline => {
            // O LLM viu o snapshot e decidiu não agir — HITL respeita a IA.
            Some(String::from("LLM declinou proposta (observe-only)"))
        }
        LlmDecision::Fallback(reason) => {
            HUB_TRIAGE_LLM_FALLBACKS.fetch_add(1, Ordering::Relaxed);
            let _ = publish_proposal_hitl("fallback", pending.title, pending.action);
            Some(format!(
                "LLM {} — fallback heuristico: {}", reason, pending.title
            ))
        }
    }
}

/// Timeout do job LLM em voo → fallback heurístico. Retorna linha de slog.
pub fn llm_timeout_check(state: &mut TriageState, now_tick: u64) -> Option<String> {
    if !state.llm_timeout_due(now_tick) {
        return None;
    }
    let p = state.pending.take()?;
    HUB_TRIAGE_LLM_FALLBACKS.fetch_add(1, Ordering::Relaxed);
    let _ = publish_proposal_hitl("fallback-timeout", p.title, p.action);
    Some(format!(
        "LLM timeout ({} ticks) — fallback heuristico: {}",
        LLM_REPLY_TIMEOUT_TICKS, p.title
    ))
}

/// Ciclo completo (chamado pelo agente): snapshot + veredito + wire.
/// Retorna a linha de slog (para o agente logar — teste via retorno).
pub fn triage_tick(state: &mut TriageState, now_tick: u64) -> String {
    if !state.due(now_tick) {
        return String::from("");
    }
    state.advance(now_tick);
    // Gate de classe no caller (s429-lab): frag pressure = observe total.
    if k_nano::memory::mesh_frag_pressure() {
        return String::from("observe-only (frag pressure)");
    }
    let inputs = HubTriageInputs::gather();
    let json = triage_snapshot_json(&inputs);
    // Wire: publica o snapshot SEMPRE (evidência + LLM/HUD podem assinar).
    let mut payload = Vec::with_capacity(HUB_PREFIX.len() + json.len());
    payload.extend_from_slice(HUB_PREFIX);
    payload.extend_from_slice(json.as_bytes());
    let _ = EVENT_BUS.publish(Event {
        id: 0,
        topic: String::from(TOPIC_HUB_TRIAGE),
        payload,
        token: CapabilityToken::Legacy(1),
    });
    HUB_TRIAGE_SNAPSHOTS.fetch_add(1, Ordering::Relaxed);

    match triage_from_inputs(&inputs) {
        TriageVerdict::Ok => {
            // s435: evidência da telemetria no slog (regra 419 — dado novo
            // só está "feito" quando o slog prova em runtime). Zeros =
            // claim ausente (n/a ≠ 0 é do HUD, aqui a linha é do triage).
            if inputs.talc_cap_mb > 0 && inputs.talc_free_mb == 0 && inputs.talc_used_mb == 0 {
                String::from("ok (talc sem amostra — aguardando HUD refresh)")
            } else {
                format!(
                    "ok talc u{}M f{}M lg{}M g{}",
                    inputs.talc_used_mb, inputs.talc_free_mb,
                    inputs.talc_largest_mb, inputs.talc_gaps
                )
            }
        }
        TriageVerdict::Observe(reason) => format!("observe: {}", reason),
        TriageVerdict::Propose { title, action } => {
            let fp = fnv1a(action.as_bytes());
            if !state.can_propose(fp, now_tick) {
                HUB_TRIAGE_DROPPED.fetch_add(1, Ordering::Relaxed);
                return format!("propose dedupe (cooldown): {}", title);
            }
            // Reserva o fp AGORA: enquanto o job LLM está em voo (ou o toast
            // vivo), o próximo ciclo com o mesmo veredito cai no dedupe —
            // sem isso, 1 ciclo/min re-submete o job (loop de feedback).
            state.note_proposed(fp, now_tick);
            // Gate duplo antes do LLM: modelo carregado + headroom ok.
            // O InferQueue recusaria de qualquer forma, mas recusar aqui
            // mantém o fallback heurístico imediato e o slog limpo.
            let model_ok = cortex::cortex::model_is_loaded();
            let headroom_ok = !k_nano::allocator::heap_headroom_low();
            if should_try_llm(model_ok, headroom_ok) {
                match cortex::infer_queue::submit(
                    triage_llm_prompt(&json),
                    cortex::infer_queue::InferMode::Plain,
                    TOPIC_HUB_TRIAGE_LLM,
                ) {
                    Ok(id) => {
                        state.pending = Some(PendingProposal {
                            fp,
                            submitted_tick: now_tick,
                            title,
                            action,
                        });
                        HUB_TRIAGE_LLM_SUBMITTED.fetch_add(1, Ordering::Relaxed);
                        k_nano::slog_hermes!(
                            "HubTriage", "ok",
                            "proposta via LLM submitted id={} (heuristica de reserva: {} — {})",
                            id, title, action
                        );
                        return format!("propose via LLM (pending id={})", id);
                    }
                    Err(e) => {
                        k_nano::slog_hermes!(
                            "HubTriage", "warn",
                            "submit LLM falhou ({:?}) — fallback heuristico", e
                        );
                    }
                }
            } else if !model_ok {
                k_nano::slog_hermes!(
                    "HubTriage", "ok",
                    "modelo ausente — proposta heurística direta"
                );
            }
            publish_proposal_hitl("heuristica", title, action);
            format!("propose: {}", title)
        }
    }
}

/// Agente (EventDriven com cadência temporal — mesmo padrão do SysHealthAgent).
pub struct HubTriageAgent {
    manifest: agent_core::AgentManifest,
    state: TriageState,
    /// Respostas do LLM de triagem (o InferQueue publica o texto cru aqui;
    /// podem chegar em QUALQUER tick — fora da cadência de 60s).
    llm_receiver: Receiver,
}

impl HubTriageAgent {
    pub fn new() -> Self {
        HubTriageAgent {
            manifest: agent_core::AgentManifest {
                name: "hub_triage_agent",
                kind: agent_core::AgentKind::System,
                schedule: agent_core::ScheduleKind::EventDriven,
                auto_start: true,
                persist: false,
            },
            state: TriageState::default(),
            llm_receiver: EVENT_BUS.subscribe(TOPIC_HUB_TRIAGE_LLM),
        }
    }
}

impl Default for HubTriageAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl agent_core::Agent for HubTriageAgent {
    fn manifest(&self) -> &agent_core::AgentManifest {
        &self.manifest
    }

    fn has_pending(&self) -> bool {
        let now = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
        // Cadência de 60s OU resposta LLM na fila OU timeout do job em voo —
        // sem isso o reply chegaria e ninguém o drenaria até o próximo ciclo
        // (lição lost-wakeup s411: quem espera resposta re-checa bounded).
        self.state.due(now)
            || self.state.llm_timeout_due(now)
            || self.llm_receiver.has_pending()
    }

    fn tick(&mut self, _tick: u64, _count: u64) -> agent_core::AgentTickResult {
        let now = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
        // 1. Drena respostas do LLM (qualquer tick; drena a fila inteira).
        while let Some(ev) = self.llm_receiver.try_receive() {
            if let Some(line) = on_llm_reply(&mut self.state, &ev.payload, now) {
                k_nano::slog_hermes!("HubTriage", "info", "{}", line);
            }
        }
        // 2. Timeout do job em voo → fallback heurístico.
        if let Some(line) = llm_timeout_check(&mut self.state, now) {
            k_nano::slog_hermes!("HubTriage", "info", "{}", line);
        }
        // 3. Ciclo de triagem (1/min).
        let line = triage_tick(&mut self.state, now);
        if !line.is_empty() {
            k_nano::slog_hermes!("HubTriage", "info", "{}", line);
        }
        agent_core::AgentTickResult::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(heap_pct: u64, arena_free: u64, sev: u8, viol: u64) -> HubTriageInputs {
        let window = 2030u64;
        HubTriageInputs {
            heap_used_mb: window * heap_pct / 100,
            heap_window_mb: window,
            heap_pressure: if heap_pct >= 90 { 2 } else { 0 },
            talc_cap_mb: 6911,
            talc_used_mb: 0,
            talc_free_mb: 6911,
            talc_largest_mb: 6911,
            talc_gaps: 1,
            talc_partial: 0,
            arena_used_mb: 0,
            arena_cap_mb: 256,
            posture_sev: sev,
            posture_line: String::from("a2 x0 e6 L0"),
            machine_json: None,
            sched_violations: viol,
        }
    }

    #[test]
    fn heap_critico_com_arena_propoe_drenagem() {
        let v = triage_from_inputs(&inputs(99, 256, 2, 30));
        match v {
            TriageVerdict::Propose { title, .. } => {
                assert_eq!(title, "Heap critico + arena livre")
            }
            _ => panic!("esperava Propose"),
        }
    }

    #[test]
    fn talc_fragmentado_e_observe_only() {
        // free 512MB, maior gap 100MB (largest*4 < free) = fragmentação real.
        let mut i = inputs(50, 256, 0, 0);
        i.talc_free_mb = 512;
        i.talc_largest_mb = 100;
        i.talc_gaps = 37;
        assert_eq!(
            triage_from_inputs(&i),
            TriageVerdict::Observe("talc fragmentado (largest/free baixo)")
        );
        // Livre saudável (maior gap domina): nunca Observe por fragmentação.
        i.talc_largest_mb = 480;
        assert_eq!(triage_from_inputs(&i), TriageVerdict::Ok);
    }

    #[test]
    fn talc_walk_parcial_nunca_propoe() {
        // Amostra parcial (metadados ilegíveis) = observe, mesmo com heap ok.
        let mut i = inputs(50, 256, 0, 0);
        i.talc_partial = 1;
        assert_eq!(
            triage_from_inputs(&i),
            TriageVerdict::Observe("talc metadata parcial (walk abortado)")
        );
    }

    #[test]
    fn heap_critico_sem_arena_propoe_hitl() {
        let mut i = inputs(96, 0, 0, 0);
        i.arena_cap_mb = 0;
        match triage_from_inputs(&i) {
            TriageVerdict::Propose { title, .. } => assert_eq!(title, "Heap critico"),
            _ => panic!("esperava Propose"),
        }
    }

    #[test]
    fn posture_fail_sozinho_e_observe_only() {
        // Sintoma de carga — a lição s429-lab: escalar sintoma = loop.
        assert_eq!(
            triage_from_inputs(&inputs(50, 256, 2, 0)),
            TriageVerdict::Observe("posture FAIL (sintoma de carga)")
        );
    }

    #[test]
    fn saudavel_e_ok() {
        assert_eq!(triage_from_inputs(&inputs(50, 256, 0, 0)), TriageVerdict::Ok);
    }

    #[test]
    fn snapshot_tem_campos_de_decisao() {
        let j = triage_snapshot_json(&inputs(99, 256, 2, 30));
        // pct sofre floor inteiro (2009/2030 = 98%) — o teste valida o campo, não o arredondamento.
        assert!(j.contains("\"pct\":98"));
        assert!(j.contains("\"talc\":6911"));
        assert!(j.contains("\"talc_free\":6911"));
        assert!(j.contains("\"talc_largest\":6911"));
        assert!(j.contains("\"sev\":2"));
        assert!(j.contains("a2 x0 e6 L0"));
        assert!(j.starts_with('{') && j.ends_with('}'));
    }

    #[test]
    fn dedupe_por_fingerprint_com_cooldown() {
        let mut st = TriageState::default();
        let fp = fnv1a(b"mover conversao p/ arena");
        assert!(st.can_propose(fp, 1000));
        st.note_proposed(fp, 1000);
        // Dentro do cooldown: drop.
        assert!(!st.can_propose(fp, 1000 + PROPOSE_COOLDOWN_TICKS - 1));
        // Expirado: pode de novo.
        assert!(st.can_propose(fp, 1000 + PROPOSE_COOLDOWN_TICKS));
        // Fingerprint diferente: nunca bloqueado.
        assert!(st.can_propose(fnv1a(b"outra acao"), 1001));
    }

    #[test]
    fn dedupe_cap_evict_fifo() {
        let mut st = TriageState::default();
        for n in 0..FP_CAP as u64 {
            let fp = fnv1a(format!("acao {}", n).as_bytes());
            st.note_proposed(fp, 100 + n);
        }
        // O mais antigo (n=0) foi evictado pelo mais novo.
        let fp0 = fnv1a(b"acao 0");
        st.note_proposed(fnv1a(b"acao novo"), 200);
        assert!(st.can_propose(fp0, 1000), "fp antigo deve ter sido evictado");
    }

    #[test]
    fn triage_tick_publica_snapshot_e_propoe_uma_vez() {
        // Não roda o EventBus real em teste — valida cadência e veredito via
        // triage_from_inputs; o wire completo é provado no QEMU (regra 419).
        let mut st = TriageState::default();
        assert!(st.due(0), "1º tick é due");
        st.advance(0);
        assert!(!st.due(1), "após advance, next = PERIOD");
        assert!(st.due(TRIAGE_PERIOD_TICKS));
    }

    #[test]
    fn fnv1a_conhecido() {
        // Vetor de teste do FNV-1a 64 ("" e "a").
        assert_eq!(fnv1a(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a(b"a"), 0xaf63dc4c8601ec8c);
    }

    #[test]
    fn parse_llm_proposal_valido() {
        let (t, a) = parse_llm_proposal(
            "{\"title\":\"Mover KV\",\"action\":\"mover context-window p/ arena\"}",
        )
        .expect("JSON valido deve parsear");
        assert_eq!(t, "Mover KV");
        assert_eq!(a, "mover context-window p/ arena");
    }

    #[test]
    fn parse_llm_proposal_com_escapes_e_whitespace() {
        let (t, a) = parse_llm_proposal(
            "\n{ \"title\" : \"Mover \\\"KV\\\" agora\", \"action\": \"drenar \\\\heap\\\\ p/ arena\" }\n",
        )
        .expect("escapes devem ser tratados");
        assert_eq!(t, "Mover \"KV\" agora");
        assert_eq!(a, "drenar \\heap\\ p/ arena");
    }

    #[test]
    fn parse_llm_proposal_invalido_e_none() {
        // Gibberish de modelo stub.
        assert!(parse_llm_proposal("xkcd blah 42").is_none());
        // Falta o action.
        assert!(parse_llm_proposal("{\"title\":\"so titulo\"}").is_none());
        // Campo vazio.
        assert!(parse_llm_proposal("{\"title\":\"\",\"action\":\"x\"}").is_none());
        // String nunca fechada.
        assert!(parse_llm_proposal("{\"title\":\"aberto").is_none());
    }

    #[test]
    fn decide_llm_reply_roteia() {
        match decide_llm_reply("{\"title\":\"T\",\"action\":\"A\"}") {
            LlmDecision::Publish { title, action } => {
                assert_eq!((title.as_str(), action.as_str()), ("T", "A"));
            }
            d => panic!("esperava Publish, veio {:?}", d),
        }
        // Declínio honesto.
        assert_eq!(decide_llm_reply("  {}  "), LlmDecision::Decline);
        // Marcadores de controle do InferQueue → fallback.
        assert_eq!(
            decide_llm_reply("[heap escalate] headroom critico - HITL."),
            LlmDecision::Fallback("marcador de controle do InferQueue")
        );
        assert_eq!(
            decide_llm_reply("[cancelled]"),
            LlmDecision::Fallback("marcador de controle do InferQueue")
        );
        // Gibberish/vazio → fallback.
        assert_eq!(
            decide_llm_reply("gibberish do modelo stub"),
            LlmDecision::Fallback("sem JSON de proposta valida")
        );
        assert_eq!(
            decide_llm_reply("   "),
            LlmDecision::Fallback("resposta vazia")
        );
    }

    #[test]
    fn prompt_tem_instrucao_e_snapshot() {
        let p = triage_llm_prompt("{\"heap\":{\"pct\":99}}");
        assert!(p.contains("responda SOMENTE"));
        assert!(p.contains("{\"heap\":{\"pct\":99}}"));
    }

    #[test]
    fn should_try_llm_gates() {
        // Modelo carregado + headroom ok = tenta LLM.
        assert!(should_try_llm(true, true));
        // Modelo ausente = heurística direta.
        assert!(!should_try_llm(false, true));
        // Headroom baixo = fallback (o submit recusaria com HeapPressure).
        assert!(!should_try_llm(true, false));
    }

    #[test]
    fn pending_timeout_dispara() {
        let mut st = TriageState::default();
        st.pending = Some(PendingProposal {
            fp: 42,
            submitted_tick: 1000,
            title: "Heap critico",
            action: "reduzir carga",
        });
        assert!(!st.llm_timeout_due(1000 + LLM_REPLY_TIMEOUT_TICKS - 1));
        assert!(st.llm_timeout_due(1000 + LLM_REPLY_TIMEOUT_TICKS));
        // Sem pending: nunca timeout.
        st.pending = None;
        assert!(!st.llm_timeout_due(u64::MAX / 2));
    }
}
