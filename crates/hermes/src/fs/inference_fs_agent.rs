//! InferenceFsAgent — buffer efêmero sob `/inference/` (não é treino real).
//! Honesty SESSION_379: reads nunca inventam “Training”; writes = RAM só.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;
use crate::fs::FilesystemAgent;

static INFERENCE_COUNTER: AtomicU64 = AtomicU64::new(0);
static EPHEMERAL_BUF: Mutex<Vec<(String, Vec<u8>)>> = Mutex::new(Vec::new());

pub struct InferenceFsAgent;

impl InferenceFsAgent {
    pub fn new() -> Self {
        k_nano::slog_bin!("INFERENCE", "ok", "/inference/ ephemeral buffer (not real training)");
        InferenceFsAgent
    }
}

impl FilesystemAgent for InferenceFsAgent {
    fn name(&self) -> &str { "inference" }
    fn mount_point(&self) -> &str { "/inference" }

    fn read(&self, path: &str) -> Result<Vec<u8>, &str> {
        let key = path.trim_matches('/');
        if key.is_empty() {
            return Err("no path");
        }
        INFERENCE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let buf = EPHEMERAL_BUF.lock();
        if let Some((_, data)) = buf.iter().find(|(k, _)| k == key) {
            return Ok(data.clone());
        }
        // Honesty: sem entrada = erro, não texto sintético fingindo modelo.
        Err("no_model")
    }

    fn write(&mut self, path: &str, data: &[u8]) -> Result<(), &str> {
        let key = String::from(path.trim_matches('/'));
        let mut buf = EPHEMERAL_BUF.lock();
        if buf.len() >= 100 {
            buf.remove(0);
        }
        buf.push((key, data.to_vec()));
        k_nano::slog_bin!(
            "INFERENCE",
            "warn",
            "ephemeral_only path={} bytes={} (not persisted training)",
            path,
            data.len()
        );
        Ok(())
    }

    fn list(&self, path: &str) -> Result<Vec<String>, &str> {
        match path.trim_matches('/') {
            "" => {
                let buf = EPHEMERAL_BUF.lock();
                if buf.is_empty() {
                    Ok(vec![String::from("(empty — write then list)")])
                } else {
                    Ok(buf.iter().map(|(k, _)| k.clone()).collect())
                }
            }
            _ => Err("not a directory"),
        }
    }
}
