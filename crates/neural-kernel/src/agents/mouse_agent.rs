//! MouseAgent — PS/2 mouse driver como agente.
//! Lê IRQ12 via LAST_MOUSE_PACKET, processa pacote de 3 bytes,
//! publica MOUSE_MOVED e MOUSE_CLICK no EventBus.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use crate::interrupts::LAST_MOUSE_PACKET;
use crate::EVENT_BUS;
use crate::{Event, CapabilityToken, serial_println};
use alloc::string::String;
use alloc::vec::Vec;

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
        // Enable PS/2 mouse: command 0xA8, then set compaq status bit 1
        unsafe {
            // Write 0xA8 to 0x64 = enable mouse
            x86_64::instructions::port::Port::new(0x64).write(0xA8u8);
            // Write 0x60 to 0x64 = set compaq status
            x86_64::instructions::port::Port::new(0x64).write(0x60u8);
            // Read current status from 0x60
            let mut st: u8 = 0;
            core::arch::asm!("in al, dx", out("al") st, in("dx") 0x60u16, options(nostack, preserves_flags));
            // Set bit 1 (enable mouse IRQ12)
            st |= 0x02;
            // Write back
            x86_64::instructions::port::Port::new(0x60).write(st);
            // Enable mouse packet streaming: 0xF4 to 0x60
            x86_64::instructions::port::Port::new(0x60).write(0xF4u8);
        }
        serial_println!("[MOUSE] PS/2 mouse enabled.");
        MouseAgent {
            x: 640, y: 360,
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
    fn manifest(&self) -> &AgentManifest { &MOUSE_MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        // Read mouse packet from IRQ12 handler
        let packet = LAST_MOUSE_PACKET.swap(0, core::sync::atomic::Ordering::Acquire);
        if packet == 0 { return AgentTickResult::Pending; }

        // Parse PS/2 3-byte packet (packed in u32: byte0 | (byte1<<8) | (byte2<<16))
        let b0 = (packet & 0xFF) as u8;
        let b1 = ((packet >> 8) & 0xFF) as u8;
        let b2 = ((packet >> 16) & 0xFF) as u8;

        // Byte 0: button flags
        let new_buttons = b0 & 0x07; // bits: LMB=1, RMB=2, MMB=4

        // Bytes 1,2: delta X, Y (signed, 9-bit twos complement)
        let raw_dx = b1 as i16;
        let raw_dy = -(b2 as i16); // negate Y (screen coords: down=positive)

        let dx = if raw_dx & 0x80 != 0 { raw_dx - 256 } else { raw_dx };
        let dy = if raw_dy & 0x80 != 0 { raw_dy - 256 } else { raw_dy };

        // Scale sensitivity
        let dx = dx / 2;
        let dy = dy / 2;

        // Update position (clamped to screen)
        let _old_x = self.x;
        let _old_y = self.y;
        self.x = (self.x as i32 + dx as i32).max(0).min(1279) as u16;
        self.y = (self.y as i32 + dy as i32).max(0).min(719) as u16;

        // Always publish movement
        let mut payload = Vec::with_capacity(8);
        payload.extend_from_slice(&self.x.to_le_bytes());
        payload.extend_from_slice(&self.y.to_le_bytes());
        payload.extend_from_slice(&(dx as i16).to_le_bytes());
        payload.extend_from_slice(&(dy as i16).to_le_bytes());
        self.publish_mouse_event(TOPIC_MOUSE_MOVED, payload);

        // Detect button state changes
        let pressed = new_buttons & !self.prev_buttons;
        let released = self.prev_buttons & !new_buttons;
        self.prev_buttons = new_buttons;
        self.buttons = new_buttons;

        // Click event (on press)
        if pressed != 0 {
            let mut payload = Vec::with_capacity(5);
            payload.push(pressed);
            payload.extend_from_slice(&self.x.to_le_bytes());
            payload.extend_from_slice(&self.y.to_le_bytes());
            self.publish_mouse_event(TOPIC_MOUSE_CLICK, payload);

            // Start drag
            self.dragging = true;
            self.drag_start_x = self.x;
            self.drag_start_y = self.y;
        }

        // Release event → end drag
        if released != 0 && self.dragging {
            self.dragging = false;
            let mut payload = Vec::with_capacity(6);
            payload.extend_from_slice(&self.drag_start_x.to_le_bytes());
            payload.extend_from_slice(&self.drag_start_y.to_le_bytes());
            payload.extend_from_slice(&self.x.to_le_bytes());
            payload.extend_from_slice(&self.y.to_le_bytes());
            self.publish_mouse_event(TOPIC_MOUSE_DRAG, payload);
        }

        AgentTickResult::Pending
    }
}
