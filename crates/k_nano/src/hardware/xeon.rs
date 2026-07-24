//! Intel Xeon Hardware Discovery Module
//! 
//! Low-level CPUID and ACPI (SRAT/SLIT) parsing for Intel Xeon processors
//! Extracts NUMA topology, cache hierarchy, and instruction flags during boot
//! 
//! # Architecture Detection
//! - Old Xeon (E5 v3/v4): AVX2, Octa-Channel DDR4, QPI bus
//! - Modern Xeon (Sapphire/Emerald/Granite Rapids): AVX-512, AMX, High Core Count

#![allow(dead_code)]
#![allow(unused_unsafe)]


/// NUMA Node information
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct NumaNode {
    /// Node ID (0-based)
    pub node_id: u8,
    /// Physical socket this node belongs to
    pub socket_id: u8,
    /// First logical CPU in this node
    pub cpu_start: u8,
    /// Number of logical CPUs in this node
    pub cpu_count: u8,
    /// Physical RAM start address (bytes)
    pub memory_start: u64,
    /// Physical RAM size (bytes)
    pub memory_size: u64,
    /// Distance to other nodes (SLIT table)
    pub distances: [u8; 8], // Max 8 nodes supported
}

/// Cache information per socket
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct CacheInfo {
    /// L1 data cache size (bytes)
    pub l1d: u32,
    /// L1 instruction cache size (bytes)
    pub l1i: u32,
    /// L2 cache size (bytes)
    pub l2: u32,
    /// L3 cache size (bytes)
    pub l3: u32,
    /// Cache line size (bytes)
    pub line_size: u32,
    /// Associativity
    pub associativity: u8,
}

/// Physical Socket information
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct SocketInfo {
    /// Socket ID (0-based)
    pub socket_id: u8,
    /// Number of physical cores
    pub physical_cores: u8,
    /// Number of logical threads (SMT)
    pub logical_threads: u8,
    /// First APIC ID for this socket
    pub apic_start: u32,
    /// Cache information
    pub cache: CacheInfo,
    /// NUMA node IDs for this socket
    pub numa_nodes: [u8; 4], // Max 4 NUMA nodes per socket
    /// Number of NUMA nodes
    pub numa_count: u8,
}

/// CPU Instruction Flags
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct CpuFlags {
    /// AVX2 support (256-bit SIMD)
    pub avx2: bool,
    /// AVX-512F support (512-bit SIMD foundation)
    pub avx512f: bool,
    /// AVX-512VNNI support (vector neural network instructions)
    pub avx512vnni: bool,
    /// AMX support (Advanced Matrix Extensions)
    pub amx: bool,
    /// BMI2 support (bit manipulation)
    pub bmi2: bool,
    /// FMA support (fused multiply-add)
    pub fma: bool,
    /// POPCNT support
    pub popcnt: bool,
    /// SSE4.2 support
    pub sse42: bool,
}

/// Complete Xeon Topology Report
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct XeonTopologyReport {
    /// Number of physical sockets
    pub socket_count: u8,
    /// Total physical cores across all sockets
    pub total_physical_cores: u16,
    /// Total logical threads across all sockets
    pub total_logical_threads: u16,
    /// Socket information (max 2 sockets for dual-socket)
    pub sockets: [SocketInfo; 2],
    /// CPU instruction flags
    pub flags: CpuFlags,
    /// Detected Xeon generation
    pub generation: XeonGeneration,
    /// Total system memory (bytes)
    pub total_memory: u64,
    /// Interconnect type (QPI vs UPI vs DMI)
    pub interconnect: InterconnectType,
}

/// Xeon Generation Classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum XeonGeneration {
    /// Unknown or unsupported
    Unknown = 0,
    /// Old Xeon (E5 v3/v4 - Haswell/Broadwell): AVX2, DDR4, QPI
    Old = 1,
    /// Modern Xeon (Skylake/Cascade Lake): AVX-512, DDR4, UPI
    Modern = 2,
    /// Latest Xeon (Sapphire/Emerald/Granite Rapids): AVX-512, AMX, DDR5, UPI
    Latest = 3,
}

/// Interconnect Type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum InterconnectType {
    Unknown = 0,
    /// Intel QuickPath Interconnect (old Xeon)
    QPI = 1,
    /// Intel Ultra Path Interconnect (modern Xeon)
    UPI = 2,
    /// Direct Media Interface (single socket)
    DMI = 3,
}

