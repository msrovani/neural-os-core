//! Notification overlay — estilo COSMIC, sobre o compositor.
//! Notificações temporarias que aparecem no canto superior direito.

use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use crate::display::fb::DoubleBuffer;
use crate::display::theme;
use crate::display::font;

#[derive(Debug, Clone)]
pub struct Notification {
    pub title: String,
    pub body: String,
    pub severity: u8, // 1=info, 2=warning, 3=error
    pub expire_tick: u64,
    pub id: u64,
}

pub struct NotificationOverlay {
    pub queue: VecDeque<Notification>,
    next_id: u64,
}

impl NotificationOverlay {
    pub fn new() -> Self {
        NotificationOverlay { queue: VecDeque::new(), next_id: 1 }
    }

    pub fn push(&mut self, title: &str, body: &str, severity: u8, duration_ticks: u64) {
        let tick = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        if self.queue.len() >= 5 { self.queue.pop_front(); }
        self.queue.push_back(Notification {
            title: String::from(title),
            body: String::from(body),
            severity,
            expire_tick: tick + duration_ticks,
            id: self.next_id,
        });
        self.next_id += 1;
    }

    pub fn render(&mut self, fb: &mut DoubleBuffer) {
        let tick = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        let t = theme::current();

        // Remove expired
        self.queue.retain(|n| tick < n.expire_tick);

        // Render notifications do canto superior direito
        let mut y_offset = 10i32;
        for notif in &self.queue {
            let color = match notif.severity {
                3 => (220, 50, 50),    // red
                2 => (220, 200, 0),    // yellow
                _ => (0, 200, 200),    // cyan
            };
            let bg = (20, 20, 40);
            let box_w = 300u32;
            let box_h = 50u32;
            let x = (fb.info.width as i32) - 310;

            // Background
            Compositor::draw_rect(fb, x, y_offset, box_w, box_h, bg);
            // Title
            Compositor::draw_text(fb, x + 4, y_offset + 2, &notif.title, color, bg);
            // Body
            Compositor::draw_text(fb, x + 4, y_offset + 18, &notif.body, t.fg, bg);

            y_offset += box_h as i32 + 4;
        }
    }
}

// Reuse compositor draw utils
use crate::display::compositor::Compositor;
