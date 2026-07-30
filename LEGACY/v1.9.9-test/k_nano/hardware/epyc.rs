//! AMD EPYC Hardware Discovery Module
//! 
//! Low-level CPUID and ACPI (SRAT/SLIT) parsing for AMD EPYC server processors
//! Detects multi-socket topology, complex NUMA nodes, CCDs, and 3D V-Cache
//! 
//! # Supported EPYC Generations
//! - EPYC 7001/7002/7003 (Naples/Rome/Milan) - Zen 1/2/3
//! - EPYC 7004 (Genoa/Bergamo) - Zen 4/Zen 4c
//! - EPYC 9004 (Genoa-X/Bergamo-X) - Zen 4/Zen 4c with 3D V-Cache
//! - EPYC 8004 (Turin) - Zen 5

#![allow(dead_code)]
#![allow(unused_unsafe)]


/// EPYC Generation Classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EpycGeneration {
    /// Unknown or unsupported
    Unknown = 0,
    /// EPYC 7001 (Naples) - Zen 1
    Naples = 1,
    /// EPYC 7002 (Rome) - Zen 2
    Rome = 2,
    /// EPYC 7003 (Milan) - Zen 3
    Milan = 3,
    /// EPYC 7004 (Genoa) - Zen 4 Standard
    Genoa = 4,
    /// EPYC 7004 (Bergamo) - Zen 4c Dense
    Bergamo = 5,
    /// EPYC 9004 (Genoa-X) - Zen 4 Standard + 3D V-Cache
    GenoaX = 6,
    /// EPYC 9004 (Bergamo-X) - Zen 4c Dense + 3D V-Cache
    BergamoX = 7,
    /// EPYC 8004 (Turin) - Zen 5 Standard
    Turin = 8,
    /// EPYC 8004 (Turin-Dense) - Zen 5c Dense
    TurinDense = 9,
    /// EPYC 9004 (Siena) - Zen 4c Dense (embedded/server)
    Siena = 10,
}

impl EpycGeneration {
    pub fn name(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Naples => "EPYC 7001 (Naples - Zen 1)",
            Self::Rome => "EPYC 7002 (Rome - Zen 2)",
            Self::Milan => "EPYC 7003 (Milan - Zen 3)",
            Self::Genoa => "EPYC 7004 (Genoa - Zen 4 Standard)",
            Self::Bergamo => "EPYC 7004 (Bergamo - Zen 4c Dense)",
            Self::GenoaX => "EPYC 9004 (Genoa-X - Zen 4 + 3D V-Cache)",
            Self::BergamoX => "EPYC 9004 (Bergamo-X - Zen 4c + 3D V-Cache)",
            Self::Turin => "EPYC 8004 (Turin - Zen 5 Standard)",
            Self::TurinDense => "EPYC 8004 (Turin-Dense - Zen 5c Dense)",
            Self::Siena => "EPYC 9004 (Siena - Zen 4c Dense)",
        }
    }
    
    pub fn zen_version(self) -> u8 {
        match self {
            Self::Naples => 1,
            Self::Rome => 2,
            Self::Milan => 3,
            Self::Genoa | Self::GenoaX => 4,
            Self::Bergamo | Self::BergamoX | Self::Siena => 4,
            Self::Turin => 5,
            Self::TurinDense => 5,
            Self::Unknown => 0,
        }
    }
    
    pub fn is_dense(self) -> bool {
        matches!(
            self,
            Self::Bergamo | Self::BergamoX | Self::TurinDense | Self::Siena
        )
    }
    
    pub fn is_standard(self) -> bool {
        matches!(
            self,
            Self::Genoa | Self::GenoaX | Self::Turin
        )
    }
}

/// NUMA Per Socket (NPS) configuration for EPYC
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EpycNpsMode {
    /// Unknown
    Unknown = 0,
    /// NPS1: Single NUMA node per socket
    Nps1 = 1,
    /// NPS2: Two NUMA nodes per socket
    Nps2 = 2,
    /// NPS4: Four NUMA nodes per socket
    Nps4 = 4,
}

impl EpycNpsMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Nps1 => "NPS1 (Single NUMA node)",
            Self::Nps2 => "NPS2 (Two NUMA nodes)",
            Self::Nps4 => "NPS4 (Four NUMA nodes)",
        }
    }
}

