//! Boot Integration for Hardware-Aware AI
//! 
//! This module integrates the hardware discovery, cognitive adaptation,
//! and SIMD kernel dispatch into a unified boot flow.
//! 
//! # Boot Flow
//! 1. Early boot: k-nano discovers Xeon topology via CPUID/ACPI
//! 2. Mid boot: Hermes receives topology report and generates adaptation policy
//! 3. Late boot: k-ai selects optimal SIMD kernels based on policy
//! 4. Runtime: AI operations use hardware-optimized kernels with proper NUMA affinity

#![allow(dead_code)]
#![allow(unused_unsafe)]

use crate::hardware::xeon::{
    XeonTopologyReport, discover_xeon_topology, log_topology_report,
    is_old_xeon, is_modern_xeon, has_avx512, has_amx,
};

/// Boot-time hardware discovery state
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct BootHardwareState {
    /// Hardware topology report
    pub topology: XeonTopologyReport,
    /// Discovery completed flag
    pub discovery_complete: bool,
    /// Adaptation completed flag
    pub adaptation_complete: bool,
    /// Kernel dispatch completed flag
    pub dispatch_complete: bool,
}

/// Global boot hardware state
static mut BOOT_STATE: BootHardwareState = BootHardwareState {
    topology: XeonTopologyReport::const_default(),
    discovery_complete: false,
    adaptation_complete: false,
    dispatch_complete: false,
};

/// Perform hardware discovery during early boot
/// 
/// This should be called as early as possible in the boot process,
/// before any memory allocation or SMP initialization.
/// 
/// # Returns
/// The discovered Xeon topology report
pub fn boot_hardware_discovery() -> XeonTopologyReport {
    let topology = discover_xeon_topology();
    
    unsafe {
        BOOT_STATE.topology = topology;
        BOOT_STATE.discovery_complete = true;
    }
    
    log_topology_report(&topology);
    
    topology
}

/// Get the boot-time topology report
/// 
/// # Safety
/// Must only be called after boot_hardware_discovery()
pub unsafe fn get_boot_topology() -> XeonTopologyReport {
    BOOT_STATE.topology
}

/// Check if hardware discovery is complete
pub fn is_discovery_complete() -> bool {
    unsafe { BOOT_STATE.discovery_complete }
}

/// Mark adaptation as complete (called by Hermes)
pub unsafe fn mark_adaptation_complete() {
    BOOT_STATE.adaptation_complete = true;
}

/// Check if adaptation is complete
pub fn is_adaptation_complete() -> bool {
    unsafe { BOOT_STATE.adaptation_complete }
}

/// Mark kernel dispatch as complete (called by k-ai)
pub unsafe fn mark_dispatch_complete() {
    BOOT_STATE.dispatch_complete = true;
}

/// Check if kernel dispatch is complete
pub fn is_dispatch_complete() -> bool {
    unsafe { BOOT_STATE.dispatch_complete }
}

/// Get the complete boot state
pub unsafe fn get_boot_state() -> BootHardwareState {
    BOOT_STATE
}

/// Check if the system is ready for AI operations
pub fn is_ai_ready() -> bool {
    is_discovery_complete() && is_adaptation_complete() && is_dispatch_complete()
}

/// Get hardware capability summary for logging
pub fn hardware_summary() -> &'static str {
    if !is_discovery_complete() {
        return "Hardware discovery not complete";
    }
    
    let topology = unsafe { BOOT_STATE.topology };
    
    if is_old_xeon(&topology) {
        "Old Xeon detected: AVX2, QPI, DDR4 - Socket isolation enabled"
    } else if is_modern_xeon(&topology) {
        if has_amx(&topology) {
            "Latest Xeon detected: AMX, AVX-512, DDR5, UPI - Massive cellular distribution"
        } else {
            "Modern Xeon detected: AVX-512, DDR4/5, UPI - High core count distribution"
        }
    } else {
        "Unknown/Unsupported hardware - Fallback mode"
    }
}

