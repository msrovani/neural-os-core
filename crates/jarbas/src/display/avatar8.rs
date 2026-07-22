use core::f32::consts::PI;
use libm::{sinf, cosf, sqrtf};
use crate::display::fb::DoubleBuffer;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Avatar8State {
    Idle,
    Listening,
    Processing,
    Speaking,
    Thinking,
    Dreaming,
    Alert,
    Updating,
}

impl Avatar8State {
    pub fn color(&self) -> (u8, u8, u8) {
        match self {
            Avatar8State::Idle => (0, 150, 255),
            Avatar8State::Listening => (0, 255, 255),
            Avatar8State::Processing => (255, 165, 0),
            Avatar8State::Speaking => (50, 255, 50),
            Avatar8State::Thinking => (180, 60, 255),
            Avatar8State::Dreaming => (75, 0, 130),
            Avatar8State::Alert => (255, 40, 40),
            Avatar8State::Updating => (180, 255, 60),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Avatar8State::Idle => "IDLE",
            Avatar8State::Listening => "LISTEN",
            Avatar8State::Processing => "PROCESS",
            Avatar8State::Speaking => "SPEAK",
            Avatar8State::Thinking => "THINK",
            Avatar8State::Dreaming => "DREAM",
            Avatar8State::Alert => "ALERT",
            Avatar8State::Updating => "UPDATE",
        }
    }
}

#[derive(Clone, Copy)]
pub struct Particle {
    pub x: f32, pub y: f32,
    pub vx: f32, pub vy: f32,
    pub r: u8, pub g: u8, pub b: u8,
    pub size: f32,
    pub life: f32,
}

pub struct Avatar8 {
    pub state: Avatar8State,
    progress: f32,
    particles: [Particle; 96],
    particle_count: usize,
    frame: u64,
    fb_w: usize,
    fb_h: usize,
    cx: f32,
    cy: f32,
}

impl Avatar8 {
    pub fn new(fb_w: usize, fb_h: usize) -> Self {
        Avatar8 {
            state: Avatar8State::Idle,
            progress: 0.0,
            particles: [Particle { x: 0.0, y: 0.0, vx: 0.0, vy: 0.0, r: 0, g: 0, b: 0, size: 0.0, life: 0.0 }; 96],
            particle_count: 0,
            frame: 0,
            fb_w, fb_h,
            cx: fb_w as f32 / 2.0,
            cy: fb_h as f32 / 2.0,
        }
    }

    pub fn set_state(&mut self, new: Avatar8State) {
        if self.state != new {
            self.state = new;
            self.progress = 0.0;
        }
    }

