//! LAPIC TSC-Deadline Mode — latência sub-nanosegundo.
//!
//! Substitui o modo periódico do LAPIC Timer (que usa o barramento APIC para
//! decrementar o counter) pelo TSC-Deadline: o disparo da interrupção é
//! programado diretamente no contador de ciclos da CPU (`IA32_TSC_DEADLINE`).
//!
//! # Vantagens sobre o modo periódico
//! - **Zero latência de barramento APIC**: o timer é comparado localmente com
//!   `rdtsc`, sem acesso ao registrador `INIT_COUNT` a cada tick.
//! - **Precisão de 1 ciclo**: o deadline é em ticks de TSC, não em divisões
//!   do APIC timer (que perdem ciclos com dividers).
//! - **Skippable**: se a CPU está busy, o deadline é reprogramado no próximo
//!   tick — sem acumulação de interrupções pendentes (tick coalescing).
//!
//! # CPUID Detection
//! - CPUID.01H:ECX[24] = TSC-Deadline mode suportado
//! - MSR `IA32_TSC_DEADLINE` = 0x6E0
//!
//! # Fallback
//! Se o hardware não suporta TSC-Deadline, mantém o modo periódico do APIC.
//! O `init_tsc_deadline` retorna `true` se o modo foi ativado.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// MSR address for IA32_TSC_DEADLINE.
pub const MSR_IA32_TSC_DEADLINE: u32 = 0x6E0;

/// Bit position in CR4 for TSD (Time Stamp Disable) — não confundir com TSC-Deadline.
/// CPUID.01H:ECX[24] indica suporte ao modo TSC-Deadline no LVT Timer.
const CPUID_TSC_DEADLINE_BIT: u32 = 24;

/// LAPIC LVT Timer register offset.
const LAPIC_LVT_TIMER: u64 = 0x320;

/// Timer mode bits in LVT Timer: bits [18:17].
/// 00 = One-shot, 01 = Periodic, 10 = TSC-Deadline.
const LVT_TIMER_TSC_DEADLINE: u32 = 10 << 17;

/// Global flag: TSC-Deadline ativo.
static TSC_DEADLINE_ACTIVE: AtomicBool = AtomicBool::new(false);

/// TSC-Deadline calibration: ciclos por tick (para converter intervalos).
/// Calculado durante init comparando TSC delta entre deadlines.
static CYCLES_PER_TICK: AtomicU64 = AtomicU64::new(0);

/// Detecta suporte a TSC-Deadline via CPUID.01H:ECX[24].
#[cfg(target_arch = "x86_64")]
pub fn has_tsc_deadline_support() -> bool {
    unsafe {
        let cpuid = core::arch::x86_64::__cpuid(1);
        (cpuid.ecx >> CPUID_TSC_DEADLINE_BIT) & 1 == 1
    }
}

#[cfg(not(target_arch = "x86_64"))]
pub fn has_tsc_deadline_support() -> bool {
    false
}

/// Retorna true se TSC-Deadline está ativo.
#[inline]
pub fn is_active() -> bool {
    TSC_DEADLINE_ACTIVE.load(Ordering::Acquire)
}

/// Retorna ciclos por tick (para conversões).
#[inline]
pub fn cycles_per_tick() -> u64 {
    CYCLES_PER_TICK.load(Ordering::Relaxed)
}

/// Programa o próximo deadline em ciclos de TSC a partir de agora.
///
/// # Safety
/// - Requer `wrmsr` ring 0
/// - MSR 0x6E0 só pode ser escrito se CPUID.01H:ECX[24] = 1
/// - O APIC LVT Timer deve estar em modo TSC-Deadline (bits 18:17 = 10)
///
/// # Exemplo
/// ```no_run
/// // Dispara interrupção em 1.000.000 ciclos (~0.3ms a 3GHz)
/// unsafe { set_tsc_deadline(1_000_000); }
/// ```
#[inline(always)]
pub unsafe fn set_tsc_deadline(delta_cycles: u64) {
    // LFENCE garante que todas as instruções antes do RDTSC foram completadas
    // antes de calcular o deadline. Sem LFENCE, a CPU pode speculative-executar
    // o RDTSC antes de stores anteriores serem visíveis.
    core::arch::x86_64::_mm_lfence();
    let target = crate::tsc::rdtsc().wrapping_add(delta_cycles);
    wrmsr(MSR_IA32_TSC_DEADLINE, target);
}

/// Limpa o deadline (desativa a próxima interrupção TSC-Deadline).
///
/// Útil no shutdown ou quando o timer deve ser silenciado temporariamente.
#[inline(always)]
pub unsafe fn clear_tsc_deadline() {
    wrmsr(MSR_IA32_TSC_DEADLINE, 0);
}

/// Lê o valor atual do deadline (para debug/diagnóstico).
#[inline(always)]
pub fn read_tsc_deadline() -> u64 {
    unsafe { rdmsr(MSR_IA32_TSC_DEADLINE) }
}

