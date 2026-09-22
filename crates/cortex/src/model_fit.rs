//! FitPolicy Neural — scoring de footprint inspirado em llmfit (sem std/host).
//! Dono das fórmulas: cortex (ModelHub). k_ai re-exporta para MemoryAgent.
//! VRAM honesty: se total_vram_mb=0, score usa só RAM.

use core::sync::atomic::{AtomicU64, Ordering};

/// Classe de encaixe (espelho llmfit Perfect/Good/Marginal + TooTight/Deny).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FitClass {
    Perfect = 0,
    Good = 1,
    Marginal = 2,
    TooTight = 3,
    Deny = 4,
}

impl FitClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Perfect => "Perfect",
            Self::Good => "Good",
            Self::Marginal => "Marginal",
            Self::TooTight => "TooTight",
            Self::Deny => "Deny",
        }
    }

    /// Aceitável para ativar slot sem escalate.
    pub fn is_acceptable(self) -> bool {
        matches!(self, Self::Perfect | Self::Good | Self::Marginal)
    }

    /// Preferível (Good+).
    pub fn is_good_plus(self) -> bool {
        matches!(self, Self::Perfect | Self::Good)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FitReport {
    pub class: FitClass,
    pub usage_pct_x100: u32,
    pub model_mb: u64,
    pub free_after_mb: u64,
    /// Estimativa grosseira tok/s (bandwidth proxy); 0 = N/A.
    pub tok_s_est: u32,
}

/// RAM medida pelo MemoryAgent (MB); ModelHub lê no select.
static HOST_RAM_MB: AtomicU64 = AtomicU64::new(0);
static HOST_VRAM_MB: AtomicU64 = AtomicU64::new(0);

pub fn set_host_memory(ram_mb: u64, vram_mb: u64) {
    HOST_RAM_MB.store(ram_mb, Ordering::Release);
    HOST_VRAM_MB.store(vram_mb, Ordering::Release);
}

pub fn host_ram_mb() -> u64 {
    HOST_RAM_MB.load(Ordering::Acquire)
}

pub fn host_vram_mb() -> u64 {
    HOST_VRAM_MB.load(Ordering::Acquire)
}

/// BitNet ternário 2-bit packing: params * 2 bits / 8 → bytes → MB.
pub fn estimate_bitnet_mb(params: u64) -> u64 {
    let bytes = params.saturating_mul(2) / 8;
    (bytes / (1024 * 1024)).max(if params > 0 { 1 } else { 0 })
}

/// Falcon3 3B — preset principal (v6 genérico já suporta dims arbitrárias).
/// FALCON3 constants removidos (Fase 1 autonomia).
/// Todos os valores agora vem de parse_model_header() em runtime.
/// Use loaded_model_header() ou v6_file_size() em vez de constantes.
pub fn estimate_kv_mb(params: u64) -> u64 {
    let mb = params / 40 / (1024 * 1024);
    mb.clamp(8, 4096)
}

pub fn estimate_heap_mb(params: u64) -> u64 {
    let detected = k_nano::memory::TOTAL_RAM_MB.load(core::sync::atomic::Ordering::Relaxed) as u64;
    estimate_heap_mb_at(params, detected)
}

pub fn estimate_heap_mb_at(params: u64, ram_mb: u64) -> u64 {
    let model_gb = params as f64 / 1_000_000_000.0;
    let heap = (model_gb * 100.0) as u64;
    let ram_cap = k_nano::memory::heap_budget_mb(ram_mb) as u64;
    heap.clamp(128, ram_cap.max(128))
}

/// AIOS: residente vs AirLLM (GGUF layer-wise). Usa RAM passada (testável).
pub fn needs_airllm(params: u64, model_file_mb: u64) -> bool {
    let ram = k_nano::memory::TOTAL_RAM_MB.load(core::sync::atomic::Ordering::Relaxed) as u64;
    needs_airllm_at(ram, params, model_file_mb)
}

pub fn needs_airllm_at(ram_mb: u64, params: u64, model_file_mb: u64) -> bool {
    if ram_mb == 0 {
        return false;
    }
    let budget = k_nano::memory::heap_budget_mb(ram_mb) as u64;
    let need = model_file_mb.saturating_add(estimate_heap_mb_at(params, ram_mb));
    need > budget
}

fn classify(usage_x10000: u32) -> FitClass {
    // usage_x10000: 5000 = 50%
    if usage_x10000 <= 5000 {
        FitClass::Perfect
    } else if usage_x10000 <= 8000 {
        FitClass::Good
    } else if usage_x10000 <= 9500 {
        FitClass::Marginal
    } else if usage_x10000 <= 10500 {
        FitClass::TooTight
    } else {
        FitClass::Deny
    }
}

fn tok_s_est(needed_mb: u64, ram_mb: u64, vram_mb: u64) -> u32 {
    let bw = if vram_mb > 0 { vram_mb } else { ram_mb };
    if bw == 0 || needed_mb == 0 {
        return 0;
    }
    // (bw/1024)*40 / (needed/256) — proxy informativo
    let num = (bw / 1024).saturating_mul(40).saturating_mul(256);
    let den = needed_mb.max(1);
    (num / den).max(1) as u32
}

pub fn score_fit(
    model_mb: u64,
    kv_mb: u64,
    heap_mb: u64,
    total_ram_mb: u64,
    total_vram_mb: u64,
) -> FitReport {
    let needed = model_mb.saturating_add(kv_mb).saturating_add(heap_mb);
    let pool = if total_ram_mb > 0 {
        total_ram_mb
    } else {
        1
    };
    let usage_x10000 = if pool > 0 {
        ((needed.saturating_mul(10_000)) / pool) as u32
    } else {
        20_000
    };
    let class = if model_mb == 0 && kv_mb == 0 {
        FitClass::Perfect
    } else {
        classify(usage_x10000)
    };
    let free = pool.saturating_sub(needed);
    FitReport {
        class,
        usage_pct_x100: usage_x10000,
        model_mb,
        free_after_mb: free,
        tok_s_est: tok_s_est(needed, total_ram_mb, total_vram_mb),
    }
}

/// Footprints por slot (MB). Para slots LLM tenta header dinâmico primeiro
/// (parse_model_header → file_size_mb), fallback para constantes conhecidas.
/// smoke/hwexpert mantêm fallback pequeno (4/1 MB) quando sem header.
pub fn slot_footprint_mb(slot_name: &str) -> Option<u64> {
    // header dinâmico se algum LLM foi carregado
    let dyn_mb = crate::model::loaded_model_header().map(|h| h.file_size_mb());
    match slot_name.to_ascii_lowercase().as_str() {
        "generator_fast" | "fast" | "850m" | "850" => {
            dyn_mb.or(Some(220))
        }
        "active" | "current" | "generator" => {
            dyn_mb.or(Some(Falcon3Kind::Daily3B.file_mb_hint()))
        }
        "generator_pro" | "pro" | "7b" | "falcon7b" | "falcon3-7b" => {
            dyn_mb.or(Some(Falcon3Kind::Goal7B.file_mb_hint()))
        }
        "10b" | "falcon10b" | "falcon3-10b" | "large" => {
            dyn_mb.or(Some(Falcon3Kind::Large10B.file_mb_hint()))
        }
        "3b" | "bitnet3b" | "falcon3" | "falcon" | "f3" | "falcon3b" | "falcon-3b"
        | "falcon3-3b" | "daily" | "lab" => dyn_mb.or(Some(Falcon3Kind::Daily3B.file_mb_hint())),
        "1b" | "falcon1b" | "falcon3-1b" | "tiny1b" => {
            dyn_mb.or(Some(Falcon3Kind::Tiny1B.file_mb_hint()))
        }
        "tinystories" | "tiny" | "smoke" => Some(4),
        "rust_coder" | "rustcoder" => Some(260),
        "hw_identify" | "hwexpert" => Some(1),
        "learner" | "qwen05" | "qwen0.5b" => {
            dyn_mb.or(Some(Falcon3Kind::Tiny1B.file_mb_hint()))
        }
        "13" | "1.3b" | "xl" => Some(320),
        "2b" => Some(590),
        _ => None,
    }
}

/// Score de um slot pelo footprint estático + RAM host registrada.
pub fn score_slot(slot_name: &str) -> Option<FitReport> {
    let model_mb = slot_footprint_mb(slot_name)?;
    let ram = host_ram_mb();
    let vram = host_vram_mb();
    if ram == 0 {
        // Sem medida ainda — assume Marginal (não Deny) para não matar boot cedo
        return Some(FitReport {
            class: FitClass::Marginal,
            usage_pct_x100: 9000,
            model_mb,
            free_after_mb: 0,
            tok_s_est: 0,
        });
    }
    let kv = (model_mb / 6).max(8);
    let heap = 128u64;
    Some(score_fit(model_mb, kv, heap, ram, vram))
}

/// True se slot pode ser escolhido sem escalate por memória.
pub fn slot_fits(slot_name: &str) -> bool {
    match score_slot(slot_name) {
        Some(r) => r.class.is_acceptable(),
        None => true,
    }
}

/// TooTight ou Deny → escalate / fallback.
pub fn slot_too_tight(slot_name: &str) -> bool {
    match score_slot(slot_name) {
        Some(r) => matches!(r.class, FitClass::TooTight | FitClass::Deny),
        None => false,
    }
}

/// Família Falcon3 Instruct **1.58bit** (tiiuae) — opções 1B / 3B / 7B / 10B.
/// Lab default = 3B (ADR-0101). Shapes HF (config.json); NÃO copiar BitNet-2B 2560.
/// Carga: residente se couber; senão AirLLM/GGUF (ADR-0046).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Falcon3Kind {
    Tiny1B,
    Daily3B,
    Goal7B,
    Large10B,
}

