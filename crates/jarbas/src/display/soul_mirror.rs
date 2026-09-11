//! Orb JARVIS (Soul Mirror) — holograma MCU dirigido por relógio de parede.
//!
//! Contrato visual:
//!   - corpo SEMPRE família ciano (tint por estado, sem misturar hue de estado);
//!   - acentos (halo/rim/ticks/anéis/ripples) por `OrbState`;
//!   - hot path sem `/` por pixel, sem `sqrtf`, sem `sinf/cosf` — `SIN_LUT`
//!     para ângulo e spans/bands (LUT de meias-larguras) para preenchimento.
//!
//! Camadas (trás → frente): lattice hex (cache) → halo (bandas de linha) →
//! metade traseira dos anéis → corpo/inner/core/specular (spans planos) →
//! bandas de scan → rim + anéis dianteiros → ticks radiais → sparks → ripple.
//! LOD adaptativo pelo custo EWMA do frame (`compositor::frame_cost_us`).

use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;
use crate::display::fb::{
    build_half_width_lut, cos_q8, sin_q8, DoubleBuffer,
};

// ── Paleta: corpo ciano, acentos por estado ────────────────────────────
pub const BODY_INNER: (u8, u8, u8) = (0, 86, 150);
pub const CORE: (u8, u8, u8) = (200, 240, 255);
pub const SPARK: (u8, u8, u8) = (0, 212, 255);

/// Pixels aproximados pintados no último paint do orb (diagnóstico/bench).
pub static ORB_PIXELS: AtomicU32 = AtomicU32::new(0);
/// LOD corrente do orb (2=full, 1=médio, 0=mínimo) — telemetria Hub Health.
pub static ORB_LOD: AtomicU32 = AtomicU32::new(2);

/// LOD corrente do orb (0..=2).
pub fn orb_lod() -> u8 {
    ORB_LOD.load(Ordering::Relaxed) as u8
}

/// Estado visual do orb — dirigido por sinais REAIS (voz/cognição/saúde).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OrbState {
    Idle,
    Listening,
    Thinking,
    Speaking,
    Dreaming,
    Degraded,
    Alert,
    Updating,
}

impl OrbState {
    /// Acento (halo/rim/ticks/anéis/ripples). Nunca entra no corpo.
    pub fn accent(&self) -> (u8, u8, u8) {
        match self {
            OrbState::Idle => (0, 212, 255),      // ciano base
            OrbState::Listening => (0, 255, 255), // ciano claro
            OrbState::Thinking => (150, 90, 255), // violeta
            OrbState::Speaking => (0, 230, 255),  // ciano elétrico
            OrbState::Dreaming => (70, 60, 170),  // violeta profundo
            OrbState::Degraded => (255, 176, 32), // âmbar
            OrbState::Alert => (255, 48, 56),     // vermelho
            OrbState::Updating => (64, 255, 160), // verde
        }
    }

    /// Tint do corpo — sempre família ciano (B ≥ G > 0).
    pub fn body_tint(&self) -> (u8, u8, u8) {
        match self {
            OrbState::Idle => (0, 150, 220),
            OrbState::Listening => (0, 175, 240),
            OrbState::Thinking => (0, 145, 225),
            OrbState::Speaking => (0, 200, 245),
            OrbState::Dreaming => (0, 110, 190),
            OrbState::Degraded => (0, 160, 215),
            OrbState::Alert => (0, 150, 220),
            OrbState::Updating => (0, 170, 225),
        }
    }

    /// Alpha máximo do halo (acento) — ALERT pulsa mais fundo.
    pub fn halo_strength(&self) -> u8 {
        match self {
            OrbState::Idle => 34,
            OrbState::Listening => 48,
            OrbState::Thinking => 62,
            OrbState::Speaking => 78,
            OrbState::Dreaming => 36,
            OrbState::Degraded => 56,
            OrbState::Alert => 96,
            OrbState::Updating => 60,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            OrbState::Idle => "IDLE",
            OrbState::Listening => "LISTEN",
            OrbState::Thinking => "THINK",
            OrbState::Speaking => "SPEAK",
            OrbState::Dreaming => "DREAM",
            OrbState::Degraded => "DEGRADED",
            OrbState::Alert => "ALERT",
            OrbState::Updating => "UPDATE",
        }
    }
}

/// Sinais reais que o DisplayAgent publica a cada tick (booleanos com janela
/// temporal no agente; aqui já chegam "acesos" ou não).
#[derive(Clone, Copy, Default)]
pub struct OrbSignals {
    pub listening: bool,
    pub thinking: bool,
    pub speaking: bool,
    pub alert: bool,
    pub degraded: bool,
    pub dreaming: bool,
    pub updating: bool,
    /// Peers de mesh visíveis (≤8 desenhados).
    pub peers: u8,
    /// Contador cumulativo de eventos cognitivos (tokens LLM/TTS) — acende ticks.
    pub activity: u32,
}

