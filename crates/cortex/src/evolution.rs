use alloc::vec::Vec;
use alloc::vec;

pub struct PlasticityController {
    pub region_entropy: Vec<f32>,
    pub region_error_rate: Vec<f32>,
    pub region_activation: Vec<f64>,
    pub growth_threshold: f32,
    pub prune_threshold: f32,
    pub region_hits: Vec<u64>,
    tick: u64,
}

impl PlasticityController {
    pub fn new(num_regions: usize, growth_threshold: f32, prune_threshold: f32) -> Self {
        PlasticityController {
            region_entropy: vec![0.0; num_regions],
            region_error_rate: vec![0.0; num_regions],
            region_activation: vec![0.0; num_regions],
            growth_threshold,
            prune_threshold,
            region_hits: vec![0; num_regions],
            tick: 0,
        }
    }

    pub fn observe(&mut self, region: usize, entropy: f32, error: f32, activated: f64) {
        if region >= self.region_entropy.len() { return; }
        self.region_entropy[region] = self.region_entropy[region] * 0.9 + entropy * 0.1;
        self.region_error_rate[region] = self.region_error_rate[region] * 0.9 + error * 0.1;
        self.region_activation[region] = self.region_activation[region] * 0.95 + activated * 0.05;
        self.region_hits[region] += 1;
    }

    pub fn tick_advance(&mut self) {
        self.tick += 1;
    }

    pub fn should_grow(&self, region: usize) -> bool {
        if region >= self.region_entropy.len() { return false; }
        self.region_entropy[region] > self.growth_threshold
            && self.region_hits[region] > 10
    }

    pub fn should_prune(&self, region: usize) -> bool {
        if region >= self.region_activation.len() { return false; }
        self.region_activation[region] < self.prune_threshold as f64
            && self.region_hits[region] > 100
    }

    pub fn region_entropy(&self, region: usize) -> f32 {
        self.region_entropy.get(region).copied().unwrap_or(0.0)
    }

    pub fn num_regions(&self) -> usize {
        self.region_entropy.len()
    }
}
