//! MouseAgent — PS/2 mouse driver como agente.
//! Lê IRQ12 via LAST_MOUSE_PACKET, processa pacote de 3 bytes,
//! publica MOUSE_MOVED e MOUSE_CLICK no EventBus.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use k_nano::interrupts::LAST_MOUSE_PACKET;
use k_nano::globals::EVENT_BUS;
use event_bus::{Event, CapabilityToken};
use alloc::string::String;
use alloc::vec::Vec;
use x86_64::instructions::port::Port;

// Stub for display compositor (avoids jarbas_crate cycle)
mod mouse_compositor_stub {
    use spin::Mutex;
    pub static MOUSE_X: Mutex<usize> = Mutex::new(640);
    pub static MOUSE_Y: Mutex<usize> = Mutex::new(360);
}
use mouse_compositor_stub::{MOUSE_X, MOUSE_Y};

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

/// 8042 presente? Status ≠ 0xFF. **Não** rodar self-test 0xAA em runtime:
/// destrói o stream já habilitado no boot (`mouse init cfg=… ack=0xfa`) e
/// em TCG o 0xAA frequentemente TIMEOUT → falso "PS/2 ausente".
fn ps2_check_exists() -> bool {
    unsafe {
        let st: u8 = Port::<u8>::new(0x64).read();
        if st == 0xFF {
            k_nano::slog_hermes!("MOUSE", "ok", "8042 status=0xFF — ausente");
            return false;
        }
        // Probe leve: ler config (0x20). Resposta qualquer ≠ hole = controller vivo.
        ps2_wait_write();
        Port::<u8>::new(0x64).write(0x20);
        if ps2_wait_read() {
            let cfg: u8 = Port::<u8>::new(0x60).read();
            k_nano::slog_hermes!("MOUSE", "ok", "8042 present cfg={:#04x} status={:#04x}", cfg, st);
            true
        } else {
            // Boot já falou com o mouse (ack=0xfa) — status válido basta.
            k_nano::slog_hermes!("MOUSE", "ok", "8042 present status={:#04x} (cfg read timeout)", st);
            true
        }
    }
}

/// Espera buffer de entrada do 8042 livre (bit1=0), com timeout.
fn ps2_wait_write() {
    for _ in 0..100_000 {
        let st: u8 = unsafe { Port::<u8>::new(0x64).read() };
        if st & 0x02 == 0 {
            return;
        }
    }
}

/// Espera dado no buffer de saída (bit0=1), com timeout.
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