/// Máquina de estado com dwell ≥400 ms; ALERT preempta; SPEAKING tem cauda 1.5 s.
pub struct OrbMachine {
    pub state: OrbState,
    state_since_us: u64,
    speaking_until_us: u64,
    listening_until_us: u64,
    inited: bool,
}

impl OrbMachine {
    const DWELL_US: u64 = 400_000;
    const SPEAK_TAIL_US: u64 = 1_500_000;
    const LISTEN_WINDOW_US: u64 = 6_000_000;

    pub fn new() -> Self {
        OrbMachine {
            state: OrbState::Idle,
            state_since_us: 0,
            speaking_until_us: 0,
            listening_until_us: 0,
            inited: false,
        }
    }

    /// Atualiza janelas/estado. `now_us` = relógio de parede (TSC), 0 = indisponível.
    pub fn update(&mut self, sig: &OrbSignals, now_us: u64) -> OrbState {
        // Dwell conta a partir da primeira observação (não do boot).
        if !self.inited {
            self.inited = true;
            self.state_since_us = now_us;
        }
        if sig.listening {
            self.listening_until_us = window_until(now_us, Self::LISTEN_WINDOW_US);
        }
        if sig.speaking {
            self.speaking_until_us = window_until(now_us, Self::SPEAK_TAIL_US);
        }
        let listening = sig.listening || now_active(now_us, self.listening_until_us);
        let speaking = sig.speaking || now_active(now_us, self.speaking_until_us);

        let candidate = if sig.alert {
            OrbState::Alert
        } else if sig.updating {
            OrbState::Updating
        } else if sig.dreaming {
            OrbState::Dreaming
        } else if sig.thinking {
            OrbState::Thinking
        } else if speaking {
            OrbState::Speaking
        } else if listening {
            OrbState::Listening
        } else if sig.degraded {
            OrbState::Degraded
        } else {
            OrbState::Idle
        };

        // ALERT preempta o dwell; resto troca após ≥400 ms no estado atual.
        if candidate == OrbState::Alert && self.state != OrbState::Alert {
            self.state = OrbState::Alert;
            self.state_since_us = now_us;
        } else if candidate != self.state
            && now_us.saturating_sub(self.state_since_us) >= Self::DWELL_US
        {
            self.state = candidate;
            self.state_since_us = now_us;
        }
        self.state
    }
}

/// Janela "até quando": com `now==0` (sem TSC) vira flag ligada (u64::MAX).
fn window_until(now_us: u64, dur_us: u64) -> u64 {
    if now_us == 0 { u64::MAX } else { now_us.saturating_add(dur_us) }
}
fn now_active(now_us: u64, until_us: u64) -> bool {
    if now_us == 0 { until_us == u64::MAX } else { now_us < until_us }
}

// ── §3.2 (ADR-0090 Tier 1): grid pre-render ─────────────────────────────
// Dot geometry depends only on (cx, cy, fw, fh) — tick only gates the dim
// pulse visibility. Positions + distance fade are computed once per resize
// into GRID_BUF; per frame blit_grid() walks the list with unchecked writes.
const GRID_SPACING: isize = 56;
// ponytail: fixed cap 4096 dots (~1080p needs ~260); beyond it extra dots drop.
const GRID_MAX_PTS: usize = 4096;

#[derive(Clone, Copy)]
struct GridDot {
    x: u16,
    y: u16,
    /// Distance fade 0.0–1.0 (`1 - (d²/max_d).min(1)`).
    fade: f32,
}

struct GridCache {
    dots: [GridDot; GRID_MAX_PTS],
    len: usize,
    cx: isize,
    cy: isize,
    fw: isize,
    fh: isize,
}

static GRID_BUF: Mutex<GridCache> = Mutex::new(GridCache {
    dots: [GridDot { x: 0, y: 0, fade: 0.0 }; GRID_MAX_PTS],
    len: 0,
    cx: 0,
    cy: 0,
    fw: 0,
    fh: 0,
});