/// Inicializa o modo TSC-Deadline no LAPIC Timer.
///
/// 1. Verifica suporte via CPUID
/// 2. Programa LVT Timer com mode=10 (TSC-Deadline) ao invés de mode=01 (Periodic)
/// 3. Não escreve INIT_COUNT (não usado em TSC-Deadline)
///
/// Retorna `true` se TSC-Deadline foi ativado, `false` se fallback para periódico.
pub unsafe fn init_tsc_deadline(
    lapic_base: u64,
    using_x2apic: bool,
    periodic_fallback: impl FnOnce(),
) -> bool {
    if !has_tsc_deadline_support() {
        crate::slog_nano!(
            "APIC", "warn",
            "TSC-Deadline NÃO suportado (CPUID.01H:ECX[24]=0) — usando modo periódico"
        );
        periodic_fallback();
        return false;
    }

    // Detecta se o hypervisor esconde TSC-Deadline (WHPX/TCG podem não suportar)
    if matches!(crate::platform_probe::hypervisor(), crate::platform_probe::HypervisorKind::Tcg | crate::platform_probe::HypervisorKind::QemuGeneric) {
        crate::slog_nano!(
            "APIC", "warn",
            "TSC-Deadline: hypervisor emulado detectado — usando modo periódico (fallback seguro)"
        );
        periodic_fallback();
        return false;
    }

    // LVT Timer: vector=32 | delivery_mode=0 (fixed) | mode=10 (TSC-Deadline)
    // Bit 18:17 = 10 para TSC-Deadline
    let lvt_value = 32u32 | LVT_TIMER_TSC_DEADLINE;
    write_lapic_reg(lapic_base, using_x2apic, LAPIC_LVT_TIMER, lvt_value);

    // IMPORTANTE: TSC-Deadline NÃO usa INIT_COUNT
    // O timer dispara quando TSC >= IA32_TSC_DEADLINE
    // Não escreve INIT_COUNT nem DIVIDE_CONFIG (divisor irrelevante)

    TSC_DEADLINE_ACTIVE.store(true, Ordering::Release);

    crate::slog_nano!(
        "APIC", "info",
        "TSC-Deadline mode ATIVADO — vetor 32, MSR 0x6E0, sem barramento APIC"
    );
    true
}

/// Reagenda o timer periódico em TSC-Deadline mode.
///
/// Chama-se no handler de interrupção do timer (vetor 32) para
/// programar o próximo tick.
///
/// # Safety
/// Deve ser chamado apenas do handler de timer com interrupts desabilitados.
#[inline(always)]
pub unsafe fn rearm_next_tick(base_cycles: u64) {
    set_tsc_deadline(base_cycles);
}

/// Helper: escreve no registrador LAPIC (MMIO ou MSR/x2APIC).
#[inline(always)]
unsafe fn write_lapic_reg(base: u64, using_x2apic: bool, reg: u64, value: u32) {
    if using_x2apic {
        let msr_addr = 0x800 + (reg >> 4) as u32;
        let mut msr = x86_64::registers::model_specific::Msr::new(msr_addr);
        msr.write(value as u64);
    } else {
        core::ptr::write_volatile((base + reg) as *mut u32, value);
    }
}

/// Raw MSR write (ring 0 only).
///
/// # Safety
/// MSR must be valid and CPU must support WRMSR for this address.
#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn wrmsr(msr: u32, value: u64) {
    core::arch::asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") value as u32,
        in("edx") (value >> 32) as u32,
        options(nostack, preserves_flags),
    );
}

#[cfg(not(target_arch = "x86_64"))]
unsafe fn wrmsr(_msr: u32, _value: u64) {}

/// Raw MSR read (ring 0 only).
#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn rdmsr(msr: u32) -> u64 {
    let lo: u32;
    let hi: u32;
    core::arch::asm!(
        "rdmsr",
        in("ecx") msr,
        out("eax") lo,
        out("edx") hi,
        options(nostack, preserves_flags),
    );
    ((hi as u64) << 32) | (lo as u64)
}

#[cfg(not(target_arch = "x86_64"))]
unsafe fn rdmsr(_msr: u32) -> u64 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msr_address_is_correct() {
        assert_eq!(MSR_IA32_TSC_DEADLINE, 0x6E0);
    }

    #[test]
    fn lvt_mode_bits() {
        // TSC-Deadline = mode 10 = bits 18:17
        assert_eq!(LVT_TIMER_TSC_DEADLINE, 10 << 17);
        // Periodic = mode 01 = bits 18:17
        let periodic = 01u32 << 17;
        assert_ne!(periodic, LVT_TIMER_TSC_DEADLINE);
    }

    #[test]
    fn cpuid_bit_position() {
        assert_eq!(CPUID_TSC_DEADLINE_BIT, 24);
    }
}
