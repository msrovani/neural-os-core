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

        // Clock (top-right) — s391 JD-14: stack buf, zero String no paint.
        let mut clock_buf = [0u8; 8];
        let clock_str = format_time_into(&mut clock_buf);
        let clock_x = self.rect.x + self.rect.width as i32 - clock_str.len() as i32 * 6 - 16;
        super::compositor::draw_text(target, clock_x as usize, (self.rect.y + 16) as usize, clock_str, self.rect.width as usize, theme.fg.0, theme.fg.1, theme.fg.2);

        // s391 JD-05: tray honesto — só NET se mesh vivo (sem BAT/VOL fake).
        draw_system_tray(target, theme, self.rect);
    }
}

/// Partes do relógio do dock. `hz` fora de 8..=256 vira 18 (PIT) — calibração
/// LAPIC absurda deixava o display em 00:00 por minutos.
pub(crate) fn clock_parts(ticks: u64, hz_raw: u64) -> (u64, u64, u64) {
    let hz = if !(8..=256).contains(&hz_raw) { 18 } else { hz_raw };
    let secs = ticks / hz;
    ((secs / 3600) % 24, (secs / 60) % 60, secs % 60)
}

fn write_2dig(buf: &mut [u8], off: usize, v: u64) {
    buf[off] = b'0' + ((v / 10) % 10) as u8;
    buf[off + 1] = b'0' + (v % 10) as u8;
}

fn format_time_into(buf: &mut [u8; 8]) -> &str {
    // TODO: RTC real quando disponível. Até 1h mostra mm:ss (visível a cada segundo).
    let ticks = k_nano::interrupts::wall_ticks();
    let hz = k_nano::interrupts::TIMER_HZ.load(core::sync::atomic::Ordering::Relaxed);
    let (hours, mins, secs) = clock_parts(ticks, hz);
    if hours == 0 {
        write_2dig(buf, 0, mins);
        buf[2] = b':';
        write_2dig(buf, 3, secs);
        core::str::from_utf8(&buf[..5]).unwrap_or("00:00")
    } else {
        write_2dig(buf, 0, hours);
        buf[2] = b':';
        write_2dig(buf, 3, mins);
        core::str::from_utf8(&buf[..5]).unwrap_or("00:00")
    }
}

#[cfg(test)]
mod clock_tests {
    use super::clock_parts;

    #[test]
    fn hz_absurdo_nao_congela_em_zero() {
        // 1_000_000 Hz + 180 ticks (10s @18Hz) não pode ser 00:00.
        let (h, m, s) = clock_parts(180, 1_000_000);
        assert_eq!((h, m, s), (0, 0, 10));
        let (h0, m0, s0) = clock_parts(0, 18);
        assert_eq!((h0, m0, s0), (0, 0, 0));
        let (h1, m1, s1) = clock_parts(18, 18);
        assert_eq!((h1, m1, s1), (0, 0, 1));
    }
}

fn draw_system_tray(target: &mut super::fb::DoubleBuffer, theme: &super::theme::Theme, dock_rect: Rect) {
    // Honesty: sem bateria/volume medidos → não inventar BAT/VOL.
    // NET só quando mesh engine está vivo (rótulo curto, zero alloc).
    let mesh_live = k_nano::net::mesh::MESH_ENGINE.lock().is_some();
    if !mesh_live {
        return;
    }
    let x = dock_rect.x + 10;
    super::compositor::draw_text(
        target,
        x as usize,
        (dock_rect.y + 16) as usize,
        "NET",
        dock_rect.width as usize,
        theme.success.0,
        theme.success.1,
        theme.success.2,
    );
}
