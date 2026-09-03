//! Orb JARVIS (Soul Mirror) — holograma MCU com glow radial, arcs e particles.
//!
//! Hot path TCG: glow ≤2.2r, arcs finos (sem glow 4px/ponto), hex grid rare,
//! 12 particles. Distance² + quadratic falloff (sem sqrtf/expf).

use core::f32::consts::PI;
use libm::{sinf, cosf};
use crate::display::fb::DoubleBuffer;

const JARVIS_CYAN: (u8, u8, u8) = (0, 212, 255);
const JARVIS_CYAN_DIM: (u8, u8, u8) = (0, 80, 120);
const JARVIS_BLUE: (u8, u8, u8) = (0, 100, 200);
const JARVIS_WHITE: (u8, u8, u8) = (200, 240, 255);

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
            pulse_speed: 0.55,
            size_scale: 0.85,
            ring_count: 3,
            rotation_deg: 0,
            brightness: 0.9,
        }
    }
}

const NUM_PARTICLES: usize = 12;

struct Particle {
    angle: f32,
    dist: f32,
    speed: f32,
    size: u8,
}

impl Particle {
    const fn new(angle: f32, dist: f32, speed: f32, size: u8) -> Self {
        Particle { angle, dist, speed, size }
    }
}

const PARTICLES: [Particle; NUM_PARTICLES] = [
    Particle::new(0.0, 1.25, 0.018, 2),
    Particle::new(0.7, 1.55, 0.014, 1),
    Particle::new(1.4, 1.20, 0.022, 2),
    Particle::new(2.1, 1.70, 0.011, 1),
    Particle::new(2.8, 1.35, 0.017, 2),
    Particle::new(3.5, 1.15, 0.020, 1),
    Particle::new(4.2, 1.60, 0.013, 2),
    Particle::new(4.9, 1.30, 0.019, 1),
    Particle::new(5.5, 1.80, 0.010, 2),
    Particle::new(0.4, 1.45, 0.016, 1),
    Particle::new(1.9, 1.10, 0.021, 2),
    Particle::new(3.8, 1.65, 0.012, 1),
];

pub struct SoulMirrorRenderer {
    pub state: SoulMirrorState,
    pub frame: u64,
    pub fb_w: usize,
    pub fb_h: usize,
    pub cx: isize,
    pub cy: isize,
    pub base_r: f32,
    /// Rotação contínua (graus) — independente do Affect snapshot.
    spin_deg: f32,
}

impl SoulMirrorRenderer {
    pub fn new(fb_w: usize, fb_h: usize) -> Self {
        let base = (core::cmp::min(fb_w, fb_h) as f32 * 0.17).max(40.0);
        SoulMirrorRenderer {
            state: SoulMirrorState::neutral(),
            frame: 0,
            fb_w, fb_h,
            cx: (fb_w / 2) as isize,
            cy: (fb_h / 2) as isize,
            base_r: base,
            spin_deg: 0.0,
        }
    }

    pub fn resize(&mut self, fb_w: usize, fb_h: usize) {
        self.fb_w = fb_w;
        self.fb_h = fb_h;
        self.cx = (fb_w / 2) as isize;
        self.cy = (fb_h / 2) as isize;
        self.base_r = (core::cmp::min(fb_w, fb_h) as f32 * 0.17).max(40.0);
    }

