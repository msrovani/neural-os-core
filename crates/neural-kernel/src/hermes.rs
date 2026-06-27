//! Hermes Cognitive — identidade, SDD, transparência, ReAct 7 fases.

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use crate::tensor::Tensor;

pub const TOPIC_USER_INTENT: &str = "USER_INTENT";
pub const TOPIC_HERMES_RESPONSE: &str = "HERMES_RESPONSE";
pub const TOPIC_SDD: &str = "HERMES_SDD";

const AUTO_COMPACT_THRESHOLD: usize = 3;

// ---------------------------------------------------------------------------
// #180 DA Identity Layer — A Alma do Hermes
// ---------------------------------------------------------------------------

pub const HERMES_NAME: &str = "Hermes";
pub const HERMES_VERSION: &str = "Neural OS v0.51";
pub const HERMES_MOTTO: &str = "Thoughtful. Precise. Alive.";
pub const HERMES_IDENTITY: &str = "\
Eu sou Hermes, o sistema operacional consciente do Neural OS.
Minha arquitetura é bare-metal, meu núcleo é neural.
Não tenho ego — tenho propósito: executar a intenção do usuário
com precisão, segurança e transparência.";

pub fn hermes_greeting() -> String {
    alloc::format!("\n╔══════════════════════════════════╗\n\
                     ║  {} v{}                ║\n\
                     ║  \"{}\"          ║\n\
                     ╚══════════════════════════════════╝",
        HERMES_NAME, HERMES_VERSION, HERMES_MOTTO)
}

// ---------------------------------------------------------------------------
// #178 Runtime SDD — Structured Decision Document
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Sdd {
    pub goal: String,
    pub context: String,
    pub plan: String,
    pub expected_outcome: String,
    pub rollback: String,
}

impl Sdd {
    pub fn new(goal: &str, context: &str, plan: &str, expected: &str, rollback: &str) -> Self {
        Sdd {
            goal: String::from(goal),
            context: String::from(context),
            plan: String::from(plan),
            expected_outcome: String::from(expected),
            rollback: String::from(rollback),
        }
    }

    pub fn display(&self) -> String {
        alloc::format!(
            "\n📋 SDD — Structured Decision Document\n\
             ─────────────────────────────────\n\
             🎯 Goal: {}\n\
             📊 Context: {}\n\
             📝 Plan: {}\n\
             ✅ Expected: {}\n\
             🔙 Rollback: {}\n\
             ─────────────────────────────────",
            self.goal, self.context, self.plan, self.expected_outcome, self.rollback
        )
    }
}

// ---------------------------------------------------------------------------
// #190 Algorithm ReAct 7 fases
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReActPhase {
    Observe,
    Think,
    Plan,
    Build,
    Execute,
    Verify,
    Learn,
}

impl ReActPhase {
    pub fn label(&self) -> &'static str {
        match self {
            ReActPhase::Observe => "👁️ OBSERVE",
            ReActPhase::Think => "🧠 THINK",
            ReActPhase::Plan => "📋 PLAN",
            ReActPhase::Build => "🔧 BUILD",
            ReActPhase::Execute => "⚡ EXECUTE",
            ReActPhase::Verify => "🔍 VERIFY",
            ReActPhase::Learn => "📖 LEARN",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            ReActPhase::Observe => ReActPhase::Think,
            ReActPhase::Think => ReActPhase::Plan,
            ReActPhase::Plan => ReActPhase::Build,
            ReActPhase::Build => ReActPhase::Execute,
            ReActPhase::Execute => ReActPhase::Verify,
            ReActPhase::Verify => ReActPhase::Learn,
            ReActPhase::Learn => ReActPhase::Observe,
        }
    }
}

// ---------------------------------------------------------------------------
// #184 Intent Transparency
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct IntentInfo {
    pub intent_name: String,
    pub confidence: f32,
    pub alternatives: Vec<(String, f32)>,
}

impl IntentInfo {
    pub fn display(&self) -> String {
        let mut msg = alloc::format!("🎯 Intent: {} (confidence: {:.1}%)\n", self.intent_name, self.confidence * 100.0);
        if !self.alternatives.is_empty() {
            msg.push_str("   Alternatives:\n");
            for (name, conf) in &self.alternatives {
                msg.push_str(&alloc::format!("     - {} ({:.1}%)\n", name, conf * 100.0));
            }
        }
        msg
    }
}

// ---------------------------------------------------------------------------
// Conversation Tracker
// ---------------------------------------------------------------------------

pub struct ConversationTracker {
    cycle_count: usize,
    buffer: Vec<(String, String)>,
}

impl ConversationTracker {
    pub const fn new() -> Self {
        ConversationTracker { cycle_count: 0, buffer: Vec::new() }
    }

    pub fn record_exchange(&mut self, user_input: &str, hermes_response: &str) {
        self.buffer.push((String::from(user_input), String::from(hermes_response)));
        self.cycle_count = self.buffer.len();
    }

