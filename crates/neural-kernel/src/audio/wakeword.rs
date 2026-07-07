use alloc::vec;
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::{CapabilityToken, Event, Receiver};

const WAKEWORD_MANIFEST: AgentManifest = AgentManifest {
    name: "wakeword",
    kind: AgentKind::Skill,
    schedule: ScheduleKind::EventDriven,
    auto_start: true,
    persist: false,
};

pub struct WakeWordAgent {
    receiver: Receiver,
}

impl WakeWordAgent {
    pub fn new() -> Self {
        WakeWordAgent {
            receiver: crate::EVENT_BUS.subscribe(crate::audio::TOPIC_AUDIO_IN),
        }
    }
}

impl Agent for WakeWordAgent {
    fn manifest(&self) -> &AgentManifest { &WAKEWORD_MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        while let Some(_ev) = self.receiver.try_receive() {
            // Stub: publica wake word em qualquer audio (demo mode)
            let _ = crate::EVENT_BUS.publish(Event {
                id: 0,
                topic: alloc::string::String::from(crate::audio::TOPIC_WAKEWORD),
                payload: vec![b'j', b'a', b'r', b'v', b'i', b's'],
                token: CapabilityToken::Legacy(1),
            });
        }
        AgentTickResult::Pending
    }
}
