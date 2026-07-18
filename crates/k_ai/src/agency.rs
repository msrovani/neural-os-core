//! Catálogo data-driven da Agency, materializado pelo seed gerado de AGENT.md.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::agency_seed::AGENCY_SEEDS;

#[derive(Debug, Clone)]
pub struct AgentSpec {
    pub name: String,
    pub division: String,
    pub mission: String,
    pub skills: Vec<String>,
    pub deliverable: String,
}

#[derive(Debug)]
pub struct Division {
    pub name: String,
    pub agents: Vec<AgentSpec>,
}

pub struct Agency {
    pub divisions: Vec<Division>,
}

impl Agency {
    /// Fallback compilado, gerado deterministicamente dos AGENT.md.
    pub fn new() -> Self {
        let specs = AGENCY_SEEDS
            .iter()
            .map(|seed| AgentSpec {
                name: String::from(seed.name),
                division: String::from(seed.division),
                mission: String::from(seed.mission),
                skills: seed
                    .skills
                    .iter()
                    .map(|skill| String::from(*skill))
                    .collect(),
                deliverable: String::from("auto"),
            })
            .collect();
        Self::from_specs(specs)
    }

    /// Constrói as divisões a partir do catálogo PackageHub/NeuralFS.
    pub fn from_specs(specs: Vec<AgentSpec>) -> Self {
        let mut grouped: BTreeMap<String, Vec<AgentSpec>> = BTreeMap::new();
        for spec in specs {
            grouped
                .entry(spec.division.clone())
                .or_default()
                .push(spec);
        }
        let divisions = grouped
            .into_iter()
            .map(|(name, agents)| Division { name, agents })
            .collect();
        Agency { divisions }
    }

    pub fn find(&self, name: &str) -> Option<&AgentSpec> {
        self.divisions
            .iter()
            .flat_map(|division| division.agents.iter())
            .find(|agent| agent.name == name)
    }

    pub fn for_task(&self, task: &str) -> Vec<&AgentSpec> {
        let lower = task.to_ascii_lowercase();
        let mut result = Vec::new();
        for agent in self.divisions.iter().flat_map(|division| &division.agents) {
            if agent
                .skills
                .iter()
                .any(|skill| lower.contains(skill.as_str()))
                || agent.mission.to_ascii_lowercase().contains(&lower)
            {
                result.push(agent);
            }
        }
        result.truncate(5);
        result
    }

    pub fn count(&self) -> usize {
        self.divisions
            .iter()
            .map(|division| division.agents.len())
            .sum()
    }

    pub fn delegate(&self, task: &str, n: usize) -> Vec<&AgentSpec> {
        let lower = task.to_ascii_lowercase();
        self.divisions
            .iter()
            .flat_map(|division| division.agents.iter())
            .filter(|agent| {
                agent
                    .skills
                    .iter()
                    .any(|skill| lower.contains(skill.as_str()))
            })
            .take(n)
            .collect()
    }

    pub fn llm_context(&self) -> String {
        let mut context = format!(
            "The Agency — {} agentes, {} divisoes:\n",
            self.count(),
            self.divisions.len()
        );
        for division in &self.divisions {
            context.push_str(&format!(
                "\n[{}] ({} agents)\n",
                division.name,
                division.agents.len()
            ));
            for agent in &division.agents {
                context.push_str(&format!("  {}: {}\n", agent.name, agent.mission));
            }
        }
        context
    }
}

impl Default for Agency {
    fn default() -> Self {
        Self::new()
    }
}
