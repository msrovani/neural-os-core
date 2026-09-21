//! ORPHAN — not in hermes/src/lib.rs (SESSION_379). Do not treat as wired.
//! Cognitive Adaptation Engine (LEGACY restore) — policy generation only;
//! `set_adaptation_policy` / `get_adaptation_policy` are honesty stubs until
//! a global policy store exists.
//!
//! Hermes (Meta-Cognitive Supervisor) receives hardware topology reports from k-nano
//! and automatically decides execution policies for the operating system.
//! 
//! # Adaptation Cycle
//! 1. Receive XeonTopologyReport from k-nano during boot
//! 2. Classify hardware generation (Old vs Modern Xeon)
//! 3. Generate ExecutionStrategy based on hardware capabilities
//! 4. Apply policies: socket isolation, core pinning, SIMD dispatch, MoE sizing
//!
//! Restored from LEGACY/v1.9.9-test/hermes/adaptation/adaptation.rs
//! Xeon topology types inlined (hardware/ module is in LEGACY).

#![allow(dead_code)]
#![allow(unused_unsafe)]

use core::sync::atomic::{AtomicBool, Ordering};

// ─── Xeon topology types (inlined from LEGACY k_nano/hardware/xeon.rs) ───

/// Xeon Generation Classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum XeonGeneration {
    Unknown = 0,
    Old = 1,
    Modern = 2,
    Latest = 3,
}

/// Interconnect Type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum InterconnectType {
    Unknown = 0,
    QPI = 1,
    UPI = 2,
    DMI = 3,
}

/// CPU Instruction Flags
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct CpuFlags {
    pub avx2: bool,
    pub avx512f: bool,
    pub avx512vnni: bool,
    pub amx: bool,
    pub bmi2: bool,
    pub fma: bool,
    pub popcnt: bool,
    pub sse42: bool,
}

/// Cache information per socket
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct CacheInfo {
    pub l1d: u32,
    pub l1i: u32,
    pub l2: u32,
    pub l3: u32,
    pub line_size: u32,
    pub associativity: u8,
}

/// Physical Socket information
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct SocketInfo {
    pub socket_id: u8,
    pub physical_cores: u8,
    pub logical_threads: u8,
    pub apic_start: u32,
    pub cache: CacheInfo,
    pub numa_nodes: [u8; 4],
    pub numa_count: u8,
}

/// Complete Xeon Topology Report
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct XeonTopologyReport {
    pub socket_count: u8,
    pub total_physical_cores: u16,
    pub total_logical_threads: u16,
    pub sockets: [SocketInfo; 2],
    pub flags: CpuFlags,
    pub generation: XeonGeneration,
    pub total_memory: u64,
    pub interconnect: InterconnectType,
}

impl Default for XeonTopologyReport {
    fn default() -> Self {
        Self {
            socket_count: 1,
            total_physical_cores: 1,
            total_logical_threads: 1,
            sockets: [SocketInfo::default(), SocketInfo::default()],
            flags: CpuFlags::default(),
            generation: XeonGeneration::Unknown,
            total_memory: 0,
            interconnect: InterconnectType::Unknown,
        }
    }
}

/// Get L3 cache size per socket (bytes)
pub fn l3_cache_per_socket(report: &XeonTopologyReport) -> u32 {
    if report.socket_count > 0 {
        report.sockets[0].cache.l3
    } else {
        0
    }
}

/// Calculate optimal MoE size to fit in L3 cache
pub fn optimal_moe_expert_size(report: &XeonTopologyReport, num_experts: usize) -> usize {
    let l3_size = l3_cache_per_socket(report) as usize;
    let available = (l3_size * 80) / 100;
    available / num_experts.max(1)
}

