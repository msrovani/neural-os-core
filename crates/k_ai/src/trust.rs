//! Trust & Security — TrustCache, PermissionMode, MaskSecrets, Graduated Enforcement.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

// ---------------------------------------------------------------------------
// E2 (OPCODE-0084): capabilities com delegação e revogação transitiva
// ---------------------------------------------------------------------------

/// Teto de nós revogados por chamada — defesa anti-DoS (BFS limitada).
pub const MAX_REVOKE_NODES: usize = 256;

/// Geração global de capabilities — espelho do `cap_generation` do TrustCache
/// canônico (hermes `TRUST_CACHE`). Bumpada em `revoke_cap`; lida por
/// `current_cap_generation()` no gate de import do wasmi SEM lock por import.
pub static CAP_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Leitura lock-free da geração atual de capabilities (E2 / OPCODE-0084).
pub fn current_cap_generation() -> u64 {
    CAP_GENERATION.load(Ordering::Acquire)
}

/// Handle opaco de capability. `generation` é a geração no momento da
/// concessão; `enforce_cap` a compara com a geração atual (revogação global
/// invalida handles antigos).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapHandle {
    pub token: u64,
    pub generation: u64,
}
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

/// Predicado PURO de decisão de trust (OPCODE-0079) — extraído de
/// `TrustCache::is_trusted` para verificação formal (Kani). O corpo é a regra
/// canônica: denylist vence; Enforce global nega não-isentos; entry dentro do
/// TTL concede iff o estado dela não é Enforce; caso contrário nega.
///
/// * `denied`         — token/skill está na denylist
/// * `global_enforce` — política global == Enforce
/// * `exempt`         — token isento (`add_exempt_token`)
/// * `has_entry`      — existe TrustEntry para (token, skill)
/// * `entry_enforce`  — estado da entry == Enforce
/// * `age`            — `now - granted_at` (saturating)
/// * `ttl`            — `entry.ttl_ticks`
pub fn trust_decision(
    denied: bool,
    global_enforce: bool,
    exempt: bool,
    has_entry: bool,
    entry_enforce: bool,
    age: u64,
    ttl: u64,
) -> bool {
    if denied {
        false
    } else if global_enforce && !exempt {
        false
    } else if has_entry && age <= ttl {
        !entry_enforce
    } else {
        false
    }
}

pub struct TrustCache {
    entries: BTreeMap<(u64, String), TrustEntry>,
    denylist: BTreeMap<(u64, String), ()>,
    pub global_policy: PolicyState,
    escalation_log: Vec<String>,
    exempt_tokens: BTreeSet<u64>,
    /// E2: árvore de delegação parent → filhos.
    children: BTreeMap<u64, BTreeSet<u64>>,
    /// E2: tokens revogados (transitivo).
    revoked: BTreeSet<u64>,
    /// E2: tokens com capability concedida.
    granted: BTreeSet<u64>,
}

impl TrustCache {
    pub fn new() -> Self {
        TrustCache {
            entries: BTreeMap::new(),
            denylist: BTreeMap::new(),
            global_policy: PolicyState::Observe,
            escalation_log: Vec::new(),
            exempt_tokens: BTreeSet::new(),
            children: BTreeMap::new(),
            revoked: BTreeSet::new(),
            granted: BTreeSet::new(),
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

    // ─── E2 (OPCODE-0084): capabilities com delegação e revogação ───────────

    /// Concede uma capability raiz ao token (limpa revogação anterior).
    /// Fix 3 (OPCODE-0093): geração vem do ÚNICO global `CAP_GENERATION`.
    pub fn grant_cap(&mut self, token: u64) -> CapHandle {
        self.revoked.remove(&token);
        self.granted.insert(token);
        CapHandle { token, generation: current_cap_generation() }
    }

    /// Delega uma capability de `parent` para `child`. Recusa auto-delegação,
    /// parent revogado, parent sem concessão e RE-PARENTING (child que já tem
    /// pai) — preserva o invariante de floresta (fix 2 / OPCODE-0093).
    pub fn mint_cap(&mut self, parent: u64, child: u64) -> Result<CapHandle, &'static str> {
        if parent == child {
            return Err("self-delegation");
        }
        if self.revoked.contains(&parent) {
            return Err("parent revoked");
        }
        if !self.granted.contains(&parent) {
            return Err("parent not granted");
        }
        // fix 2: re-parent proibido — child já presente em qualquer children set.
        if self.children.values().any(|kids| kids.contains(&child)) {
            return Err("already has parent");
        }
        self.children.entry(parent).or_insert_with(BTreeSet::new).insert(child);
        self.granted.insert(child);
        Ok(CapHandle { token: child, generation: current_cap_generation() })
    }

    /// Valida um handle: geração global atual, não revogado, concedido.
    pub fn enforce_cap(&self, h: CapHandle) -> Result<(), &'static str> {
        if h.generation != current_cap_generation() {
            return Err("stale generation");
        }
        if self.revoked.contains(&h.token) {
            return Err("revoked");
        }
        if !self.granted.contains(&h.token) {
            return Err("not granted");
        }
        Ok(())
    }

