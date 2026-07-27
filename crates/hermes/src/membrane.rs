//! Membrane — zero ambient authority para execução de skills (ADR-0076 F2).
//! Inspirado pelo modelo de membranes do Wetware: toda skill começa sem
//! nenhuma capacidade, e só recebe o que a membrana concede.
//!
//! AIOS na veia: segurança por default deny, não por default allow.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

/// Capacidade que pode ser concedida a uma skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    VfsRead,
    VfsWrite,
    NetTcp,
    NetUdp,
    DisplayFb,
    AudioPlay,
    AudioCapture,
    UsbAccess,
    GpuCompute,
    RawMmio,      // Jamais concedido para apps legacy
    PortIo,       // Jamais concedido para apps legacy
}

impl Capability {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "vfs_read" => Some(Capability::VfsRead),
            "vfs_write" => Some(Capability::VfsWrite),
            "net_tcp" => Some(Capability::NetTcp),
            "net_udp" => Some(Capability::NetUdp),
            "display_fb" => Some(Capability::DisplayFb),
            "audio_play" => Some(Capability::AudioPlay),
            "audio_capture" => Some(Capability::AudioCapture),
            "usb_access" => Some(Capability::UsbAccess),
            "gpu_compute" => Some(Capability::GpuCompute),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Capability::VfsRead => "vfs_read",
            Capability::VfsWrite => "vfs_write",
            Capability::NetTcp => "net_tcp",
            Capability::NetUdp => "net_udp",
            Capability::DisplayFb => "display_fb",
            Capability::AudioPlay => "audio_play",
            Capability::AudioCapture => "audio_capture",
            Capability::UsbAccess => "usb_access",
            Capability::GpuCompute => "gpu_compute",
            Capability::RawMmio => "raw_mmio",
            Capability::PortIo => "port_io",
        }
    }
}

/// Operação que uma skill tenta executar.
#[derive(Debug, Clone)]
pub enum Operation {
    FileRead(String),
    FileWrite(String),
    NetConnect(String, u16),
    CapabilityUse(Capability),
}

impl Operation {
    pub fn name(&self) -> &'static str {
        match self {
            Operation::FileRead(_) => "file_read",
            Operation::FileWrite(_) => "file_write",
            Operation::NetConnect(_, _) => "net_connect",
            Operation::CapabilityUse(_) => "capability_use",
        }
    }
}

/// Resultado da verificação de uma operação contra a membrana.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Deny,
    Escalate,  // Precisa de aprovação humana (HITL)
}

/// Glob pattern simplificado para paths de filesystem.
fn glob_match(pattern: &str, path: &str) -> bool {
    if pattern.ends_with("/*") {
        let prefix = &pattern[..pattern.len() - 1];
        path.starts_with(prefix)
    } else {
        pattern == path
    }
}

/// Membrana — bundle de capabilities que define o que uma skill pode fazer.
#[derive(Debug, Clone)]
pub struct Membrane {
    /// Nome da skill/app
    pub name: String,
    /// Paths de filesystem permitidos (glob)
    pub fs_allow: Vec<String>,
    /// Paths bloqueados (glob) — sobrescreve allow
    pub fs_deny: Vec<String>,
    /// Endpoints de rede permitidos
    pub net_allow: Vec<String>,
    /// Capacidades concedidas
    pub capabilities: Vec<Capability>,
    /// Fuel máximo (instruções WASM)
    pub fuel_budget: u64,
    /// Heap máximo (bytes)
    pub heap_max: usize,
    /// Timeout de execução (ms)
    pub timeout_ms: u64,
}

impl Membrane {
    /// Membrana para app legacy — zero capacidades, mínimo absoluto.
    pub fn for_legacy(app_name: &str) -> Self {
        Self {
            name: String::from(app_name),
            fs_allow: vec![
                alloc::format!("/jail/{}/*", app_name),
                String::from("/tmp/*"),
            ],
            fs_deny: vec![
                String::from("/etc/*"),
                String::from("/boot/*"),
                String::from("/dev/*"),
                String::from("/sys/*"),
            ],
            net_allow: Vec::new(),  // Sem rede
            capabilities: Vec::new(), // Sem HW
            fuel_budget: 10_000_000,
            heap_max: 256 * 1024 * 1024,
            timeout_ms: 60_000,
        }
    }

