//! MetricsAgent — amostra CPU/MEM/GPU/HD a cada ~0,5s para o HUD Jarbas.
//! Compositor só lê o snapshot (sem amostrar no hot path de frame).

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use core::sync::atomic::Ordering;

/// ~18 Hz PIT → 0,5 s ≈ 9 ticks de scheduler (halt/tick alinhado ao timer).
pub const METRICS_POLL_TICKS: u64 = 9;
/// Gate wall-clock extra (TIMER_TICKS / 18 ≈ segundos).
const METRICS_PERIOD_TIMER: usize = 9; // 0.5 * 18
/// Log serial a cada N amostras (~10s) — evita flood COM no HW.
const LOG_EVERY_N: u32 = 20;

const METRICS_MANIFEST: AgentManifest = AgentManifest {
    name: "sys_metrics",
    kind: AgentKind::System,
    schedule: ScheduleKind::PollEvery(METRICS_POLL_TICKS),
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
        k_nano::slog_jarbas!("Metrics", "info", "MetricsAgent ativo — refresh a cada ~0.5s");
    }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        let now = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed);
        if self.last_timer != 0 && now.wrapping_sub(self.last_timer) < METRICS_PERIOD_TIMER {
            return AgentTickResult::Pending;
        }
        self.last_timer = now;
        self.samples = self.samples.wrapping_add(1);
        let log = self.samples % LOG_EVERY_N == 0;
        crate::display::gauges::refresh_snapshot(log);
        AgentTickResult::Pending
    }
}