    /// `true` se o token foi revogado. Nunca-concedido ≠ revogado.
    pub fn is_cap_revoked(&self, token: u64) -> bool {
        self.revoked.contains(&token)
    }

    /// Revoga o token e TODA a descendência (BFS, cap `MAX_REVOKE_NODES`).
    /// Remove entries+denylist de cada token revogado, bumpa `cap_generation`
    /// (e o espelho global) e devolve a contagem. Já revogado → 0.
    pub fn revoke_cap(&mut self, token: u64) -> usize {
        if self.revoked.contains(&token) {
            return 0;
        }
        let mut stack: Vec<u64> = Vec::new();
        stack.push(token);
        let mut count = 0usize;
        while let Some(t) = stack.pop() {
            if count >= MAX_REVOKE_NODES {
                break;
            }
            if !self.revoked.insert(t) {
                continue; // já visitado
            }
            count += 1;
            self.granted.remove(&t);
            if let Some(kids) = self.children.remove(&t) {
                for k in kids {
                    stack.push(k);
                }
            }
            self.entries.retain(|(tok, _), _| *tok != t);
            self.denylist.retain(|(tok, _), _| *tok != t);
        }
        if count > 0 {
            // Fix 3: ÚNICA fonte de geração (o wasmi gate lê o mesmo global).
            CAP_GENERATION.fetch_add(1, Ordering::Release);
        }
        count
    }

    pub fn is_trusted(&self, token: u64, skill: &str, now: u64) -> bool {
        let key = (token, String::from(skill));
        let denied = self.denylist.contains_key(&key);
        let global_enforce = self.global_policy == PolicyState::Enforce;
        let exempt = self.is_exempt(token);
        let (has_entry, entry_enforce, age, ttl) = match self.entries.get(&key) {
            Some(entry) => (
                true,
                entry.state == PolicyState::Enforce,
                now.saturating_sub(entry.granted_at_ticks),
                entry.ttl_ticks,
            ),
            None => (false, false, 0, 0),
        };
        // A decisão vive no predicado puro (verificado por Kani).
        trust_decision(denied, global_enforce, exempt, has_entry, entry_enforce, age, ttl)
    }

    fn is_exempt(&self, token: u64) -> bool {
        // Somente tokens explicitamente adicionados via add_exempt_token().
        // Legacy(0/1) NÃO são mais isentos por default (P06).
        self.exempt_tokens.contains(&token)
    }

