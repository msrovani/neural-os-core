//! SkillMarket — reputação de skills (HANR hub ranking).
//! Unifica agent+skill scoring e scoreboard WASM.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
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
        // Persiste no SGDB a cada atualização
        let _ = self.save_score_to_sgdb(agent, skill);
    }

    /// Salva score de um par (agent, skill) no SGDB `skill/rate/{agent}/{skill}`.
    fn save_score_to_sgdb(&self, agent: &str, skill: &str) -> Result<(), ()> {
        let key = (String::from(agent), String::from(skill));
        let Some(score) = self.scores.get(&key) else {
            return Ok(());
        };
        let buf = alloc::format!(
            "{} {} {} {}",
            score.avg_ticks,
            (score.success_rate * 10000.0) as u32,
            score.calls,
            score.skill
        );
        let sk = alloc::format!("skill/rate/{}/{}", agent, skill);
        k_ai::sgdb::store::put_kv(&sk, buf.as_bytes()).map_err(|_| ())
    }

    /// Carrega scores do SGDB (boot / warm start).
    pub fn load_from_sgdb(&mut self) -> usize {
        let prefix = "skill/rate/";
        let keys = k_nano::storage::with_tickv(|kv| kv.keys_with_prefix(prefix))
            .unwrap_or_default();
        let mut n = 0usize;
        for sk in keys {
            let Ok(Some(buf)) = k_ai::sgdb::store::get_kv(&sk) else { continue };
            let s = core::str::from_utf8(&buf).unwrap_or("");
            let parts: Vec<&str> = s.splitn(4, ' ').collect();
            if parts.len() < 4 { continue; }
            let ticks: u64 = parts[0].parse().unwrap_or(0);
            let rate: u32 = parts[1].parse().unwrap_or(0);
            let calls: u32 = parts[2].parse().unwrap_or(0);
            let skill_name = parts[3];
            // key = "skill/rate/{agent}/{skill}"
            let rest = sk.strip_prefix(prefix).unwrap_or("");
            let mut segs = rest.splitn(2, '/');
            let agent = segs.next().unwrap_or("?").to_string();
            let key = (agent.clone(), skill_name.to_string());
            self.scores.insert(key, SkillScore {
                agent,
                skill: skill_name.to_string(),
                avg_ticks: ticks,
                success_rate: rate as f32 / 10000.0,
                calls,
            });
            n += 1;
        }
        n
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

/// Auto-load do SGDB na primeira chamada (lazy, sem hook de boot).
static SGDB_LOADED: AtomicBool = AtomicBool::new(false);

pub fn record_outcome(agent: &str, skill: &str, ticks: u64, ok: bool) {
    if !SGDB_LOADED.load(Ordering::Relaxed) {
        let n = SKILL_MARKET.lock().load_from_sgdb();
        if n > 0 {
            k_nano::slog_hermes!("SkillMkt", "sgdb", "carregados {} scores", n);
        }
        SGDB_LOADED.store(true, Ordering::Relaxed);
    }
    SKILL_MARKET.lock().record(agent, skill, ticks, ok);
}






