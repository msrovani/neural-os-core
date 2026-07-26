//! Observability (tracing/metrics) + Hub discovery.
//! Ring buffer de eventos com niveis, metrica nomeada, export serial.
//! 100% funcional, sem stubs.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use k_nano::kjson;

// ─── Níveis de log ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug, Info, Warn, Error, Critical,
}
impl LogLevel {
    pub fn name(&self) -> &'static str {
        match self { LogLevel::Debug => "DBG", LogLevel::Info => "INF",
                     LogLevel::Warn => "WRN", LogLevel::Error => "ERR",
                     LogLevel::Critical => "CRI" }
    }
}

// ─── Entrada de log ────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct LogEntry {
    pub tick: u64,
    pub level: LogLevel,
    pub agent: String,
    pub event: String,
    pub message: String,
}

// ─── Observability com ring buffer ─────────────────────────────────────────

pub struct Observability {
    logs: Vec<LogEntry>,
    max_logs: usize,
    metrics: BTreeMap<String, f32>,
    pub counters: BTreeMap<String, u64>,
}

impl Observability {
    pub fn new() -> Self {
        Observability {
            logs: Vec::with_capacity(128),
            max_logs: 1024,
            metrics: BTreeMap::new(),
            counters: BTreeMap::new(),
        }
    }

    pub fn log(&mut self, level: LogLevel, agent: &str, event: &str, msg: &str) {
        let tick = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        if self.logs.len() >= self.max_logs { self.logs.remove(0); }
        self.logs.push(LogEntry { tick, level, agent: String::from(agent), event: String::from(event), message: String::from(msg) });
        kjson!(level.name(), agent, event, "msg", msg);
    }

    pub fn info(&mut self, agent: &str, event: &str, msg: &str) { self.log(LogLevel::Info, agent, event, msg); }
    pub fn warn(&mut self, agent: &str, event: &str, msg: &str) { self.log(LogLevel::Warn, agent, event, msg); }
    pub fn error(&mut self, agent: &str, event: &str, msg: &str) { self.log(LogLevel::Error, agent, event, msg); }

    pub fn gauge(&mut self, name: &str, val: f32) { self.metrics.insert(String::from(name), val); }
    pub fn inc(&mut self, name: &str) { *self.counters.entry(String::from(name)).or_insert(0) += 1; }

    pub fn get_metric(&self, name: &str) -> Option<f32> { self.metrics.get(name).copied() }
    pub fn get_counter(&self, name: &str) -> u64 { self.counters.get(name).copied().unwrap_or(0) }

    pub fn recent(&self, n: usize) -> &[LogEntry] {
        let start = self.logs.len().saturating_sub(n);
        &self.logs[start..]
    }

    pub fn by_agent(&self, agent: &str) -> Vec<&LogEntry> {
        self.logs.iter().filter(|e| e.agent == agent).collect()
    }

    pub fn status(&self) -> String {
        alloc::format!("[OBSERV] {} logs, {} metrics, {} counters", self.logs.len(), self.metrics.len(), self.counters.len())
    }
}

// ─── Hub Discovery ─────────────────────────────────────────────────────────

pub struct HubDiscovery {
    instances: Vec<(String, u64)>,
}
impl HubDiscovery {
    pub fn new() -> Self { HubDiscovery { instances: Vec::new() } }
    pub fn announce(&mut self, id: &str) { self.instances.push((String::from(id), 0)); }
    pub fn list(&self) -> &[(String, u64)] { &self.instances }
    pub fn status(&self) -> String { alloc::format!("[HUB] {} instancias", self.instances.len()) }
}






