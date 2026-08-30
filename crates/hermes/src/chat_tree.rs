//! ChatSession — árvore de conversa com branching.
//! Cada nodo é uma mensagem (usuário, assistente, ferramenta).
//! O "active_leaf" é a ponta ativa da conversa.
//!
//! Inspirado no ChatMessageDetail + parent_message do Onyx.

use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone, PartialEq)]
pub enum MsgType {
    User,
    Assistant,
    ToolCall,
    ToolResult,
    System,
}

#[derive(Debug, Clone)]
pub struct ChatNode {
    pub id: u32,
    pub parent: Option<u32>,
    pub msg_type: MsgType,
    /// O conteúdo textual da mensagem
    pub content: String,
    /// Reasoning (chain-of-thought) que precedeu esta msg
    pub reasoning: Option<String>,
    /// Citações: (doc_id, texto, url?)
    pub citations: Vec<(u32, String, Option<String>)>,
    /// IDs dos filhos (branching)
    pub children: Vec<u32>,
    /// Tool calls que produziram esta mensagem
    pub tool_calls: Vec<ToolCallRecord>,
    /// Timestamp (tick do timer)
    pub tick: u64,
}

#[derive(Debug, Clone)]
pub struct ToolCallRecord {
    pub id: u32,
    pub kind: crate::stream_packet::ToolKind,
    pub label: String,
    pub input: String,
    pub output: String,
}

#[derive(Debug, Clone)]
pub struct ChatSession {
    /// ID da sessão
    pub id: u32,
    /// Todos os nodos da conversa
    pub nodes: Vec<ChatNode>,
    /// ID do nodo ativo (folha atual)
    pub active_leaf: u32,
    /// Próximo ID disponível
    next_id: u32,
    /// Timestamp de criação (tick)
    pub created_tick: u64,
}

impl ChatSession {
    pub fn new(id: u32, tick: u64) -> Self {
        Self {
            id,
            nodes: Vec::new(),
            active_leaf: 0,
            next_id: 1,
            created_tick: tick,
        }
    }

    /// Adiciona um nodo como filho do active_leaf.
    /// Retorna o id do novo nodo.
    pub fn add_node(&mut self, msg_type: MsgType, content: &str, tick: u64) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let parent = if self.active_leaf != 0 { Some(self.active_leaf) } else { None };

        // Registra como filho do parent
        if let Some(pid) = parent {
            if let Some(pnode) = self.nodes.iter_mut().find(|n| n.id == pid) {
                pnode.children.push(id);
            }
        }

        self.nodes.push(ChatNode {
            id,
            parent,
            msg_type,
            content: String::from(content),
            reasoning: None,
            citations: Vec::new(),
            children: Vec::new(),
            tool_calls: Vec::new(),
            tick,
        });

        self.active_leaf = id;
        id
    }

    /// Adiciona reasoning ao nodo ativo
    pub fn set_reasoning(&mut self, reasoning: &str) {
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == self.active_leaf) {
            node.reasoning = Some(String::from(reasoning));
        }
    }

    /// Adiciona citação ao nodo ativo
    pub fn add_citation(&mut self, doc_id: u32, text: &str, url: Option<&str>) {
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == self.active_leaf) {
            node.citations.push((doc_id, String::from(text), url.map(String::from)));
        }
    }

    /// Adiciona tool call ao nodo ativo
    pub fn add_tool_call(
        &mut self,
        id: u32,
        kind: crate::stream_packet::ToolKind,
        label: &str,
        input: &str,
        output: &str,
    ) {
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == self.active_leaf) {
            // Procura se já existe tool com este id
            if let Some(existing) = node.tool_calls.iter_mut().find(|t| t.id == id) {
                existing.output = String::from(output);
                if !input.is_empty() {
                    existing.input = String::from(input);
                }
            } else {
                node.tool_calls.push(ToolCallRecord {
                    id,
                    kind,
                    label: String::from(label),
                    input: String::from(input),
                    output: String::from(output),
                });
            }
        }
    }

    /// Append ao conteúdo do nodo ativo (streaming)
    pub fn append_content(&mut self, delta: &str) {
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == self.active_leaf) {
            node.content.push_str(delta);
        }
    }

    /// Obtém o chain ativo (raiz → active_leaf)
    pub fn active_chain(&self) -> Vec<&ChatNode> {
        let mut chain = Vec::new();
        let mut current = self.active_leaf;
        loop {
            if let Some(node) = self.nodes.iter().find(|n| n.id == current) {
                chain.push(node);
                match node.parent {
                    Some(pid) => current = pid,
                    None => break,
                }
            } else {
                break;
            }
        }
        chain.reverse();
        chain
    }

    /// Obtém os nodos para exibição (chain ativo limitado)
    pub fn display_nodes(&self, max_count: usize) -> Vec<&ChatNode> {
        let chain = self.active_chain();
        chain.into_iter().rev().take(max_count).rev().collect()
    }

    /// Navega para o nodo pai (volta)
    pub fn go_parent(&mut self) -> bool {
        if let Some(node) = self.nodes.iter().find(|n| n.id == self.active_leaf) {
            if let Some(pid) = node.parent {
                self.active_leaf = pid;
                return true;
            }
        }
        false
    }

    /// Navega para um filho específico
    pub fn go_child(&mut self, child_id: u32) -> bool {
        if self.nodes.iter().any(|n| n.id == child_id) {
            self.active_leaf = child_id;
            true
        } else {
            false
        }
    }
}

