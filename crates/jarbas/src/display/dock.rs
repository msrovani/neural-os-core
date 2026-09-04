//! Dock / Panel — app launchers + running windows + clock + system tray.
//! Posição: bottom (como hoje), 48px, sempre visível (sem transparency real).

use alloc::vec::Vec;
use alloc::string::String;
use super::window::AppId;
use super::tiling::Rect;

#[derive(Debug, Clone)]
pub struct DockItem {
    pub app_id: AppId,
    pub label: String,
    pub icon: Option<&'static [u8]>, // bitmap pré-rasterizado
    pub running: bool,
    pub window_count: u32,
}

#[derive(Debug, Clone)]
pub struct Dock {
    pub items: Vec<DockItem>,
    pub rect: Rect,
    pub height: u32, // 48px
    pub visible: bool,
}

impl Dock {
    pub fn new(screen_width: u32, screen_height: u32) -> Self {
        let height = 48;
        Self {
            items: Vec::new(),
            rect: Rect {
                x: 0,
                y: screen_height as i32 - height as i32,
                width: screen_width,
                height,
            },
            height,
            visible: true,
        }
    }

    pub fn add_launcher(&mut self, app_id: AppId, label: &str) {
        self.items.push(DockItem {
            app_id,
            label: String::from(label),
            icon: None,
            running: false,
            window_count: 0,
        });
    }

    pub fn set_running(&mut self, app_id: AppId, running: bool, count: u32) {
        if let Some(item) = self.items.iter_mut().find(|i| i.app_id == app_id) {
            item.running = running;
            item.window_count = count;
        }
    }

    pub fn hit_test(&self, x: i32, y: i32) -> Option<usize> {
        if !self.visible || y < self.rect.y {
            return None;
        }
        let item_width = self.rect.width / self.items.len().max(1) as u32;
        let idx = ((x - self.rect.x) / item_width as i32) as usize;
        self.items.get(idx).map(|_| idx)
    }

    pub fn render(&self, target: &mut super::fb::DoubleBuffer, theme: &super::theme::Theme) {
        

        // Background (native fill_rect avoids FbTarget borrow conflict)
        target.fill_rect(self.rect.x as usize, self.rect.y as usize,
            self.rect.width as usize, self.height as usize,
            theme.bg_alt.0, theme.bg_alt.1, theme.bg_alt.2);
        target.fill_rect(self.rect.x as usize, self.rect.y as usize,
            self.rect.width as usize, 1,
            theme.border.0, theme.border.1, theme.border.2);

        let item_width = self.rect.width / self.items.len().max(1) as u32;
        for (idx, item) in self.items.iter().enumerate() {
            let x = self.rect.x + (idx as u32 * item_width) as i32;

            // Running indicator
            if item.running {
                target.fill_rect((x + 8) as usize, (self.rect.y + self.height as i32 - 4) as usize,
                    (item_width - 16) as usize, 3,
                    theme.accent.0, theme.accent.1, theme.accent.2);
            }

            // Label
            let text_x = x + (item_width as i32 - item.label.len() as i32 * 6) / 2;
            let text_y = self.rect.y + (self.height as i32 - 15) / 2;
            let color = if item.running { theme.fg } else { theme.fg_muted };
            super::compositor::draw_text(target, text_x as usize, text_y as usize, &item.label, self.rect.width as usize, color.0, color.1, color.2);
        }

        // Clock (top-right)
        let clock_str = format_time();
        let clock_x = self.rect.x + self.rect.width as i32 - clock_str.len() as i32 * 6 - 16;
        super::compositor::draw_text(target, clock_x as usize, (self.rect.y + 16) as usize, &clock_str, self.rect.width as usize, theme.fg.0, theme.fg.1, theme.fg.2);

        // System tray indicators (battery, volume, network) - placeholders
        draw_system_tray(target, theme, self.rect);
    }
}

fn format_time() -> String {
    // TODO: usar RTC real quando disponível
    // SESSION_310: usa TIMER_HZ calibrado (64 Hz no QEMU TCG, não 1000).
    let ticks = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
    let hz = k_nano::interrupts::TIMER_HZ.load(core::sync::atomic::Ordering::Relaxed);
    let secs = if hz > 0 { ticks / hz } else { ticks / 64 };
    let hours = (secs / 3600) % 24;
    let mins = (secs / 60) % 60;
    alloc::format!("{:02}:{:02}", hours, mins)
}

fn draw_system_tray(target: &mut super::fb::DoubleBuffer, theme: &super::theme::Theme, dock_rect: Rect) {
    // Placeholders para bateria, volume, rede
    let indicators = ["BAT", "VOL", "NET"];
    let mut x = dock_rect.x + 10;
    for ind in indicators {
        super::compositor::draw_text(target, x as usize, (dock_rect.y + 16) as usize, ind, dock_rect.width as usize, theme.fg_muted.0, theme.fg_muted.1, theme.fg_muted.2);
        x += 30;
    }
}