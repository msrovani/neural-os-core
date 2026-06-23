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
