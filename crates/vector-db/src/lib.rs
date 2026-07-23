//! ADR-0064 — VectorStore TF-IDF + cosine (RAG L1 lexical).
//! Persistência: schema binário → TicKV `vdb/*` (ADR-0063), sem serde_json no kernel.

#![no_std]

extern crate alloc;

pub mod persist;
pub mod similarity;
pub mod tfidf;
pub mod tokenize;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use similarity::cosine_similarity;
use tfidf::compute_tfidf;
use tokenize::tokenize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum EntryKind {
    Decision = 0,
    Memory = 1,
    Skill = 2,
    Session = 3,
    Reference = 4,
}

impl EntryKind {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Decision,
            2 => Self::Skill,
            3 => Self::Session,
            4 => Self::Reference,
            _ => Self::Memory,
        }
    }
}

#[derive(Clone, Debug)]
pub struct EntryMetadata {
    pub agent: String,
    pub kind: EntryKind,
    pub timestamp: u64,
    pub tags: Vec<String>,
    pub source: Option<String>,
}

impl EntryMetadata {
    pub fn new(agent: &str, kind: EntryKind) -> Self {
        EntryMetadata {
            agent: String::from(agent),
            kind,
            timestamp: 0,
            tags: Vec::new(),
            source: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct VectorEntry {
    pub id: String,
    pub text: String,
    pub embedding: Vec<f32>,
    pub metadata: EntryMetadata,
}

pub struct VectorStore {
    vocabulary: BTreeMap<String, usize>,
    entries: Vec<VectorEntry>,
    df: Vec<u32>,
    doc_count: u32,
    next_id: u64,
    dirty: bool,
}

impl VectorStore {
    pub fn new() -> Self {
        VectorStore {
            vocabulary: BTreeMap::new(),
            entries: Vec::new(),
            df: Vec::new(),
            doc_count: 0,
            next_id: 1,
            dirty: false,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn vocab_size(&self) -> usize {
        self.vocabulary.len()
    }
    pub fn all_entries(&self) -> &[VectorEntry] {
        &self.entries
    }
    pub fn vocabulary(&self) -> &BTreeMap<String, usize> {
        &self.vocabulary
    }
    pub fn df(&self) -> &[u32] {
        &self.df
    }
    pub fn doc_count(&self) -> u32 {
        self.doc_count
    }
    pub fn next_id(&self) -> u64 {
        self.next_id
    }

    pub fn insert(&mut self, text: &str, metadata: EntryMetadata) -> String {
        let id = alloc::format!("vec_{}", self.next_id);
        self.next_id += 1;
        self.insert_with_id(id.clone(), text, metadata);
        id
    }

    pub fn insert_with_id(&mut self, id: String, text: &str, metadata: EntryMetadata) {
        let tokens = tokenize(text);
        let mut grew = false;
        for t in &tokens {
            if !self.vocabulary.contains_key(t) {
                let idx = self.vocabulary.len();
                self.vocabulary.insert(t.clone(), idx);
                self.df.push(0);
                grew = true;
            }
        }
        let mut seen = BTreeMap::new();
        for t in &tokens {
            if let Some(&idx) = self.vocabulary.get(t) {
                if seen.insert(idx, ()).is_none() {
                    if idx < self.df.len() {
                        self.df[idx] = self.df[idx].saturating_add(1);
                    }
                }
            }
        }
        self.doc_count = self.doc_count.saturating_add(1);
        if grew {
            self.dirty = true;
            self.rebuild_embeddings();
        }
        let embedding = compute_tfidf(&tokens, &self.vocabulary, &self.df, self.doc_count);
        self.entries.push(VectorEntry {
            id,
            text: String::from(text),
            embedding,
            metadata,
        });
        if self.dirty {
            self.rebuild_embeddings();
            self.dirty = false;
        }
    }

    fn rebuild_embeddings(&mut self) {
        for e in &mut self.entries {
            let tokens = tokenize(&e.text);
            e.embedding = compute_tfidf(&tokens, &self.vocabulary, &self.df, self.doc_count);
        }
    }

    pub fn search(&self, query: &str, top_k: usize) -> Vec<(f32, &VectorEntry)> {
        let tokens = tokenize(query);
        let q = compute_tfidf(&tokens, &self.vocabulary, &self.df, self.doc_count.max(1));
        let mut scored: Vec<(f32, &VectorEntry)> = self
            .entries
            .iter()
            .map(|e| (cosine_similarity(&q, &e.embedding), e))
            .filter(|(s, _)| *s > 0.0)
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(core::cmp::Ordering::Equal));
        scored.truncate(top_k);
        scored
    }

    pub fn delete(&mut self, id: &str) -> bool {
        if let Some(pos) = self.entries.iter().position(|e| e.id == id) {
            self.entries.remove(pos);
            true
        } else {
            false
        }
    }

    pub fn update(&mut self, id: &str, new_text: &str) -> bool {
        let meta = match self.entries.iter().find(|e| e.id == id) {
            Some(e) => e.metadata.clone(),
            None => return false,
        };
        self.delete(id);
        self.insert_with_id(String::from(id), new_text, meta);
        true
    }

    /// Restore from persist module without re-tokenizing df (caller supplies maps).
    pub fn from_parts(
        vocabulary: BTreeMap<String, usize>,
        df: Vec<u32>,
        doc_count: u32,
        next_id: u64,
        entries: Vec<(String, String, EntryMetadata)>,
    ) -> Self {
        let mut store = VectorStore {
            vocabulary,
            entries: Vec::new(),
            df,
            doc_count,
            next_id,
            dirty: false,
        };
        for (id, text, meta) in entries {
            let tokens = tokenize(&text);
            let embedding = compute_tfidf(&tokens, &store.vocabulary, &store.df, store.doc_count.max(1));
            store.entries.push(VectorEntry {
                id,
                text,
                embedding,
                metadata: meta,
            });
        }
        store
    }
}

impl Default for VectorStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Store global (ADR-0064 §2.9 — Mutex para multi-agente).
pub static GLOBAL_STORE: Mutex<Option<VectorStore>> = Mutex::new(None);

fn with_global<R>(f: impl FnOnce(&mut VectorStore) -> R) -> R {
    let mut g = GLOBAL_STORE.lock();
    if g.is_none() {
        *g = Some(VectorStore::new());
    }
    f(g.as_mut().unwrap())
}

/// Prefixo de contexto RAG para injeção no prompt (Onda 4).
pub fn rag_context_prefix(query: &str, top_k: usize) -> String {
    with_global(|store| {
        if store.is_empty() {
            return String::new();
        }
        let hits = store.search(query, top_k);
        if hits.is_empty() {
            return String::new();
        }
        let mut s = String::from("[RAG]\n");
        for (score, e) in hits {
            s.push_str(&alloc::format!("({:.2}) {}\n", score, e.text));
        }
        s
    })
}

pub fn rag_remember(agent: &str, text: &str, kind: EntryKind) -> String {
    with_global(|store| store.insert(text, EntryMetadata::new(agent, kind)))
}

pub fn global_replace(store: VectorStore) {
    *GLOBAL_STORE.lock() = Some(store);
}

pub fn global_persist_bytes() -> Vec<u8> {
    with_global(|s| persist::to_bytes(s))
}

pub fn global_load_bytes(data: &[u8]) -> Result<(), &'static str> {
    let s = persist::from_bytes(data)?;
    global_replace(s);
    Ok(())
}

/// Self-check F1 — retorna true se PASS.
pub fn demo() -> bool {
    let mut store = VectorStore::new();
    store.insert(
        "Rust bare metal OS development",
        EntryMetadata::new("test", EntryKind::Decision),
    );
    store.insert(
        "Python scripting and data science",
        EntryMetadata::new("test", EntryKind::Decision),
    );
    store.insert(
        "Bare metal kernel development in Rust",
        EntryMetadata::new("test", EntryKind::Decision),
    );
    let results = store.search("Rust kernel", 2);
    if results.is_empty() {
        return false;
    }
    let top = &results[0].1.text;
    if !(top.contains("Rust") || top.contains("kernel") || top.contains("Bare")) {
        return false;
    }
    let v = alloc::vec![1.0f32, 2.0, 3.0];
    if (cosine_similarity(&v, &v) - 1.0).abs() >= 0.001 {
        return false;
    }
    if cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]).abs() >= 0.001 {
        return false;
    }
    // Roundtrip persist
    let bytes = persist::to_bytes(&store);
    match persist::from_bytes(&bytes) {
        Ok(s2) => s2.len() == store.len() && !s2.search("Rust", 1).is_empty(),
        Err(_) => false,
    }
}
