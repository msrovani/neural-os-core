//! #312b FineTuningPipeline — on-device fine-tuning using LEARNER.BIN.
//! Connects DataCollector (EventBus system data) with TrainingAgent (BitNetTrainer)
//! to run fine-tuning cycles on collected system data.
//!
//! Usage:
//!   let mut pipe = FineTuningPipeline::new(64);
//!   let loss = pipe.run_cycle(&mut weights);

use crate::data_collector::DataCollector;
use crate::training_agent::TrainingAgent;

/// Pipeline that connects system data collection to on-device fine-tuning.
pub struct FineTuningPipeline {
    collector: DataCollector,
    trainer: TrainingAgent,
}

impl FineTuningPipeline {
    /// Create a new pipeline with a DataCollector of given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            collector: DataCollector::new(capacity),
            trainer: TrainingAgent::new(),
        }
    }

    /// Run one fine-tuning cycle: collect data, train 3 epochs, return average loss.
    /// `model_weights` are mutated in place (ternary update).
    pub fn run_cycle(&mut self, model_weights: &mut [i8]) -> f32 {
        let data = self.collector.collect();
        if data.is_empty() {
            return 0.0;
        }
        let loss = self.trainer.fine_tune(model_weights, &data, 3);
        loss
    }

    /// Replace the collector with a new one of given capacity (drops old data).
    pub fn set_capacity(&mut self, n: usize) {
        self.collector = DataCollector::new(n);
    }

    /// Access the collector for manual poll/record.
    pub fn collector_mut(&mut self) -> &mut DataCollector {
        &mut self.collector
    }

    /// Access the trainer for status inspection.
    pub fn trainer(&self) -> &TrainingAgent {
        &self.trainer
    }
}

/// Convenience API: creates a pipeline with 64-sample buffer, runs one cycle.
/// Returns the average loss.
pub fn run_fine_tuning_cycle(weights: &mut [i8]) -> f32 {
    let mut pipeline = FineTuningPipeline::new(64);
    pipeline.run_cycle(weights)
}
