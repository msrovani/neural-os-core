use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};

const HDA_MANIFEST: AgentManifest = AgentManifest {
    name: "hda_audio",
    kind: AgentKind::Driver,
    schedule: ScheduleKind::Oneshot,
    auto_start: true,
    persist: false,
};

pub struct HdaAudioAgent;

impl HdaAudioAgent {
    pub fn new() -> Self { HdaAudioAgent }
}

impl Agent for HdaAudioAgent {
    fn manifest(&self) -> &AgentManifest { &HDA_MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        crate::serial_println!("[HDA] Intel HDA audio driver stub — Sprint Sound");
        crate::serial_println!("[HDA] PCI scan + BAR0 UC mapping + DMA ring buffer pendente");
        AgentTickResult::Done
    }
}
