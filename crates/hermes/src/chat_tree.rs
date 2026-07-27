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

pub static CHAT_SESSION: Mutex<Option<ChatSession>> = Mutex::new(None);

/// Inicializa ou reseta a sessão de chat
pub fn init_session(tick: u64) -> u32 {
    let mut session = CHAT_SESSION.lock();
    let id = session.as_ref().map(|s| s.id + 1).unwrap_or(1);
    *session = Some(ChatSession::new(id, tick));
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
