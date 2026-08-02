//! SelfLearningAgent — IDEA #313: Self-Learning OS pipeline.
//!
//! # Pipeline
//! 1. `DataCollector` (EventBus `HERMES_RESPONSE` + `USER_INTENT` + `KERNEL_ERROR`)
//! 2. `TrainingAgent` (`BitNetTrainer` on-device ternary fine-tuning)
//! 3. `ModelHub` (marca slot `Learner` como carregado)
//! 4. SGDB (L4Semantic): persistência de pesos + pares + recall associativo
//!
//! PollEvery(5000): coleta dados do EventBus, converte pares texto → embeddings f32,
//! fine-tuna pesos ternários placeholder, registra modelo no ModelHub e persiste o
//! estado (pesos + pares recentes) no SGDB. O aprendizado sobrevive reboot e vira
//! memória associativa consumível via `learner_recall` / `recall` global.

use agent_core::{Agent, AgentKind, AgentManifest, AgentTickResult, ScheduleKind};
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::data_collector::DataCollector;
use crate::data_collector::TrainingPair;
use crate::training_agent::TrainingAgent;

const MANIFEST: AgentManifest = AgentManifest {
    name: "self-learning",
    kind: AgentKind::System,
    schedule: ScheduleKind::PollEvery(5000),
    auto_start: true,
    persist: false,
};

/// Máximo de pares na memória associativa em RAM (persistida + lembrada).
const MAX_LEARNED: usize = 256;
/// Quantos pares mais recentes são persistidos por ciclo.
const PERSIST_RECENT: usize = 32;
/// Limiar de similaridade de cosseno (embeddings centrados) para recall.
/// Com centragem o cosseno fica bem mais discriminativo que o bruto:
/// exato=1.0, typo de 1 char ≈ 0.93–0.998, palavras distintas ≤ 0.71.
/// 0.85 separa melhor que 0.8 (0.8 pré-centragem aceitava QUALQUER par de
/// mesmo comprimento — "hello" vs "zzzzz" ≈ 0.999).
const RECALL_THRESHOLD: f32 = 0.85;

/// Agente de auto-aprendizado contínuo.
///
/// Coleta pares (input, output) do sistema via EventBus, converte para
/// representação vetorial f32, fine-tuna um modelo ternário placeholder e
/// persiste pesos + pares recentes no SGDB (L4Semantic). O estado restaurado
/// alimenta `learner_recall` — memória associativa consumível por outros agentes.
pub struct SelfLearningAgent {
    /// Coletor de dados do EventBus
    pub collector: DataCollector,
    /// Trainer com BitNetTrainer para fine-tuning on-device
    pub trainer: TrainingAgent,
    /// Contador de ciclos de aprendizado
    pub cycle: u32,
    /// Loss do último fine-tuning
    pub last_loss: f32,
    /// Memória associativa: pares restaurados do SGDB + recentes por ciclo
    learned: Vec<TrainingPair>,
    /// `true` se o estado (pesos/pares) foi restaurado do SGDB no construtor
    restored: bool,
    /// Pesos ternários placeholder (64 elementos)
    weights: Vec<i8>,
}

impl SelfLearningAgent {
    /// Cria novo agente com buffer de 1000 pares, pesos placeholder 64-dim e
    /// estado restaurado do SGDB (best-effort: falha de carga = segue com zeros).
    pub fn new() -> Self {
        let mut agent = Self {
            collector: DataCollector::new(1000),
            trainer: TrainingAgent::new(),
            cycle: 0,
            last_loss: 0.0,
            learned: Vec::new(),
            restored: false,
            weights: alloc::vec![0i8; 64],
        };
        agent.load();
        agent
    }

    /// Ciclo principal de aprendizado:
    /// 1. Poll EventBus → coleta pares (input, output)
    /// 2. Converte strings → embeddings f32 (byte encoding)
    /// 3. Fine-tune pesos ternários (2 epochs)
    /// 4. Sincroniza memória associativa + persiste no SGDB
    /// 5. Marca slot Learner no ModelHub
    /// 6. Retorna loss média (0.0 se sem dados)
    pub fn learn_tick(&mut self) -> f32 {
        self.cycle += 1;

        // 1. Poll EventBus receivers
        self.collector.poll(self.cycle as u64);
        let pairs = self.collector.snapshot();
        if pairs.is_empty() {
            return 0.0;
        }

        // 2. Convert string pairs → float embeddings
        let data = Self::pairs_to_data(&pairs);

        // 3. Fine-tune on-device
        let loss = self.trainer.fine_tune(&mut self.weights, &data, 2);
        self.last_loss = loss;

        // 4. Memória associativa + persistência (sobrevive reboot)
        for pair in pairs.iter().rev().take(PERSIST_RECENT) {
            self.push_learned(pair.clone());
        }
        self.persist();

        // 5. Mark Learner slot in ModelHub
        cortex::model_hub::mark_slot(cortex::model_hub::ModelSlot::Learner, true);

        k_nano::slog_kai!("SELF-LEARN", "info",
            "cycle={} samples={} loss={:.4} learned={}",
            self.cycle, pairs.len(), loss, self.learned.len());

        loss
    }