pub fn has_avx512(report: &XeonTopologyReport) -> bool { report.flags.avx512f }
pub fn has_amx(report: &XeonTopologyReport) -> bool { report.flags.amx }
pub fn is_old_xeon(report: &XeonTopologyReport) -> bool { report.generation == XeonGeneration::Old }
pub fn is_modern_xeon(report: &XeonTopologyReport) -> bool {
    report.generation == XeonGeneration::Modern || report.generation == XeonGeneration::Latest
}

pub fn recommended_simd_width(report: &XeonTopologyReport) -> u32 {
    if report.flags.amx { 512 }
    else if report.flags.avx512f { 512 }
    else if report.flags.avx2 { 256 }
    else { 128 }
}

// ─── Core adaptation logic ───

/// Execution Strategy determined by Hermes based on hardware
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExecutionStrategy {
    Unknown = 0,
    OldXeon = 1,
    ModernXeon = 2,
    Fallback = 3,
}

impl ExecutionStrategy {
    pub fn name(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::OldXeon => "Old Xeon (E5 v3/v4)",
            Self::ModernXeon => "Modern Xeon (AVX-512/AMX)",
            Self::Fallback => "Fallback",
        }
    }
}

/// Socket isolation policy for NUMA systems
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct SocketIsolationPolicy {
    pub hermes_socket: u8,
    pub cortex_socket: u8,
    pub strict_isolation: bool,
    pub max_inter_socket_rate: u32,
}

/// Core pinning policy for thread-to-core assignment
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct CorePinningPolicy {
    pub enable_pinning: bool,
    pub spin_loop_critical: bool,
    pub cognitive_cell_count: u16,
    pub hermes_cores: [u8; 8],
    pub cortex_cores: [u8; 64],
}

impl Default for CorePinningPolicy {
    fn default() -> Self {
        Self {
            enable_pinning: false,
            spin_loop_critical: false,
            cognitive_cell_count: 0,
            hermes_cores: [0; 8],
            cortex_cores: [0; 64],
        }
    }
}

/// SIMD dispatch policy
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct SimdDispatchPolicy {
    pub simd_width: u32,
    pub use_avx512: bool,
    pub use_amx: bool,
    pub use_avx2: bool,
    pub weights_per_cycle: u32,
}

/// MoE sizing policy
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MoESizingPolicy {
    pub max_expert_size_bytes: usize,
    pub experts_per_socket: u8,
    pub force_in_cache: bool,
    pub l3_utilization_percent: u8,
}

/// Complete adaptation policy generated by Hermes
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct AdaptationPolicy {
    pub strategy: ExecutionStrategy,
    pub socket_isolation: SocketIsolationPolicy,
    pub core_pinning: CorePinningPolicy,
    pub simd_dispatch: SimdDispatchPolicy,
    pub moe_sizing: MoESizingPolicy,
}

impl Default for AdaptationPolicy {
    fn default() -> Self {
        Self {
            strategy: ExecutionStrategy::Unknown,
            socket_isolation: SocketIsolationPolicy::default(),
            core_pinning: CorePinningPolicy::default(),
            simd_dispatch: SimdDispatchPolicy::default(),
            moe_sizing: MoESizingPolicy::default(),
        }
    }
}

/// Global adaptation policy (set once during boot)
static ADAPTATION_POLICY: core::sync::atomic::AtomicPtr<AdaptationPolicy> =
    core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());

/// Flag indicating if adaptation has been performed
static ADAPTED: AtomicBool = AtomicBool::new(false);

/// Generate execution strategy based on Xeon topology report
pub fn generate_strategy(report: &XeonTopologyReport) -> ExecutionStrategy {
    match report.generation {
        XeonGeneration::Old => ExecutionStrategy::OldXeon,
        XeonGeneration::Modern | XeonGeneration::Latest => ExecutionStrategy::ModernXeon,
        XeonGeneration::Unknown => {
            if report.flags.avx2 {
                ExecutionStrategy::OldXeon
            } else if report.flags.sse42 {
                ExecutionStrategy::Fallback
            } else {
                ExecutionStrategy::Fallback
            }
        }
    }
}

