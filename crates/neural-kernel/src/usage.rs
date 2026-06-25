use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

pub struct UsageSnapshot {
    pub total_calls: u64,
    pub by_skill: Vec<(String, u64)>,
    pub total_exec_time_ticks: u64,
}

pub struct UsageTracker {
    call_counts: BTreeMap<String, u64>,
    exec_time: BTreeMap<String, u64>,
    start_tick: u64,
}

impl UsageTracker {
    pub const fn new() -> Self {
        UsageTracker {
            call_counts: BTreeMap::new(),
            exec_time: BTreeMap::new(),
            start_tick: 0,
        }
    }

    pub fn init(&mut self, now_ticks: u64) {
        self.start_tick = now_ticks;
    }

    pub fn record_call(&mut self, skill_name: &str, duration_ticks: u64) {
        let count = self.call_counts.entry(String::from(skill_name)).or_insert(0);
        *count += 1;
        let time = self.exec_time.entry(String::from(skill_name)).or_insert(0);
        *time += duration_ticks;
    }

    pub fn snapshot(&self) -> UsageSnapshot {
        let mut by_skill = Vec::new();
        for (name, count) in &self.call_counts {
            by_skill.push((name.clone(), *count));
        }
        let total: u64 = self.call_counts.values().sum();
        let total_time: u64 = self.exec_time.values().sum();
        UsageSnapshot {
            total_calls: total,
            by_skill,
            total_exec_time_ticks: total_time,
        }
    }

    pub fn to_metrics_tensor(&self) -> [f32; 4] {
        let snap = self.snapshot();
        let total = core::cmp::max(snap.total_calls, 1) as f32;
        let call_rate = self.call_counts.len() as f32 / total;
        let time_per_call = if snap.total_calls > 0 {
            snap.total_exec_time_ticks as f32 / snap.total_calls as f32
        } else {
            0.0
        };
        [total, call_rate, time_per_call, self.start_tick as f32]
    }
}

static USAGE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn record_event() {
    USAGE_COUNTER.fetch_add(1, Ordering::Relaxed);
}

pub fn event_count() -> u64 {
    USAGE_COUNTER.load(Ordering::Relaxed)
}
