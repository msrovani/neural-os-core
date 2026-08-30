//! Orb afetivo (Soul Mirror) — discos scanline, sem sqrtf por pixel.
use core::f32::consts::PI;
use libm::{sinf, cosf};
use crate::display::fb::DoubleBuffer;

/// Visual state derived from AffectVector + LoopPhase.
/// Color from valence, pulse from arousal, size from dominance,
/// rings from curiosity, rotation from LoopPhase, brightness from coherence.
#[derive(Clone, Copy)]
pub struct SoulMirrorState {
    pub color: (u8, u8, u8),
    pub prev_color: (u8, u8, u8),
    pub pulse_speed: f32,
    pub size_scale: f32,
    pub ring_count: u32,
    pub rotation_deg: u32,
    pub brightness: f32,
    pub avatar_override: Option<(u8, u8, u8)>,  // Jarbas palette override
}

impl SoulMirrorState {
    /// Jarbas palette: override color based on avatar state.
    /// Returns Some(color) if the state has a dedicated Jarbas color.
    pub fn jarbas_color(state: &str) -> Option<(u8, u8, u8)> {
        match state {
            "DREAM" => Some((75, 0, 130)),    // Indigo — SleepCycle
            "THINK" => Some((180, 60, 255)),  // Purple — inferência
            "IDLE"  => Some((106, 13, 173)),  // Deep purple — Jarbas identity
            "ALERT" => Some((224, 17, 95)),   // Ruby — emergência
            _ => None,                         // Estados emocionais mantêm valence
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
            color: (0, 150, 255),
            prev_color: (0, 150, 255),
            avatar_override: None,
            pulse_speed: 0.5,
            size_scale: 0.8,
            ring_count: 2,
            rotation_deg: 0,
            brightness: 0.8,
        }
    }

    /// Linear interpolation between two colors (0..16 alpha = 1/16 steps)
    fn lerp_color(a: (u8,u8,u8), b: (u8,u8,u8), t: u8) -> (u8,u8,u8) {
        let inv = 16 - t as u16;
        let r = (a.0 as u16 * inv + b.0 as u16 * t as u16) / 16;
        let g = (a.1 as u16 * inv + b.1 as u16 * t as u16) / 16;
        let bl = (a.2 as u16 * inv + b.2 as u16 * t as u16) / 16;
        (r as u8, g as u8, bl as u8)
    }
}

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
        let base = (core::cmp::min(fb_w, fb_h) as f32 * 0.15).max(28.0);
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
        self.base_r = (core::cmp::min(fb_w, fb_h) as f32 * 0.15).max(28.0);
    }

    pub fn update_state(&mut self, state: SoulMirrorState) {
        self.state = state;
    }

    /// Renders the soul mirror orb on the framebuffer.
    /// Layers: ambient → rings → body → hot core → rotation arc → particles.
    /// Renders the soul mirror orb on the framebuffer -- JARVIS style.
    /// Gradient radial + aneis orbitais + pulse visivel + breathing.
    pub fn render(&mut self, fb: &mut DoubleBuffer, fft_energy: f32) {
        self.frame = self.frame.wrapping_add(1);
        let tick = self.frame;
        let cx = self.cx;
        let cy = self.cy;

        // Smooth color transition
        let lerp_t = (tick % 30) as u8;
        let (cr, cg, cb) = SoulMirrorState::lerp_color(self.state.prev_color, self.state.color, lerp_t);

        let base = self.base_r * self.state.size_scale;
        // Breathing pulse
        let breath = sinf(tick as f32 * 0.04 * self.state.pulse_speed);
        let pulse_r = base + breath * base * 0.25;
        // Audio reactivity
        let fft_boost = if fft_energy > 0.01 { fft_energy.min(1.0) * base * 0.3 } else { 0.0 };
        let final_r = pulse_r + fft_boost;

        // Outer glow (halo largo, suave)
        let glow_r = (final_r * 2.2) as isize;
        let gr = cr.saturating_sub(50);
        let gg = cg.saturating_sub(50);
        let gb = cb.saturating_sub(50);
        fb.fill_circle_gradient(cx, cy, glow_r, gr, gg, gb, 15, 0.04);

        // Main body (gradiente radial)
        let (body_r, body_g, body_b) = self.state.avatar_override.unwrap_or((cr, cg, cb));
        let body_radius = final_r as isize;
        fb.fill_circle_gradient(cx, cy, body_radius, body_r, body_g, body_b, 35, 0.06);

        // Bright core
        let core_r = (final_r * 0.3).max(4.0) as isize;
        let hc_r = 255u8.min(body_r.saturating_add(80));
        let hc_g = 255u8.min(body_g.saturating_add(80));
        let hc_b = 255u8.min(body_b.saturating_add(80));
        fb.fill_circle_gradient(cx, cy, core_r, hc_r, hc_g, hc_b, 50, 0.08);

        // Specular highlight
        let spec_r = (final_r * 0.12).max(2.0) as isize;
        let spec_off = (final_r * 0.35) as isize;
        fb.fill_circle_gradient(cx - spec_off, cy - spec_off, spec_r, 255, 255, 255, 60, 0.1);

        // Orbital rings
        let ring_count = self.state.ring_count.max(1).min(4);
        let rot_rad = (self.state.rotation_deg as f32) * PI / 180.0;
        for i in 0..ring_count {
            let rf = (i as f32 + 1.0) / (ring_count as f32 + 1.0);
            let ring_r = (final_r * (1.3 + rf * 0.8)) as isize;
            let thickness = ((2.0 + rf * 1.5) * self.state.brightness) as isize;
            let angle = rot_rad + rf * PI * 0.7;
            let arc_len = 1.2 + rf * 0.5;
            let steps = ((ring_r as f32 * arc_len * 2.0) as u32).max(4);
            for s in 0..=steps {
                let t = angle + (s as f32 / steps as f32) * arc_len;
                let px = cx + (cosf(t) * ring_r as f32) as isize;
                let py = cy + (sinf(t) * ring_r as f32 * 0.4) as isize;
                if px >= 0 && py >= 0 && px < self.fb_w as isize && py < self.fb_h as isize {
                    fb.fill_circle_gradient(px, py, thickness, body_r, body_g, body_b, 40, 0.1);
                }
            }
        }

        // Animated arcs
        let arc_r = (final_r + 4.0) as isize;
        let arc1_steps = ((arc_r as f32) as u32).max(6);
        let arc2_steps = ((arc_r as f32 * 0.7) as u32).max(4);
        for i in 0..=arc1_steps {
            let t = rot_rad + (i as f32 / arc1_steps as f32) * 0.9;
            let px = cx + (cosf(t) * arc_r as f32) as isize;
            let py = cy + (sinf(t) * arc_r as f32) as isize;
            if px >= 0 && py >= 0 && px < self.fb_w as isize && py < self.fb_h as isize {
                fb.fill_circle_gradient(px, py, 2, 255, 255, 255, 70, 0.15);
            }
        }
        let rot_rad2 = rot_rad + PI * 0.6;
        for i in 0..=arc2_steps {
            let t = rot_rad2 + (i as f32 / arc2_steps as f32) * 0.6;
            let px = cx + (cosf(t) * arc_r as f32) as isize;
            let py = cy + (sinf(t) * arc_r as f32) as isize;
            if px >= 0 && py >= 0 && px < self.fb_w as isize && py < self.fb_h as isize {
                fb.fill_circle_gradient(px, py, 2, hc_r, hc_g, hc_b, 60, 0.12);
            }
        }
    }
}