// ── Helpers para compat com agente existente ──

/// Sessão global (singleton simplificado para hermes)
use spin::Mutex;


// ── Persistência NeuralFS ───────────────────────────────────────────────

const CHAT_SESSION_PATH: &str = "/mnt/neural/CHAT_SESSION.bin";
const CHAT_SESSION_MAGIC: &[u8; 4] = b"CHAT";
const CHAT_SESSION_VERSION: u32 = 1;

/// Serializa ChatSession em binário length-prefixed.
fn serialize_session(s: &ChatSession) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(CHAT_SESSION_MAGIC);
    buf.extend_from_slice(&CHAT_SESSION_VERSION.to_le_bytes());
    buf.extend_from_slice(&s.id.to_le_bytes());
    buf.extend_from_slice(&s.active_leaf.to_le_bytes());
    buf.extend_from_slice(&s.next_id.to_le_bytes());
    buf.extend_from_slice(&s.created_tick.to_le_bytes());
    buf.extend_from_slice(&(s.nodes.len() as u32).to_le_bytes());
    for node in &s.nodes {
        buf.extend_from_slice(&node.id.to_le_bytes());
        buf.extend_from_slice(&node.parent.map(|p| p + 1).unwrap_or(0).to_le_bytes());
        let mt_byte = match node.msg_type {
            MsgType::User => 0u8,
            MsgType::Assistant => 1,
            MsgType::ToolCall => 2,
            MsgType::ToolResult => 3,
            MsgType::System => 4,
        };
        buf.extend_from_slice(&mt_byte.to_le_bytes());
        let cb = node.content.as_bytes();
        buf.extend_from_slice(&(cb.len() as u32).to_le_bytes());
        buf.extend_from_slice(cb);
        match &node.reasoning {
            Some(r) => {
                buf.push(1);
                let rb = r.as_bytes();
                buf.extend_from_slice(&(rb.len() as u32).to_le_bytes());
                buf.extend_from_slice(rb);
            }
            None => buf.push(0),
        }
        buf.extend_from_slice(&(node.citations.len() as u32).to_le_bytes());
        for (doc_id, text, url) in &node.citations {
            buf.extend_from_slice(&doc_id.to_le_bytes());
            let tb = text.as_bytes();
            buf.extend_from_slice(&(tb.len() as u32).to_le_bytes());
            buf.extend_from_slice(tb);
            match url {
                Some(u) => {
                    buf.push(1);
                    let ub = u.as_bytes();
                    buf.extend_from_slice(&(ub.len() as u32).to_le_bytes());
                    buf.extend_from_slice(ub);
                }
                None => buf.push(0),
            }
        }
        buf.extend_from_slice(&(node.children.len() as u32).to_le_bytes());
        for child_id in &node.children {
            buf.extend_from_slice(&child_id.to_le_bytes());
        }
        buf.extend_from_slice(&(node.tool_calls.len() as u32).to_le_bytes());
        for tc in &node.tool_calls {
            buf.extend_from_slice(&tc.id.to_le_bytes());
                let kind_byte = match &tc.kind {
                    crate::stream_packet::ToolKind::Search => 0u8,
                    crate::stream_packet::ToolKind::UrlFetch => 1,
                    crate::stream_packet::ToolKind::CodeExec => 2,
                    crate::stream_packet::ToolKind::Reasoning => 3,
                    crate::stream_packet::ToolKind::Custom(_) => 4,
                    crate::stream_packet::ToolKind::Memory => 5,
                    crate::stream_packet::ToolKind::FileReader => 6,
                    crate::stream_packet::ToolKind::DeepResearch => 7,
                    crate::stream_packet::ToolKind::Bash => 8,
                };
                buf.push(kind_byte);
                if let crate::stream_packet::ToolKind::Custom(ref s) = tc.kind {
                    let sb = s.as_bytes();
                    buf.extend_from_slice(&(sb.len() as u32).to_le_bytes());
                    buf.extend_from_slice(sb);
                }
            let lb = tc.label.as_bytes();
            buf.extend_from_slice(&(lb.len() as u32).to_le_bytes());
            buf.extend_from_slice(lb);
            let ib = tc.input.as_bytes();
            buf.extend_from_slice(&(ib.len() as u32).to_le_bytes());
            buf.extend_from_slice(ib);
            let ob = tc.output.as_bytes();
            buf.extend_from_slice(&(ob.len() as u32).to_le_bytes());
            buf.extend_from_slice(ob);
        }
        buf.extend_from_slice(&node.tick.to_le_bytes());
    }
    buf
}

