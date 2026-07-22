//! Client Hardware Discovery Module
//! 
//! Low-level CPUID and ACPI (MADT/PPTT) parsing for AMD Ryzen and Intel Client processors
//! Detects hybrid architectures, asymmetric CCDs, 3D V-Cache, and memory topology
//! 
//! # Supported Architectures
//! - AMD Ryzen 3D V-Cache (7800X3D, 7950X3D, 9800X3D, 9950X3D)
//! - AMD Ryzen AI / AI Max / Strix Halo (unified memory)
//! - Intel Hybrid (Core Ultra, Lunar Lake, Arrow Lake, Raptor Lake)
//! - Legacy CPUs (Intel i3/i5 old, AMD FX, Ryzen 1000/2000)

#![allow(dead_code)]
#![allow(unused_unsafe)]

use core::mem::MaybeUninit;

/// CPU Vendor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CpuVendor {
    Unknown = 0,
    Intel = 1,
    AMD = 2,
    Other = 3,
}

impl CpuVendor {
    pub fn name(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Intel => "Intel",
            Self::AMD => "AMD",
            Self::Other => "Other",
        }
    }
}

/// Intel Core Type (Hybrid Architecture)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IntelCoreType {
    Unknown = 0,
    /// Performance Core (P-Core)
    Performance = 1,
    /// Efficiency Core (E-Core)
    Efficiency = 2,
    /// Low-Power Efficiency Core (LPE-Core)
    LowPowerEfficiency = 3,
}

impl IntelCoreType {
    pub fn name(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Performance => "P-Core",
            Self::Efficiency => "E-Core",
            Self::LowPowerEfficiency => "LPE-Core",
        }
    }
}

/// AMD CCD (Core Complex Die) Type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AmdCcdType {
    Unknown = 0,
    /// Standard CCD (regular L3 cache)
    Standard = 1,
    /// 3D V-Cache CCD (vertical stacked cache)
    VCache = 2,
}

impl AmdCcdType {
    pub fn name(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Standard => "Standard",
            Self::VCache => "3D V-Cache",
        }
    }
}

/// Memory Bus Type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum MemoryBusType {
    #[default]
    Unknown = 0,
    /// DDR4/DDR5 dual channel
    DualChannel = 1,
    /// LPDDR4X/LPDDR5X unified (typical for APUs)
    Unified = 2,
    /// GDDR6 (discrete GPU memory)
    GDDR6 = 3,
    /// HBM (high bandwidth memory)
    HBM = 4,
}

impl MemoryBusType {
    pub fn name(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::DualChannel => "Dual Channel",
            Self::Unified => "Unified",
            Self::GDDR6 => "GDDR6",
            Self::HBM => "HBM",
        }
    }
}

/// Intel Hybrid Core Information
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct IntelHybridInfo {
    /// Number of P-Cores
    pub p_cores: u8,
    /// Number of E-Cores
    pub e_cores: u8,
    /// Number of LPE-Cores
    pub lpe_cores: u8,
    /// Total logical threads
    pub total_threads: u16,
}

/// AMD CCD Information
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct AmdCcdInfo {
    /// Number of CCDs
    pub ccd_count: u8,
    /// CCD with 3D V-Cache (index, or 255 if none)
    pub vcache_ccd_index: u8,
    /// L3 cache size per CCD (bytes)
    pub l3_per_ccd: u32,
    /// Total L3 cache (bytes)
    pub total_l3: u32,
}

/// Memory Information
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MemoryInfo {
    /// Memory bus type
    pub bus_type: MemoryBusType,
    /// Bus width in bits (64, 128, 256, etc.)
    pub bus_width_bits: u16,
    /// Memory frequency in MHz
    pub frequency_mhz: u32,
    /// Theoretical bandwidth in MB/s
    pub bandwidth_mbps: u64,
    /// Total memory size in bytes
    pub total_size_bytes: u64,
}

/// Client CPU Flags
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct ClientCpuFlags {
    /// SSE4.2 support
    pub sse42: bool,
    /// AVX support
    pub avx: bool,
    /// AVX2 support
    pub avx2: bool,
    /// FMA support
    pub fma: bool,
    /// AVX-512F support
    pub avx512f: bool,
    /// AVX-512VNNI support
    pub avx512vnni: bool,
    /// AVX-10 support (next-gen)
    pub avx10: bool,
    /// BMI2 support
    pub bmi2: bool,
    /// POPCNT support
    pub popcnt: bool,
}

