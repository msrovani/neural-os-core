//! SkillIndex — catalogo de skills por dominio e capacidade.
//! Progressive disclosure: Hermes mostra apenas skills relevantes ao contexto.
//! Carregado a quente via VFS (/system/skills/index.md) ou runtime.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone)]
pub struct SkillRef {
    pub name: String,
    pub domain: String,
    pub description: String,
    pub capabilities: Vec<String>,
}

pub struct SkillIndex {
    by_domain: BTreeMap<String, Vec<SkillRef>>,
    by_capability: BTreeMap<String, Vec<SkillRef>>,
    all: Vec<SkillRef>,
}

impl SkillIndex {
    pub fn new() -> Self {
        SkillIndex {
            by_domain: BTreeMap::new(),
            by_capability: BTreeMap::new(),
            all: Vec::new(),
        }
    }

    pub fn register(&mut self, skill: SkillRef) {
        // Indexa por dominio
        self.by_domain
            .entry(skill.domain.clone())
            .or_default()
            .push(skill.clone());

        // Indexa por capacidade
        for cap in &skill.capabilities {
            self.by_capability
                .entry(cap.clone())
                .or_default()
                .push(skill.clone());
        }

        self.all.push(skill);
    }

    /// Skills de um dominio especifico (ex: "engineering", "network")
    pub fn by_domain(&self, domain: &str) -> &[SkillRef] {
        self.by_domain.get(domain).map_or(&[], |v| v.as_slice())
    }

    /// Skills com uma capacidade especifica (ex: "pci", "llm", "http")
    pub fn by_capability(&self, capability: &str) -> &[SkillRef] {
        self.by_capability.get(capability).map_or(&[], |v| v.as_slice())
    }

    /// Skills relevantes para um dado contexto (interseccao de capacidades)
    pub fn relevant(&self, capabilities: &[&str]) -> Vec<&SkillRef> {
        let mut result: Vec<&SkillRef> = Vec::new();
        'outer: for skill in &self.all {
            for cap in capabilities {
                if skill.capabilities.iter().any(|c| c == cap) {
                    result.push(skill);
                    continue 'outer;
                }
            }
        }
        result
    }

    pub fn all(&self) -> &[SkillRef] {
        &self.all
    }

    pub fn len(&self) -> usize {
        self.all.len()
    }
}
