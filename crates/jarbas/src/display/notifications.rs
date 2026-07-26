//! Notification queue — toasts no canto superior direito.
//! Gerenciado pelo DisplayAgent e renderizado no compositor.

use alloc::vec::Vec;
use alloc::string::String;
use super::tiling::Rect;
use super::fb::DoubleBuffer;
use super::theme::Theme;

#[derive(Debug, Clone)]
pub struct Notification {
    pub message: String,
    pub expires_at: u64,
}

#[derive(Debug, Clone)]
pub struct NotificationQueue {
    pub notifications: Vec<Notification>,
    pub max_visible: usize,
    pub ttl_ticks: u64,
}

impl NotificationQueue {
    pub fn new() -> Self {
        Self {
            notifications: Vec::new(),
            max_visible: 4,
            ttl_ticks: 300, // ~3s a 100 ticks/s
        }
    }

    pub fn push(&mut self, message: &str, now: u64) {
        self.notifications.push(Notification {
            message: String::from(message),
            expires_at: now + self.ttl_ticks,
        });
    }

    pub fn prune(&mut self, now: u64) {
        self.notifications.retain(|n| n.expires_at > now);
    }

    pub fn render(&mut self, fb: &mut DoubleBuffer, theme: &Theme, _screen: Rect) {
        self.prune(0); // pass real tick if available
        let top_right_x = 10;
        let mut y = 4;
        for n in &self.notifications {
            if y > 300 { break; }
            let w = (n.message.len() * 8 + 20).min(400);
            fb.fill_rect(top_right_x, y, w, 20, theme.bg_alt.0, theme.bg_alt.1, theme.bg_alt.2);
            fb.fill_rect(top_right_x, y, w, 1, theme.accent.0, theme.accent.1, theme.accent.2);
            super::compositor::draw_text(fb, top_right_x + 8, y + 4, &n.message, 1280, theme.fg.0, theme.fg.1, theme.fg.2);
            y += 24;
        }
    }
}

impl Default for NotificationQueue {
    fn default() -> Self {
        Self::new()
    }
}
