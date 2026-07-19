//! MouseAgent — PS/2 mouse driver como agente (espelho hermes).
//! Lê IRQ12 via LAST_MOUSE_PACKET; init correto 0xD4/0xF4.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use k_nano::interrupts::LAST_MOUSE_PACKET;
use k_nano::EVENT_BUS;
use event_bus::{Event, CapabilityToken};
use alloc::string::String;
use alloc::vec::Vec;
use x86_64::instructions::port::Port;

pub const TOPIC_MOUSE_MOVED: &str = "MOUSE_MOVED";
pub const TOPIC_MOUSE_CLICK: &str = "MOUSE_CLICK";
pub const TOPIC_MOUSE_DRAG: &str = "MOUSE_DRAG";
pub const TOPIC_MOUSE_SCROLL: &str = "MOUSE_SCROLL";

const MOUSE_MANIFEST: AgentManifest = AgentManifest {
    name: "mouse",
    kind: AgentKind::Console,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

fn ps2_wait_write() {
    for _ in 0..100_000 {
        let st: u8 = unsafe { Port::<u8>::new(0x64).read() };
        if st & 0x02 == 0 {
            return;
        }
    }
}

fn ps2_wait_read() -> bool {
    for _ in 0..100_000 {
        let st: u8 = unsafe { Port::<u8>::new(0x64).read() };
        if st & 0x01 != 0 {
            return true;
        }
    }
    false
}

fn ps2_drain() {
    for _ in 0..16 {
        let st: u8 = unsafe { Port::<u8>::new(0x64).read() };
        if st & 0x01 == 0 {
            break;
        }
        let _: u8 = unsafe { Port::<u8>::new(0x60).read() };
    }
}

fn enable_ps2_mouse() {
    unsafe {
        ps2_drain();
        ps2_wait_write();
        Port::<u8>::new(0x64).write(0xA8);
        ps2_wait_write();
        Port::<u8>::new(0x64).write(0x20);
        let mut cfg = if ps2_wait_read() {
            Port::<u8>::new(0x60).read()
        } else {
            0x47
        };
        cfg |= 0x02;
        cfg &= !0x20;
        ps2_wait_write();
        Port::<u8>::new(0x64).write(0x60);
        ps2_wait_write();
        Port::<u8>::new(0x60).write(cfg);
        ps2_wait_write();
        Port::<u8>::new(0x64).write(0xD4);
        ps2_wait_write();
        Port::<u8>::new(0x60).write(0xF4);
        if ps2_wait_read() {
            let _ack: u8 = Port::<u8>::new(0x60).read();
        }
    }
    k_nano::slog_hermes!("MOUSE", "info", "PS/2 mouse enabled (IRQ12 + stream).");
}

pub struct MouseAgent {
    x: u16,
    y: u16,
    buttons: u8,
    prev_buttons: u8,
    dragging: bool,
    drag_start_x: u16,
    drag_start_y: u16,
    inited: bool,
}

impl MouseAgent {
    pub fn new() -> Self {
        MouseAgent {
            x: 640,
            y: 360,
            buttons: 0,
            prev_buttons: 0,
            dragging: false,
            drag_start_x: 0,
            drag_start_y: 0,
            inited: false,
        }
    }

    fn publish_mouse_event(&self, topic: &str, payload: Vec<u8>) {
        let _ = EVENT_BUS.publish(Event {
            id: 0,
            topic: String::from(topic),
            payload,
            token: CapabilityToken::Legacy(1),
        });
    }
}

impl Agent for MouseAgent {
    fn manifest(&self) -> &AgentManifest {
        &MOUSE_MANIFEST
    }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        if !self.inited {
            enable_ps2_mouse();
            self.inited = true;
        }

        let packet = LAST_MOUSE_PACKET.swap(0, core::sync::atomic::Ordering::Acquire);
        if packet == 0 {
            return AgentTickResult::Pending;
        }

        let b0 = (packet & 0xFF) as u8;
        let b1 = ((packet >> 8) & 0xFF) as u8;
        let b2 = ((packet >> 16) & 0xFF) as u8;
        if b0 & 0x08 == 0 {
            return AgentTickResult::Pending;
        }

        let new_buttons = b0 & 0x07;
        let dx = b1 as i8 as i16;
        let dy = -(b2 as i8 as i16);

        self.x = (self.x as i32 + dx as i32).clamp(0, 1279) as u16;
        self.y = (self.y as i32 + dy as i32).clamp(0, 719) as u16;

        let mut payload = Vec::with_capacity(8);
        payload.extend_from_slice(&self.x.to_le_bytes());
        payload.extend_from_slice(&self.y.to_le_bytes());
        payload.extend_from_slice(&dx.to_le_bytes());
        payload.extend_from_slice(&dy.to_le_bytes());
        self.publish_mouse_event(TOPIC_MOUSE_MOVED, payload);

        let pressed = new_buttons & !self.prev_buttons;
        let released = self.prev_buttons & !new_buttons;
        self.prev_buttons = new_buttons;
        self.buttons = new_buttons;

        if pressed != 0 {
            let mut payload = Vec::with_capacity(5);
            payload.push(pressed);
            payload.extend_from_slice(&self.x.to_le_bytes());
            payload.extend_from_slice(&self.y.to_le_bytes());
            self.publish_mouse_event(TOPIC_MOUSE_CLICK, payload);
            self.dragging = true;
            self.drag_start_x = self.x;
            self.drag_start_y = self.y;
        }

        if released != 0 && self.dragging {
            self.dragging = false;
            let mut payload = Vec::with_capacity(8);
            payload.extend_from_slice(&self.drag_start_x.to_le_bytes());
            payload.extend_from_slice(&self.drag_start_y.to_le_bytes());
            payload.extend_from_slice(&self.x.to_le_bytes());
            payload.extend_from_slice(&self.y.to_le_bytes());
            self.publish_mouse_event(TOPIC_MOUSE_DRAG, payload);
        }

        AgentTickResult::Pending
    }
}
