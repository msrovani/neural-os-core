use crate::tensor::PackedTernaryTensor;
use alloc::vec::Vec;
use alloc::vec;

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum ExpertKind {
    HwIdentify,
    RustCoder,
    DiskDiag,
    Security,
    Generator,
    SpeechSynth,
    Unknown,
}

pub struct Expert {
    pub kind: ExpertKind,
    pub name: &'static str,
    pub description: &'static str,
    pub weight: Option<PackedTernaryTensor>,
}

pub struct TrinityRouter {
    experts: Vec<Expert>,
    router_weight: Option<PackedTernaryTensor>,
    /// Embedding table: VOCAB_SIZE x HIDDEN (f32)
    router_embed: Option<Vec<f32>>,
}

const VOCAB: usize = 99;
pub const ROUTER_HIDDEN: usize = 64;
pub const ROUTER_MAX_EXPERTS: usize = 8;
const BOS: u16 = 0;
const EOS: u16 = 1;
const CHAR_OFFSET: u16 = 3;

fn encode(text: &str) -> Vec<u16> {
    let mut tokens = vec![BOS];
    for b in text.bytes() {
        if b >= 32 && b <= 126 {
            tokens.push((b - 32) as u16 + CHAR_OFFSET);
        }
    }
    tokens.push(EOS);
    tokens.truncate(32);
    tokens
}

fn softmax(scores: &mut [f32]) {
    let max = scores.iter().fold(core::f32::NEG_INFINITY, |a, &b| a.max(b));
    let mut sum = 0.0;
    for v in scores.iter_mut() { *v = libm::expf(*v - max); sum += *v; }
    let inv = 1.0 / sum;
    for v in scores.iter_mut() { *v *= inv; }
}

impl TrinityRouter {
    pub fn new() -> Self {
        TrinityRouter { experts: Vec::new(), router_weight: None, router_embed: None }
    }

    pub fn register_expert(&mut self, expert: Expert) {
        crate::serial_println!("[TRINITY] Expert '{}' registered: {}", expert.name, expert.description);
        self.experts.push(expert);
    }

    /// Carrega pesos do router treinado.
    /// embed: VOCAB * HIDDEN floats (embedding table)
    /// weight: PackedTernaryTensor (HIDDEN x NUM_EXPERTS)
    pub fn load_router(&mut self, embed: Vec<f32>, weight: PackedTernaryTensor) {
        self.router_embed = Some(embed);
        self.router_weight = Some(weight);
        crate::serial_println!("[TRINITY] Router MoE loaded: {} dim, {} experts", ROUTER_HIDDEN, self.experts.len());
    }

    /// Classifica intent e grava logits na TensorArena (R3 rollout).
    /// Router NÃƒO deve ser re-chamado no update â€” usar trace retornado.
    pub fn classify_intent_with_trace(
        &self,
        text: &str,
        arena: &mut crate::arena::TensorArena,
    ) -> (&Expert, crate::r3::RouteTrace) {
        if let (Some(ref embed_table), Some(ref weight)) = (&self.router_embed, &self.router_weight) {
            let tokens = encode(text);
            if !tokens.is_empty() {
                let mut embedding = [0.0f32; ROUTER_HIDDEN];
                for &tok in &tokens {
                    let idx = (tok as usize).min(VOCAB - 1);
                    let start = idx * ROUTER_HIDDEN;
                    for j in 0..ROUTER_HIDDEN {
                        embedding[j] += embed_table.get(start + j).copied().unwrap_or(0.0);
                    }
                }
                let norm = libm::sqrtf(embedding.iter().map(|v| v * v).sum::<f32>() + 1e-8);
                for v in embedding.iter_mut() {
                    *v /= norm;
                }

                let num_exp = self.experts.len().min(ROUTER_MAX_EXPERTS);
                let emb_tensor = crate::tensor::Tensor::from_row_major((1, ROUTER_HIDDEN), embedding.to_vec()).unwrap();
                let scores_t = weight.matmul_hybrid(&emb_tensor).unwrap();
                let mut scores = scores_t.data;
                scores.truncate(num_exp);
                if scores.len() == num_exp {
                    softmax(&mut scores);
                    let mut best_idx = 0usize;
                    let mut best_score = scores[0];
                    for (i, &s) in scores.iter().enumerate().skip(1) {
                        if s > best_score {
                            best_idx = i;
                            best_score = s;
                        }
                    }
                    if best_score > 0.15 {
                        if let Some(trace) = crate::r3::record_router_trace(
                            arena,
                            &embedding,
                            &scores,
                            best_idx,
                        ) {
                            crate::serial_println!(
                                "[TRINITY] MoE router (R3): expert {} (score={:.3}) arena_used={} B",
                                self.experts[best_idx].name,
                                best_score,
                                arena.used_bytes()
                            );
                            return (&self.experts[best_idx], trace);
                        }
                    }
                }
            }
        }
        let expert = self.classify_intent(text);
        let emb = [0.0f32; ROUTER_HIDDEN];
        let logits = [1.0f32];
        let trace = crate::r3::record_router_trace(arena, &emb, &logits, 0)
            .unwrap_or(crate::r3::RouteTrace {
                embedding_addr: 0,
                logits_addr: 0,
                num_experts: 1,
                selected_expert: 0,
                old_log_prob: 0.0,
                token_ids_addr: 0,
                token_count: 0,
            });
        (expert, trace)
    }

