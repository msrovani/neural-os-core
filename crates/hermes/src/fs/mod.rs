//! Trait que todo agente de filesystem deve implementar.
//! O VFS resolve o path e delega a leitura/escrita ao agente.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use spin::Mutex;

pub mod ata_agent;
pub mod dev_fs_agent;
pub mod proc_fs_agent;
pub mod inference_fs_agent;
pub mod hermes_fs_agent;
pub mod ram_fs_agent;
pub mod log_fs_agent;
pub mod mhi_scheduler;

pub trait FilesystemAgent: Send {
    fn name(&self) -> &str;
    fn read(&self, path: &str) -> Result<Vec<u8>, &str>;
    fn write(&mut self, path: &str, data: &[u8]) -> Result<(), &str>;
    fn list(&self, path: &str) -> Result<Vec<String>, &str>;
    fn mount_point(&self) -> &str;
}

pub struct FsAgentEntry {
    pub agent: Box<dyn FilesystemAgent>,
}

pub static FS_AGENTS: Mutex<Vec<FsAgentEntry>> = Mutex::new(Vec::new());

pub fn register_fs_agent(agent: Box<dyn FilesystemAgent>) {
    let name = alloc::format!("{}", agent.name());
    let mp = alloc::format!("{}", agent.mount_point());
    FS_AGENTS.lock().push(FsAgentEntry { agent });
    k_nano::slog_bin!("FS", "info", "Agent '{}' registrado em {}", name, mp);
}

/// Find agent by name and call a read operation
pub fn read_vfs(path: &str) -> Result<Vec<u8>, &'static str> {
    let agent_name;
    let rel_path;
    {
        let agents_opt = crate::vfs::VFS.lock();
        let vfs = agents_opt.as_ref().ok_or("VFS not initialized")?;
        let (_mount, rp, an) = vfs.resolve(path);
        agent_name = an.ok_or("No agent for path")?.to_string();
        rel_path = alloc::format!("/{}", rp);
    }

    let guard = FS_AGENTS.lock();
    for entry in guard.iter() {
        if entry.agent.name() == agent_name {
            let data = entry.agent.read(&rel_path).unwrap_or_else(|_| Vec::new());
            drop(guard);
            return Ok(data);
        }
    }
    drop(guard);
    Err("Agent not found")
}

/// Write to a VFS path
pub fn write_vfs(path: &str, data: &[u8]) -> Result<(), &'static str> {
    let agent_name;
    let rel_path;
    {
        let agents_opt = crate::vfs::VFS.lock();
        let vfs = agents_opt.as_ref().ok_or("VFS not initialized")?;
        let (_mount, rp, an) = vfs.resolve(path);
        agent_name = an.ok_or("No agent for path")?.to_string();
        rel_path = alloc::format!("/{}", rp);
    }

    let mut guard = FS_AGENTS.lock();
    for entry in guard.iter_mut() {
        if entry.agent.name() == agent_name {
            let ok = entry.agent.write(&rel_path, data).is_ok();
            drop(guard);
            if ok { return Ok(()); }
            return Err("Write failed");
        }
    }
    Err("Agent not found")
}

/// List VFS directory
pub fn list_vfs(path: &str) -> Result<Vec<String>, &'static str> {
    let agent_name;
    let rel_path;
    {
        let agents_opt = crate::vfs::VFS.lock();
        let vfs = agents_opt.as_ref().ok_or("VFS not initialized")?;
        let (_mount, rp, an) = vfs.resolve(path);
        agent_name = an.ok_or("No agent for path")?.to_string();
        rel_path = alloc::format!("/{}", rp);
    }

    let guard = FS_AGENTS.lock();
    for entry in guard.iter() {
        if entry.agent.name() == agent_name {
            let items = entry.agent.list(&rel_path).unwrap_or_else(|_| Vec::new());
            drop(guard);
            return Ok(items);
        }
    }
    Err("Agent not found")
}

/// Ring buffer storage — evicts oldest entries when max bytes exceeded.
pub struct RingBufStore {
    files: Mutex<BTreeMap<alloc::string::String, Vec<u8>>>,
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
        let key = alloc::string::String::from(path.trim_matches('/'));
        if key.is_empty() { return Err("no path"); }
        let mut files = self.files.lock();

        // Evict oldest entries until quota fits
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

    pub fn keys(&self) -> Vec<alloc::string::String> {
        self.files.lock().keys().cloned().collect()
    }

    pub fn lock(&self) -> spin::MutexGuard<'_, BTreeMap<alloc::string::String, Vec<u8>>> {
        self.files.lock()
    }
}

pub fn init_fs_agents() {
    register_fs_agent(Box::new(ata_agent::AtaAgent::new()));
    register_fs_agent(Box::new(dev_fs_agent::DevFsAgent::new()));
    register_fs_agent(Box::new(proc_fs_agent::ProcFsAgent::new()));
    register_fs_agent(Box::new(inference_fs_agent::InferenceFsAgent::new()));
    register_fs_agent(Box::new(hermes_fs_agent::HermesFsAgent::new()));
    register_fs_agent(Box::new(ram_fs_agent::RamFsAgent::new()));
    register_fs_agent(Box::new(log_fs_agent::LogFsAgent::new()));
    // NeuralFS CoW — RAM 4MB format+mount (nao sobrescreve FAT)
    register_fs_agent(Box::new(
        crate::neural_fs::neural_fs_agent::NeuralFsAgent::new(),
    ));
}







