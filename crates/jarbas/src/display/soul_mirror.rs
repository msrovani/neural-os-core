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
            avatar_override: None,
            pulse_speed: 0.5,
            size_scale: 0.8,
            ring_count: 2,
            rotation_deg: 0,
            brightness: 0.8,
        }
    }
}

pub struct SoulMirrorRenderer {
    pub state: SoulMirrorState,
    frame: u64,
    fb_w: usize,
    fb_h: usize,
    cx: isize,
    cy: isize,
    base_r: f32,
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
    pub fn render(&mut self, fb: &mut DoubleBuffer, fft_energy: f32) {
        self.frame = self.frame.wrapping_add(1);
        let (cr, cg, cb) = self.state.color;
        let tick = self.frame;
        let cx = self.cx;
        let cy = self.cy;

        let r = self.base_r * self.state.size_scale;
        let pulse_offset = sinf(tick as f32 * 0.05 * self.state.pulse_speed) * (r * 0.18);
        let pulse_r = r + pulse_offset;
        let bri = if fft_energy > 0.0 {
            (130.0 + fft_energy.min(1.0) * 125.0) * self.state.brightness
        } else {
            (130.0 + sinf(tick as f32 * 0.03) * 40.0) * self.state.brightness
        };
        let bri_u8 = bri.clamp(30.0, 255.0) as u8;

        // Ambient — 1.35x (antes 2.2x ≈ r=264 e ~280k sqrts/frame)
        let ambient_r = (pulse_r * 1.35) as isize;
        let amb_alpha = (18.0 * self.state.brightness) as u8;
        fb.fill_circle_glow(cx, cy, ambient_r, cr.saturating_sub(40), cg.saturating_sub(40), cb.saturating_sub(40), amb_alpha);

        let ring_count = self.state.ring_count.min(2);
        for i in 0..ring_count {
            let ring_r = (pulse_r * (1.0 + (i + 1) as f32 * 0.22)) as isize;
            let ring_alpha = (18 + 8 * (ring_count as i32 - i as i32)) as u8;
            let fade = (ring_count as f32 - i as f32) / ring_count as f32;
            let rr = (cr as f32 * fade) as u8;
            let gg = (cg as f32 * fade) as u8;
            let bb = (cb as f32 * fade) as u8;
            fb.fill_circle_glow(cx, cy, ring_r, rr, gg, bb, ring_alpha);
        }

        // Corpo: um disco (halo já veio do ambient/rings — glow 1.5x era o 2º maior fill).
        let (body_r, body_g, body_b) = self.state.avatar_override.unwrap_or((cr, cg, cb));
        let core_r = pulse_r as isize;
        fb.fill_circle_glow(cx, cy, core_r, body_r, body_g, body_b, bri_u8 / 3);

        // Hot core
        let hot_r = (pulse_r * 0.28).max(4.0) as isize;
        let hc_r = (255u8).min(body_r.saturating_add(60));
        let hc_g = (255u8).min(body_g.saturating_add(60));
        let hc_b = (255u8).min(body_b.saturating_add(60));
        fb.fill_circle_glow(cx, cy, hot_r, hc_r, hc_g, hc_b, 90);

        // Rotation arc — animated dash from LoopPhase rotation
        let rot_r = (pulse_r + 6.0) as isize;
        let rot_rad = (self.state.rotation_deg as f32) * PI / 180.0;
        let arc_len = pulse_r * 0.3;
        let arc_steps = ((arc_len * 2.0) as u32).clamp(4, 16);
        for i in 0..=arc_steps {
            let t = rot_rad + (i as f32 / arc_steps as f32) * 0.8;
            let px = cx + (cosf(t) * rot_r as f32) as isize;
            let py = cy + (sinf(t) * rot_r as f32) as isize;
            if px >= 0 && py >= 0 && px < self.fb_w as isize && py < self.fb_h as isize {
                fb.set_pixel(px as usize, py as usize, 255, 255, 255);
            }
        }
        // Second arc (complementary)
        let rot_rad2 = rot_rad + PI;
        for i in 0..=arc_steps {
            let t = rot_rad2 + (i as f32 / arc_steps as f32) * 0.8;
            let px = cx + (cosf(t) * rot_r as f32) as isize;
            let py = cy + (sinf(t) * rot_r as f32) as isize;
            if px >= 0 && py >= 0 && px < self.fb_w as isize && py < self.fb_h as isize {
                fb.set_pixel(px as usize, py as usize, hc_r, hc_g, hc_b);
            }
        }
    }
}
