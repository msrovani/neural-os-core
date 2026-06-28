use alloc::string::String;
use alloc::vec::Vec;

pub struct HybridSearch {
    docs: Vec<String>,
}

impl HybridSearch {
    pub fn new() -> Self { HybridSearch { docs: Vec::new() } }

    pub fn index(&mut self, text: &str) -> usize {
        let id = self.docs.len();
        self.docs.push(String::from(text));
        id
    }

    fn tokenize(s: &str) -> Vec<String> {
        s.split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|w| w.len() > 1)
            .map(|w| w.to_ascii_lowercase())
            .collect()
    }

    fn tf_score(qtokens: &[String], doc: &str) -> f32 {
        let dtokens = Self::tokenize(doc);
        let dl = dtokens.len() as f32;
        if dl == 0.0 { return 0.0; }
        let mut matches = 0usize;
        for q in qtokens {
            matches += dtokens.iter().filter(|t| *t == q).count();
        }
        (matches as f32) / dl
    }

    pub fn search(&self, query: &str, mlp_score: f32) -> Vec<(usize, f32)> {
        let qtokens = Self::tokenize(query);
        if qtokens.is_empty() { return Vec::new(); }
        let mut results: Vec<_> = self.docs.iter().enumerate().map(|(id, doc)| {
            let tf = Self::tf_score(&qtokens, doc);
            (id, tf * 0.6 + mlp_score * 0.4)
        }).filter(|(_, s)| *s > 0.01).collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(core::cmp::Ordering::Equal));
        results.truncate(10);
        results
    }
}