    /// Memória associativa: codifica `text` no embedding 64-dim, varre os pares
    /// conhecidos (memória persistida + buffer do collector) e retorna o output
    /// do melhor par se `sim >= RECALL_THRESHOLD`; senão `None`. Loga hit/miss.
    pub fn learner_recall(&self, text: &str) -> Option<String> {
        match self.learner_recall_scored(text) {
            Some((out, sim)) if sim >= RECALL_THRESHOLD => {
                k_nano::slog_kai!("SELF-LEARN", "recall", "hit sim={:.3} -> {}", sim, out);
                Some(out)
            }
            Some((_, sim)) => {
                k_nano::slog_kai!("SELF-LEARN", "recall", "miss best_sim={:.3}", sim);
                None
            }
            None => {
                k_nano::slog_kai!("SELF-LEARN", "recall", "miss (no pairs)");
                None
            }
        }
    }

    /// Recall associativo com score: mesmo cálculo de `learner_recall`, mas
    /// retorna `(output, sim)` do melhor par SEM aplicar threshold — o
    /// consumidor (ex. Hermes) decide com limiar estrito (ex. 0.98 = resposta
    /// direta). `None` apenas quando não há pares conhecidos.
    pub fn learner_recall_scored(&self, text: &str) -> Option<(String, f32)> {
        let query = Self::embed(text);
        let mut best_sim = -1.0f32;
        let mut best: Option<&str> = None;

        for pair in self.learned.iter() {
            let sim = Self::cosine(&query, &Self::embed(&pair.input));
            if sim > best_sim {
                best_sim = sim;
                best = Some(&pair.output);
            }
        }
        let snap = self.collector.snapshot();
        for pair in snap.iter() {
            let sim = Self::cosine(&query, &Self::embed(&pair.input));
            if sim > best_sim {
                best_sim = sim;
                best = Some(&pair.output);
            }
        }

        best.map(|out| (String::from(out), best_sim))
    }

    /// Registra um par na memória associativa E no buffer de treino manualmente.
    pub fn remember(&mut self, input: &str, output: &str) {
        self.push_learned(TrainingPair {
            input: String::from(input),
            output: String::from(output),
            source: "self",
            timestamp: self.cycle as u64,
        });
        self.collector.remember(input, output, self.cycle as u64);
    }

    /// Persiste pesos (64 bytes crus) + top-32 pares recentes no SGDB L4Semantic
    /// (keys `learner/weights`, `learner/pairs`). No-op se SGDB indisponível.
    pub fn persist(&self) {
        if !crate::sgdb::ready() {
            return;
        }
        let wbytes: Vec<u8> = self.weights.iter().map(|&w| w as u8).collect();
        let _ = crate::sgdb::put_doc(crate::sgdb::MemoryDoc::new(
            crate::sgdb::MemoryLayer::L4Semantic,
            "learner/weights",
            wbytes,
        ));

        let snap = self.collector.snapshot();
        if snap.is_empty() {
            return;
        }
        let mut payload = Vec::new();
        for pair in snap.iter().rev().take(PERSIST_RECENT) {
            // Escapa \n/\0 nos textos para o layout "input\0output\n" ser unívoco
            let input = pair.input.replace(['\n', '\0'], " ");
            let output = pair.output.replace(['\n', '\0'], " ");
            payload.extend_from_slice(input.as_bytes());
            payload.push(0);
            payload.extend_from_slice(output.as_bytes());
            payload.push(b'\n');
        }
        let _ = crate::sgdb::put_doc(crate::sgdb::MemoryDoc::new(
            crate::sgdb::MemoryLayer::L4Semantic,
            "learner/pairs",
            payload,
        ));
    }

    /// Converte TrainingPair → (Vec<f32>, Vec<f32>) via byte embedding.
    /// Cada byte do input/output é mapeado para f32 em [0, 1] (64 dims).
    fn pairs_to_data(pairs: &[TrainingPair]) -> Vec<(Vec<f32>, Vec<f32>)> {
        let mut data = Vec::with_capacity(pairs.len());
        for pair in pairs {
            data.push((Self::embed(&pair.input).to_vec(), Self::embed(&pair.output).to_vec()));
        }
        data
    }

