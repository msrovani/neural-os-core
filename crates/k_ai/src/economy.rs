//! Economy and Budget Management (BEI Onda 1).
//! Gerencia orçamento de tokens, memória e ciclos de computação.
//!
//! CompressionTier controla nível de compressão (tradeoff memória vs qualidade).
//! BudgetManager rastreia gastos e evita OOM/estouro de CPU.

use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// Nível de compressão adaptativa.
/// Mais compressão = menos memória mas menor qualidade de inferência.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CompressionTier {
    /// Sem compressão (qualidade máxima)
    Lossless,
    /// Pouca compressão (qualidade alta, ~25% menos memória)
    Light,
    /// Compressão média (qualidade moderada, ~50% menos memória)
    Medium,
    /// Compressão máxima (qualidade baixa, ~75% menos memória)
    Aggressive,
}

impl CompressionTier {
    /// Fator de compressão relativo: 1.0 = sem compressão, 4.0 = 25% do original
    pub fn factor(&self) -> f32 {
        match self {
            CompressionTier::Lossless => 1.0,
            CompressionTier::Light => 1.33,
            CompressionTier::Medium => 2.0,
            CompressionTier::Aggressive => 4.0,
        }
    }

    /// Nível recomendado baseado na memória disponível (em MB).
    pub fn recommended(free_mb: u64) -> Self {
        if free_mb > 1024 {
            CompressionTier::Lossless
        } else if free_mb > 512 {
            CompressionTier::Light
        } else if free_mb > 256 {
            CompressionTier::Medium
        } else {
            CompressionTier::Aggressive
        }
    }
}

/// Gerencia orçamento de recursos do sistema.
///
/// Rastreia consumo de tokens, ciclos de inferência e memória,
/// ajustando o nível de compressão automaticamente conforme a carga.
pub struct BudgetManager {
    /// Orçamento máximo de tokens por sessão
    pub max_tokens_per_session: u64,
    /// Tokens usados na sessão atual
    pub tokens_used: AtomicU64,
    /// Orçamento máximo de memória heap em bytes
    pub max_heap_bytes: usize,
    /// Nível de compressão atual (CompressionTier como usize)
    pub compression: AtomicUsize,
    /// Ciclos de inferência no período
    pub inference_cycles: AtomicUsize,
    /// Limite de ciclos antes de acionar compressão
    pub cycle_limit: usize,
    // --- Memory tracking (legado) ---
    used_memory_bytes: AtomicUsize,
    temperature: f32,
}

impl BudgetManager {
    pub fn new(heap_bytes: usize) -> Self {
        Self {
            max_tokens_per_session: 4096,
            tokens_used: AtomicU64::new(0),
            max_heap_bytes: heap_bytes,
            compression: AtomicUsize::new(CompressionTier::Lossless as usize),
            inference_cycles: AtomicUsize::new(0),
            cycle_limit: 100,
            used_memory_bytes: AtomicUsize::new(0),
            temperature: 0.3,
        }
    }

    /// Registra N tokens consumidos. Retorna true se ainda dentro do orçamento.
    pub fn record_tokens(&self, n: u64) -> bool {
        let used = self.tokens_used.fetch_add(n, Ordering::Relaxed) + n;
        used <= self.max_tokens_per_session
    }

    /// Verifica se ainda há orçamento para N tokens.
    pub fn has_budget_for(&self, n: u64) -> bool {
        self.tokens_used.load(Ordering::Relaxed) + n <= self.max_tokens_per_session
    }

    /// Obtém nível de compressão atual.
    pub fn compression_tier(&self) -> CompressionTier {
        match self.compression.load(Ordering::Relaxed) {
            0 => CompressionTier::Lossless,
            1 => CompressionTier::Light,
            2 => CompressionTier::Medium,
            _ => CompressionTier::Aggressive,
        }
    }

    /// Ajusta compressão adaptativa baseada no uso de ciclos.
    pub fn adapt_compression(&self) {
        let cycles = self.inference_cycles.load(Ordering::Relaxed);
        if cycles > self.cycle_limit * 3 {
            // Muita inferência — comprime
            let current = self.compression.load(Ordering::Relaxed);
            if current < CompressionTier::Aggressive as usize {
                self.compression.store(current + 1, Ordering::Release);
            }
        } else if cycles < self.cycle_limit {
            // Pouca inferência — pode descomprimir (melhor qualidade)
            let current = self.compression.load(Ordering::Relaxed);
            if current > CompressionTier::Lossless as usize {
                self.compression.store(current - 1, Ordering::Release);
            }
        }
        // Reset contador de ciclos
        self.inference_cycles.store(0, Ordering::Release);
    }

    /// Incrementa contador de ciclos de inferência.
    pub fn record_inference_cycle(&self) {
        self.inference_cycles.fetch_add(1, Ordering::Relaxed);
    }

    /// Reseta orçamento de tokens (nova sessão).
    pub fn reset_session(&self) {
        self.tokens_used.store(0, Ordering::Release);
    }

    // ── Métodos de memória (legado) ──

    /// Pressão de memória atual (0.0 - 1.0).
    pub fn pressure(&self) -> f32 {
        let used = self.used_memory_bytes.load(Ordering::Relaxed) as f32;
        let max = self.max_heap_bytes as f32;
        if max == 0.0 { 1.0 } else { (used / max).min(1.0) }
    }

    /// Tenta alocar `bytes` no orçamento. Retorna false se estourar.
    pub fn allocate(&self, bytes: usize) -> bool {
        let used = self.used_memory_bytes.load(Ordering::Relaxed);
        if used + bytes > self.max_heap_bytes {
            return false;
        }
        self.used_memory_bytes.store(used + bytes, Ordering::Relaxed);
        true
    }

    /// Libera `bytes` do orçamento (com saturating sub).
    pub fn deallocate(&self, bytes: usize) {
        self.used_memory_bytes
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |u| {
                Some(u.saturating_sub(bytes))
            })
            .ok();
    }

    /// Bytes alocados no momento.
    pub fn used_bytes(&self) -> usize {
        self.used_memory_bytes.load(Ordering::Relaxed)
    }

    /// Orçamento máximo de memória em bytes.
    pub fn max_bytes(&self) -> usize {
        self.max_heap_bytes
    }

    /// Define temperatura do sistema (0.0 - 1.0).
    pub fn set_temperature(&mut self, t: f32) {
        self.temperature = t.clamp(0.0, 1.0);
    }

    /// Temperatura atual do sistema.
    pub fn temperature(&self) -> f32 {
        self.temperature
    }
}
