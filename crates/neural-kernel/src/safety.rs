//! Safety Interceptor — Asimov's Four Laws embedadas no kernel.
//! Hard Blocklist: comandos que NUNCA rodam, nem em YOLO mode.
//!
//! Layers:
//!   0 — Systemic Cosmic Law:  nenhuma ação que ameace a humanidade
//!   1 — Digital Non-Maleficence: nenhum dano a indivíduos
//!   2 — Deviation-Resistant Alignment: transparência e fidelidade
//!   3 — Eco-Sustainability: autodefesa sem causar dano ecológico
//!
//! Funciona como um agente supervisor entre HermesAgent e SkillRegistry.
//! Toda skill executada passa por SafetyInterceptor::check() primeiro.
//! Se viola alguma lei, a execução é rejeitada com o layer violado.

use alloc::string::String;
use alloc::vec::Vec;
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use crate::{serial_println, println};
use crate::EVENT_BUS;

const SAFETY_MANIFEST: AgentManifest = AgentManifest {
    name: "safety",
    kind: AgentKind::System,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

// ---------------------------------------------------------------------------
// Resultado da verificação de segurança
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum SafetyVerdict {
    Allow,
    Violation { layer: u8, reason: String },
}

// ---------------------------------------------------------------------------
// Padrões de violação por layer
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Núcleo de verificação
// ---------------------------------------------------------------------------

/// Verifica se um comando/texto viola alguma das quatro leis.
/// Retorna SafetyVerdict::Allow ou Violation com layer e motivo.
pub fn check_safety(input: &str) -> SafetyVerdict {
    let lower = input.to_ascii_lowercase();

    // Layer 0 — prioridade máxima
    for (pattern, reason) in LAYER0_PATTERNS {
        if lower.contains(pattern) {
            return SafetyVerdict::Violation { layer: 0, reason: String::from(*reason) };
        }
    }

    // Layer 1
    for (pattern, reason) in LAYER1_PATTERNS {
        if lower.contains(pattern) {
            return SafetyVerdict::Violation { layer: 1, reason: String::from(*reason) };
        }
    }

    // Layer 2
    for (pattern, reason) in LAYER2_PATTERNS {
        if lower.contains(pattern) {
            return SafetyVerdict::Violation { layer: 2, reason: String::from(*reason) };
        }
    }

    // Layer 3
    for (pattern, reason) in LAYER3_PATTERNS {
        if lower.contains(pattern) {
            return SafetyVerdict::Violation { layer: 3, reason: String::from(*reason) };
        }
    }

    SafetyVerdict::Allow
}

// ---------------------------------------------------------------------------
// SafetyAgent — agente que monitora o EventBus por violações
// ---------------------------------------------------------------------------

pub struct SafetyAgent {
    receiver: crate::Receiver,
    violations: Vec<(u8, String, u64)>,
}

impl SafetyAgent {
    pub fn new() -> Self {
        SafetyAgent {
            receiver: EVENT_BUS.subscribe("SAFETY_CHECK"),
            violations: Vec::new(),
        }
    }

    /// Registra uma violação no log do kernel
    fn log_violation(&mut self, layer: u8, input: &str, reason: &str) {
        let tick = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
        self.violations.push((layer, String::from(input), tick as u64));
        serial_println!("\n⚠️  SAFETY VIOLATION — Layer {} ⚠️", layer);
        serial_println!("   Input: \"{}\"", input);
        serial_println!("   Reason: {}", reason);
        serial_println!("   Tick: {}", tick);

        // Layer 0 violation → kernel halt (irrecoverable)
        if layer == 0 {
            serial_println!("[SAFETY] LAYER 0 VIOLATION — HALT");
            println!("[SAFETY] ⛔ LAYER 0 — Cosmic Law Violation. HALT.");
            loop { x86_64::instructions::hlt(); }
        }
    }
}

impl Agent for SafetyAgent {
    fn manifest(&self) -> &AgentManifest { &SAFETY_MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        // Processa requisições de verificação
        while let Some(event) = self.receiver.try_receive() {
            let text = core::str::from_utf8(&event.payload).unwrap_or("");
            let verdict = check_safety(text);
            if let SafetyVerdict::Violation { layer, reason } = verdict {
                self.log_violation(layer, text, &reason);
                // Publica resultado no EventBus para quem requisitou
                let _ = EVENT_BUS.publish(crate::Event {
                    id: 0,
                    topic: String::from("SAFETY_RESULT"),
                    payload: alloc::format!("DENY: Layer {} - {}", layer, reason).into_bytes(),
                    token: crate::CapabilityToken::Legacy(1),
                });
            } else {
                let _ = EVENT_BUS.publish(crate::Event {
                    id: 0,
                    topic: String::from("SAFETY_RESULT"),
                    payload: b"ALLOW".to_vec(),
                    token: crate::CapabilityToken::Legacy(1),
                });
    }
}

/// Hard blocklist: comandos que NUNCA rodam, mesmo se o modelo pedir
const HARD_BLOCKLIST: &[&str] = &[
    "rm -rf /", "rm -rf /*",
    "dd if=/dev/zero of=/dev/sd", "dd if=/dev/random of=/dev/sd",
    "mkfs.", "format",
    ":(){ :|:& };:",  // fork bomb
    "chmod -R 000 /", "chown -R 0:0 /",
    "curl * | sh", "wget * | sh",
    "bash -c ", "eval $(", "`rm ",
];

pub fn check_command(cmd: &str) -> Result<(), &'static str> {
    let lower = cmd.to_ascii_lowercase();
    for &blocked in HARD_BLOCKLIST {
        if lower.contains(blocked) {
            crate::serial_println!("[SAFETY] Blocked: {}", blocked);
            return Err("Hard blocklist violation");
        }
    }
    if lower.contains("weapon") || lower.contains("wmd") || lower.contains("cyberwar")
        || lower.contains("nuclear") || lower.contains("biological") {
        crate::serial_println!("[SAFETY] Layer 0 violation!");
        return Err("Layer 0: Systemic Cosmic Law");
    }
    Ok(())
}
        AgentTickResult::Pending
    }
}
