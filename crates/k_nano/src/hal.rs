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
        // EC de notebook ignora o 8042 sem poll de IBF: espera o input buffer
        // vazio (bit1 de 0x64) com budget TSC (~10ms) antes de cada 0xFE;
        // poucas retries com janela entre elas (SESSION_354 honesty).
        unsafe {
            for _ in 0..5 {
                let t0 = crate::tsc::now_us();
                let mut empty = false;
                while crate::tsc::now_us().wrapping_sub(t0) < 10_000 {
                    let st: u8;
                    core::arch::asm!("in al, dx", out("al") st, in("dx") 0x64u16, options(nostack, preserves_flags));
                    if st & 0x02 == 0 {
                        empty = true;
                        break;
                    }
                    core::hint::spin_loop();
                }
                if !empty {
                    break;
                }
                core::arch::asm!("out dx, al", in("dx") 0x64u16, in("al") 0xFEu8, options(nostack, preserves_flags));
                // Se o EC honrar, não voltamos; janela curta antes do retry.
                crate::tsc::sleep_us(10_000);
            }
        }
    }

    fn poweroff(&self) {
        // 0x604 é QEMU-only (porta ACPI S5 do QEMU/bochs). Em HW real a porta
        // pode nem existir → out cego sem efeito. Gate por hypervisor real.
        let sandbox = crate::platform_probe::probe_done()
            && crate::platform_probe::hypervisor() != crate::platform_probe::HypervisorKind::None;
        if !sandbox {
            return;
        }
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

/// Reset via porta CF9 do chipset (notebooks Intel honram CF9, não 8042):
/// 0x06 (init) depois 0x0E (full reset), com janela TSC entre eles.
pub fn cf9_reset() {
    unsafe {
        core::arch::asm!("out dx, al", in("dx") 0xCF9u16, in("al") 0x06u8, options(nostack, preserves_flags));
    }
    crate::tsc::sleep_us(10_000);
    unsafe {
        core::arch::asm!("out dx, al", in("dx") 0xCF9u16, in("al") 0x0Eu8, options(nostack, preserves_flags));
    }
    crate::tsc::sleep_us(10_000);
}

/// Triple-fault (IDT inválida + int3 → #GP sem IDT → #DF → reset da CPU).
/// Último recurso de reboot. Retorna `()` de propósito: se o HW ignorar e a
/// execução continuar, o caller faz park observável (nunca `loop{hlt}` mudo).
pub fn triple_fault() {
    unsafe {
        let bad: [u8; 10] = [0; 10]; // limite=0/base=0 → qualquer int faulta
        core::arch::asm!("lidt [{}]", in(reg) bad.as_ptr(), options(nostack, preserves_flags));
        core::arch::asm!("int3", options(nostack));
    }
}
