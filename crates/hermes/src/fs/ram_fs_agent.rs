//! RamFsAgent — arquivos em DRAM (cache para tiers inferiores).
//! Mount: /mnt/ram/
//! Delega ao RingBufStore genérico.

use alloc::string::String;
use alloc::vec::Vec;
use crate::fs::{FilesystemAgent, RingBufStore};
static STORE: RingBufStore = RingBufStore::new(1024 * 1024);

pub struct RamFsAgent;

impl RamFsAgent {
    pub fn new() -> Self {
        k_nano::slog_bin!("RAM", "FS", "/mnt/ram/ pronto. Max: 1MB");
        RamFsAgent
    }
}

impl FilesystemAgent for RamFsAgent {
    fn name(&self) -> &str { "ramfs" }
    fn mount_point(&self) -> &str { "/mnt/ram" }

    fn read(&self, path: &str) -> Result<Vec<u8>, &str> { STORE.read(path) }

    fn write(&mut self, path: &str, data: &[u8]) -> Result<(), &str> { STORE.write(path, data) }

    fn list(&self, path: &str) -> Result<Vec<String>, &str> {
        match path.trim_matches('/') {
            "" => Ok(STORE.keys()),
            _ => Ok(Vec::new()),
        }
    }
}







