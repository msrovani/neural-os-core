//! ADR-0065 FASE 1.2 — Server-Side Decorations (SSD).
//!
//! Provides window title bar, control buttons (close/maximize/minimize),
//! hit-testing for decoration elements, and utility rounded-rect drawing.
//! All rendering is CPU framebuffer via `DoubleBuffer::fill_rect`.

use crate::display::fb::DoubleBuffer;
use crate::display::window::{HitArea, Window};
use crate::display::theme::Theme;

// ── Geometry constants ──────────────────────────────────────────────────────

/// Title bar height in pixels.
const TITLE_BAR_H: usize = 28;
/// Side length of control buttons (close, maximize, minimize).
const BUTTON_SIZE: usize = 20;
/// Gap between adjacent buttons.
const BTN_GAP: usize = 2;
/// Width of the top border line.
const BORDER_WIDTH: usize = 1;
/// Edge/corner strip width for resize hit testing.
const RESIZE_EDGE: usize = 10;

// ── Drawing ─────────────────────────────────────────────────────────────────

/// Draw SSD (server-side decorations) for a window.
///
/// Renders the title bar, border, and control buttons using the provided theme.
/// Skips all drawing if `win.decorated` is `false`.
pub fn draw_window_decorations(
    fb: &mut DoubleBuffer,
    win: &Window,
    theme: &Theme,
    scr_w: usize,
) {
    if !win.decorated {
        return;
    }

    let x = win.rect.x as usize;
    let y = win.rect.y as usize;
    let w = win.rect.width as usize;

    // ── 1px top border line ────────────────────────────────────────────
    fb.fill_rect(x, y, w, BORDER_WIDTH, theme.border.0, theme.border.1, theme.border.2);

    // ── Title bar background ───────────────────────────────────────────
    let (bar_r, bar_g, bar_b) = if win.focused {
        (theme.accent.0, theme.accent.1, theme.accent.2)
    } else {
        (theme.bg_alt.0, theme.bg_alt.1, theme.bg_alt.2)
    };
    fb.fill_rect(
        x,
        y + BORDER_WIDTH,
        w,
        TITLE_BAR_H - BORDER_WIDTH,
        bar_r,
        bar_g,
        bar_b,
    );

    // ── Title text (left-aligned at x+6, y+6) ──────────────────────────
    let text_color = if win.focused {
        theme.fg
    } else {
        theme.fg_muted
    };
    crate::display::font::draw_text_scaled(
        fb,
        x + 6,
        y + 6,
        &win.title,
        1,
        scr_w,
        text_color.0,
        text_color.1,
        text_color.2,
    );

    // ── Control buttons (right-aligned) ────────────────────────────────
    let btn_y = y + (TITLE_BAR_H - BUTTON_SIZE) / 2;
    let close_x = x + w - BUTTON_SIZE - BTN_GAP;
    let max_x = close_x - BUTTON_SIZE - BTN_GAP;
    let min_x = max_x - BUTTON_SIZE - BTN_GAP;

    // Close [×] — red
    fb.fill_rect(
        close_x,
        btn_y,
        BUTTON_SIZE,
        BUTTON_SIZE,
        theme.error.0,
        theme.error.1,
        theme.error.2,
    );
    crate::display::font::draw_text_scaled(fb, close_x + 5, btn_y + 2, "×", 1, scr_w, 255, 255, 255);

    // Maximize [□] or Restore [R] — card_bg
    fb.fill_rect(
        max_x,
        btn_y,
        BUTTON_SIZE,
        BUTTON_SIZE,
        theme.card_bg.0,
        theme.card_bg.1,
        theme.card_bg.2,
    );
    let max_label = if win.maximized { "R" } else { "□" };
    crate::display::font::draw_text_scaled(
        fb, max_x + 5, btn_y + 2, max_label, 1, scr_w, 255, 255, 255,
    );

    // Minimize [─] — card_bg
    fb.fill_rect(
        min_x,
        btn_y,
        BUTTON_SIZE,
        BUTTON_SIZE,
        theme.card_bg.0,
        theme.card_bg.1,
        theme.card_bg.2,
    );
    crate::display::font::draw_text_scaled(fb, min_x + 5, btn_y + 2, "─", 1, scr_w, 255, 255, 255);
}