/// Init PS/2 aux correto: reset + enable IRQ12 + stream (0xD4/0xF4).
fn enable_ps2_mouse() {
    unsafe {
        ps2_drain();

        // Enable auxiliary device interface
        ps2_wait_write();
        Port::<u8>::new(0x64).write(0xA8);

        // Read controller config (cmd 0x20)
        ps2_wait_write();
        Port::<u8>::new(0x64).write(0x20);
        let mut cfg = if ps2_wait_read() {
            Port::<u8>::new(0x60).read()
        } else {
            0x47
        };
        k_nano::slog_hermes!("MOUSE", "ok", "8042 cfg_before={:#04x}", cfg);
        cfg |= 0x02; // IRQ12
        cfg |= 0x01; // IRQ1
        cfg &= !0x20; // mouse clock on
        cfg &= !0x10; // keyboard clock on
        // Write controller config (cmd 0x60)
        ps2_wait_write();
        Port::<u8>::new(0x64).write(0x60);
        ps2_wait_write();
        Port::<u8>::new(0x60).write(cfg);
        k_nano::slog_hermes!("MOUSE", "ok", "8042 cfg_after={:#04x}", cfg);

        // Reset mouse: 0xD4 / 0xFF -> ACK FA, BAT AA, ID 00
        ps2_wait_write();
        Port::<u8>::new(0x64).write(0xD4);
        ps2_wait_write();
        Port::<u8>::new(0x60).write(0xFF);
        for i in 0..3u32 {
            if ps2_wait_read() {
                let b: u8 = Port::<u8>::new(0x60).read();
                k_nano::slog_hermes!("MOUSE", "info", "reset_rsp[{}]={:#04x}", i, b);
            }
        }

        // Enable data reporting: 0xD4 / 0xF4 -> ACK FA
        ps2_wait_write();
        Port::<u8>::new(0x64).write(0xD4);
        ps2_wait_write();
        Port::<u8>::new(0x60).write(0xF4);
        if ps2_wait_read() {
            let ack: u8 = Port::<u8>::new(0x60).read();
            k_nano::slog_hermes!("MOUSE", "info", "enable_ack={:#04x} (expect 0xfa)", ack);
        } else {
            k_nano::slog_hermes!("MOUSE", "info", "enable_ack=TIMEOUT");
        }

        // Diagnóstico: Status Request 0xE9 -> se responder, o device vive;
        // se nunca vier byte de movimento depois, o host QEMU não está injetando.
        ps2_wait_write();
        Port::<u8>::new(0x64).write(0xD4);
        ps2_wait_write();
        Port::<u8>::new(0x60).write(0xE9);
        for i in 0..4u32 {
            if ps2_wait_read() {
                let b: u8 = Port::<u8>::new(0x60).read();
                k_nano::slog_hermes!("MOUSE", "info", "status_req[{}]={:#04x}", i, b);
            } else {
                k_nano::slog_hermes!("MOUSE", "info", "status_req[{}]=TIMEOUT", i);
                break;
            }
        }

        // Re-enable stream após E9
        ps2_wait_write();
        Port::<u8>::new(0x64).write(0xD4);
        ps2_wait_write();
        Port::<u8>::new(0x60).write(0xF4);
        if ps2_wait_read() {
            let ack: u8 = Port::<u8>::new(0x60).read();
            k_nano::slog_hermes!("MOUSE", "info", "re_enable_ack={:#04x}", ack);
        }
    }
    k_nano::slog_hermes!("MOUSE", "ok", "PS/2 mouse enabled (IRQ12 + stream).");
    k_nano::interrupts::mouse_log_status("after_enable");
}

fn screen_max() -> (u16, u16) {
    // Ponytail: stub - real impl would query jarbas_crate::display::fb::GPU.lock()
    // Avoids cyclic dependency hermes -> jarbas -> hermes
    (1279, 719)
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
            if ps2_check_exists() {
                enable_ps2_mouse();
            } else {
                k_nano::slog_hermes!("MOUSE", "ok", "PS/2 ausente — so USB HID mouse");
            }
            let (mw, mh) = screen_max();
            self.x = mw / 2;
            self.y = mh / 2;
            *MOUSE_X.lock() = self.x as usize;
            *MOUSE_Y.lock() = self.y as usize;
            self.inited = true;
        }

        // Poll aux — DisplayAgent é o consumidor primário com MOUSE_PORT_LOCK;
        // MouseAgent só publica EventBus a partir de LAST_MOUSE_PACKET.
        // ADR-0062 P24b: USB HID boot mouse -> mesmo path ABS/packet
        unsafe {
            let _ = k_nano::xhci::poll_mouse();
        }

        let packet = LAST_MOUSE_PACKET.swap(0, core::sync::atomic::Ordering::Acquire);
        if packet == 0 {
            return AgentTickResult::Pending;
        }

        let b0 = (packet & 0xFF) as u8;
        let b1 = ((packet >> 8) & 0xFF) as u8;
        let b2 = ((packet >> 16) & 0xFF) as u8;

        // Bit 3 do 1º byte deve ser 1 (sync). Se não, descarta.
        if b0 & 0x08 == 0 {
            return AgentTickResult::Pending;
        }

        let new_buttons = b0 & 0x07;
        let dx = b1 as i8 as i16;
        let dy = -(b2 as i8 as i16); // tela: Y para baixo

        // Posição canônica = IRQ (MOUSE_ABS_*). Não reaplicar delta (senão dobra).
        use core::sync::atomic::Ordering;
        self.x = k_nano::interrupts::MOUSE_ABS_X.load(Ordering::Acquire) as u16;
        self.y = k_nano::interrupts::MOUSE_ABS_Y.load(Ordering::Acquire) as u16;
        *MOUSE_X.lock() = self.x as usize;
        *MOUSE_Y.lock() = self.y as usize;

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