impl XeonGeneration {
    pub fn name(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Old => "Old (E5 v3/v4 Haswell/Broadwell)",
            Self::Modern => "Modern (Skylake/Cascade Lake)",
            Self::Latest => "Latest (Sapphire/Emerald/Granite Rapids)",
        }
    }
}

impl InterconnectType {
    pub fn name(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::QPI => "QPI",
            Self::UPI => "UPI",
            Self::DMI => "DMI",
        }
    }
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

impl XeonTopologyReport {
    /// Const-compatible default for use in statics.
    pub const fn const_default() -> Self {
        Self {
            socket_count: 1,
            total_physical_cores: 1,
            total_logical_threads: 1,
            sockets: [
                SocketInfo::const_default(),
                SocketInfo::const_default(),
            ],
            flags: CpuFlags::const_default(),
            generation: XeonGeneration::Unknown,
            total_memory: 0,
            interconnect: InterconnectType::Unknown,
        }
    }
}

impl SocketInfo {
    pub const fn const_default() -> Self {
        Self {
            socket_id: 0,
            physical_cores: 0,
            logical_threads: 0,
            apic_start: 0,
            cache: CacheInfo::const_default(),
            numa_nodes: [0; 4],
            numa_count: 0,
        }
    }
}

impl CacheInfo {
    pub const fn const_default() -> Self {
        Self {
            l1d: 0,
            l1i: 0,
            l2: 0,
            l3: 0,
            line_size: 0,
            associativity: 0,
        }
    }
}

impl CpuFlags {
    pub const fn const_default() -> Self {
        Self {
            avx2: false,
            avx512f: false,
            avx512vnni: false,
            amx: false,
            bmi2: false,
            fma: false,
            popcnt: false,
            sse42: false,
        }
    }
}

/// CPUID Leaf 1 (Processor Info and Feature Bits)
#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct CpuidLeaf1 {
    eax: u32,
    ebx: u32,
    ecx: u32,
    edx: u32,
}

/// CPUID Leaf 4 (Cache Parameters)
#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct CpuidLeaf4 {
    eax: u32,
    ebx: u32,
    ecx: u32,
    edx: u32,
}

/// CPUID Leaf 7 (Extended Features)
#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct CpuidLeaf7 {
    eax: u32,
    ebx: u32,
    ecx: u32,
    edx: u32,
}

/// CPUID Leaf 0xB (Extended Topology)
#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct CpuidLeafB {
    eax: u32,
    ebx: u32,
    ecx: u32,
    edx: u32,
}

/// CPUID Leaf 0x8000_0001 (Extended Function Info)
#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct CpuidLeaf80000001 {
    eax: u32,
    ebx: u32,
    ecx: u32,
    edx: u32,
}

/// Execute CPUID instruction
#[inline(always)]
#[cfg(target_arch = "x86_64")]
unsafe fn cpuid(leaf: u32) -> CpuidLeaf1 {
    let r = core::arch::x86_64::__cpuid(leaf);
    CpuidLeaf1 { eax: r.eax, ebx: r.ebx, ecx: r.ecx, edx: r.edx }
}

/// Execute CPUID with subleaf
#[inline(always)]
#[cfg(target_arch = "x86_64")]
unsafe fn cpuid_count(leaf: u32, subleaf: u32) -> CpuidLeaf4 {
    let r = core::arch::x86_64::__cpuid_count(leaf, subleaf);
    CpuidLeaf4 { eax: r.eax, ebx: r.ebx, ecx: r.ecx, edx: r.edx }
}

/// Detect CPU instruction flags via CPUID
pub fn detect_cpu_flags() -> CpuFlags {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let leaf1 = cpuid(1);
        let leaf7 = cpuid_count(7, 0);
        
        CpuFlags {
            sse42: (leaf1.ecx & (1 << 20)) != 0,
            popcnt: (leaf1.ecx & (1 << 23)) != 0,
            avx2: (leaf7.ebx & (1 << 5)) != 0,
            fma: (leaf1.ecx & (1 << 12)) != 0,
            bmi2: (leaf7.ebx & (1 << 8)) != 0,
            avx512f: (leaf7.ebx & (1 << 16)) != 0,
            avx512vnni: (leaf7.ecx & (1 << 11)) != 0,
            amx: (leaf7.edx & (1 << 24)) != 0,
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        CpuFlags::default()
    }
}

