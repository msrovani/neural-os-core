use crate::tensor::PackedTernaryTensor;
use alloc::vec::Vec;
use alloc::vec;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ExpertKind {
    HwIdentify,
    /// Controles de plataforma (volume, mute, brilho) — skill/HW, sem LLM.
    HwControl,
    RustCoder,
    DiskDiag,
    Security,
    Generator,
    SpeechSynth,
    Unknown,
}

/// Extrai o pedido do usuário de prompts envelopados (skills L0 / cognitive context).
/// Trinity DEVE classificar só isto — nunca o catálogo de skills no system prompt.
pub fn extract_user_utterance(text: &str) -> &str {
    let t = text.trim();
    if let Some((_, rest)) = t.rsplit_once("PERGUNTA:") {
        let u = rest.trim();
        if !u.is_empty() {
            return u;
        }
    }
    if let Some(idx) = t.rfind("[User]") {
        let u = t[idx + "[User]".len()..].trim();
        if !u.is_empty() {
            return u;
        }
    }
    if t.contains("[NEURAL-OS COGNITIVE CONTEXT") || t.contains("[SKILLS L0") {
        if let Some(line) = t.lines().rev().find(|l| {
            let s = l.trim();
            s.len() >= 2
                && s.len() < 220
                && !s.starts_with('[')
                && !s.starts_with("Instruções:")
                && !s.starts_with("Use /")
        }) {
            return line.trim();
        }
    }
    t
}

pub struct Expert {
    pub kind: ExpertKind,
    pub name: &'static str,
    pub description: &'static str,
    pub weight: Option<PackedTernaryTensor>,
}

