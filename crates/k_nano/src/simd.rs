//! SIMD/FPU enable (CR0/CR4) + OSXSAVE/XCR0 quando FeatureGate permite AVX (ADR-0055).
//! ADR-0061: estendido para AVX-512 (XCR0 bits 5=opmask, 6=ZMM-high, 7=hi16-ZMM).

use core::arch::x86_64::__cpuid;
use x86_64::registers::control::{Cr0, Cr0Flags, Cr4, Cr4Flags};

/// Habilita SSE (sempre) e, se FeatureGate permite, AVX e AVX-512 via OSXSAVE + XCR0.
pub fn enable_simd() {
    let allow_avx = crate::platform_probe::allow_avx2()
        || crate::platform_probe::cpu_features().avx;
    let allow_avx512 = crate::platform_probe::allow_avx512();
    enable_simd_ex(allow_avx, allow_avx512);
}

/// Habilita SSE sempre; AVX se `allow_avx`; AVX-512 se `allow_avx512`.
///
/// XCR0 bits:
/// - 0 = x87, 1 = SSE, 2 = AVX (YMM)
/// - 5 = opmask registers (k0-k7)
/// - 6 = ZMM high 256 bits
/// - 7 = hi16 ZMM registers
pub fn enable_simd_ex(allow_avx: bool, allow_avx512: bool) {
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
            if allow_avx {
                // CR4.OSXSAVE = bit 18 (x86_64 0.14 may lack named flag)
                *flags |= Cr4Flags::from_bits_truncate(1 << 18);
            }
        });
    }
    if allow_avx {
        // XCR0: bit0=x87, bit1=SSE, bit2=AVX
        // ADR-0061: se AVX-512, habilita bits 5 (opmask), 6 (ZMM-hi256), 7 (hi16-ZMM)
        unsafe {
            let mut eax: u32;
            let mut edx: u32;
            core::arch::asm!(
                "xgetbv",
                inout("ecx") 0u32 => _,
                out("eax") eax,
                out("edx") edx,
                options(nostack, preserves_flags)
            );
            eax |= 0x07; // bits 0,1,2: x87, SSE, AVX
            if allow_avx512 {
                eax |= 0xE0; // bits 5,6,7: opmask, ZMM-hi256, hi16-ZMM
            }
            core::arch::asm!(
                "xsetbv",
                in("ecx") 0u32,
                in("eax") eax,
                in("edx") edx,
                options(nostack, preserves_flags)
            );
        }
    }
}

pub fn has_whpx() -> bool {
    crate::platform_probe::hypervisor() == crate::platform_probe::HypervisorKind::MicrosoftHv
}

pub fn has_kvm() -> bool {
    matches!(
        crate::platform_probe::hypervisor(),
        crate::platform_probe::HypervisorKind::Kvm
    )
}

#[allow(dead_code)]
fn _cpuid_vendor_leaf() -> u32 {
    unsafe { __cpuid(0x40000000).eax }
}
