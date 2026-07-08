//! VisionAgent — enxerga via USB camera, processa frames, descreve cenas.
//! UvcDriverAgent controla o hardware, VisionAgent interpreta.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::{CapabilityToken, Event, Receiver};
use alloc::vec::Vec;
use alloc::string::String;
use crate::serial_println;

const VISION_MANIFEST: AgentManifest = AgentManifest {
    name: "vision",
    kind: AgentKind::Skill,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

pub struct VisionAgent {
    frame_receiver: Receiver,
    last_frame: Vec<u8>,
    width: u32, height: u32,
}

impl VisionAgent {
    pub fn new() -> Self {
        VisionAgent {
            frame_receiver: crate::EVENT_BUS.subscribe("CAMERA_FRAME"),
            last_frame: Vec::new(),
            width: 640, height: 480,
        }
    }

    fn process_frame(&mut self, frame: &[u8]) -> String {
        // Analise simples baseada em histograma e bordas
        let w = self.width as usize;
        let h = self.height as usize;
        if frame.len() < w * h { return String::from("frame pequeno"); }

        let mut brightness = 0u64;
        let mut edges = 0u64;
        for y in 1..h-1 {
            for x in 1..w-1 {
                let i = (y * w + x) * 3;
                if i + 2 >= frame.len() { continue; }
                let r = frame[i] as u32; let g = frame[i+1] as u32; let b = frame[i+2] as u32;
                brightness += (r + g + b) as u64;
                let gy = (r as i32).wrapping_sub(frame[(y-1)*w*3 + x*3] as i32).unsigned_abs() as u64;
                edges += gy;
            }
        }
        let total = (w * h) as u64;
        let avg_bright = brightness / total / 3;
        let avg_edge = edges / total;

        if avg_edge > 80 { String::from("cena com bordas nítidas — possivel objeto/texto") }
        else if avg_bright > 180 { String::from("cena clara — ambiente bem iluminado") }
        else if avg_bright < 60 { String::from("cena escura — pouca luz") }
        else { alloc::format!("cena media: brilho={}, bordas={}", avg_bright, avg_edge) }
    }
}

impl Agent for VisionAgent {
    fn manifest(&self) -> &AgentManifest { &VISION_MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        while let Some(ev) = self.frame_receiver.try_receive() {
            let desc = self.process_frame(&ev.payload);
            self.last_frame = ev.payload.clone();
            serial_println!("[VISION] {}", desc);
            let _ = crate::EVENT_BUS.publish(Event {
                id: 0, topic: alloc::string::String::from("HERMES_RESPONSE"),
                payload: alloc::format!("[VISION] {}", desc).into_bytes(),
                token: CapabilityToken::Legacy(1),
            });
        }
        AgentTickResult::Pending
    }
}
