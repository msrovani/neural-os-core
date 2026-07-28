//! Manifestos dos 25 agentes nativos (A-001 a A-025).
//! Cada agente tem um SkillManifest FYY-compatível com system_skill=true + auto_install=true.
//! ADR-0076 Onda 1.5: agentes nativos expostos como skills do sistema.

use crate::skill_manifest::{SkillManifest, RiskLevel, Interop};

/// Retorna a lista completa de manifests dos agentes nativos.
pub fn all_native_agent_manifests() -> alloc::vec::Vec<SkillManifest> {
    alloc::vec![
        a001_system(),
        a002_monitor(),
        a003_hw_bridge(),
        a004_net(),
        a005_input(),
        a006_cortex(),
        a007_hermes(),
        a008_display(),
        a009_net_driver(),
        a010_usb_driver(),
        a011_boot_self_heal(),
        a012_boot_trust(),
        a013_platform(),
        a014_memory(),
        a015_gpu_driver(),
        a016_hw_detect(),
        a017_cron(),
        a018_security(),
        a019_safety(),
        a020_optimizer(),
        a021_sleep_cycle(),
        a022_auto_learn(),
        a023_wifi(),
        a024_wake_word(),
        a025_hda_audio(),
    ]
}

fn base(id: &str, name: &str, desc: &str) -> SkillManifest {
    let mut m = SkillManifest::new(name, desc);
    m.system_skill = true;
    m.auto_install = true;
    m.risk_level = RiskLevel::Critical;
    m.capabilities = alloc::vec![
        alloc::string::String::from("system"),
        alloc::format!("agent:{}", id),
    ];
    m.interop = Interop {
        mcp: true,
        fyy: true,
        agent_skills: true,
        a2a: false,
        clawhub: false,
        skillnet: false,
    };
    m
}

fn a001_system() -> SkillManifest {
    base("A-001", "SystemAgent", "Init, SYSTEM_READY, EchoSkill — boot phase orchestrator")
}

fn a002_monitor() -> SkillManifest {
    base("A-002", "MonitorAgent", "Publishes SYSTEM_READY, monitors boot status")
}

fn a003_hw_bridge() -> SkillManifest {
    base("A-003", "HwBridgeAgent", "Scancode IRQ bridge — keyboard/mouse raw input routing")
}

fn a004_net() -> SkillManifest {
    let mut m = base("A-004", "NetAgent", "smoltcp poll + HTTP + DNS — continuous network stack");
    m.permissions.network = alloc::string::String::from("allow");
    m
}

fn a005_input() -> SkillManifest {
    base("A-005", "InputAgent", "Keyboard PS/2 + USB xHCI — continuous input handling")
}

fn a006_cortex() -> SkillManifest {
    let mut m = base("A-006", "CortexAgent", "LLM + Medusa + Trinity MoE — on-device inference");
    m.permissions.hardware = alloc::string::String::from("allow");
    m
}

fn a007_hermes() -> SkillManifest {
    base("A-007", "HermesAgent", "Intent routing + ReAct + Skills — orchestrator")
}

fn a008_display() -> SkillManifest {
    let mut m = base("A-008", "DisplayAgent", "Framebuffer BGRA32 + compositor — UI layer");
    m.permissions.hardware = alloc::string::String::from("display");
    m
}

fn a009_net_driver() -> SkillManifest {
    let mut m = base("A-009", "NetDriverAgent", "RTL8139 + VirtIO-net — NIC driver init");
    m.permissions.hardware = alloc::string::String::from("allow");
    m
}

fn a010_usb_driver() -> SkillManifest {
    let mut m = base("A-010", "UsbDriverAgent", "xHCI port scan — USB host controller init");
    m.permissions.hardware = alloc::string::String::from("allow");
    m
}

fn a011_boot_self_heal() -> SkillManifest {
    base("A-011", "BootSelfHealAgent", "SelfHeal init — health check bootstrap")
}

fn a012_boot_trust() -> SkillManifest {
    base("A-012", "BootTrustAgent", "TrustCache init — capability token bootstrap")
}

fn a013_platform() -> SkillManifest {
    let mut m = base("A-013", "PlatformAgent", "PCI + ACPI + APIC + SMP — platform init");
    m.permissions.hardware = alloc::string::String::from("allow");
    m
}

fn a014_memory() -> SkillManifest {
    base("A-014", "MemoryAgent", "MHI + Adaptive Heap — memory management")
}

fn a015_gpu_driver() -> SkillManifest {
    let mut m = base("A-015", "GpuDriverAgent", "GPU backend detect — vendor probe");
    m.permissions.hardware = alloc::string::String::from("display");
    m
}

fn a016_hw_detect() -> SkillManifest {
    base("A-016", "HwDetectAgent", "HwIdentifySkill + IA device tree — hardware inventory")
}

fn a017_cron() -> SkillManifest {
    base("A-017", "CronAgent", "Cron Scheduler — periodic task execution")
}

fn a018_security() -> SkillManifest {
    base("A-018", "SecurityAgent", "5 detectors + Pipeline — threat detection")
}

fn a019_safety() -> SkillManifest {
    base("A-019", "SafetyAgent", "4 invariants I1-I4 — system safety monitoring")
}

fn a020_optimizer() -> SkillManifest {
    base("A-020", "OptimizerAgent", "Self-Optimization — performance tuning")
}

fn a021_sleep_cycle() -> SkillManifest {
    base("A-021", "SleepCycleAgent", "5-phase REPLAY→DREAM→CONSOLIDATE→PRUNE→REFLECT")
}

fn a022_auto_learn() -> SkillManifest {
    base("A-022", "AutoLearnAgent", "Detect need → train → register expert — skill auto-generation")
}

fn a023_wifi() -> SkillManifest {
    let mut m = base("A-023", "WifiAgent", "802.11 scan + WPA2 + connection — wireless networking");
    m.permissions.network = alloc::string::String::from("allow");
    m
}

fn a024_wake_word() -> SkillManifest {
    base("A-024", "WakeWordAgent", "Jarvis wake-word detection by energy")
}

fn a025_hda_audio() -> SkillManifest {
    let mut m = base("A-025", "HdaAudioAgent", "Intel HDA audio driver — playback + capture");
    m.permissions.hardware = alloc::string::String::from("audio");
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_native_agents_have_manifests() {
        let agents = all_native_agent_manifests();
        assert_eq!(agents.len(), 25, "Must have exactly 25 native agents (A-001 to A-025)");
    }

    #[test]
    fn test_each_agent_has_unique_name() {
        let agents = all_native_agent_manifests();
        let mut names: alloc::vec::Vec<&str> = agents.iter().map(|a| a.name.as_str()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 25, "All agent names must be unique");
    }

    #[test]
    fn test_all_agents_are_system_skills() {
        for a in all_native_agent_manifests() {
            assert!(a.system_skill, "Agent {} must be system_skill", a.name);
            assert!(a.auto_install, "Agent {} must be auto_install", a.name);
        }
    }

    #[test]
    fn test_agent_manifest_roundtrip() {
        for a in all_native_agent_manifests() {
            let json = a.to_json();
            let parsed = crate::skill_manifest::SkillManifest::from_json_str(&json).unwrap();
            assert_eq!(a.name, parsed.name);
            assert_eq!(a.system_skill, parsed.system_skill);
        }
    }
}