struct BufCursor<'a> { data: &'a [u8], pos: usize }
impl<'a> BufCursor<'a> {
    fn new(d: &'a [u8]) -> Self { Self { data: d, pos: 0 } }
    fn u8(&mut self) -> Option<u8> {
        let v = *self.data.get(self.pos)?; self.pos += 1; Some(v)
    }
    fn u32(&mut self) -> Option<u32> {
        let end = self.pos + 4; if end > self.data.len() { return None; }
        let v = u32::from_le_bytes(self.data[self.pos..end].try_into().ok()?);
        self.pos = end; Some(v)
    }
    fn u64(&mut self) -> Option<u64> {
        let end = self.pos + 8; if end > self.data.len() { return None; }
        let v = u64::from_le_bytes(self.data[self.pos..end].try_into().ok()?);
        self.pos = end; Some(v)
    }
    fn bytes(&mut self) -> Option<&'a [u8]> {
        let len = self.u32()? as usize;
        let end = self.pos + len; if end > self.data.len() { return None; }
        let s = &self.data[self.pos..end]; self.pos = end; Some(s)
    }
}

fn deserialize_session(data: &[u8]) -> Option<ChatSession> {
    let mut r = BufCursor::new(data);
    let magic = [r.u8()?, r.u8()?, r.u8()?, r.u8()?];
    if magic != *CHAT_SESSION_MAGIC { return None; }
    if r.u32()? != CHAT_SESSION_VERSION { return None; }
    let id = r.u32()?;
    let active_leaf = r.u32()?;
    let next_id = r.u32()?;
    let created_tick = r.u64()?;
    let node_count = r.u32()? as usize;
    let mut nodes = Vec::with_capacity(node_count);
    for _ in 0..node_count {
        let nid = r.u32()?;
        let parent_raw = r.u32()?;
        let parent = if parent_raw == 0 { None } else { Some(parent_raw - 1) };
        let msg_type = match r.u8()? {
            0 => MsgType::User, 1 => MsgType::Assistant,
            2 => MsgType::ToolCall, 3 => MsgType::ToolResult, _ => MsgType::System,
        };
        let content = String::from_utf8_lossy(r.bytes()?).into_owned();
        let reasoning = if r.u8()? == 1 { Some(String::from_utf8_lossy(r.bytes()?).into_owned()) } else { None };
        let cit_count = r.u32()? as usize;
        let mut citations = Vec::with_capacity(cit_count);
        for _ in 0..cit_count {
            let doc_id = r.u32()?;
            let text = String::from_utf8_lossy(r.bytes()?).into_owned();
            let url = if r.u8()? == 1 { Some(String::from_utf8_lossy(r.bytes()?).into_owned()) } else { None };
            citations.push((doc_id, text, url));
        }
        let child_count = r.u32()? as usize;
        let mut children = Vec::with_capacity(child_count);
        for _ in 0..child_count { children.push(r.u32()?); }
        let tc_count = r.u32()? as usize;
        let mut tool_calls = Vec::with_capacity(tc_count);
        for _ in 0..tc_count {
            let tc_id = r.u32()?;
            let kind = match r.u8()? {
                0 => crate::stream_packet::ToolKind::Search,
                1 => crate::stream_packet::ToolKind::UrlFetch,
                2 => crate::stream_packet::ToolKind::CodeExec,
                3 => crate::stream_packet::ToolKind::Reasoning,
                4 => {
                    let cs = String::from_utf8_lossy(r.bytes()?).into_owned();
                    crate::stream_packet::ToolKind::Custom(cs)
                }
                5 => crate::stream_packet::ToolKind::Memory,
                6 => crate::stream_packet::ToolKind::FileReader,
                7 => crate::stream_packet::ToolKind::DeepResearch,
                _ => crate::stream_packet::ToolKind::Bash,
            };
            let label = String::from_utf8_lossy(r.bytes()?).into_owned();
            let input = String::from_utf8_lossy(r.bytes()?).into_owned();
            let output = String::from_utf8_lossy(r.bytes()?).into_owned();
            tool_calls.push(ToolCallRecord { id: tc_id, kind, label, input, output });
        }
        let tick = r.u64()?;
        nodes.push(ChatNode { id: nid, parent, msg_type, content, reasoning, citations, children, tool_calls, tick });
    }
    Some(ChatSession { id, nodes, active_leaf, next_id, created_tick })
}

