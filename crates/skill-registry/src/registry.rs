use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use event_bus::CapabilityToken;

use crate::skill::Skill;

pub struct SkillRegistry {
    skills: BTreeMap<String, Box<dyn Skill>>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        SkillRegistry {
            skills: BTreeMap::new(),
        }
    }

    pub fn register(&mut self, skill: Box<dyn Skill>) {
        let name = skill.manifest().name.clone();
        self.skills.insert(name, skill);
    }

    pub fn has_skill(&self, name: &str) -> bool {
        self.skills.contains_key(name)
    }

    pub fn validate_token(&self, name: &str, token: &CapabilityToken) -> bool {
        if let Some(skill) = self.skills.get(name) {
            let manifest = skill.manifest();
            return manifest.required_tokens.contains(&token.0);
        }
        false
    }

    pub fn execute_skill_unchecked(&self, name: &str, payload: &[u8]) -> Result<Vec<u8>, &'static str> {
        let skill = self.skills.get(name).ok_or("skill nao encontrada")?;
        skill.execute(payload)
    }

    pub fn execute_skill(
        &self,
        name: &str,
        payload: &[u8],
        token: &CapabilityToken,
    ) -> Result<Vec<u8>, &'static str> {
        let skill = self.skills.get(name).ok_or("skill nao encontrada")?;
        let manifest = skill.manifest();
        if !manifest.required_tokens.contains(&token.0) {
            return Err("token de capacidade nao autorizado para esta skill");
        }
        skill.execute(payload)
    }
}