impl Falcon3Kind {
    pub const ALL: [Falcon3Kind; 4] = [
        Falcon3Kind::Tiny1B,
        Falcon3Kind::Daily3B,
        Falcon3Kind::Goal7B,
        Falcon3Kind::Large10B,
    ];

    pub fn params(self) -> u64 {
        match self {
            Self::Tiny1B => 1_000_000_000,
            Self::Daily3B => 3_000_000_000,
            Self::Goal7B => 7_000_000_000,
            Self::Large10B => 10_000_000_000,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tiny1B => "falcon3-1b",
            Self::Daily3B => "falcon3-3b",
            Self::Goal7B => "falcon3-7b",
            Self::Large10B => "falcon3-10b",
        }
    }

    /// Parse aliases: `1b`/`3b`/`7b`/`10b` / `falcon3-Nb` / `tiny`/`daily`/`pro`/`large`.
    pub fn from_str(s: &str) -> Option<Self> {
        let u = s.trim().to_ascii_lowercase();
        match u.as_str() {
            "1b" | "falcon3-1b" | "falcon1b" | "tiny" | "tiny1b" => Some(Self::Tiny1B),
            "3b" | "falcon3" | "falcon3-3b" | "falcon3b" | "daily" | "daily3b" | "lab" => {
                Some(Self::Daily3B)
            }
            "7b" | "falcon3-7b" | "falcon7b" | "pro" | "goal7b" => Some(Self::Goal7B),
            "10b" | "falcon3-10b" | "falcon10b" | "large" | "large10b" => Some(Self::Large10B),
            _ => None,
        }
    }

