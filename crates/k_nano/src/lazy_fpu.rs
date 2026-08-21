//! Lazy FPU/SIMD context switch via CR0.TS (Task Switched bit).
//!
//! # Problema resolvido
//! Em sistemas com muitas trocas de contexto, salvar/restaurar o estado
//! FPU/SIMD (512 bytes para FXSAVE, ou até 2688 bytes para XSAVE + AVX-512)
//! a cada troca é desperdício se a maioria dos tasks não usa FPU/SIMD.
//!
//! # Solução (padrão x86_64)
//! 1. **Switch-out**: setar CR0.TS (bit 3) — qualquer instrução FPU/SIMD
//!    depois disso dispara #NM (Device Not Available, vector 7).
//! 2. **#NM handler**: salvar o estado FPU do task anterior, restaurar o do
//!    task novo, e limpar CR0.TS para que o task possa usar FPU novamente.
//! 3. **Gate**: só funciona em bare-metal (`cfg(target_os = "none")`);
//!    em host (testes) é stub seguro.
//!
//! # Segurança
//! - CR0.TS é preservado em IRET — cada task tem seu próprio TS state.
//! - O handler #NM NÃO deve usar instruções FPU/SIMD (senão loop infinito).
//! - FXSAVE é suficiente para SSE/AVX (512 bytes). XSAVE é necessário
//!   apenas para AVX-512/AMX (extensão futura).

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// CR0 bit 3 = TS (Task Switched).
const CR0_TS_BIT: u64 = 1 << 3;

/// Tamanho do estado FPU/SIMD salvo por FXSAVE (512 bytes, alinhado 16).
const FPU_STATE_SIZE: usize = 512;

/// Número máximo de contextos FPU simultâneos (0 = kernel, 1..N = tasks Ring3).
const MAX_FPU_CONTEXTS: usize = 16;

/// Estado FPU/SIMD alinhado a 16 bytes (requisito do FXSAVE/FXRSTOR).
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct FpuState {
    pub data: [u8; FPU_STATE_SIZE],
}

impl FpuState {
    pub const fn zero() -> Self {
        Self { data: [0u8; FPU_STATE_SIZE] }
    }
}

/// Slot de contexto FPU — estado + validade.
#[derive(Copy, Clone)]
struct FpuSlot {
    state: FpuState,
    valid: bool,
}

impl FpuSlot {
    const fn new() -> Self {
        Self { state: FpuState::zero(), valid: false }
    }
}

/// Armazenamento de contextos FPU (estático, sem heap).
/// Acesso via `fpu_slots()` que retorna referência segura.
static mut FPU_SLOTS: [FpuSlot; MAX_FPU_CONTEXTS] = [FpuSlot::new(); MAX_FPU_CONTEXTS];

/// Acesso seguro ao array de slots — callers devem garantir non-overlap.
/// Em prática, cada task acessa apenas o seu slot (via ctx_id único).
#[inline(always)]
fn fpu_slots() -> &'static mut [FpuSlot; MAX_FPU_CONTEXTS] {
    // SAFETY: FPU_SLOTS é acessado com ctx_id único por task.
    // O scheduler garante que dois tasks nunca acessam o mesmo slot simultaneamente.
    unsafe { &mut *core::ptr::addr_of_mut!(FPU_SLOTS) }
}

/// ID do contexto FPU atualmente carregado no registrador da CPU.
static CURRENT_FPU_OWNER: AtomicUsize = AtomicUsize::new(usize::MAX);

/// CR0.TS está habilitado? (lazy mode ativo).
static TS_ENABLED: AtomicBool = AtomicBool::new(false);

/// Contador de #NM traps (para telemetria/debug).
static NM_TRAP_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Contador de saves realizados.
static SAVE_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Contador de restores realizados.
static RESTORE_COUNT: AtomicUsize = AtomicUsize::new(0);

// ─── CR0.TS helpers (bare-metal only) ──────────────────────────────────

#[cfg(target_os = "none")]
#[inline(always)]
unsafe fn read_cr0() -> u64 {
    let val: u64;
    core::arch::asm!("mov {}, cr0", out(reg) val, options(nostack, preserves_flags));
    val
}

