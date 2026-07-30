//! SelfLearningAgent — IDEA #313: Self-Learning OS pipeline.
//!
//! # Pipeline
//! 1. `DataCollector` (EventBus `HERMES_RESPONSE` + `USER_INTENT` + `KERNEL_ERROR`)
//! 2. `TrainingAgent` (`BitNetTrainer` on-device ternary fine-tuning)
//! 3. `ModelHub` (marca slot `Learner` como carregado)
//!
//! PollEvery(5000): coleta dados do EventBus, converte pares texto → embeddings f32,
//! fine-tuna pesos ternários placeholder, registra modelo no ModelHub.
//! O sistema aprende dos próprios dados — sem internet, sem humano.

use agent_core::{Agent, AgentKind, AgentManifest, AgentTickResult, ScheduleKind};
use alloc::vec::Vec;
use crate::data_collector::DataCollector;
use crate::data_collector::TrainingPair;
use crate::training_agent::TrainingAgent;

const MANIFEST: AgentManifest = AgentManifest {
    name: "self-learning",
    kind: AgentKind::System,
    schedule: ScheduleKind::PollEvery(5000),
    auto_start: true,
    persist: false,
};

/// Agente de auto-aprendizado contínuo.
///
/// Coleta pares (input, output) do sistema via EventBus, converte para
/// representação vetorial f32, e fine-tuna um modelo ternário placeholder.
pub struct SelfLearningAgent {
    /// Coletor de dados do EventBus
    pub collector: DataCollector,
    /// Trainer com BitNetTrainer para fine-tuning on-device
    pub trainer: TrainingAgent,
    /// Contador de ciclos de aprendizado
    pub cycle: u32,
    /// Loss do último fine-tuning
    pub last_loss: f32,
    /// Pesos ternários placeholder (64 elementos)
    weights: Vec<i8>,
}

impl SelfLearningAgent {
    /// Cria novo agente com buffer de 1000 pares e pesos placeholder 64-dim.
    pub fn new() -> Self {
        Self {
            collector: DataCollector::new(1000),
            trainer: TrainingAgent::new(),
            cycle: 0,
            last_loss: 0.0,
            weights: alloc::vec![0i8; 64],
        }
    }

    /// Ciclo principal de aprendizado:
    /// 1. Poll EventBus → coleta pares (input, output)
    /// 2. Converte strings → embeddings f32 (byte encoding)
    /// 3. Fine-tune pesos ternários (2 epochs)
    /// 4. Marca slot Learner no ModelHub
    /// 5. Retorna loss média (0.0 se sem dados)
    pub fn learn_tick(&mut self) -> f32 {
        self.cycle += 1;

        // 1. Poll EventBus receivers
        self.collector.poll(self.cycle as u64);
        let pairs = self.collector.snapshot();
        if pairs.is_empty() {
            return 0.0;
        }

        // 2. Convert string pairs → float embeddings
        let data = Self::pairs_to_data(&pairs);

        // 3. Fine-tune on-device
        let loss = self.trainer.fine_tune(&mut self.weights, &data, 2);
        self.last_loss = loss;

        // 4. Mark Learner slot in ModelHub
        cortex::model_hub::mark_slot(cortex::model_hub::ModelSlot::Learner, true);

        k_nano::slog_kai!("SELF-LEARN", "info",
            "cycle={} samples={} loss={:.4}",
            self.cycle, pairs.len(), loss);

        loss
    }

    /// Converte TrainingPair → (Vec<f32>, Vec<f32>) via byte embedding.
    /// Cada byte do input/output é mapeado para f32 em [0, 1].
    fn pairs_to_data(pairs: &[TrainingPair]) -> Vec<(Vec<f32>, Vec<f32>)> {
        let mut data = Vec::with_capacity(pairs.len());
        for pair in pairs {
            let mut input = alloc::vec![0.0f32; 64];
            let mut target = alloc::vec![0.0f32; 64];
            for (i, b) in pair.input.bytes().take(64).enumerate() {
                input[i] = (b as f32) / 255.0;
            }
            for (i, b) in pair.output.bytes().take(64).enumerate() {
                target[i] = (b as f32) / 255.0;
            }
            data.push((input, target));
        }
        data
    }
}

impl Agent for SelfLearningAgent {
    fn manifest(&self) -> &AgentManifest {
        &MANIFEST
    }

    fn tick(&mut self, _tick: u64, _tick_count: u64) -> AgentTickResult {
        self.learn_tick();
        AgentTickResult::Done
    }

    fn on_activate(&mut self) {
        k_nano::slog_kai!("SELF-LEARN", "info", "SelfLearningAgent activated");
    }
}

// ── Self-test ──────────────────────────────────────────────

/// Testa SelfLearningAgent: criação, learn_tick sem eventos, ciclo.
pub fn demo() -> bool {
    let mut agent = SelfLearningAgent::new();

    // Nenhum evento publicado → snapshot vazio → loss = 0.0
    let loss = agent.learn_tick();
    if loss != 0.0 {
        return false;
    }
    if agent.cycle != 1 {
        return false;
    }

    // Verifica pesos placeholder intactos
    if agent.weights.len() != 64 {
        return false;
    }
    // Todos os pesos inicialmente zero
    if agent.weights.iter().any(|&w| w != 0) {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_learning_demo() {
        assert!(demo());
    }
}