    /// Dims HF 1.58bit (SESSION_298 / ADR-0101 + config 10B).
    /// 7B e 10B compartilham hidden/FFN; diferem em layers (28 vs 40).
    pub fn hidden(self) -> usize {
        match self {
            Self::Tiny1B => 2048,
            Self::Daily3B | Self::Goal7B | Self::Large10B => 3072,
        }
    }

    pub fn intermediate(self) -> usize {
        match self {
            Self::Tiny1B => 8192,
            Self::Daily3B => 9216,
            Self::Goal7B | Self::Large10B => 23040,
        }
    }

    pub fn layers(self) -> usize {
        match self {
            Self::Tiny1B => 18,
            Self::Daily3B => 22,
            Self::Goal7B => 28,
            Self::Large10B => 40,
        }
    }

    pub fn num_heads(self) -> usize {
        match self {
            Self::Tiny1B => 8,
            Self::Daily3B | Self::Goal7B | Self::Large10B => 12,
        }
    }

    pub fn kv_heads(self) -> usize {
        4
    }

    pub fn head_dim(self) -> usize {
        256
    }

    /// max_position_embeddings HF 1.58bit Instruct (ADR-0101).
    /// Runtime ainda pode clamp≤4096 (OOM mask) — este é o contrato do checkpoint.
    pub fn ctx_len(self) -> usize {
        match self {
            Self::Tiny1B => 8192,
            Self::Daily3B => 4096,
            Self::Goal7B | Self::Large10B => 32768,
        }
    }

