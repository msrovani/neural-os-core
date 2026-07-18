//! Skills nativas dos experts Trinity — disk_diag / security.
//! Evita cair em system_status genérico quando o MoE roteia domínio.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use skill_registry::{McpManifest, OutputSchema, Skill};

/// Diagnóstico de armazenamento (expert disk_diag).
pub struct DiskDiagSkill;

impl Skill for DiskDiagSkill {
    fn manifest(&self) -> McpManifest {
        McpManifest {
            name: String::from("disk_diag"),
            description: String::from(
                "Diagnostico de disco/armazenamento: DiskAgent, NeuralFS, capacidade",
            ),
            required_tokens: vec![1],
            preconditions: Vec::new(),
            context_links: Vec::new(),
            output_schema: OutputSchema::Any,
            idempotent: true,
            contracts: Vec::new(),
        }
    }

    fn verify(&self, _payload: &[u8]) -> Result<(), &'static str> {
        Ok(())
    }

    fn execute(&self, _payload: &[u8]) -> Result<Vec<u8>, &'static str> {
        Ok(disk_diag_report().into_bytes())
    }
}

/// Postura de segurança (expert security) — CapGate/Trust/session, não o agent Continuous.
pub struct SecuritySkill;

impl Skill for SecuritySkill {
    fn manifest(&self) -> McpManifest {
        McpManifest {
            name: String::from("security"),
            description: String::from(
                "Postura de seguranca: Trust, session Ed25519, CapGate, HITL",
            ),
            required_tokens: vec![1],
            preconditions: Vec::new(),
            context_links: Vec::new(),
            output_schema: OutputSchema::Any,
            idempotent: true,
            contracts: Vec::new(),
        }
    }

    fn verify(&self, _payload: &[u8]) -> Result<(), &'static str> {
        Ok(())
    }

    fn execute(&self, _payload: &[u8]) -> Result<Vec<u8>, &'static str> {
        Ok(security_report().into_bytes())
    }
}

pub fn disk_diag_report() -> String {
    let mut s = String::from("[DISK_DIAG]\n");
    let init = k_nano::disk_agent::DISK_AGENT_INIT.load(core::sync::atomic::Ordering::Relaxed);
    s.push_str(&alloc::format!(
        "disk_agent_init={}\n",
        if init { "yes" } else { "no (probe pending or absent)" }
    ));

    // NeuralFS / mounts leves via VFS se existir
    match crate::globals::read_vfs("/mnt/neural") {
        Ok(_) => s.push_str("neuralfs: /mnt/neural readable\n"),
        Err(e) => s.push_str(&alloc::format!("neuralfs: /mnt/neural — {}\n", e)),
    }
    match crate::globals::read_vfs("/mnt/neural/MEMORY.md") {
        Ok(b) => s.push_str(&alloc::format!("MEMORY.md: {} bytes\n", b.len())),
        Err(_) => s.push_str("MEMORY.md: absent\n"),
    }
    s.push_str("hint: use ATA/SMART via DiskIntelligenceAgent tick; /hw for PCI storage class\n");
    s
}

pub fn security_report() -> String {
    let mut s = String::from("[SECURITY]\n");
    s.push_str(&alloc::format!(
        "session_ed25519={}\n",
        if k_nano::identity::session_ready() {
            "ready"
        } else {
            "absent"
        }
    ));
    {
        let tc = crate::globals::TRUST_CACHE.lock();
        let now = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        let llm_ok = tc.is_trusted(1, "llm_generate", now)
            || tc.is_trusted_agent(1, "cortex", "llm_generate", now);
        s.push_str(&alloc::format!(
            "trust llm_generate(boot)={}\n",
            if llm_ok { "cached" } else { "not cached" }
        ));
    }
    {
        let gate = crate::globals::APPROVAL_GATE.lock();
        s.push_str(&gate.status());
        s.push('\n');
    }
    s.push_str("detectors: PortScan/PingFlood/DHCP/ARP/Timer (SecurityAgent Continuous)\n");
    s.push_str("policy: deny-by-default PackageHub; CapGate SEND_TCP on net skills\n");
    s
}