/// Render grid dots once per geometry into GRID_BUF. Same lattice + radius
/// gate as the legacy per-frame loop; no-op when geometry is unchanged.
pub fn init_grid_buffer(cx: isize, cy: isize, fw: isize, fh: isize) {
    {
        let c = GRID_BUF.lock();
        if c.len > 0 && c.cx == cx && c.cy == cy && c.fw == fw && c.fh == fh {
            return;
        }
    }
    let spacing = GRID_SPACING;
    let hex_h = (spacing as f32 * 0.866) as isize;
    let max_d = (fw * fw + fh * fh) as f32 * 0.22;
    let mut cache = GRID_BUF.lock();
    cache.len = 0;
    cache.cx = cx;
    cache.cy = cy;
    cache.fw = fw;
    cache.fh = fh;
    let mut row = 0isize;
    let mut y = -spacing;
    while y < fh + spacing {
        let offset_x = if row % 2 == 1 { spacing / 2 } else { 0 };
        let mut x = -spacing + offset_x;
        while x < fw + spacing {
            if x >= 0 && y >= 0 && x < fw && y < fh {
                let dx = x - cx;
                let dy = y - cy;
                let d_sq = (dx * dx + dy * dy) as f32;
                if d_sq < max_d {
                    if cache.len < GRID_MAX_PTS {
                        let dist_fade = 1.0 - (d_sq / max_d).min(1.0);
                        let n = cache.len;
                        cache.dots[n] = GridDot { x: x as u16, y: y as u16, fade: dist_fade };
                        cache.len = n + 1;
                    }
                }
            }
            x += spacing;
        }
        y += hex_h;
        row += 1;
    }
}

/// Blit cached grid dots. `pulse_phase` (0..255) vem do relógio de parede;
/// `stride` pula dots em LOD baixo (nunca pula o paint — sem blink).
/// Retorna os dots desenhados.
pub fn blit_grid(fb: &mut DoubleBuffer, pulse_phase: u8, stride: usize) -> usize {
    let cache = GRID_BUF.lock();
    if cache.len == 0 {
        return 0;
    }
    let pulsing = 0.75 + (sin_q8(pulse_phase) as f32 / 127.0) * 0.25;
    let (dw, dh) = (fb.info.width, fb.info.height);
    let step = stride.max(1);
    let mut drawn = 0usize;
    let mut i = 0usize;
    while i < cache.len {
        let d = &cache.dots[i];
        let a = (10.0 * d.fade * pulsing) as u8;
        if a >= 3 {
            let (x, y) = (d.x as usize, d.y as usize);
            if x < dw && y < dh {
                fb.set_pixel_unchecked(x, y, 0, 80, 120); // JARVIS_CYAN_DIM
                drawn += 1;
            }
        }
        i += step;
    }
    drawn
}

// ── Partículas (sparks) — posição dirigida por tempo, sem trig ──────────
const NUM_PARTICLES: usize = 12;

struct Particle {
    phase8: u8,
    /// Distância em q8 do raio do corpo (256 = 1.0·r).
    dist_q8: u16,
    /// Velocidade em q8 de volta/segundo.
    speed_q8: u16,
    size: u8,
}

const PARTICLES: [Particle; NUM_PARTICLES] = [
    Particle { phase8: 0,   dist_q8: 320, speed_q8: 11, size: 2 },
    Particle { phase8: 89,  dist_q8: 397, speed_q8: 9,  size: 1 },
    Particle { phase8: 179, dist_q8: 307, speed_q8: 14, size: 2 },
    Particle { phase8: 26,  dist_q8: 435, speed_q8: 7,  size: 1 },
    Particle { phase8: 115, dist_q8: 346, speed_q8: 11, size: 2 },
    Particle { phase8: 204, dist_q8: 294, speed_q8: 13, size: 1 },
    Particle { phase8: 38,  dist_q8: 410, speed_q8: 8,  size: 2 },
    Particle { phase8: 128, dist_q8: 333, speed_q8: 12, size: 1 },
    Particle { phase8: 217, dist_q8: 461, speed_q8: 6,  size: 2 },
    Particle { phase8: 13,  dist_q8: 371, speed_q8: 10, size: 1 },
    Particle { phase8: 102, dist_q8: 282, speed_q8: 13, size: 2 },
    Particle { phase8: 192, dist_q8: 422, speed_q8: 8,  size: 1 },
];

const TICK_COUNT: usize = 24;
/// LUT de meias-larguras: cobre halo até 4K (raio ≤ 1023). `paint` clampa `r`
/// para `ORB_LUT_R/2.1` — em telas gigantes o glow satura, nunca some.
const ORB_LUT_MAX: usize = 2048;
const ORB_LUT_R: usize = (ORB_LUT_MAX - 1) / 2;

/// Convergência exponencial por canal: `c += (t-c)·k/256` (k≈51 ≈ 0.2),
/// arredondando e garantindo passo ≥1 — converge EXATO sem stall de trunc.
#[inline]
fn converge(c: (u8, u8, u8), t: (u8, u8, u8), k: u32) -> (u8, u8, u8) {
    let f = |a: u8, b: u8| -> u8 {
        let d = b as i32 - a as i32;
        if d == 0 {
            return a;
        }
        let step = (((d.abs() * k as i32) + 128) >> 8).max(1) * d.signum();
        (a as i32 + step).clamp(0, 255) as u8
    };
    (f(c.0, t.0), f(c.1, t.1), f(c.2, t.2))
}