    /// rope_theta HF (todos Falcon3-*-1.58bit = 1000042).
    pub fn rope_theta(self) -> f32 {
        1_000_042.0
    }

    pub fn file_mb_hint(self) -> u64 {
        match self {
            Self::Tiny1B => 350,
            Self::Daily3B => 989,
            Self::Goal7B => 1780,
            Self::Large10B => 2500,
        }
    }

    /// HF repo 1.58bit Instruct.
    pub fn hf_repo(self) -> &'static str {
        match self {
            Self::Tiny1B => "tiiuae/Falcon3-1B-Instruct-1.58bit",
            Self::Daily3B => "tiiuae/Falcon3-3B-Instruct-1.58bit",
            Self::Goal7B => "tiiuae/Falcon3-7B-Instruct-1.58bit",
            Self::Large10B => "tiiuae/Falcon3-10B-Instruct-1.58bit",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmTier {
    Tight,
    Daily,
    Goal,
    Large,
}

#[derive(Debug, Clone, Copy)]
pub struct LlmBootPlan {
    pub tier: LlmTier,
    pub pick: Falcon3Kind,
    pub load_daily_3b: bool,
    pub load_pro_7b_resident: bool,
    pub try_7b_airllm: bool,
    pub try_10b: bool,
    pub max_resident_mb: u64,
}

impl LlmBootPlan {
    pub fn as_str(self) -> &'static str {
        match (self.load_pro_7b_resident, self.try_7b_airllm, self.pick) {
            (true, _, _) => "resident-7b",
            (false, true, _) => "airllm-7b",
            (_, _, Falcon3Kind::Large10B) => "resident-10b",
            (_, _, Falcon3Kind::Daily3B) => "resident-3b",
            (_, _, Falcon3Kind::Tiny1B) => "resident-1b",
            _ => "falcon3-fit",
        }
    }
}

/// Ordem FAT: **3B lab primeiro** (ADR-0101), depois GGUF AirLLM, 7B/10B opcionais, 1B comparativo.
pub fn falcon3_boot_names() -> &'static [&'static str] {
    &[
        "FALCON3.V6", "FALCON3B.v6", "FALCON3B.BIN", "FALCN3B.GGUF",
        "PRO.v6", "PRO.BIN", "FALCON7B.v6", "FALCON7B.BIN",
        "PRO.GGUF", "FALCN7B.GGUF",
        "FALCON10.v6", "FALCON10.BIN", "F10B.v6", "FALCN10.GGUF",
        "FALCON1B.v6", "FALCON1B.BIN", "F1B.v6", "FALCN1B.GGUF",
        "BITNET2B.v6", "BITNET2B.BIN", "BITNET13.BIN", "BITNET850.BIN",
        "MICRO.BITNET", "LLAMA8B.BIN",
    ]
}

