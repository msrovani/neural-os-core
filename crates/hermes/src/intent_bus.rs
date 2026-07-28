//! Intent Bus — sistema canônico de intenções tipadas (ADR-0076 Onda 4.4).
//! Inspirado por Squirrel AIOS: agents emitem intents semânticos, não syscalls.
//! Hermes já roteia comandos via `Command` enum — este módulo formaliza o padrão.

use alloc::string::String;

/// Categoria do intent (usada para roteamento de alto nível).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentCategory {
    System, File, Network, Agent, Ai, Security, Package, Ui, Debug, Hardware,
}

impl IntentCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            IntentCategory::System => "system",    IntentCategory::File => "file",
            IntentCategory::Network => "network",  IntentCategory::Agent => "agent",
            IntentCategory::Ai => "ai",            IntentCategory::Security => "security",
            IntentCategory::Package => "package",  IntentCategory::Ui => "ui",
            IntentCategory::Debug => "debug",      IntentCategory::Hardware => "hardware",
        }
    }
}

/// Intent canônico — o que o agente/usuário quer fazer.
#[derive(Debug, Clone)]
pub enum Intent {
    Ping, Echo(String), Uptime, Shutdown, Reboot,
    Read(String), Write(String, String), List(String), Find(String),
    HttpGet(String), PingHost(String), DnsResolve(String),
    ListAgents, InspectAgent(String), ListSkills, Learn(String, String),
    Chat(String), Infer(String), ModelInfo,
    TrustAllow(u64, String), TrustDeny(u64, String), Approve(u64), Deny(u64),
    PkgList(Option<String>), PkgInstall(String, String, String), PkgRemove(String, String),
    ThemeSet(String), UiMode(String),
    Inspect, Trace(usize), HeapInfo, Telemetry,
    PciInfo, CpuInfo, MemInfo, GpuInfo,
}

impl Intent {
    pub fn category(&self) -> IntentCategory {
        match self {
            Intent::Ping | Intent::Echo(_) | Intent::Uptime | Intent::Shutdown | Intent::Reboot => IntentCategory::System,
            Intent::Read(_) | Intent::Write(_, _) | Intent::List(_) | Intent::Find(_) => IntentCategory::File,
            Intent::HttpGet(_) | Intent::PingHost(_) | Intent::DnsResolve(_) => IntentCategory::Network,
            Intent::ListAgents | Intent::InspectAgent(_) | Intent::ListSkills | Intent::Learn(_, _) => IntentCategory::Agent,
            Intent::Chat(_) | Intent::Infer(_) | Intent::ModelInfo => IntentCategory::Ai,
            Intent::TrustAllow(_, _) | Intent::TrustDeny(_, _) | Intent::Approve(_) | Intent::Deny(_) => IntentCategory::Security,
            Intent::PkgList(_) | Intent::PkgInstall(_, _, _) | Intent::PkgRemove(_, _) => IntentCategory::Package,
            Intent::ThemeSet(_) | Intent::UiMode(_) => IntentCategory::Ui,
            Intent::Inspect | Intent::Trace(_) | Intent::HeapInfo | Intent::Telemetry => IntentCategory::Debug,
            Intent::PciInfo | Intent::CpuInfo | Intent::MemInfo | Intent::GpuInfo => IntentCategory::Hardware,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Intent::Ping => "ping", Intent::Echo(_) => "echo", Intent::Uptime => "uptime",
            Intent::Shutdown => "shutdown", Intent::Reboot => "reboot",
            Intent::Read(_) => "read", Intent::Write(_, _) => "write", Intent::List(_) => "list",
            Intent::Find(_) => "find", Intent::HttpGet(_) => "http_get", Intent::PingHost(_) => "ping_host",
            Intent::DnsResolve(_) => "dns_resolve", Intent::ListAgents => "list_agents",
            Intent::InspectAgent(_) => "inspect_agent", Intent::ListSkills => "list_skills",
            Intent::Learn(_, _) => "learn", Intent::Chat(_) => "chat", Intent::Infer(_) => "infer",
            Intent::ModelInfo => "model_info", Intent::TrustAllow(_, _) => "trust_allow",
            Intent::TrustDeny(_, _) => "trust_deny", Intent::Approve(_) => "approve",
            Intent::Deny(_) => "deny", Intent::PkgList(_) => "pkg_list",
            Intent::PkgInstall(_, _, _) => "pkg_install", Intent::PkgRemove(_, _) => "pkg_remove",
            Intent::ThemeSet(_) => "theme_set", Intent::UiMode(_) => "ui_mode",
            Intent::Inspect => "inspect", Intent::Trace(_) => "trace", Intent::HeapInfo => "heap_info",
            Intent::Telemetry => "telemetry", Intent::PciInfo => "pci_info", Intent::CpuInfo => "cpu_info",
            Intent::MemInfo => "mem_info", Intent::GpuInfo => "gpu_info",
        }
    }
}
