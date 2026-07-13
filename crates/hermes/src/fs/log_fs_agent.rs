//! LogFsAgent — /logs/ — arquivos de log com timestamp para analise do Cortex.
//! Delega ao RingBufStore genérico.

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use crate::fs::{FilesystemAgent, RingBufStore};
use crate::serial_println;

static STORE: RingBufStore = RingBufStore::new(256 * 1024);

pub struct LogFsAgent;

impl LogFsAgent {
    pub fn new() -> Self {
        serial_println!("[LOG-FS] /logs/ pronto. Max: 256KB (ring)");
        LogFsAgent
    }
}

impl FilesystemAgent for LogFsAgent {
    fn name(&self) -> &str { "logfs" }
    fn mount_point(&self) -> &str { "/logs" }

    fn read(&self, path: &str) -> Result<Vec<u8>, &str> { STORE.read(path) }

    fn write(&mut self, path: &str, data: &[u8]) -> Result<(), &str> { STORE.write(path, data) }

    fn list(&self, path: &str) -> Result<Vec<String>, &str> {
        let key = path.trim_matches('/');
        if key.is_empty() {
            let mut dirs: Vec<String> = Vec::new();
            for k in STORE.keys() {
                if let Some(slash) = k.find('/') {
                    let agent = &k[..slash];
                    if !dirs.contains(&String::from(agent)) {
                        dirs.push(String::from(agent));
                    }
                } else {
                    dirs.push(k);
                }
            }
            Ok(dirs)
        } else {
            let prefix = String::from(key) + "/";
            Ok(STORE.keys().into_iter().filter(|k| k.starts_with(&prefix))
                .map(|k| k[prefix.len()..].to_string()).collect())
        }
    }
}