/// M7: match por **prefixo** FALCON* (um filename FAT 8.3 nunca contém FALCON no
/// meio) + alias legado `BITNET3B*` → Daily3B (o nome BITNET3B era o SKU errado —
/// SESSION_298; o blob de 3B lab é Falcon3).
pub fn falcon3_kind_of_name(name: &str) -> Option<Falcon3Kind> {
    let u = name.to_ascii_uppercase();
    if u.starts_with("BITNET3B") {
        return Some(Falcon3Kind::Daily3B);
    }
    // Tokens de tamanho primeiro ("FALCON3_1B.V6" é 1B, não 3B — SESSION_298).
    if u.contains("10B") || u.starts_with("FALCON10") || u.starts_with("FALCN10") || u.starts_with("F10B") {
        return Some(Falcon3Kind::Large10B);
    }
    if u.contains("PRO") || u.contains("7B") || u.starts_with("FALCON7") || u.starts_with("FALCN7") {
        return Some(Falcon3Kind::Goal7B);
    }
    if u.contains("1B") || u.starts_with("FALCN1") || u.starts_with("F1B") {
        return Some(Falcon3Kind::Tiny1B);
    }
    if u.starts_with("FALCON3") || u.starts_with("FALCN3") || u.starts_with("F3B") {
        return Some(Falcon3Kind::Daily3B);
    }
    None
}

pub fn kind_mask_bit(k: Falcon3Kind) -> u8 {
    match k {
        Falcon3Kind::Tiny1B => 1,
        Falcon3Kind::Daily3B => 2,
        Falcon3Kind::Goal7B => 4,
        Falcon3Kind::Large10B => 8,
    }
}

pub fn resident_footprint_mb(file_mb: u64) -> u64 {
    file_mb.saturating_add((file_mb / 6).max(8)).saturating_add(128)
}

/// Ainda cabe mais um blob residente no budget (pack, não “só o primeiro”).
pub fn pack_resident_ok(ram_mb: u64, already_mb: u64, file_mb: u64) -> bool {
    let budget = k_nano::memory::heap_budget_mb(ram_mb) as u64;
    let need = already_mb.saturating_add(resident_footprint_mb(file_mb));
    need <= budget
}

pub fn hub_slot_for_kind(k: Falcon3Kind) -> crate::model_hub::ModelSlot {
    match k {
        Falcon3Kind::Goal7B | Falcon3Kind::Large10B => crate::model_hub::ModelSlot::GeneratorPro,
        Falcon3Kind::Daily3B => crate::model_hub::ModelSlot::Active,
        Falcon3Kind::Tiny1B => crate::model_hub::ModelSlot::Learner,
    }
}

pub fn llm_boot_plan(ram_mb: u64) -> LlmBootPlan {
    let budget = k_nano::memory::heap_budget_mb(ram_mb) as u64;
    let slack = (budget / 16).max(64);
    let max_resident_mb = budget.saturating_sub(slack).max(64);
    let mb7 = Falcon3Kind::Goal7B.file_mb_hint();
    let air7 = needs_airllm_at(ram_mb, Falcon3Kind::Goal7B.params(), mb7);
    let res7 = !air7 && ram_mb > 0;
    let res10 = !needs_airllm_at(
        ram_mb,
        Falcon3Kind::Large10B.params(),
        Falcon3Kind::Large10B.file_mb_hint(),
    ) && ram_mb > 0;
    let res3 = !needs_airllm_at(
        ram_mb,
        Falcon3Kind::Daily3B.params(),
        Falcon3Kind::Daily3B.file_mb_hint(),
    ) && ram_mb > 0;
    let pick = if res3 {
        Falcon3Kind::Daily3B
    } else if res7 {
        Falcon3Kind::Goal7B
    } else {
        Falcon3Kind::Tiny1B
    };
    let tier = if res3 {
        LlmTier::Daily
    } else if res7 {
        LlmTier::Goal
    } else if res10 {
        LlmTier::Large
    } else {
        LlmTier::Tight
    };
    LlmBootPlan {
        tier,
        pick,
        load_daily_3b: res3 || !res7,
        load_pro_7b_resident: res7,
        try_7b_airllm: air7,
        try_10b: res10 || needs_airllm_at(
            ram_mb,
            Falcon3Kind::Large10B.params(),
            Falcon3Kind::Large10B.file_mb_hint(),
        ),
        max_resident_mb,
    }
}