#[inline]
fn scale_color(c: (u8, u8, u8), bright: i32) -> (u8, u8, u8) {
    let b = bright.clamp(0, 255) as u32;
    (
        ((c.0 as u32 * b) >> 8) as u8,
        ((c.1 as u32 * b) >> 8) as u8,
        ((c.2 as u32 * b) >> 8) as u8,
    )
}

pub struct SoulMirrorRenderer {
    pub machine: OrbMachine,
    pub fb_w: usize,
    pub fb_h: usize,
    pub cx: isize,
    pub cy: isize,
    pub base_r: f32,
    // Affect (movimento): escala/respiração/energia.
    size_scale: f32,
    pulse_speed: f32,
    ring_count: u32,
    rotation_deg: u32,
    // Relógio de parede + fase acumulada.
    now_us: u64,
    anim_us: u64,
    spin_deg256: u32,
    // Cores correntes (convergem para o alvo; sem serrilhado).
    body: (u8, u8, u8),
    accent: (u8, u8, u8),
    // LUT de meias-larguras (rebuild só quando o raio inteiro muda).
    half_lut: [u16; ORB_LUT_MAX],
    lut_r: isize,
    // LOD adaptativo (custo EWMA → 2/1/0) com dwell de 32 paints.
    lod: u8,
    lod_votes: u8,
    // Atividade cognitiva → ticks/ripples.
    activity_seen: u32,
    tick_lit: [u8; TICK_COUNT],
    flash_head: usize,
    ripple_start_us: u64,
}

impl SoulMirrorRenderer {
    pub fn new(fb_w: usize, fb_h: usize) -> Self {
        let base = (core::cmp::min(fb_w, fb_h) as f32 * 0.17).max(40.0);
        SoulMirrorRenderer {
            machine: OrbMachine::new(),
            fb_w,
            fb_h,
            cx: (fb_w / 2) as isize,
            cy: (fb_h / 2) as isize,
            base_r: base,
            size_scale: 0.85,
            pulse_speed: 0.55,
            ring_count: 2,
            rotation_deg: 0,
            now_us: 0,
            anim_us: 0,
            spin_deg256: 0,
            body: OrbState::Idle.body_tint(),
            accent: OrbState::Idle.accent(),
            half_lut: [0u16; ORB_LUT_MAX],
            lut_r: -1,
            lod: 2,
            lod_votes: 0,
            activity_seen: 0,
            tick_lit: [0u8; TICK_COUNT],
            flash_head: 0,
            ripple_start_us: 0,
        }
    }

    pub fn resize(&mut self, fb_w: usize, fb_h: usize) {
        self.fb_w = fb_w;
        self.fb_h = fb_h;
        self.cx = (fb_w / 2) as isize;
        self.cy = (fb_h / 2) as isize;
        self.base_r = (core::cmp::min(fb_w, fb_h) as f32 * 0.17).max(40.0);
        self.lut_r = -1;
    }

    /// Affect do supervisor: só parâmetros de movimento (cores = OrbState).
    pub fn set_affect(&mut self, affect: &hermes::affect::AffectVector, phase_deg: u32) {
        self.pulse_speed = 0.5 + affect.arousal * 0.5;
        self.size_scale = 0.5 + affect.dominance * 0.5;
        self.ring_count = (2 + (affect.curiosity * 2.0) as u32).min(3);
        self.rotation_deg = phase_deg.wrapping_add((affect.urgency * 120.0) as u32);
    }

    /// Raio da bbox que o compositor precisa limpar/swapar (2.9R + margem).
    pub fn bounds_radius(&self) -> f32 {
        self.base_r * self.size_scale * 2.9 + 16.0
    }

    /// LUT de meias-larguras para o raio inteiro (cacheado).
    fn ensure_lut(&mut self, radius: usize) -> &[u16] {
        let r = radius.min(ORB_LUT_R);
        if r != self.lut_r as usize {
            build_half_width_lut(r, &mut self.half_lut);
            self.lut_r = r as isize;
        }
        &self.half_lut[..r * 2 + 1]
    }

    fn update_lod(&mut self) {
        let cost = crate::display::compositor::frame_cost_us();
        let want = if cost == 0 || cost < 4_500 { 2 } else if cost < 12_000 { 1 } else { 0 };
        if want == self.lod {
            self.lod_votes = 0;
            return;
        }
        self.lod_votes = self.lod_votes.saturating_add(1);
        if self.lod_votes >= 32 {
            self.lod = want;
            self.lod_votes = 0;
            ORB_LOD.store(want as u32, Ordering::Relaxed);
        }
    }

