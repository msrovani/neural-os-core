//! MSched — Belady/OPT eviction predictor para VRAM (#334).
//! Monitora padrao de acesso a VRAM, prediz working set do proximo kernel GPU.

use alloc::collections::VecDeque;

pub struct MschedPredictor {
    history: VecDeque<u64>,
    window: usize,
}

impl MschedPredictor {
    pub fn new(window: usize) -> Self {
        Self { history: VecDeque::with_capacity(window), window }
    }

    pub fn record_access(&mut self, addr: u64) {
        if self.history.len() >= self.window { self.history.pop_front(); }
        self.history.push_back(addr);
    }

    /// Prediz qual pagina de VRAM nao sera usada por mais tempo (OPT/Belady).
    pub fn predict_evict(&self, working_set: &[u64]) -> Option<u64> {
        let mut farthest = None;
        let mut farthest_dist = 0usize;
        for &page in working_set {
            let dist = self.history.iter().rev().position(|&a| a == page).unwrap_or(usize::MAX);
            if dist > farthest_dist { farthest_dist = dist; farthest = Some(page); }
        }
        farthest
    }

    pub fn status(&self) -> alloc::string::String {
        alloc::format!("[MSCHED] {} amostras, window={}", self.history.len(), self.window)
    }
}