#[cfg(test)]
mod ram_policy_tests {
    use super::*;

    #[test]
    fn llm_plan_3b_lab_first_7b_optional_when_room() {
        let d = llm_boot_plan(2048);
        assert!(!d.load_pro_7b_resident);
        assert!(d.try_7b_airllm);
        let f = llm_boot_plan(32768);
        assert_eq!(f.pick, Falcon3Kind::Daily3B);
        assert!(f.load_pro_7b_resident);
        assert!(!f.try_7b_airllm);
        assert!(f.max_resident_mb > 2000);
    }

    #[test]
    fn heap_budget_scales_with_ram() {
        let b = k_nano::memory::heap_budget_mb(16384);
        assert!(b > 1536, "16GB deve orçar heap >> 1536MB, got {b}");
    }

    #[test]
    fn pack_ok_on_32g_two_models() {
        assert!(pack_resident_ok(32768, 2000, 989));
        assert!(!pack_resident_ok(2048, 1500, 1750));
    }

    #[test]
    fn falcon3_kind_shapes_1_58bit_family() {
        assert_eq!(Falcon3Kind::Tiny1B.hidden(), 2048);
        assert_eq!(Falcon3Kind::Tiny1B.intermediate(), 8192);
        assert_eq!(Falcon3Kind::Tiny1B.layers(), 18);
        assert_eq!(Falcon3Kind::Daily3B.hidden(), 3072);
        assert_eq!(Falcon3Kind::Daily3B.intermediate(), 9216);
        assert_eq!(Falcon3Kind::Daily3B.layers(), 22);
        assert_eq!(Falcon3Kind::Goal7B.intermediate(), 23040);
        assert_eq!(Falcon3Kind::Goal7B.layers(), 28);
        assert_eq!(Falcon3Kind::Large10B.layers(), 40);
        assert_eq!(Falcon3Kind::Large10B.intermediate(), 23040);
        assert_eq!(Falcon3Kind::Daily3B.ctx_len(), 4096);
        assert_eq!(Falcon3Kind::Tiny1B.ctx_len(), 8192);
        assert_eq!(Falcon3Kind::Goal7B.ctx_len(), 32768);
        assert!((Falcon3Kind::Daily3B.rope_theta() - 1_000_042.0).abs() < 1.0);
        assert_eq!(Falcon3Kind::from_str("1b"), Some(Falcon3Kind::Tiny1B));
        assert_eq!(Falcon3Kind::from_str("10b"), Some(Falcon3Kind::Large10B));
        assert_eq!(Falcon3Kind::from_str("lab"), Some(Falcon3Kind::Daily3B));
    }

    #[test]
    fn kind_from_fat_name() {
        assert_eq!(falcon3_kind_of_name("PRO.v6"), Some(Falcon3Kind::Goal7B));
        assert_eq!(falcon3_kind_of_name("FALCON3B.BIN"), Some(Falcon3Kind::Daily3B));
        assert_eq!(falcon3_kind_of_name("FALCON3.V6"), Some(Falcon3Kind::Daily3B));
        assert_eq!(falcon3_kind_of_name("FALCON3_1B.V6"), Some(Falcon3Kind::Tiny1B));
        assert_eq!(falcon3_kind_of_name("FALCON10.v6"), Some(Falcon3Kind::Large10B));
        assert_eq!(hub_slot_for_kind(Falcon3Kind::Daily3B), crate::model_hub::ModelSlot::Active);
        assert_eq!(falcon3_boot_names()[0], "FALCON3.V6");
    }
}
