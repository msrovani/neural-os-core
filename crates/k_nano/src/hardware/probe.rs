//! Hardware Probe — Unified hardware detection for ADR-0061 CPU-First BitNet
//! 
//! Detects hardware topology and classifies into 4 profiles:
//! - MultiDomainNuma: Dual Xeon, EPYC, Threadripper (SRAT required)
//! - AsymmetricCcd: Ryzen 9 X3D (CCD pinning)
//! - IntelHybrid: Core Ultra (P/E/LPE-core separation)
//! - StandardUma: i3/i5/Ryzen 5/7 monolithic (fast path)

use alloc::alloc::{alloc, Layout};
use crate::acpi::NumaTopologyMap;
use crate::hardware::topology::{ClientTopologyReport, CpuVendor};
use crate::hardware::xeon::XeonTopologyReport;
use crate::hardware::epyc::EpycTopologyReport;
use crate::platform_probe::{FeatureGate, IsaPath};
use crate::numa_alloc::{numa_node_for_apic, numa_allocate_frame, numa_topology, numa_allocate_local, numa_allocate_contiguous, numa_allocate_huge_2mb, numa_allocate_huge_1gb, numa_node_for_phys, numa_node_count, numa_node_ids, numa_stats, init_numa_allocators};
use crate::apic::lapic_id;

/// Hardware profile classification for CPU-First BitNet dispatch
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HardwareProfile {
    /// Multi-socket NUMA (Dual Xeon, EPYC, Threadripper)
    /// Requires SRAT parsing, per-node frame allocator, socket isolation
    MultiDomainNuma = 0,
    
    /// Asymmetric CCD (Ryzen 9 X3D: 7800X3D, 7950X3D, 9800X3D, 9950X3D)
    /// Pin AI to 3D V-Cache CCD, in-cache expert sizing
    AsymmetricCcd = 1,
    
    /// Intel Hybrid (Core Ultra, Lunar Lake, Arrow Lake)
    /// P-core for AI math, E-core for supervision, LPE-core for I/O
    IntelHybrid = 2,
    
    /// Standard UMA (i3/i5/Ryzen 5/7 monolithic)
    /// Fast path: global allocator O(1), MPMC queue
    StandardUma = 3,
}

impl HardwareProfile {
    pub fn name(self) -> &'static str {
        match self {
            HardwareProfile::MultiDomainNuma => "MultiDomainNuma",
            HardwareProfile::AsymmetricCcd => "AsymmetricCcd",
            HardwareProfile::IntelHybrid => "IntelHybrid",
            HardwareProfile::StandardUma => "StandardUma",
        }
    }
}

/// Hardware detection report
#[derive(Debug, Clone)]
pub struct HardwareReport {
    /// Classified hardware profile
    pub profile: HardwareProfile,
    
    /// Feature gate for ISA dispatch
    pub feature_gate: FeatureGate,
    
    /// ISA path for kernel dispatch
    pub isa_path: IsaPath,
    
    /// NUMA topology (if MultiDomainNuma)
    pub numa_topology: Option<NumaTopologyMap>,
    
    /// Client topology (if AsymmetricCcd/IntelHybrid/StandardUma)
    pub client_topology: Option<ClientTopologyReport>,
    
    /// Xeon topology (if MultiDomainNuma Intel)
    pub xeon_topology: Option<XeonTopologyReport>,
    
    /// EPYC topology (if MultiDomainNuma AMD)
    pub epyc_topology: Option<EpycTopologyReport>,
    
    /// Detected CPU vendor
    pub vendor: CpuVendor,
    
    /// Total physical cores
    pub total_cores: u16,
    
    /// Total logical threads
    pub total_threads: u16,
}

