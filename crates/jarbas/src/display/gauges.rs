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
    pub per_core_load: [f32; 32],
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
            per_core_load: [0.0; 32],
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
    per_core_load: [0.0; 32],
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

/// Amostra load por-core via runqueue CpuStats (até 32 cores no HUD).
#[cfg(feature = "smp-runqueue")]
fn sample_per_core_load() -> ([f32; 32], u8) {
    let cores = k_nano::smp::percpu::CPU_COUNT.load(Ordering::Relaxed) as usize;
    let mut load = [0.0f32; 32];
    let n = cores.min(32);
    for c in 0..n {
        let stats = k_nano::smp::runqueue::cpu_stats(c);
        let running = stats.running.load(Ordering::Relaxed) as f32;
        let blocked = stats.blocked.load(Ordering::Relaxed) as f32;
        let total = running + blocked;
        load[c] = if total > 0.0 { (running / total).clamp(0.0, 1.0) } else { 0.0 };
    }
    (load, n as u8)
}

#[cfg(not(feature = "smp-runqueue"))]
fn sample_per_core_load() -> ([f32; 32], u8) { ([0.0; 32], 0) }

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
    // Hub Health: mesmo fill 2 Hz (painel F12 não amostra no paint).
    refresh_hub_health();
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
    fb.fill_rect(0, 0, screen_w, STATUS_BAR_H, 8, 12, 24);

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
    let ticks = k_nano::interrupts::wall_ticks() as usize;
    let hz = k_nano::interrupts::TIMER_HZ.load(Ordering::Relaxed).max(1) as usize;
    let secs = ticks / hz;
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
}fn draw_gauge(fb: &mut DoubleBuffer, cx: isize, cy: isize, r: isize, pct: f32) {
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

// ══════════════════════════════════════════════════════════════════════════
// Hub Health — snapshot Copy do painel diagnóstico (F12 / clique no orb).
// Preenchido a ~2 Hz junto do refresh_snapshot; o paint só lê (zero alloc).
// Honestidade: dado ausente = `n/a`, nunca 0 inventado.
// ══════════════════════════════════════════════════════════════════════════

pub const HUB_ROWS: usize = 15;
pub const HUB_ROW_LEN: usize = 36;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HubState {
    Ok,
    Warn,
    Fail,
    Na,
}

impl HubState {
    pub fn color(self) -> (u8, u8, u8) {
        match self {
            HubState::Ok => (0x30, 0xFF, 0x90),
            HubState::Warn => (0xFF, 0xB0, 0x20),
            HubState::Fail => (0xFF, 0x4A, 0x5A),
            HubState::Na => (0x7A, 0x87, 0x94),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            HubState::Ok => "OK",
            HubState::Warn => "WARN",
            HubState::Fail => "FAIL",
            HubState::Na => "n/a",
        }
    }

    /// Severidade p/ o pill — `Na` não puxa (ausência ≠ falha).
    fn severity(self) -> u8 {
        match self {
            HubState::Ok => 0,
            HubState::Warn => 1,
            HubState::Fail => 2,
            HubState::Na => 0,
        }
    }

    pub fn worst(self, other: HubState) -> HubState {
        if other.severity() > self.severity() { other } else { self }
    }

    /// Código de severidade publicado no HubHealthAgent (0=ok 1=warn 2=fail 3=n/a).
    pub fn sev_code(self) -> u8 {
        match self {
            HubState::Ok => 0,
            HubState::Warn => 1,
            HubState::Fail => 2,
            HubState::Na => 3,
        }
    }
}

#[derive(Clone, Copy)]
pub struct HubRow {
    pub label: &'static str,
    value: [u8; HUB_ROW_LEN],
    vlen: u8,
    pub state: HubState,
    /// true = participa do pill do header (fault histórico contido não puxa).
    pub pill: bool,
}

impl HubRow {
    pub fn value(&self) -> &str {
        core::str::from_utf8(&self.value[..self.vlen as usize]).unwrap_or("")
    }
}

const EMPTY_ROW: HubRow = HubRow {
    label: "",
    value: [0u8; HUB_ROW_LEN],
    vlen: 0,
    state: HubState::Na,
    pill: false,
};

/// Snapshot Copy do painel — rows formatadas no fill (2 Hz), zero alloc no paint.
#[derive(Clone, Copy)]
pub struct HubHealth {
    pub rows: [HubRow; HUB_ROWS],
    pub worst: HubState,
    pub checksum: u64,
    live: [u8; 64],
    live_len: u8,
}

impl HubHealth {
    pub const fn empty() -> Self {
        HubHealth {
            rows: [EMPTY_ROW; HUB_ROWS],
            worst: HubState::Na,
            checksum: 0,
            live: [0u8; 64],
            live_len: 0,
        }
    }

    pub fn live_line(&self) -> &str {
        core::str::from_utf8(&self.live[..self.live_len as usize]).unwrap_or("")
    }
}

/// Worst das linhas com `pill=true` (Na e linhas fora do pill não contam).
pub fn hub_worst(rows: &[HubRow; HUB_ROWS]) -> HubState {
    let mut w = HubState::Ok;
    for r in rows.iter() {
        if r.pill {
            w = w.worst(r.state);
        }
    }
    w
}

static HUB: Mutex<Option<HubHealth>> = Mutex::new(None);

/// Snapshot corrente do Hub Health (Copy — sem lock no paint além do leque).
pub fn hub_health() -> HubHealth {
    let g = HUB.lock();
    match *g {
        Some(h) => h,
        None => HubHealth::empty(),
    }
}

fn fnv1a(mut h: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    h
}

fn hub_set(row: &mut HubRow, label: &'static str, state: HubState, pill: bool, s: alloc::string::String) {
    row.label = label;
    row.state = state;
    row.pill = pill;
    let b = s.as_bytes();
    let n = b.len().min(HUB_ROW_LEN);
    row.value[..n].copy_from_slice(&b[..n]);
    row.vlen = n as u8;
}

/// Amostra os fatos do hub (2 Hz, junto do refresh_snapshot). Nunca no paint.
pub fn refresh_hub_health() {
    use core::sync::atomic::Ordering;
    let mut hh = HubHealth::empty();

    // ── USB / xHCI ──
    let stage = k_nano::xhci::xhci_last_stage();
    let cands = k_nano::xhci::xhci_cand_count();
    let bdf = k_nano::xhci::xhci_last_bdf();
    let bdf_s = alloc::format!("{:02x}:{:02x}.{}", (bdf >> 8) & 0xFF, (bdf >> 3) & 0x1F, bdf & 7);
    let (st, val, pill) = match stage {
        0 => (HubState::Na, alloc::string::String::from("not scanned"), false),
        1 => (HubState::Fail, alloc::format!("no ctrl c{}", cands), true),
        10 => (HubState::Ok, alloc::format!("c{} st{} {}", cands, stage, bdf_s), true),
        20 => (HubState::Fail, alloc::format!("down c{}", cands), true),
        _ => (HubState::Warn, alloc::format!("c{} st{} {}", cands, stage, bdf_s), true),
    };
    hub_set(&mut hh.rows[0], "xhci", st, pill, val);

    let msc = k_nano::globals::USB_MSC.lock().is_some();
    let maxp = k_nano::xhci::host_max_ports();
    let ccs = k_nano::xhci::host_ccs_count();
    let msc_info = k_nano::xhci::host_msc_info();
    let (st, val, pill) = match (msc, maxp) {
        (true, _) => {
            let v = match msc_info {
                Some((p, 0)) => alloc::format!("msc p{} s?", p),
                Some((p, s)) => alloc::format!("msc p{} s{}", p, s),
                None => alloc::string::String::from("msc ready"),
            };
            (HubState::Ok, v, true)
        }
        (false, Some(p)) => {
            let st = if k_nano::xhci::xhci_msc_down() { HubState::Fail } else { HubState::Warn };
            (st, alloc::format!("{}p ccs{} no msc", p, ccs), true)
        }
        (false, None) => (HubState::Na, alloc::string::String::from("no xhci"), false),
    };
    hub_set(&mut hh.rows[1], "usb", st, pill, val);

    // ── TIMER ──
    let hz = k_nano::apic::tick_hz();
    let alive = k_nano::interrupts::timer_alive();
    let cap = k_hal::timer_cap::cap();
    let src = match cap.source {
        k_hal::timer_cap::TickSource::Lapic => "lapic",
        k_hal::timer_cap::TickSource::Pit => "pit",
        k_hal::timer_cap::TickSource::Soft => "soft",
        k_hal::timer_cap::TickSource::Unknown => "?",
    };
    let star = if cap.trusted { "*" } else { "" };
    let (st, val, pill) = if !alive {
        (HubState::Fail, alloc::format!("{}Hz {} DEAD", hz, src), true)
    } else if cap.trusted {
        (HubState::Ok, alloc::format!("{}Hz {}{}", hz, src, star), true)
    } else {
        (HubState::Warn, alloc::format!("{}Hz {}{}", hz, src, star), true)
    };
    hub_set(&mut hh.rows[2], "timer", st, pill, val);

    let frame_us = crate::display::compositor::frame_cost_us();
    let soft = k_nano::interrupts::SOFT_TIMER_TICKS.load(Ordering::Relaxed);
    let lod = crate::display::soul_mirror::orb_lod();
    let (st, val, pill) = if frame_us == 0 {
        (HubState::Na, alloc::string::String::from("no sample"), false)
    } else {
        let st = if frame_us < 16_000 {
            HubState::Ok
        } else if frame_us < 33_000 {
            HubState::Warn
        } else {
            HubState::Fail
        };
        (
            st,
            alloc::format!(
                "{}.{}ms j{}ppm d{} L{}",
                frame_us / 1000,
                (frame_us % 1000) / 100,
                cap.jitter_ppm,
                soft,
                lod
            ),
            true,
        )
    };
    hub_set(&mut hh.rows[3], "frame", st, pill, val);

    // ── STORAGE ──
    let ata = k_nano::globals::ATA_DRIVER.lock().is_some();
    let ahci = k_nano::globals::AHCI_DRIVER.lock().is_some();
    let fat = k_nano::boot_logger::FAT_READY.load(Ordering::Relaxed);
    let bus = if ata {
        "ata"
    } else if ahci {
        "ahci"
    } else if msc {
        "usb"
    } else {
        "none"
    };
    let (st, val, pill) = if bus == "none" {
        (HubState::Na, alloc::string::String::from("no dev"), false)
    } else if fat {
        (HubState::Ok, alloc::format!("{} fat32", bus), true)
    } else {
        (HubState::Warn, alloc::format!("{} no fat", bus), true)
    };
    hub_set(&mut hh.rows[4], "storage", st, pill, val);

    let ui = k_nano::boot_logger::ui_is_live();
    let (st, val, pill) = if fat {
        (HubState::Ok, alloc::string::String::from("fat ok"), true)
    } else if ui {
        (HubState::Warn, alloc::string::String::from("ram only"), true)
    } else {
        (HubState::Na, alloc::string::String::from("pending"), false)
    };
    hub_set(&mut hh.rows[5], "bootlog", st, pill, val);

    let kv_ready = k_nano::storage::is_ready();
    let backend = k_nano::storage::backend_name();
    let (st, val, pill) = if kv_ready && backend != "ram" {
        (HubState::Ok, alloc::format!("{} ready", backend), true)
    } else if kv_ready {
        (HubState::Warn, alloc::string::String::from("ram volatile"), true)
    } else {
        (HubState::Na, alloc::format!("{} not ready", backend), false)
    };
    hub_set(&mut hh.rows[6], "kv", st, pill, val);

    // ── MEMORY ──
    let used = k_nano::allocator::heap_used_bytes();
    let limit = k_nano::allocator::HEAP_LIMIT.load(Ordering::Relaxed).max(1);
    let pct = (used / (limit / 100).max(1)).min(999);
    let (st, val, pill) = if pct < 70 {
        (HubState::Ok, alloc::format!("{}/{}M {}%", used / 1048576, limit / 1048576, pct), true)
    } else if pct < 90 {
        (HubState::Warn, alloc::format!("{}/{}M {}%", used / 1048576, limit / 1048576, pct), true)
    } else {
        (HubState::Fail, alloc::format!("{}/{}M {}%", used / 1048576, limit / 1048576, pct), true)
    };
    hub_set(&mut hh.rows[7], "heap", st, pill, val);

    let total_mb = k_nano::memory::TOTAL_RAM_MB.load(Ordering::Relaxed);
    let (frames_used, frames_tot) = {
        let g = k_nano::memory::GLOBAL_ALLOCATOR.lock();
        match *g {
            Some(ref alloc) => (alloc.allocated_count, alloc.usable_frames),
            None => (0, 0),
        }
    };
    let (st, val, pill) = if total_mb == 0 || frames_tot == 0 {
        (HubState::Na, alloc::string::String::from("n/a"), false)
    } else {
        let fpct = (frames_used * 100 / frames_tot.max(1)) as u32;
        let st = if fpct < 85 {
            HubState::Ok
        } else if fpct < 95 {
            HubState::Warn
        } else {
            HubState::Fail
        };
        (st, alloc::format!("{}M f{}%", total_mb, fpct), true)
    };
    hub_set(&mut hh.rows[8], "ram", st, pill, val);

    let (au, ac) = cortex::global_arena::arena_stats();
    let (st, val, pill) = if ac == 0 {
        (HubState::Na, alloc::string::String::from("n/a"), false)
    } else {
        let apct = (au * 100 / ac) as u32;
        let st = if apct < 80 {
            HubState::Ok
        } else if apct < 95 {
            HubState::Warn
        } else {
            HubState::Fail
        };
        (st, alloc::format!("{}/{}M", au / 1048576, ac / 1048576), true)
    };
    hub_set(&mut hh.rows[9], "arena", st, pill, val);

    let (st, val, pill) = match cortex::model::loaded_model_header() {
        Some(m) => (
            HubState::Ok,
            alloc::format!("{}L h{} v{}", m.num_layers, m.hidden, m.vocab),
            true,
        ),
        None => (HubState::Na, alloc::string::String::from("n/a"), false),
    };
    hub_set(&mut hh.rows[10], "model", st, pill, val);

    // ── AGENTS ──
    let agents = agent_core::LAST_SCHED_AGENTS.load(Ordering::Relaxed);
    let polled = agent_core::LAST_SCHED_POLLED.load(Ordering::Relaxed);
    let (st, val, pill) = if agents == 0 {
        (HubState::Na, alloc::string::String::from("n/a"), false)
    } else if polled == 0 {
        (HubState::Warn, alloc::format!("{} poll 0", agents), true)
    } else {
        (HubState::Ok, alloc::format!("{} poll {}", agents, polled), true)
    };
    hub_set(&mut hh.rows[11], "agents", st, pill, val);

    let (peers, min_rtt) = {
        let g = crate::display::agent::MESH_GRAPH.lock();
        let rtt = g.iter().map(|p| p.avg_rtt).min().unwrap_or(0);
        (g.len(), rtt)
    };
    let (st, val, pill) = if peers == 0 {
        (HubState::Na, alloc::string::String::from("n/a"), false)
    } else {
        let st = if min_rtt < 800 { HubState::Ok } else { HubState::Warn };
        (st, alloc::format!("{}p {}ms", peers, min_rtt), true)
    };
    hub_set(&mut hh.rows[12], "mesh", st, pill, val);

    let q = cortex::infer_queue::queue_pending();
    let running = cortex::infer_queue::active_job_id() != 0;
    let (st, val, pill) = if q > 4 {
        (HubState::Warn, alloc::format!("q{} run{}", q, if running { "1" } else { "0" }), true)
    } else {
        (HubState::Ok, alloc::format!("q{} {}", q, if running { "run" } else { "idle" }), true)
    };
    hub_set(&mut hh.rows[13], "infer", st, pill, val);

    let exc_n = k_nano::interrupts::last_exc_count();
    let exc_age = k_nano::interrupts::last_exc_age_ms();
    let (st, val, pill) = if exc_n == 0 {
        (HubState::Ok, alloc::string::String::from("none"), false)
    } else if exc_age < 60_000 {
        (
            HubState::Warn,
            alloc::format!("{} x{} {}s", k_nano::interrupts::last_exc_name(), exc_n, exc_age / 1000),
            true,
        )
    } else {
        (
            HubState::Ok,
            alloc::format!("{} x{} old", k_nano::interrupts::last_exc_name(), exc_n),
            false,
        )
    };
    hub_set(&mut hh.rows[14], "fault", st, pill, val);

    // ── Live line + worst + checksum ──
    let wall = k_nano::interrupts::wall_ticks();
    let secs = wall / hz.max(1);
    let fw = k_nano::interrupts::FB_W.load(Ordering::Relaxed);
    let fh = k_nano::interrupts::FB_H.load(Ordering::Relaxed);
    let live = alloc::format!(
        "T+{}s tick {}Hz {}{} f{}.{}ms {}x{}",
        secs,
        hz,
        src,
        star,
        frame_us / 1000,
        (frame_us % 1000) / 100,
        fw,
        fh
    );
    let lb = live.as_bytes();
    let ln = lb.len().min(hh.live.len());
    hh.live[..ln].copy_from_slice(&lb[..ln]);
    hh.live_len = ln as u8;

    hh.worst = hub_worst(&hh.rows);
    // Amostra → HubHealthAgent (política do painel). Pior linha com pill.
    let mut worst_row: u8 = 0xFF;
    let mut worst_sev: u8 = 0;
    for (i, r) in hh.rows.iter().enumerate() {
        if r.pill {
            let s = r.state.sev_code();
            if s < 3 && s > worst_sev {
                worst_sev = s;
                worst_row = i as u8;
            }
        }
    }
    hermes::hub_health::set_sample(hh.worst.sev_code(), worst_row);
    let mut ck = 0x811C_9DC5u64;
    for r in hh.rows.iter() {
        ck = fnv1a(ck, r.label.as_bytes());
        ck = fnv1a(ck, r.value().as_bytes());
        ck = fnv1a(ck, &[r.state as u8, r.pill as u8]);
    }
    ck = fnv1a(ck, hh.live_line().as_bytes());
    hh.checksum = ck;

    *HUB.lock() = Some(hh);
}

#[cfg(test)]
mod hub_tests {
    use super::*;

    #[test]
    fn hub_worst_ignores_na_and_non_pill_rows() {
        let mut rows = [EMPTY_ROW; HUB_ROWS];
        // Só linhas Na (fora do pill): worst = Ok.
        assert_eq!(hub_worst(&rows), HubState::Ok);
        // Warn com pill puxa.
        rows[0].state = HubState::Warn;
        rows[0].pill = true;
        assert_eq!(hub_worst(&rows), HubState::Warn);
        // Fail puxa sobre Warn.
        rows[1].state = HubState::Fail;
        rows[1].pill = true;
        assert_eq!(hub_worst(&rows), HubState::Fail);
        // Linha fora do pill não puxa (fault histórico).
        rows[1].state = HubState::Ok;
        rows[2].state = HubState::Fail;
        rows[2].pill = false;
        assert_eq!(hub_worst(&rows), HubState::Warn);
    }

    #[test]
    fn hub_row_set_changes_state_and_truncates() {
        let mut a = HubRow { label: "", value: [0u8; HUB_ROW_LEN], vlen: 0, state: HubState::Na, pill: false };
        hub_set(&mut a, "xhci", HubState::Ok, true, alloc::string::String::from("c1 st10"));
        assert_eq!(a.value(), "c1 st10");
        assert_eq!(a.state, HubState::Ok);
        assert!(a.pill);
        // Truncagem: nunca estoura HUB_ROW_LEN.
        let long = alloc::string::String::from("x".repeat(HUB_ROW_LEN + 20));
        hub_set(&mut a, "big", HubState::Na, false, long);
        assert_eq!(a.value().len(), HUB_ROW_LEN);
    }

    #[test]
    fn hub_health_defaults_to_empty_na() {
        let h = HubHealth::empty();
        assert_eq!(h.rows.len(), HUB_ROWS);
        assert_eq!(h.worst, HubState::Na);
        assert_eq!(h.live_line(), "");
        for r in h.rows.iter() {
            assert_eq!(r.state, HubState::Na);
            assert_eq!(r.value(), "");
        }
    }
}


