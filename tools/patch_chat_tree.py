#!/usr/bin/env python3
"""Patch chat_tree.rs: add serialize/deserialize + save/load + tests."""
import sys

path = "crates/hermes/src/chat_tree.rs"
with open(path, "r", encoding="utf-8") as f:
    content = f.read()

SAVE_LOAD = r'''
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
        buf.extend_from_slice(&(node.msg_type as u8).to_le_bytes());
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
            buf.push(tc.kind as u8);
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
                0 => crate::stream_packet::ToolKind::WebSearch,
                1 => crate::stream_packet::ToolKind::CodeExec,
                2 => crate::stream_packet::ToolKind::FileOp, _ => crate::stream_packet::ToolKind::Custom,
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

'''

TESTS = r'''
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
        let root = s.add_node(MsgType::User, "start", 501);
        let child1 = s.add_node(MsgType::Assistant, "reply1", 502);
        s.go_parent();
        let child2 = s.add_node(MsgType::Assistant, "reply2", 503);
        assert_eq!(s.nodes.iter().find(|n| n.id == root).unwrap().children.len(), 2);
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
'''

marker = 'pub static CHAT_SESSION'
idx = content.find(marker)
if idx < 0:
    print("ERROR: marker not found")
    sys.exit(1)
content = content[:idx] + SAVE_LOAD + '\n' + content[idx:]

# Add tests before the last closing brace
last_brace = content.rfind('}')
if last_brace > 0:
    # Find the test module closing brace
    test_mod_start = content.rfind('#[cfg(test)]')
    if test_mod_start > 0:
        # Add before the last } in the test module
        content = content[:last_brace] + TESTS + '\n' + content[last_brace:]

with open(path, "w", encoding="utf-8") as f:
    f.write(content)

print(f"Patched {path}: +{len(SAVE_LOAD.splitlines())} lines save/load + tests")