/// Get recommended memory allocation strategy
pub fn memory_allocation_strategy() -> &'static str {
    if !is_discovery_complete() {
        return "Unknown - discovery pending";
    }
    
    let topology = unsafe { BOOT_STATE.topology };
    
    if is_old_xeon(&topology) {
        if topology.socket_count >= 2 {
            "Strict NUMA isolation: Socket 0 for OS/VFS, Socket 1 for AI"
        } else {
            "Single-socket: Unified memory allocation"
        }
    } else if is_modern_xeon(&topology) {
        if topology.socket_count >= 2 {
            "Relaxed NUMA: UPI allows cross-socket traffic"
        } else {
            "Single-socket: Unified memory allocation"
        }
    } else {
        "Fallback: Conservative memory allocation"
    }
}

/// Get recommended thread scheduling strategy
pub fn thread_scheduling_strategy() -> &'static str {
    if !is_discovery_complete() {
        return "Unknown - discovery pending";
    }
    
    let topology = unsafe { BOOT_STATE.topology };
    
    if is_old_xeon(&topology) {
        "Core pinning with spin-loop for critical threads"
    } else if is_modern_xeon(&topology) {
        "Core pinning with preemption for massive cellular distribution"
    } else {
        "Fallback: Standard OS scheduling"
    }
}

/// Get SIMD kernel recommendation
pub fn simd_kernel_recommendation() -> &'static str {
    if !is_discovery_complete() {
        return "Unknown - discovery pending";
    }
    
    let topology = unsafe { BOOT_STATE.topology };
    
    if has_amx(&topology) {
        "AMX tiles (512-bit) with matrix extensions"
    } else if has_avx512(&topology) {
        "AVX-512F (512-bit) vector operations"
    } else if topology.flags.avx2 {
        "AVX2 (256-bit) vector operations"
    } else {
        "Scalar fallback"
    }
}

/// Estimate peak performance (operations per cycle per core)
pub fn estimate_peak_ops_per_cycle() -> u32 {
    if !is_discovery_complete() {
        return 64; // Conservative fallback
    }
    
    let topology = unsafe { BOOT_STATE.topology };
    
    if has_amx(&topology) {
        512 // AMX can process 512 weights/cycle
    } else if has_avx512(&topology) {
        256 // AVX-512 processes 256 weights/cycle
    } else if topology.flags.avx2 {
        128 // AVX2 processes 128 weights/cycle
    } else {
        64 // Scalar fallback
    }
}

/// Get cache-aware MoE sizing recommendation
pub fn moe_sizing_recommendation() -> (usize, u8) {
    if !is_discovery_complete() {
        return (1024 * 1024, 1); // 1MB fallback, 1 expert
    }
    
    let topology = unsafe { BOOT_STATE.topology };
    let l3_size = topology.sockets[0].cache.l3 as usize;
    
    if is_old_xeon(&topology) {
        // Old Xeon: 4 experts, 80% L3 utilization
        let expert_size = (l3_size * 80 / 100) / 4;
        (expert_size, 4)
    } else if is_modern_xeon(&topology) {
        // Modern Xeon: More experts, 75% L3 utilization
        let experts = (topology.total_physical_cores as u8 / 4).max(4).min(16);
        let expert_size = (l3_size * 75 / 100) / (experts as usize);
        (expert_size, experts)
    } else {
        (1024 * 1024, 1)
    }
}

/// Complete boot integration - call this to finalize hardware-aware setup
/// 
/// This function should be called at the end of the boot process to ensure
/// all hardware discovery, adaptation, and kernel dispatch are complete.
/// 
/// # Returns
/// true if the system is ready for AI operations, false otherwise
pub fn finalize_boot_integration() -> bool {
    if !is_ai_ready() {
        return false;
    }
    
    // Log final hardware configuration
    let _ = hardware_summary();
    let _ = memory_allocation_strategy();
    let _ = thread_scheduling_strategy();
    let _ = simd_kernel_recommendation();
    let _ = estimate_peak_ops_per_cycle();
    let _ = moe_sizing_recommendation();
    
    true
}
