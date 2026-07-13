//! LogFsAgent — /logs/ — arquivos de log com timestamp para analise do Cortex.
//! Cada agente/skill escreve relatorios em /logs/<agent_name>/<tick>.log.
//! LogAnalystAgent le, analisa via LLM, extrai insights e anomalias.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;
use crate::fs::FilesystemAgent;
use crate::serial_println;

static LOG_FILES: Mutex<BTreeMap<String, Vec<u8>>> = Mutex::new(BTreeMap::new());
static LOG_BYTES: AtomicU64 = AtomicU64::new(0);
const LOG_MAX_BYTES: u64 = 256 * 1024; // 256KB ring buffer

pub struct LogFsAgent;

impl LogFsAgent {
    pub fn new() -> Self {
        serial_println!("[LOG-FS] /logs/ pronto. Max: {} bytes (ring)", LOG_MAX_BYTES);
        LogFsAgent
    }
}

impl FilesystemAgent for LogFsAgent {
    fn name(&self) -> &str { "logfs" }
    fn mount_point(&self) -> &str { "/logs" }

    fn read(&self, path: &str) -> Result<Vec<u8>, &str> {
        let key = path.trim_matches('/');
        if key.is_empty() { return Err("no path"); }
        let files = LOG_FILES.lock();
        files.get(key).cloned().ok_or("log not found")
    }

    fn write(&mut self, path: &str, data: &[u8]) -> Result<(), &str> {
        let key = String::from(path.trim_matches('/'));
        if key.is_empty() { return Err("no path"); }
        let mut files = LOG_FILES.lock();

        let new_total = LOG_BYTES.load(Ordering::Relaxed) + data.len() as u64;
        if new_total > LOG_MAX_BYTES {
            // Ring buffer: evict oldest
            while LOG_BYTES.load(Ordering::Relaxed) + data.len() as u64 > LOG_MAX_BYTES {
                if let Some(oldest) = files.keys().next().cloned() {
                    if let Some(removed) = files.remove(&oldest) {
                        LOG_BYTES.fetch_sub(removed.len() as u64, Ordering::Relaxed);
                    }
                } else { break; }
            }
        }

        LOG_BYTES.fetch_add(data.len() as u64, Ordering::Relaxed);
        files.insert(key, Vec::from(data));
        Ok(())
    }

    fn list(&self, path: &str) -> Result<Vec<String>, &str> {
        let key = path.trim_matches('/');
        let files = LOG_FILES.lock();
        if key.is_empty() {
            // List all top-level "directories" (agent names)
            let mut dirs: Vec<String> = Vec::new();
            for k in files.keys() {
                if let Some(slash) = k.find('/') {
                    let agent = &k[..slash];
                    if !dirs.contains(&String::from(agent)) {
                        dirs.push(String::from(agent));
                    }
                } else {
                    dirs.push(k.clone());
                }
            }
            Ok(dirs)
        } else {
            // List files in a "directory"
            let prefix = String::from(key) + "/";
            let mut items: Vec<String> = Vec::new();
            for k in files.keys() {
                if k.starts_with(&prefix) {
                    items.push(k[prefix.len()..].to_string());
                }
            }
            Ok(items)
        }
    }
}
