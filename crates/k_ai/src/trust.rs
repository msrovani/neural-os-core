//! Trust & Security — TrustCache, PermissionMode, MaskSecrets, Graduated Enforcement.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;
// ---------------------------------------------------------------------------
// #166 Multi-mode Trust
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum PermissionMode {
    /// Token totalmente autorizado — sem restrições
    TotalAccess,
    /// Toda execução requer confirmação do usuário
    AskEveryTime,
    /// Autorizado apenas dentro de um escopo (ex: skill específica, pasta)
    Scoped(Vec<String>),
}

// ---------------------------------------------------------------------------
// #258 Graduated Enforcement
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PolicyState {
    /// Apenas observa e loga — sem bloqueio
    Observe,
    /// Loga aviso mas permite execução
    Warn,
    /// Contém — permite execução mas limita recursos (ex: sem rede)
    Contain,
    /// Bloqueia totalmente
    Enforce,
}

impl PolicyState {
    pub fn escalate(&self) -> Self {
        match self {
            PolicyState::Observe => PolicyState::Warn,
            PolicyState::Warn => PolicyState::Contain,
            PolicyState::Contain => PolicyState::Enforce,
            PolicyState::Enforce => PolicyState::Enforce,
        }
    }
}

// ---------------------------------------------------------------------------
// #257 Mask Secrets — padrões sensíveis
// ---------------------------------------------------------------------------

const SECRET_PATTERNS: &[&str] = &[
    "API_KEY", "SECRET", "PASSWORD", "TOKEN", "BEARER",
    "sk-", "ghp_", "gho_", "ghu_", "xoxb-", "xoxp-",
];

/// Substitui todas as ocorrências de `mask` por `*` em uma string (UTF-8 safe).
pub fn mask_secrets(input: &str, mask: &str) -> alloc::string::String {
    let mut result = alloc::string::String::with_capacity(input.len());
    let mut remaining = input;
    while let Some(pos) = remaining.find(mask) {
        // Copy everything before the mask
        result.push_str(&remaining[..pos]);
        // Replace mask with asterisks
        result.push_str(&"*".repeat(mask.len()));
        remaining = &remaining[pos + mask.len()..];
    }
    result.push_str(remaining);
    result
}

// ---------------------------------------------------------------------------
// #256 Path Confinement — allowlist de paths por skill
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PathRule {
    pub allowed_prefixes: Vec<String>,
    pub blocked_patterns: Vec<String>,
}

// ---------------------------------------------------------------------------
// TrustCache com suporte a Multi-mode + Graduated Enforcement
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TrustEntry {
    pub granted_at_ticks: u64,
    pub ttl_ticks: u64,
    pub mode: PermissionMode,
    pub state: PolicyState,
    pub path_rule: Option<PathRule>,
}

pub struct TrustCache {
    entries: BTreeMap<(u64, String), TrustEntry>,
    denylist: BTreeMap<(u64, String), ()>,
    pub global_policy: PolicyState,
    escalation_log: Vec<String>,
    exempt_tokens: BTreeSet<u64>,
}

impl TrustCache {
    pub fn new() -> Self {
        TrustCache {
            entries: BTreeMap::new(),
            denylist: BTreeMap::new(),
            global_policy: PolicyState::Observe,
            escalation_log: Vec::new(),
            exempt_tokens: BTreeSet::new(),
        }
    }

    /// #166: trust allow com modo de permissão
    pub fn trust_allow_with_mode(&mut self, token: u64, skill: &str, now: u64, mode: PermissionMode) {
        let key = (token, String::from(skill));
        self.denylist.remove(&key);
        self.entries.insert(key, TrustEntry {
            granted_at_ticks: now,
            ttl_ticks: u64::MAX,
            mode,
            state: self.global_policy,
            path_rule: None,
        });
    }

    pub fn trust_allow(&mut self, token: u64, skill: &str, now: u64) {
        self.trust_allow_with_mode(token, skill, now, PermissionMode::TotalAccess);
    }

    /// Chave composta (token, agent, skill) — ADR-0042 N2 / AGENTS.md.
    fn agent_skill_key(agent: &str, skill: &str) -> String {
        alloc::format!("{}:{}", agent, skill)
    }

    /// Concede trust por (token, agent, skill).
    pub fn trust_allow_agent(&mut self, token: u64, agent: &str, skill: &str, now: u64) {
        let key = Self::agent_skill_key(agent, skill);
        self.trust_allow(token, &key, now);
        k_nano::slog_kai!("Trust", "info", "allow (token,agent,skill)=({},{},{})", token, agent, skill);
    }

    pub fn is_trusted_agent(&self, token: u64, agent: &str, skill: &str, now: u64) -> bool {
        self.is_trusted(token, &Self::agent_skill_key(agent, skill), now)
    }

    pub fn check_or_cache_agent(
        &mut self,
        token: u64,
        agent: &str,
        skill: &str,
        now: u64,
        ttl: u64,
    ) -> bool {
        self.check_or_cache(token, &Self::agent_skill_key(agent, skill), now, ttl)
    }

    pub fn trust_deny(&mut self, token: u64, skill: &str) {
        let key = (token, String::from(skill));
        self.entries.remove(&key);
        self.denylist.insert(key, ());
    }

    pub fn is_trusted(&self, token: u64, skill: &str, now: u64) -> bool {
        let key = &(token, String::from(skill));
        if self.denylist.contains_key(key) { return false; }
        if self.global_policy == PolicyState::Enforce && !self.is_exempt(token) { return false; }
        if let Some(entry) = self.entries.get(key) {
            if now.saturating_sub(entry.granted_at_ticks) <= entry.ttl_ticks {
                return entry.state != PolicyState::Enforce;
            }
        }
        false
    }

