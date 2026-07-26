//! DataCollector — coleta dados do sistema para auto-aprendizado (IDEA #313).
//! Subscreve EventBus, coleta logs e métricas, estrutura como pares (input, output)
//! para treino on-device.
//!
//! AIOS na veia: o sistema aprende dos próprios dados, sem internet, sem humano.

use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use event_bus::Receiver;
use k_nano::EVENT_BUS;

/// Um par de treino: (input, output) que o modelo pode aprender.
#[derive(Debug, Clone)]
pub struct TrainingPair {
    pub input: String,
    pub output: String,
    pub source: &'static str, // "hermes", "boot", "smart", etc.
    pub timestamp: u64,
}

/// Coletor de dados do sistema.
pub struct DataCollector {
    /// Buffer circular de pares de treino
    pairs: VecDeque<TrainingPair>,
    /// Capacidade máxima do buffer
    capacity: usize,
    /// Total de pares coletados desde o boot
    total_collected: u64,
    /// Receivers do EventBus
    hermes_rx: Receiver,
    intent_rx: Receiver,
    error_rx: Receiver,
    /// Input pendente (USER_INTENT) aguardando resposta (HERMES_RESPONSE)
    pending_input: Option<String>,
}

impl DataCollector {
    /// Cria um novo DataCollector com buffer de N pares.
    pub fn new(capacity: usize) -> Self {
        Self {
            pairs: VecDeque::with_capacity(capacity),
            capacity,
            total_collected: 0,
            hermes_rx: EVENT_BUS.subscribe("HERMES_RESPONSE"),
            intent_rx: EVENT_BUS.subscribe("USER_INTENT"),
            error_rx: EVENT_BUS.subscribe("KERNEL_ERROR"),
            pending_input: None,
        }
    }

    /// Poll todos os receivers e coleta novos pares.
    /// Deve ser chamado a cada tick do agente coletor.
    pub fn poll(&mut self, tick: u64) {
        // USER_INTENT → input
        while let Some(ev) = self.intent_rx.try_receive() {
            let text = core::str::from_utf8(&ev.payload).unwrap_or("");
            if !text.is_empty() {
                self.push_pending_input(text, tick);
            }
        }

        // HERMES_RESPONSE → output (match com input pendente)
        while let Some(ev) = self.hermes_rx.try_receive() {
            let response = core::str::from_utf8(&ev.payload).unwrap_or("");
            if !response.is_empty() {
                if let Some(input) = self.pop_pending_input() {
                    self.push(TrainingPair {
                        input,
                        output: String::from(response),
                        source: "hermes",
                        timestamp: tick,
                    });
                }
            }
        }

        // KERNEL_ERROR → par de debug (input=error msg, output=stack context)
        while let Some(ev) = self.error_rx.try_receive() {
            let error = core::str::from_utf8(&ev.payload).unwrap_or("unknown");
            self.push(TrainingPair {
                input: String::from(error),
                output: String::from("self-heal recovery attempted"),
                source: "kernel",
                timestamp: tick,
            });
        }
    }

    /// Retorna um snapshot dos pares atuais para export.
    pub fn snapshot(&self) -> Vec<TrainingPair> {
        self.pairs.iter().cloned().collect()
    }

    /// Exporta pares como JSON lines.
    pub fn export_jsonl(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for pair in &self.pairs {
            let escaped_input = pair.input.replace('"', "\\\"");
            let escaped_output = pair.output.replace('"', "\\\"");
            let line = alloc::format!(
                "{{\"input\":\"{}\",\"output\":\"{}\",\"source\":\"{}\",\"ts\":{}}}\n",
                escaped_input,
                escaped_output,
                pair.source,
                pair.timestamp
            );
            out.extend_from_slice(line.as_bytes());
        }
        out
    }

    /// Número de pares atualmente no buffer.
    pub fn count(&self) -> usize {
        self.pairs.len()
    }

    /// Total de pares coletados desde o boot.
    pub fn total(&self) -> u64 {
        self.total_collected
    }

    // ── Internals ──────────────────────────────────────────────

    fn push(&mut self, pair: TrainingPair) {
        if self.pairs.len() >= self.capacity {
            self.pairs.pop_front();
        }
        self.total_collected += 1;
        self.pairs.push_back(pair);
    }

    fn push_pending_input(&mut self, input: &str, _tick: u64) {
        // ponytail: single pending slot; upgrade to queue if pipelining needed
        self.pending_input = Some(String::from(input));
    }

    fn pop_pending_input(&mut self) -> Option<String> {
        self.pending_input.take()
    }
}

// ── Self-test ──────────────────────────────────────────────

/// Testa DataCollector: push/pop, poll sem EventBus ativo, export JSONL.
/// Retorna `true` se todos os checks passarem.
pub fn demo() -> bool {
    let mut dc = DataCollector::new(4);

    // 1. Push manual via push_pending + pop_pending + push
    dc.push_pending_input("hello", 1);
    let popped = dc.pop_pending_input();
    if popped != Some(String::from("hello")) {
        return false;
    }
    dc.push(TrainingPair {
        input: String::from("hello"),
        output: String::from("hi there"),
        source: "hermes",
        timestamp: 1,
    });
    if dc.count() != 1 || dc.total() != 1 {
        return false;
    }

    // 2. Push até encher buffer e verificar evicção circular
    dc.push(TrainingPair {
        input: String::from("a"),
        output: String::from("1"),
        source: "test",
        timestamp: 2,
    });
    dc.push(TrainingPair {
        input: String::from("b"),
        output: String::from("2"),
        source: "test",
        timestamp: 3,
    });
    dc.push(TrainingPair {
        input: String::from("c"),
        output: String::from("3"),
        source: "test",
        timestamp: 4,
    });
    if dc.count() != 4 || dc.total() != 4 {
        return false;
    }
    // Um push extra → evicção do mais antigo ("hello")
    dc.push(TrainingPair {
        input: String::from("d"),
        output: String::from("4"),
        source: "test",
        timestamp: 5,
    });
    if dc.count() != 4 || dc.total() != 5 {
        return false;
    }
    let snapshot = dc.snapshot();
    if snapshot[0].input != "a" {
        // "hello" foi evictado
        return false;
    }

    // 3. Export JSONL e verificar estrutura
    let jsonl = dc.export_jsonl();
    let text = core::str::from_utf8(&jsonl).unwrap_or("");
    if !text.contains("\"input\":\"a\"") || !text.contains("\"output\":\"4\"") {
        return false;
    }

    // 4. Poll sem EventBus ativo (basta não crashar)
    dc.poll(99);
    // Nenhum evento publicado → nada deve mudar
    if dc.count() != 4 {
        return false;
    }

    // 5. Linha especial: erro vazio não adiciona par
    dc.push_pending_input("", 0);
    let empty = dc.pop_pending_input();
    if empty != Some(String::new()) {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_collector_demo() {
        assert!(demo());
    }
}
