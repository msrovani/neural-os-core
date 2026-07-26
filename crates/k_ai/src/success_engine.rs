//! Success Engine — feedback loop (IDEA #149–#152).
//! Coleta 👍/👎 do usuário, mantém buffer de experiência,
//! alimenta SleepCycle REPLAY para fine-tuning on-device.

use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

/// Feedback do usuário sobre uma resposta do Hermes.
#[derive(Debug, Clone)]
pub struct Feedback {
    pub input: String,
    pub response: String,
    pub positive: bool,
    pub timestamp: u64,
}

/// Gerenciador do ciclo de feedback.
pub struct SuccessEngine {
    /// Buffer circular com últimos N feedbacks
    buffer: VecDeque<Feedback>,
    /// Capacidade máxima
    capacity: usize,
    /// Total de feedbacks recebidos (para estatística)
    pub total_feedback: AtomicU64,
    /// Total de 👍
    pub thumbs_up: AtomicU64,
    /// Total de 👎
    pub thumbs_down: AtomicU64,
}

impl SuccessEngine {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
            total_feedback: AtomicU64::new(0),
            thumbs_up: AtomicU64::new(0),
            thumbs_down: AtomicU64::new(0),
        }
    }

    /// Registra um feedback.
    pub fn record(&mut self, input: &str, response: &str, positive: bool, tick: u64) {
        if self.buffer.len() >= self.capacity {
            self.buffer.pop_front();
        }
        self.total_feedback.fetch_add(1, Ordering::Relaxed);
        if positive {
            self.thumbs_up.fetch_add(1, Ordering::Relaxed);
        } else {
            self.thumbs_down.fetch_add(1, Ordering::Relaxed);
        }
        self.buffer.push_back(Feedback {
            input: String::from(input),
            response: String::from(response),
            positive,
            timestamp: tick,
        });
    }

    /// Retorna amostras para o SleepCycle REPLAY processar.
    /// Pega N amostras mais recentes do buffer (ou todas se < N).
    pub fn replay_samples(&self, n: usize) -> Vec<Feedback> {
        self.buffer.iter().rev().take(n).cloned().collect()
    }

    /// Taxa de aprovação (0.0 a 1.0).
    pub fn approval_rate(&self) -> f32 {
        let total = self.total_feedback.load(Ordering::Relaxed);
        if total == 0 {
            return 0.5;
        }
        let up = self.thumbs_up.load(Ordering::Relaxed);
        up as f32 / total as f32
    }

    pub fn feedback_count(&self) -> usize {
        self.buffer.len()
    }
}

// ── Self-test ──────────────────────────────────────────────

/// Testa SuccessEngine: record, replay, approval_rate, evicção circular.
pub fn self_test() -> bool {
    let mut eng = SuccessEngine::new(3);

    // 1. Record alguns feedbacks
    eng.record("ping", "pong", true, 1);
    eng.record("1+1", "2", false, 2);
    eng.record("cor?", "azul", true, 3);
    if eng.feedback_count() != 3 {
        return false;
    }

    // 2. Taxa de aprovação
    let rate = eng.approval_rate();
    if (rate - 2.0 / 3.0).abs() > 0.001 {
        return false;
    }

    // 3. Buffer cheio → evicção do mais antigo
    eng.record("time?", "manha", true, 4);
    if eng.feedback_count() != 3 {
        return false;
    }
    // "ping" foi evictado, "1+1" é o mais antigo agora
    let samples = eng.replay_samples(10);
    if samples.len() != 3 {
        return false;
    }
    // Mais recente primeiro
    if samples[0].input != "time?" || samples[2].input != "1+1" {
        return false;
    }

    // 4. Contadores atômicos
    if eng.total_feedback.load(Ordering::Relaxed) != 4 {
        return false;
    }
    if eng.thumbs_up.load(Ordering::Relaxed) != 3 {
        return false;
    }
    if eng.thumbs_down.load(Ordering::Relaxed) != 1 {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_engine_works() {
        assert!(self_test());
    }
}