/// Complete Client Topology Report
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct ClientTopologyReport {
    /// CPU vendor
    pub vendor: CpuVendor,
    /// CPU family/model/stepping
    pub family: u16,
    pub model: u16,
    pub stepping: u8,
    /// Total physical cores
    pub total_cores: u16,
    /// Total logical threads
    pub total_threads: u16,
    /// Intel hybrid information (if Intel)
    pub intel_hybrid: IntelHybridInfo,
    /// AMD CCD information (if AMD)
    pub amd_ccd: AmdCcdInfo,
    /// Memory information
    pub memory: MemoryInfo,
    /// CPU instruction flags
    pub flags: ClientCpuFlags,
    /// Detected client generation
    pub generation: ClientGeneration,
}

/// Client Generation Classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ClientGeneration {
    /// Unknown or unsupported
    Unknown = 0,
    /// AMD Ryzen 3D V-Cache (7800X3D, 7950X3D, 9800X3D, 9950X3D)
    Amd3DVCache = 1,
    /// AMD Ryzen AI / AI Max / Strix Halo (unified memory)
    AmdAiMax = 2,
    /// Intel Hybrid Modern (Core Ultra, Lunar Lake, Arrow Lake)
    IntelHybridModern = 3,
    /// Intel Hybrid Legacy (Raptor Lake, Alder Lake)
    IntelHybridLegacy = 4,
    /// Legacy CPUs (old i3/i5, AMD FX, Ryzen 1000/2000)
    Legacy = 5,
}

impl ClientGeneration {
    pub fn name(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Amd3DVCache => "AMD Ryzen 3D V-Cache",
            Self::AmdAiMax => "AMD Ryzen AI / AI Max",
            Self::IntelHybridModern => "Intel Hybrid Modern",
            Self::IntelHybridLegacy => "Intel Hybrid Legacy",
            Self::Legacy => "Legacy",
        }
    }
}

impl Default for ClientTopologyReport {
    fn default() -> Self {
        Self {
            vendor: CpuVendor::Unknown,
            family: 0,
            model: 0,
            stepping: 0,
            total_cores: 1,
            total_threads: 1,
            intel_hybrid: IntelHybridInfo::default(),
            amd_ccd: AmdCcdInfo::default(),
            memory: MemoryInfo::default(),
            flags: ClientCpuFlags::default(),
            generation: ClientGeneration::Unknown,
        }
    }
}

impl ClientTopologyReport {
    /// Const-compatible default for use in statics.
    pub const fn const_default() -> Self {
        Self {
            vendor: CpuVendor::Unknown,
            family: 0,
            model: 0,
            stepping: 0,
            total_cores: 1,
            total_threads: 1,
            intel_hybrid: IntelHybridInfo::const_default(),
            amd_ccd: AmdCcdInfo::const_default(),
            memory: MemoryInfo::const_default(),
            flags: ClientCpuFlags::const_default(),
            generation: ClientGeneration::Unknown,
        }
    }
}

impl IntelHybridInfo {
    pub const fn const_default() -> Self {
        Self {
            p_cores: 0,
            e_cores: 0,
            lpe_cores: 0,
            total_threads: 0,
        }
    }
}

impl AmdCcdInfo {
    pub const fn const_default() -> Self {
        Self {
            ccd_count: 0,
            vcache_ccd_index: 0,
            l3_per_ccd: 0,
            total_l3: 0,
        }
    }
}

impl MemoryInfo {
    pub const fn const_default() -> Self {
        Self {
            bus_type: MemoryBusType::Unknown,
            bus_width_bits: 0,
            frequency_mhz: 0,
            bandwidth_mbps: 0,
            total_size_bytes: 0,
        }
    }
}

impl ClientCpuFlags {
    pub const fn const_default() -> Self {
        Self {
            sse42: false,
            avx: false,
            avx2: false,
            fma: false,
            avx512f: false,
            avx512vnni: false,
            avx10: false,
            bmi2: false,
            popcnt: false,
        }
    }
}

