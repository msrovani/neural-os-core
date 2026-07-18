//! Seed Agency — ADR-0052: stubs copiados NÃO são artefatos.
//! Fleet Agency só sobe via PackageHub com AGENT.md assinado + hash + acionaveis.
//! Não regenerar em massa com export_agent_packages.py.

pub struct AgentSeedRecord {
    pub name: &'static str,
    pub division: &'static str,
    pub mission: &'static str,
    pub skills: &'static [&'static str],
}

/// Vazio de propósito: SpecialistAgent stub sem missão executável = deny.
pub const AGENCY_SEEDS: &[AgentSeedRecord] = &[];
