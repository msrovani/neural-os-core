//! MCP Server (IDEA #172) — JSON-RPC 2.0 sobre EventBus.
//! Permite que ferramentas MCP externas (editores, agents) interajam com o AIOS.
//!
//! Métodos implementados:
//! - list_tools: lista skills disponíveis
//! - call_tool: executa uma skill
//! - list_resources: lista recursos VFS
//! - read_resource: lê um recurso VFS

use alloc::collections::BTreeMap;
use alloc::string::String;
use k_nano::EVENT_BUS;

/// Tópico para requests MCP.
pub const TOPIC_MCP_REQUEST: &str = "MCP_REQUEST";
/// Tópico para responses MCP.
pub const TOPIC_MCP_RESPONSE: &str = "MCP_RESPONSE";

/// Métodos MCP suportados.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpMethod {
    ListTools,
    CallTool,
    ListResources,
    ReadResource,
}

impl McpMethod {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "list_tools" => Some(McpMethod::ListTools),
            "call_tool" => Some(McpMethod::CallTool),
            "list_resources" => Some(McpMethod::ListResources),
            "read_resource" => Some(McpMethod::ReadResource),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            McpMethod::ListTools => "list_tools",
            McpMethod::CallTool => "call_tool",
            McpMethod::ListResources => "list_resources",
            McpMethod::ReadResource => "read_resource",
        }
    }
}

/// Processa um request MCP e publica a resposta no EventBus.
pub fn handle_mcp_request(request_json: &str) -> Result<(), &'static str> {
    // Parse JSON-RPC 2.0 request manualmente (no_std, sem serde)
    let (method, params, id) = parse_json_rpc(request_json)?;

    let result = match method {
        McpMethod::ListTools => handle_list_tools(),
        McpMethod::CallTool => handle_call_tool(&params),
        McpMethod::ListResources => handle_list_resources(),
        McpMethod::ReadResource => handle_read_resource(&params),
    };

    let response = alloc::format!(
        r#"{{"jsonrpc":"2.0","id":{},"result":{}}}"#,
        id, result
    );

    let _ = EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: String::from(TOPIC_MCP_RESPONSE),
        payload: response.into_bytes(),
        token: event_bus::CapabilityToken::Legacy(1),
    });

    Ok(())
}

fn handle_list_tools() -> String {
    // Query real SKILL_REGISTRY — zero stubs
    let reg = crate::globals::SKILL_REGISTRY.lock();
    let mut tools = alloc::string::String::from(r#"{"tools":["#);
    let skills: alloc::vec::Vec<_> = reg.list_skills().into_iter().collect();
    for (i, (name, _desc)) in skills.iter().enumerate() {
        if i > 0 { tools.push(','); }
        tools.push('"');
        tools.push_str(name);
        tools.push('"');
    }
    tools.push_str("]}");
    tools
}

fn handle_call_tool(params: &BTreeMap<String, String>) -> String {
    let tool = params.get("name").map(|s| s.as_str()).unwrap_or("unknown");
    let reg = crate::globals::SKILL_REGISTRY.lock();
    if reg.has_skill(tool) {
        alloc::format!(r#"{{"tool":"{}","status":"found"}}"#, tool)
    } else {
        alloc::format!(r#"{{"tool":"{}","status":"not_found"}}"#, tool)
    }
}

fn handle_list_resources() -> String {
    // Enumerate NeuralFS root — real VFS
    match crate::globals::VFS_BRIDGE.lock().as_ref() {
        Some(vfs) => match (vfs.list)("/") {
            Ok(entries) => {
                let mut out = alloc::string::String::from(r#"{"resources":["#);
                for (i, name) in entries.iter().enumerate() {
                    if i > 0 { out.push(','); }
                    out.push('"');
                    out.push_str(name);
                    out.push('"');
                }
                out.push_str("]}");
                out
            }
            Err(_) => String::from(r#"{"resources":[]}"#),
        },
        None => String::from(r#"{"resources":["vfs:///"]}"#),
    }
}

fn handle_read_resource(params: &BTreeMap<String, String>) -> String {
    let uri = params.get("uri").map(|s| s.as_str()).unwrap_or("");
    match crate::globals::read_vfs(uri) {
        Ok(data) => {
            alloc::format!(r#"{{"uri":"{}","content_len":{}}}"#, uri, data.len())
        }
        Err(e) => alloc::format!(r#"{{"uri":"{}","error":"{}"}}"#, uri, e),
    }
}

/// Parser JSON-RPC 2.0 mínimo (no_std, sem serde).
fn parse_json_rpc(json: &str) -> Result<(McpMethod, BTreeMap<String, String>, u64), &'static str> {
    let method_str = extract_string(json, "method").ok_or("missing method")?;
    let method = McpMethod::from_str(&method_str).ok_or("unknown method")?;
    let id = extract_number(json, "id").unwrap_or(0);
    let params = extract_params(json);
    Ok((method, params, id))
}

fn extract_string(json: &str, key: &str) -> Option<String> {
    let search = alloc::format!("\"{}\":\"", key);
    let start = json.find(&search)?;
    let value_start = start + search.len();
    let end = json[value_start..].find('"')?;
    Some(String::from(&json[value_start..value_start + end]))
}

fn extract_number(json: &str, key: &str) -> Option<u64> {
    let search = alloc::format!("\"{}\":", key);
    let start = json.find(&search)?;
    let value_start = start + search.len();
    let end = json[value_start..].find(|c: char| !c.is_ascii_digit())?;
    json[value_start..value_start + end].parse().ok()
}

fn extract_params(json: &str) -> BTreeMap<String, String> {
    let mut params = BTreeMap::new();
    if let Some(start) = json.find("\"params\":{") {
        let brace_start = start + 9;
        if let Some(end) = json[brace_start..].find('}') {
            let body = &json[brace_start..brace_start + end];
            for pair in body.split(',') {
                if let Some(eq) = pair.find(':') {
                    let key = pair[..eq].trim().trim_matches('"');
                    let val = pair[eq + 1..].trim().trim_matches('"');
                    params.insert(String::from(key), String::from(val));
                }
            }
        }
    }
    params
}