    pub fn classify_intent(&self, text: &str) -> &Expert {
        // Tenta router neurnal primeiro
        if let (Some(ref embed_table), Some(ref weight)) = (&self.router_embed, &self.router_weight) {
            let tokens = encode(text);
            if !tokens.is_empty() {
                let mut embedding = vec![0.0f32; ROUTER_HIDDEN];
                for &tok in &tokens {
                    let idx = (tok as usize).min(VOCAB - 1);
                    let start = idx * ROUTER_HIDDEN;
                    for j in 0..ROUTER_HIDDEN {
                        embedding[j] += embed_table.get(start + j).copied().unwrap_or(0.0);
                    }
                }
                // Normaliza
                let norm = libm::sqrtf(embedding.iter().map(|v| v * v).sum::<f32>() + 1e-8);
                for v in embedding.iter_mut() { *v /= norm; }

                // Router weight: (HIDDEN, num_experts) ternary â†’ scores
                let num_exp = self.experts.len();
                let emb_tensor = crate::tensor::Tensor::from_row_major((1, ROUTER_HIDDEN), embedding).unwrap();
                let scores_t = weight.matmul_hybrid(&emb_tensor).unwrap();
                let mut scores = scores_t.data.clone();
                if scores.len() == num_exp {
                    softmax(&mut scores);
                    let mut best_idx = 0;
                    let mut best_score = scores[0];
                    for (i, &s) in scores.iter().enumerate().skip(1) {
                        if s > best_score { best_idx = i; best_score = s; }
                    }
                    if best_score > 0.15 {
                        crate::serial_println!("[TRINITY] MoE router: expert {} (score={:.3})", 
                            self.experts[best_idx].name, best_score);
                        return &self.experts[best_idx];
                    }
                }
            }
        }

        // Fallback: keyword matching
        let lower = text.to_lowercase();
        for expert in &self.experts {
            match expert.kind {
                ExpertKind::HwIdentify => {
                    if lower.contains("/hw") || lower.contains("hardware") || lower.contains("pci")
                        || lower.contains("dispositivo") || lower.contains("device")
                    { return expert; }
                }
                ExpertKind::RustCoder => {
                    if lower.contains("crie") || lower.contains("create") || lower.contains("code")
                        || lower.contains("write") || lower.contains("implement")
                    { return expert; }
                }
                ExpertKind::DiskDiag => {
                    if lower.contains("disk") || lower.contains("disco") || lower.contains("smart")
                        || lower.contains("storage") || lower.contains("armazenamento")
                    { return expert; }
                }
                ExpertKind::Security => {
                    if lower.contains("security") || lower.contains("seguranca")
                        || lower.contains("cve") || lower.contains("attack") || lower.contains("ataque")
                    { return expert; }
                }
                ExpertKind::SpeechSynth => {
                    if lower.contains("fale") || lower.contains("speak") || lower.contains("diga")
                        || lower.contains("say") || lower.contains("tts")
                        || lower.contains("pronuncie") || lower.contains("pronounce")
                        || lower.contains("audio") || lower.contains("voz") || lower.contains("voice")
                    { return expert; }
                }
                // Sprint 107 Loop2: chat/clima → generator (LLM 2B), nao hw_identify.
                ExpertKind::Generator => {
                    if lower.contains("tempo") || lower.contains("clima") || lower.contains("weather")
                        || lower.contains("previsao") || lower.contains("previsão")
                        || lower.contains("amanha") || lower.contains("amanhã")
                        || lower.contains("hoje") || lower.contains("chat")
                    { return expert; }
                }
                ExpertKind::Unknown => {}
            }
        }
        // Default = generator (LLM CURRENT_MODEL). Antes era experts[0]=hw_identify —
        // com HWEXPERT LOADED o clima e2e gerava no vocab=64 → "LOA,BLOA…".
        self.experts.iter().find(|e| e.kind == ExpertKind::Generator)
            .unwrap_or(&self.experts[self.experts.len() - 1])
    }

    pub fn agent_count(&self) -> usize { self.experts.len() }

    /// True se pesos do router MoE neural estão carregados (senão: keyword/R3).
    pub fn moe_router_loaded(&self) -> bool {
        self.router_weight.is_some() && self.router_embed.is_some()
    }

    /// Tem expert generator registrado (rota default segura).
    pub fn has_generator(&self) -> bool {
        self.experts.iter().any(|e| e.kind == ExpertKind::Generator)
    }
}

pub fn init_trinity() -> TrinityRouter {
    let mut router = TrinityRouter::new();
    // Generator primeiro = default seguro se find() falhar (Sprint 107 Loop2).
    router.register_expert(Expert {
        kind: ExpertKind::Generator, name: "generator",
        description: "Geracao generica de texto e respostas", weight: None,
    });
    router.register_expert(Expert {
        kind: ExpertKind::HwIdentify, name: "hw_identify",
        description: "Identifica dispositivos de hardware por PCI ID", weight: None,
    });
    router.register_expert(Expert {
        kind: ExpertKind::RustCoder, name: "rust_coder",
        description: "Gera codigo Rust sob demanda", weight: None,
    });
    router.register_expert(Expert {
        kind: ExpertKind::DiskDiag, name: "disk_diag",
        description: "Diagnostico de disco e armazenamento", weight: None,
    });
    router.register_expert(Expert {
        kind: ExpertKind::Security, name: "security",
        description: "Analise de seguranca e vulnerabilidades", weight: None,
    });
    router.register_expert(Expert {
        kind: ExpertKind::SpeechSynth, name: "speech_synth",
        description: "Sintese de fala — TTS, voz, audio", weight: None,
    });
    router
}

