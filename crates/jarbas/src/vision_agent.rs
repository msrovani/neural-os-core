//! VisionAgent — interpreta CAMERA_FRAME após HalOffer bind (via Hermes/UVC FE).
//! Não toca PCI/MMIO — só EventBus.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::{CapabilityToken, Event, Receiver};
use alloc::vec::Vec;
use alloc::string::String;

const VISION_MANIFEST: AgentManifest = AgentManifest {
    name: "vision",
    kind: AgentKind::Skill,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

pub struct VisionAgent {
    frame_receiver: Receiver,
    bound_receiver: Receiver,
    last_frame: Vec<u8>,
    width: u32,
    height: u32,
    offer_ok: bool,
}

impl VisionAgent {
    pub fn new() -> Self {
        VisionAgent {
            frame_receiver: k_nano::EVENT_BUS.subscribe(k_hal::offer::TOPIC_CAMERA_FRAME),
            bound_receiver: k_nano::EVENT_BUS.subscribe(k_hal::offer::TOPIC_CAMERA_BOUND),
            last_frame: Vec::new(),
            width: 640,
            height: 480,
            offer_ok: false,
        }
    }

    fn ensure_offer(&mut self) {
        if self.offer_ok {
            return;
        }
        let r = hermes::hal_offer::request_video("vision");
        self.offer_ok = r.ok;
        if r.ok {
            k_nano::slog_jarbas!("VISION", "offer", "{}", r.ack);
        }
    }

    fn process_frame(&mut self, frame: &[u8]) -> String {
        let w = self.width as usize;
        let h = self.height as usize;
        if frame.len() < w * h {
            return String::from("frame pequeno");
        }

        let mut brightness = 0u64;
        let mut edges = 0u64;
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let i = (y * w + x) * 3;
                if i + 2 >= frame.len() {
                    continue;
                }
                let r = frame[i] as u32;
                let g = frame[i + 1] as u32;
                let b = frame[i + 2] as u32;
                brightness += (r + g + b) as u64;
                let gy = (r as i32)
                    .wrapping_sub(frame[(y - 1) * w * 3 + x * 3] as i32)
                    .unsigned_abs() as u64;
                edges += gy;
            }
        }
        let total = (w * h) as u64;
        let avg_bright = brightness / total / 3;
        let avg_edge = edges / total;

        if let Some(ref mut desk) = *crate::display::compositor::COMPOSITOR.lock() {
            if let Some(cam) = desk
                .apps
                .iter_mut()
                .find(|a| a.id == crate::display::compositor::AppId::Camera)
            {
                let desc = if avg_edge > 80 {
                    "Objeto/texto detectado"
                } else if avg_bright > 180 {
                    "Ambiente claro"
                } else if avg_bright < 60 {
                    "Ambiente escuro"
                } else {
                    "Cena media"
                };
                cam.data = alloc::format!(
                    "{}\n{}x{} brilho={} bordas={}",
                    desc, w, h, avg_bright, avg_edge
                );
            }
        }

        if avg_edge > 80 {
            String::from("cena com bordas nítidas — possivel objeto/texto")
        } else if avg_bright > 180 {
            String::from("cena clara — ambiente bem iluminado")
        } else if avg_bright < 60 {
            String::from("cena escura — pouca luz")
        } else {
            alloc::format!("cena media: brilho={}, bordas={}", avg_bright, avg_edge)
        }
    }
}

impl Agent for VisionAgent {
    fn manifest(&self) -> &AgentManifest {
        &VISION_MANIFEST
    }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        while let Some(_ev) = self.bound_receiver.try_receive() {
            self.offer_ok = true;
            k_nano::slog_jarbas!("VISION", "bound", "CAMERA_BOUND recebido");
        }
        self.ensure_offer();
        if !self.offer_ok {
            return AgentTickResult::Pending;
        }
        while let Some(ev) = self.frame_receiver.try_receive() {
            let desc = self.process_frame(&ev.payload);
            self.last_frame = ev.payload.clone();
            k_nano::slog_jarbas!("VISION", "info", "{}", desc);
            let _ = k_nano::EVENT_BUS.publish(Event {
                id: 0,
                topic: alloc::string::String::from("HERMES_RESPONSE"),
                payload: alloc::format!("[VISION] {}", desc).into_bytes(),
                token: CapabilityToken::Legacy(1),
            });
        }
        AgentTickResult::Pending
    }
}
