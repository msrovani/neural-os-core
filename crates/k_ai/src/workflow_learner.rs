//! Workflow Learner — Usage Pattern Analyzer + Workflow Predictor (IDEA #157-#158).
//! Observa intenções do usuário, classifica workflows, e prediz necessidades de recursos.
//!
//! AIOS na veia: o sistema aprende a rotina do usuário e adapta recursos automaticamente.

use alloc::vec::Vec;
use alloc::string::String;
use core::sync::atomic::{AtomicU64, Ordering};

/// Workflows detectáveis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkflowKind {
    Development,   // coding, IDE, compile
    Office,        // documents, email, calendar
    Media,         // music, video, audio
    Communication, // chat, call, messaging
    Gaming,        // games, graphics
    System,        // settings, config, monitoring
    Idle,          // no activity
    Unknown,
}

impl WorkflowKind {
    pub fn from_intent(intent: &str) -> Self {
        let lower = intent.to_lowercase();
        if lower.contains("code") || lower.contains("rust") || lower.contains("ide")
            || lower.contains("compile") || lower.contains("skill") {
            WorkflowKind::Development
        } else if lower.contains("email") || lower.contains("document")
            || lower.contains("file") || lower.contains("save") {
            WorkflowKind::Office
        } else if lower.contains("music") || lower.contains("audio")
            || lower.contains("play") || lower.contains("video") {
            WorkflowKind::Media
        } else if lower.contains("chat") || lower.contains("call")
            || lower.contains("message") || lower.contains("hello") {
            WorkflowKind::Communication
        } else if lower.contains("game") || lower.contains("gpu") {
            WorkflowKind::Gaming
        } else if lower.contains("setting") || lower.contains("config")
            || lower.contains("status") || lower.contains("help") {
            WorkflowKind::System
        } else {
            WorkflowKind::Unknown
        }
    }
}

/// Uma observação de intenção do usuário.
#[derive(Debug, Clone)]
pub struct IntentObservation {
    pub intent: String,
    pub workflow: WorkflowKind,
    pub tick: u64,
    pub hour: u8,      // 0-23
    pub day_of_week: u8, // 0=Sunday
}

/// Analisador de padrões de uso.
pub struct UsagePatternAnalyzer {
    /// Observações recentes (últimas N intenções)
    observations: Vec<IntentObservation>,
    /// Capacidade máxima
    capacity: usize,
    /// Contagem por workflow
    workflow_counts: [AtomicU64; 8], // um por WorkflowKind
    /// Workflow atual detectado
    current: WorkflowKind,
    /// Workflow mais comum por hora do dia
    hourly_pattern: [WorkflowKind; 24],
}

impl UsagePatternAnalyzer {
    pub fn new(capacity: usize) -> Self {
        let hourly = [WorkflowKind::Unknown; 24];
        Self {
            observations: Vec::with_capacity(capacity),
            capacity,
            workflow_counts: Default::default(),
            current: WorkflowKind::Unknown,
            hourly_pattern: hourly,
        }
    }

    /// Registra uma intenção do usuário.
    pub fn observe(&mut self, intent: &str, tick: u64) {
        let hour = ((tick / 3600000) % 24) as u8; // ~1000 ticks/sec
        let day = ((tick / 86400000) % 7) as u8;
        let workflow = WorkflowKind::from_intent(intent);
        
        if self.observations.len() >= self.capacity {
            self.observations.remove(0);
        }
        self.observations.push(IntentObservation {
            intent: String::from(intent),
            workflow,
            tick,
            hour,
            day_of_week: day,
        });
        
        let idx = workflow as usize;
        if idx < 8 {
            self.workflow_counts[idx].fetch_add(1, Ordering::Relaxed);
        }
        
        self.update_current();
        self.update_hourly_pattern(hour, workflow);
    }

    /// Workflow atual detectado.
    pub fn current_workflow(&self) -> WorkflowKind { self.current }

    /// Workflow previsto para esta hora.
    pub fn predicted_workflow(&self, hour: u8) -> WorkflowKind {
        self.hourly_pattern[hour as usize]
    }

    /// Confiança no workflow atual (0.0 a 1.0).
    pub fn confidence(&self) -> f32 {
        let total: u64 = self.workflow_counts.iter().map(|c| c.load(Ordering::Relaxed)).sum();
        if total == 0 { return 0.0; }
        let current_idx = self.current as usize;
        let current_count = if current_idx < 8 { self.workflow_counts[current_idx].load(Ordering::Relaxed) } else { 0 };
        current_count as f32 / total as f32
    }

    fn update_current(&mut self) {
        // Pega o workflow mais frequente nas últimas observações
        let mut counts = [0u32; 8];
        for obs in &self.observations {
            let idx = obs.workflow as usize;
            if idx < 8 { counts[idx] += 1; }
        }
        let mut max = 0;
        let mut best = WorkflowKind::Unknown;
        for (i, &c) in counts.iter().enumerate() {
            if c > max {
                max = c;
                best = match i {
                    0 => WorkflowKind::Development,
                    1 => WorkflowKind::Office,
                    2 => WorkflowKind::Media,
                    3 => WorkflowKind::Communication,
                    4 => WorkflowKind::Gaming,
                    5 => WorkflowKind::System,
                    6 => WorkflowKind::Idle,
                    _ => WorkflowKind::Unknown,
                };
            }
        }
        self.current = best;
    }

    fn update_hourly_pattern(&mut self, hour: u8, workflow: WorkflowKind) {
        // Simplificado: última workflow vista nesta hora vence
        self.hourly_pattern[hour as usize] = workflow;
    }
}
