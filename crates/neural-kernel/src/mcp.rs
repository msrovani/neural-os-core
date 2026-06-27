//! MCP Server — Model Context Protocol minimal sobre TCP.
//! Aceita conexões, interpreta JSON-RPC 2.0 simplificado, roteia para EventBus.
//!
//! Protocolo: mensagens terminadas em \n formato:
//!   method args\n
//!   echo "hello"
//!   status
//!   skill list
//!
//! Futuro: parser JSON-RPC 2.0 completo.

use alloc::string::String;
use alloc::vec::Vec;
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use crate::serial_println;
use crate::EVENT_BUS;
use crate::hermes;

const MCP_MANIFEST: AgentManifest = AgentManifest {
    name: "mcp",
    kind: AgentKind::Network,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

pub struct McpAgent {
    pending: Vec<String>,
}

impl McpAgent {
    pub fn new() -> Self {
        McpAgent { pending: Vec::new() }
    }

    fn handle_method(&mut self, method: &str) -> Option<String> {
        let parts: Vec<&str> = method.splitn(2, ' ').collect();
        let cmd = parts[0];
        let arg = parts.get(1).copied().unwrap_or("");

        match cmd {
            "echo" => Some(alloc::format!("OK: {}", arg)),
            "status" => {
                let diag = crate::net::run_network_diagnostics();
                Some(diag)
            }
            "skill" if arg == "list" => {
                let storage = crate::SKILL_STORAGE.lock();
                let skills = storage.list_skills();
                drop(storage);
                let mut msg = String::from("Skills:\n");
                for (i, (name, desc, _bytes)) in skills.iter().enumerate() {
                    msg.push_str(&alloc::format!("{}. {} - {}\n", i + 1, name, desc));
                }
                Some(msg)
            }
            "help" => Some("Comandos: echo <txt>, status, skill list, help".into()),
            _ => {
                // Publica como USER_INTENT para o Hermes processar
                let _ = EVENT_BUS.publish(crate::Event {
                    id: 0,
                    topic: String::from(hermes::TOPIC_USER_INTENT),
                    payload: method.as_bytes().to_vec(),
                    token: crate::CapabilityToken::Legacy(1),
                });
                Some(alloc::format!("MCP: '{}' roteado para Hermes", cmd))
            }
        }
    }
}

impl Agent for McpAgent {
    fn manifest(&self) -> &AgentManifest { &MCP_MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        // Verifica se há resposta pendente do Hermes
        if let Some(response) = self.pending.pop() {
            serial_println!("[MCP] Response: {}", response);
        }

        // TODO: TCP listener usando smoltcp socket (requer HTTP server)
        // Por enquanto, MCP funciona via comandos do Hermes

        AgentTickResult::Pending
    }
}
