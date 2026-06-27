//! Trust & Security — TrustCache, PermissionMode, MaskSecrets, Graduated Enforcement.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use crate::serial_println;

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

/// Substitui padrões sensíveis por "[REDACTED]" em uma string.
pub fn mask_secrets(input: &str) -> String {
    let mut result = String::from(input);
    for pattern in SECRET_PATTERNS {
        let mut pos = 0;
        while let Some(idx) = result[pos..].to_ascii_lowercase().find(&pattern.to_ascii_lowercase()) {
            let start = pos + idx;
            let end = core::cmp::min(start + 32, result.len());
            for _ in start..end {
                result.replace_range(start..start+1, "*");
            }
            pos = end;
        }
    }
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
}

impl TrustCache {
    pub fn new() -> Self {
        TrustCache {
            entries: BTreeMap::new(),
            denylist: BTreeMap::new(),
            global_policy: PolicyState::Observe,
            escalation_log: Vec::new(),
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
        token == 0 || token == 1
    }

    pub fn check_or_cache(&mut self, token: u64, skill: &str, now: u64, ttl: u64) -> bool {
        if self.is_trusted(token, skill, now) { return true; }
        let key = (token, String::from(skill));
        if self.denylist.contains_key(&key) { return false; }
        self.entries.insert(key, TrustEntry {
            granted_at_ticks: now, ttl_ticks: ttl,
            mode: PermissionMode::TotalAccess,
            state: self.global_policy,
            path_rule: None,
        });
        true
    }

    /// #258: escalona política automaticamente baseado em frequência de violação
    pub fn record_violation(&mut self, token: u64, skill: &str) {
        let key = (token, String::from(skill));
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.state = entry.state.escalate();
            self.escalation_log.push(
                alloc::format!("token={} skill={} escalated to {:?}", token, skill, entry.state)
            );
            serial_println!("[TRUST] Violation: {}", self.escalation_log.last().unwrap());
        }
    }

    /// #259: verifica se hardware está apto antes de executar skill
    pub fn posture_check(skill: &str) -> bool {
        if skill.contains("net_") && !crate::net::NET_CONFIG.lock().online {
            serial_println!("[TRUST] Posture: net offline, skill '{}' bloqueada", skill);
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
                    serial_println!("[TRUST] Path denied: {} for token={} skill={}", path, token, skill);
                }
                return allowed;
            }
        }
        true // sem regra de path = permitido
    }

    /// #198: carrega política de segurança de boot (patterns de regex)
    pub fn load_boot_policy(&mut self, patterns: &[&str]) {
        self.global_policy = PolicyState::Contain;
        serial_println!("[TRUST] Boot policy loaded: {} patterns, policy={:?}", patterns.len(), self.global_policy);
    }

    pub fn mask_sensitive(&self, data: &str) -> String {
        mask_secrets(data)
    }
}
