//! Relógios de medição (HUD) — CPU/MEM/GPU/HD com dados honestos do K³CHJ.
//! Amostragem: MetricsAgent (~0,5s). Compositor só desenha o snapshot.

use alloc::string::String;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use libm::{cosf, sinf};
use spin::Mutex;
use crate::display::fb::DoubleBuffer;
use crate::display::compositor::draw_text;

const PI: f32 = core::f32::consts::PI;
/// Altura da barra de status (compositor reserva y≥28 para janelas).
pub const STATUS_BAR_H: usize = 28;

#[derive(Clone)]
pub struct GaugeSnapshot {
    pub cpu_pct: f32,
    pub mem_pct: f32,
    pub gpu_pct: f32,
    pub hd_pct: f32,
    pub cpu_val: String,
    pub mem_val: String,
    pub gpu_val: String,
    pub hd_val: String,
    pub timer_at: usize,
    /// Load por core (0.0-1.0).
    pub per_core_load: [f32; 16],
    pub core_count: u8,
    /// Trinity MoE routing telemetry.
    pub trinity_neural: u64,
    pub trinity_keyword: u64,
    pub trinity_fallback: u64,
    pub expert_resident_kb: usize,
}

impl GaugeSnapshot {
    fn empty() -> Self {
        GaugeSnapshot {
            cpu_pct: 0.0,
            mem_pct: 0.0,
            gpu_pct: 0.0,
            hd_pct: 0.0,
            cpu_val: String::from("-"),
            mem_val: String::from("-"),
            gpu_val: String::from("-"),
            hd_val: String::from("-"),
            timer_at: 0,
            per_core_load: [0.0; 16],
            core_count: 0,
            trinity_neural: 0,
            trinity_keyword: 0,
            trinity_fallback: 0,
            expert_resident_kb: 0,
        }
    }
}

static SNAPSHOT: Mutex<GaugeSnapshot> = Mutex::new(GaugeSnapshot {
    cpu_pct: 0.0,
    mem_pct: 0.0,
    gpu_pct: 0.0,
    hd_pct: 0.0,
    cpu_val: String::new(),
    mem_val: String::new(),
    gpu_val: String::new(),
    hd_val: String::new(),
    timer_at: 0,
    per_core_load: [0.0; 16],
    core_count: 0,
    trinity_neural: 0,
    trinity_keyword: 0,
    trinity_fallback: 0,
    expert_resident_kb: 0,
});
static SNAPSHOT_READY: AtomicBool = AtomicBool::new(false);
static CACHED_HD_SECTORS: AtomicU64 = AtomicU64::new(u64::MAX);

fn sample_mem() -> (f32, u32, u32) {
    let guard = k_nano::memory::GLOBAL_ALLOCATOR.lock();
    if let Some(ref alloc) = *guard {
        let usable = alloc.usable_frames.max(1);
        let used = alloc.allocated_count.min(usable);
        let pct = used as f32 / usable as f32;
        let used_mb = ((used as u64) * 4 / 1024) as u32;
        let tot_mb = ((usable as u64) * 4 / 1024) as u32;
        (pct.clamp(0.0, 1.0), used_mb, tot_mb.max(1))
    } else {
        (0.0, 0, 0)
    }
}