    /// Membrana para skill WASM confiável — mais permissiva.
    pub fn for_wasm(name: &str, caps: Vec<Capability>) -> Self {
        Self {
            name: String::from(name),
            fs_allow: vec![
                String::from("/tmp/*"),
                String::from("/home/*"),
            ],
            fs_deny: vec![
                String::from("/etc/*"),
                String::from("/boot/*"),
            ],
            net_allow: Vec::new(),
            capabilities: caps,
            fuel_budget: 50_000_000,
            heap_max: 512 * 1024 * 1024,
            timeout_ms: 120_000,
        }
    }

    /// Verifica se uma operação é permitida pela membrana.
    pub fn check(&self, op: &Operation) -> Verdict {
        match op {
            Operation::FileRead(path) => self.check_fs(path, false),
            Operation::FileWrite(path) => self.check_fs(path, true),
            Operation::NetConnect(host, _port) => {
                if self.net_allow.is_empty() {
                    return Verdict::Deny;
                }
                if self.net_allow.iter().any(|a| a == host || a == "*") {
                    Verdict::Allow
                } else {
                    Verdict::Escalate
                }
            }
            Operation::CapabilityUse(cap) => {
                if self.capabilities.contains(cap) {
                    Verdict::Allow
                } else if matches!(cap, Capability::RawMmio | Capability::PortIo) {
                    Verdict::Deny  // Jamais permitido
                } else {
                    Verdict::Escalate  // Pode ser permitido com aprovação
                }
            }
        }
    }

    fn check_fs(&self, path: &str, write: bool) -> Verdict {
        // Deny paths têm prioridade
        for deny in &self.fs_deny {
            if glob_match(deny, path) {
                return Verdict::Deny;
            }
        }
        // Allow paths
        for allow in &self.fs_allow {
            if glob_match(allow, path) {
                return Verdict::Allow;
            }
        }
        // Escalate se for write (pode ser necessário), deny se for read estranho
        if write { Verdict::Escalate } else { Verdict::Deny }
    }
}

impl Default for Membrane {
    fn default() -> Self {
        Self {
            name: String::from("default"),
            fs_allow: Vec::new(),
            fs_deny: Vec::new(),
            net_allow: Vec::new(),
            capabilities: Vec::new(),
            fuel_budget: 1_000_000,
            heap_max: 64 * 1024 * 1024,
            timeout_ms: 30_000,
        }
    }
}

impl Membrane {
    /// Retorna uma descrição legível da membrana.
    pub fn describe(&self) -> String {
        let caps: Vec<&str> = self.capabilities.iter().map(|c| c.as_str()).collect();
        alloc::format!(
            "Membrane[{}]: fs_allow={:?} fs_deny={:?} net={:?} caps={} fuel={} heap={}",
            self.name, self.fs_allow, self.fs_deny,
            self.net_allow, caps.join(","),
            self.fuel_budget, self.heap_max
        )
    }
}

/// Self-test da Membrane.
pub fn demo() -> bool {
    let m = Membrane::for_legacy("test_app");

    // Paths permitidos
    assert!(m.check(&Operation::FileRead(String::from("/jail/test_app/doc.txt"))) == Verdict::Allow);
    assert!(m.check(&Operation::FileWrite(String::from("/jail/test_app/out.txt"))) == Verdict::Allow);

    // Paths negados
    assert!(m.check(&Operation::FileRead(String::from("/etc/passwd"))) == Verdict::Deny);
    assert!(m.check(&Operation::FileWrite(String::from("/boot/init"))) == Verdict::Deny);

    // Rede negada
    assert!(m.check(&Operation::NetConnect(String::from("google.com"), 80)) == Verdict::Deny);

    // Capability não concedida
    assert!(m.check(&Operation::CapabilityUse(Capability::GpuCompute)) == Verdict::Escalate);

    // RawMmio sempre deny
    assert!(m.check(&Operation::CapabilityUse(Capability::RawMmio)) == Verdict::Deny);

    true
}
