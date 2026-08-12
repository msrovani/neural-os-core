//! SysInfoAgent — retry de flush do BOOT.LOG em HW real.
//! O card de debug (CPU/memória/agentes/uptime/rede) foi removido da UI;
//! o mesh graph do compositor substituiu a visualização central.

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
        // Retry BOOT.LOG flush se ainda não foi escrito (USB-MSC pode demorar
        // a enumerar em HW real). O SysInfoAgent roda a cada 50 ticks (~2.7s).
        if !k_nano::boot_logger::FAT_READY.load(Ordering::Relaxed) {
            crate::boot_logger::flush();
        }

        AgentTickResult::Pending
    }
}
