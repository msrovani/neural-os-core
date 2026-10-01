//! s432 — Triagem IA do HUB HEALTH (premissa máxima ADR-0088: a IA consome o
//! painel sozinha, o humano lê a foto). O HUB HEALTH é painel humano; os dados
//! que o alimentam já existem em statics acessíveis. Este módulo:
//!
//! 1. Monta o snapshot `HUB\0` + JSON das linhas de DECISÃO do painel
//!    (não das 18 visuais — só as que a IA pode agir: heap, arena, posture,
//!    sys/audio verdicts, I4 sched).
//! 2. Classifica pior-estado com regras determinísticas PURAS (testáveis).
//! 3. Em `Propose`, publica USER_INTENT (prompt único p/ LLM) + TOAST HITL,
//!    com dedupe por fingerprint (FNV-1a) e cooldown 10min por fingerprint —
//!    mesma lição do mesh_knowledge (s410d): sem dedupe, a escalada vira loop
//!    de feedback (233 intents no boot mesh).
//!
//! Honestidade: `Observe` nunca escala (I5/sched = sintoma de carga, lição
//! s429-lab); `mesh_frag_pressure` desliga tudo (caller gate, s429-lab); o
//! snapshot publicado no EventBus é a evidência de wire (regra SESSION_419:
//! tópico novo só está feito quando tem produtor + consumidor + slog).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use event_bus::{CapabilityToken, Event};
use k_nano::EVENT_BUS;

/// Tópico do snapshot (evidência de wire; HUD/LLM podem assinar).
pub const TOPIC_HUB_TRIAGE: &str = "HUB_TRIAGE";
/// Prefixo de wire do snapshot (padrão MCH\0/HUB contract).
pub const HUB_PREFIX: &[u8] = b"HUB\0";

/// Cadência da triagem: 60 s (~3600 ticks @60Hz — painel 2 Hz, triagem lenta).
pub const TRIAGE_PERIOD_TICKS: u64 = 3600;
/// Cooldown por fingerprint: 10 min (600 s).
pub const PROPOSE_COOLDOWN_TICKS: u64 = 36000;
/// CAP do dedupe (runtime hygiene s410d: CAP + evicção FIFO).
const FP_CAP: usize = 8;

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
    // 3. Postura de decisão FAIL com escaladas dominantes = degradação real,
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
        "{{\"heap\":{{\"used\":{},\"window\":{},\"pct\":{},\"pressure\":{},\"talc\":{}}},\
\"arena\":{{\"used\":{},\"cap\":{}}},\
\"decide\":{{\"sev\":{},\"line\":\"{}\"}},\
\"sched_violations\":{},\"machine\":{}}}",
        i.heap_used_mb,
        i.heap_window_mb,
        heap_pct,
        i.heap_pressure,
        i.talc_cap_mb,
        i.arena_used_mb,
        i.arena_cap_mb,
        i.posture_sev,
        i.posture_line,
        i.sched_violations,
        machine
    )
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
}

impl Default for TriageState {
    fn default() -> Self {
        TriageState {
            fps: [0; FP_CAP],
            fp_len: 0,
            fp_head: 0,
            fp_tick: [0; FP_CAP],
            next_tick: 0,
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
        TriageVerdict::Ok => String::from("ok"),
        TriageVerdict::Observe(reason) => format!("observe: {}", reason),
        TriageVerdict::Propose { title, action } => {
            let fp = fnv1a(action.as_bytes());
            if !state.can_propose(fp, now_tick) {
                HUB_TRIAGE_DROPPED.fetch_add(1, Ordering::Relaxed);
                return format!("propose dedupe (cooldown): {}", title);
            }
            state.note_proposed(fp, now_tick);
            HUB_TRIAGE_PROPOSALS.fetch_add(1, Ordering::Relaxed);
            // Prompt único p/ LLM (padrão machine_prompt s417).
            let _ = EVENT_BUS.publish(Event {
                id: 0,
                topic: String::from(crate::hermes::TOPIC_USER_INTENT),
                payload: format!(
                    "proposta de otimizacao (HUB triage): {} — {}. Snapshot: {}",
                    title, action, json
                )
                .into_bytes(),
                token: CapabilityToken::Legacy(1),
            });
            // HITL toast: o humano vê e aprova/veta na UI (premissa: HITL forte).
            let _ = EVENT_BUS.publish(Event {
                id: 0,
                topic: String::from("TOAST"),
                payload: format!("IA propoe: {} ({})", title, action).into_bytes(),
                token: CapabilityToken::Legacy(1),
            });
            k_nano::slog_hermes!("HubTriage", "ok", "proposta HITL: {} — {}", title, action);
            format!("propose: {}", title)
        }
    }
}

/// Agente (EventDriven com cadência temporal — mesmo padrão do SysHealthAgent).
pub struct HubTriageAgent {
    manifest: agent_core::AgentManifest,
    state: TriageState,
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
        self.state
            .due(k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64)
    }

    fn tick(&mut self, _tick: u64, _count: u64) -> agent_core::AgentTickResult {
        let now = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
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
}
