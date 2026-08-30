//! Orb JARVIS (Soul Mirror) — holograma MCU com grid hex, glow radial, particles.
//!
//! Técnicas de rendering:
//! - Distance² (sem sqrtf) + quadratic falloff (sem expf)
//! - LUT seno para particles (256 entradas, sem libm sinf/cosf)
//! - Scanline fill para anéis (1 draw_rect por linha)
//! - Alpha blend inteiro: `(fg*α + bg*(255-α)) >> 8`

use core::f32::consts::PI;
use libm::{sinf, cosf};
use crate::display::fb::DoubleBuffer;

// ═══════════════════════════════════════════════════════════════
// JARVIS COLOR PALETTE
// ═══════════════════════════════════════════════════════════════
const JARVIS_CYAN: (u8, u8, u8) = (0, 212, 255);      // #00D4FF — primary
const JARVIS_CYAN_DIM: (u8, u8, u8) = (0, 80, 120);    // dim cyan for glow
const JARVIS_BLUE: (u8, u8, u8) = (0, 100, 200);       // #0064C8 — rings
const JARVIS_WHITE: (u8, u8, u8) = (200, 240, 255);    // #C8F0FF — core highlight
const JARVIS_BG: (u8, u8, u8) = (8, 12, 24);           // #080C18 — deep navy

/// Visual state derived from AffectVector + LoopPhase.
#[derive(Clone, Copy)]
pub struct SoulMirrorState {
    pub color: (u8, u8, u8),
    pub prev_color: (u8, u8, u8),
    pub pulse_speed: f32,
    pub size_scale: f32,
    pub ring_count: u32,
    pub rotation_deg: u32,
    pub brightness: f32,
    pub avatar_override: Option<(u8, u8, u8)>,
}

impl SoulMirrorState {
    pub fn jarbas_color(state: &str) -> Option<(u8, u8, u8)> {
        match state {
            "DREAM" => Some((30, 0, 80)),
            "THINK" => Some((0, 180, 255)),
            "IDLE"  => Some((0, 120, 200)),
            "ALERT" => Some((255, 60, 80)),
            _ => None,
        }
    }

    pub fn from_affect(affect: &hermes::affect::AffectVector, phase_deg: u32, avatar_state: Option<&str>) -> Self {
        let (r, g, b) = affect.valence_to_rgb();
        let avatar_override = avatar_state.and_then(Self::jarbas_color);
        let color = avatar_override.unwrap_or((r, g, b));
        SoulMirrorState {
            color,
            prev_color: color,
            avatar_override,
            pulse_speed: 0.5 + affect.arousal * 0.5,
            size_scale: 0.5 + affect.dominance * 0.5,
            ring_count: (affect.curiosity * 6.0) as u32,
            rotation_deg: phase_deg.wrapping_add((affect.urgency * 120.0) as u32),
            brightness: affect.coherence.max(0.15),
        }
    }

