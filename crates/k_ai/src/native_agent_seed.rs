//! Agent seed definitions loaded from embedded SKILL.md files at compile time.
//! Each `skills/agents/<name>/SKILL.md` is the canonical source.
//! Replaces the old NATIVE_AGENT_SEEDS compile-time constant.

use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug)]
pub struct AgentSeed {
    pub name: String,
    pub division: String,
    pub mission: String,
    pub schedule: String,
    pub native_impl: String,
    pub kind: String,
    pub skills: Vec<String>,
}

/// All embedded agent SKILL.md files.
const AGENT_SKILL_SOURCES: &[&str] = &[
    include_str!("../../../skills/agents/audio_mixer/SKILL.md"),
    include_str!("../../../skills/agents/audio_pipeline/SKILL.md"),
    include_str!("../../../skills/agents/auto_learn/SKILL.md"),
    include_str!("../../../skills/agents/boot_log/SKILL.md"),
    include_str!("../../../skills/agents/browser/SKILL.md"),
    include_str!("../../../skills/agents/cortex_llm/SKILL.md"),
    include_str!("../../../skills/agents/cron/SKILL.md"),
    include_str!("../../../skills/agents/disk_intelligence/SKILL.md"),
    include_str!("../../../skills/agents/display/SKILL.md"),
    include_str!("../../../skills/agents/fs_bridge/SKILL.md"),
    include_str!("../../../skills/agents/gpu_driver/SKILL.md"),
    include_str!("../../../skills/agents/hda_audio/SKILL.md"),
    include_str!("../../../skills/agents/hw_bridge/SKILL.md"),
    include_str!("../../../skills/agents/hw_detect/SKILL.md"),
    include_str!("../../../skills/agents/input/SKILL.md"),
    include_str!("../../../skills/agents/intent_router/SKILL.md"),
    include_str!("../../../skills/agents/jarvis/SKILL.md"),
    include_str!("../../../skills/agents/jarvis_voice/SKILL.md"),
    include_str!("../../../skills/agents/log_analyst/SKILL.md"),
    include_str!("../../../skills/agents/mcp/SKILL.md"),
    include_str!("../../../skills/agents/memory/SKILL.md"),
    include_str!("../../../skills/agents/memory_agent/SKILL.md"),
    include_str!("../../../skills/agents/monitor/SKILL.md"),
    include_str!("../../../skills/agents/mouse/SKILL.md"),
    include_str!("../../../skills/agents/net_driver/SKILL.md"),
    include_str!("../../../skills/agents/network_agent/SKILL.md"),
    include_str!("../../../skills/agents/optimizer/SKILL.md"),
    include_str!("../../../skills/agents/platform/SKILL.md"),
    include_str!("../../../skills/agents/safety/SKILL.md"),
    include_str!("../../../skills/agents/security/SKILL.md"),
    include_str!("../../../skills/agents/self_evolve/SKILL.md"),
    include_str!("../../../skills/agents/self_heal/SKILL.md"),
    include_str!("../../../skills/agents/sleep_cycle/SKILL.md"),
    include_str!("../../../skills/agents/system/SKILL.md"),
    include_str!("../../../skills/agents/trust/SKILL.md"),
    include_str!("../../../skills/agents/usb_audio/SKILL.md"),
    include_str!("../../../skills/agents/usb_driver/SKILL.md"),
    include_str!("../../../skills/agents/uvc_driver/SKILL.md"),
    include_str!("../../../skills/agents/vision/SKILL.md"),
    include_str!("../../../skills/agents/wakeword/SKILL.md"),
    include_str!("../../../skills/agents/wifi_agent/SKILL.md"),
];

/// Parse and return all agent seed definitions from embedded SKILL.md files.
pub fn load_all() -> Vec<AgentSeed> {
    AGENT_SKILL_SOURCES.iter().map(|src| parse(src)).collect()
}

fn parse(content: &str) -> AgentSeed {
    // Extract frontmatter between `---` markers
    let fm = content.split("---").nth(1).unwrap_or("");
    let mut name = String::new();
    let mut division = String::new();
    let mut mission = String::new();
    let mut schedule = String::new();
    let mut native_impl = String::new();
    let mut kind = String::new();
    let mut skills: Vec<String> = Vec::new();

    for line in fm.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(pos) = line.find(':') {
            let key = line[..pos].trim();
            let val = line[pos + 1..].trim();
            match key {
                "name" => name = String::from(val),
                "division" => division = String::from(val),
                "mission" => mission = String::from(val),
                "schedule" => schedule = String::from(val),
                "native_impl" => native_impl = String::from(val),
                "kind" => kind = String::from(val),
                "skills" => {
                    let inner = val
                        .trim_start_matches('[')
                        .trim_end_matches(']');
                    skills = inner
                        .split(',')
                        .map(|s| s.trim().trim_matches('"').trim_matches('\'').trim().into())
                        .filter(|s: &String| !s.is_empty())
                        .collect();
                }
                _ => {}
            }
        }
    }

    AgentSeed {
        name,
        division,
        mission,
        schedule,
        native_impl,
        kind,
        skills,
    }
}

/// Convenience: number of embedded agent seeds (at compile time).
pub const fn count() -> usize {
    AGENT_SKILL_SOURCES.len()
}
