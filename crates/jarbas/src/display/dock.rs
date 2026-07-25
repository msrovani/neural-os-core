//! Dock / Panel — app launchers + running windows + clock + system tray.
//! Posição: bottom (como hoje), 48px, sempre visível (sem transparency real).

use alloc::vec::Vec;
use super::window::AppId;
use super::tiling::Rect;

#[derive(Debug, Clone)]
pub struct DockItem {
    pub app_id: AppId,
    pub label: heapless::String<32>,
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
            label: heapless::String::from(label).unwrap_or_default(),
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
        use embedded_graphics::{
            prelude::*,
            primitives::{Rectangle, PrimitiveStyle},
            pixelcolor::Rgb888,
        };

        // Background
        let bg_rect = Rectangle::new(
            Point::new(self.rect.x, self.rect.y),
            Size::new(self.rect.width, self.height),
        );
        bg_rect.into_styled(PrimitiveStyle::with_fill(theme.bg_alt)).draw(target).ok();

        // Separator top
        let sep_rect = Rectangle::new(
            Point::new(self.rect.x, self.rect.y),
            Size::new(self.rect.width, 1),
        );
        sep_rect.into_styled(PrimitiveStyle::with_fill(theme.border)).draw(target).ok();

        let item_width = self.rect.width / self.items.len().max(1) as u32;
        for (idx, item) in self.items.iter().enumerate() {
            let x = self.rect.x + (idx as u32 * item_width) as i32;
            let item_rect = Rectangle::new(
                Point::new(x, self.rect.y),
                Size::new(item_width, self.height),
            );

            // Running indicator
            if item.running {
                let indicator = Rectangle::new(
                    Point::new(x + 8, self.rect.y + self.height as i32 - 4),
                    Size::new(item_width - 16, 3),
                );
                indicator.into_styled(PrimitiveStyle::with_fill(theme.accent)).draw(target).ok();
            }

            // Label
            let text_x = x + (item_width as i32 - item.label.len() as i32 * 6) / 2;
            let text_y = self.rect.y + (self.height as i32 - 15) / 2;
            let color = if item.running { theme.fg } else { theme.fg_muted };
            super::compositor::draw_text(target, &item.label, text_x, text_y, color, &super::font::FONT_6X10);
        }

        // Clock (top-right of dock)
        let clock_str = format_time();
        let clock_x = self.rect.x + self.rect.width as i32 - clock_str.len() as i32 * 6 - 16;
        super::compositor::draw_text(target, &clock_str, clock_x, self.rect.y + 16, theme.fg, &super::font::FONT_6X10);

        // System tray indicators (battery, volume, network) - placeholders
        draw_system_tray(target, theme, self.rect);
    }
}

fn format_time() -> heapless::String<8> {
    // TODO: usar RTC real quando disponível
    let ticks = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
    let hours = (ticks / 3600000) % 24; // assumindo 1000 ticks/sec
    let mins = (ticks / 60000) % 60;
    heapless::String::from(alloc::format!("{:02}:{:02}", hours, mins)).unwrap_or_default()
}

fn draw_system_tray(target: &mut super::fb::DoubleBuffer, theme: &super::theme::Theme, dock_rect: Rect) {
    // Placeholders para bateria, volume, rede
    let indicators = ["🔋", "🔊", "📶"];
    let mut x = dock_rect.x + 10;
    for ind in indicators {
        super::compositor::draw_text(target, ind, x, dock_rect.y + 16, theme.fg_muted, &super::font::FONT_6X10);
        x += 30;
    }
}