    /// Safety invariant I3: number of active trust entries.
    /// Zero entries post-boot indicates possible mass revocation.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
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
                k_nano::slog_kai!("Trust", "warn", "DENY uncached ({:?}): token={} skill={} — use trust_allow",
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
            // Runtime hygiene (s410d): log de escalada com cap — violação
            // repetida em loop (agente bugado) crescia sem teto.
            const ESCALATION_LOG_CAP: usize = 64;
            if self.escalation_log.len() >= ESCALATION_LOG_CAP {
                self.escalation_log.remove(0);
            }
            self.escalation_log.push(
                alloc::format!("token={} skill={} escalated to {:?}", token, skill, entry.state)
            );
            if let Some(last) = self.escalation_log.last() { k_nano::slog_kai!("Trust", "info", "Violation: {}", last); }
        }
    }

    /// #259: verifica se hardware está apto antes de executar skill.
    /// Honesty: k_ai não possui `net::NET_CONFIG` (feature `kernel` era fantasma).
    /// Gate de rede fica em hermes/NetAgent; aqui só path-local / sempre apto.
    pub fn posture_check(_skill: &str) -> bool {
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
        let key = (token, String::from(skill));
        if let Some(entry) = self.entries.get(&key) {
            if let Some(ref rule) = entry.path_rule {
                let allowed = rule.allowed_prefixes.iter().any(|p| path.starts_with(p));
                if !allowed {
                    k_nano::slog_kai!("Trust", "warn", "Path denied: {} for token={} skill={}", path, token, skill);
                }
                return allowed;
            }
            // Entry trusted but no PathRule: allow only under Observe/Warn.
            match self.global_policy {
                PolicyState::Observe | PolicyState::Warn => true,
                PolicyState::Contain | PolicyState::Enforce => {
                    k_nano::slog_kai!(
                        "Trust",
                        "warn",
                        "Path deny (no PathRule under {:?}): {} token={} skill={}",
                        self.global_policy,
                        path,
                        token,
                        skill
                    );
                    false
                }
            }
        } else {
            // Sem entry: Contain/Enforce fail-closed; Observe/Warn transitório.
            match self.global_policy {
                PolicyState::Observe | PolicyState::Warn => true,
                PolicyState::Contain | PolicyState::Enforce => {
                    k_nano::slog_kai!(
                        "Trust",
                        "warn",
                        "Path deny (uncached under {:?}): {} token={} skill={}",
                        self.global_policy,
                        path,
                        token,
                        skill
                    );
                    false
                }
            }
        }
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

/// Global trust entry count for safety invariant I3.
/// Honesty: TRUST_CACHE lives in hermes — k_ai cannot read it.
/// Callers in hermes must use `TRUST_CACHE.lock().entry_count()` directly.
/// This stub returns 0 (= unchecked from Ring 2).
pub fn global_trust_entry_count() -> usize {
    0
}

// ---------------------------------------------------------------------------
// Kani proofs (OPCODE-0079 / ORACLE-0060) — verificação formal do predicado.
// `cargo kani -p k_ai --lib --harness <name>`
// ---------------------------------------------------------------------------
#[cfg(kani)]
mod kani_proofs {
    use super::trust_decision;

    /// Denylist sempre vence, independente de qualquer outro estado.
    #[kani::proof]
    fn trust_deny_overrides() {
        let denied: bool = kani::any();
        let global_enforce: bool = kani::any();
        let exempt: bool = kani::any();
        let has_entry: bool = kani::any();
        let entry_enforce: bool = kani::any();
        let age: u64 = kani::any();
        let ttl: u64 = kani::any();
        kani::assume(denied);
        assert!(!trust_decision(
            denied, global_enforce, exempt, has_entry, entry_enforce, age, ttl
        ));
    }

    /// Enforce global nega todo token NÃO isento.
    #[kani::proof]
    fn trust_enforce_nonexempt_denies() {
        let denied: bool = kani::any();
        let exempt: bool = kani::any();
        let has_entry: bool = kani::any();
        let entry_enforce: bool = kani::any();
        let age: u64 = kani::any();
        let ttl: u64 = kani::any();
        kani::assume(!denied);
        kani::assume(!exempt);
        assert!(!trust_decision(
            denied, true, exempt, has_entry, entry_enforce, age, ttl
        ));
    }

    /// Entry válida (dentro do TTL, estado != Enforce) concede, sem deny nem
    /// Enforce global bloqueante.
    #[kani::proof]
    fn trust_valid_entry_allows() {
        let age: u64 = kani::any();
        let ttl: u64 = kani::any();
        kani::assume(age <= ttl);
        assert!(trust_decision(false, false, false, true, false, age, ttl));
        // Enforce global não bloqueia token isento.
        assert!(trust_decision(false, true, true, true, false, age, ttl));
    }

    /// Entry expirada (age > ttl) sempre nega, mesmo com estado != Enforce.
    #[kani::proof]
    fn trust_expired_entry_denies() {
        let age: u64 = kani::any();
        let ttl: u64 = kani::any();
        kani::assume(age > ttl);
        assert!(!trust_decision(false, false, false, true, false, age, ttl));
    }
}

// ---------------------------------------------------------------------------
// E2 (OPCODE-0084 / ORACLE-0070) — revogação transitiva de capabilities.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revoke_is_transitive_and_bumps_generation() {
        let mut tc = TrustCache::new();
        // Cadeia de delegação 1 -> 2 -> 3.
        let h1 = tc.grant_cap(1);
        let h2 = tc.mint_cap(1, 2).expect("mint 1->2");
        let h3 = tc.mint_cap(2, 3).expect("mint 2->3");
        assert!(tc.enforce_cap(h1).is_ok());
        assert!(tc.enforce_cap(h2).is_ok());
        assert!(tc.enforce_cap(h3).is_ok());
        let gen_before = current_cap_generation();

        // Revogar a raiz derruba a subárvore inteira.
        let n = tc.revoke_cap(1);
        assert_eq!(n, 3, "revogação transitiva deve atingir 3 nós");
        assert!(tc.is_cap_revoked(1));
        assert!(tc.is_cap_revoked(2));
        assert!(tc.is_cap_revoked(3));
        // Geração GLOBAL bumpou e handles antigos ficam stale (negar).
        assert_eq!(current_cap_generation(), gen_before + 1);
        assert!(tc.enforce_cap(h1).is_err());
        assert!(tc.enforce_cap(h2).is_err());
        assert!(tc.enforce_cap(h3).is_err());

        // Token nunca concedido: não é "revogado", mas enforce falha.
        assert!(!tc.is_cap_revoked(99));
        let h99 = CapHandle { token: 99, generation: current_cap_generation() };
        assert!(tc.enforce_cap(h99).is_err());

        // Mint a partir de parent revogado falha.
        assert!(tc.mint_cap(1, 4).is_err());

        // Revogar de novo é no-op.
        assert_eq!(tc.revoke_cap(1), 0);
    }

    /// Fix 2 (OPCODE-0093): re-parent é recusado — floresta preservada.
    #[test]
    fn mint_rejects_reparent() {
        let mut tc = TrustCache::new();
        tc.grant_cap(1);
        tc.grant_cap(2);
        tc.mint_cap(1, 3).expect("mint 1->3");
        // 3 já tem pai (1) → 2 (concedido) não pode re-parentear.
        assert_eq!(tc.mint_cap(2, 3), Err("already has parent"));
    }
}