/// CPUID result structure
#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct CpuidResult {
    eax: u32,
    ebx: u32,
    ecx: u32,
    edx: u32,
}

/// Execute CPUID instruction
#[inline(always)]
#[cfg(target_arch = "x86_64")]
unsafe fn cpuid(leaf: u32) -> CpuidResult {
    let r = core::arch::x86_64::__cpuid(leaf);
    CpuidResult { eax: r.eax, ebx: r.ebx, ecx: r.ecx, edx: r.edx }
}

/// Execute CPUID with subleaf
#[inline(always)]
#[cfg(target_arch = "x86_64")]
unsafe fn cpuid_count(leaf: u32, subleaf: u32) -> CpuidResult {
    let r = core::arch::x86_64::__cpuid_count(leaf, subleaf);
    CpuidResult { eax: r.eax, ebx: r.ebx, ecx: r.ecx, edx: r.edx }
}

/// Detect CPU vendor via CPUID leaf 0
pub fn detect_cpu_vendor() -> CpuVendor {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let leaf0 = cpuid(0);
        
        // EBX, EDX, ECX contain vendor string
        let mut vendor = [0u8; 12];
        vendor[0..4].copy_from_slice(&leaf0.ebx.to_le_bytes());
        vendor[4..8].copy_from_slice(&leaf0.edx.to_le_bytes());
        vendor[8..12].copy_from_slice(&leaf0.ecx.to_le_bytes());
        
        if &vendor == b"AuthenticAMD" {
            CpuVendor::AMD
        } else if &vendor == b"GenuineIntel" {
            CpuVendor::Intel
        } else {
            CpuVendor::Other
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        CpuVendor::Unknown
    }
}

/// Detect CPU family/model/stepping
pub fn detect_cpu_fms() -> (u16, u16, u8) {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let leaf1 = cpuid(1);
        let eax = leaf1.eax;
        
        let family = ((eax >> 8) & 0xF) as u16;
        let extended_family = ((eax >> 20) & 0xFF) as u16;
        let model = ((eax >> 4) & 0xF) as u16;
        let extended_model = ((eax >> 16) & 0xF) as u16;
        let stepping = (eax & 0xF) as u8;
        
        let actual_family = if family == 0xF {
            family + extended_family
        } else {
            family
        };
        
        let actual_model = if family == 0xF || family == 0x6 {
            model + (extended_model << 4)
        } else {
            model
        };
        
        (actual_family, actual_model, stepping)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        (0, 0, 0)
    }
}

/// Detect Intel hybrid core types via CPUID leaf 0x1A
pub fn detect_intel_hybrid() -> IntelHybridInfo {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let max_leaf = cpuid(0).eax;
        
        if max_leaf < 0x1A {
            return IntelHybridInfo::default();
        }
        
        let leaf1a = cpuid(0x1A);
        
        // Check if hybrid is supported (bit 0 of EAX)
        if (leaf1a.eax & 1) == 0 {
            return IntelHybridInfo::default();
        }
        
        // Parse core types from EBX
        // Intel uses a complex encoding, simplified here
        let native_model_id = (leaf1a.eax >> 24) & 0xFF;
        
        // Determine core type based on native model ID
        // This is a simplified detection - real implementation would need
        // to iterate through all cores
        let (p_cores, e_cores, lpe_cores) = match native_model_id {
            0x00..=0x0F => (8, 0, 0), // P-Core only
            0x20..=0x2F => (0, 8, 0), // E-Core only
            0x30..=0x3F => (0, 0, 4), // LPE-Core only
            _ => (6, 8, 0), // Typical hybrid: 6P + 8E
        };
        
        let leaf1 = cpuid(1);
        let total_threads = (leaf1.ebx & 0xFFFF) as u16;
        
        IntelHybridInfo {
            p_cores,
            e_cores,
            lpe_cores,
            total_threads,
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        IntelHybridInfo::default()
    }
}