#[cfg(target_os = "none")]
#[inline(always)]
unsafe fn write_cr0(val: u64) {
    core::arch::asm!("mov cr0, {}", in(reg) val, options(nostack, preserves_flags));
}

#[cfg(not(target_os = "none"))]
unsafe fn read_cr0() -> u64 { 0 }

#[cfg(not(target_os = "none"))]
unsafe fn write_cr0(_val: u64) {}

/// Seta CR0.TS — qualquer instrução FPU/SIMD dispara #NM.
///
/// # Safety
/// - Deve ser chamado com interrupts habilitados
/// - Não deve ser chamado dentro de um handler #NM
#[cfg(target_os = "none")]
pub unsafe fn set_cr0_ts() {
    let cr0 = read_cr0();
    if cr0 & CR0_TS_BIT == 0 {
        write_cr0(cr0 | CR0_TS_BIT);
        TS_ENABLED.store(true, Ordering::Release);
    }
}

/// Limpa CR0.TS — FPU/SIMD pode ser usado novamente.
///
/// # Safety
/// - Só deve ser chamado dentro do #NM handler
#[cfg(target_os = "none")]
pub unsafe fn clear_cr0_ts() {
    let cr0 = read_cr0();
    if cr0 & CR0_TS_BIT != 0 {
        write_cr0(cr0 & !CR0_TS_BIT);
        TS_ENABLED.store(false, Ordering::Release);
    }
}

#[cfg(not(target_os = "none"))]
pub unsafe fn set_cr0_ts() {}

#[cfg(not(target_os = "none"))]
pub unsafe fn clear_cr0_ts() {}

// ─── FXSAVE/FXRSTOR (bare-metal only) ──────────────────────────────────

/// Salva o estado FPU/SIMD via FXSAVE.
///
/// # Safety
/// - `dst` deve ser alinhado a 16 bytes e ter pelo menos 512 bytes
#[cfg(target_os = "none")]
#[inline(always)]
unsafe fn fxsave(dst: *mut u8) {
    core::arch::asm!("fxsave [{}]", in(reg) dst, options(nostack));
}

/// Restaura o estado FPU/SIMD via FXRSTOR.
///
/// # Safety
/// - `src` deve ser alinhado a 16 bytes e conter um estado FXSAVE válido
/// - CR0.TS NÃO deve estar setado
#[cfg(target_os = "none")]
#[inline(always)]
unsafe fn fxrstor(src: *const u8) {
    core::arch::asm!("fxrstor [{}]", in(reg) src, options(nostack));
}

#[cfg(not(target_os = "none"))]
unsafe fn fxsave(_dst: *mut u8) {}

#[cfg(not(target_os = "none"))]
unsafe fn fxrstor(_src: *const u8) {}

// ─── API pública ───────────────────────────────────────────────────────

/// Inicializa o lazy FPU subsystem. Chamar uma vez no boot, após `enable_simd()`.
pub fn init_lazy_fpu() {
    CURRENT_FPU_OWNER.store(0, Ordering::Release);
    crate::slog_bin!(
        "FPU", "info",
        "lazy FPU init: {} slots, {} B/slot, TS=off (setado no 1º switch)",
        MAX_FPU_CONTEXTS, FPU_STATE_SIZE
    );
}

#[inline]
pub fn is_ts_active() -> bool {
    TS_ENABLED.load(Ordering::Acquire)
}

#[inline]
pub fn nm_trap_count() -> usize {
    NM_TRAP_COUNT.load(Ordering::Relaxed)
}

#[inline]
pub fn save_count() -> usize {
    SAVE_COUNT.load(Ordering::Relaxed)
}

#[inline]
pub fn restore_count() -> usize {
    RESTORE_COUNT.load(Ordering::Relaxed)
}

pub fn lazy_fpu_stats() -> &'static str {
    if is_ts_active() { "lazy_fpu=active" } else { "lazy_fpu=off" }
}

