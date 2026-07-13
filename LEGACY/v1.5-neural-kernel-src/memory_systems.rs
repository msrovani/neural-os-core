//! #314 SleepCycle, #214-#225 Memory, #359 BGE embedding (carregado do FAT).

use alloc::vec::Vec;
use alloc::vec;
use alloc::string::String;
use core::sync::atomic::{AtomicBool, Ordering};
use alloc::collections::BTreeMap;

pub static BGE_LOADED: AtomicBool = AtomicBool::new(false);
static mut BGE_WEIGHTS: Option<Vec<f32>> = None;
static mut BGE_VOCAB: usize = 0;
static mut BGE_HIDDEN: usize = 384;

/// Carrega modelo BGE do .bitnet v3 (embedding table apenas).
/// Formato: magic(4) + ver(4) + vocab(4) + hidden(4) + layers(4) + ffn(4) + heads(4) + max_seq(4)
/// + model_type(16) + ntensors(4) + tensors...
pub fn load_bge(data: &[u8]) -> bool {
    if data.len() < 32 { return false; }
    let r4 = |off: usize| u32::from_le_bytes(data[off..off+4].try_into().unwrap_or([0; 4]));
    if r4(0) != 0xBE11BE11 { return false; }

    let _ver = r4(4);
    let _vocab = r4(8) as usize;
    let hidden = r4(12) as usize;
    let _layers = r4(16);

    // Pula ate os tensores
    let off = 32 + 16 + 4; // header + model_type(16) + ntensors(4)
    if off + 4 > data.len() { return false; }
    let ntensors = r4(off - 4) as usize;

    let mut pos = off;
    for _ in 0..ntensors {
        if pos + 64 + 8 > data.len() { break; }
        let name_bytes = &data[pos..pos+64];
        let name_end = name_bytes.iter().position(|&b| b==0).unwrap_or(64);
        let name = core::str::from_utf8(&name_bytes[..name_end]).unwrap_or("");
        let n_orig = r4(pos + 64) as usize;
        let n_quant = r4(pos + 64 + 4) as usize;
        let f32_bytes = n_orig * 4;
        pos += 64 + 8;

        if name.contains("word_embeddings_weight") {
            if pos + f32_bytes <= data.len() {
                let floats: &[f32] = unsafe {
                    core::slice::from_raw_parts(data[pos..].as_ptr() as *const f32, n_orig)
                };
                unsafe {
                    BGE_VOCAB = n_orig / hidden;
                    BGE_HIDDEN = hidden;
                    BGE_WEIGHTS = Some(floats.to_vec());
                }
            }
        }
        pos += f32_bytes + n_quant;
    }

    let loaded = unsafe { BGE_WEIGHTS.is_some() };
    BGE_LOADED.store(loaded, Ordering::Relaxed);
    if loaded {
        crate::serial_println!("[BGE] Carregado: vocab={} hidden={} ({} MB)",
            unsafe { BGE_VOCAB }, unsafe { BGE_HIDDEN },
            unsafe { BGE_WEIGHTS.as_ref().map_or(0, |w| w.len() * 4 / 1024 / 1024) });
    }
    loaded
}

/// Gera embedding de 384 dims por media dos embeddings dos tokens.
pub fn bge_embed(text: &str) -> Vec<f32> {
    if !BGE_LOADED.load(Ordering::Relaxed) { return Vec::new(); }
    let hidden = unsafe { BGE_HIDDEN };
    let Some(weights) = (unsafe { BGE_WEIGHTS.as_ref() }) else { return Vec::new(); };
    let vocab = unsafe { BGE_VOCAB };

    let tokens = crate::bpe::encode(text);
    if tokens.is_empty() { return vec![0.0f32; hidden]; }

    let mut out = vec![0.0f32; hidden];
    let ntok = tokens.len().max(1) as f32;
    for &tok in &tokens {
        let idx = (tok as usize) % vocab;
        let start = idx * hidden;
        for j in 0..hidden.min(weights.len().saturating_sub(start)) {
            out[j] += weights[start + j] / ntok;
        }
    }
    out
}

pub fn bge_status() -> String {
    if BGE_LOADED.load(Ordering::Relaxed) {
        alloc::format!("[BGE] {} dim, loaded=true", unsafe { BGE_HIDDEN })
    } else {
        String::from("[BGE] ausente — use build_image.py --all")
    }
}

/// Registro de embedding para busca semântica
pub struct EmbeddingEntry {
    pub label: String,
    pub embedding: Vec<f32>,
}

pub(crate) static EMBED_INDEX: spin::Mutex<Vec<EmbeddingEntry>> = spin::Mutex::new(Vec::new());

/// Indexa um texto para busca semântica futura
pub fn index_embedding(label: &str, text: &str) {
    let emb = bge_embed(text);
    if emb.is_empty() { return; }
    EMBED_INDEX.lock().push(EmbeddingEntry { label: String::from(label), embedding: emb });
}

/// Busca semântica: top-k por similaridade cosseno
pub fn semantic_search(query: &str, top_k: usize) -> Vec<(String, f32)> {
    let q_emb = bge_embed(query);
    if q_emb.is_empty() { return Vec::new(); }
    let index = EMBED_INDEX.lock();
    let mut results: Vec<(String, f32)> = index.iter().map(|entry| {
        let dot: f32 = entry.embedding.iter().zip(q_emb.iter()).map(|(a,b)| a*b).sum();
        let norm_a = libm::sqrtf(entry.embedding.iter().map(|v| v*v).sum::<f32>() + 1e-8);
        let norm_b = libm::sqrtf(q_emb.iter().map(|v| v*v).sum::<f32>() + 1e-8);
        let sim = dot / (norm_a * norm_b);
        (entry.label.clone(), sim)
    }).collect();
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(core::cmp::Ordering::Equal));
    results.truncate(top_k);
    results
}

// ═══════════════════════════════════════════════════════════════════════════════
// Sprint 96: #226 Team Memory + #227 Memory Git Snapshots
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Clone)]
pub struct TeamMemoryEntry {
    pub agent: String,
    pub key: String,
    pub value: Vec<f32>,
    pub ts: u64,
}

pub struct TeamMemory {
    pub store: BTreeMap<String, Vec<TeamMemoryEntry>>,
    pub snapshots: Vec<String>,
    pub ts: u64,
}
impl TeamMemory {
    pub fn new() -> Self { TeamMemory { store: BTreeMap::new(), snapshots: Vec::new(), ts: 0 } }
    pub fn share(&mut self, agent: &str, key: &str, value: Vec<f32>) {
        self.ts += 1;
        self.store.entry(String::from(key)).or_default().push(TeamMemoryEntry { agent: String::from(agent), key: String::from(key), value, ts: self.ts });
    }
    pub fn recall(&self, key: &str) -> Option<&Vec<TeamMemoryEntry>> { self.store.get(key) }
    pub fn snapshot(&mut self) {
        let snap = alloc::format!("snap-{}:{}agents", self.ts, self.store.len());
        self.snapshots.push(snap);
    }
    pub fn status(&self) -> String {
        alloc::format!("[TMEM] {} keys, {} snapshots", self.store.len(), self.snapshots.len())
    }
}
