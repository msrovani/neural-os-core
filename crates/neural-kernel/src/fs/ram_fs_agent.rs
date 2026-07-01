//! RamFsAgent — arquivos em DRAM (cache para tiers inferiores).
//! Mount: /mnt/ram/
//! read/write para arquivos temporarios em memoria.
//! Usado como cache pelo MHI para promover arquivos quentes de HDD→DRAM.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;
use crate::fs::FilesystemAgent;
use crate::serial_println;

static RAM_FILES: Mutex<BTreeMap<String, Vec<u8>>> = Mutex::new(BTreeMap::new());
static RAM_BYTES: AtomicU64 = AtomicU64::new(0);
const RAM_MAX_BYTES: u64 = 1024 * 1024; // 1MB max

pub struct RamFsAgent;

impl RamFsAgent {
    pub fn new() -> Self {
        serial_println!("[RAM-FS] /mnt/ram/ pronto. Max: {} bytes", RAM_MAX_BYTES);
        RamFsAgent
    }
}

impl FilesystemAgent for RamFsAgent {
    fn name(&self) -> &str { "ramfs" }
    fn mount_point(&self) -> &str { "/mnt/ram" }

    fn read(&self, path: &str) -> Result<Vec<u8>, &str> {
        let key = path.trim_matches('/');
        if key.is_empty() { return Err("no path"); }
        let files = RAM_FILES.lock();
        files.get(key).cloned().ok_or("file not found")
    }

    fn write(&mut self, path: &str, data: &[u8]) -> Result<(), &str> {
        let key = String::from(path.trim_matches('/'));
        if key.is_empty() { return Err("no path"); }
        let mut files = RAM_FILES.lock();

        // Check quota
        let new_bytes = RAM_BYTES.load(Ordering::Relaxed) + data.len() as u64;
        if new_bytes > RAM_MAX_BYTES {
            // Evict oldest entry
            if let Some(oldest) = files.keys().next().cloned() {
                if let Some(removed) = files.remove(&oldest) {
                    RAM_BYTES.fetch_sub(removed.len() as u64, Ordering::Relaxed);
                }
            }
        }

        files.insert(key, data.to_vec());
        RAM_BYTES.fetch_add(data.len() as u64, Ordering::Relaxed);
        Ok(())
    }

    fn list(&self, path: &str) -> Result<Vec<String>, &str> {
        match path.trim_matches('/') {
            "" => {
                let files = RAM_FILES.lock();
                Ok(files.keys().cloned().collect())
            }
            _ => Ok(Vec::new()),
        }
    }
}

/// Return total bytes used in RAM FS
pub fn ram_used_bytes() -> u64 {
    RAM_BYTES.load(Ordering::Relaxed)
}