fn sample_cpu() -> (f32, u32) {
    let agents = agent_core::LAST_SCHED_AGENTS.load(Ordering::Relaxed) as u32;
    let polled = agent_core::LAST_SCHED_POLLED.load(Ordering::Relaxed);
    let cores = k_nano::smp::percpu::CPU_COUNT.load(Ordering::Relaxed) as u32;
    let pct = if agents > 0 {
        (polled as f32 / agents as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (pct, cores.max(1))
}

fn sample_gpu() -> (f32, String) {
    if let Some((used, total)) = k_hal::gpu::vram::vram_usage() {
        if total > 0 {
            let pct = (used as f32 / total as f32).clamp(0.0, 1.0);
            let a = used / (1024 * 1024);
            let b = (total / (1024 * 1024)).max(1);
            return (pct, alloc::format!("{}/{}M", a, b));
        }
    }
    // FB presente ≠ occupancy de GPU. Sem VRAM meter → n/a (não inventar 0.15).
    (0.0, String::from("n/a"))
}

fn sample_hd() -> (f32, String) {
    let cached = CACHED_HD_SECTORS.load(Ordering::Relaxed);
    let sectors = if cached != u64::MAX {
        cached
    } else {
        let sectors = {
            let guard = k_nano::ATA_DRIVER.lock();
            if let Some(ref ata) = *guard {
                unsafe { ata.total_sectors().unwrap_or(0) }
            } else {
                0
            }
        };
        CACHED_HD_SECTORS.store(sectors, Ordering::Relaxed);
        sectors
    };
    if sectors == 0 {
        return (0.0, String::from("n/a"));
    }
    let gb = ((sectors * 512) / (1024 * 1024 * 1024)).max(1);
    // Capacidade conhecida; ocupação real do FS não medida aqui.
    (0.0, alloc::format!("{}G", gb))
}

/// Chamado pelo MetricsAgent (~0,5s) — amostra e publica snapshot.
/// `log_serial`: true só na 1ª amostra / periodicamente (HW sem serial satura COM).

/// Amostra load por-core via runqueue CpuStats.
#[cfg(feature = "smp-runqueue")]
fn sample_per_core_load() -> ([f32; 16], u8) {
    let cores = k_nano::smp::percpu::CPU_COUNT.load(Ordering::Relaxed) as usize;
    let mut load = [0.0f32; 16];
    for c in 0..cores.min(16) {
        let stats = k_nano::smp::runqueue::cpu_stats(c);
        let running = stats.running.load(Ordering::Relaxed) as f32;
        let blocked = stats.blocked.load(Ordering::Relaxed) as f32;
        let total = running + blocked;
        load[c] = if total > 0.0 { (running / total).clamp(0.0, 1.0) } else { 0.0 };
    }
    (load, cores.min(16) as u8)
}

#[cfg(not(feature = "smp-runqueue"))]
fn sample_per_core_load() -> ([f32; 16], u8) { ([0.0; 16], 0) }

pub fn refresh_snapshot(log_serial: bool) {
    let (cpu_pct, cores) = sample_cpu();
    let (mem_pct, mem_used, mem_tot) = sample_mem();
    let (gpu_pct, gpu_val) = sample_gpu();
    let (hd_pct, hd_val) = sample_hd();
    let now = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed);
    let snap = GaugeSnapshot {
        cpu_pct,
        mem_pct,
        gpu_pct,
        hd_pct,
        cpu_val: alloc::format!("{}c{}%", cores, (cpu_pct * 100.0) as u32),
        mem_val: alloc::format!("{}/{}M", mem_used, mem_tot),
        gpu_val: gpu_val.clone(),
        hd_val: hd_val.clone(),
        timer_at: now,
        per_core_load: sample_per_core_load().0,
        core_count: sample_per_core_load().1,
        trinity_neural: 0,
        trinity_keyword: 0,
        trinity_fallback: 0,
        expert_resident_kb: {
            let mut total = 0usize;
            for kind in &[cortex::trinity::ExpertKind::HwIdentify, cortex::trinity::ExpertKind::RustCoder] {
                total += cortex::trinity::expert_resident_bytes_kind(*kind);
            }
            total / 1024
        },
    };
    *SNAPSHOT.lock() = snap;
    SNAPSHOT_READY.store(true, Ordering::Release);
    if log_serial {
        k_nano::slog_jarbas!(
            "Metrics",
            "info",
            "snapshot cpu={}% mem={}% gpu={} hd={} tri={}KB",
            (cpu_pct * 100.0) as u32,
            (mem_pct * 100.0) as u32,
            gpu_val,
            hd_val,
            {
                let mut total = 0usize;
                for kind in &[cortex::trinity::ExpertKind::HwIdentify, cortex::trinity::ExpertKind::RustCoder] {
                    total += cortex::trinity::expert_resident_bytes_kind(*kind);
                }
                total / 1024
            }
        );
    }
}

/// Desenha a barra a partir do snapshot (sem reamostrar).
pub fn draw_status_gauges(fb: &mut DoubleBuffer, screen_w: usize) {
    fb.fill_rect(0, 0, screen_w, STATUS_BAR_H, 12, 16, 24);

    let snap = if SNAPSHOT_READY.load(Ordering::Acquire) {
        SNAPSHOT.lock().clone()
    } else {
        GaugeSnapshot::empty()
    };

    let readings: [(&str, f32, &str); 4] = [
        ("CPU", snap.cpu_pct, snap.cpu_val.as_str()),
        ("MEM", snap.mem_pct, snap.mem_val.as_str()),
        ("GPU", snap.gpu_pct, snap.gpu_val.as_str()),
        ("HD", snap.hd_pct, snap.hd_val.as_str()),
    ];

    // Trinity info (após gauges, à direita)
    if snap.expert_resident_kb > 0 {
        let trinity_text = alloc::format!("TRI {}KB", snap.expert_resident_kb);
        let tx = screen_w.saturating_sub(trinity_text.len() * 6 + 8);
        draw_text(fb, tx, 0, &trinity_text, screen_w, 0, 255, 200);
    }

    let n = readings.len();
    let slot = screen_w / n.max(1);
    let radius = 10isize;
    for (i, (label, pct, value)) in readings.iter().enumerate() {
        let cx = (slot * i + slot / 2) as isize;
        let cy = 12isize;
        draw_gauge(fb, cx, cy, radius, *pct);
        let lx = (cx as usize).saturating_sub(12);
        draw_text(fb, lx, 0, label, screen_w, 140, 180, 210);
        let vx = (cx as usize).saturating_sub(value.len().saturating_mul(4));
        draw_text(
            fb,
            vx.min(screen_w.saturating_sub(8)),
            18,
            value,
            screen_w,
            200,
            220,
            240,
        );
    }

    // Uptime ao vivo (barato); métricas dos gauges vêm do snapshot 0,5s.
    let ticks = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed);
    let secs = ticks / 18;
    let up = alloc::format!("T{}s", secs);
    draw_text(
        fb,
        screen_w.saturating_sub(up.len() * 8 + 4),
        10,
        &up,
        screen_w,
        100,
        140,
        160,
    );
}

