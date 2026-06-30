//! Trait que todo agente de filesystem deve implementar.
//! O VFS resolve o path e delega a leitura/escrita ao agente.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

pub mod ata_agent;
pub mod dev_fs_agent;
pub mod proc_fs_agent;

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
    crate::serial_println!("[FS] Agent '{}' registrado em {}", name, mp);
}

pub fn agent_for_mount(mount_point: &str) -> Option<&'static mut Box<dyn FilesystemAgent>> {
    // This would need interior mutability — simplified for now
    None
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
            if data.is_empty() { return Err("Read failed"); }
            return Ok(data);
        }
    }
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

pub fn init_fs_agents() {
    register_fs_agent(Box::new(ata_agent::AtaAgent::new()));
    register_fs_agent(Box::new(dev_fs_agent::DevFsAgent::new()));
    register_fs_agent(Box::new(proc_fs_agent::ProcFsAgent::new()));
}
