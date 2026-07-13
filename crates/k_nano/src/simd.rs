//! SIMD/FPU enable (CR0/CR4) + CPU feature detection.

use core::arch::x86_64::__cpuid;
use x86_64::registers::control::{Cr0, Cr0Flags, Cr4, Cr4Flags};

pub fn enable_simd() {
    unsafe {
        Cr0::update(|flags| {
            flags.remove(Cr0Flags::EMULATE_COPROCESSOR);
            flags.insert(Cr0Flags::MONITOR_COPROCESSOR);
            flags.insert(Cr0Flags::NUMERIC_ERROR);
        });
    }
    unsafe {
        Cr4::update(|flags| {
            flags.insert(Cr4Flags::OSFXSR);
            flags.insert(Cr4Flags::OSXMMEXCPT_ENABLE);
        });
    }
}

/// Detecta suporte a WHPX (Windows Hypervisor Platform) via CPUID.
/// WHPX expoe a hypervisor leaf 0x40000000 com vendor "Microsoft Hv"
pub fn has_whpx() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        unsafe {
            let leaf = __cpuid(0x40000000);
            // Verifica vendor string: "Microsoft Hv" nos registradores EBX:ECX:EDX
            let vendor: [u8; 12] = [
                (leaf.ebx >> 0) as u8, (leaf.ebx >> 8) as u8, (leaf.ebx >> 16) as u8, (leaf.ebx >> 24) as u8,
                (leaf.ecx >> 0) as u8, (leaf.ecx >> 8) as u8, (leaf.ecx >> 16) as u8, (leaf.ecx >> 24) as u8,
                (leaf.edx >> 0) as u8, (leaf.edx >> 8) as u8, (leaf.edx >> 16) as u8, (leaf.edx >> 24) as u8,
            ];
            &vendor == b"Microsoft Hv"
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    { false }
}

/// Detecta suporte a KVM (Linux KVM) via CPUID leaf 0x40000000
pub fn has_kvm() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        unsafe {
            let leaf = __cpuid(0x40000000);
            let vendor: [u8; 12] = [
                (leaf.ebx >> 0) as u8, (leaf.ebx >> 8) as u8, (leaf.ebx >> 16) as u8, (leaf.ebx >> 24) as u8,
                (leaf.ecx >> 0) as u8, (leaf.ecx >> 8) as u8, (leaf.ecx >> 16) as u8, (leaf.ecx >> 24) as u8,
                (leaf.edx >> 0) as u8, (leaf.edx >> 8) as u8, (leaf.edx >> 16) as u8, (leaf.edx >> 24) as u8,
            ];
            &vendor == b"KVMKVMKVM\0\0\0"
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    { false }
}