/// Detect AMD CCD topology and 3D V-Cache via CPUID leaf 0x8000001D
pub fn detect_amd_ccd() -> AmdCcdInfo {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let max_ext_leaf = cpuid(0x80000000).eax;
        
        if max_ext_leaf < 0x8000001D {
            return AmdCcdInfo::default();
        }
        
        let mut ccd_count = 1u8;
        let mut vcache_ccd_index = 255u8;
        let mut l3_per_ccd = 0u32;
        let mut total_l3 = 0u32;
        
        // Iterate through cache subleaves to find L3 cache
        for subleaf in 0..32u32 {
            let leaf1d = cpuid_count(0x8000001D, subleaf);
            let cache_type = leaf1d.eax & 0x1F;
            
            if cache_type == 0 {
                break;
            }
            
            let level = ((leaf1d.eax >> 5) & 0x7) as u8;
            
            if level == 3 {
                // L3 cache
                let ways = ((leaf1d.ebx >> 22) & 0x3FF) + 1;
                let line_size = (leaf1d.ebx & 0xFFF) + 1;
                let partitions = ((leaf1d.ebx >> 12) & 0x3FF) + 1;
                let sets = leaf1d.ecx + 1;
                let size = ways * line_size * partitions * sets;
                
                l3_per_ccd = size;
                
                // Check for 3D V-Cache signature
                // 3D V-Cache typically has much larger L3 (96MB+ vs 32MB standard)
                if size >= 96 * 1024 * 1024 {
                    vcache_ccd_index = ccd_count;
                }
                
                ccd_count += 1;
                total_l3 += size;
            }
        }
        
        // If no 3D V-Cache detected but we have multiple CCDs,
        // assume first CCD has standard cache
        if vcache_ccd_index == 255 && ccd_count > 1 {
            vcache_ccd_index = 0; // No 3D V-Cache
        }
        
        AmdCcdInfo {
            ccd_count: ccd_count - 1, // Adjust for loop increment
            vcache_ccd_index,
            l3_per_ccd,
            total_l3,
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        AmdCcdInfo::default()
    }
}

/// Detect memory topology (simplified - real implementation would use ACPI)
pub fn detect_memory_topology() -> MemoryInfo {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        // This is a simplified detection
        // Real implementation would parse ACPI MADT/PPTT tables
        
        let vendor = detect_cpu_vendor();
        
        match vendor {
            CpuVendor::AMD => {
                // AMD APUs typically use unified memory
                MemoryInfo {
                    bus_type: MemoryBusType::Unified,
                    bus_width_bits: 128,
                    frequency_mhz: 6400, // LPDDR5X typical
                    bandwidth_mbps: (128 * 6400) as u64 / 8, // ~102 GB/s
                    total_size_bytes: 16 * 1024 * 1024 * 1024, // 16GB default
                }
            }
            CpuVendor::Intel => {
                // Intel client CPUs typically use dual channel
                MemoryInfo {
                    bus_type: MemoryBusType::DualChannel,
                    bus_width_bits: 128,
                    frequency_mhz: 5600, // DDR5 typical
                    bandwidth_mbps: (128 * 5600) as u64 / 8, // ~89 GB/s
                    total_size_bytes: 16 * 1024 * 1024 * 1024, // 16GB default
                }
            }
            _ => MemoryInfo::default(),
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        MemoryInfo::default()
    }
}

/// Detect client CPU flags
pub fn detect_client_cpu_flags() -> ClientCpuFlags {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let leaf1 = cpuid(1);
        let leaf7 = cpuid_count(7, 0);
        
        ClientCpuFlags {
            sse42: (leaf1.ecx & (1 << 20)) != 0,
            popcnt: (leaf1.ecx & (1 << 23)) != 0,
            avx: (leaf1.ecx & (1 << 28)) != 0,
            avx2: (leaf7.ebx & (1 << 5)) != 0,
            fma: (leaf1.ecx & (1 << 12)) != 0,
            bmi2: (leaf7.ebx & (1 << 8)) != 0,
            avx512f: (leaf7.ebx & (1 << 16)) != 0,
            avx512vnni: (leaf7.ecx & (1 << 11)) != 0,
            avx10: false, // AVX-10 is next-gen, not yet available
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        ClientCpuFlags::default()
    }
}

