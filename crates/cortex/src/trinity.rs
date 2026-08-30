//! Trinity MoE — um único router no crate cortex (fonte da verdade).
//! `moe_router_loaded` = pesos TREINADOS. LCG seed=42 não roteia (keyword).

use crate::tensor::PackedTernaryTensor;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::vec;
use core::sync::atomic::{AtomicBool, Ordering};
use event_bus::{CapabilityToken, Event};
use lazy_static::lazy_static;
use ticket_lock::TicketLock;

/// EventBus: MoE treinado vs keyword (Hermes/Jarbas observam).
pub const TOPIC_CORTEX_POSTURE: &str = "CORTEX_POSTURE";

static MOE_POSTURE_TRAINED: AtomicBool = AtomicBool::new(false);

/// HUD/gates sem lock: true só após `load_router(..., true)` ou pesos federados.
pub fn moe_posture_trained() -> bool {
    MOE_POSTURE_TRAINED.load(Ordering::Relaxed)
}

pub fn publish_cortex_posture(trained: bool) {
    MOE_POSTURE_TRAINED.store(trained, Ordering::Release);
    let payload: &[u8] = if trained {
        b"CORTEX_POSTURE:moe=trained"
    } else {
        b"CORTEX_POSTURE:moe=keyword"
    };
    let _ = k_nano::EVENT_BUS.publish(Event {
        id: 0,
        topic: String::from(TOPIC_CORTEX_POSTURE),
        payload: payload.to_vec(),
        token: CapabilityToken::Legacy(1),
    });
}

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
    /// Só true se veio de ROUTER.BITNET (não LCG).
    router_trained: bool,
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
        TrinityRouter {
            experts: Vec::new(),
            router_weight: None,
            router_embed: None,
            router_trained: false,
        }
    }

    pub fn register_expert(&mut self, expert: Expert) {
        k_nano::slog_cortex!("TRINITY", "info", "Expert '{}' registered: {}", expert.name, expert.description);
        self.experts.push(expert);
    }

    /// Carrega pesos do router MoE.
    /// embed: VOCAB * HIDDEN floats (embedding table)
    /// weight: PackedTernaryTensor (HIDDEN x NUM_EXPERTS)
    /// `trained`: true = pesos vindos de arquivo treinado; false = fallback
    /// determinístico (LCG seed=42, NÃO treinado). O log distingue os dois —
    /// nunca anunciar "loaded/trained" para ruído determinístico (auditoria 7.2).
    pub fn load_router(&mut self, embed: Vec<f32>, weight: PackedTernaryTensor, trained: bool) {
        if !trained {
            // Honesty: LCG seed=42 não entra no matmul nem no flag "loaded".
            self.router_embed = None;
            self.router_weight = None;
            self.router_trained = false;
            publish_cortex_posture(false);
            k_nano::slog_cortex!(
                "TRINITY",
                "warn",
                "Router MoE recusou fallback LCG — roteamento = keyword (ADR-0083/SESSION_273)"
            );
            let _ = (embed, weight);
            return;
        }
        self.router_embed = Some(embed);
        self.router_weight = Some(weight);
        self.router_trained = true;
        publish_cortex_posture(true);
        k_nano::slog_cortex!(
            "TRINITY",
            "info",
            "Router MoE loaded (trained): {} dim, {} experts",
            ROUTER_HIDDEN,
            self.experts.len()
        );
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
        if self.router_embed.is_some() {
            self.router_trained = true;
            publish_cortex_posture(true);
        }
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
        if self.router_trained {
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
        } // router_trained
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
        // Neural só com router TREINADO. LCG seed=42 não é MoE (ADR-0083 / SESSION_273).
        if self.router_trained {
            if let (Some(ref embed_table), Some(ref weight)) =
                (&self.router_embed, &self.router_weight)
            {
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
                let num_exp = self.experts.len().min(ROUTER_MAX_EXPERTS);
                let emb_tensor =
                    crate::tensor::Tensor::from_row_major((1, ROUTER_HIDDEN), embedding).unwrap();
                let scores_t = weight.matmul_hybrid(&emb_tensor).unwrap();
                let mut scores = scores_t.data.clone();
                scores.truncate(num_exp);
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
            || has_word("volumen")
            || has_word("silencio")
            || has_word("luminosidad")
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
                        || has_word("seguridad")
                        || has_word("vulnerabilidade")
                        || has_word("vulnerability")
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

    /// Slice dos experts registrados (telemetria/auditoria).
    pub fn experts(&self) -> &[Expert] {
        &self.experts
    }

    pub fn agent_count(&self) -> usize { self.experts.len() }

    /// True só com pesos de arquivo treinado. LCG/ausente = keyword (honesto).
    pub fn moe_router_loaded(&self) -> bool {
        self.router_trained
            && self.router_weight.is_some()
            && self.router_embed.is_some()
    }

    /// Tem expert generator registrado (rota default segura).
    pub fn has_generator(&self) -> bool {
        self.experts.iter().any(|e| e.kind == ExpertKind::Generator)
    }

    /// Carrega pesos de um expert na Cortex Arena sob demanda (Efeito Matrix).
    /// Retorna Some(&PackedTernaryTensor) se o expert ja esta residente.
    /// Se nao, verifica se o ModelSlot correspondente esta loaded no boot.
    pub fn get_or_mmap_expert(&self, kind: ExpertKind) -> Option<&PackedTernaryTensor> {
        // 1. Ja residente?
        if let Some(e) = self.experts.iter().find(|e| e.kind == kind) {
            return e.weight.as_ref();
        }
        // 2. Mapear kind -> ModelSlot para buscar no FAT
        let slot = match kind {
            ExpertKind::HwIdentify => Some(crate::model_hub::ModelSlot::HwExpert),
            ExpertKind::RustCoder => Some(crate::model_hub::ModelSlot::RustCoder),
            ExpertKind::Generator => Some(crate::model_hub::ModelSlot::GeneratorPro),
            _ => None,
        };
        let slot = slot?;
        // 3. Se o slot ja foi carregado no boot, o expert esta residente
        if crate::model_hub::slot_loaded(slot) {
            let names = crate::model_hub::fat_names_for(slot);
            if let Some(&name) = names.first() {
                k_nano::slog_cortex!("TRINITY", "info",
                    "Expert {:?} residente no slot {} (pre-loaded)", kind, name);
            }
        }
        None
    }

    /// Define os pesos de um expert diretamente (usado pelo injection pipeline).
    pub fn set_expert_weight(&mut self, kind: ExpertKind, weight: PackedTernaryTensor) {
        if let Some(e) = self.experts.iter_mut().find(|e| e.kind == kind) {
            k_nano::slog_cortex!("TRINITY", "info",
                "Expert {:?} pesos injetados: {}KB (Efeito Matrix)",
                kind, weight.packed_data.len() / 1024);
            e.weight = Some(weight);
        }
    }

    /// Conta expert com pesos residentes (para telemetria/HUD).
    pub fn expert_resident_count(&self) -> usize {
        self.experts.iter().filter(|e| e.weight.is_some()).count()
    }

    /// Tamanho total dos pesos residentes em bytes.
    pub fn expert_resident_bytes(&self) -> usize {
        self.experts.iter()
            .filter_map(|e| e.weight.as_ref())
            .map(|w| w.packed_data.len())
            .sum()
    }
}

/// Nome canônico de cada ExpertKind (para logging/telemetria).
pub fn expert_kind_name(kind: ExpertKind) -> &'static str {
    match kind {
        ExpertKind::HwIdentify => "hw_identify",
        ExpertKind::HwControl => "hw_control",
        ExpertKind::RustCoder => "rust_coder",
        ExpertKind::DiskDiag => "disk_diag",
        ExpertKind::Security => "security",
        ExpertKind::SpeechSynth => "speech_synth",
        ExpertKind::Generator => "generator",
        ExpertKind::Unknown => "unknown",
    }
}

/// Generate RANDOM (UNTRAINED) router weights using LCG with seed 42.
/// Embedding table: VOCAB×HIDDEN random f32 in [-0.1, 0.1].
/// Weight matrix: HIDDEN×num_experts PackedTernaryTensor (-1/0/1).
/// UNTRAINED — o roteamento efetivo é keyword fallback (classify_keywords)
/// até um router treinado passar o gate do ADR-0083 (§5.3, >80% holdout).
pub fn generate_random_router_weights(num_experts: usize) -> (Vec<f32>, PackedTernaryTensor) {
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
    // Layout v6 posicional (ADR-0085 model_type=2, tools/train_router.py export_bitnet):
    //   preamble: magic u32 + version u16 + num_params u64 + model_type u8 + reserved[3]
    //   router bloco: vocab u32 + hidden u16 + n_experts u16
    //   embed f32[vocab*hidden] + weight i8[hidden*n_experts] (row-major)
    if data.len() < 18 + 8 { return false; }
    let r4 = |off: usize| u32::from_le_bytes(data[off..off+4].try_into().unwrap_or([0; 4]));
    let r2 = |off: usize| u16::from_le_bytes(data[off..off+2].try_into().unwrap_or([0; 2]));
    if r4(0) != 0xBE11BE11 { return false; }

    let _version = r2(4);
    let vocab = r4(18) as usize;
    let hidden = r2(22) as usize;
    let n_exp = r2(24) as usize;

    if hidden != ROUTER_HIDDEN {
        k_nano::slog_cortex!("TRINITY", "warn", "Router hidden mismatch: file={} expected={}", hidden, ROUTER_HIDDEN);
        return false;
    }
    if n_exp < 1 || n_exp > ROUTER_MAX_EXPERTS {
        k_nano::slog_cortex!("TRINITY", "warn", "Router n_experts fora do range: {}", n_exp);
        return false;
    }

    // Dados posicionais: embed (vocab*hidden f32) + weight (hidden*n_exp i8)
    let mut pos = 26;
    let embed_bytes = vocab * hidden * 4;
    if pos + embed_bytes + hidden * n_exp > data.len() { return false; }

    let floats: Vec<f32> = data[pos..pos + embed_bytes]
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect();
    if floats.len() != VOCAB * ROUTER_HIDDEN {
        k_nano::slog_cortex!("TRINITY", "warn", "Router embed count: {} (esperado {})", floats.len(), VOCAB * ROUTER_HIDDEN);
        return false;
    }
    pos += embed_bytes;

    let weights: Vec<i8> = data[pos..pos + hidden * n_exp]
        .iter()
        .map(|&b| b as i8)
        .collect();
    let weight_tensor = PackedTernaryTensor {
        shape: (ROUTER_HIDDEN, n_exp),
        packed_data: PackedTernaryTensor::pack_weights(&weights),
    };

    *ROUTER_EMBED.lock() = Some(floats);
    *ROUTER_WEIGHT.lock() = Some(weight_tensor);
    k_nano::slog_cortex!(
        "TRINITY",
        "info",
        "Router MoE loaded from file (v6): {} dim, {} experts",
        ROUTER_HIDDEN,
        n_exp
    );
    true
}

/// Static storage for router weights loaded from file (before TrinityRouter init).
static ROUTER_EMBED: spin::Mutex<Option<Vec<f32>>> = spin::Mutex::new(None);
static ROUTER_WEIGHT: spin::Mutex<Option<PackedTernaryTensor>> = spin::Mutex::new(None);

/// Toma pesos parseados por `load_router_from_file`. None = sem arquivo (keyword).
/// Não gera LCG — o boot não pode fingir MoE (SESSION_273).
pub fn init_router_weights(_num_experts: usize) -> Option<(Vec<f32>, PackedTernaryTensor)> {
    let embed = ROUTER_EMBED.lock().take();
    let weight = ROUTER_WEIGHT.lock().take();
    match (embed, weight) {
        (Some(e), Some(w)) => Some((e, w)),
        _ => None,
    }
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

lazy_static! {
    /// Fonte única — Hermes/Jarbas/bin reexportam. Não duplicar (SESSION_237/273).
    pub static ref TRINITY: TicketLock<TrinityRouter> = TicketLock::new(init_trinity());
}

#[cfg(test)]
mod tests {
    use super::{
        generate_random_router_weights, init_trinity, load_router_from_file, ROUTER_EMBED,
        ROUTER_HIDDEN, ROUTER_WEIGHT,
    };

    /// Monta um blob v6 posicional em memória (espelha tools/train_router.py export_bitnet)
    /// e valida que o loader Rust o parseia — a ponte treino→kernel (item 11 ADR-0083).
    #[test]
    fn load_router_v6_roundtrip() {
        const VOCAB: usize = 99;
        const N_EXPERTS: usize = 7;
        let embed: Vec<f32> = (0..VOCAB * ROUTER_HIDDEN).map(|i| (i % 7) as f32 * 0.01).collect();
        let weights: Vec<i8> = (0..ROUTER_HIDDEN * N_EXPERTS).map(|i| (i % 3) as i8 - 1).collect();

        let mut blob = Vec::new();
        blob.extend_from_slice(&0xBE11BE11u32.to_le_bytes()); // magic
        blob.extend_from_slice(&6u16.to_le_bytes());          // version
        blob.extend_from_slice(&0u64.to_le_bytes());          // num_params (informativo)
        blob.push(2u8);                                       // model_type=router
        blob.extend_from_slice(&[0u8; 3]);                    // reserved
        blob.extend_from_slice(&(VOCAB as u32).to_le_bytes()); // vocab
        blob.extend_from_slice(&(ROUTER_HIDDEN as u16).to_le_bytes()); // hidden
        blob.extend_from_slice(&(N_EXPERTS as u16).to_le_bytes());     // n_experts
        for f in &embed { blob.extend_from_slice(&f.to_le_bytes()); }
        blob.extend_from_slice(weights.iter().map(|&w| w as u8).collect::<Vec<u8>>().as_slice());

        assert!(load_router_from_file(&blob), "loader deve aceitar blob v6");
        assert!(ROUTER_EMBED.lock().is_some(), "embed carregado");
        assert!(ROUTER_WEIGHT.lock().is_some(), "weight carregado");

        // Limpa statics p/ não vazar para outros testes.
        *ROUTER_EMBED.lock() = None;
        *ROUTER_WEIGHT.lock() = None;
    }

    #[test]
    fn untrained_lcg_is_not_moe_and_keywords_win() {
        let r = init_trinity();
        assert!(!r.moe_router_loaded());
        assert_eq!(r.classify_intent("mute volume").name, "hw_control");
        let n = r.agent_count();
        let (embed, weight) = generate_random_router_weights(n);
        let mut r2 = init_trinity();
        r2.load_router(embed, weight, false);
        assert!(!r2.moe_router_loaded());
        assert!(!super::moe_posture_trained());
        assert_eq!(r2.classify_intent("mute volume").name, "hw_control");
    }
}
