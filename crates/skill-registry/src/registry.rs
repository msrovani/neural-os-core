use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use event_bus::CapabilityToken;

use crate::skill::Skill;

#[derive(Clone, Debug)]
pub struct ToolPolicy {
    pub enabled: bool,
    pub auto_approve: bool,
}

impl Default for ToolPolicy {
    fn default() -> Self {
        // Deny-by-default CapGate: without explicit policy, disabled.
        // Boot must `set_policy("*", ToolPolicy { enabled: true, .. })`.
        ToolPolicy {
            enabled: false,
            auto_approve: false,
        }
    }
}

/// Entrada tipada de `list_skills` (SESSION_377 — fim do blob `"name: desc"`).
#[derive(Clone, Debug)]
pub struct SkillListEntry {
    pub name: String,
    pub description: String,
    pub policy: ToolPolicy,
}

pub struct SkillRegistry {
    skills: BTreeMap<String, Box<dyn Skill>>,
    policies: BTreeMap<String, ToolPolicy>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        SkillRegistry {
            skills: BTreeMap::new(),
            policies: BTreeMap::new(),
        }
    }

    pub fn register(&mut self, skill: Box<dyn Skill>) {
        let name = skill.manifest().name.clone();
        self.skills.insert(name, skill);
    }

    /// Remove skill do catálogo. Retorna true se existia.
    pub fn unregister(&mut self, name: &str) -> bool {
        let gone = self.skills.remove(name).is_some();
        self.policies.remove(name);
        gone
    }

    pub fn has_skill(&self, name: &str) -> bool {
        self.skills.contains_key(name)
    }

    pub fn set_policy(&mut self, name: &str, policy: ToolPolicy) {
        self.policies.insert(String::from(name), policy);
    }

    pub fn get_policy(&self, name: &str) -> Option<&ToolPolicy> {
        self.policies.get(name).or_else(|| self.policies.get("*"))
    }

    pub fn is_enabled(&self, name: &str) -> bool {
        self.get_policy(name).map_or(false, |p| p.enabled)
    }

    pub fn is_auto_approve(&self, name: &str) -> bool {
        self.get_policy(name).map_or(false, |p| p.auto_approve)
    }

    /// `required_tokens` vazio ⇒ sempre false (skill morta sob gate — honesty).
    pub fn validate_token(&self, name: &str, token: &CapabilityToken) -> bool {
        if let Some(skill) = self.skills.get(name) {
            let manifest = skill.manifest();
            if manifest.required_tokens.is_empty() {
                return false;
            }
            return manifest.required_tokens.contains(&token.as_legacy());
        }
        false
    }

    /// Idempotent flag do manifesto (p/ OutputCache). None se skill ausente.
    pub fn is_idempotent(&self, name: &str) -> Option<bool> {
        self.skills.get(name).map(|s| s.manifest().idempotent)
    }

    fn run_contracts(manifest: &crate::mcp::McpManifest, output: &[u8]) -> Result<(), &'static str> {
        for contract in &manifest.contracts {
            if let Err(reason) = contract.verify(output) {
                match contract.on_failure {
                    crate::contract::ContractAction::WarnOnly => {
                        // Sem slog na crate (zero-dep log); caller pode observar.
                        // Não engole como sucesso falso além de permitir output.
                    }
                    crate::contract::ContractAction::RejectOutput => return Err(reason),
                    // Nome histórico RetrySkill — não há loop de retry nesta API.
                    crate::contract::ContractAction::RetrySkill => {
                        return Err("contrato_falhou_retry_unavailable");
                    }
                }
            }
        }
        Ok(())
    }

    pub fn execute_skill_unchecked(&self, name: &str, payload: &[u8]) -> Result<Vec<u8>, &'static str> {
        let skill = self.skills.get(name).ok_or("skill nao encontrada")?;
        if !self.is_enabled(name) {
            return Err("skill desabilitada por politica");
        }
        skill.verify(payload)?;
        let output = skill.execute(payload)?;
        let manifest = skill.manifest();
        if !manifest.output_schema.validate(&output) {
            return Err("output_schema_mismatch");
        }
        Self::run_contracts(&manifest, &output)?;
        Ok(output)
    }

    pub fn execute_skill(
        &self,
        name: &str,
        payload: &[u8],
        token: &CapabilityToken,
    ) -> Result<Vec<u8>, &'static str> {
        let skill = self.skills.get(name).ok_or("skill nao encontrada")?;
        if !self.is_enabled(name) {
            return Err("skill desabilitada por politica");
        }
        if !self.is_auto_approve(name) {
            let manifest = skill.manifest();
            if manifest.required_tokens.is_empty() {
                return Err("skill_sem_required_tokens");
            }
            if !manifest.required_tokens.contains(&token.as_legacy()) {
                return Err("token de capacidade nao autorizado para esta skill");
            }
        }
        skill.verify(payload)?;
        let output = skill.execute(payload)?;
        let manifest = skill.manifest();
        if !manifest.output_schema.validate(&output) {
            return Err("output_schema_mismatch");
        }
        Self::run_contracts(&manifest, &output)?;
        Ok(output)
    }

    pub fn skill_count(&self) -> usize {
        self.skills.len()
    }

    pub fn list_skills(&self) -> Vec<SkillListEntry> {
        let mut result = Vec::new();
        for (name, skill) in &self.skills {
            let policy = self.get_policy(name).cloned().unwrap_or_default();
            let description = skill.manifest().description.clone();
            result.push(SkillListEntry {
                name: name.clone(),
                description,
                policy,
            });
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynskill::DynamicSkill;
    use crate::mcp::{McpManifest, OutputSchema};
    use crate::Skill;
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;

    struct Echo;
    impl Skill for Echo {
        fn manifest(&self) -> McpManifest {
            McpManifest {
                name: String::from("echo"),
                description: String::from("e"),
                required_tokens: vec![1],
                preconditions: Vec::new(),
                context_links: Vec::new(),
                output_schema: OutputSchema::String,
                idempotent: true,
                contracts: Vec::new(),
            }
        }
        fn execute(&self, payload: &[u8]) -> Result<Vec<u8>, &'static str> {
            Ok(payload.to_vec())
        }
    }

    struct EmptyTok;
    impl Skill for EmptyTok {
        fn manifest(&self) -> McpManifest {
            McpManifest {
                name: String::from("empty"),
                description: String::from("e"),
                required_tokens: Vec::new(),
                preconditions: Vec::new(),
                context_links: Vec::new(),
                output_schema: OutputSchema::Any,
                idempotent: false,
                contracts: Vec::new(),
            }
        }
        fn execute(&self, _payload: &[u8]) -> Result<Vec<u8>, &'static str> {
            Ok(b"x".to_vec())
        }
    }

    #[test]
    fn empty_tokens_deny() {
        let mut r = SkillRegistry::new();
        r.set_policy("*", ToolPolicy { enabled: true, auto_approve: false });
        r.register(Box::new(EmptyTok));
        let tok = CapabilityToken::Legacy(1);
        assert!(!r.validate_token("empty", &tok));
        assert_eq!(
            r.execute_skill("empty", b"", &tok),
            Err("skill_sem_required_tokens")
        );
    }

    #[test]
    fn execute_ok_with_token() {
        let mut r = SkillRegistry::new();
        r.set_policy("*", ToolPolicy { enabled: true, auto_approve: false });
        r.register(Box::new(Echo));
        let out = r
            .execute_skill("echo", b"hi", &CapabilityToken::Legacy(1))
            .unwrap();
        assert_eq!(out, b"hi");
        assert!(r.is_idempotent("echo") == Some(true));
    }

    #[test]
    fn disabled_by_default_without_policy() {
        let mut r = SkillRegistry::new();
        r.register(Box::new(Echo));
        assert!(!r.is_enabled("echo"));
        assert_eq!(
            r.execute_skill("echo", b"", &CapabilityToken::Legacy(1)),
            Err("skill desabilitada por politica")
        );
    }

    #[test]
    fn unregister_removes() {
        let mut r = SkillRegistry::new();
        r.set_policy("*", ToolPolicy { enabled: true, auto_approve: false });
        r.register(Box::new(DynamicSkill::new("d", "x", "y")));
        assert!(r.unregister("d"));
        assert!(!r.has_skill("d"));
    }

    #[test]
    fn list_skills_typed() {
        let mut r = SkillRegistry::new();
        r.set_policy("*", ToolPolicy { enabled: true, auto_approve: false });
        r.register(Box::new(Echo));
        let list = r.list_skills();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "echo");
        assert_eq!(list[0].description, "e");
    }
}