    pub fn neutral() -> Self {
        SoulMirrorState {
            color: JARVIS_CYAN,
            prev_color: JARVIS_CYAN,
            avatar_override: None,
            pulse_speed: 0.5,
            size_scale: 0.8,
            ring_count: 3,
            rotation_deg: 0,
            brightness: 0.9,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// PARTICLE SYSTEM — small dots floating around the orb
// ═══════════════════════════════════════════════════════════════
const NUM_PARTICLES: usize = 24;

struct Particle {
    angle: f32,      // radians around orb
    dist: f32,       // distance from center (1.0 = orb surface)
    speed: f32,      // angular speed
    size: u8,        // radius in pixels
    alpha: u8,       // brightness
}

impl Particle {
    const fn new(angle: f32, dist: f32, speed: f32, size: u8, alpha: u8) -> Self {
        Particle { angle, dist, speed, size, alpha }
    }
}

// Pre-computed particle positions (deterministic from seed)
const PARTICLES: [Particle; NUM_PARTICLES] = [
    Particle::new(0.0,   1.3, 0.020, 2, 180),
    Particle::new(0.5,   1.6, 0.015, 1, 140),
    Particle::new(1.0,   1.2, 0.025, 2, 200),
    Particle::new(1.5,   1.8, 0.012, 1, 120),
    Particle::new(2.0,   1.4, 0.018, 3, 160),
    Particle::new(2.5,   1.1, 0.022, 1, 190),
    Particle::new(3.0,   1.7, 0.014, 2, 130),
    Particle::new(3.5,   1.3, 0.019, 1, 170),
    Particle::new(4.0,   1.5, 0.016, 2, 150),
    Particle::new(4.5,   1.2, 0.021, 1, 185),
    Particle::new(5.0,   1.9, 0.011, 2, 110),
    Particle::new(5.5,   1.4, 0.017, 3, 165),
    Particle::new(0.3,   2.0, 0.010, 1, 100),
    Particle::new(1.8,   1.6, 0.013, 2, 145),
    Particle::new(2.8,   1.1, 0.023, 1, 195),
    Particle::new(3.8,   1.8, 0.012, 2, 115),
    Particle::new(4.8,   1.3, 0.020, 1, 175),
    Particle::new(5.8,   1.5, 0.015, 3, 135),
    Particle::new(0.7,   1.7, 0.014, 1, 155),
    Particle::new(1.3,   1.2, 0.021, 2, 180),
    Particle::new(2.3,   1.9, 0.011, 1, 105),
    Particle::new(3.3,   1.4, 0.018, 2, 160),
    Particle::new(4.3,   1.6, 0.016, 1, 140),
    Particle::new(5.3,   1.1, 0.024, 3, 190),
];

pub struct SoulMirrorRenderer {
    pub state: SoulMirrorState,
    pub frame: u64,
    pub fb_w: usize,
    pub fb_h: usize,
    pub cx: isize,
    pub cy: isize,
    pub base_r: f32,
}

impl SoulMirrorRenderer {
    pub fn new(fb_w: usize, fb_h: usize) -> Self {
        let base = (core::cmp::min(fb_w, fb_h) as f32 * 0.18).max(36.0);
        SoulMirrorRenderer {
            state: SoulMirrorState::neutral(),
            frame: 0,
            fb_w, fb_h,
            cx: (fb_w / 2) as isize,
            cy: (fb_h / 2) as isize,
            base_r: base,
        }
    }

    pub fn resize(&mut self, fb_w: usize, fb_h: usize) {
        self.fb_w = fb_w;
        self.fb_h = fb_h;
        self.cx = (fb_w / 2) as isize;
        self.cy = (fb_h / 2) as isize;
        self.base_r = (core::cmp::min(fb_w, fb_h) as f32 * 0.18).max(36.0);
    }

    pub fn update_state(&mut self, state: SoulMirrorState) {
        if self.state.color != state.color {
            let t = (self.frame % 20) as u8;
            let lerped = Self::lerp_color(self.state.prev_color, self.state.color, t);
            self.state.prev_color = lerped;
        }
        self.state.color = state.color;
        self.state.avatar_override = state.avatar_override;
        self.state.pulse_speed = state.pulse_speed;
        self.state.size_scale = state.size_scale;
        self.state.ring_count = state.ring_count;
        self.state.rotation_deg = state.rotation_deg;
        self.state.brightness = state.brightness;
    }

    /// Render JARVIS MCU-style orb with hex grid, glow, particles, rings.
    pub fn render(&mut self, fb: &mut DoubleBuffer, fft_energy: f32) {
        self.frame = self.frame.wrapping_add(1);
        let tick = self.frame;
        let cx = self.cx;
        let cy = self.cy;
        let fw = self.fb_w as isize;
        let fh = self.fb_h as isize;

        // Smooth color lerp over 20 frames
        let lerp_t = (tick % 20) as u8;
        let (cr, cg, cb) = Self::lerp_color(self.state.prev_color, self.state.color, lerp_t);

        // JARVIS palette: use cyan-blue tones regardless of affect color
        let (body_r, body_g, body_b) = self.state.avatar_override.unwrap_or((cr, cg, cb));
        // Shift affect color toward JARVIS cyan
        let jr = ((body_r as u16 * 30 + JARVIS_CYAN.0 as u16 * 70) / 100) as u8;
        let jg = ((body_g as u16 * 30 + JARVIS_CYAN.1 as u16 * 70) / 100) as u8;
        let jb = ((body_b as u16 * 30 + JARVIS_CYAN.2 as u16 * 70) / 100) as u8;

        // Breathing pulse
        let base = self.base_r * self.state.size_scale;
        let breath = sinf(tick as f32 * 0.035 * self.state.pulse_speed);
        let pulse_r = base + breath * base * 0.15;
        // FFT audio boost
        let fft_boost = if fft_energy > 0.01 { fft_energy.min(1.0) * base * 0.3 } else { 0.0 };
        let final_r = pulse_r + fft_boost;

        // ── HEX GRID (subtle background dots) ──
        self.draw_hex_grid(fb, tick, fw, fh);

        // ── OUTER GLOW: large, soft, low alpha ──
        let glow_r = (final_r * 2.8) as isize;
        let glow_r_sq = (glow_r * glow_r) as i64;
        fb.fill_circle_gradient_scanline(cx, cy, glow_r,
            JARVIS_CYAN_DIM.0, JARVIS_CYAN_DIM.1, JARVIS_CYAN_DIM.2,
            glow_r_sq);

        // ── MAIN BODY: radial gradient ──
        let r_sq = (final_r * final_r) as i64;
        fb.fill_circle_gradient_scanline(cx, cy, final_r as isize,
            jr, jg, jb, r_sq);

        // ── INNER RING: bright ring at ~70% radius ──
        let inner_r = (final_r * 0.7) as isize;
        let inner_r_sq = (inner_r * inner_r) as i64;
        fb.fill_circle_gradient_scanline(cx, cy, inner_r,
            (jr / 2).max(20), (jg / 2).max(40), jb,
            inner_r_sq);

        // ── BRIGHT CORE: small, intense ──
        let core_r = (final_r * 0.18).max(4.0) as isize;
        let core_r_sq = (core_r * core_r) as i64;
        fb.fill_circle_gradient_scanline(cx, cy, core_r,
            JARVIS_WHITE.0, JARVIS_WHITE.1, JARVIS_WHITE.2,
            core_r_sq);

        // ── SPECULAR HIGHLIGHT: white dot top-left ──
        let spec_r = (final_r * 0.06).max(2.0) as isize;
        let spec_off = (final_r * 0.25) as isize;
        let spec_r_sq = (spec_r * spec_r) as i64;
        fb.fill_circle_gradient_scanline(cx - spec_off, cy - spec_off, spec_r,
            255, 255, 255, spec_r_sq);

        // ── CONCENTRIC RINGS: JARVIS hologram style ──
        let ring_count = self.state.ring_count.max(2).min(5);
        let rot_rad = (self.state.rotation_deg as f32) * PI / 180.0;
        for i in 0..ring_count {
            let rf = (i as f32 + 1.0) / (ring_count as f32 + 1.0);
            let rr = (final_r * (1.15 + rf * 0.55)) as isize;
            let angle = rot_rad + rf * PI * 0.6;
            let arc_span = 0.9 + rf * 0.3;
            let steps = ((rr as f32 * arc_span * 1.5) as u32).max(12);
            let ring_alpha = (140.0 * self.state.brightness * (1.0 - rf * 0.15)) as u8;
            for s in 0..=steps {
                let t = angle + (s as f32 / steps as f32) * arc_span;
                let px = cx + (cosf(t) * rr as f32) as isize;
                // Flatten Y for perspective (ellipse)
                let py = cy + (sinf(t) * rr as f32 * 0.3) as isize;
                if px >= 0 && py >= 0 && px < fw && py < fh {
                    // Glow around ring point
                    let ring_glow = 4isize;
                    let rgsq = 16i64;
                    fb.fill_circle_gradient_scanline(px, py, ring_glow,
                        JARVIS_BLUE.0, JARVIS_BLUE.1, JARVIS_BLUE.2, rgsq);
                    // Bright dot at ring point
                    fb.fill_circle_gradient_scanline(px, py, 1,
                        jr, jg, jb, 1);
                }
            }
        }

        // ── ROTATING ARCS: 2 bright arcs ──
        let arc_r = (final_r + 4.0) as isize;
        let arc1_steps = ((arc_r as f32 * 0.7) as u32).max(10);
        let arc2_steps = ((arc_r as f32 * 0.45) as u32).max(8);
        // Arc 1: bright cyan
        for i in 0..=arc1_steps {
            let t = rot_rad * 1.3 + (i as f32 / arc1_steps as f32) * 0.65;
            let px = cx + (cosf(t) * arc_r as f32) as isize;
            let py = cy + (sinf(t) * arc_r as f32) as isize;
            if px >= 0 && py >= 0 && px < fw && py < fh {
                fb.fill_circle_gradient_scanline(px, py, 3,
                    JARVIS_CYAN.0, JARVIS_CYAN.1, JARVIS_CYAN.2, 9);
            }
        }
        // Arc 2: dimmer, complementary
        let rot_rad2 = rot_rad * 0.8 + PI * 0.6;
        for i in 0..=arc2_steps {
            let t = rot_rad2 + (i as f32 / arc2_steps as f32) * 0.45;
            let px = cx + (cosf(t) * arc_r as f32) as isize;
            let py = cy + (sinf(t) * arc_r as f32) as isize;
            if px >= 0 && py >= 0 && px < fw && py < fh {
                fb.fill_circle_gradient_scanline(px, py, 2,
                    JARVIS_BLUE.0, JARVIS_BLUE.1, JARVIS_BLUE.2, 4);
            }
        }

        // ── PARTICLES: floating dots around orb ──
        for p in PARTICLES.iter() {
            let angle = p.angle + tick as f32 * p.speed;
            let dist = p.dist * final_r;
            let px = cx + (cosf(angle) * dist) as isize;
            let py = cy + (sinf(angle) * dist * 0.6) as isize; // slight ellipse
            if px >= 0 && py >= 0 && px < fw && py < fh {
                let ps = p.size as isize;
                let ps_sq = (ps * ps) as i64;
                fb.fill_circle_gradient_scanline(px, py, ps,
                    JARVIS_CYAN.0, JARVIS_CYAN.1, JARVIS_CYAN.2, ps_sq);
            }
        }

        // ── SCANLINES: subtle horizontal lines (hologram effect) ──
        let scan_y_start = (cy - final_r as isize).max(0) as usize;
        let scan_y_end = (cy + final_r as isize).min(fh as isize) as usize;
        let scan_x0 = (cx - final_r as isize).max(0) as usize;
        let scan_xw = ((final_r * 2.0) as isize).min(fw) as usize;
        for y in (scan_y_start..scan_y_end).step_by(4) {
            let x_end = (scan_x0 + scan_xw).min(fb.info.width);
            // Draw a thin line across the orb at this scanline
            fb.fill_rect_fast(scan_x0, y, x_end.saturating_sub(scan_x0), 1,
                JARVIS_CYAN_DIM.0, JARVIS_CYAN_DIM.1, JARVIS_CYAN_DIM.2);
        }
    }

    /// Draw subtle hexagonal grid in background — dots at hex intersections.
    fn draw_hex_grid(&self, fb: &mut DoubleBuffer, tick: u64, fw: isize, fh: isize) {
        let spacing = 48isize;
        let hex_h = (spacing as f32 * 0.866) as isize; // √3/2
        let dot_alpha = 8u8; // very subtle
        let pulsing = sinf((tick % 60) as f32 / 60.0 * PI) * 0.3 + 0.7; // gentle pulse

        let mut row = 0isize;
        let mut y = -spacing;
        while y < fh + spacing {
            let offset_x = if row % 2 == 1 { spacing / 2 } else { 0 };
            let mut x = -spacing + offset_x;
            while x < fw + spacing {
                if x >= 0 && y >= 0 && x < fw && y < fh {
                    // Distance from center — dim dots farther away
                    let dx = x - self.cx;
                    let dy = y - self.cy;
                    let d_sq = (dx * dx + dy * dy) as f32;
                    let max_d = (fw * fw + fh * fh) as f32 * 0.25;
                    if d_sq < max_d {
                        let dist_fade = 1.0 - (d_sq / max_d).min(1.0);
                        let a = (dot_alpha as f32 * dist_fade * pulsing) as u8;
                        if a >= 2 {
                            fb.fill_circle_gradient_scanline(x, y, 1,
                                JARVIS_CYAN_DIM.0, JARVIS_CYAN_DIM.1, JARVIS_CYAN_DIM.2, 1);
                        }
                    }
                }
                x += spacing;
            }
            y += hex_h;
            row += 1;
        }
    }

    fn lerp_color(a: (u8,u8,u8), b: (u8,u8,u8), t: u8) -> (u8,u8,u8) {
        let inv = 20 - t as u16;
        let r = (a.0 as u16 * inv + b.0 as u16 * t as u16) / 20;
        let g = (a.1 as u16 * inv + b.1 as u16 * t as u16) / 20;
        let bl = (a.2 as u16 * inv + b.2 as u16 * t as u16) / 20;
        (r as u8, g as u8, bl as u8)
    }
}