/// Detect and classify hardware profile
pub fn probe() -> HardwareReport {
    // 1. Run platform probe (HV + ISA + Cache + FeatureGate)
    crate::platform_probe::detect();
    let gate = crate::platform_probe::gate();
    let isa_path = crate::platform_probe::isa_path();
    
    // 2. Detect CPU vendor
    let vendor = crate::hardware::topology::detect_cpu_vendor();
    
    // 3. Get core/thread count
    let (total_cores, total_threads) = crate::hardware::topology::detect_core_thread_count();
    
    // 4. Try to parse SRAT for NUMA topology
    let phys_off = crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    let numa_topology = unsafe { crate::acpi::parse_srat_from_rsdp(phys_off) };
    
    // 5. Classify hardware profile
    let profile = classify_profile(&vendor, &numa_topology, total_cores);
    
    // 6. Gather detailed topology based on profile
    let (client_topo, xeon_topo, epyc_topo) = match profile {
        HardwareProfile::MultiDomainNuma => {
            if vendor == CpuVendor::Intel {
                let xeon = crate::hardware::xeon::discover_xeon_topology();
                (None, Some(xeon), None)
            } else if vendor == CpuVendor::AMD {
                let epyc = crate::hardware::epyc::discover_epyc_topology();
                (None, None, Some(epyc))
            } else {
                (None, None, None)
            }
        }
        HardwareProfile::AsymmetricCcd | HardwareProfile::IntelHybrid | HardwareProfile::StandardUma => {
            let client = crate::hardware::topology::discover_client_topology();
            (Some(client), None, None)
        }
    };
    
    HardwareReport {
        profile,
        feature_gate: gate,
        isa_path,
        numa_topology,
        client_topology: client_topo,
        xeon_topology: xeon_topo,
        epyc_topology: epyc_topo,
        vendor,
        total_cores,
        total_threads,
    }
}

/// Classify hardware into one of 4 profiles
fn classify_profile(vendor: &CpuVendor, numa: &Option<NumaTopologyMap>, total_cores: u16) -> HardwareProfile {
    // Multi-domain NUMA takes precedence
    if let Some(numa) = numa {
        if numa.is_multi_domain() {
            return HardwareProfile::MultiDomainNuma;
        }
    }
    
    // Client classification
    match vendor {
        CpuVendor::AMD => {
            // Check for 3D V-Cache (AsymmetricCcd)
            // We need client topology for this, but we can check cores
            if total_cores >= 8 {
                // Likely Ryzen 9 class - could be X3D
                // Full check requires client topology
                HardwareProfile::AsymmetricCcd
            } else {
                HardwareProfile::StandardUma
            }
        }
        CpuVendor::Intel => {
            // Check for hybrid architecture
            // Need client topology for full check
            if total_cores >= 6 {
                HardwareProfile::IntelHybrid
            } else {
                HardwareProfile::StandardUma
            }
        }
        _ => HardwareProfile::StandardUma,
    }
}

/// Refine profile after client topology is available
pub fn refine_profile(report: &mut HardwareReport) {
    if let Some(client) = &report.client_topology {
        report.profile = match client.generation {
            crate::hardware::topology::ClientGeneration::Amd3DVCache => HardwareProfile::AsymmetricCcd,
            crate::hardware::topology::ClientGeneration::IntelHybridModern 
                | crate::hardware::topology::ClientGeneration::IntelHybridLegacy => HardwareProfile::IntelHybrid,
            _ => HardwareProfile::StandardUma,
        };
    }
}

/// Get recommended SIMD width for current hardware
pub fn recommended_simd_width() -> u32 {
    let gate = crate::platform_probe::gate();
    if gate.allow_avx512 { 512 }
    else if gate.allow_avx2 { 256 }
    else if gate.allow_prefetch { 128 } // SSE4.2 proxy
    else { 64 }
}

/// Get recommended MoE expert size for current hardware (bytes)
pub fn recommended_expert_size() -> usize {
    let gate = crate::platform_probe::gate();
    if gate.allow_avx512 {
        256 * 1024 // 256KB for AVX-512 (fits in L2)
    } else if gate.allow_avx2 {
        128 * 1024 // 128KB for AVX2
    } else {
        64 * 1024 // 64KB fallback
    }
}

/// Get recommended core pinning strategy
pub fn recommended_core_pinning() -> CorePinningStrategy {
    let gate = crate::platform_probe::gate();
    if gate.allow_ep_core_detect {
        CorePinningStrategy::HybridAware
    } else if gate.allow_smp {
        CorePinningStrategy::NumaAware
    } else {
        CorePinningStrategy::SingleThread
    }
}

/// Core pinning strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorePinningStrategy {
    SingleThread,
    NumaAware,
    HybridAware,
}

/// Memory and thread strategy trait (ADR-0061 Phase 3)
pub trait MemoryAndThreadStrategy {
    /// Allocate memory local to the current NUMA node/CCD
    fn alloc_local(&self, size: usize, align: usize) -> Option<*mut u8>;
    