/// Detect cache topology via CPUID leaf 4
pub fn detect_cache_topology() -> CacheInfo {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let mut cache = CacheInfo::default();
        
        // Default cache line size from CPUID leaf 1
        let leaf1 = cpuid(1);
        cache.line_size = ((leaf1.ebx >> 8) & 0xFF) * 8;
        if cache.line_size == 0 {
            cache.line_size = 64;
        }
        
        // Query cache parameters via CPUID leaf 4
        for subleaf in 0..16u32 {
            let leaf4 = cpuid_count(4, subleaf);
            let cache_type = leaf4.eax & 0x1F;
            
            if cache_type == 0 {
                break; // No more caches
            }
            
            let level = ((leaf4.eax >> 5) & 0x7) as u8;
            let ways = ((leaf4.ebx >> 22) & 0x3FF) + 1;
            let line_size = (leaf4.ebx & 0xFFF) + 1;
            let partitions = ((leaf4.ebx >> 12) & 0x3FF) + 1;
            let sets = leaf4.ecx + 1;
            let size = ways * partitions * line_size * sets;
            
            cache.associativity = ways as u8;
            
            match level {
                1 if cache_type == 1 => cache.l1d = size, // Data cache
                1 if cache_type == 2 => cache.l1i = size, // Instruction cache
                1 if cache_type == 3 => cache.l1d = size, // Unified cache
                2 => cache.l2 = size,
                3 => cache.l3 = size,
                _ => {}
            }
        }
        
        // Apply defaults if detection failed
        if cache.l1d == 0 { cache.l1d = 32768; }
        if cache.l1i == 0 { cache.l1i = 32768; }
        if cache.l2 == 0 { cache.l2 = 262144; }
        if cache.l3 == 0 { cache.l3 = 4194304; }
        
        cache
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        CacheInfo::default()
    }
}

/// Detect topology (cores, threads, sockets) via CPUID leaf 0xB
pub fn detect_topology() -> (u8, u8, u8) {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let mut logical_threads = 1u8;
        let mut physical_cores = 1u8;
        let mut sockets = 1u8;
        
        // CPUID leaf 0xB provides extended topology enumeration
        for level in 0..3u32 {
            let leafb = cpuid_count(0xB, level);
            let level_type = ((leafb.ecx >> 8) & 0xFF) as u8;
            
            if level_type == 0 {
                break;
            }
            
            let processors = (leafb.ebx & 0xFFFF) as u8;
            
            match level_type {
                1 => logical_threads = processors, // SMT threads
                2 => physical_cores = processors, // Physical cores
                _ => {}
            }
        }
        
        // Estimate socket count from APIC ID space
        let leaf1 = cpuid(1);
        let _initial_apic_id = (leaf1.ebx >> 24) as u32;
        
        // For dual-socket systems, APIC IDs are typically offset by core count
        if physical_cores > 8 {
            sockets = 2;
        }
        
        (sockets, physical_cores, logical_threads)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        (1, 1, 1)
    }
}

/// Classify Xeon generation based on CPU features
pub fn classify_xeon_generation(flags: &CpuFlags) -> XeonGeneration {
    if flags.amx {
        // AMX indicates latest generation (Sapphire/Emerald/Granite Rapids)
        XeonGeneration::Latest
    } else if flags.avx512f {
        // AVX-512 indicates modern generation (Skylake/Cascade Lake)
        XeonGeneration::Modern
    } else if flags.avx2 {
        // AVX2 indicates old generation (Haswell/Broadwell E5 v3/v4)
        XeonGeneration::Old
    } else {
        XeonGeneration::Unknown
    }
}

/// Detect interconnect type based on generation
pub fn detect_interconnect(generation: XeonGeneration, socket_count: u8) -> InterconnectType {
    if socket_count == 1 {
        InterconnectType::DMI
    } else {
        match generation {
            XeonGeneration::Old => InterconnectType::QPI,
            XeonGeneration::Modern | XeonGeneration::Latest => InterconnectType::UPI,
            _ => InterconnectType::Unknown,
        }
    }
}