/// NUMA Node information for EPYC
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct EpycNumaNode {
    /// Node ID (0-based)
    pub node_id: u8,
    /// Socket this node belongs to
    pub socket_id: u8,
    /// CCD this node belongs to
    pub ccd_id: u8,
    /// First physical core in this node
    pub core_start: u8,
    /// Number of physical cores in this node
    pub core_count: u8,
    /// Physical RAM start address (bytes)
    pub memory_start: u64,
    /// Physical RAM size (bytes)
    pub memory_size: u64,
    /// Distance to other nodes (SLIT table)
    pub distances: [u8; 24], // Max 24 NUMA nodes for dual-socket EPYC
}

/// CCD (Core Complex Die) information
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct EpycCcdInfo {
    /// CCD ID within socket
    pub ccd_id: u8,
    /// Socket this CCD belongs to
    pub socket_id: u8,
    /// Number of cores in this CCD
    pub core_count: u8,
    /// L3 cache size (bytes)
    pub l3_size: u32,
    /// Has 3D V-Cache (vertical stacked)
    pub has_3d_vcache: bool,
    /// 3D V-Cache size (bytes, if applicable)
    pub vcache_size: u32,
}

/// Socket information for EPYC
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct EpycSocketInfo {
    /// Socket ID (0-based)
    pub socket_id: u8,
    /// Number of CCDs in this socket
    pub ccd_count: u8,
    /// Total physical cores in this socket
    pub core_count: u16,
    /// Total logical threads in this socket
    pub thread_count: u16,
    /// I/O die memory channels (typically 8)
    pub memory_channels: u8,
    /// Memory type (DDR4/DDR5)
    pub memory_type: EpycMemoryType,
    /// CCD information (max 12 CCDs per socket)
    pub ccds: [EpycCcdInfo; 12],
}

/// Memory type for EPYC
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum EpycMemoryType {
    #[default]
    Unknown = 0,
    DDR4 = 1,
    DDR5 = 2,
}

impl EpycMemoryType {
    pub fn name(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::DDR4 => "DDR4",
            Self::DDR5 => "DDR5",
        }
    }
}

/// EPYC CPU Flags
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct EpycCpuFlags {
    /// AVX2 support
    pub avx2: bool,
    /// AVX-512F support (not available on EPYC, but structure for future)
    pub avx512f: bool,
    /// AVX-512DQ support
    pub avx512dq: bool,
    /// AVX-512BW support
    pub avx512bw: bool,
    /// AVX-512VNNI support
    pub avx512vnni: bool,
    /// BMI2 support
    pub bmi2: bool,
    /// POPCNT support
    pub popcnt: bool,
    /// SSE4.2 support
    pub sse42: bool,
    /// CLZERO support
    pub clzero: bool,
    /// RDPRU support
    pub rdpru: bool,
    /// MCOMMIT support
    pub mcommit: bool,
}

/// Complete EPYC Topology Report
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct EpycTopologyReport {
    /// EPYC generation
    pub generation: EpycGeneration,
    /// Number of physical sockets
    pub socket_count: u8,
    /// Total physical cores across all sockets
    pub total_cores: u16,
    /// Total logical threads across all sockets
    pub total_threads: u16,
    /// NPS (NUMA Per Socket) mode
    pub nps_mode: EpycNpsMode,
    /// Total NUMA nodes
    pub numa_node_count: u8,
    /// Socket information (max 2 sockets)
    pub sockets: [EpycSocketInfo; 2],
    /// NUMA node information (max 24 nodes)
    pub numa_nodes: [EpycNumaNode; 24],
    /// CPU instruction flags
    pub flags: EpycCpuFlags,
    /// Total system memory (bytes)
    pub total_memory: u64,
    /// Has 3D V-Cache (total L3 > 1GB per socket)
    pub has_3d_vcache: bool,
    /// Total L3 cache size (bytes)
    pub total_l3_cache: u64,
}

impl Default for EpycTopologyReport {
    fn default() -> Self {
        Self {
            generation: EpycGeneration::Unknown,
            socket_count: 1,
            total_cores: 1,
            total_threads: 1,
            nps_mode: EpycNpsMode::Unknown,
            numa_node_count: 1,
            sockets: [EpycSocketInfo::default(), EpycSocketInfo::default()],
            numa_nodes: [EpycNumaNode::default(); 24],
            flags: EpycCpuFlags::default(),
            total_memory: 0,
            has_3d_vcache: false,
            total_l3_cache: 0,
        }
    }
}

impl EpycTopologyReport {
    /// Const-compatible default for use in statics.
    pub const fn const_default() -> Self {
        Self {
            generation: EpycGeneration::Unknown,
            socket_count: 1,
            total_cores: 1,
            total_threads: 1,
            nps_mode: EpycNpsMode::Unknown,
            numa_node_count: 1,
            sockets: [
                EpycSocketInfo::const_default(),
                EpycSocketInfo::const_default(),
            ],
            numa_nodes: [EpycNumaNode::const_default(); 24],
            flags: EpycCpuFlags::const_default(),
            total_memory: 0,
            has_3d_vcache: false,
            total_l3_cache: 0,
        }
    }
}

