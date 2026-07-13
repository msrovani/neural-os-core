// hermes: Agent runtime filesystem bridges
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

pub mod dev_fs_agent;
pub mod hermes_fs_agent;
pub mod log_fs_agent;
pub mod proc_fs_agent;
pub mod ram_fs_agent;

/// Ring buffer storage — evicts oldest entries when max bytes exceeded.
pub struct RingBufStore {
    files: Mutex<BTreeMap<String, Vec<u8>>>,
    bytes: core::sync::atomic::AtomicU64,
    max: u64,
}

impl RingBufStore {
    pub const fn new(max: u64) -> Self {
        RingBufStore { files: Mutex::new(BTreeMap::new()), bytes: core::sync::atomic::AtomicU64::new(0), max }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, &str> {
        let key = path.trim_matches('/');
        if key.is_empty() { return Err("no path"); }
        self.files.lock().get(key).cloned().ok_or("not found")
    }

    pub fn write(&self, path: &str, data: &[u8]) -> Result<(), &str> {
        let key = String::from(path.trim_matches('/'));
        if key.is_empty() { return Err("no path"); }
        let mut files = self.files.lock();

        while self.bytes.load(core::sync::atomic::Ordering::Relaxed) + data.len() as u64 > self.max {
            if let Some(oldest) = files.keys().next().cloned() {
                if let Some(removed) = files.remove(&oldest) {
                    self.bytes.fetch_sub(removed.len() as u64, core::sync::atomic::Ordering::Relaxed);
                }
            } else { break; }
        }

        self.bytes.fetch_add(data.len() as u64, core::sync::atomic::Ordering::Relaxed);
        files.insert(key, Vec::from(data));
        Ok(())
    }

    pub fn keys(&self) -> Vec<String> {
        self.files.lock().keys().cloned().collect()
    }
}
