//! ADR-0061: Core pinning por tipo de núcleo.
//!
//! Detecta classes de núcleo (P-core/E-core/LPE-core/CCD-3D-VCache/NUMA-node)
//! e fixa threads em cores específicos baseado na função:
//! - Hermes (Supervisor) → E-cores ou CCD sem 3D V-Cache
//! - Cortex (BitNet matmul) → P-cores ou CCD com 3D V-Cache
//! - Jarbas (Display) → LPE-cores ou qualquer core disponível
//! - I/O → cores reservados
//!
//! # Detecção
//! - Intel Hybrid: CPUID leaf 0x1A (EAX bits 31:24 = Core Type)
//! - AMD CCD: CPUID 0x8000001E (EBX bits 7:0 = Core ID)
//! - NUMA: SRAT (já parseado em `acpi::NumaTopologyMap`)

#![allow(dead_code)]

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// Classe de núcleo detectada via CPUID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CoreClass {
    /// Tipo desconhecido (fallback)
    Unknown = 0,
    /// Intel P-core (Performance)
    IntelPCore = 1,
    /// Intel E-core (Efficiency)
    IntelECore = 2,
    /// Intel LPE-core (Low Power Efficiency)
    IntelLPECore = 3,
    /// AMD CCD com 3D V-Cache (Ryzen X3D)
    AmdCcdVCache = 4,
    /// AMD CCD sem 3D V-Cache (frequência mais alta)
    AmdCcdStandard = 5,
    /// AMD CCD Dense (Zen 4c/5c — Bergamo/Turin-Dense)
    AmdCcdDense = 6,
}

/// Função que uma thread exerce no sistema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CoreRole {
    /// Hermes (Supervisor Meta-Cognitivo)
    Hermes = 0,
    /// Cortex (BitNet inference)
    Cortex = 1,
    /// Jarbas (Display/Front-End)
    Jarbas = 2,
    /// I/O (drivers, network, storage)
    Io = 3,
    /// Worker genérico (Cognitive Cell)
    Worker = 4,
}

/// Pool de cores disponíveis para uma função.
#[derive(Debug, Clone, Copy)]
pub struct CorePool {
    pub role: CoreRole,
    pub cores: [u32; 32],
    pub count: u8,
}

impl CorePool {
    pub const fn empty(role: CoreRole) -> Self {
        Self {
            role,
            cores: [0; 32],
            count: 0,
        }
    }

    pub fn add(&mut self, apic_id: u32) {
        if (self.count as usize) < self.cores.len() {
            self.cores[self.count as usize] = apic_id;
            self.count += 1;
        }
    }

    pub fn first(&self) -> Option<u32> {
        if self.count > 0 {
            Some(self.cores[0])
        } else {
            None
        }
    }
}

/// Detecta a classe do núcleo atual via CPUID.
///
/// Intel Hybrid (CPUID leaf 0x1A):
/// - EAX bits 31:24 = Core Type (0x40 = P-core, 0x20 = E-core)
/// - EAX bits 7:0 = Native model ID
///
/// AMD: CPUID 0x8000001E EBX bits 7:0 = Core ID (não distingue CCD)
#[cfg(target_arch = "x86_64")]
pub fn detect_current_core_class() -> CoreClass {
    unsafe {
        // Tenta Intel Hybrid primeiro
        let leaf = core::arch::x86_64::__cpuid(0);
        if leaf.eax >= 0x1A {
            let hybrid = core::arch::x86_64::__cpuid(0x1A);
            let core_type = (hybrid.eax >> 24) & 0xFF;
            match core_type {
                0x40 => return CoreClass::IntelPCore,
                0x20 => return CoreClass::IntelECore,
                _ => {}
            }
        }

        // AMD: tenta detectar via CPUID 0x8000001E
        let max_ext = core::arch::x86_64::__cpuid(0x80000000).eax;
        if max_ext >= 0x8000_001E {
            let amd = core::arch::x86_64::__cpuid(0x8000_001E);
            let core_id = amd.ebx & 0xFF;
            // Heurística: Core ID 0 = CCD 0 (geralmente 3D V-Cache em X3D)
            // Core ID 1 = CCD 1 (frequência mais alta)
            // Esta é uma simplificação — detecção real requer MSR
            if core_id == 0 {
                return CoreClass::AmdCcdVCache;
            } else {
                return CoreClass::AmdCcdStandard;
            }
        }

        CoreClass::Unknown
    }
}

#[cfg(not(target_arch = "x86_64"))]
pub fn detect_current_core_class() -> CoreClass {
    CoreClass::Unknown
}

/// Lê o APIC ID do núcleo atual.
#[cfg(target_arch = "x86_64")]
pub fn current_apic_id() -> u32 {
    unsafe {
        // CPUID leaf 0x0B, subleaf 0: EDX = x2APIC ID
        let result = core::arch::x86_64::__cpuid_count(0x0B, 0);
        result.edx
    }
}

#[cfg(not(target_arch = "x86_64"))]
pub fn current_apic_id() -> u32 {
    0
}

/// Estratégia de pinning por arquitetura.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PinningStrategy {
    /// Sem pinning (UMA monolítico)
    None = 0,
    /// Intel Hybrid: P-core → Cortex, E-core → Hermes
    IntelHybrid = 1,
    /// AMD X3D: CCD 3D V-Cache → Cortex, CCD standard → Hermes
    AmdX3D = 2,
    /// NUMA multi-socket: Socket 0 → Hermes, Socket 1 → Cortex
    NumaSocket = 3,
    /// NUMA EPYC NPS4: 4 nós por socket
    NumaEpycNps4 = 4,
}

