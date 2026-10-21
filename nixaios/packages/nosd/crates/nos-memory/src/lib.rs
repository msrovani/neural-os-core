//! nos-memory — substrato alinhado a neural-sgdb (ADR-0091).
//!
//! Scaffold: FileStorage JSONL append-only + recall lexical.
//! Quando o crate `neural-sgdb` estiver no path do flake, substituir o store
//! interno por `FileStorage` nsgdb (sem dual-truth).

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use tracing::info;

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub key: String,
    pub text: String,
    pub scope: String,
    pub entities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hit {
    pub key: String,
    pub text: String,
    pub score: f32,
    pub matched_terms: Vec<String>,
}

#[derive(Clone)]
pub struct NosMemory {
    path: PathBuf,
    // single writer lock (doutrina nsgdb)
    lock: Arc<Mutex<()>>,
}

impl NosMemory {
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, MemoryError> {
        let dir = dir.as_ref();
        fs::create_dir_all(dir)?;
        let path = dir.join("memories.jsonl");
        if !path.exists() {
            File::create(&path)?;
        }
        info!(target: "nosd", "nsgdb facade open {}", path.display());
        Ok(Self {
            path,
            lock: Arc::new(Mutex::new(())),
        })
    }

    pub fn remember(&self, rec: MemoryRecord) -> Result<String, MemoryError> {
        let _g = self.lock.lock();
        let mut f = OpenOptions::new().append(true).open(&self.path)?;
        let line = serde_json::to_string(&rec)?;
        writeln!(f, "{line}")?;
        Ok(rec.key.clone())
    }

    pub fn recall_lexical(&self, query: &str, k: usize) -> Result<Vec<Hit>, MemoryError> {
        let _g = self.lock.lock();
        let f = fs::File::open(&self.path)?;
        let terms: Vec<&str> = query.split_whitespace().filter(|t| t.len() > 1).collect();
        let mut hits = Vec::new();
        for line in BufReader::new(f).lines() {
            let line = line?;
            if line.is_empty() {
                continue;
            }
            let rec: MemoryRecord = serde_json::from_str(&line)?;
            let lower = rec.text.to_ascii_lowercase();
            let mut matched = Vec::new();
            let mut score = 0.0f32;
            for t in &terms {
                let tl = t.to_ascii_lowercase();
                if lower.contains(&tl) {
                    matched.push((*t).to_string());
                    score += 1.0;
                }
            }
            if score > 0.0 || terms.is_empty() {
                hits.push(Hit {
                    key: rec.key,
                    text: rec.text,
                    score,
                    matched_terms: matched,
                });
            }
        }
        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        hits.truncate(k.max(1));
        Ok(hits)
    }

    pub fn health(&self) -> bool {
        self.path.exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn remember_recall() {
        let d = tempdir().unwrap();
        let m = NosMemory::open(d.path()).unwrap();
        m.remember(MemoryRecord {
            key: "md/L4/t1".into(),
            text: "neural os mesh peers".into(),
            scope: "project/neural-os-core".into(),
            entities: vec!["mom/fact".into()],
        })
        .unwrap();
        let hits = m.recall_lexical("mesh", 5).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].matched_terms.iter().any(|t| t == "mesh"));
    }
}