    /// Pin current thread to appropriate core class
    fn pin_thread(&self, role: CoreRole);
    
    /// Get core pool for a role
    fn pool_for(&self, role: CoreRole) -> &CorePool;
}

/// Core role for pinning
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreRole {
    Hermes,      // Supervisor, UI, VFS
    Cortex,      // BitNet inference
    Memory,      // External memory VFS, fact indexing
    Worker,      // Small logical verification cells
}

/// Core pool for a role
#[derive(Debug, Clone, Copy)]
pub struct CorePool {
    pub core_ids: &'static [u8],
    pub count: u8,
}

/// Standard UMA strategy (fast path)
pub struct StandardUmaStrategy;

impl MemoryAndThreadStrategy for StandardUmaStrategy {
    fn alloc_local(&self, size: usize, align: usize) -> Option<*mut u8> {
        // Global allocator O(1)
        let layout = Layout::from_size_align(size, align).ok()?;
        Some(unsafe { alloc(layout) })
    }
    
    fn pin_thread(&self, _role: CoreRole) {
        // No pinning needed on UMA
    }
    
    fn pool_for(&self, _role: CoreRole) -> &CorePool {
        static POOL: CorePool = CorePool { core_ids: &[], count: 0 };
        &POOL
    }
}

/// Multi-domain NUMA strategy (Dual Xeon, EPYC)
pub struct MultiDomainNumaStrategy;

impl MemoryAndThreadStrategy for MultiDomainNumaStrategy {
    fn alloc_local(&self, size: usize, align: usize) -> Option<*mut u8> {
        // Allocate from local NUMA node
        let node = numa_node_for_apic(lapic_id() as u32)?;
        numa_allocate_frame(node).map(|f| f.start_address().as_u64() as *mut u8)
    }
    
    fn pin_thread(&self, role: CoreRole) {
        // Pin to socket based on role
        if let Some(topology) = numa_topology() {
            let socket = match role {
                CoreRole::Hermes | CoreRole::Memory => 0,
                CoreRole::Cortex | CoreRole::Worker => 1,
            };
            // Implementation would set thread affinity
            let _ = socket;
        }
    }
    
    fn pool_for(&self, role: CoreRole) -> &CorePool {
        static POOL: CorePool = CorePool { core_ids: &[], count: 0 };
        &POOL
    }
}

/// Asymmetric CCD strategy (Ryzen X3D)
pub struct AsymmetricCcdStrategy;

impl MemoryAndThreadStrategy for AsymmetricCcdStrategy {
    fn alloc_local(&self, size: usize, align: usize) -> Option<*mut u8> {
        // Allocate from 3D V-Cache CCD if available
        let layout = Layout::from_size_align(size, align).ok()?;
        Some(unsafe { alloc(layout) })
    }
    
    fn pin_thread(&self, role: CoreRole) {
        // Pin AI to 3D V-Cache CCD
        let client = crate::hardware::topology::discover_client_topology();
        let vcache_ccd = client.amd_ccd.vcache_ccd_index;
        if vcache_ccd != 255 {
            // Pin to V-Cache CCD cores
        }
    }
    
    fn pool_for(&self, role: CoreRole) -> &CorePool {
        static POOL: CorePool = CorePool { core_ids: &[], count: 0 };
        &POOL
    }
}

/// Intel Hybrid strategy (Core Ultra)
pub struct IntelHybridStrategy;

impl MemoryAndThreadStrategy for IntelHybridStrategy {
    fn alloc_local(&self, size: usize, align: usize) -> Option<*mut u8> {
        let layout = Layout::from_size_align(size, align).ok()?;
        Some(unsafe { alloc(layout) })
    }
    
    fn pin_thread(&self, role: CoreRole) {
        // P-cores for Cortex, E-cores for Hermes/Worker
        let client = crate::hardware::topology::discover_client_topology();
        let _p_cores = client.intel_hybrid.p_cores;
        let _e_cores = client.intel_hybrid.e_cores;
        
        match role {
            CoreRole::Cortex => {
                // Pin to P-cores
            }
            CoreRole::Hermes | CoreRole::Worker => {
                // Pin to E-cores
            }
            CoreRole::Memory => {
                // Pin to P-cores for memory bandwidth
            }
        }
    }
    
    fn pool_for(&self, role: CoreRole) -> &CorePool {
        static POOL: CorePool = CorePool { core_ids: &[], count: 0 };
        &POOL
    }
}