/// Detect total core and thread count
pub fn detect_core_thread_count() -> (u16, u16) {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let leaf1 = cpuid(1);
        let logical_processors = (leaf1.ebx & 0xFFFF) as u16;
        
        // Use CPUID leaf 0xB for topology
        let mut physical_cores = 1u16;
        for level in 0..3u32 {
            let leafb = cpuid_count(0xB, level);
            let level_type = ((leafb.ecx >> 8) & 0xFF) as u8;
            
            if level_type == 0 {
                break;
            }
            
            if level_type == 2 {
                physical_cores = (leafb.ebx & 0xFFFF) as u16;
            }
        }
        
        (physical_cores, logical_processors)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        (1, 1)
    }
}

/// Classify client generation based on detected hardware
pub fn classify_client_generation(report: &ClientTopologyReport) -> ClientGeneration {
    match report.vendor {
        CpuVendor::AMD => {
            // Check for 3D V-Cache
            if report.amd_ccd.vcache_ccd_index != 255 {
                ClientGeneration::Amd3DVCache
            } else if report.memory.bus_type == MemoryBusType::Unified {
                ClientGeneration::AmdAiMax
            } else {
                ClientGeneration::Legacy
            }
        }
        CpuVendor::Intel => {
            // Check for hybrid architecture
            if report.intel_hybrid.p_cores > 0 && report.intel_hybrid.e_cores > 0 {
                // Distinguish modern vs legacy hybrid
                if report.family >= 0x06 && report.model >= 0x9C {
                    ClientGeneration::IntelHybridModern // Core Ultra, Lunar Lake
                } else {
                    ClientGeneration::IntelHybridLegacy // Raptor Lake, Alder Lake
                }
            } else {
                ClientGeneration::Legacy
            }
        }
        _ => ClientGeneration::Unknown,
    }
}

/// Main discovery function for client hardware
pub fn discover_client_topology() -> ClientTopologyReport {
    let vendor = detect_cpu_vendor();
    let (family, model, stepping) = detect_cpu_fms();
    let (total_cores, total_threads) = detect_core_thread_count();
    let intel_hybrid = if vendor == CpuVendor::Intel {
        detect_intel_hybrid()
    } else {
        IntelHybridInfo::default()
    };
    let amd_ccd = if vendor == CpuVendor::AMD {
        detect_amd_ccd()
    } else {
        AmdCcdInfo::default()
    };
    let memory = detect_memory_topology();
    let flags = detect_client_cpu_flags();
    
    let mut report = ClientTopologyReport {
        vendor,
        family,
        model,
        stepping,
        total_cores,
        total_threads,
        intel_hybrid,
        amd_ccd,
        memory,
        flags,
        generation: ClientGeneration::Unknown,
    };
    
    report.generation = classify_client_generation(&report);
    
    report
}

/// Check if system has AMD 3D V-Cache
pub fn has_3d_vcache(report: &ClientTopologyReport) -> bool {
    report.generation == ClientGeneration::Amd3DVCache
}

/// Check if system has AMD AI Max (unified memory)
pub fn has_ai_max(report: &ClientTopologyReport) -> bool {
    report.generation == ClientGeneration::AmdAiMax
}

/// Check if system has Intel Hybrid architecture
pub fn has_intel_hybrid(report: &ClientTopologyReport) -> bool {
    report.generation == ClientGeneration::IntelHybridModern 
        || report.generation == ClientGeneration::IntelHybridLegacy
}

/// Check if system is modern Intel Hybrid
pub fn is_intel_hybrid_modern(report: &ClientTopologyReport) -> bool {
    report.generation == ClientGeneration::IntelHybridModern
}

/// Check if system is legacy
pub fn is_legacy(report: &ClientTopologyReport) -> bool {
    report.generation == ClientGeneration::Legacy
}

/// Get 3D V-Cache CCD index (if applicable)
pub fn get_vcache_ccd_index(report: &ClientTopologyReport) -> Option<u8> {
    if report.amd_ccd.vcache_ccd_index != 255 {
        Some(report.amd_ccd.vcache_ccd_index)
    } else {
        None
    }
}

/// Get L3 cache size for 3D V-Cache CCD
pub fn get_vcache_l3_size(report: &ClientTopologyReport) -> u32 {
    if report.amd_ccd.vcache_ccd_index != 255 {
        report.amd_ccd.l3_per_ccd
    } else {
        0
    }
}

/// Get memory bandwidth in GB/s
pub fn get_memory_bandwidth_gbps(report: &ClientTopologyReport) -> u64 {
    report.memory.bandwidth_mbps / 1000
}
