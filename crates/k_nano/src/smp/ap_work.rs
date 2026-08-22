//! Fila de trabalho para APs — jobs com barreira (ADR-0055 Fase A/B).
//!
//! ## Idle power management
//! APs usam `hlt` (C1) por padrão. Se a CPU suporta MWAIT (CPUID.1:ECX[3]),
//! o loop idle emite `monitor`/`mwait` com hint configurável, permitindo
//! C-states mais profundos (C1E, C2, C6). O wake é atômico: o BSP enfileira
//! um job no slot por-AP e o IPI de reschedule acorda o AP dormindo.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU32, AtomicU8, AtomicUsize, Ordering};

/// A wrapper around UnsafeCell that implements Sync.
/// SAFETY: Access to SLOTS is guarded by atomic HEAD/TAIL indices:
/// BSP writes at TAIL, APs read at HEAD, and the atomic compare-exchange
/// ensures no concurrent read/write of the same slot.
struct SyncCell<T>(UnsafeCell<T>);
unsafe impl<T> Sync for SyncCell<T> {}

pub type ApJobFn = unsafe fn(job_id: usize, worker: usize);

const MAX_JOBS: usize = 64;

#[derive(Clone, Copy)]
struct JobSlot {
    f: Option<ApJobFn>,
    job_id: usize,
}

// SAFETY: SLOTS is only written by BSP (enqueue) and read by APs (try_dequeue).
// HEAD/TAIL atomic indices ensure no concurrent read/write of the same slot.
static SLOTS: SyncCell<[JobSlot; MAX_JOBS]> = SyncCell(UnsafeCell::new([JobSlot {
    f: None,
    job_id: 0,
}; MAX_JOBS]));
static HEAD: AtomicUsize = AtomicUsize::new(0);
static TAIL: AtomicUsize = AtomicUsize::new(0);
static PENDING: AtomicU32 = AtomicU32::new(0);
static DONE: AtomicU32 = AtomicU32::new(0);
/// Epoch da leva atual — APs só executam se epoch == ACTIVE_EPOCH.
static ACTIVE_EPOCH: AtomicU32 = AtomicU32::new(0);

pub fn reset_barrier(pending: u32) {
    DONE.store(0, Ordering::Release);
    PENDING.store(pending, Ordering::Release);
}

pub fn wait_barrier() {
    let target = PENDING.load(Ordering::Acquire);
    while DONE.load(Ordering::Acquire) < target {
        core::hint::spin_loop();
    }
}

pub fn job_done() {
    DONE.fetch_add(1, Ordering::Release);
}

/// Enfileira job (BSP). Retorna false se cheio.
pub fn enqueue(f: ApJobFn, job_id: usize) -> bool {
    let t = TAIL.load(Ordering::Relaxed);
    let h = HEAD.load(Ordering::Acquire);
    if t.wrapping_sub(h) >= MAX_JOBS {
        return false;
    }
    unsafe {
        (*SLOTS.0.get())[t % MAX_JOBS] = JobSlot {
            f: Some(f),
            job_id,
        };
    }
    TAIL.store(t + 1, Ordering::Release);
    // Wake APs: write the monitor flag so MWAIT-idle APs see store-before-IPI.
    MONITOR_FLAG.0.store(MONITOR_FLAG.0.load(Ordering::Relaxed).wrapping_add(1), Ordering::Release);
    true
}

/// AP: tenta pegar um job.
pub fn try_dequeue() -> Option<(ApJobFn, usize)> {
    let h = HEAD.load(Ordering::Relaxed);
    let t = TAIL.load(Ordering::Acquire);
    if h >= t {
        return None;
    }
    if HEAD
        .compare_exchange_weak(h, h + 1, Ordering::SeqCst, Ordering::Relaxed)
        .is_err()
    {
        return None;
    }
    unsafe {
        let slot = &(*SLOTS.0.get())[h % MAX_JOBS];
        let f = slot.f?;
        Some((f, slot.job_id))
    }
}

// ─── MWAIT support ──────────────────────────────────────────────────────────

