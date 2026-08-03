//! SysInfoAgent — painel de debug do sistema como card Jarbas.
//! Coleta dados de CPU, memória, agentes, uptime e rede de fontes
//! lock-free (atômicas) e atualiza um card na tela a cada ~50 ticks.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use alloc::string::String;
use alloc::format;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;
use jarbas_crate::display::card::{UiDeclaration, Widget};
use jarbas_crate::display::window::WindowContent;
use jarbas_crate::display::compositor::COMPOSITOR;

const CARD_ID: u32 = 9001;
const CARD_TITLE: &str = "SysInfo";
const CARD_X: i32 = 60;
const CARD_Y: i32 = 92;
const CARD_W: i32 = 340;
const CARD_H: i32 = 360;

const SYSINFO_MANIFEST: AgentManifest = AgentManifest {
    name: "sysinfo",
    kind: AgentKind::System,
    schedule: ScheduleKind::PollEvery(50),
    auto_start: true,
    persist: false,
};

pub struct SysInfoAgent {
    spawned: bool,
}

impl SysInfoAgent {
    pub fn new() -> Self {
        SysInfoAgent { spawned: false }
    }

    fn collect_body(&self) -> Vec<Widget> {
        let mut body = Vec::new();

        // ── CPU ──
        let cores = k_nano::smp::total_cores();
        let ap_on = k_nano::smp::percpu::AP_ONLINE.load(Ordering::Relaxed);
        let ap_cnt = k_nano::smp::AP_COUNT.load(Ordering::Relaxed);
        let pool = k_nano::smp::corepools::pools();
        let pool_str = if let Some(p) = pool {
            format!("R0:{} R1:{} R2:{}", p.ring0_len, p.ring1_len, p.ring2_len)
        } else {
            String::from("N/A")
        };
        body.push(Widget::KeyValue(
            String::from("CPU"),
            format!("{} cores (AP {}/{} {})", cores, ap_on, ap_cnt, pool_str),
        ));

        // ── Memória ──
        let ram_mb = k_nano::memory::TOTAL_RAM_MB.load(Ordering::Relaxed);
        let heap_mb = k_nano::allocator::CURRENT_HEAP_MB.load(Ordering::Relaxed);
        let heap_used_mb = k_nano::allocator::heap_used_bytes() / (1024 * 1024);
        // Frame allocator
        let frames = k_nano::memory::GLOBAL_ALLOCATOR.lock();
        let (frame_pct, _frame_used, _frame_total) = if let Some(fa) = frames.as_ref() {
            let pct = if fa.usable_frames > 0 {
                fa.allocated_count as f32 / fa.usable_frames as f32 * 100.0
            } else { 0.0 };
            (pct, fa.allocated_count, fa.usable_frames)
        } else { (0.0, 0, 0) };
        drop(frames);
        body.push(Widget::KeyValue(
            String::from("RAM"),
            format!("{}MB total, heap {}M/{}M used", ram_mb, heap_used_mb, heap_mb),
        ));
        body.push(Widget::Gauge {
            label: String::from("Frames"),
            value: frame_pct as i32,
            max: 100,
            unit: String::from("%"),
        });

        // ── Agentes ──
        let n_ag = agent_core::LAST_SCHED_AGENTS.load(Ordering::Relaxed);
        let n_poll = agent_core::LAST_SCHED_POLLED.load(Ordering::Relaxed);
        body.push(Widget::KeyValue(
            String::from("Agents"),
            format!("{} total, {} polled/tick", n_ag, n_poll),
        ));

        // ── Uptime ──
        let ticks = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed);
        let uptime_s = ticks / 18;
        let uptime_m = uptime_s / 60;
        let uptime_h = uptime_m / 60;
        body.push(Widget::KeyValue(
            String::from("Uptime"),
            format!("{}h{}m ({} ticks)", uptime_h, uptime_m % 60, ticks),
        ));

        // ── Network ──
        let net = crate::net::NET_CONFIG.lock();
        let net_str = if net.configured && net.online {
            format!("IP {}.{}.{}.{} online", net.ip[0], net.ip[1], net.ip[2], net.ip[3])
        } else {
            String::from("offline")
        };
        drop(net);
        body.push(Widget::KeyValue(String::from("Net"), net_str));

        // ── Storage ──
        let fat_ready = k_nano::boot_logger::FAT_READY.load(Ordering::Relaxed);
        let storage_bus = k_nano::storage_bus::STORAGE_BUS.lock();
        let disk_count = storage_bus.entries().len();
        drop(storage_bus);

        let pf_count = k_nano::interrupts::PAGE_FAULT_COUNT.load(Ordering::Relaxed);
        body.push(Widget::KeyValue(
            String::from("Storage"),
            format!("FAT:{} {} disks PF:{}",
                if fat_ready { "Y" } else { "N" },
                disk_count, pf_count),
        ));

        body
    }
}

impl Agent for SysInfoAgent {
    fn manifest(&self) -> &AgentManifest {
        &SYSINFO_MANIFEST
    }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        // Retry BOOT.LOG flush se ainda não foi escrito (USB-MSC pode demorar
        // a enumerar em HW real). O SysInfoAgent roda a cada 50 ticks (~2.7s).
        if !k_nano::boot_logger::FAT_READY.load(Ordering::Relaxed) {
            crate::boot_logger::flush();
        }

        // Só tenta spawn se o compositor já existe (DisplayAgent precisa ter
        // inicializado antes). Se não, tenta de novo no próximo tick.
        if let Some(ref mut desktop) = *COMPOSITOR.lock() {
            if !self.spawned {
                // First successful spawn
                let body = self.collect_body();
                let mut decl = UiDeclaration::new(
                    CARD_ID,
                    CARD_TITLE,
                    CARD_X, CARD_Y, CARD_W, CARD_H,
                );
                decl.body = body;
                desktop.spawn_card(decl);
                self.spawned = true;
            } else {
                // Subsequent ticks: update card body in-place
                let body = self.collect_body();
                for win in &mut desktop.windows {
                    if let WindowContent::Card(ref mut existing) = win.content {
                        if existing.id == CARD_ID {
                            existing.body = body;
                            break;
                        }
                    }
                }
            }
        }

        AgentTickResult::Pending
    }
}