    /// Embedding 64-dim byte/255 (mesmo mapeamento de `pairs_to_data`).
    fn embed(text: &str) -> [f32; 64] {
        let mut v = [0.0f32; 64];
        for (i, b) in text.bytes().take(64).enumerate() {
            v[i] = (b as f32) / 255.0;
        }
        v
    }

    /// Similaridade de cosseno entre embeddings 64-dim, com centragem nos dims
    /// ATIVOS (não-zero): subtrai a média dos bytes não-zero de cada vetor antes
    /// do produto escalar; o padding de zeros permanece 0.
    ///
    /// Por que centrar só os ativos e não o vetor inteiro: com o padding de
    /// zeros dominando (59/64 dims), centrar os 64 dims não discrimina — ambos
    /// os vetores ficam "5 dims grandes, 59 dims ≈ −média" e o cosseno de
    /// "hello" vs "zzzzz" (mesmo comprimento) segue ≈ 0.999. Centrar os ativos
    /// distingue os VALORES dos bytes ("hello"=104..111 vs "zzzzz"=122): o
    /// cosseno cai para 0.0. Vetores constantes (ex. "zzzzz") têm variância
    /// zero após centragem → sim 0.0 (conservador, sem falso-positivo).
    fn cosine(a: &[f32; 64], b: &[f32; 64]) -> f32 {
        let ac = Self::centered(a);
        let bc = Self::centered(b);
        let mut dot = 0.0f32;
        let mut na = 0.0f32;
        let mut nb = 0.0f32;
        for i in 0..64 {
            dot += ac[i] * bc[i];
            na += ac[i] * ac[i];
            nb += bc[i] * bc[i];
        }
        if na <= 0.0 || nb <= 0.0 {
            return 0.0;
        }
        dot / (libm::sqrtf(na) * libm::sqrtf(nb) + 1e-8)
    }

    /// Copia o embedding subtraindo a média dos dims ativos (padding fica 0).
    fn centered(v: &[f32; 64]) -> [f32; 64] {
        let mut c = [0.0f32; 64];
        let mut sum = 0.0f32;
        let mut n = 0u32;
        for i in 0..64 {
            if v[i] != 0.0 {
                sum += v[i];
                n += 1;
            }
        }
        if n == 0 {
            return c;
        }
        let mean = sum / n as f32;
        for i in 0..64 {
            if v[i] != 0.0 {
                c[i] = v[i] - mean;
            }
        }
        c
    }

    /// Adiciona par à memória associativa (evicção FIFO em `MAX_LEARNED`).
    fn push_learned(&mut self, pair: TrainingPair) {
        if self.learned.len() >= MAX_LEARNED {
            self.learned.remove(0);
        }
        self.learned.push(pair);
    }

    /// Restaura pesos + pares do SGDB (L4Semantic `learner/*`). Best-effort.
    fn load(&mut self) {
        if !crate::sgdb::ready() {
            return;
        }
        if let Ok(Some(doc)) = crate::sgdb::get_doc(
            crate::sgdb::MemoryLayer::L4Semantic,
            "learner/weights",
        ) {
            if doc.payload.len() == 64 {
                self.weights = doc.payload.iter().map(|&b| b as i8).collect();
                self.restored = true;
            }
        }
        if let Ok(Some(doc)) = crate::sgdb::get_doc(
            crate::sgdb::MemoryLayer::L4Semantic,
            "learner/pairs",
        ) {
            if let Ok(text) = core::str::from_utf8(&doc.payload) {
                for line in text.split('\n') {
                    if line.is_empty() {
                        continue;
                    }
                    if let Some((input, output)) = line.split_once('\0') {
                        self.push_learned(TrainingPair {
                            input: String::from(input),
                            output: String::from(output),
                            source: "persisted",
                            timestamp: 0,
                        });
                    }
                }
            }
        }
        if self.restored {
            k_nano::slog_kai!("SELF-LEARN", "info",
                "restored weights + {} pairs from SGDB", self.learned.len());
        }
    }
}

impl Agent for SelfLearningAgent {
    fn manifest(&self) -> &AgentManifest {
        &MANIFEST
    }

    fn tick(&mut self, _tick: u64, _tick_count: u64) -> AgentTickResult {
        self.learn_tick();
        AgentTickResult::Done
    }

    fn on_activate(&mut self) {
        k_nano::slog_kai!("SELF-LEARN", "info", "SelfLearningAgent activated");
    }
}

// ── Acessores globais (padrão do repo: static Mutex<Option<T>>) ──────────────

/// Singleton do SelfLearningAgent (init lazy na primeira chamada).
static LEARNER: Mutex<Option<SelfLearningAgent>> = Mutex::new(None);

/// Acessor do singleton global.
pub fn learner_global() -> &'static Mutex<Option<SelfLearningAgent>> {
    &LEARNER
}