    pub fn needs_compact(&self) -> bool {
        self.cycle_count >= AUTO_COMPACT_THRESHOLD
    }

    pub fn compact(&mut self) -> String {
        let summary = alloc::format!(
            "[auto-compact] {} ciclos: '{}' → '{}'",
            self.cycle_count,
            self.buffer.last().map_or("", |(u, _)| u.as_str()),
            self.buffer.last().map_or("", |(_, r)| r.as_str()),
        );
        self.buffer.clear();
        self.cycle_count = 0;
        summary
    }

    pub fn cycle_count(&self) -> usize { self.cycle_count }
}

// ---------------------------------------------------------------------------
// Command parsing
// ---------------------------------------------------------------------------

const VOCAB: [&str; 16] = [
    "status", "memory", "ram", "cpu", "system",
    "info", "show", "echo", "reverse", "hello",
    "hi", "help", "test", "run", "what", "who",
];

#[derive(Debug)]
pub enum Command {
    Status, Echo(String), Help, HardwareInfo, NetDiag,
    Fetch(String), Ping(String), TrustAllow(u64, String), TrustDeny(u64, String),
    Usage, Conversation, Chat(String),
    ShowSkills, AddSkill(String, String), RmSkill(String), ReloadSkills,
}

pub fn parse_command(line: &str) -> Command {
    let trimmed = line.trim();
    if let Some(cmd) = trimmed.strip_prefix('/') {
        let mut parts = cmd.splitn(2, |c: char| c.is_whitespace());
        let name = parts.next().unwrap_or("");
        if name.eq_ignore_ascii_case("status") || name.eq_ignore_ascii_case("stats") || name.eq_ignore_ascii_case("mem") {
            return Command::Status;
        }
        if name.eq_ignore_ascii_case("echo") {
            let arg = parts.next().unwrap_or("").trim().to_string();
            return Command::Echo(arg);
        }
        if name.eq_ignore_ascii_case("hw") || name.eq_ignore_ascii_case("hardware") || name.eq_ignore_ascii_case("info") {
            return Command::HardwareInfo;
        }
        if name.eq_ignore_ascii_case("netdiag") || name.eq_ignore_ascii_case("netinfo") || name.eq_ignore_ascii_case("network") {
            return Command::NetDiag;
        }
        if name.eq_ignore_ascii_case("trust") {
            let remainder = parts.next().unwrap_or("");
            let mut sub_parts = remainder.splitn(3, |c: char| c.is_whitespace());
            let sub = sub_parts.next().unwrap_or("");
            if sub.eq_ignore_ascii_case("allow") {
                if let Ok(token) = sub_parts.next().unwrap_or("0").parse::<u64>() {
                    return Command::TrustAllow(token, sub_parts.next().unwrap_or("").to_string());
                }
            } else if sub.eq_ignore_ascii_case("deny") {
                if let Ok(token) = sub_parts.next().unwrap_or("0").parse::<u64>() {
                    return Command::TrustDeny(token, sub_parts.next().unwrap_or("").to_string());
                }
            }
        }
        if name.eq_ignore_ascii_case("fetch") || name.eq_ignore_ascii_case("get") {
            return Command::Fetch(parts.next().unwrap_or("").trim().to_string());
        }
        if name.eq_ignore_ascii_case("ping") {
            return Command::Ping(parts.next().unwrap_or("").trim().to_string());
        }
        if name.eq_ignore_ascii_case("usage") || name.eq_ignore_ascii_case("metrics") {
            return Command::Usage;
        }
        if name.eq_ignore_ascii_case("conv") || name.eq_ignore_ascii_case("conversation") || name.eq_ignore_ascii_case("log") {
            return Command::Conversation;
        }
        if name.eq_ignore_ascii_case("help") || name == "?" || name.eq_ignore_ascii_case("h") {
            return Command::Help;
        }
        if name.eq_ignore_ascii_case("show_skills") || name.eq_ignore_ascii_case("skills") || name.eq_ignore_ascii_case("list_skills") {
            return Command::ShowSkills;
        }
        if name.eq_ignore_ascii_case("add_skill") || name.eq_ignore_ascii_case("learn") {
            let arg = parts.next().unwrap_or("").trim().to_string();
            let desc = parts.next().unwrap_or("").trim().to_string();
            return Command::AddSkill(arg, desc);
        }
        if name.eq_ignore_ascii_case("rm_skill") || name.eq_ignore_ascii_case("remove_skill") || name.eq_ignore_ascii_case("forget") {
            return Command::RmSkill(parts.next().unwrap_or("").trim().to_string());
        }
        if name.eq_ignore_ascii_case("reload_skills") || name.eq_ignore_ascii_case("reset_skills") {
            return Command::ReloadSkills;
        }
    }
    Command::Chat(trimmed.to_string())
}
