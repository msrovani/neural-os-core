use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::tfidf::{compute_tfidf_from_indices, cosine_similarity};
use crate::tokenize::tokenize;

/// A single document entry in the vector store.
pub struct VectorEntry {
    pub id: String,
    pub text: String,
    pub embedding: Vec<f32>, // TF-IDF vector (not serialized; rebuilt on load)
    pub metadata: BTreeMap<String, String>,
}

/// In-memory TF-IDF vector store with cosine similarity search.
///
/// Thread-safe via internal Mutex. Vocabulary is built incrementally
/// as documents are inserted.
pub struct VectorStore {
    inner: Mutex<VectorStoreInner>,
}

pub(crate) struct VectorStoreInner {
    pub(crate) vocabulary: BTreeMap<String, usize>, // term → index
    pub(crate) entries: Vec<VectorEntry>,
    pub(crate) df: Vec<u32>,       // document frequency per term
    pub(crate) doc_count: u32,
    pub(crate) next_id: u64,
}

impl VectorStore {
    /// Create a new empty vector store.
    pub fn new() -> Self {
        VectorStore {
            inner: Mutex::new(VectorStoreInner {
                vocabulary: BTreeMap::new(),
                entries: Vec::new(),
                df: Vec::new(),
                doc_count: 0,
                next_id: 1,
            }),
        }
    }

    /// Insert a document. Returns the assigned ID (e.g. `"vec_1"`, `"vec_2"`).
    pub fn insert(&mut self, text: &str, metadata: BTreeMap<String, String>) -> String {
        let mut inner = self.inner.lock();
        let tokens = tokenize(text);
        let id = alloc::format!("vec_{}", inner.next_id);
        inner.next_id += 1;

        // Resolve tokens to vocabulary indices, adding new terms as needed.
        let indices: Vec<usize> = tokens
            .iter()
            .map(|t| {
                let vocab_len = inner.vocabulary.len();
                let idx = *inner.vocabulary.entry(t.clone()).or_insert(vocab_len);
                // Extend df vector if this is a new term.
                if idx >= inner.df.len() {
                    inner.df.push(0);
                }
                idx
            })
            .collect();

        // Update document frequencies: each unique term in this doc gets +1.
        let mut seen = alloc::collections::BTreeSet::new();
        for &idx in &indices {
            if seen.insert(idx) {
                if idx < inner.df.len() {
                    inner.df[idx] += 1;
                }
            }
        }

        inner.doc_count += 1;
        let embedding = compute_tfidf_from_indices(
            &indices,
            &inner.df,
            inner.doc_count,
            inner.vocabulary.len(),
        );

        inner.entries.push(VectorEntry {
            id,
            text: text.into(),
            embedding,
            metadata,
        });

        alloc::format!("vec_{}", inner.next_id - 1)
    }

    /// Search for the top-k most similar documents to `query`.
    /// Returns (score, index) pairs sorted by descending similarity.
    pub fn search(&self, query: &str, top_k: usize) -> Vec<(f32, usize)> {
        let inner = self.inner.lock();
        if inner.entries.is_empty() || top_k == 0 {
            return Vec::new();
        }

        let q_tokens = tokenize(query);
        let q_indices: Vec<usize> = q_tokens
            .iter()
            .filter_map(|t| inner.vocabulary.get(t).copied())
            .collect();

        // If no query terms match vocabulary, return empty.
        if q_indices.is_empty() {
            return Vec::new();
        }

        let q_vec = compute_tfidf_from_indices(
            &q_indices,
            &inner.df,
            inner.doc_count,
            inner.vocabulary.len(),
        );

        let mut scores: Vec<(f32, usize)> = inner
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| (cosine_similarity(&q_vec, &e.embedding), i))
            .collect();

        // Sort descending by score.
        scores.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(core::cmp::Ordering::Equal));

        scores
            .into_iter()
            .take(top_k)
            .filter(|(s, _)| *s > 0.0)
            .collect()
    }

    /// Search and return cloned entries (owned, no lifetime issues).
    pub fn search_cloned(&self, query: &str, top_k: usize) -> Vec<(f32, VectorEntry)> {
        let indices = self.search(query, top_k);
        let inner = self.inner.lock();
        indices
            .into_iter()
            .filter_map(|(s, i)| {
                inner.entries.get(i).map(|e| (s, e.clone()))
            })
            .collect()
    }

    /// Delete an entry by ID. Returns true if the entry was found and removed.
    pub fn delete(&mut self, id: &str) -> bool {
        let mut inner = self.inner.lock();
        let pos = inner.entries.iter().position(|e| e.id == id);
        if let Some(idx) = pos {
            inner.entries.swap_remove(idx);
            // ponytail: don't recompute df/vocabulary on delete — the
            // embedding vectors of remaining entries are still valid
            // (they were computed with the old df). Recompute from
            // scratch if memory pressure is a concern.
            inner.doc_count = inner.doc_count.saturating_sub(1);
            true
        } else {
            false
        }
    }

    /// Number of entries in the store.
    pub fn len(&self) -> usize {
        self.inner.lock().entries.len()
    }

    /// Vocabulary size (number of unique terms).
    pub fn vocab_size(&self) -> usize {
        self.inner.lock().vocabulary.len()
    }

    /// Clone all entries (avoids lifetime issues with the internal mutex).
    pub fn all_entries(&self) -> Vec<VectorEntry> {
        self.inner.lock().entries.clone()
    }

    /// Internal access for serialization.
    pub(crate) fn lock_inner(&self) -> spin::MutexGuard<'_, VectorStoreInner> {
        self.inner.lock()
    }
}

impl VectorEntry {
    /// Create a fresh entry (used by JSON deserialization).
    pub fn new(id: String, text: String, embedding: Vec<f32>, metadata: BTreeMap<String, String>) -> Self {
        VectorEntry { id, text, embedding, metadata }
    }
}

impl Clone for VectorEntry {
    fn clone(&self) -> Self {
        VectorEntry {
            id: self.id.clone(),
            text: self.text.clone(),
            embedding: self.embedding.clone(),
            metadata: self.metadata.clone(),
        }
    }
}