/// Fallback quando `experts` está vazio (evita `len-1` → usize::MAX).
static FALLBACK_GENERATOR: Expert = Expert {
    kind: ExpertKind::Generator,
    name: "generator",
    description: "trinity empty fallback",
    weight: None,
};

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
        k_nano::slog_cortex!("TRINITY", "info", "Expert '{}' registered: {}", expert.name, expert.description);
        self.experts.push(expert);
    }

    /// Carrega pesos do router treinado.
    /// embed: VOCAB * HIDDEN floats (embedding table)
    /// weight: PackedTernaryTensor (HIDDEN x NUM_EXPERTS)
    pub fn load_router(&mut self, embed: Vec<f32>, weight: PackedTernaryTensor) {
        self.router_embed = Some(embed);
        self.router_weight = Some(weight);
        k_nano::slog_cortex!("TRINITY", "info", "Router MoE loaded: {} dim, {} experts", ROUTER_HIDDEN, self.experts.len());
    }

    /// Substitui os pesos do router MoE por pesos treinados/federados (F4).
    /// Mantém a embedding table; formato row-major HIDDEN×num_experts com
    /// valores ternários {-1,0,1}. Retorna false se tamanho inválido.
    pub fn set_router_weights(&mut self, weights: &[i8]) -> bool {
        let n_exp = weights.len() / ROUTER_HIDDEN;
        if n_exp == 0 || weights.len() != n_exp * ROUTER_HIDDEN || n_exp > ROUTER_MAX_EXPERTS {
            return false;
        }
        self.router_weight = Some(PackedTernaryTensor {
            shape: (ROUTER_HIDDEN, n_exp),
            packed_data: PackedTernaryTensor::pack_weights(weights),
        });
        crate::global_arena::reset_moe_cache();
        k_nano::slog_cortex!(
            "TRINITY", "info",
            "Router weights set: {}x{} (MoE cache reset)", ROUTER_HIDDEN, n_exp
        );
        true
    }

    /// Número de experts da matriz de pesos carregada (0 se ausente).
    pub fn router_num_experts(&self) -> usize {
        self.router_weight.as_ref().map(|t| t.shape.1).unwrap_or(0)
    }

    /// Desempacota os pesos do router para Vec<i8> row-major {-1,0,1}
    /// (inverso de `set_router_weights`). None se o router não tem pesos.
    /// C1 (oracle): base p/ iniciar o buffer de replay com o router VIVO —
    /// posições não tocadas preservam o estado atual (não viram 0 no delta).
    pub fn unpack_router_weights(&self) -> Option<Vec<i8>> {
        let t = self.router_weight.as_ref()?;
        let mut out = Vec::with_capacity(t.shape.0 * t.shape.1);
        for i in 0..(t.shape.0 * t.shape.1) {
            out.push(t.get_weight(i));
        }
        Some(out)
    }

    /// Classifica intent e grava logits na TensorArena (R3 rollout).
    /// Router NÃO deve ser re-chamado no update — usar trace retornado.
    pub fn classify_intent_with_trace(
        &self,
        text: &str,
        arena: &mut crate::arena::TensorArena,
    ) -> (&Expert, crate::r3::RouteTrace) {
        if self.experts.is_empty() {
            return (&FALLBACK_GENERATOR, crate::r3::RouteTrace {
                embedding_addr: 0, logits_addr: 0, num_experts: 1, selected_expert: 0,
                old_log_prob: 0.0, token_ids_addr: 0, token_count: 0,
            });
        }
        let text = extract_user_utterance(text);
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
                            k_nano::slog_cortex!("TRINITY", "info", "MoE router (R3): expert {} (score={:.3}) arena_used={} B",
                                self.experts[best_idx].name,
                                best_score,
                                arena.used_bytes());
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
        if self.experts.is_empty() {
            return &FALLBACK_GENERATOR;
        }
        let text = extract_user_utterance(text);
        // Tenta router neural primeiro (só no utterance)
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
                let norm = libm::sqrtf(embedding.iter().map(|v| v * v).sum::<f32>() + 1e-8);
                for v in embedding.iter_mut() {
                    *v /= norm;
                }
                let num_exp = self.experts.len();
                let emb_tensor =
                    crate::tensor::Tensor::from_row_major((1, ROUTER_HIDDEN), embedding).unwrap();
                let scores_t = weight.matmul_hybrid(&emb_tensor).unwrap();
                let mut scores = scores_t.data.clone();
                if scores.len() == num_exp {
                    softmax(&mut scores);
                    let mut best_idx = 0;
                    let mut best_score = scores[0];
                    for (i, &s) in scores.iter().enumerate().skip(1) {
                        if s > best_score {
                            best_idx = i;
                            best_score = s;
                        }
                    }
                    if best_score > 0.15 {
                        k_nano::slog_cortex!(
                            "TRINITY",
                            "info",
                            "MoE router: expert {} (score={:.3})",
                            self.experts[best_idx].name,
                            best_score
                        );
                        return &self.experts[best_idx];
                    }
                }
            }
        }
        self.classify_keywords(text)
    }

    /// Keyword path — utterance puro (já extraído).
    fn classify_keywords(&self, text: &str) -> &Expert {
        let lower = text.to_lowercase();
        let has_word = |w: &str| -> bool {
            let b = lower.as_bytes();
            let n = w.as_bytes();
            if n.is_empty() || b.len() < n.len() {
                return false;
            }
            let is_alnum = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
            let mut i = 0usize;
            while i + n.len() <= b.len() {
                if &b[i..i + n.len()] == n {
                    let left_ok = i == 0 || !is_alnum(b[i - 1]);
                    let right = i + n.len();
                    let right_ok = right == b.len() || !is_alnum(b[right]);
                    if left_ok && right_ok {
                        return true;
                    }
                }
                i += 1;
            }
            false
        };

        let is_hw_control = has_word("volume")
            || has_word("mute")
            || has_word("unmute")
            || has_word("brilho")
            || has_word("brightness")
            || lower.contains("volume para")
            || lower.contains("vol para")
            || lower.contains("vol=")
            || ((has_word("ajuste")
                || has_word("ajustar")
                || has_word("ajusta")
                || has_word("ajste")
                || has_word("set")
                || has_word("definir")
                || has_word("defina"))
                && (has_word("volume") || has_word("vol") || has_word("brilho")));

        // 1) Controles de HW/plataforma — skill direta, sem LLM / sem hw_identify 128h
        if is_hw_control {
            if let Some(e) = self.experts.iter().find(|e| e.kind == ExpertKind::HwControl) {
                k_nano::slog_cortex!("TRINITY", "info", "keyword: hw_control (volume/mute/brilho)");
                return e;
            }
        }

        // 2) Conversa / saudacao / "oi jarbas" → generator (BitNet fluente)
        let is_chat = has_word("oi")
            || has_word("ola")
            || has_word("olá")
            || has_word("hello")
            || has_word("hey")
            || has_word("hi")
            || has_word("greeting")
            || has_word("saudacao")
            || has_word("saudação")
            || lower.contains("bom dia")
            || lower.contains("boa tarde")
            || lower.contains("boa noite")
            || lower.contains("como vai")
            || lower.contains("tudo bem")
            || lower.contains("single short sentence greeting")
            || ((has_word("jarbas") || has_word("JARBAS"))
                && !is_hw_control
                && (has_word("oi")
                    || has_word("ola")
                    || has_word("olá")
                    || has_word("hello")
                    || has_word("hey")
                    || has_word("hi")
                    || has_word("greeting")
                    || has_word("generate")
                    || text.len() < 48));

        if is_chat {
            if let Some(gen) = self.experts.iter().find(|e| e.kind == ExpertKind::Generator) {
                k_nano::slog_cortex!("TRINITY", "info", "keyword: chat/saudacao → generator");
                return gen;
            }
        }

        for expert in &self.experts {
            match expert.kind {
                ExpertKind::HwControl => {}
                ExpertKind::HwIdentify => {
                    // Só identificação (PCI/USB ID) — não "hardware" genérico do catálogo.
                    let hex_id = lower.contains(':')
                        && lower.chars().filter(|c| c.is_ascii_hexdigit()).count() >= 4;
                    if has_word("pci")
                        || has_word("hwid")
                        || lower.contains("/hw")
                        || hex_id
                        || ((has_word("identifique") || has_word("identify"))
                            && (has_word("hardware")
                                || has_word("device")
                                || has_word("dispositivo")
                                || has_word("usb")))
                        || (has_word("hardware") && (has_word("qual") || has_word("id")))
                    {
                        return expert;
                    }
                }
                ExpertKind::RustCoder => {
                    if has_word("crie")
                        || has_word("create")
                        || has_word("code")
                        || has_word("write")
                        || has_word("implement")
                        || has_word("codigo")
                        || has_word("código")
                    {
                        return expert;
                    }
                }
                ExpertKind::DiskDiag => {
                    if has_word("disk")
                        || has_word("disco")
                        || has_word("smart")
                        || has_word("storage")
                        || has_word("armazenamento")
                    {
                        return expert;
                    }
                }
                ExpertKind::Security => {
                    if has_word("security")
                        || has_word("seguranca")
                        || has_word("segurança")
                        || has_word("cve")
                        || has_word("attack")
                        || has_word("ataque")
                    {
                        return expert;
                    }
                }
                ExpertKind::SpeechSynth => {
                    // Pedido explícito de sintetizar fala — não "TTS mode" meta nem volume.
                    if !is_hw_control
                        && (has_word("fale")
                            || has_word("diga")
                            || has_word("pronuncie")
                            || has_word("pronounce")
                            || (has_word("tts") && !has_word("greeting") && !has_word("mode"))
                            || (has_word("speak") && !has_word("greeting")))
                    {
                        return expert;
                    }
                }
                ExpertKind::Generator => {
                    if has_word("tempo")
                        || has_word("clima")
                        || has_word("weather")
                        || has_word("previsao")
                        || has_word("previsão")
                        || has_word("amanha")
                        || has_word("amanhã")
                        || has_word("hoje")
                        || has_word("chat")
                    {
                        return expert;
                    }
                }
                ExpertKind::Unknown => {}
            }
        }
        self.experts
            .iter()
            .find(|e| e.kind == ExpertKind::Generator)
            .or_else(|| self.experts.last())
            .unwrap_or(&FALLBACK_GENERATOR)
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

/// Generate deterministic router weights using LCG with seed 42.
/// Embedding table: VOCAB×HIDDEN random f32 in [-0.1, 0.1].
/// Weight matrix: HIDDEN×num_experts PackedTernaryTensor (-1/0/1).
pub fn generate_router_weights(num_experts: usize) -> (Vec<f32>, PackedTernaryTensor) {
    let mut seed: u32 = 42;
    let embed_size = VOCAB * ROUTER_HIDDEN;

    // Embedding table: LCG → f32 in [-0.1, 0.1]
    let mut embed = vec![0.0f32; embed_size];
    for v in embed.iter_mut() {
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        let r = (seed >> 16) & 0x7FFF;
        *v = (r as f32 / 32767.0) * 0.2 - 0.1;
    }

    // PackedTernaryTensor: same LCG as cortex::random_ternary
    let rows = ROUTER_HIDDEN;
    let cols = num_experts;
    let packed_len = (rows * cols + 3) / 4;
    let mut packed = vec![0u8; packed_len];
    for byte in packed.iter_mut() {
        let mut b = 0u8;
        for j in 0..4 {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            let r = (seed % 3) as i8;
            let tv = if r == 2 { -1i8 } else { r };
            let bits = match tv {
                -1 => 0b10,
                0 => 0b00,
                1 => 0b01,
                _ => 0b00,
            };
            b |= bits << (j * 2);
        }
        *byte = b;
    }
    let weight = PackedTernaryTensor { shape: (rows, cols), packed_data: packed };

    (embed, weight)
}

/// Carrega pesos do router MoE a partir de arquivo .bitnet v3+.
/// Procura tensores chamados "router_embed" (VOCAB*HIDDEN f32) e
/// "router_weight" (HIDDEN*MAX_EXPERTS i8 ternário).
/// Retorna true se carregado com sucesso.
pub fn load_router_from_file(data: &[u8]) -> bool {
    if data.len() < 32 { return false; }
    let r4 = |off: usize| u32::from_le_bytes(data[off..off+4].try_into().unwrap_or([0; 4]));
    if r4(0) != 0xBE11BE11 { return false; }

    let _ver = r4(4);
    let _vocab = r4(8) as usize;
    let hidden = r4(12) as usize;
    let _layers = r4(16);

    if hidden != ROUTER_HIDDEN {
        k_nano::slog_cortex!("TRINITY", "warn", "Router hidden mismatch: file={} expected={}", hidden, ROUTER_HIDDEN);
        return false;
    }

    // Pula até os tensores
    let off = 32 + 16 + 4; // header + model_type(16) + ntensors(4)
    if off + 4 > data.len() { return false; }
    let ntensors = r4(off - 4) as usize;

    let mut pos = off;
    let mut embed_loaded = false;
    let mut weight_loaded = false;
    let mut embed_vec: Option<Vec<f32>> = None;
    let mut weight_tensor: Option<PackedTernaryTensor> = None;

    for _ in 0..ntensors {
        if pos + 64 + 8 > data.len() { break; }
        let name_bytes = &data[pos..pos+64];
        let name_end = name_bytes.iter().position(|&b| b==0).unwrap_or(64);
        let name = core::str::from_utf8(&name_bytes[..name_end]).unwrap_or("");
        let n_orig = r4(pos + 64) as usize;
        let n_quant = r4(pos + 64 + 4) as usize;
        let f32_bytes = n_orig * 4;
        pos += 64 + 8;

        if name.contains("router_embed") {
            if pos + f32_bytes <= data.len() {
                let floats: Vec<f32> = data[pos..pos + f32_bytes]
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                    .collect();
                if floats.len() == VOCAB * ROUTER_HIDDEN {
                    embed_vec = Some(floats);
                    embed_loaded = true;
                }
            }
        } else if name.contains("router_weight") {
            if pos + n_orig <= data.len() {
                // n_orig = HIDDEN * MAX_EXPERTS i8 values
                let weights: Vec<i8> = data[pos..pos + n_orig]
                    .iter()
                    .map(|&b| b as i8)
                    .collect();
                if weights.len() == ROUTER_HIDDEN * ROUTER_MAX_EXPERTS {
                    weight_tensor = Some(PackedTernaryTensor {
                        shape: (ROUTER_HIDDEN, ROUTER_MAX_EXPERTS),
                        packed_data: PackedTernaryTensor::pack_weights(&weights),
                    });
                    weight_loaded = true;
                }
            }
        }
        pos += f32_bytes + n_quant;
    }

    if embed_loaded && weight_loaded {
        // Store in a static for the router to pick up
        *ROUTER_EMBED.lock() = embed_vec;
        *ROUTER_WEIGHT.lock() = weight_tensor;
        k_nano::slog_cortex!("TRINITY", "info", "Router MoE loaded from file: {} dim, {} experts", ROUTER_HIDDEN, ROUTER_MAX_EXPERTS);
        true
    } else {
        k_nano::slog_cortex!("TRINITY", "warn", "Router file missing tensors: embed={} weight={}", embed_loaded, weight_loaded);
        false
    }
}

/// Static storage for router weights loaded from file (before TrinityRouter init).
static ROUTER_EMBED: spin::Mutex<Option<Vec<f32>>> = spin::Mutex::new(None);
static ROUTER_WEIGHT: spin::Mutex<Option<PackedTernaryTensor>> = spin::Mutex::new(None);

/// Tenta carregar router do arquivo; se falhar, gera determinístico (seed=42).
/// Deve ser chamado ANTES de TrinityRouter::new() ou load_router().
pub fn init_router_weights(num_experts: usize) -> (Vec<f32>, PackedTernaryTensor) {
    // Try to take from file-loaded statics
    let embed = ROUTER_EMBED.lock().take();
    let weight = ROUTER_WEIGHT.lock().take();
    if let (Some(e), Some(w)) = (embed, weight) {
        return (e, w);
    }
    // Fallback: deterministic LCG
    generate_router_weights(num_experts)
}

pub fn init_trinity() -> TrinityRouter {
    let mut router = TrinityRouter::new();
    // Generator primeiro = default seguro se find() falhar (Sprint 107 Loop2).
    router.register_expert(Expert {
        kind: ExpertKind::Generator, name: "generator",
        description: "Geracao generica (ModelHub: pro/fast/tiny)", weight: None,
    });
    router.register_expert(Expert {
        kind: ExpertKind::HwControl, name: "hw_control",
        description: "Controle HW: volume, mute, brilho (skill, sem LLM)", weight: None,
    });
    router.register_expert(Expert {
        kind: ExpertKind::HwIdentify, name: "hw_identify",
        description: "Identifica dispositivos de hardware por PCI ID", weight: None,
    });
    router.register_expert(Expert {
        kind: ExpertKind::RustCoder, name: "rust_coder",
        description: "Gera codigo Rust sob demanda (2B/3B se carregado)", weight: None,
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
