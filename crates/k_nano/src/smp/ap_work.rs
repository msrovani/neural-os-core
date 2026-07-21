//! Fila de trabalho para APs — jobs com barreira (ADR-0055 Fase A/B).

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

pub type ApJobFn = unsafe fn(job_id: usize, worker: usize);

const MAX_JOBS: usize = 64;

#[derive(Clone, Copy)]
struct JobSlot {
    f: Option<ApJobFn>,
    job_id: usize,
}

static mut SLOTS: [JobSlot; MAX_JOBS] = [JobSlot {
    f: None,
    job_id: 0,
}; MAX_JOBS];
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
        SLOTS[t % MAX_JOBS] = JobSlot {
            f: Some(f),
            job_id,
        };
    }
    TAIL.store(t + 1, Ordering::Release);
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
        let slot = &SLOTS[h % MAX_JOBS];
        let f = slot.f?;
        Some((f, slot.job_id))
    }
}

/// Loop de idle do AP: processa jobs enfileirados, tenta steal, senão `hlt`.
///
/// ADR-0057 WS-F: os APs sobem **sem IDT próprio** e com IF=0 (o trampoline faz
/// `cli` e `ap_entry` não faz `lidt`/`sti`). Enquanto assim, um AP que dorme com
/// `hlt` só pode ser acordado por trabalho já enfileirado antes do `hlt`, e o
/// `parallel_*` (WS-B) por isso é **gated** por `ap_pollable()` (hoje `false`):
/// o BSP faz o matmul (AVX2/scalar) e nenhum AP é enfileirado → sem deadlock.
///
/// Para transformar os APs em workers vivos sob demanda (F1-full) é preciso:
/// IDT compartilhada + LAPIC habilitado + handler do reschedule-IPI + `wake_aps`.
/// Isso mexe em interrupções por-core (risco de #DF) e exige **validação em HW**
/// (residual WS-F). `hlt` mantém o wake sequencial confiável e baixo custo no TCG.
pub fn ap_idle_loop(worker_id: usize) -> ! {
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
        x86_64::instructions::hlt();
    }
}

pub fn bump_epoch() -> u32 {
    ACTIVE_EPOCH.fetch_add(1, Ordering::Release) + 1
}

pub fn clear_queue() {
    HEAD.store(0, Ordering::Release);
    TAIL.store(0, Ordering::Release);
}