/// Draw a rectangle with simulated rounded corners.
///
/// Fills the body in three sections:
/// - Main body `(x+r, y, w-2r, h)`
/// - Left strip `(x, y+r, r, h-2r)`
/// - Right strip `(x+w-r, y+r, r, h-2r)`
///
/// Then fills the four outermost corner pixels with the fill colour to give a
/// slight visual rounding effect (the larger `r`, the larger the unfilled
/// corner notch, and the more rounded the appearance).
///
/// If `w < 2*r || h < 2*r` the whole rect is filled as a plain rectangle.
pub fn draw_rounded_rect(
    fb: &mut DoubleBuffer,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    r: usize,
    fill_r: u8,
    fill_g: u8,
    fill_b: u8,
) {
    if w < 2 * r || h < 2 * r || r == 0 {
        fb.fill_rect(x, y, w, h, fill_r, fill_g, fill_b);
        return;
    }

    // Main body (full height between the left/right strips)
    fb.fill_rect(x + r, y, w - 2 * r, h, fill_r, fill_g, fill_b);
    // Left strip (skips top and bottom r rows)
    fb.fill_rect(x, y + r, r, h - 2 * r, fill_r, fill_g, fill_b);
    // Right strip (skips top and bottom r rows)
    fb.fill_rect(x + w - r, y + r, r, h - 2 * r, fill_r, fill_g, fill_b);

    // Corner tip pixels (single pixel at each outermost corner)
    fb.fill_rect(x, y, 1, 1, fill_r, fill_g, fill_b);                     // top-left
    fb.fill_rect(x + w - 1, y, 1, 1, fill_r, fill_g, fill_b);            // top-right
    fb.fill_rect(x, y + h - 1, 1, 1, fill_r, fill_g, fill_b);            // bottom-left
    fb.fill_rect(x + w - 1, y + h - 1, 1, 1, fill_r, fill_g, fill_b);   // bottom-right
}

// ── Hit testing ────────────────────────────────────────────────────────────

/// Returns which decoration element (`HitArea`) was clicked at `(cx, cy)`,
/// or `None` if outside the window bounds entirely.
///
/// Priority order: buttons → title bar → resize edges → body.
/// Resize edges are only returned when `!win.maximized`.
pub fn hit_test(cx: usize, cy: usize, win: &Window) -> Option<HitArea> {
    let x = win.rect.x as usize;
    let y = win.rect.y as usize;
    let w = win.rect.width as usize;
    let h = win.rect.height as usize;

    // Outside window bounds
    if cx < x || cx >= x + w || cy < y || cy >= y + h {
        return None;
    }

    // Undecorated windows have no chrome — everything is body
    if !win.decorated {
        return Some(HitArea::Body);
    }

    // ── Button row (top-right corner of title bar) ─────────────────────
    let btn_y = y + (TITLE_BAR_H - BUTTON_SIZE) / 2;
    let close_x = x + w - BUTTON_SIZE - BTN_GAP;
    let max_x = close_x - BUTTON_SIZE - BTN_GAP;
    let min_x = max_x - BUTTON_SIZE - BTN_GAP;

    if cy >= btn_y && cy < btn_y + BUTTON_SIZE {
        if cx >= close_x && cx < close_x + BUTTON_SIZE {
            return Some(HitArea::CloseButton);
        }
        if cx >= max_x && cx < max_x + BUTTON_SIZE {
            return Some(HitArea::MaximizeButton);
        }
        if cx >= min_x && cx < min_x + BUTTON_SIZE {
            return Some(HitArea::MinimizeButton);
        }
    }

    // ── Title bar (top 28 px, excluding the buttons tested above) ──────
    if cy < y + TITLE_BAR_H {
        return Some(HitArea::TitleBar);
    }

    // ── Resize edges/corners (10 px strips, only when NOT maximized) ───
    if !win.maximized {
        let left = cx - x < RESIZE_EDGE;
        let right = x + w - cx <= RESIZE_EDGE;
        let top = cy - y < RESIZE_EDGE + TITLE_BAR_H;
        let bottom = y + h - cy <= RESIZE_EDGE;

        if left && top {
            return Some(HitArea::ResizeTopLeft);
        }
        if right && top {
            return Some(HitArea::ResizeTopRight);
        }
        if left && bottom {
            return Some(HitArea::ResizeBottomLeft);
        }
        if right && bottom {
            return Some(HitArea::ResizeBottomRight);
        }
        if left {
            return Some(HitArea::ResizeLeft);
        }
        if right {
            return Some(HitArea::ResizeRight);
        }
        if top {
            return Some(HitArea::ResizeTop);
        }
        if bottom {
            return Some(HitArea::ResizeBottom);
        }
    }

    // ── Window body (client area below title bar) ──────────────────────
    Some(HitArea::Body)
}
