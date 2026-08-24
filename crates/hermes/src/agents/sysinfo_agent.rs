//! SysInfoAgent — retry de flush do BOOT.LOG em HW real (canônico hermes).

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use core::sync::atomic::Ordering;

const SYSINFO_MANIFEST: AgentManifest = AgentManifest {
    name: "sysinfo",
    kind: AgentKind::System,
    schedule: ScheduleKind::PollEvery(50),
    auto_start: true,
    persist: false,
};

pub struct SysInfoAgent;

impl SysInfoAgent {
    pub fn new() -> Self {
        SysInfoAgent
    }
}

impl Agent for SysInfoAgent {
    fn manifest(&self) -> &AgentManifest {
        &SYSINFO_MANIFEST
    }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        if !k_nano::boot_logger::FAT_READY.load(Ordering::Relaxed) {
            let _ = k_nano::boot_logger::ensure_persisted();
        }
        AgentTickResult::Pending
    }
}
