//! SkillMarket — scoring e selecao de skills por performance.
//! SuperAGI-inspired: cada skill tem avg_ticks, success_rate.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

#[derive(Debug, Clone)]
pub struct SkillScore {
    pub agent: String,
    pub skill: String,
    pub avg_ticks: u64,
    pub success_rate: f32,
    pub calls: u32,
}

pub struct SkillMarket {
    scores: BTreeMap<(String, String), SkillScore>,
}

impl SkillMarket {
    pub fn new() -> Self { SkillMarket { scores: BTreeMap::new() } }

    pub fn record(&mut self, agent: &str, skill: &str, ticks: u64, ok: bool) {
        let key = (String::from(agent), String::from(skill));
        let entry = self.scores.entry(key).or_insert(SkillScore {
            agent: String::from(agent), skill: String::from(skill),
            avg_ticks: 0, success_rate: 1.0, calls: 0,
        });
        let n = entry.calls as f32;
        entry.avg_ticks = ((entry.avg_ticks as f32 * n + ticks as f32) / (n + 1.0)) as u64;
        entry.success_rate = (entry.success_rate * n + if ok { 1.0 } else { 0.0 }) / (n + 1.0);
        entry.calls += 1;
    }

    pub fn best_agent(&self, skill: &str) -> Option<&str> {
        self.scores.iter()
            .filter(|((_, s), _)| s == skill)
            .max_by(|a, b| a.1.success_rate.partial_cmp(&b.1.success_rate).unwrap())
            .map(|((a, _), _)| a.as_str())
    }

    pub fn top_skills(&self, n: usize) -> Vec<&SkillScore> {
        let mut v: Vec<_> = self.scores.values().collect();
        v.sort_by(|a, b| b.success_rate.partial_cmp(&a.success_rate).unwrap());
        v.truncate(n);
        v
    }

    pub fn report(&self) -> String {
        let mut out = String::from("Skill Market Report:\n");
        for s in self.scores.values() {
            let _ = core::fmt::write(&mut out, format_args!("  {}:{} {} ticks {}% ({})\n",
                s.agent, s.skill, s.avg_ticks, (s.success_rate * 100.0) as u8, s.calls));
        }
        out
    }
}
