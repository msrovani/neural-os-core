//! HAL — Hardware Abstraction Layer (AxiomOS-inspired).
//! Isola arquitetura x86_64 por tras de traits, permitindo
//! futuramente portar para aarch64 (RPi5) e riscv64.

use alloc::string::String;

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
        unsafe { x86_64::instructions::port::Port::new(0x64u16).write(0xFEu8); }
    }

    fn poweroff(&self) {
        // ACPI shutdown
        unsafe { core::arch::asm!("out dx, al", in("dx") 0x604u16, in("al") 0x10u8, options(nostack)); }
    }

    fn read_timestamp(&self) -> u64 {
        #[cfg(target_arch = "x86_64")]
        { crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64 }
        #[cfg(not(target_arch = "x86_64"))]
        { 0 }
    }
}

pub static ARCH: X86_64 = X86_64;