fn generate_old_xeon_socket_isolation(report: &XeonTopologyReport) -> SocketIsolationPolicy {
    if report.socket_count >= 2 {
        SocketIsolationPolicy {
            hermes_socket: 0,
            cortex_socket: 1,
            strict_isolation: true,
            max_inter_socket_rate: 1000,
        }
    } else {
        SocketIsolationPolicy {
            hermes_socket: 0,
            cortex_socket: 0,
            strict_isolation: false,
            max_inter_socket_rate: u32::MAX,
        }
    }
}

fn generate_modern_xeon_socket_isolation(report: &XeonTopologyReport) -> SocketIsolationPolicy {
    if report.socket_count >= 2 {
        SocketIsolationPolicy {
            hermes_socket: 0,
            cortex_socket: 1,
            strict_isolation: false,
            max_inter_socket_rate: 10000,
        }
    } else {
        SocketIsolationPolicy {
            hermes_socket: 0,
            cortex_socket: 0,
            strict_isolation: false,
            max_inter_socket_rate: u32::MAX,
        }
    }
}

fn generate_old_xeon_core_pinning(report: &XeonTopologyReport) -> CorePinningPolicy {
    let total_cores = report.total_physical_cores;
    let hermes_cores_count = ((total_cores as u16) / 4).min(8) as u16;
    let cortex_cores_count = total_cores as u16 - hermes_cores_count;

    let mut hermes_cores = [0u8; 8];
    let mut cortex_cores = [0u8; 64];

    for i in 0..hermes_cores_count as usize {
        if i < 8 { hermes_cores[i] = i as u8; }
    }
    for i in 0..cortex_cores_count as usize {
        if i < 64 { cortex_cores[i] = (hermes_cores_count as u8 + i as u8).min(255); }
    }

    CorePinningPolicy {
        enable_pinning: true,
        spin_loop_critical: true,
        cognitive_cell_count: cortex_cores_count,
        hermes_cores,
        cortex_cores,
    }
}

fn generate_modern_xeon_core_pinning(report: &XeonTopologyReport) -> CorePinningPolicy {
    let total_cores = report.total_physical_cores;
    let hermes_cores_count = ((total_cores as u16) / 8).min(8) as u16;
    let cortex_cores_count = total_cores as u16 - hermes_cores_count;

    let mut hermes_cores = [0u8; 8];
    let mut cortex_cores = [0u8; 64];

    for i in 0..hermes_cores_count as usize {
        if i < 8 { hermes_cores[i] = i as u8; }
    }
    for i in 0..cortex_cores_count as usize {
        if i < 64 { cortex_cores[i] = (hermes_cores_count as u8 + i as u8).min(255); }
    }

    CorePinningPolicy {
        enable_pinning: true,
        spin_loop_critical: false,
        cognitive_cell_count: cortex_cores_count,
        hermes_cores,
        cortex_cores,
    }
}

fn generate_simd_dispatch(report: &XeonTopologyReport) -> SimdDispatchPolicy {
    let simd_width = recommended_simd_width(report);
    let weights_per_cycle = if simd_width >= 512 { 256 } else if simd_width >= 256 { 128 } else { 64 };

    SimdDispatchPolicy {
        simd_width,
        use_avx512: report.flags.avx512f,
        use_amx: report.flags.amx,
        use_avx2: report.flags.avx2,
        weights_per_cycle,
    }
}

fn generate_old_xeon_moe_sizing(report: &XeonTopologyReport) -> MoESizingPolicy {
    let l3_size = l3_cache_per_socket(report) as usize;
    let max_expert_size = (l3_size * 80 / 100) / 4;

    MoESizingPolicy {
        max_expert_size_bytes: max_expert_size,
        experts_per_socket: 4,
        force_in_cache: true,
        l3_utilization_percent: 80,
    }
}