/// Parse ACPI SRAT (System Resource Affinity Table)
/// This is a simplified stub - real implementation would parse ACPI tables
pub fn parse_srat() -> [NumaNode; 8] {
    // In a real implementation, this would parse the ACPI SRAT table
    // to extract NUMA node topology and memory affinity
    let mut nodes = [NumaNode::default(); 8];
    
    // Default: single NUMA node
    nodes[0] = NumaNode {
        node_id: 0,
        socket_id: 0,
        cpu_start: 0,
        cpu_count: 1,
        memory_start: 0,
        memory_size: 0, // Would be filled from ACPI
        distances: [10, 20, 20, 20, 20, 20, 20, 20], // Local distance = 10
    };
    
    nodes
}

/// Parse ACPI SLIT (System Locality Information Table)
/// This is a simplified stub - real implementation would parse ACPI tables
pub fn parse_slit() -> [[u8; 8]; 8] {
    // In a real implementation, this would parse the ACPI SLIT table
    // to extract NUMA distance matrix
    let mut slit = [[20u8; 8]; 8];
    
    // Local distance is 10, remote is 20
    for i in 0..8 {
        slit[i][i] = 10;
    }
    
    slit
}

/// Main discovery function - builds complete Xeon topology report
pub fn discover_xeon_topology() -> XeonTopologyReport {
    let flags = detect_cpu_flags();
    let cache = detect_cache_topology();
    let (socket_count, physical_cores, logical_threads) = detect_topology();
    let generation = classify_xeon_generation(&flags);
    let interconnect = detect_interconnect(generation, socket_count);
    
    let total_physical_cores = (physical_cores as u16) * (socket_count as u16);
    let total_logical_threads = (logical_threads as u16) * (socket_count as u16);
    
    let mut sockets = [SocketInfo::default(), SocketInfo::default()];
    
    for i in 0..socket_count as usize {
        sockets[i] = SocketInfo {
            socket_id: i as u8,
            physical_cores,
            logical_threads,
            apic_start: (i as u32) * (physical_cores as u32),
            cache,
            numa_nodes: [i as u8, 0, 0, 0],
            numa_count: 1,
        };
    }
    
    // Parse ACPI tables for NUMA info
    let _numa_nodes = parse_srat();
    let _slit = parse_slit();
    
    XeonTopologyReport {
        socket_count,
        total_physical_cores,
        total_logical_threads,
        sockets,
        flags,
        generation,
        total_memory: 0, // Would be filled from memory map
        interconnect,
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

/// Calculate optimal MoE (Mixture of Experts) size to fit in L3 cache
/// Returns maximum expert size in bytes that fits in L3 per socket
pub fn optimal_moe_expert_size(report: &XeonTopologyReport, num_experts: usize) -> usize {
    let l3_size = l3_cache_per_socket(report) as usize;
    // Reserve 20% of L3 for other data
    let available = (l3_size * 80) / 100;
    // Divide among experts
    available / num_experts.max(1)
}

/// Check if system supports AVX-512
pub fn has_avx512(report: &XeonTopologyReport) -> bool {
    report.flags.avx512f
}

/// Check if system supports AMX
pub fn has_amx(report: &XeonTopologyReport) -> bool {
    report.flags.amx
}

/// Check if system is old Xeon (QPI, AVX2 only)
pub fn is_old_xeon(report: &XeonTopologyReport) -> bool {
    report.generation == XeonGeneration::Old
}

/// Check if system is modern Xeon (UPI, AVX-512)
pub fn is_modern_xeon(report: &XeonTopologyReport) -> bool {
    report.generation == XeonGeneration::Modern || report.generation == XeonGeneration::Latest
}

/// Get recommended SIMD width based on hardware
pub fn recommended_simd_width(report: &XeonTopologyReport) -> u32 {
    if report.flags.amx {
        512 // AMX uses 512-bit tiles
    } else if report.flags.avx512f {
        512 // AVX-512
    } else if report.flags.avx2 {
        256 // AVX2
    } else {
        128 // SSE/Scalar
    }
}

/// Log topology report for debugging
pub fn log_topology_report(report: &XeonTopologyReport) {
    #[cfg(target_arch = "x86_64")]
    {
        let _ = (report,); // Suppress unused warning in non-log builds
        
        // In a real implementation, this would use the logging system
        // For now, we'll just structure the data for the caller
    }
}