/// Recall associativo global: init lazy do agente, delega a `learner_recall`.
pub fn recall(text: &str) -> Option<String> {
    let mut g = LEARNER.lock();
    if g.is_none() {
        *g = Some(SelfLearningAgent::new());
    }
    g.as_mut()?.learner_recall(text)
}

/// Ciclo de aprendizado global (init lazy): retorna loss do último fine-tuning.
pub fn learn_tick_global() -> f32 {
    let mut g = LEARNER.lock();
    if g.is_none() {
        *g = Some(SelfLearningAgent::new());
    }
    match g.as_mut() {
        Some(a) => a.learn_tick(),
        None => 0.0,
    }
}

/// Diagnóstico: total de pares conhecidos (memória associativa + buffer vivo).
pub fn learned_pairs_count() -> usize {
    let g = LEARNER.lock();
    match g.as_ref() {
        Some(a) => a.learned.len() + a.collector.count(),
        None => 0,
    }
}

/// Pares da memória associativa (input, output) para broadcast no mesh como
/// memória coletiva L4. Operando no singleton global; init lazy se `None`.
pub fn learned_pairs() -> Vec<(String, String)> {
    let mut g = LEARNER.lock();
    if g.is_none() {
        *g = Some(SelfLearningAgent::new());
    }
    g.as_ref()
        .map(|a| {
            a.learned
                .iter()
                .map(|p| (p.input.clone(), p.output.clone()))
                .collect()
        })
        .unwrap_or_default()
}

// ── Self-test ──────────────────────────────────────────────

/// Testa SelfLearningAgent: criação, learn_tick sem eventos, ciclo.
pub fn demo() -> bool {
    let mut agent = SelfLearningAgent::new();

    // Nenhum evento publicado → snapshot vazio → loss = 0.0
    let loss = agent.learn_tick();
    if loss != 0.0 {
        return false;
    }
    if agent.cycle != 1 {
        return false;
    }

    // Verifica pesos placeholder intactos
    if agent.weights.len() != 64 {
        return false;
    }
    // Todos os pesos inicialmente zero (exceto se estado foi restaurado do SGDB)
    if !agent.restored && agent.weights.iter().any(|&w| w != 0) {
        return false;
    }

    true
}

/// Self-test estendido: recall associativo + persistência round-trip (se SGDB).
pub fn learner_self_test() -> bool {
    let mut agent = SelfLearningAgent::new();
    agent.remember("hello", "world");
    agent.remember("status", "ok");

    // Recall exato (cosseno 1.0 >= 0.8)
    match agent.learner_recall("hello") {
        Some(out) if out == "world" => {}
        _ => {
            k_nano::slog_kai!("SELF-LEARN", "self-test", "FAIL: recall('hello') != 'world'");
            return false;
        }
    }
    // Texto não relacionado → None (cosseno < threshold)
    if agent.learner_recall("42").is_some() {
        k_nano::slog_kai!("SELF-LEARN", "self-test", "FAIL: recall('42') should be None");
        return false;
    }

    // Discriminação pós-centragem: "zzzzz" tem MESMO comprimento de "hello"
    // (5 bytes ativos) — o cosseno bruto dava ≈0.999 e o learner respondia
    // "world". Com centragem nos dims ativos o sim cai ≈0.0; nunca pode ser
    // um hit confiante (>= 0.85).
    match agent.learner_recall_scored("zzzzz") {
        Some((_, sim)) if sim >= 0.85 => {
            k_nano::slog_kai!("SELF-LEARN", "self-test",
                "FAIL: scored('zzzzz') sim={:.3} >= 0.85 (false positive)", sim);
            return false;
        }
        _ => {}
    }

    // Persistência round-trip: put → novo agente → get → pesos/pares iguais
    if crate::sgdb::ready() {
        agent.persist();
        let b = SelfLearningAgent::new();
        if b.weights != agent.weights {
            k_nano::slog_kai!("SELF-LEARN", "self-test", "FAIL: weights round-trip mismatch");
            return false;
        }
        let has_hello = b.learned.iter().any(|p| p.input == "hello" && p.output == "world");
        let has_status = b.learned.iter().any(|p| p.input == "status" && p.output == "ok");
        if !has_hello || !has_status {
            k_nano::slog_kai!("SELF-LEARN", "self-test", "FAIL: pairs round-trip mismatch");
            return false;
        }
        k_nano::slog_kai!("SELF-LEARN", "self-test",
            "round-trip PASS (weights {}B, {} pairs)", b.weights.len(), b.learned.len());
    }

    k_nano::slog_kai!("SELF-LEARN", "self-test", "PASS");
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_learning_demo() {
        assert!(demo());
    }

    #[test]
    fn self_learning_recall() {
        assert!(learner_self_test());
    }
}