/// Detecta a estratégia de pinning baseada na topologia.
pub fn detect_pinning_strategy() -> PinningStrategy {
    let class = detect_current_core_class();
    match class {
        CoreClass::IntelPCore | CoreClass::IntelECore | CoreClass::IntelLPECore => {
            PinningStrategy::IntelHybrid
        }
        CoreClass::AmdCcdVCache | CoreClass::AmdCcdStandard => PinningStrategy::AmdX3D,
        _ => {
            // Verifica se NUMA está ativo
            if crate::numa_alloc::initialized_node_count() > 1 {
                PinningStrategy::NumaSocket
            } else {
                PinningStrategy::None
            }
        }
    }
}

/// Pools de cores por função (inicializado em boot).
static mut HERMES_POOL: CorePool = CorePool::empty(CoreRole::Hermes);
static mut CORTEX_POOL: CorePool = CorePool::empty(CoreRole::Cortex);
static mut JARBAS_POOL: CorePool = CorePool::empty(CoreRole::Jarbas);
static mut IO_POOL: CorePool = CorePool::empty(CoreRole::Io);

/// Flag indicando se os pools foram inicializados.
static POOLS_INITIALIZED: AtomicU32 = AtomicU32::new(0);

/// Estratégia de pinning ativa.
static ACTIVE_STRATEGY: AtomicU64 = AtomicU64::new(0);

/// Inicializa os pools de cores baseado na estratégia detectada.
///
/// # Argumentos
/// - `total_cores`: número total de cores lógicos detectados
pub fn init_pools(total_cores: u32) {
    let strategy = detect_pinning_strategy();
    ACTIVE_STRATEGY.store(strategy as u64 as u64, Ordering::Release);

    unsafe {
        match strategy {
            PinningStrategy::IntelHybrid => {
                // P-cores (primeiros) → Cortex, E-cores (últimos) → Hermes
                // Heurística: metade dos cores são P-cores
                let p_count = total_cores / 2;
                for i in 0..p_count {
                    CORTEX_POOL.add(i);
                }
                for i in p_count..total_cores {
                    HERMES_POOL.add(i);
                }
                JARBAS_POOL.add(total_cores.saturating_sub(1));
            }
            PinningStrategy::AmdX3D => {
                // CCD 0 (3D V-Cache) → Cortex, CCD 1 → Hermes
                // Heurística: primeiros 8 cores = CCD 0
                let ccd0_count = (total_cores / 2).min(8);
                for i in 0..ccd0_count {
                    CORTEX_POOL.add(i);
                }
                for i in ccd0_count..total_cores {
                    HERMES_POOL.add(i);
                }
                JARBAS_POOL.add(total_cores.saturating_sub(1));
            }
            PinningStrategy::NumaSocket | PinningStrategy::NumaEpycNps4 => {
                // Socket 0 → Hermes, Socket 1 → Cortex
                let half = total_cores / 2;
                for i in 0..half {
                    HERMES_POOL.add(i);
                }
                for i in half..total_cores {
                    CORTEX_POOL.add(i);
                }
                JARBAS_POOL.add(total_cores.saturating_sub(1));
            }
            PinningStrategy::None => {
                // Sem pinning — todos os cores disponíveis para todos
                for i in 0..total_cores {
                    HERMES_POOL.add(i);
                    CORTEX_POOL.add(i);
                    JARBAS_POOL.add(i);
                    IO_POOL.add(i);
                }
            }
        }
    }

    POOLS_INITIALIZED.store(1, Ordering::Release);

    crate::slog_nano!(
        "PIN",
        "info",
        "Core pinning inicializado: strategy={:?} hermes={} cortex={} jarbas={}",
        strategy,
        unsafe { HERMES_POOL.count },
        unsafe { CORTEX_POOL.count },
        unsafe { JARBAS_POOL.count }
    );
}

/// Retorna o pool de cores para uma função.
pub fn pool_for(role: CoreRole) -> Option<CorePool> {
    if POOLS_INITIALIZED.load(Ordering::Acquire) == 0 {
        return None;
    }
    unsafe {
        match role {
            CoreRole::Hermes => Some(HERMES_POOL),
            CoreRole::Cortex => Some(CORTEX_POOL),
            CoreRole::Jarbas => Some(JARBAS_POOL),
            CoreRole::Io => Some(IO_POOL),
            CoreRole::Worker => Some(CORTEX_POOL),
        }
    }
}

/// Retorna a estratégia de pinning ativa.
pub fn active_strategy() -> PinningStrategy {
    match ACTIVE_STRATEGY.load(Ordering::Acquire) {
        1 => PinningStrategy::IntelHybrid,
        2 => PinningStrategy::AmdX3D,
        3 => PinningStrategy::NumaSocket,
        4 => PinningStrategy::NumaEpycNps4,
        _ => PinningStrategy::None,
    }
}

/// Log de diagnóstico do estado de pinning.
pub fn log_pinning_state() {
    let strategy = active_strategy();
    crate::slog_nano!("PIN", "info", "Pinning strategy: {:?}", strategy);
    if let Some(p) = pool_for(CoreRole::Hermes) {
        crate::slog_nano!("PIN", "info", "Hermes pool: {} cores", p.count);
    }
    if let Some(p) = pool_for(CoreRole::Cortex) {
        crate::slog_nano!("PIN", "info", "Cortex pool: {} cores", p.count);
    }
    if let Some(p) = pool_for(CoreRole::Jarbas) {
        crate::slog_nano!("PIN", "info", "Jarbas pool: {} cores", p.count);
    }
}