    /// Pinta o orb inteiro. `now_us` = relógio de parede (TSC); 0 = fallback
    /// de período nominal (30 fps). Retorna pixels aproximados.
    pub fn paint(
        &mut self,
        fb: &mut DoubleBuffer,
        fft_energy: f32,
        now_us: u64,
        sig: &OrbSignals,
    ) -> u32 {
        let dt = if now_us == 0 {
            33_333
        } else {
            now_us.saturating_sub(self.now_us).min(250_000)
        }
        .max(1);
        self.now_us = now_us;
        self.anim_us = self.anim_us.wrapping_add(dt);

        let st = self.machine.update(sig, now_us);
        self.body = converge(self.body, st.body_tint(), 51);
        self.accent = converge(self.accent, st.accent(), 51);
        self.update_lod();

        // Atividade cognitiva: acende ticks e dispara ripple.
        let delta = sig.activity.wrapping_sub(self.activity_seen);
        self.activity_seen = sig.activity;
        if delta > 0 {
            for _ in 0..delta.min(8) {
                self.flash_head = (self.flash_head + 1) % TICK_COUNT;
                self.tick_lit[self.flash_head] = 240;
            }
            self.ripple_start_us = self.anim_us;
        }
        for l in self.tick_lit.iter_mut() {
            *l = l.saturating_sub(10);
        }

        let (cx, cy) = (self.cx, self.cy);
        let base = self.base_r * self.size_scale;
        if base < 8.0 {
            return 0;
        }
        let breath_ph = ((self.anim_us / 15_625) & 0xFF) as u8; // ~4 s
        let breath = sin_q8(breath_ph) as f32 / 127.0;
        let amp = if st == OrbState::Speaking { 0.11 } else { 0.06 };
        let fft_boost = if fft_energy > 0.01 {
            fft_energy.min(1.0) * base * 0.14
        } else {
            0.0
        };
        let r = base * (1.0 + breath * amp * self.pulse_speed) + fft_boost;
        // Teto da LUT: halo (2.1r) tem de caber; acima disso o glow satura.
        let r = r.min(ORB_LUT_R as f32 / 2.1);
        let r_i = r as isize;

        // Spin fixed-point (grau·256): 12°/s · pulse_speed · mesh (peers vivos).
        let inc = ((dt as u64 * 12 * 256) / 1_000_000) as u32;
        let mesh_boost = 1.0 + (sig.peers.min(8) as f32) * 0.02;
        self.spin_deg256 = self
            .spin_deg256
            .wrapping_add(((inc as f32) * self.pulse_speed * mesh_boost) as u32);
        let spin8 = ((self.spin_deg256 + (self.rotation_deg << 8)) >> 8) as u8;
        let ring_phase = (self.anim_us / 62_500) as u8; // 16 s/volta

        let mut px = 0u32;

        // 1. Lattice hex — todo paint (stride só reduz densidade; sem blink).
        let grid_stride = match self.lod {
            2 => 1,
            1 => 2,
            _ => 3,
        };
        px += blit_grid(fb, ring_phase, grid_stride) as u32;

        // 2. Halo externo (acento) + interno (ciano) — bandas de linha.
        let halo_r = (r * 2.1) as isize;
        if halo_r > 0 {
            let (ar, ag, ab) = self.accent;
            let strength = st.halo_strength();
            let lut = self.ensure_lut(halo_r as usize);
            px += fb.fill_circle_alpha_bands(cx, cy, halo_r, lut, ar, ag, ab, strength) as u32;
        }
        let inner_halo = (r * 1.28) as isize;
        if inner_halo > 0 {
            let lut = self.ensure_lut(inner_halo as usize);
            px += fb.fill_circle_alpha_bands(
                cx, cy, inner_halo, lut, 0, 150, 220, 120,
            ) as u32;
        }

        // 3. Anéis (metade traseira, dy<0) + rim traseiro — atrás do corpo.
        let nrings = match self.lod {
            2 => self.ring_count.min(3) as usize,
            1 => (self.ring_count.min(2)) as usize,
            _ => 0,
        };
        let mut ring_geo = [(0isize, 0isize, 0u8); 3];
        for i in 0..nrings {
            let rf = 1.26 + i as f32 * 0.24;
            let rx = (r * rf) as isize;
            let ry = (rx as f32 * 0.30) as isize;
            ring_geo[i] = (rx, ry, ring_phase.wrapping_add((i as u8) * 64));
            if ry >= 2 {
                let col = scale_color(self.accent, 110);
                let lut = self.ensure_lut(ry as usize);
                px += fb.ring_spans(
                    cx, cy, rx, ry, 2, col.0, col.1, col.2,
                    ring_geo[i].2, -ry, -1, lut,
                ) as u32;
            }
        }
        if r_i > 0 {
            let col = scale_color(self.accent, 150);
            let lut = self.ensure_lut(r_i as usize);
            px += fb.ring_spans(cx, cy, r_i, r_i, 2, col.0, col.1, col.2, spin8, -r_i, -1, lut) as u32;
        }

        // 4. Corpo / inner / core / specular — spans planos.
        if r_i > 0 {
            let bc = self.body;
            let lut = self.ensure_lut(r_i as usize);
            px += fb.fill_circle_flat_spans(cx, cy, r_i, lut, bc.0, bc.1, bc.2) as u32;
        }
        let ir = (r * 0.68) as isize;
        if ir > 0 {
            let lut = self.ensure_lut(ir as usize);
            px += fb.fill_circle_flat_spans(
                cx, cy, ir, lut, BODY_INNER.0, BODY_INNER.1, BODY_INNER.2,
            ) as u32;
        }
        let cr = ((r * 0.16) as isize).max(3);
        {
            let lut = self.ensure_lut(cr as usize);
            px += fb.fill_circle_flat_spans(cx, cy, cr, lut, CORE.0, CORE.1, CORE.2) as u32;
        }
        let sr = ((r * 0.05) as isize).max(2);
        let spx = cx - (r * 0.20) as isize;
        let spy = cy - (r * 0.20) as isize;
        {
            let lut = self.ensure_lut(sr as usize);
            px += fb.fill_circle_flat_spans(spx, spy, sr, lut, 255, 255, 255) as u32;
        }

        // 5. Bandas de scan (holograma) — escurecem o corpo, viajam com o tempo.
        if self.lod >= 1 {
            let bw = (r * 2.0 * 0.92) as usize;
            let bx = (cx - (bw as isize / 2)).max(0) as usize;
            let fh = fb.info.height as isize;
            for i in 0..3u64 {
                let travel = ((self.anim_us / 14_000) + i * 120) % 360;
                let y = cy - r_i + ((travel as isize * 2 * r_i) / 360);
                if y >= 0 && y < fh {
                    px += fb.fill_rect_darken_tint(bx, y as usize, bw, 2, 0, 26, 48, 120) as u32;
                }
            }
        }

        // 6. Rim frontal + anéis dianteiros (dy≥0) — por cima do corpo.
        for i in 0..nrings {
            let (rx, ry, ph) = ring_geo[i];
            if ry >= 2 {
                let col = scale_color(self.accent, 190);
                let lut = self.ensure_lut(ry as usize);
                px += fb.ring_spans(
                    cx, cy, rx, ry, 2, col.0, col.1, col.2, ph, 0, ry, lut,
                ) as u32;
            }
        }
        if r_i > 0 {
            let col = scale_color(self.accent, 220);
            let lut = self.ensure_lut(r_i as usize);
            px += fb.ring_spans(cx, cy, r_i, r_i, 2, col.0, col.1, col.2, spin8, 0, r_i, lut) as u32;
        }

        // 7. Ticks radiais (24) — acendem com tokens/eventos.
        if self.lod >= 1 {
            let rr0 = r_i + 3;
            for i in 0..TICK_COUNT {
                if self.lod == 1 && i % 2 == 1 {
                    continue;
                }
                let lit = self.tick_lit[i] as i32;
                let a8 = ((i as u32 * 256) / TICK_COUNT as u32) as u8;
                let ang = a8.wrapping_add(spin8);
                let (s, c) = (sin_q8(ang) as isize, cos_q8(ang) as isize);
                let rr1 = rr0 + if lit > 40 { 9 } else { 5 };
                let x0 = cx + ((c * rr0) >> 7);
                let y0 = cy + ((s * rr0) >> 7);
                let x1 = cx + ((c * rr1) >> 7);
                let y1 = cy + ((s * rr1) >> 7);
                let col = scale_color(self.accent, 96 + lit);
                fb.draw_line(x0, y0, x1, y1, col.0, col.1, col.2);
                px += 6;
            }
        }

        // 8. Sparks (~12) — órbitas elípticas dirigidas por tempo.
        let nsparks = match self.lod {
            2 => NUM_PARTICLES,
            1 => 8,
            _ => 4,
        };
        let t_ms = self.anim_us / 1000;
        for p in PARTICLES.iter().take(nsparks) {
            let phase = (p.phase8 as u64 + (t_ms * p.speed_q8 as u64) / 1000) as u8;
            let (s, c) = (sin_q8(phase) as isize, cos_q8(phase) as isize);
            let dist = (r * (p.dist_q8 as f32 / 256.0)) as isize;
            let sx = cx + ((c * dist) >> 7);
            let sy = cy + (((s * dist) >> 7) * 55 / 100);
            if sx >= 0 && sy >= 0 {
                let col = if p.size > 1 { SPARK } else { scale_color(SPARK, 160) };
                px += fb.fill_rect_fast(
                    (sx as usize).saturating_sub(1),
                    (sy as usize).saturating_sub(1),
                    p.size as usize,
                    p.size as usize,
                    col.0, col.1, col.2,
                ) as u32;
            }
        }

        // 9. Ripple de atividade (expande ~0.7 s após tokens/eventos).
        let age = self.anim_us.wrapping_sub(self.ripple_start_us);
        if self.ripple_start_us != 0 && age < 700_000 && self.lod >= 1 {
            let t = age as f32 / 700_000.0;
            let rx = (r * (0.9 + t * 0.8)) as isize;
            let ry = (rx as f32 * 0.30) as isize;
            if ry >= 2 {
                let col = scale_color(self.accent, ((1.0 - t) * 220.0) as i32);
                let lut = self.ensure_lut(ry as usize);
                px += fb.ring_spans(cx, cy, rx, ry, 1, col.0, col.1, col.2, 0, -ry, ry, lut) as u32;
            }
        }

        ORB_PIXELS.store(px, Ordering::Relaxed);
        px
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::fb::{DoubleBuffer, GpuDevice};

    /// GRID_BUF é global — os testes de grid serializam nele.
    static GRID_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn test_fb(w: usize, h: usize) -> DoubleBuffer {
        DoubleBuffer::from_gpu(&GpuDevice::from_probe(0, w as u32, h as u32, w as u32, 4, true))
    }

    /// Referência: mesma expressão do blit (fade f32 + pulso vindo da LUT).
    fn legacy_hex_grid(fb: &mut DoubleBuffer, pulse_phase: u8, cx: isize, cy: isize, fw: isize, fh: isize) {
        let spacing = 56isize;
        let hex_h = (spacing as f32 * 0.866) as isize;
        let pulsing = 0.75 + (sin_q8(pulse_phase) as f32 / 127.0) * 0.25;
        let mut row = 0isize;
        let mut y = -spacing;
        while y < fh + spacing {
            let offset_x = if row % 2 == 1 { spacing / 2 } else { 0 };
            let mut x = -spacing + offset_x;
            while x < fw + spacing {
                if x >= 0 && y >= 0 && x < fw && y < fh {
                    let dx = x - cx;
                    let dy = y - cy;
                    let d_sq = (dx * dx + dy * dy) as f32;
                    let max_d = (fw * fw + fh * fh) as f32 * 0.22;
                    if d_sq < max_d {
                        let dist_fade = 1.0 - (d_sq / max_d).min(1.0);
                        let a = (10.0 * dist_fade * pulsing) as u8;
                        if a >= 3 {
                            fb.set_pixel(
                                x as usize, y as usize,
                                0, 80, 120,
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

    #[test]
    fn grid_blit_matches_legacy_pixel_identical() {
        let _guard = GRID_TEST_LOCK.lock();
        for (fw, fh) in [(320usize, 200usize), (640, 360), (1280, 720)] {
            let (cx, cy) = (fw as isize / 2, fh as isize / 2);
            init_grid_buffer(cx, cy, fw as isize, fh as isize);
            assert!(GRID_BUF.lock().len > 0, "grid vazio em {fw}x{fh}");
            for phase in (0..64u8).step_by(8) {
                let mut a = test_fb(fw, fh);
                let mut b = test_fb(fw, fh);
                blit_grid(&mut a, phase, 1);
                legacy_hex_grid(&mut b, phase, cx, cy, fw as isize, fh as isize);
                for y in 0..fh {
                    for x in 0..fw {
                        assert_eq!(a.get_pixel(x, y), b.get_pixel(x, y), "grid ({x},{y}) phase {phase} {fw}x{fh}");
                    }
                }
            }
        }
    }

    #[test]
    fn grid_stride_is_subset_and_stable() {
        let _guard = GRID_TEST_LOCK.lock();
        let (fw, fh) = (640usize, 360usize);
        let (cx, cy) = (fw as isize / 2, fh as isize / 2);
        init_grid_buffer(cx, cy, fw as isize, fh as isize);
        let mut full = test_fb(fw, fh);
        let mut half = test_fb(fw, fh);
        blit_grid(&mut full, 0, 1);
        blit_grid(&mut half, 0, 2);
        for y in 0..fh {
            for x in 0..fw {
                let p = full.get_pixel(x, y);
                if half.get_pixel(x, y) != Some((0, 0, 0)) {
                    assert_eq!(p, half.get_pixel(x, y), "stride extrai pixel fora do full em ({x},{y})");
                }
            }
        }
    }

    #[test]
    fn orb_state_machine_dwell_and_alert_preempt() {
        let mut m = OrbMachine::new();
        let none = OrbSignals::default();
        assert_eq!(m.update(&none, 1_000_000), OrbState::Idle);
        // Thinking troca após o dwell de 400 ms.
        let think = OrbSignals { thinking: true, ..Default::default() };
        assert_eq!(m.update(&think, 1_100_000), OrbState::Idle);
        assert_eq!(m.update(&think, 1_450_000), OrbState::Thinking);
        // ALERT preempta imediatamente.
        let alert = OrbSignals { thinking: true, alert: true, ..Default::default() };
        assert_eq!(m.update(&alert, 1_460_000), OrbState::Alert);
        // Speaking tem cauda de 1.5 s mesmo após o sinal cair.
        let speak = OrbSignals { speaking: true, ..Default::default() };
        assert_eq!(m.update(&speak, 1_500_000), OrbState::Alert);
        assert_eq!(m.update(&speak, 1_950_000), OrbState::Speaking);
        assert_eq!(m.update(&none, 2_400_000), OrbState::Speaking);
        assert_eq!(m.update(&none, 3_600_000), OrbState::Idle);
    }

    #[test]
    fn orb_color_converges_without_sawtooth() {
        let mut c = (0u8, 0u8, 0u8);
        let target = OrbState::Thinking.accent();
        let mut prev_dist = 1020u32;
        for _ in 0..80 {
            c = converge(c, target, 51);
            let dist = (c.0 as i32 - target.0 as i32).unsigned_abs()
                + (c.1 as i32 - target.1 as i32).unsigned_abs()
                + (c.2 as i32 - target.2 as i32).unsigned_abs();
            assert!(dist <= prev_dist, "distância aumentou (sawtooth)");
            prev_dist = dist;
        }
        assert_eq!(c, target, "não convergiu em 80 paints");
    }

    #[test]
    fn orb_paint_stays_in_budget_and_counts_pixels() {
        let (fw, fh) = (1280usize, 720usize);
        let mut fb = test_fb(fw, fh);
        let mut r = SoulMirrorRenderer::new(fw, fh);
        let mut sig = OrbSignals { thinking: true, peers: 2, ..Default::default() };
        // 3 frames para estabilizar LUT/caches.
        for i in 0..3u64 {
            r.paint(&mut fb, 0.0, (i + 1) * 33_333, &sig);
        }
        sig.activity = sig.activity.wrapping_add(1);
        let px = r.paint(&mut fb, 0.25, 4 * 33_333, &sig);
        assert!(px > 10_000, "orb pintou poucos pixels: {px}");
        assert_eq!(ORB_PIXELS.load(Ordering::Relaxed), px);
        assert!(fb.dirty);
    }

    /// Benchmark host do paint do orb (exclui o swap UC). Rode com `--release`.
    #[test]
    fn orb_bench_print() {
        let (fw, fh) = (1280usize, 720usize);
        let mut fb = test_fb(fw, fh);
        let mut r = SoulMirrorRenderer::new(fw, fh);
        let mut sig = OrbSignals { thinking: true, peers: 3, ..Default::default() };
        const N: u64 = 300;
        // Warmup (LUT + primeiros paints).
        for i in 0..8u64 {
            r.paint(&mut fb, 0.3, (i + 1) * 33_333, &sig);
        }
        let t0 = std::time::Instant::now();
        for i in 0..N {
            sig.activity = sig.activity.wrapping_add(1);
            r.paint(&mut fb, 0.3, (i + 9) * 33_333, &sig);
        }
        let dt = t0.elapsed();
        eprintln!(
            "BENCH orb {}x{} frames={} total={:?} per_frame={:.1}us pixels_last={}",
            fw,
            fh,
            N,
            dt,
            dt.as_micros() as f64 / N as f64,
            ORB_PIXELS.load(Ordering::Relaxed),
        );
    }

    #[test]
    fn grid_bench_print() {
        let _guard = GRID_TEST_LOCK.lock();
        let (fw, fh) = (640usize, 360usize);
        let (cx, cy) = (fw as isize / 2, fh as isize / 2);
        let mut a = test_fb(fw, fh);
        let t0 = std::time::Instant::now();
        for phase in (0..64u8).step_by(4) {
            legacy_hex_grid(&mut a, phase, cx, cy, fw as isize, fh as isize);
        }
        let t_old = t0.elapsed();
        init_grid_buffer(cx, cy, fw as isize, fh as isize);
        let mut b = test_fb(fw, fh);
        let t1 = std::time::Instant::now();
        for phase in (0..64u8).step_by(4) {
            blit_grid(&mut b, phase, 1);
        }
        let t_new = t1.elapsed();
        eprintln!(
            "BENCH grid dots={} old={:?} new={:?} speedup={:.2}x",
            GRID_BUF.lock().len,
            t_old,
            t_new,
            t_old.as_secs_f64() / t_new.as_secs_f64().max(1e-9)
        );
    }
}