    pub fn update_state(&mut self, state: SoulMirrorState) {
        if self.state.color != state.color {
            let t = (self.frame % 24) as u8;
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

    /// Render JARVIS MCU-style orb (versão leve p/ TCG ~18 Hz).
    pub fn render(&mut self, fb: &mut DoubleBuffer, fft_energy: f32) {
        self.frame = self.frame.wrapping_add(1);
        let tick = self.frame;
        let cx = self.cx;
        let cy = self.cy;
        let fw = self.fb_w as isize;
        let fh = self.fb_h as isize;

        // Spin suave: Affect dá offset; frame dá continuidade.
        self.spin_deg = (self.spin_deg + 3.2 * self.state.pulse_speed) % 360.0;
        let rot_rad = (self.spin_deg + self.state.rotation_deg as f32) * PI / 180.0;

        let lerp_t = (tick % 24) as u8;
        let (cr, cg, cb) = Self::lerp_color(self.state.prev_color, self.state.color, lerp_t);
        let (body_r, body_g, body_b) = self.state.avatar_override.unwrap_or((cr, cg, cb));
        let jr = ((body_r as u16 * 25 + JARVIS_CYAN.0 as u16 * 75) / 100) as u8;
        let jg = ((body_g as u16 * 25 + JARVIS_CYAN.1 as u16 * 75) / 100) as u8;
        let jb = ((body_b as u16 * 25 + JARVIS_CYAN.2 as u16 * 75) / 100) as u8;

        // Respiração mais suave (menos amplitude = menos “piscar”).
        let base = self.base_r * self.state.size_scale;
        let breath = sinf(tick as f32 * 0.028 * self.state.pulse_speed);
        let pulse_r = base + breath * base * 0.08;
        let fft_boost = if fft_energy > 0.01 { fft_energy.min(1.0) * base * 0.18 } else { 0.0 };
        let final_r = pulse_r + fft_boost;

        // Hex grid só a cada 4 frames (custo alto, quase estático).
        if tick % 4 == 0 {
            self.draw_hex_grid(fb, tick, fw, fh);
        }

        // Outer glow (2.1× — antes 2.8 matava TCG)
        let glow_r = (final_r * 2.1) as isize;
        let glow_r_sq = (glow_r * glow_r) as i64;
        fb.fill_circle_gradient_scanline(
            cx, cy, glow_r,
            JARVIS_CYAN_DIM.0, JARVIS_CYAN_DIM.1, JARVIS_CYAN_DIM.2,
            glow_r_sq,
        );

        let r_body = final_r as isize;
        let r_sq = (final_r * final_r) as i64;
        fb.fill_circle_gradient_scanline(cx, cy, r_body, jr, jg, jb, r_sq);

        let inner_r = (final_r * 0.68) as isize;
        fb.fill_circle_gradient_scanline(
            cx, cy, inner_r,
            (jr / 2).max(20), (jg / 2).max(40), jb,
            (inner_r * inner_r) as i64,
        );

        let core_r = (final_r * 0.16).max(4.0) as isize;
        fb.fill_circle_gradient_scanline(
            cx, cy, core_r,
            JARVIS_WHITE.0, JARVIS_WHITE.1, JARVIS_WHITE.2,
            (core_r * core_r) as i64,
        );

        let spec_r = (final_r * 0.05).max(2.0) as isize;
        let spec_off = (final_r * 0.22) as isize;
        fb.fill_circle_gradient_scanline(
            cx - spec_off, cy - spec_off, spec_r,
            255, 255, 255,
            (spec_r * spec_r) as i64,
        );

        // Arcs elípticos finos (sem glow 4px por ponto).
        let ring_count = self.state.ring_count.max(2).min(4);
        for i in 0..ring_count {
            let rf = (i as f32 + 1.0) / (ring_count as f32 + 1.0);
            let rr = (final_r * (1.12 + rf * 0.45)) as isize;
            let angle = rot_rad + rf * PI * 0.55;
            let arc_span = 0.85 + rf * 0.25;
            let steps = ((rr as f32 * arc_span * 0.9) as u32).max(10).min(48);
            for s in 0..=steps {
                let t = angle + (s as f32 / steps as f32) * arc_span;
                let px = cx + (cosf(t) * rr as f32) as isize;
                let py = cy + (sinf(t) * rr as f32 * 0.32) as isize;
                if px >= 0 && py >= 0 && px < fw && py < fh {
                    fb.set_pixel(px as usize, py as usize, JARVIS_BLUE.0, JARVIS_BLUE.1, JARVIS_BLUE.2);
                }
            }
        }

        // Arc brilhante principal
        let arc_r = (final_r + 3.0) as isize;
        let arc_steps = ((arc_r as f32 * 0.55) as u32).max(10).min(40);
        for i in 0..=arc_steps {
            let t = rot_rad * 1.25 + (i as f32 / arc_steps as f32) * 0.7;
            let px = cx + (cosf(t) * arc_r as f32) as isize;
            let py = cy + (sinf(t) * arc_r as f32) as isize;
            if px >= 0 && py >= 0 && px < fw && py < fh {
                fb.fill_circle_gradient_scanline(
                    px, py, 2,
                    JARVIS_CYAN.0, JARVIS_CYAN.1, JARVIS_CYAN.2, 4,
                );
            }
        }

        // Particles
        for p in PARTICLES.iter() {
            let angle = p.angle + tick as f32 * p.speed;
            let dist = p.dist * final_r;
            let px = cx + (cosf(angle) * dist) as isize;
            let py = cy + (sinf(angle) * dist * 0.55) as isize;
            if px >= 0 && py >= 0 && px < fw && py < fh {
                let ps = p.size as isize;
                fb.fill_circle_gradient_scanline(
                    px, py, ps,
                    JARVIS_CYAN.0, JARVIS_CYAN.1, JARVIS_CYAN.2,
                    (ps * ps) as i64,
                );
            }
        }

        // Scanlines holográficas (a cada 5 px)
        let scan_y0 = (cy - final_r as isize).max(0) as usize;
        let scan_y1 = (cy + final_r as isize).min(fh) as usize;
        let scan_x0 = (cx - final_r as isize).max(0) as usize;
        let scan_xw = ((final_r * 2.0) as usize).min(fw as usize);
        for y in (scan_y0..scan_y1).step_by(5) {
            let x_end = (scan_x0 + scan_xw).min(fb.info.width);
            fb.fill_rect_fast(
                scan_x0, y, x_end.saturating_sub(scan_x0), 1,
                JARVIS_CYAN_DIM.0 / 2, JARVIS_CYAN_DIM.1 / 2, JARVIS_CYAN_DIM.2 / 2,
            );
        }
    }

    fn draw_hex_grid(&self, fb: &mut DoubleBuffer, tick: u64, fw: isize, fh: isize) {
        let spacing = 56isize;
        let hex_h = (spacing as f32 * 0.866) as isize;
        let pulsing = sinf((tick % 80) as f32 / 80.0 * PI) * 0.25 + 0.75;
        let mut row = 0isize;
        let mut y = -spacing;
        while y < fh + spacing {
            let offset_x = if row % 2 == 1 { spacing / 2 } else { 0 };
            let mut x = -spacing + offset_x;
            while x < fw + spacing {
                if x >= 0 && y >= 0 && x < fw && y < fh {
                    let dx = x - self.cx;
                    let dy = y - self.cy;
                    let d_sq = (dx * dx + dy * dy) as f32;
                    let max_d = (fw * fw + fh * fh) as f32 * 0.22;
                    if d_sq < max_d {
                        let dist_fade = 1.0 - (d_sq / max_d).min(1.0);
                        let a = (10.0 * dist_fade * pulsing) as u8;
                        if a >= 3 {
                            fb.set_pixel(
                                x as usize, y as usize,
                                JARVIS_CYAN_DIM.0, JARVIS_CYAN_DIM.1, JARVIS_CYAN_DIM.2,
                            );
                        }
                    }
                }
                x += spacing;
            }
            y += hex_h;
            row += 1;
        }
    }

    fn lerp_color(a: (u8, u8, u8), b: (u8, u8, u8), t: u8) -> (u8, u8, u8) {
        let inv = 24u16.saturating_sub(t as u16);
        let t = t as u16;
        (
            ((a.0 as u16 * inv + b.0 as u16 * t) / 24) as u8,
            ((a.1 as u16 * inv + b.1 as u16 * t) / 24) as u8,
            ((a.2 as u16 * inv + b.2 as u16 * t) / 24) as u8,
        )
    }
}
