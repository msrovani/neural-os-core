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
        ToolPolicy {
            enabled: true,
            auto_approve: false,
        }
    }
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
        self.get_policy(name).map_or(true, |p| p.enabled)
    }

    pub fn is_auto_approve(&self, name: &str) -> bool {
        self.get_policy(name).map_or(false, |p| p.auto_approve)
    }

    pub fn validate_token(&self, name: &str, token: &CapabilityToken) -> bool {
        if let Some(skill) = self.skills.get(name) {
            let manifest = skill.manifest();
            return manifest.required_tokens.contains(&token.as_legacy());
        }
        false
    }

    pub fn execute_skill_unchecked(&self, name: &str, payload: &[u8]) -> Result<Vec<u8>, &'static str> {
        let skill = self.skills.get(name).ok_or("skill nao encontrada")?;
        if !self.is_enabled(name) {
            return Err("skill desabilitada por politica");
        }
        skill.execute(payload)
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
            if !manifest.required_tokens.contains(&token.as_legacy()) {
                return Err("token de capacidade nao autorizado para esta skill");
            }
        }
        skill.execute(payload)
    }

    pub fn list_skills(&self) -> Vec<(String, ToolPolicy)> {
        let mut result = Vec::new();
        for (name, skill) in &self.skills {
            let policy = self.get_policy(name).cloned().unwrap_or_default();
            let desc = skill.manifest().description.clone();
            result.push((alloc::format!("{}: {}", name, desc), policy));
        }
        result
    }
}