fn gauge_color(pct: f32) -> (u8, u8, u8) {
    if pct < 0.55 {
        (40, 200, 120)
    } else if pct < 0.80 {
        (220, 180, 40)
    } else {
        (230, 70, 60)
    }
}

fn draw_gauge(fb: &mut DoubleBuffer, cx: isize, cy: isize, r: isize, pct: f32) {
    let pct = pct.clamp(0.0, 1.0);
    let (cr, cg, cb) = gauge_color(pct);
    let start = -PI * 0.75;
    let span = PI * 1.5;
    let steps = 28;
    for i in 0..steps {
        let t0 = start + span * (i as f32 / steps as f32);
        let x = cx + (cosf(t0) * r as f32) as isize;
        let y = cy + (sinf(t0) * r as f32) as isize;
        if x >= 0 && y >= 0 {
            fb.set_pixel(x as usize, y as usize, 40, 50, 65);
        }
    }
    let fill_steps = ((steps as f32) * pct) as isize;
    for i in 0..fill_steps.max(0) {
        let t0 = start + span * (i as f32 / steps as f32);
        let x = cx + (cosf(t0) * r as f32) as isize;
        let y = cy + (sinf(t0) * r as f32) as isize;
        if x >= 0 && y >= 0 {
            fb.set_pixel(x as usize, y as usize, cr, cg, cb);
            let x2 = cx + (cosf(t0) * (r - 1) as f32) as isize;
            let y2 = cy + (sinf(t0) * (r - 1) as f32) as isize;
            if x2 >= 0 && y2 >= 0 {
                fb.set_pixel(x2 as usize, y2 as usize, cr, cg, cb);
            }
        }
    }
    let ang = start + span * pct;
    let tip_r = (r - 2) as f32;
    let tx = cx + (cosf(ang) * tip_r) as isize;
    let ty = cy + (sinf(ang) * tip_r) as isize;
    let steps_n = r.max(1);
    for s in 0..=steps_n {
        let t = s as f32 / steps_n as f32;
        let x = cx as f32 + (tx - cx) as f32 * t;
        let y = cy as f32 + (ty - cy) as f32 * t;
        if x >= 0.0 && y >= 0.0 {
            fb.set_pixel(x as usize, y as usize, 230, 240, 255);
        }
    }
}

/// Retorna copia do snapshot (compositor per-core).
pub fn snapshot() -> GaugeSnapshot {
    SNAPSHOT.lock().clone()
}
