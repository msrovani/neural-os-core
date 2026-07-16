//! #312 TrainingAgent — on-device fine-tuning pipeline.
//! Usa BitNetTrainer (cognitive.rs) para treino ternario on-device.

use alloc::vec::Vec;
use alloc::string::String;
use crate::cognitive::BitNetTrainer;
use k_nano::kjson;

pub struct TrainingAgent {
    pub trainer: BitNetTrainer,
    pub trained_count: u64,
    pub last_loss: f32,
}

impl TrainingAgent {
    pub fn new() -> Self {
        TrainingAgent {
            trainer: BitNetTrainer::new(),
            trained_count: 0,
            last_loss: 0.0,
        }
    }

    pub fn train_step(&mut self, weights: &mut [i8], inputs: &[f32], targets: &[f32]) -> f32 {
        let loss = self.trainer.train_step(weights, inputs, targets);
        self.trained_count += 1;
        self.last_loss = loss;
        loss
    }

    pub fn fine_tune(&mut self, weights: &mut [i8], data: &[(Vec<f32>, Vec<f32>)], epochs: usize) -> f32 {
        let mut avg_loss = 0.0f32;
        for epoch in 0..epochs {
            let mut epoch_loss = 0.0f32;
            for (input, target) in data {
                epoch_loss += self.train_step(weights, input, target);
            }
            avg_loss = epoch_loss / data.len() as f32;
            kjson!("TRAIN", "EPOCH", "done", "e", epoch, "loss", avg_loss);
        }
        avg_loss
    }

    pub fn status(&self) -> String {
        alloc::format!("[TRAIN] {} steps, loss={:.4}", self.trained_count, self.last_loss)
    }
}

pub struct DataCollector {
    pub replay_buffer: Vec<(Vec<f32>, Vec<f32>)>,
    pub max_samples: usize,
}

impl DataCollector {
    pub fn new(max: usize) -> Self {
        DataCollector { replay_buffer: Vec::with_capacity(max), max_samples: max }
    }

    pub fn record(&mut self, input: Vec<f32>, target: Vec<f32>) {
        if self.replay_buffer.len() >= self.max_samples {
            self.replay_buffer.remove(0);
        }
        self.replay_buffer.push((input, target));
    }

    pub fn collect_from_system(&mut self) {
        let sources = crate::cognitive::get_training_sources();
        for s in sources {
            // Features derivadas do nome/fonte — não vec![0.5] constante
            let mut emb = alloc::vec![0.0f32; 64];
            let mut tgt = alloc::vec![0.0f32; 64];
            for (i, b) in s.name.bytes().enumerate() {
                if i >= 64 { break; }
                emb[i] = (b as f32) / 255.0;
                tgt[i] = 1.0 - emb[i];
            }
            let h = s.name.len() as f32;
            if emb.len() > 0 {
                emb[0] = (h % 64.0) / 64.0;
            }
            self.record(emb, tgt);
            kjson!("COLLECT", &s.name, "samples", "n", self.replay_buffer.len());
        }
    }

    pub fn status(&self) -> String {
        alloc::format!("[COLLECT] {} samples / {} max", self.replay_buffer.len(), self.max_samples)
    }
}
