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

/// KV heurístico alinhado ao MemoryAgent legado (params/40 → MB clamp).
pub fn estimate_kv_mb(params: u64) -> u64 {
    let mb = params / 40 / (1024 * 1024);
    mb.clamp(8, 4096)
}

pub fn estimate_heap_mb(params: u64) -> u64 {
    let model_gb = params as f64 / 1_000_000_000.0;
    let heap = (model_gb * 100.0) as u64;
    heap.clamp(128, 2048)
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

/// Footprints estáticos por nome de slot / token (MB on-disk + runtime order).
pub fn slot_footprint_mb(slot_name: &str) -> Option<u64> {
    match slot_name {
        "generator_fast" | "fast" | "850m" | "850" | "active" | "current" | "generator" => {
            Some(220)
        }
        "generator_pro" | "pro" | "3b" | "bitnet3b" => Some(700),
        "tinystories" | "tiny" | "smoke" => Some(4),
        "rust_coder" | "rustcoder" => Some(260),
        "hw_identify" | "hwexpert" => Some(1),
        "learner" | "qwen05" | "qwen0.5b" => Some(125),
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
