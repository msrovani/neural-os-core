//! #218 4-Tier memory consolidation (jcode-inspired "ambient mode").
//! L1Working → L2EpisodicShort → L3EpisodicLong → L4Semantic → L5Procedural.
//! Chamado periodicamente pelo SleepCycleAgent (fase CONSOLIDATE).

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use crate::sgdb::{self, MemoryDoc, MemoryLayer};

pub const TOPIC_MEMORY_TIER: &str = "MEMORY_TIER";

#[derive(Clone, Debug, Default)]
pub struct TierStats {
    pub episodic_read: usize,
    pub topics: usize,
    pub promoted_l3: usize,
    pub promoted_l4: usize,
    pub procedural_skills: usize,
}

static TOPIC_CYCLES: Mutex<BTreeMap<String, u32>> = Mutex::new(BTreeMap::new());
static TOTAL_L3: AtomicU32 = AtomicU32::new(0);
static TOTAL_L4: AtomicU32 = AtomicU32::new(0);

/// Stopwords pt/en (~32) — tokens frequentes sem valor de tópico.
static STOPWORDS: &[&str] = &[
    "de", "do", "da", "dos", "das", "o", "a", "os", "as", "um", "uma", "e", "ou", "que", "para",
    "por", "com", "sem", "no", "na", "em", "é", "são", "the", "an", "and", "or", "for", "to",
    "of", "in", "on", "is", "are", "you", "i", "it", "this", "that",
];

/// Extrai tópicos frequentes (count >= 2) do texto, desc por frequência, cap 3.
fn top_topics(text: &str) -> Vec<(String, u32)> {
    let lower = text.to_lowercase();
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    for word in lower.split_whitespace() {
        let w = word.trim_matches(|c: char| {
            matches!(c, ',' | '.' | ';' | ':' | '?' | '!' | '(' | ')' | '[' | ']' | '{' | '}' | '"' | '\'')
        });
        if w.len() < 3 || STOPWORDS.contains(&w) {
            continue;
        }
        *counts.entry(String::from(w)).or_insert(0) += 1;
    }
    let mut ranked: Vec<(String, u32)> = counts
        .into_iter()
        .filter(|(_, c)| *c >= 2)
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1));
    ranked.truncate(3);
    ranked
}

/// Primeira linha do batch que contém o tópico (case-insensitive), truncada em 200 chars.
fn line_with_topic(batch: &str, topic: &str) -> String {
    batch
        .lines()
        .find(|l| l.to_lowercase().contains(topic))
        .map(|l| l.trim().chars().take(200).collect())
        .unwrap_or_default()
}

/// Publica transição de tier no EventBus (mesmo padrão de cognitive_bridge).
fn publish_tier(msg: &str) {
    let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: String::from(TOPIC_MEMORY_TIER),
        payload: msg.as_bytes().to_vec(),
        token: event_bus::CapabilityToken::Legacy(1),
    });
}

/// Consolidação ambient 4-tier. Retorna estatísticas do ciclo.
pub fn consolidate_tiers(tick: u64) -> TierStats {
    let mut stats = TierStats::default();
    if !sgdb::ready() {
        return stats;
    }
    let batch = sgdb::prompt_slice(400);
    stats.episodic_read = batch.len();
    if batch.trim().is_empty() {
        return stats;
    }

    let topics = top_topics(&batch);
    stats.topics = topics.len();
    for (topic, count) in &topics {
        // L2 → L3 episódico longo
        let line = line_with_topic(&batch, topic);
        let payload = alloc::format!("topic:{} count:{}\n{}", topic, count, line);
        let key = alloc::format!("topic/{}", topic);
        let _ = sgdb::put_doc(MemoryDoc::new(
            MemoryLayer::L3EpisodicLong,
            &key,
            payload.into_bytes(),
        ));
        stats.promoted_l3 += 1;
        TOTAL_L3.fetch_add(1, Ordering::Relaxed);
        k_nano::slog_kai!("TIERS", "info", "L2→L3 {} (count={})", topic, count);
        publish_tier(&alloc::format!("L2→L3 {}", topic));

        // L3 → L4 semântico (estabilidade >= 2 ciclos)
        let cycles = {
            let mut m = TOPIC_CYCLES.lock();
            let c = m.entry(topic.clone()).and_modify(|c| *c += 1).or_insert(1);
            *c
        };
        if cycles >= 2 {
            let sem_key = alloc::format!("sem/{}/{}", tick, topic);
            let sem_payload = alloc::format!("semantic:{} hits:{}", topic, count);
            let _ = sgdb::put_doc(MemoryDoc::new(
                MemoryLayer::L4Semantic,
                &sem_key,
                sem_payload.into_bytes(),
            ));
            stats.promoted_l4 += 1;
            TOTAL_L4.fetch_add(1, Ordering::Relaxed);
            k_nano::slog_kai!("TIERS", "info", "L3→L4 {} (cycles={})", topic, cycles);
            publish_tier(&alloc::format!("L3→L4 {}", topic));
        }
        if cycles >= 4 {
            // refresh: remove para o próximo ciclo recomeçar a contagem
            TOPIC_CYCLES.lock().remove(topic);
        }
    }

    // L4 → L5 procedural (snapshot do registry de skills)
    let skills = k_nano::SKILL_REGISTRY.lock().list_skills();
    stats.procedural_skills = skills.len();
    let mut payload = String::new();
    for (name, _pol) in &skills {
        if payload.len() + name.len() + 1 > 512 {
            break;
        }
        if !payload.is_empty() {
            payload.push('\n');
        }
        payload.push_str(name);
    }
    let _ = sgdb::put_doc(MemoryDoc::new(
        MemoryLayer::L5Procedural,
        "proc/skills",
        payload.into_bytes(),
    ));
    k_nano::slog_kai!(
        "TIERS",
        "info",
        "L4→L5 registry ({} skills)",
        stats.procedural_skills
    );
    publish_tier("L4→L5 registry");

    stats
}

/// Status acumulado para logs do boot (ex.: `[TIERS] l3=.. l4=.. topics=..`).
pub fn tiers_status() -> String {
    alloc::format!(
        "[TIERS] l3_total={} l4_total={} topics={}",
        TOTAL_L3.load(Ordering::Relaxed),
        TOTAL_L4.load(Ordering::Relaxed),
        TOPIC_CYCLES.lock().len()
    )
}
