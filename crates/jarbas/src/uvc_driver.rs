//! UVC Driver FE — câmera via HalOffer (Hermes → k-hal), sem PCI/MMIO no R3.
//! Publica CAMERA_FRAME só após CAMERA_BOUND / bind OK.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::{CapabilityToken, Event};
use alloc::vec::Vec;

const UVC_MANIFEST: AgentManifest = AgentManifest {
    name: "uvc_driver",
    kind: AgentKind::Driver,
    schedule: ScheduleKind::Oneshot,
    auto_start: true,
    persist: false,
};

pub struct UvcDriverAgent {
    bound: bool,
}

impl UvcDriverAgent {
    pub fn new() -> Self {
        UvcDriverAgent { bound: false }
    }

    /// Pede câmera ao Hermes/HalOffer — não escaneia PCI.
    fn ensure_bound(&mut self) -> bool {
        if self.bound {
            return true;
        }
        match hermes::hal_offer::ensure_camera_bound("uvc_driver") {
            Ok(h) => {
                k_nano::slog_jarbas!(
                    "UVC",
                    "bind",
                    "HalOffer OK topic={} slot={}",
                    h.topic,
                    h.slot
                );
                self.bound = true;
                true
            }
            Err(e) => {
                k_nano::slog_jarbas!("UVC", "bind", "HalOffer DENY {:?}", e);
                false
            }
        }
    }
}

impl Agent for UvcDriverAgent {
    fn manifest(&self) -> &AgentManifest {
        &UVC_MANIFEST
    }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        if !self.ensure_bound() {
            return AgentTickResult::Done;
        }
        // Stub frame — captura isoc real fica no BE k-hal (fora deste MVP 1.8.x)
        let test_frame: Vec<u8> = alloc::vec![128u8; 640 * 480 * 3];
        let _ = k_nano::EVENT_BUS.publish(Event {
            id: 0,
            topic: alloc::string::String::from(k_hal::offer::TOPIC_CAMERA_FRAME),
            payload: test_frame,
            token: CapabilityToken::Legacy(1),
        });
        k_nano::slog_jarbas!("UVC", "frame", "stub 640x480 publicado (FE)");
        AgentTickResult::Done
    }
}