/// Cache-line aligned flag for the `monitor` instruction. The BSP writes this
/// after enqueuing jobs so the AP wakes from MWAIT even without an IPI.
/// Aligned to 64 bytes (cache line) as required by `monitor`.
#[repr(align(64))]
struct MonitorFlag(core::sync::atomic::AtomicU8);
static MONITOR_FLAG: MonitorFlag = MonitorFlag(core::sync::atomic::AtomicU8::new(0));

/// MWAIT hint currently in use. Written by `set_mwait_hint`.
static MWAIT_HINT: AtomicU8 = AtomicU8::new(0); // 0 = C1

/// Set the MWAIT C-state hint used by APs.
/// 0 = C1 (fast wake, ~same as HLT), 1 = C1E, 2 = C2, 3..6 = C3..C6.
/// Ignored on CPUs without MWAIT (they use `hlt` unconditionally).
pub fn set_mwait_hint(cstate: u8) {
    let hint = cstate.min(6);
    MWAIT_HINT.store(hint, Ordering::Release);
}

/// Returns true if there are pending jobs in the global queue.
/// Used by governor `ondemand_tick` to decide frequency scaling.
pub fn has_pending() -> bool {
    HEAD.load(Ordering::Acquire) < TAIL.load(Ordering::Acquire)
}

/// Emit `monitor` then `mwait` with the current hint.
///
/// # Safety
/// Must only be called if `crate::platform_probe::has_mwait()` is true.
unsafe fn mwait_idle() {
    let flag_ptr = &MONITOR_FLAG.0 as *const _ as u64;
    let hint = MWAIT_HINT.load(Ordering::Relaxed) as u32;
    // EAX[7:4] = C-state hint, EAX[0] = break on interrupt
    let eax = (hint.min(6) as u32) << 4;
    core::arch::asm!(
        "monitor",
        in("rax") flag_ptr,
        in("ecx") 0u32,
        in("edx") 0u32,
        options(nostack, preserves_flags)
    );
    core::arch::asm!(
        "mwait",
        in("eax") eax,
        in("ecx") 0u32,
        options(nostack, preserves_flags)
    );
}

/// Loop de idle do AP: processa jobs enfileirados, tenta steal, senão dorme
/// com `hlt` (C1) ou `mwait` (C1–C6 se a CPU suportar).
///
/// ## Wake
/// O BSP enfileira jobs no slot circular e dispara IPI de reschedule.
/// O AP acorda do `hlt`/`mwait` com o IPI, re-entra no topo do loop e
/// processa o job. Sem IPI, o `mwait` também acorda via `monitor` store
/// (quando o BSP escreve MONITOR_FLAG após enfileirar).
///
/// ## Segurança (AP IDT)
/// Os APs já carregam IDT + TSS (via `ap_load_idt_and_tss`) e habilitam
/// interrupções (`sti`). Portanto `hlt`/`mwait` acordam por IPI sem risco.
/// O trampoline faz `cli`, mas `ap_entry` faz `sti` depois de carregar IDT.
pub fn ap_idle_loop(worker_id: usize) -> ! {
    let use_mwait = crate::platform_probe::has_mwait();

    loop {
        if let Some((f, jid)) = try_dequeue() {
            unsafe { f(jid, worker_id) };
            job_done();
            continue;
        }
        if let Some(task) = super::work_stealing::try_steal_global(worker_id) {
            unsafe { task(core::ptr::null_mut()) };
            continue;
        }
        if let Some(task) = super::work_stealing::global_pool()
            .and_then(|p| p.pop_local(worker_id))
        {
            unsafe { task(core::ptr::null_mut()) };
            continue;
        }

        if use_mwait {
            unsafe { mwait_idle() };
        } else if x86_64::instructions::interrupts::are_enabled() {
            x86_64::instructions::hlt();
        } else {
            core::hint::spin_loop();
        }
    }
}

pub fn bump_epoch() -> u32 {
    ACTIVE_EPOCH.fetch_add(1, Ordering::Release) + 1
}

pub fn clear_queue() {
    HEAD.store(0, Ordering::Release);
    TAIL.store(0, Ordering::Release);
}