fn generate_modern_xeon_moe_sizing(report: &XeonTopologyReport) -> MoESizingPolicy {
    let l3_size = l3_cache_per_socket(report) as usize;
    let experts = (report.total_physical_cores as u8 / 4).max(4).min(16);
    let max_expert_size = (l3_size * 75 / 100) / (experts as usize);

    MoESizingPolicy {
        max_expert_size_bytes: max_expert_size,
        experts_per_socket: experts,
        force_in_cache: true,
        l3_utilization_percent: 75,
    }
}

fn generate_fallback_policy() -> AdaptationPolicy {
    AdaptationPolicy {
        strategy: ExecutionStrategy::Fallback,
        socket_isolation: SocketIsolationPolicy {
            hermes_socket: 0,
            cortex_socket: 0,
            strict_isolation: false,
            max_inter_socket_rate: u32::MAX,
        },
        core_pinning: CorePinningPolicy {
            enable_pinning: false,
            spin_loop_critical: false,
            cognitive_cell_count: 1,
            hermes_cores: [0; 8],
            cortex_cores: [0; 64],
        },
        simd_dispatch: SimdDispatchPolicy {
            simd_width: 128,
            use_avx512: false,
            use_amx: false,
            use_avx2: false,
            weights_per_cycle: 64,
        },
        moe_sizing: MoESizingPolicy {
            max_expert_size_bytes: 1024 * 1024, // 1MB fallback
            experts_per_socket: 1,
            force_in_cache: false,
            l3_utilization_percent: 50,
        },
    }
}

/// Main adaptation function — generates complete policy from topology report
pub fn adapt_to_hardware(report: &XeonTopologyReport) -> AdaptationPolicy {
    let strategy = generate_strategy(report);

    match strategy {
        ExecutionStrategy::OldXeon => AdaptationPolicy {
            strategy,
            socket_isolation: generate_old_xeon_socket_isolation(report),
            core_pinning: generate_old_xeon_core_pinning(report),
            simd_dispatch: generate_simd_dispatch(report),
            moe_sizing: generate_old_xeon_moe_sizing(report),
        },
        ExecutionStrategy::ModernXeon => AdaptationPolicy {
            strategy,
            socket_isolation: generate_modern_xeon_socket_isolation(report),
            core_pinning: generate_modern_xeon_core_pinning(report),
            simd_dispatch: generate_simd_dispatch(report),
            moe_sizing: generate_modern_xeon_moe_sizing(report),
        },
        _ => generate_fallback_policy(),
    }
}

/// Set the global adaptation policy (call once during boot).
/// Honesty: discards policy until a real store exists — does **not** claim adapted.
pub fn set_adaptation_policy(policy: AdaptationPolicy) {
    let _ = policy;
    // Do not set ADAPTED=true without storing the policy (SESSION_379).
    k_nano::slog_hermes!(
        "ADAPT",
        "warn",
        "set_adaptation_policy ignored (orphan/unwired store)"
    );
}

/// Get the current adaptation policy — always None until store wired.
pub fn get_adaptation_policy() -> Option<AdaptationPolicy> {
    None
}

/// Check if adaptation has been performed
pub fn is_adapted() -> bool {
    false
}

/// Get the current execution strategy
pub fn get_strategy() -> ExecutionStrategy {
    if let Some(policy) = get_adaptation_policy() {
        policy.strategy
    } else {
        ExecutionStrategy::Unknown
    }
}

/// Log the adaptation policy for debugging
pub fn log_adaptation_policy(policy: &AdaptationPolicy) {
    let _ = (policy,);
}

/// Cognitive adaptation entry point — called by Hermes during boot
///
/// # Arguments
/// * `report` — Xeon topology report from k-nano
///
/// # Returns
/// The generated adaptation policy
pub fn cognitive_adaptation(report: &XeonTopologyReport) -> AdaptationPolicy {
    let policy = adapt_to_hardware(report);
    set_adaptation_policy(policy);
    log_adaptation_policy(&policy);
    policy
}
