//! HAL — Hardware Abstraction Layer (AxiomOS-inspired).
//! Isola arquitetura x86_64 por tras de traits, permitindo
//! futuramente portar para aarch64 (RPi5) e riscv64.
// ponytail: Architecture trait — única abstração cross-arch.
// X86_64 é a impl ativa. aarch64/riscv64 quando portados.
// SystemAgent usa ARCH.reboot() e ARCH.poweroff() nos handlers de shutdown.

use core::sync::atomic::Ordering;

/// Informacoes de deteccao de hardware
#[derive(Debug, Clone)]
pub struct HalInfo {
    pub arch: &'static str,
    pub cpu_count: u64,
    pub ram_bytes: u64,
    pub has_fpu: bool,
    pub has_simd: bool,
}

pub trait Architecture: Send {
    fn name(&self) -> &str;
    fn detect(&self) -> HalInfo;
    fn halt(&self);
    fn reboot(&self);
    fn poweroff(&self);
    fn read_timestamp(&self) -> u64;
}

// ---------------------------------------------------------------------------
// Implementacao x86_64
// ---------------------------------------------------------------------------

pub struct X86_64;

impl Architecture for X86_64 {
    fn name(&self) -> &str { "x86_64" }

    fn detect(&self) -> HalInfo {
        #[cfg(target_arch = "x86_64")]
        {
            let aps = crate::smp::ap_entry_count();
            let mem = crate::memory::global_hardware_context();
            HalInfo {
                arch: "x86_64",
                cpu_count: aps + 1,
                ram_bytes: (mem[1] as u64) * 4096,
                has_fpu: true,
                has_simd: true,
            }
        }
        #[cfg(not(target_arch = "x86_64"))]
        { HalInfo { arch: "unknown", cpu_count: 1, ram_bytes: 0, has_fpu: false, has_simd: false } }
    }

    fn halt(&self) {
        loop { x86_64::instructions::hlt(); }
    }

    fn reboot(&self) {
        // ponytail: shutdown logging dropped — no k_nano::shutdown module
        unsafe { x86_64::instructions::port::Port::new(0x64u16).write(0xFEu8); }
    }

    fn poweroff(&self) {
        // ponytail: shutdown logging dropped — no k_nano::shutdown module
        // QEMU ACPI S5: 0x604 port, value 0x2000 = SLP_TYP=5 (S5) | SLP_EN
        unsafe { core::arch::asm!("out dx, ax", in("dx") 0x604u16, in("ax") 0x2000u16, options(nostack, preserves_flags)); }
    }

    fn read_timestamp(&self) -> u64 {
        #[cfg(target_arch = "x86_64")]
        { crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64 }
        #[cfg(not(target_arch = "x86_64"))]
        { 0 }
    }
}

pub static ARCH: X86_64 = X86_64;
