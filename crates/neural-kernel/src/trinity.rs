use crate::tensor::PackedTernaryTensor;
use alloc::vec::Vec;
use alloc::vec;

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
const HIDDEN: usize = 64;
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
        crate::serial_println!("[TRINITY] Router MoE loaded: {} dim, {} experts", HIDDEN, self.experts.len());
    }

    pub fn classify_intent(&self, text: &str) -> &Expert {
        // Tenta router neurnal primeiro
        if let (Some(ref embed_table), Some(ref weight)) = (&self.router_embed, &self.router_weight) {
            let tokens = encode(text);
            if !tokens.is_empty() {
                let mut embedding = vec![0.0f32; HIDDEN];
                for &tok in &tokens {
                    let idx = (tok as usize).min(VOCAB - 1);
                    let start = idx * HIDDEN;
                    for j in 0..HIDDEN {
                        embedding[j] += embed_table.get(start + j).copied().unwrap_or(0.0);
                    }
                }
                // Normaliza
                let norm = libm::sqrtf(embedding.iter().map(|v| v * v).sum::<f32>() + 1e-8);
                for v in embedding.iter_mut() { *v /= norm; }

                // Router weight: (HIDDEN, num_experts) ternary → scores
                let num_exp = self.experts.len();
                let emb_tensor = crate::tensor::Tensor::from_row_major((1, HIDDEN), embedding).unwrap();
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
                ExpertKind::Generator | ExpertKind::Unknown => {}
            }
        }
        &self.experts[0]
    }

    pub fn agent_count(&self) -> usize { self.experts.len() }
}

pub fn init_trinity() -> TrinityRouter {
    let mut router = TrinityRouter::new();
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
    router.register_expert(Expert {
        kind: ExpertKind::Generator, name: "generator",
        description: "Geracao generica de texto e respostas", weight: None,
    });
    router
}
