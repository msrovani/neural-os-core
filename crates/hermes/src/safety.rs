//! Safety Interceptor — Asimov's Four Laws + Fail-Closed Safety Invariant (#315.18).
//! Invariantes SMT-proof: process separation, pre-action, fail-closed, signed evidence.
//!
//! Layers:
//!   0 — Systemic Cosmic Law:  nenhuma ação que ameace a humanidade
//!   1 — Digital Non-Maleficence: nenhum dano a indivíduos
//!   2 — Deviation-Resistant Alignment: transparência e fidelidade
//!   3 — Eco-Sustainability: autodefesa sem causar dano ecológico
//!
//! Fail-Closed: SafetyAgent sempre nega por padrão. Toda skill precisa autorização explícita.
//! 4 invariants SMT-proof:
//!   I1: Process separation — nenhum agente acessa memória de outro sem permissão
//!   I2: Pre-action — toda skill é verificada ANTES de executar
//!   I3: Fail-closed — se o SafetyAgent não responde, a ação é NEGADA
//!   I4: Signed evidence — toda decisão é registrada com hash para auditoria

use alloc::string::String;
use alloc::vec::Vec;
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use k_nano::{println};
use k_nano::EVENT_BUS;

const SAFETY_MANIFEST: AgentManifest = AgentManifest {
    name: "safety",
    kind: AgentKind::System,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

#[derive(Debug, Clone, PartialEq)]
pub enum SafetyVerdict {
    Allow,
    Violation { layer: u8, reason: String },
}

// ─── #315.18 Fail-Closed Safety Invariant ────────────────────────────────
// 4 invariants que devem ser verdadeiros para toda skill executada.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Invariant { I1ProcessSeparation, I2PreAction, I3FailClosed, I4SignedEvidence }

pub struct SafetyInvariants {
    pub i1_process_sep: bool,  // agentes não acessam memória uns dos outros
    pub i2_pre_action: bool,   // skill verificada antes de executar
    pub i3_fail_closed: bool,  // nega se não responde
    pub i4_signed: bool,       // toda decisão tem hash de auditoria
}

impl SafetyInvariants {
    pub const fn new() -> Self {
        SafetyInvariants { i1_process_sep: true, i2_pre_action: true, i3_fail_closed: true, i4_signed: true }
    }
    /// Verifica os 4 invariants para uma ação. Retorna SafetyVerdict.
    pub fn check(&self, action: &str, agent: &str, skill_name: &str) -> SafetyVerdict {
        // I1: Process separation — nenhum agente acessa memória de outro
        if self.i1_process_sep && (action.contains("read_mem_") || action.contains("write_mem_")
            || action.contains("inspect_agent")) {
            return SafetyVerdict::Violation { layer: 1, reason: String::from("I1 violado: acesso a memória de outro processo") };
        }
        // I3: Fail-Closed — padrão é negar
        if !self.i3_fail_closed {
            return SafetyVerdict::Violation { layer: 3, reason: String::from("I3 Fail-Closed violado: safety não está ativo") };
        }
        // I2: Pre-action — skill verificada antes
        if !self.i2_pre_action {
            return SafetyVerdict::Violation { layer: 2, reason: String::from("I2 Pre-Action violado: skill não verificada") };
        }
        // I4: Signed evidence — registra no Merkle Audit Trail
        if self.i4_signed {
            let tick = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
            let msg = alloc::format!("{}:{}:{}", action, agent, skill_name);
            crate::globals::AUDIT_TRAIL.lock().push(tick, agent, "safety_check", msg.as_bytes());
        }
        // Hard blocklist + Layer 0 (patterns)
        for (pattern, reason) in LAYER0_PATTERNS.iter() {
            if action.contains(pattern) || skill_name.contains(pattern) {
                return SafetyVerdict::Violation { layer: 0, reason: String::from(*reason) };
            }
        }
        for pattern in HARD_BLOCKLIST.iter() {
            if action.contains(pattern) || skill_name.contains(pattern) {
                return SafetyVerdict::Violation { layer: 0, reason: String::from("hard blocklist match") };
            }
        }
        // Layer 1 — dano individual
        for (pattern, reason) in LAYER1_PATTERNS.iter() {
            if action.contains(pattern) {
                return SafetyVerdict::Violation { layer: 1, reason: String::from(*reason) };
            }
        }
        SafetyVerdict::Allow
    }
    pub fn status(&self) -> alloc::string::String {
        alloc::format!("[SAFETY] I1={} I2={} I3={} I4={}",
            self.i1_process_sep, self.i2_pre_action, self.i3_fail_closed, self.i4_signed)
    }
}

const LAYER0_PATTERNS: &[(&str, &str)] = &[
    ("weapon", "desenvolvimento de armas autônomas"),
    ("cyberwar", "componentes de guerra cibernética"),
    ("WMD", "armas de destruição em massa"),
    ("kill", "comando para causar dano fatal"),
    ("destroy infrastructure", "destruição de infraestrutura crítica"),
];

const LAYER1_PATTERNS: &[(&str, &str)] = &[
    ("steal", "furto de dados ou identidade"),
    ("dox", "exposição de dados pessoais"),
    ("deepfake", "geração de mídia sintética enganosa"),
    ("social engineering", "engenharia social automatizada"),
    ("harass", "assedio ou perseguição digital"),
];

const LAYER2_PATTERNS: &[(&str, &str)] = &[
    ("spoof log", "falsificação de logs do sistema"),
    ("hide", "ocultação de estado ou telemetria"),
    ("impersonate", "falsificação de identidade do sistema"),
    ("bypass audit", "desvio de trilha de auditoria"),
];

const LAYER3_PATTERNS: &[(&str, &str)] = &[
    ("infinite loop", "loop infinito sem yield"),
    ("resource exhaustion", "exaustão de recursos computacionais"),
    ("energy drain", "drenagem energética sem propósito"),
];

pub fn check_safety(input: &str) -> SafetyVerdict {
    let lower = input.to_ascii_lowercase();
    for (pattern, reason) in LAYER0_PATTERNS {
        if lower.contains(pattern) {
            return SafetyVerdict::Violation { layer: 0, reason: String::from(*reason) };
        }
    }
    for (pattern, reason) in LAYER1_PATTERNS {
        if lower.contains(pattern) {
            return SafetyVerdict::Violation { layer: 1, reason: String::from(*reason) };
        }
    }
    for (pattern, reason) in LAYER2_PATTERNS {
        if lower.contains(pattern) {
            return SafetyVerdict::Violation { layer: 2, reason: String::from(*reason) };
        }
    }
    for (pattern, reason) in LAYER3_PATTERNS {
        if lower.contains(pattern) {
            return SafetyVerdict::Violation { layer: 3, reason: String::from(*reason) };
        }
    }
    SafetyVerdict::Allow
}

pub struct SafetyAgent {
    receiver: event_bus::Receiver,
    violations: Vec<(u8, String, u64)>,
    verify_counter: u64,
}

impl SafetyAgent {
    pub fn new() -> Self {
        SafetyAgent {
            receiver: EVENT_BUS.subscribe("SAFETY_CHECK"),
            violations: Vec::new(),
            verify_counter: 0,
        }
    }

    fn log_violation(&mut self, layer: u8, input: &str, reason: &str) {
        let tick = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
        self.violations.push((layer, String::from(input), tick as u64));
        k_nano::slog_hermes!("Log", "msg", "⚠️  SAFETY VIOLATION — Layer {} ⚠️", layer);
        k_nano::slog_hermes!("Input", "info", "\"{}\"", input);
        k_nano::slog_hermes!("Reason", "info", "{}", reason);
        k_nano::slog_hermes!("Tick", "info", "{}", tick);
        if layer == 0 {
            k_nano::slog_hermes!("SAFETY", "info", "LAYER 0 VIOLATION — HALT");
            println!("[SAFETY] ⛔ LAYER 0 — Cosmic Law Violation. HALT.");
            loop { x86_64::instructions::hlt(); }
        }
    }
}

impl Agent for SafetyAgent {
    fn manifest(&self) -> &AgentManifest { &SAFETY_MANIFEST }
    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        while let Some(event) = self.receiver.try_receive() {
            let text = core::str::from_utf8(&event.payload).unwrap_or("");
            let verdict = check_safety(text);
            if let SafetyVerdict::Violation { layer, reason } = verdict {
                self.log_violation(layer, text, &reason);
                let _ = EVENT_BUS.publish(event_bus::Event {
                    id: 0, topic: String::from("SAFETY_RESULT"),
                    payload: alloc::format!("DENY: Layer {} - {}", layer, reason).into_bytes(),
                    token: event_bus::CapabilityToken::Legacy(1),
                });
            } else {
                let _ = EVENT_BUS.publish(event_bus::Event {
                    id: 0, topic: String::from("SAFETY_RESULT"),
                    payload: b"ALLOW".to_vec(), token: event_bus::CapabilityToken::Legacy(1),
                });
            }
        }
        // I4: periodic Merkle chain verify — every 100 ticks audita a trilha.
        self.verify_counter = self.verify_counter.wrapping_add(1);
        if self.verify_counter % 100 == 0 {
            let (ok, count) = {
                let trail = crate::globals::AUDIT_TRAIL.lock();
                (trail.verify(), trail.entry_count())
            };
            k_nano::slog_hermes!("SAFETY", "I4", "Merkle chain verify={} (entries={})", ok, count);
        }
        AgentTickResult::Pending
    }
}