    pub fn render(&mut self, fb: &mut DoubleBuffer) {
        self.frame = self.frame.wrapping_add(1);
        self.progress = (self.progress + 0.025).min(1.0);
        let (cr, cg, cb) = self.state.color();
        let seed = self.frame as u32;
        let cx = self.cx;
        let cy = self.cy;
        let dt = 0.016;

        match self.state {
            Avatar8State::Idle => {
                let drift = sinf(self.progress * PI * 0.3) * 6.0;
                let r = 40.0 + drift;
                self.draw_circle(fb, cx, cy, r, cr, cg, cb, 15);
                self.emit_particles(seed, 1, cx, cy, cr, cg, cb);
            }
            Avatar8State::Listening => {
                let pulse = 30.0 + sinf(self.progress * PI * 4.0) * 8.0;
                self.draw_circle(fb, cx, cy, pulse, cr, cg, cb, 25);
                let ring = pulse + 12.0 + sinf(self.progress * PI * 2.0) * 4.0;
                self.draw_ring(fb, cx, cy, ring, 3.0, cr, cg, cb, 12);
                self.emit_particles(seed, 2, cx, cy, cr, cg, cb);
            }
            Avatar8State::Processing => {
                let angle = self.progress * PI * 6.0;
                let spiral_r = 15.0 + (self.progress * 80.0) % 70.0;
                let sx = cx + cosf(angle) * spiral_r;
                let sy = cy + sinf(angle) * spiral_r;
                self.draw_circle(fb, sx, sy, 12.0, cr, cg, cb, 40);
                for i in 0..3 {
                    let ta = angle + i as f32 * 2.094;
                    let tr = spiral_r * 0.5;
                    let tx = cx + cosf(ta) * tr;
                    let ty = cy + sinf(ta) * tr;
                    self.draw_circle(fb, tx, ty, 5.0, cr, cg, cb, 20);
                }
                self.emit_particles(seed, 3, sx, sy, cr, cg, cb);
            }
            Avatar8State::Speaking => {
                let wave = sinf(self.progress * PI * 8.0) * 12.0;
                self.draw_circle(fb, cx - 18.0, cy + wave, 10.0, cr, cg, cb, 50);
                self.draw_circle(fb, cx + 18.0, cy - wave, 10.0, cr, cg, cb, 50);
                for i in 0..3 {
                    let r = 28.0 + i as f32 * 10.0 + sinf(self.progress * PI * 4.0 + i as f32) * 3.0;
                    self.draw_ring(fb, cx, cy, r, 2.0, cr, cg, cb, 8);
                }
                self.emit_particles(seed, 2, cx, cy, cr, cg, cb);
            }
            Avatar8State::Thinking => {
                let angle = self.progress * PI * 2.0;
                for i in 0..4 {
                    let a = angle + i as f32 * 1.571;
                    let r = 20.0 + sinf(self.progress * PI * 3.0 + i as f32) * 8.0;
                    let sx = cx + cosf(a) * r;
                    let sy = cy + sinf(a) * r;
                    self.draw_circle(fb, sx, sy, 6.0, cr, cg, cb, 30);
                }
                self.emit_particles(seed, 2, cx, cy, cr, cg, cb);
            }
            Avatar8State::Dreaming => {
                let slow = sinf(self.progress * PI * 0.5);
                for i in 0..3 {
                    let a = slow * 2.0 + i as f32 * 2.094;
                    let r = 10.0 + (self.progress * 60.0) % 50.0;
                    let sx = cx + cosf(a) * r;
                    let sy = cy + sinf(a) * r;
                    self.draw_circle(fb, sx, sy, 4.0, cr, cg, cb, 20);
                }
                self.draw_ring(fb, cx, cy, 45.0 + slow * 8.0, 1.0, 100, 50, 180, 10);
                self.emit_particles(seed, 1, cx, cy, cr, cg, cb);
            }
            Avatar8State::Alert => {
                let flash = sinf(self.progress * PI * 10.0).abs();
                let r = 35.0 + flash * 20.0;
                self.draw_circle(fb, cx, cy, r, cr, cg, cb, (flash * 60.0) as u8);
                self.draw_ring(fb, cx, cy, r + 10.0, 3.0, 255, 200, 0, 30);
                self.emit_particles(seed, 4, cx, cy, cr, cg, cb);
            }
            Avatar8State::Updating => {
                let sweep = (self.progress * 360.0) as f32 * PI / 180.0;
                let r = 30.0 + sinf(self.progress * PI * 2.0) * 5.0;
                self.draw_arc(fb, cx, cy, r, sweep, sweep + 1.5, cr, cg, cb, 50);
                self.draw_arc(fb, cx, cy, r + 10.0, sweep + 2.0, sweep + 3.2, cr, cg, cb, 30);
                self.emit_particles(seed, 2, cx, cy, cr, cg, cb);
            }
        }

        self.update_particles(dt);
        for i in 0..self.particle_count {
            let p = &self.particles[i];
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
    }

    fn draw_circle(&self, fb: &mut DoubleBuffer, cx: f32, cy: f32, r: f32, rc: u8, gc: u8, bc: u8, alpha: u8) {
        let ri = r as isize;
        for dy in -ri..=ri {
            for dx in -ri..=ri {
                let dist = sqrtf((dx*dx + dy*dy) as f32);
                if dist <= r {
                    let falloff = (1.0 - dist / r) * (alpha as f32 / 100.0);
                    fb.set_pixel(
                        (cx + dx as f32) as usize,
                        (cy + dy as f32) as usize,
                        (rc as f32 * falloff) as u8,
                        (gc as f32 * falloff) as u8,
                        (bc as f32 * falloff) as u8,
                    );
                }
            }
        }
    }

    fn draw_ring(&self, fb: &mut DoubleBuffer, cx: f32, cy: f32, r: f32, thickness: f32, rc: u8, gc: u8, bc: u8, alpha: u8) {
        let ri = (r + thickness) as isize;
        for dy in -ri..=ri {
            for dx in -ri..=ri {
                let dist = sqrtf((dx*dx + dy*dy) as f32);
                if dist >= r - thickness && dist <= r + thickness {
                    let falloff = (alpha as f32 / 100.0) * (1.0 - (dist - r).abs() / thickness.max(1.0));
                    fb.set_pixel(
                        (cx + dx as f32) as usize,
                        (cy + dy as f32) as usize,
                        (rc as f32 * falloff) as u8,
                        (gc as f32 * falloff) as u8,
                        (bc as f32 * falloff) as u8,
                    );
                }
            }
        }
    }

    fn draw_arc(&self, fb: &mut DoubleBuffer, cx: f32, cy: f32, r: f32, a0: f32, a1: f32, rc: u8, gc: u8, bc: u8, _alpha: u8) {
        let steps = (r.abs() * 2.0) as u32 + 4;
        for i in 0..=steps {
            let t = a0 + (a1 - a0) * (i as f32 / steps as f32);
            let px = cx + cosf(t) * r;
            let py = cy + sinf(t) * r;
            fb.set_pixel(px as usize, py as usize, rc, gc, bc);
            let px2 = cx + cosf(t) * (r + 1.0);
            let py2 = cy + sinf(t) * (r + 1.0);
            fb.set_pixel(px2 as usize, py2 as usize, rc, gc, bc);
        }
    }

    fn emit_particles(&mut self, seed: u32, count: usize, cx: f32, cy: f32, r: u8, g: u8, b: u8) {
        for _ in 0..count {
            if self.particle_count >= 96 { break; }
            let angle = ((seed + self.particle_count as u32) as f32) * 2.4;
            let speed = 0.3 + (seed % 7) as f32 * 0.25;
            let size = 1.0 + (seed % 4) as f32;
            self.particles[self.particle_count] = Particle {
                x: cx + cosf(angle) * 3.0,
                y: cy + sinf(angle) * 3.0,
                vx: cosf(angle) * speed,
                vy: sinf(angle) * speed,
                r, g, b,
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
            if p.x < 0.0 || p.x > self.fb_w as f32 || p.y < 0.0 || p.y > self.fb_h as f32 || p.life <= 0.0 {
                self.particles[i] = self.particles[self.particle_count - 1];
                self.particle_count -= 1;
            } else {
                i += 1;
            }
        }
    }
}