/// Salva a sessão ativa em NeuralFS (best-effort).
pub fn chat_session_save() {
    let session = CHAT_SESSION.lock();
    if let Some(ref s) = *session {
        let data = serialize_session(s);
        let _ = crate::globals::write_vfs(CHAT_SESSION_PATH, &data);
        k_nano::slog_hermes!("chat", "save", "session {} saved ({} bytes, {} nodes)", s.id, data.len(), s.nodes.len());
    }
}

/// Carrega sessão do NeuralFS no boot (se disponível).
pub fn chat_session_load() {
    let data = match crate::globals::read_vfs(CHAT_SESSION_PATH) {
        Ok(d) if d.len() > 8 => d,
        _ => return,
    };
    if let Some(s) = deserialize_session(&data) {
        k_nano::slog_hermes!("chat", "load", "session {} loaded ({} nodes)", s.id, s.nodes.len());
        let mut session = CHAT_SESSION.lock();
        *session = Some(s);
    }
}


pub static CHAT_SESSION: Mutex<Option<ChatSession>> = Mutex::new(None);

/// Inicializa ou reseta a sessão de chat
pub fn init_session(tick: u64) -> u32 {
    let mut session = CHAT_SESSION.lock();
    let id = session.as_ref().map(|s| s.id + 1).unwrap_or(1);
    *session = Some(ChatSession::new(id, tick));
    drop(session);
    chat_session_save();
    id
}

/// Adiciona mensagem de usuário à sessão atual
pub fn push_user_msg(content: &str, tick: u64) -> u32 {
    let mut session = CHAT_SESSION.lock();
    if let Some(ref mut s) = *session {
        s.add_node(MsgType::User, content, tick)
    } else {
        let mut s = ChatSession::new(1, tick);
        let id = s.add_node(MsgType::User, content, tick);
        *session = Some(s);
        id
    }
}

/// Adiciona nodo assistant à sessão
pub fn push_assistant_msg(content: &str, tick: u64) -> u32 {
    let mut session = CHAT_SESSION.lock();
    if let Some(ref mut s) = *session {
        s.add_node(MsgType::Assistant, content, tick)
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_roundtrip_simple() {
        let mut s = ChatSession::new(1, 100);
        s.add_node(MsgType::User, "hello", 101);
        s.add_node(MsgType::Assistant, "world", 102);
        let data = serialize_session(&s);
        let loaded = deserialize_session(&data).expect("deserialize should succeed");
        assert_eq!(loaded.id, 1);
        assert_eq!(loaded.nodes.len(), 2);
        assert_eq!(loaded.active_leaf, 2);
        assert_eq!(loaded.nodes[0].content, "hello");
        assert_eq!(loaded.nodes[1].content, "world");
        assert_eq!(loaded.nodes[1].parent, Some(1));
    }

    #[test]
    fn serialize_roundtrip_branching() {
        let mut s = ChatSession::new(7, 500);
        let _root = s.add_node(MsgType::User, "start", 501);
        let _child1 = s.add_node(MsgType::Assistant, "reply1", 502);
        s.go_parent();
        let child2 = s.add_node(MsgType::Assistant, "reply2", 503);
        let data = serialize_session(&s);
        let loaded = deserialize_session(&data).unwrap();
        assert_eq!(loaded.nodes.len(), 3);
        assert_eq!(loaded.active_leaf, child2);
    }

    #[test]
    fn deserialize_bad_magic_returns_none() {
        let data = b"XXXX";
        assert!(deserialize_session(data).is_none());
    }

    #[test]
    fn deserialize_truncated_returns_none() {
        let mut s = ChatSession::new(1, 0);
        s.add_node(MsgType::User, "hi", 1);
        let mut data = serialize_session(&s);
        data.truncate(10);
        assert!(deserialize_session(&data).is_none());
    }
}