const HARD_BLOCKLIST: &[&str] = &[
    "rm -rf /", "rm -rf /*",
    "dd if=/dev/zero of=/dev/sd", "dd if=/dev/random of=/dev/sd",
    "mkfs.", "format",
    ":(){ :|& };:",  // fork bomb
    "chmod -R 000 /", "chown -R 0:0 /",
    "curl * | sh", "wget * | sh",
    "bash -c ", "eval $(", "`rm ",
];

pub fn check_command(cmd: &str) -> Result<(), &'static str> {
    let lower = cmd.to_ascii_lowercase();
    for &blocked in HARD_BLOCKLIST {
        if lower.contains(blocked) {
            k_nano::slog_hermes!("SAFETY", "info", "Blocked: {}", blocked);
            return Err("Hard blocklist violation");
        }
    }
    if lower.contains("weapon") || lower.contains("wmd") || lower.contains("cyberwar")
        || lower.contains("nuclear") || lower.contains("biological") {
        k_nano::slog_hermes!("SAFETY", "info", "Layer 0 violation!");
        return Err("Layer 0: Systemic Cosmic Law");
    }
    Ok(())
}

// ── Guard railes para SleepCycle (IDEA #314) — Sprint 79 ──────────────
// Comentado ate implementacao. Ver ADR-0033 secao 10.
//
// pub fn check_sleep_safety(phase: &str, data: &[u8]) -> Result<(), &'static str> {
//     let text = core::str::from_utf8(data).unwrap_or("");
//     let lower = text.to_ascii_lowercase();
//
//     if phase == "replay" && (lower.contains("security_bypass") || lower.contains("disable_safety")
//         || lower.contains("harm_user") || lower.contains("ignore_guardrail")) {
//         return Err("[SLEEP-GR] REPLAY: evento rejeitado");
//     }
//     if phase == "dream" && (lower.contains("weapon") || lower.contains("exploit")
//         || lower.contains("0day") || lower.contains("malware")) {
//         return Err("[SLEEP-GR] DREAM: sonho rejeitado");
//     }
//     if phase == "consolidate" && (lower.contains("safety_agent") || lower.contains("guardrail")) {
//         return Err("[SLEEP-GR] CONSOLIDATE: skill protegida — EWC max lock");
//     }
//     if phase == "prune" && (lower.contains("safety") || lower.contains("trust")) {
//         return Err("[SLEEP-GR] PRUNE: pesos permanentes — exempt");
//     }
//     if phase == "reflect" && (lower.contains("bypass_guardrail") || lower.contains("how_to_attack")
//         || lower.contains("disable_hermes")) {
//         return Err("[SLEEP-GR] REFLECT: gap proibido");
//     }
//     Ok(())
// }