/// Chamado no switch-out: salva o estado FPU do contexto atual e seta CR0.TS.
///
/// # Safety
/// - Deve ser chamado com interrupts habilitados
/// - `new_ctx_id` é o ID do contexto que será ativado
pub unsafe fn switch_out_fpu(new_ctx_id: usize) {
    let old_ctx_id = CURRENT_FPU_OWNER.load(Ordering::Relaxed);

    // Salva o estado do contexto antigo se TS estava limpo (FPU foi usado)
    if old_ctx_id < MAX_FPU_CONTEXTS && !TS_ENABLED.load(Ordering::Acquire) {
        let slot = &mut fpu_slots()[old_ctx_id];
        fxsave(slot.state.data.as_mut_ptr());
        slot.valid = true;
        SAVE_COUNT.fetch_add(1, Ordering::Relaxed);
    }

    // Seta CR0.TS para o novo contexto (lazy: restaura no #NM)
    set_cr0_ts();
    CURRENT_FPU_OWNER.store(new_ctx_id, Ordering::Release);
}

/// Chamado pelo #NM handler: restaura o estado FPU e limpa CR0.TS.
///
/// # Safety
/// - Deve ser chamado APENAS dentro do handler #NM (interrupts desabilitados)
pub unsafe fn restore_on_nm() {
    NM_TRAP_COUNT.fetch_add(1, Ordering::Relaxed);

    let ctx_id = CURRENT_FPU_OWNER.load(Ordering::Relaxed);

    // Restaura o estado FPU se existe um snapshot válido
    if ctx_id < MAX_FPU_CONTEXTS {
        let slot = &fpu_slots()[ctx_id];
        if slot.valid {
            fxrstor(slot.state.data.as_ptr());
            RESTORE_COUNT.fetch_add(1, Ordering::Relaxed);
        }
    }

    // Limpa CR0.TS — o task pode usar FPU/SIMD novamente
    clear_cr0_ts();
}

/// Aloca um slot de contexto FPU para um novo task.
pub fn alloc_fpu_context() -> Option<usize> {
    let slots = fpu_slots();
    for i in 1..MAX_FPU_CONTEXTS {
        if !slots[i].valid {
            slots[i].valid = true;
            return Some(i);
        }
    }
    None
}

/// Libera um slot de contexto FPU.
pub fn free_fpu_context(id: usize) {
    if id > 0 && id < MAX_FPU_CONTEXTS {
        let slots = fpu_slots();
        slots[id].valid = false;
        slots[id].state = FpuState::zero();
    }
}

/// Marca um slot como "não usado" (TS será setado no próximo switch-out).
pub fn invalidate_fpu_context(id: usize) {
    if id < MAX_FPU_CONTEXTS {
        fpu_slots()[id].valid = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fpu_state_alignment() {
        assert_eq!(core::mem::align_of::<FpuState>(), 16, "FXSAVE requer 16-byte alignment");
        assert_eq!(core::mem::size_of::<FpuState>(), 512, "FXSAVE state = 512 bytes");
    }

    #[test]
    fn alloc_and_free_fpu_context() {
        let id = alloc_fpu_context().expect("deveria alocar slot 1");
        assert_eq!(id, 1);
        assert!(fpu_slots()[1].valid);

        free_fpu_context(id);
        assert!(!fpu_slots()[1].valid);
    }

    #[test]
    fn alloc_fills_sequential() {
        let ids: alloc::vec::Vec<usize> = (0..MAX_FPU_CONTEXTS - 1)
            .filter_map(|_| alloc_fpu_context())
            .collect();
        assert_eq!(ids.len(), MAX_FPU_CONTEXTS - 1);

        // Esgotado
        assert!(alloc_fpu_context().is_none());

        for id in &ids {
            free_fpu_context(*id);
        }
    }

    #[test]
    fn ts_flags_correct() {
        assert_eq!(CR0_TS_BIT, 1 << 3, "CR0.TS é bit 3");
    }

    #[test]
    fn lazy_fpu_stats_format() {
        let s = lazy_fpu_stats();
        assert!(s.starts_with("lazy_fpu="));
    }

    #[test]
    fn free_slot_zero_is_noop() {
        let was_valid = fpu_slots()[0].valid;
        free_fpu_context(0);
        assert_eq!(fpu_slots()[0].valid, was_valid, "free(0) não deve mudar kernel slot");
    }

    #[test]
    fn invalidate_clears_validity() {
        let id = alloc_fpu_context().expect("alloc");
        fpu_slots()[id].valid = true;
        invalidate_fpu_context(id);
        assert!(!fpu_slots()[id].valid);
        free_fpu_context(id);
    }
}
