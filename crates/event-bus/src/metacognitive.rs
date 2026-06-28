use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug)]
pub struct Mistake {
    pub error_type: String,
    pub skill: String,
    pub tick: u64,
    pub fix: String,
}

pub struct MetacognitiveGuard {
    mistakes: Vec<Mistake>,
}

impl MetacognitiveGuard {
    pub fn new() -> Self { MetacognitiveGuard { mistakes: Vec::new() } }

    pub fn record(&mut self, error_type: &str, skill: &str, tick: u64, fix: &str) {
        self.mistakes.push(Mistake {
            error_type: String::from(error_type), skill: String::from(skill),
            tick, fix: String::from(fix),
        });
        if self.mistakes.len() > 64 { self.mistakes.remove(0); }
    }

    pub fn check(&self, skill: &str, error_type: &str) -> Option<&str> {
        for m in self.mistakes.iter().rev() {
            if m.skill == skill && m.error_type == error_type { return Some(&m.fix); }
        }
        None
    }

    pub fn recent(&self, n: usize) -> &[Mistake] {
        let start = self.mistakes.len().saturating_sub(n);
        &self.mistakes[start..]
    }
}
