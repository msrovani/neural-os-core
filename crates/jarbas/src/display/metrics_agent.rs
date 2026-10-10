//! MetricsAgent — amostra inicial p/ o HUD Jarbas (Oneshot).
//! O refresh periódico (~0,5s) vive no tick do compositor (`render_inner`),
//! que chama `gauges::refresh_snapshot` a cada METRICS_POLL_TICKS.
//! Compositor só lê o snapshot (sem amostrar no hot path de frame).

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use core::sync::atomic::Ordering;

/// Tick do compositor entre refreshes (~18 Hz PIT → 0,5 s ≈ 9 ticks).
pub const METRICS_POLL_TICKS: u64 = 9;

const METRICS_MANIFEST: AgentManifest = AgentManifest {
    name: "sys_metrics",
    kind: AgentKind::System,
    schedule: ScheduleKind::Oneshot,
    auto_start: true,
    persist: true,
};

pub struct MetricsAgent {
    last_timer: usize,
    samples: u32,
}

impl MetricsAgent {
    pub fn new() -> Self {
        MetricsAgent {
            last_timer: 0,
            samples: 0,
        }
    }
}

impl Agent for MetricsAgent {
    fn manifest(&self) -> &AgentManifest {
        &METRICS_MANIFEST
    }

    fn on_activate(&mut self) {
        // Primeira amostra imediata — HUD não fica vazio até o 1º poll.
        crate::display::gauges::refresh_snapshot(true);
        self.last_timer = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed);
        self.samples = 1;
        k_nano::slog_jarbas!("Metrics", "ok", "MetricsAgent ativo — 1a amostra ok (refresh no tick do compositor)");
    }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        // Oneshot: a 1ª amostra já saiu no on_activate; o refresh periódico
        // é feito pelo tick do compositor. Done imediato (sem Pending eterno).
        AgentTickResult::Done
    }
}
