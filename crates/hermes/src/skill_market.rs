//! SkillMarket — reputação de skills (HANR hub ranking).
//! Unifica agent+skill scoring e scoreboard WASM.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use ticket_lock::TicketLock;

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
    pub fn new() -> Self {
        SkillMarket {
            scores: BTreeMap::new(),
        }
    }

    pub fn record(&mut self, agent: &str, skill: &str, ticks: u64, ok: bool) {
        let key = (String::from(agent), String::from(skill));
        let entry = self.scores.entry(key).or_insert(SkillScore {
            agent: String::from(agent),
            skill: String::from(skill),
            avg_ticks: 0,
            success_rate: 1.0,
            calls: 0,
        });
        let n = entry.calls as f32;
        entry.avg_ticks = ((entry.avg_ticks as f32 * n + ticks as f32) / (n + 1.0)) as u64;
        entry.success_rate =
            (entry.success_rate * n + if ok { 1.0 } else { 0.0 }) / (n + 1.0);
        entry.calls += 1;
    }

    /// Compat WASM runtime (só skill name).
    pub fn record_skill(&mut self, skill: &str, ticks: u64, ok: bool) {
        self.record("wasm", skill, ticks, ok);
    }

    pub fn best_agent(&self, skill: &str) -> Option<&str> {
        self.scores
            .iter()
            .filter(|((_, s), _)| s == skill)
            .max_by(|a, b| a.1.success_rate.total_cmp(&b.1.success_rate))
            .map(|((a, _), _)| a.as_str())
    }

    pub fn top_skills(&self, n: usize) -> Vec<&SkillScore> {
        let mut v: Vec<_> = self.scores.values().collect();
        v.sort_by(|a, b| b.success_rate.total_cmp(&a.success_rate));
        v.truncate(n);
        v
    }

    pub fn top(&self, n: usize) -> Vec<&SkillScore> {
        self.top_skills(n)
    }

    pub fn report(&self) -> String {
        let mut out = String::from("Skill Market Report:\n");
        for s in self.top_skills(32) {
            let _ = core::fmt::write(
                &mut out,
                format_args!(
                    "  {}:{} {} ticks {}% ({})\n",
                    s.agent,
                    s.skill,
                    s.avg_ticks,
                    (s.success_rate * 100.0) as u8,
                    s.calls
                ),
            );
        }
        out
    }
}

lazy_static! {
    pub static ref SKILL_MARKET: TicketLock<SkillMarket> = TicketLock::new(SkillMarket::new());
}

pub fn record_outcome(agent: &str, skill: &str, ticks: u64, ok: bool) {
    SKILL_MARKET.lock().record(agent, skill, ticks, ok);
}
