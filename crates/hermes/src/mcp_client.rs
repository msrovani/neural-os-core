//! MCP Client — bridge entre CrossOsDiscoverer e MCP Server (ADR-0076 F3/F6).
//! O discoverer nunca consulta FYY/WeftOS diretamente — sempre via MCP.
//! AIOS na veia: protocolo > acoplamento direto.
//!
//! Hoje: chamada interna (mesmo processo).
//! Amanhã: JSON-RPC sobre EventBus ou rede.

use alloc::vec::Vec;

use crate::cross_os::discoverer::SkillCandidate;

/// Busca skills no marketplace via MCP.
/// Internamente chama o MCP server; externamente seria JSON-RPC.
pub fn search_marketplace(query: &str) -> Vec<SkillCandidate> {
    crate::mcp_server::search_skills(query)
}

/// Busca skills FYY específicas.
pub fn search_fyy(query: &str) -> Vec<SkillCandidate> {
    crate::mcp_server::search_fyy_skills(query)
}

/// Busca skills WeftOS específicas.
pub fn search_weftos(query: &str) -> Vec<SkillCandidate> {
    crate::mcp_server::search_weftos_skills(query)
}
