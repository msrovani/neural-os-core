//! MCP Server — JSON-RPC 2.0 mínimo (tools/list, tools/call → USER_INTENT).
//! Sem stdio servers externos; linha \n ou objeto JSON compacto.

use alloc::string::String;
use alloc::vec::Vec;
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use k_nano::EVENT_BUS;

const MCP_MANIFEST: AgentManifest = AgentManifest {
    name: "mcp",
    kind: AgentKind::Network,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

pub struct McpAgent {
    pending: Vec<String>,
    inbox: Vec<String>,
}

impl McpAgent {
    pub fn new() -> Self {
        McpAgent {
            pending: Vec::new(),
            inbox: Vec::new(),
        }
    }

    /// Enfileira mensagem bruta (CLI / TCP futuro).
    pub fn enqueue(&mut self, line: &str) {
        self.inbox.push(String::from(line));
    }

    fn tools_list_json() -> String {
        String::from(
            "{\"jsonrpc\":\"2.0\",\"result\":{\"tools\":[\
{\"name\":\"skills_list\",\"description\":\"L0 skill index\"},\
{\"name\":\"skill_view\",\"description\":\"L1 full SKILL.md\"},\
{\"name\":\"market_search\",\"description\":\"Search local marketplace\"},\
{\"name\":\"remember\",\"description\":\"Append MEMORY.md fact\"},\
{\"name\":\"user_intent\",\"description\":\"Route text to Hermes\"}\
]},\"id\":1}",
        )
    }

    fn json_result(id: &str, result: &str) -> String {
        alloc::format!(
            "{{\"jsonrpc\":\"2.0\",\"result\":{},\"id\":{}}}",
            result, id
        )
    }

    fn json_error(id: &str, code: i32, msg: &str) -> String {
        alloc::format!(
            "{{\"jsonrpc\":\"2.0\",\"error\":{{\"code\":{},\"message\":\"{}\"}},\"id\":{}}}",
            code, msg, id
        )
    }

    fn extract_json_str(hay: &str, key: &str) -> Option<String> {
        let needle = alloc::format!("\"{}\"", key);
        let idx = hay.find(&needle)?;
        let after = &hay[idx + needle.len()..];
        let colon = after.find(':')?;
        let mut rest = after[colon + 1..].trim_start();
        if !rest.starts_with('"') {
            return None;
        }
        rest = &rest[1..];
        let end = rest.find('"')?;
        Some(String::from(&rest[..end]))
    }

    fn handle_json_rpc(&mut self, line: &str) -> String {
        let id = extract_id(line).unwrap_or_else(|| String::from("1"));
        let method = Self::extract_json_str(line, "method").unwrap_or_default();
        match method.as_str() {
            "tools/list" | "tools.list" => Self::tools_list_json().replace("\"id\":1", &alloc::format!("\"id\":{}", id)),
            "tools/call" | "tools.call" => {
                let tool = Self::extract_json_str(line, "name")
                    .or_else(|| Self::extract_json_str(line, "tool"))
                    .unwrap_or_default();
                let arg = Self::extract_json_str(line, "arguments")
                    .or_else(|| Self::extract_json_str(line, "text"))
                    .unwrap_or_default();
                let out = self.dispatch_tool(&tool, &arg);
                Self::json_result(&id, &alloc::format!("\"{}\"", escape_json(&out)))
            }
            "" => self.handle_legacy(line).unwrap_or_else(|| Self::json_error(&id, -32600, "invalid")),
            other => Self::json_error(&id, -32601, other),
        }
    }

    fn dispatch_tool(&mut self, tool: &str, arg: &str) -> String {
        match tool {
            "skills_list" => crate::memory_store::skills_l0(),
            "skill_view" => crate::memory_store::skill_view(arg),
            "market_search" => crate::marketplace::search(arg),
            "remember" => crate::memory_store::remember(arg).unwrap_or_else(|e| String::from(e)),
            "user_intent" | "" => {
                let text = if arg.is_empty() { tool } else { arg };
                let _ = EVENT_BUS.publish(event_bus::Event {
                    id: 0,
                    topic: String::from(crate::hermes::TOPIC_USER_INTENT),
                    payload: text.as_bytes().to_vec(),
                    token: event_bus::CapabilityToken::Legacy(1),
                });
                alloc::format!("MCP: intent queued '{}'", text)
            }
            other => {
                let _ = EVENT_BUS.publish(event_bus::Event {
                    id: 0,
                    topic: String::from(crate::hermes::TOPIC_USER_INTENT),
                    payload: other.as_bytes().to_vec(),
                    token: event_bus::CapabilityToken::Legacy(1),
                });
                alloc::format!("MCP: tool '{}' → USER_INTENT", other)
            }
        }
    }

    fn handle_legacy(&mut self, method: &str) -> Option<String> {
        let parts: Vec<&str> = method.splitn(2, ' ').collect();
        let cmd = parts[0];
        let arg = parts.get(1).copied().unwrap_or("");
        match cmd {
            "echo" => Some(alloc::format!("OK: {}", arg)),
            "status" => Some(crate::net::run_network_diagnostics()),
            "skill" if arg == "list" || arg.is_empty() => Some(crate::memory_store::skills_l0()),
            "tools/list" => Some(Self::tools_list_json()),
            "help" => Some(
                "MCP: tools/list | tools/call | echo | status | skill list | JSON-RPC line"
                    .into(),
            ),
            _ => {
                let _ = EVENT_BUS.publish(event_bus::Event {
                    id: 0,
                    topic: String::from(crate::hermes::TOPIC_USER_INTENT),
                    payload: method.as_bytes().to_vec(),
                    token: event_bus::CapabilityToken::Legacy(1),
                });
                Some(alloc::format!("MCP: '{}' roteado para Hermes", cmd))
            }
        }
    }

    pub fn handle_line(&mut self, line: &str) -> String {
        let t = line.trim();
        if t.starts_with('{') {
            self.handle_json_rpc(t)
        } else {
            self.handle_legacy(t).unwrap_or_else(|| String::from("MCP: empty"))
        }
    }
}

fn extract_id(line: &str) -> Option<String> {
    if let Some(v) = McpAgent::extract_json_str(line, "id") {
        return Some(alloc::format!("\"{}\"", v));
    }
    // numeric id
    let needle = "\"id\":";
    let idx = line.find(needle)?;
    let rest = line[idx + needle.len()..].trim_start();
    let mut num = String::new();
    for c in rest.chars() {
        if c.is_ascii_digit() {
            num.push(c);
        } else {
            break;
        }
    }
    if num.is_empty() {
        None
    } else {
        Some(num)
    }
}

fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

impl Agent for McpAgent {
    fn manifest(&self) -> &AgentManifest {
        &MCP_MANIFEST
    }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        while let Some(line) = self.inbox.pop() {
            let resp = self.handle_line(&line);
            k_nano::slog_hermes!("MCP", "info", "{}", resp);
            self.pending.push(resp);
        }
        if let Some(response) = self.pending.pop() {
            k_nano::slog_hermes!("MCP", "info", "Response: {}", response);
        }
        AgentTickResult::Pending
    }
}

/// Helper para Hermes CLI: `/mcp <line>`.
pub fn handle_mcp_line(line: &str) -> String {
    let mut a = McpAgent::new();
    a.handle_line(line)
}
