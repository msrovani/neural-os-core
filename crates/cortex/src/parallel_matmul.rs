//! Parallel Matmul — ADR-0055: chunks + barreira + IPI wake nos APs.

use alloc::vec::Vec;
use crate::tensor::Tensor;
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

struct MatmulJobCtx {
    a_ptr: *const f32,
    b_ptr: *const f32,
    c_ptr: *mut f32,
    m: usize,
    n: usize,
    k: usize,
    row_start: usize,
    row_end: usize,
}

static CTX: AtomicPtr<MatmulJobCtx> = AtomicPtr::new(core::ptr::null_mut());
static ROWS_CLAIMED: AtomicUsize = AtomicUsize::new(0);

unsafe fn matmul_worker(_job_id: usize, _worker: usize) {
    let ctx = CTX.load(Ordering::Acquire);
    if ctx.is_null() {
        return;
    }
    let c = &*ctx;
    let tile = k_nano::platform_probe::matmul_tile_rows(c.k, c.n).max(1);
    loop {
        let start = ROWS_CLAIMED.fetch_add(tile, Ordering::Relaxed);
        if start >= c.m {
            break;
        }
        let end = (start + tile).min(c.m);
        for i in start..end {
            for j in 0..c.n {
                let mut sum = 0.0f32;
                for l in 0..c.k {
                    sum += *c.a_ptr.add(i * c.k + l) * *c.b_ptr.add(l * c.n + j);
                }
                *c.c_ptr.add(i * c.n + j) = sum;
            }
        }
    }
}

/// Matmul: se SMP ativo e >1 CPU, distribui linhas; senão single-thread.
pub fn parallel_matmul(a: &Tensor, b: &Tensor) -> Option<Tensor> {
    let (m, k) = a.shape;
    let (k2, n) = b.shape;
    if k != k2 {
        return None;
    }

    let mut c_data = Vec::with_capacity(m * n);
    c_data.resize(m * n, 0.0f32);

    let smp_ok = k_nano::platform_probe::allow_smp()
        && k_nano::smp::ap_pollable()
        && k_nano::smp::ap_entry_count() > 0;

    if !smp_ok || m < 8 {
        // Single-core path
        for i in 0..m {
            for j in 0..n {
                let mut sum = 0.0f32;
                for l in 0..k {
                    sum += a.data[i * k + l] * b.data[l * n + j];
                }
                c_data[i * n + j] = sum;
            }
        }
        return Tensor::from_row_major((m, n), c_data);
    }

    let mut ctx = MatmulJobCtx {
        a_ptr: a.data.as_ptr(),
        b_ptr: b.data.as_ptr(),
        c_ptr: c_data.as_mut_ptr(),
        m,
        n,
        k,
        row_start: 0,
        row_end: m,
    };
    ROWS_CLAIMED.store(0, Ordering::Release);
    CTX.store(&mut ctx as *mut _, Ordering::Release);

    let aps = k_nano::smp::ap_entry_count() as usize;
    let n_workers = aps + 1;
    k_nano::smp::ap_work::clear_queue();
    // Barreira = só APs (BSP sincroniza localmente após seu próprio trabalho)
    k_nano::smp::ap_work::reset_barrier(aps.min(n_workers.saturating_sub(1)) as u32);

    for jid in 0..aps.min(n_workers.saturating_sub(1)) {
        let _ = k_nano::smp::ap_work::enqueue(matmul_worker, jid);
    }

    unsafe {
        k_nano::apic::send_ipi_reschedule();
    }

    unsafe {
        matmul_worker(0, 0);
    }
    if aps > 0 {
        k_nano::smp::ap_work::wait_barrier();
    }

    CTX.store(core::ptr::null_mut(), Ordering::Release);
    Tensor::from_row_major((m, n), c_data)
}

// ─── ADR-0057 WS-B: Ternary matmul paralelo (BitNet) entre P-cores ──────
// Particiona por COLUNAS (n) — assim o decode (m=1, uma linha) também escala.
// Semântica idêntica a `bitnet_avx2::scalar_ternary_matmul`.

use crate::tensor::PackedTernaryTensor;

struct TernaryJobCtx {
    w_ptr: *const PackedTernaryTensor,
    x_ptr: *const f32,
    c_ptr: *mut f32,
    m: usize,
    k: usize,
    n: usize,
}

static T_CTX: AtomicPtr<TernaryJobCtx> = AtomicPtr::new(core::ptr::null_mut());
static COLS_CLAIMED: AtomicUsize = AtomicUsize::new(0);

unsafe fn ternary_worker(_job_id: usize, _worker: usize) {
    let ctx = T_CTX.load(Ordering::Acquire);
    if ctx.is_null() {
        return;
    }
    let c = &*ctx;
    let w = &*c.w_ptr;
    // Tile de colunas (reusa heurística de tile por cache; mínimo 8).
    let tile = k_nano::platform_probe::matmul_tile_rows(c.k, c.n).max(8);
    loop {
        let jstart = COLS_CLAIMED.fetch_add(tile, Ordering::Relaxed);
        if jstart >= c.n {
            break;
        }
        let jend = (jstart + tile).min(c.n);
        for i in 0..c.m {
            for j in jstart..jend {
                let mut sum = 0.0f32;
                for t in 0..c.k {
                    match w.get_weight(t * c.n + j) {
                        1 => sum += *c.x_ptr.add(i * c.k + t),
                        -1 => sum -= *c.x_ptr.add(i * c.k + t),
                        _ => {}
                    }
                }
                *c.c_ptr.add(i * c.n + j) = sum;
            }
        }
    }
}

/// Ternary matmul distribuído entre BSP + APs. Retorna `None` se SMP não está
/// disponível (chamador cai no caminho AVX2/scalar).
pub fn parallel_ternary_matmul(
    weight: &PackedTernaryTensor,
    input: &Tensor,
) -> Option<Tensor> {
    let (k, n) = weight.shape;
    let (m, k2) = input.shape;
    if k != k2 {
        return None;
    }
    // ADR-0057 WS-F: só usa APs quando são workers vivos (`ap_pollable`).
    let smp_ok = k_nano::platform_probe::allow_smp()
        && k_nano::smp::ap_pollable()
        && k_nano::smp::ap_entry_count() > 0;
    if !smp_ok || n < 16 {
        return None;
    }

    let mut result = Tensor::new((m, n));
    let mut ctx = TernaryJobCtx {
        w_ptr: weight as *const _,
        x_ptr: input.data.as_ptr(),
        c_ptr: result.data.as_mut_ptr(),
        m,
        k,
        n,
    };
    COLS_CLAIMED.store(0, Ordering::Release);
    T_CTX.store(&mut ctx as *mut _, Ordering::Release);

    let aps = k_nano::smp::ap_entry_count() as usize;
    let n_workers = aps + 1;
    k_nano::smp::ap_work::clear_queue();
    k_nano::smp::ap_work::reset_barrier(aps.min(n_workers.saturating_sub(1)) as u32);
    for jid in 0..aps.min(n_workers.saturating_sub(1)) {
        let _ = k_nano::smp::ap_work::enqueue(ternary_worker, jid);
    }
    unsafe {
        k_nano::apic::send_ipi_reschedule();
        ternary_worker(0, 0);
    }
    if aps > 0 {
        k_nano::smp::ap_work::wait_barrier();
    }
    T_CTX.store(core::ptr::null_mut(), Ordering::Release);
    Some(result)
}