    fn is_exempt(&self, token: u64) -> bool {
        // Somente tokens explicitamente adicionados via add_exempt_token().
        // Legacy(0/1) NÃO são mais isentos por default (P06).
        self.exempt_tokens.contains(&token)
    }

    pub fn add_exempt_token(&mut self, token: u64) {
        self.exempt_tokens.insert(token);
        k_nano::slog_kai!("Trust", "info", "exempt token={} (sistema)", token);
    }

    // ponytail: Contain — skill sem trust_allow é negada pós-boot.
    // ponytail: Enforce — skills de sistema com Legacy(1) passam (add_exempt_token(1)).
    // Ambos verificados por check_or_cache() antes de cada execute_skill.
    /// Verifica confiança. NÃO auto-concede TotalAccess (P05).
    /// Observe/Warn: permite transitório sem cachear. Contain/Enforce: nega até trust_allow.
    pub fn check_or_cache(&mut self, token: u64, skill: &str, now: u64, _ttl: u64) -> bool {
        if self.is_trusted(token, skill, now) {
            return true;
        }
        let key = (token, String::from(skill));
        if self.denylist.contains_key(&key) {
            return false;
        }
        match self.global_policy {
            PolicyState::Observe | PolicyState::Warn => {
                k_nano::slog_kai!("Trust", "info", "transient allow ({:?}): token={} skill={}",
                    self.global_policy,
                    token,
                    skill);
                true
            }
            PolicyState::Contain | PolicyState::Enforce => {
                k_nano::slog_kai!("Trust", "info", "DENY uncached ({:?}): token={} skill={} — use trust_allow",
                    self.global_policy,
                    token,
                    skill);
                false
            }
        }
    }

    /// #258: escalona política automaticamente baseado em frequência de violação
    pub fn record_violation(&mut self, token: u64, skill: &str) {
        let key = (token, String::from(skill));
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.state = entry.state.escalate();
            self.escalation_log.push(
                alloc::format!("token={} skill={} escalated to {:?}", token, skill, entry.state)
            );
            if let Some(last) = self.escalation_log.last() { k_nano::slog_kai!("Trust", "info", "Violation: {}", last); }
        }
    }

    /// #259: verifica se hardware está apto antes de executar skill
    pub fn posture_check(_skill: &str) -> bool {
        #[cfg(feature = "kernel")]
        if _skill.contains("net_") && !crate::net::NET_CONFIG.lock().online {
            k_nano::slog_kai!("Trust", "info", "Posture: net offline, skill '{}' bloqueada", _skill);
            return false;
        }
        true
    }

    /// #256: Path Confinement — skill só acessa paths do allowlist
    pub fn set_path_rule(&mut self, token: u64, skill: &str, prefixes: Vec<&str>) {
        let key = (token, String::from(skill));
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.path_rule = Some(PathRule {
                allowed_prefixes: prefixes.iter().map(|s| String::from(*s)).collect(),
                blocked_patterns: Vec::new(),
            });
        }
    }

    pub fn check_path(&self, token: u64, skill: &str, path: &str) -> bool {
        let key = &(token, String::from(skill));
        if let Some(entry) = self.entries.get(key) {
            if let Some(ref rule) = entry.path_rule {
                let allowed = rule.allowed_prefixes.iter().any(|p| path.starts_with(p));
                if !allowed {
                    k_nano::slog_kai!("Trust", "info", "Path denied: {} for token={} skill={}", path, token, skill);
                }
                return allowed;
            }
        }
        true // sem regra de path = permitido
    }

    /// #198: carrega política de segurança de boot (patterns de regex)
    pub fn load_boot_policy(&mut self, patterns: &[&str]) {
        self.global_policy = PolicyState::Contain;
        k_nano::slog_kai!("Trust", "info", "Boot policy loaded: {} patterns, policy={:?}", patterns.len(), self.global_policy);
    }

    pub fn mask_sensitive(&self, data: &str) -> String {
        let mut result = String::from(data);
        for pattern in SECRET_PATTERNS {
            result = mask_secrets(&result, pattern);
        }
        result
    }

    /// #364: Zero-Trust Syscall — avalia permissão por classe
    pub fn check_syscall(&self, token: u64, skill: &str, class: SyscallClass) -> bool {
        let now = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
        match class {
            SyscallClass::ReadOnly => true,
            SyscallClass::Ephemeral => self.is_trusted(token, skill, now as u64),
            SyscallClass::Persistent => self.is_exempt(token),
            SyscallClass::Hardware => false,
        }
    }

}

/// #364: Quatro classes de syscall zero-trust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SyscallClass {
    /// Leitura de dados — sempre permitido (sem efeito colateral)
    ReadOnly,
    /// Alocação efêmera — permitido com budget
    Ephemeral,
    /// Escrita persistente — requer autorização explícita
    Persistent,
    /// Acesso a hardware — sempre negado por padrão
    Hardware,
}

impl SyscallClass {
    pub fn name(&self) -> &'static str {
        match self {
            SyscallClass::ReadOnly => "read",
            SyscallClass::Ephemeral => "ephemeral",
            SyscallClass::Persistent => "persistent",
            SyscallClass::Hardware => "hardware",
        }
    }
    pub fn requires_approval(&self) -> bool {
        matches!(self, SyscallClass::Persistent | SyscallClass::Hardware)
    }
}