impl EpycSocketInfo {
    pub const fn const_default() -> Self {
        Self {
            socket_id: 0,
            ccd_count: 0,
            core_count: 0,
            thread_count: 0,
            memory_channels: 0,
            memory_type: EpycMemoryType::Unknown,
            ccds: [EpycCcdInfo::const_default(); 12],
        }
    }
}

impl EpycCcdInfo {
    pub const fn const_default() -> Self {
        Self {
            ccd_id: 0,
            socket_id: 0,
            core_count: 0,
            l3_size: 0,
            has_3d_vcache: false,
            vcache_size: 0,
        }
    }
}

impl EpycNumaNode {
    pub const fn const_default() -> Self {
        Self {
            node_id: 0,
            socket_id: 0,
            ccd_id: 0,
            core_start: 0,
            core_count: 0,
            memory_start: 0,
            memory_size: 0,
            distances: [0; 24],
        }
    }
}

impl EpycCpuFlags {
    pub const fn const_default() -> Self {
        Self {
            avx2: false,
            avx512f: false,
            avx512dq: false,
            avx512bw: false,
            avx512vnni: false,
            bmi2: false,
            popcnt: false,
            sse42: false,
            clzero: false,
            rdpru: false,
            mcommit: false,
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

/// Detect if CPU is AMD EPYC via CPUID
pub fn is_epyc() -> bool {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let leaf0 = cpuid(0);
        
        // Check vendor string
        let mut vendor = [0u8; 12];
        vendor[0..4].copy_from_slice(&leaf0.ebx.to_le_bytes());
        vendor[4..8].copy_from_slice(&leaf0.edx.to_le_bytes());
        vendor[8..12].copy_from_slice(&leaf0.ecx.to_le_bytes());
        
        if &vendor != b"AuthenticAMD" {
            return false;
        }
        
        // Check extended family/model for EPYC
        let leaf1 = cpuid(1);
        let family = ((leaf1.eax >> 8) & 0xF) as u16;
        let ext_family = ((leaf1.eax >> 20) & 0xFF) as u16;
        let model = ((leaf1.eax >> 4) & 0xF) as u16;
        let ext_model = ((leaf1.eax >> 16) & 0xF) as u16;
        
        let actual_family = if family == 0xF { family + ext_family } else { family };
        let _actual_model = if family == 0xF || family == 0x6 {
            model + (ext_model << 4)
        } else {
            model
        };
        
        // EPYC uses Family 17h (Zen 1), 19h (Zen 3), 1Ah (Zen 4)
        matches!(actual_family, 0x17 | 0x19 | 0x1A)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

/// Classify EPYC generation based on CPUID
pub fn classify_epyc_generation() -> EpycGeneration {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let leaf1 = cpuid(1);
        let family = ((leaf1.eax >> 8) & 0xF) as u16;
        let ext_family = ((leaf1.eax >> 20) & 0xFF) as u16;
        let model = ((leaf1.eax >> 4) & 0xF) as u16;
        let ext_model = ((leaf1.eax >> 16) & 0xF) as u16;
        
        let actual_family = if family == 0xF { family + ext_family } else { family };
        let actual_model = if family == 0xF || family == 0x6 {
            model + (ext_model << 4)
        } else {
            model
        };
        
        match (actual_family, actual_model) {
            // Family 17h (Zen 1/2)
            (0x17, 0x0..=0x0F) => EpycGeneration::Naples,    // Zen 1
            (0x17, 0x10..=0x2F) => EpycGeneration::Rome,      // Zen 2
            (0x17, 0x30..=0x4F) => EpycGeneration::Rome,      // Zen 2
            
            // Family 19h (Zen 3)
            (0x19, 0x0..=0x0F) => EpycGeneration::Milan,     // Zen 3
            (0x19, 0x10..=0x2F) => EpycGeneration::Milan,     // Zen 3
            
            // Family 1Ah (Zen 4)
            (0x1A, 0x0..=0x0F) => EpycGeneration::Genoa,     // Zen 4 Standard
            (0x1A, 0x10..=0x1F) => EpycGeneration::Bergamo,   // Zen 4c Dense
            (0x1A, 0x20..=0x2F) => EpycGeneration::GenoaX,    // Zen 4 + 3D V-Cache
            (0x1A, 0x30..=0x3F) => EpycGeneration::BergamoX,  // Zen 4c + 3D V-Cache
            (0x1A, 0x40..=0x4F) => EpycGeneration::Siena,     // Zen 4c Dense (embedded)
            
            // Family 1Bh (Zen 5 - Turin)
            (0x1B, 0x0..=0x0F) => EpycGeneration::Turin,     // Zen 5 Standard
            (0x1B, 0x10..=0x1F) => EpycGeneration::TurinDense, // Zen 5c Dense
            
            _ => EpycGeneration::Unknown,
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        EpycGeneration::Unknown
    }
}

/// Detect EPYC CPU flags
pub fn detect_epyc_flags() -> EpycCpuFlags {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let leaf1 = cpuid(1);
        let leaf7 = cpuid_count(7, 0);
        let leaf8000_0001 = cpuid(0x8000_0001);
        let _leaf8000_0008 = cpuid(0x8000_0008);
        
        EpycCpuFlags {
            sse42: (leaf1.ecx & (1 << 20)) != 0,
            popcnt: (leaf1.ecx & (1 << 23)) != 0,
            avx2: (leaf7.ebx & (1 << 5)) != 0,
            bmi2: (leaf7.ebx & (1 << 8)) != 0,
            avx512f: false, // EPYC does not support AVX-512
            avx512dq: false,
            avx512bw: false,
            avx512vnni: false,
            clzero: (leaf8000_0001.ecx & (1 << 0)) != 0,
            rdpru: (leaf8000_0001.ebx & (1 << 4)) != 0,
            mcommit: (leaf8000_0001.ebx & (1 << 8)) != 0,
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        EpycCpuFlags::default()
    }
}

/// Detect CCD topology via CPUID leaf 0x8000001D
pub fn detect_epyc_ccd_topology() -> (u8, [EpycCcdInfo; 12]) {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let max_ext_leaf = cpuid(0x8000_0000).eax;
        
        if max_ext_leaf < 0x8000_001D {
            return (1, [EpycCcdInfo::default(); 12]);
        }
        
        let mut ccd_count = 0u8;
        let mut ccds = [EpycCcdInfo::default(); 12];
        
        // Iterate through cache subleaves to find L3 cache per CCD
        for subleaf in 0..32u32 {
            let leaf1d = cpuid_count(0x8000_001D, subleaf);
            let cache_type = leaf1d.eax & 0x1F;
            
            if cache_type == 0 {
                break;
            }
            
            let level = ((leaf1d.eax >> 5) & 0x7) as u8;
            
            if level == 3 {
                // L3 cache - indicates a CCD
                let ways = ((leaf1d.ebx >> 22) & 0x3FF) + 1;
                let line_size = (leaf1d.ebx & 0xFFF) + 1;
                let partitions = ((leaf1d.ebx >> 12) & 0x3FF) + 1;
                let sets = leaf1d.ecx + 1;
                let size = ways * line_size * partitions * sets;
                
                // Check for 3D V-Cache (typically 96MB+ vs 32MB standard)
                let has_3d_vcache = size >= 96 * 1024 * 1024;
                let vcache_size = if has_3d_vcache { size } else { 0 };
                
                if ccd_count < 12 {
                    ccds[ccd_count as usize] = EpycCcdInfo {
                        ccd_id: ccd_count,
                        socket_id: 0, // Will be filled later
                        core_count: 0,  // Will be filled later
                        l3_size: size,
                        has_3d_vcache,
                        vcache_size,
                    };
                    ccd_count += 1;
                }
            }
        }
        
        (ccd_count, ccds)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        (1, [EpycCcdInfo::default(); 12])
    }
}

/// Detect socket count via CPUID leaf 0xB
pub fn detect_epyc_socket_count() -> u8 {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let mut socket_count = 1u8;
        
        // CPUID leaf 0xB provides extended topology enumeration
        for level in 0..3u32 {
            let leafb = cpuid_count(0xB, level);
            let level_type = ((leafb.ecx >> 8) & 0xFF) as u8;
            
            if level_type == 0 {
                break;
            }
            
            let processors = (leafb.ebx & 0xFFFF) as u8;
            
            // Level type 2 typically indicates die/socket level
            if level_type == 2 && processors > 32 {
                // More than 32 cores suggests multi-socket
                socket_count = 2;
            }
        }
        
        socket_count
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        1
    }
}

/// Detect core and thread count
pub fn detect_epyc_core_thread_count() -> (u16, u16) {
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
            
            if level_type == 1 {
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

/// Detect memory type based on EPYC generation
pub fn detect_epyc_memory_type(generation: EpycGeneration) -> EpycMemoryType {
    match generation {
        EpycGeneration::Naples | EpycGeneration::Rome | EpycGeneration::Milan => {
            EpycMemoryType::DDR4
        }
        EpycGeneration::Genoa | EpycGeneration::Bergamo | 
        EpycGeneration::GenoaX | EpycGeneration::BergamoX | 
        EpycGeneration::Turin => {
            EpycMemoryType::DDR5
        }
        _ => EpycMemoryType::Unknown,
    }
}

/// Parse ACPI SRAT for NUMA topology (simplified stub)
pub fn parse_epyc_srat() -> [EpycNumaNode; 24] {
    // In a real implementation, this would parse the ACPI SRAT table
    // to extract NUMA node topology for EPYC's complex NUMA layout
    let mut nodes = [EpycNumaNode::default(); 24];
    
    // Default: single NUMA node
    nodes[0] = EpycNumaNode {
        node_id: 0,
        socket_id: 0,
        ccd_id: 0,
        core_start: 0,
        core_count: 1,
        memory_start: 0,
        memory_size: 0,
        distances: [10; 24], // Local distance = 10
    };
    
    nodes
}

/// Main discovery function for EPYC hardware
pub fn discover_epyc_topology() -> EpycTopologyReport {
    let generation = classify_epyc_generation();
    let socket_count = detect_epyc_socket_count();
    let (total_cores, total_threads) = detect_epyc_core_thread_count();
    let (ccd_count, ccds) = detect_epyc_ccd_topology();
    let flags = detect_epyc_flags();
    let memory_type = detect_epyc_memory_type(generation);
    
    // Build socket information
    let mut sockets = [EpycSocketInfo::default(); 2];
    let cores_per_socket = total_cores / socket_count as u16;
    let threads_per_socket = total_threads / socket_count as u16;
    let ccds_per_socket = ccd_count / socket_count.max(1);
    
    for i in 0..socket_count as usize {
        sockets[i] = EpycSocketInfo {
            socket_id: i as u8,
            ccd_count: ccds_per_socket,
            core_count: cores_per_socket,
            thread_count: threads_per_socket,
            memory_channels: 8, // EPYC typically has 8 memory channels
            memory_type,
            ccds: [EpycCcdInfo::default(); 12],
        };
    }
    
    // Distribute CCDs across sockets
    let mut ccd_idx = 0;
    for i in 0..socket_count as usize {
        for j in 0..ccds_per_socket as usize {
            if ccd_idx < ccd_count as usize && ccd_idx < 12 {
                sockets[i].ccds[j] = ccds[ccd_idx];
                sockets[i].ccds[j].socket_id = i as u8;
                ccd_idx += 1;
            }
        }
    }
    
    // Parse NUMA topology
    let numa_nodes = parse_epyc_srat();
    let numa_node_count = if ccd_count > 0 { ccd_count } else { 1 };
    
    EpycTopologyReport {
        generation,
        socket_count,
        total_cores,
        total_threads,
        nps_mode: EpycNpsMode::Unknown,
        numa_node_count,
        sockets,
        numa_nodes,
        flags,
        total_memory: 0, // Would be filled from memory map
        has_3d_vcache: matches!(generation, EpycGeneration::GenoaX | EpycGeneration::BergamoX),
        total_l3_cache: 0,
    }
}

/// Check if EPYC has 3D V-Cache
pub fn epyc_has_3d_vcache(report: &EpycTopologyReport) -> bool {
    matches!(
        report.generation,
        EpycGeneration::GenoaX | EpycGeneration::BergamoX
    )
}

/// Get total L3 cache size
pub fn epyc_total_l3_size(report: &EpycTopologyReport) -> u64 {
    let mut total = 0u64;
    for i in 0..report.socket_count as usize {
        for j in 0..report.sockets[i].ccd_count as usize {
            total += report.sockets[i].ccds[j].l3_size as u64;
        }
    }
    total
}

/// Get 3D V-Cache size (if applicable)
pub fn epyc_vcache_size(report: &EpycTopologyReport) -> u64 {
    let mut total = 0u64;
    for i in 0..report.socket_count as usize {
        for j in 0..report.sockets[i].ccd_count as usize {
            if report.sockets[i].ccds[j].has_3d_vcache {
                total += report.sockets[i].ccds[j].vcache_size as u64;
            }
        }
    }
    total
}

/// Check if EPYC is Zen 4 or later (supports DDR5)
pub fn epyc_is_zen4_plus(report: &EpycTopologyReport) -> bool {
    report.generation.zen_version() >= 4
}

/// Check if EPYC is Zen 5 (Turin)
pub fn epyc_is_zen5(report: &EpycTopologyReport) -> bool {
    report.generation.zen_version() >= 5
}
