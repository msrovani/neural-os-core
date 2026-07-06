//! JARVIS Avatar — partículas + estados animados no framebuffer.
//! Port do JARVIS .NET MAUI (github.com/msrovani/jarvis) para bare-metal Rust.
//! Estados: Idle, Listening, Processing, Speaking.
//! Renderiza sobre o framebuffer como overlay do Hermes Chat.

use core::f32::consts::PI;
use libm::{sinf, cosf, sqrtf};
use crate::display::fb::{DoubleBuffer, GpuDevice};
use crate::serial_println;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AvatarState { Idle, Listening, Processing, Speaking }

#[derive(Clone, Copy)]
pub struct Particle {
    pub x: f32, pub y: f32,
    pub vx: f32, pub vy: f32,
    pub r: u8, pub g: u8, pub b: u8, pub a: u8,
    pub size: f32,
    pub life: f32,
}

pub struct JarvisAvatar {
    pub state: AvatarState,
    progress: f32,       // 0.0..1.0 animation progress
    particles: [Particle; 64],
    particle_count: usize,
    frame: u64,
    fb_w: usize,
    fb_h: usize,
    fb_stride: usize,
    fb_bpp: usize,
}

impl JarvisAvatar {
    pub fn new(gpu: &GpuDevice) -> Self {
        JarvisAvatar {
            state: AvatarState::Idle,
            progress: 0.0,
            particles: [Particle { x: 0.0, y: 0.0, vx: 0.0, vy: 0.0, r: 0, g: 0, b: 0, a: 0, size: 0.0, life: 0.0 }; 64],
            particle_count: 0,
            frame: 0,
            fb_w: gpu.fb_width as usize,
            fb_h: gpu.fb_height as usize,
            fb_stride: gpu.fb_stride as usize,
            fb_bpp: gpu.fb_bpp as usize,
        }
    }

    pub fn set_state(&mut self, new: AvatarState) {
        if self.state != new {
            self.state = new;
            self.progress = 0.0;
        }
    }

    /// Atualiza partículas e renderiza no framebuffer
    pub fn render(&mut self, fb: &mut DoubleBuffer) {
        self.frame = self.frame.wrapping_add(1);
        self.progress = (self.progress + 0.02).min(1.0);

        let cx = self.fb_w as f32 / 2.0;
        let cy = self.fb_h as f32 / 2.0;
        let rng_seed = self.frame as u32;

        // Tempo delta para física
        let dt = 0.016;

        match self.state {
            AvatarState::Idle => {
                // Pulso lento + partículas orbitando
                let pulse = 30.0 + sinf(self.progress * PI * 2.0) * 5.0;
                self.draw_circle(fb, cx, cy, pulse, 0, 150, 255, 30);
                self.emit_particles(rng_seed, 1, cx, cy, 0, 150, 255);
            }
            AvatarState::Listening => {
                // Ciano pulsante + partículas espiralando
                let pulse = 40.0 + sinf(self.progress * PI * 4.0) * 10.0;
                self.draw_circle(fb, cx, cy, pulse, 0, 255, 255, 40);
                self.emit_particles(rng_seed, 2, cx, cy, 0, 255, 255);
            }
            AvatarState::Processing => {
                // Laranja girando + espiral
                let angle = self.progress * PI * 6.0;
                let spiral_r = 20.0 + (self.progress * 100.0) % 80.0;
                let sx = cx + cosf(angle) * spiral_r;
                let sy = cy + sinf(angle) * spiral_r;
                self.draw_circle(fb, sx, sy, 15.0, 255, 165, 0, 50);
                self.draw_circle(fb, cx, cy, 10.0, 255, 165, 0, 80);
                self.emit_particles(rng_seed, 3, sx, sy, 255, 165, 0);
            }
            AvatarState::Speaking => {
                // Verde + ondas
                let wave = sinf(self.progress * PI * 8.0) * 15.0;
                self.draw_circle(fb, cx - 20.0, cy + wave, 12.0, 50, 255, 50, 60);
                self.draw_circle(fb, cx + 20.0, cy - wave, 12.0, 50, 255, 50, 60);
                self.draw_circle(fb, cx, cy, 18.0, 50, 255, 50, 40);
                self.emit_particles(rng_seed, 2, cx, cy, 50, 255, 50);
            }
        }

        // Atualiza e desenha partículas
        self.update_particles(dt);
        for i in 0..self.particle_count {
            let p = &self.particles[i];
            let alpha = (p.a as f32 * p.life / 100.0) as u8;
            let size = p.size.max(1.0) as usize;
            for dy in 0..size {
                for dx in 0..size {
                    fb.set_pixel(
                        (p.x + dx as f32) as usize,
                        (p.y + dy as f32) as usize,
                        p.r, p.g, p.b,
                    );
                }
            }
        }

        // Brilho central
        self.draw_circle(fb, cx, cy, 4.0, 255, 255, 255, 60);
    }

    fn draw_circle(&self, fb: &mut DoubleBuffer, cx: f32, cy: f32, r: f32, r_color: u8, g: u8, b: u8, alpha: u8) {
        let ri = r as isize;
        for dy in -ri..=ri {
            for dx in -ri..=ri {
                let dist = sqrtf((dx*dx + dy*dy) as f32);
                if dist <= r {
                    let falloff = (1.0 - dist / r) * (alpha as f32 / 100.0);
                    let rr = (r_color as f32 * falloff) as u8;
                    let gg = (g as f32 * falloff) as u8;
                    let bb = (b as f32 * falloff) as u8;
                    fb.set_pixel((cx + dx as f32) as usize, (cy + dy as f32) as usize, rr, gg, bb);
                }
            }
        }
    }

    fn emit_particles(&mut self, seed: u32, count: usize, cx: f32, cy: f32, r: u8, g: u8, b: u8) {
        for _ in 0..count {
            if self.particle_count >= 64 { break; }
            let angle = ((seed + self.particle_count as u32) as f32) * 2.4;
            let speed = 0.5 + (seed % 5) as f32 * 0.3;
            let size = 1.0 + (seed % 3) as f32;
            self.particles[self.particle_count] = Particle {
                x: cx + cosf(angle) * 5.0,
                y: cy + sinf(angle) * 5.0,
                vx: cosf(angle) * speed,
                vy: sinf(angle) * speed,
                r, g, b, a: 200,
                size, life: 1.0,
            };
            self.particle_count += 1;
        }
    }

    fn update_particles(&mut self, dt: f32) {
        let mut i = 0;
        while i < self.particle_count {
            let p = &mut self.particles[i];
            p.x += p.vx * dt * 60.0;
            p.y += p.vy * dt * 60.0;
            p.life -= dt * 0.5;
            // Expulsa se saiu da tela ou morreu
            if p.x < 0.0 || p.x > self.fb_w as f32 || p.y < 0.0 || p.y > self.fb_h as f32 || p.life <= 0.0 {
                self.particles[i] = self.particles[self.particle_count - 1];
                self.particle_count -= 1;
            } else {
                i += 1;
            }
        }
    }
}
