//! ProcFsAgent — processos e agentes como arquivos.
//! Mount: /proc/
//! Arquivos: /proc/agents, /proc/meminfo, /proc/uptime, /proc/cpuinfo

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use crate::fs::FilesystemAgent;
use k_nano::interrupts::TIMER_TICKS;
use core::sync::atomic::Ordering;

pub struct ProcFsAgent;

impl ProcFsAgent {
    pub fn new() -> Self { ProcFsAgent }
}

impl FilesystemAgent for ProcFsAgent {
    fn name(&self) -> &str { "procfs" }
    fn mount_point(&self) -> &str { "/proc" }

    fn read(&self, path: &str) -> Result<Vec<u8>, &str> {
        match path.trim_matches('/') {
            "agents" | "processes" => {
                let mut s = String::from("Agents:\n");
                // Acessar AgentRegistry nao e possivel diretamente aqui,
                // mas listamos os principais conhecidos
                let agents = [
                    ("system", "System", "oneshot"),
                    ("hermes", "Router", "continuous"),
                    ("cortex", "Inference", "continuous"),
                    ("display", "Console", "continuous"),
                    ("security", "System", "continuous"),
                ];
                for (name, kind, sched) in &agents {
                    s.push_str(&alloc::format!("  {:<12} {:<12} {}\n", name, kind, sched));
                }
                Ok(s.into_bytes())
            }
            "meminfo" | "memory" => {
                let ctx = k_nano::memory::global_hardware_context();
                let uptime = TIMER_TICKS.load(Ordering::Relaxed);
                let s = alloc::format!(
                    "Total ticks: {}\nMemory: {:.1}%\nFrames allocated: {:.0}\n",
                    uptime, ctx[0] * 100.0, ctx[1]
                );
                Ok(s.into_bytes())
            }
            "uptime" => {
                let ticks = TIMER_TICKS.load(Ordering::Relaxed);
                let hz = k_nano::interrupts::TIMER_HZ.load(Ordering::Relaxed) as usize;
                let secs = ticks / hz;
                let s = alloc::format!("{} ticks ({} seconds)\n", ticks, secs);
                Ok(s.into_bytes())
            }
            "cpuinfo" | "cpu" => {
                let smp_aps = k_nano::smp::ap_entry_count();
                let s = alloc::format!(
                    "Processors: {}\nBSP: core 0\nAPs: {}\nModel: x86_64\n",
                    smp_aps + 1, smp_aps
                );
                Ok(s.into_bytes())
            }
            "version" | "neural-os" => {
                let s = alloc::format!(
                    "Neural OS Hermes v{}\n",
                    crate::hermes::HERMES_VERSION
                );
                Ok(s.into_bytes())
            }
            "profile" => {
                let p = k_ai::profile::ProfileManager::get();
                let s = alloc::format!("Current profile: {} {}\n", p.icon(), p.name());
                Ok(s.into_bytes())
            }
            "mhi" | "tiers" => {
                let reg = k_nano::mhi::MHI_REGISTRY.lock();
                Ok(reg.summary().into_bytes())
            }
            _ => Err("File not found in /proc/"),
        }
    }

    fn write(&mut self, _path: &str, _data: &[u8]) -> Result<(), &str> {
        Err("/proc/ is read-only")
    }

    fn list(&self, path: &str) -> Result<Vec<String>, &str> {
        match path.trim_matches('/') {
            "" => Ok(vec![
                String::from("agents"), String::from("meminfo"),
                String::from("uptime"), String::from("cpuinfo"),
                String::from("version"), String::from("profile"),
                String::from("mhi"),
            ]),
            _ => Err("Directory not found"),
        }
    }
}







