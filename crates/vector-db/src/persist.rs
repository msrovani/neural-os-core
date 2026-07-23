//! Serialização binária length-prefixed do VectorStore (ADR-0064 F2 / ponte 0063).
//! Formato: magic "NVDB" + u32 version + maps + entries (sem embeddings — rebuild on load).

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::{EntryKind, EntryMetadata, VectorStore};

const MAGIC: &[u8; 4] = b"NVDB";
const VERSION: u32 = 1;

fn push_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn push_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn push_str(out: &mut Vec<u8>, s: &str) {
    push_u32(out, s.len() as u32);
    out.extend_from_slice(s.as_bytes());
}

fn read_u32(data: &[u8], off: &mut usize) -> Option<u32> {
    if *off + 4 > data.len() {
        return None;
    }
    let v = u32::from_le_bytes(data[*off..*off + 4].try_into().ok()?);
    *off += 4;
    Some(v)
}
fn read_u64(data: &[u8], off: &mut usize) -> Option<u64> {
    if *off + 8 > data.len() {
        return None;
    }
    let v = u64::from_le_bytes(data[*off..*off + 8].try_into().ok()?);
    *off += 8;
    Some(v)
}
fn read_str(data: &[u8], off: &mut usize) -> Option<String> {
    let len = read_u32(data, off)? as usize;
    if *off + len > data.len() {
        return None;
    }
    let s = core::str::from_utf8(&data[*off..*off + len])
        .ok()?
        .to_string();
    *off += len;
    Some(s)
}

pub fn to_bytes(store: &VectorStore) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    push_u32(&mut out, VERSION);
    push_u32(&mut out, store.doc_count());
    push_u64(&mut out, store.next_id());
    push_u32(&mut out, store.vocabulary().len() as u32);
    for (w, idx) in store.vocabulary() {
        push_str(&mut out, w);
        push_u32(&mut out, *idx as u32);
    }
    push_u32(&mut out, store.df().len() as u32);
    for d in store.df() {
        push_u32(&mut out, *d);
    }
    push_u32(&mut out, store.len() as u32);
    for e in store.all_entries() {
        push_str(&mut out, &e.id);
        push_str(&mut out, &e.text);
        push_str(&mut out, &e.metadata.agent);
        out.push(e.metadata.kind as u8);
        push_u64(&mut out, e.metadata.timestamp);
        push_u32(&mut out, e.metadata.tags.len() as u32);
        for t in &e.metadata.tags {
            push_str(&mut out, t);
        }
        match &e.metadata.source {
            Some(s) => {
                out.push(1);
                push_str(&mut out, s);
            }
            None => out.push(0),
        }
    }
    out
}

pub fn from_bytes(data: &[u8]) -> Result<VectorStore, &'static str> {
    if data.len() < 8 || &data[0..4] != MAGIC {
        return Err("bad magic");
    }
    let mut off = 4;
    let ver = read_u32(data, &mut off).ok_or("trunc ver")?;
    if ver != VERSION {
        return Err("bad version");
    }
    let doc_count = read_u32(data, &mut off).ok_or("trunc doc_count")?;
    let next_id = read_u64(data, &mut off).ok_or("trunc next_id")?;
    let n_vocab = read_u32(data, &mut off).ok_or("trunc n_vocab")? as usize;
    let mut vocabulary = BTreeMap::new();
    for _ in 0..n_vocab {
        let w = read_str(data, &mut off).ok_or("trunc word")?;
        let idx = read_u32(data, &mut off).ok_or("trunc idx")? as usize;
        vocabulary.insert(w, idx);
    }
    let n_df = read_u32(data, &mut off).ok_or("trunc n_df")? as usize;
    let mut df = Vec::with_capacity(n_df);
    for _ in 0..n_df {
        df.push(read_u32(data, &mut off).ok_or("trunc df")?);
    }
    let n_ent = read_u32(data, &mut off).ok_or("trunc n_ent")? as usize;
    let mut entries = Vec::with_capacity(n_ent);
    for _ in 0..n_ent {
        let id = read_str(data, &mut off).ok_or("trunc id")?;
        let text = read_str(data, &mut off).ok_or("trunc text")?;
        let agent = read_str(data, &mut off).ok_or("trunc agent")?;
        if off >= data.len() {
            return Err("trunc kind");
        }
        let kind = EntryKind::from_u8(data[off]);
        off += 1;
        let timestamp = read_u64(data, &mut off).ok_or("trunc ts")?;
        let n_tags = read_u32(data, &mut off).ok_or("trunc n_tags")? as usize;
        let mut tags = Vec::with_capacity(n_tags);
        for _ in 0..n_tags {
            tags.push(read_str(data, &mut off).ok_or("trunc tag")?);
        }
        if off >= data.len() {
            return Err("trunc source flag");
        }
        let has_src = data[off];
        off += 1;
        let source = if has_src == 1 {
            Some(read_str(data, &mut off).ok_or("trunc source")?)
        } else {
            None
        };
        entries.push((
            id,
            text,
            EntryMetadata {
                agent,
                kind,
                timestamp,
                tags,
                source,
            },
        ));
    }
    Ok(VectorStore::from_parts(
        vocabulary, df, doc_count, next_id, entries,
    ))
}
