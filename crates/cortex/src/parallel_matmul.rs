//! Parallel Matmul — ADR-0055: chunks + barreira + IPI wake nos APs.

use crate::tensor::Tensor;
use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, AtomicUsize, Ordering};

/// Deadline do barrier SMP de um matmul. Shapes reais do Falcon3-1B levam
/// ≤150ms/workers=6 (logs s419); 5s já é margem ~30× — acima disso é wedge,
/// não lentidão. (60s fazia o BSP queimar CPU num spin mudo: "CPU 100%, log
/// morto, sem #PF" — o stall pós-teto da s418.)
const MATMUL_BARRIER_TIMEOUT_US: u64 = 5_000_000;

/// Exclusão mútua do dispatch SMP (SESSION_419, stall pós-teto).
///
/// CTX/ROWS_CLAIMED/COLS_CLAIMED e a barreira (PENDING/DONE em ap_work) são
/// GLOBALS: dois dispatches concorrentes de cores distintos (BSP processa o
/// reply pós-`done` enquanto o AP roda slice de prefill) cruzam
/// `clear_queue`+`reset_barrier` no meio do matmul alheio → DONE zerado →
/// `pending=5 done=0` eterno → spin silencioso de 60s → timeout → dump.
///
/// `try_lock`: quem chega 2º NÃO espera — cai no caminho single-core
/// (correto e honesto, só mais lento). Nunca bloquear o scheduler.
static SMP_MM_BUSY: AtomicBool = AtomicBool::new(false);

struct SmpMmGuard;
impl SmpMmGuard {
    #[inline]
    fn try_acquire() -> bool {
        SMP_MM_BUSY
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }
}
impl Drop for SmpMmGuard {
    #[inline]
    fn drop(&mut self) {
        SMP_MM_BUSY.store(false, Ordering::Release);
    }
}

/// Instrumentação s-prefill: 1 log enter/exit a cada 20 chamadas de cada caminho.
static MATMUL_LOG_CALLS: AtomicU64 = AtomicU64::new(0);
static TERNARY_LOG_CALLS: AtomicU64 = AtomicU64::new(0);
/// Fase 2 (s-prefill): separa compute (worker) de sync (barreira) no SMP.
static TERN_MAX_WORKER_US: AtomicU64 = AtomicU64::new(0);
static TERN_WORKERS_DONE: AtomicU64 = AtomicU64::new(0);

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
    if k != k2 || !a.is_valid() || !b.is_valid() {
        return None;
    }
    let Some(need) = m.checked_mul(n) else {
        return None;
    };
    // SESSION_351: nunca `with_capacity(m*n)` cru (wrap + capacity overflow).
    let mut c_data = crate::tensor::f32_zeros_2d(m, n);
    if c_data.len() != need {
        return None;
    }

    let smp_ok = k_nano::platform_probe::allow_smp()
        && k_nano::smp::ap_pollable()
        && k_nano::smp::ap_entry_count() > 0
        // SESSION_419: dispatch já em curso em outro core → single-core
        // (não enfileira jobs num barrier que vai ser resetado pelo dono).
        && SmpMmGuard::try_acquire();
    let _smp_guard = if smp_ok { Some(SmpMmGuard) } else { None };

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

    let mut ctx = alloc::boxed::Box::new(MatmulJobCtx {
        a_ptr: a.data.as_ptr(),
        b_ptr: b.data.as_ptr(),
        c_ptr: c_data.as_mut_ptr(),
        m,
        n,
        k,
        row_start: 0,
        row_end: m,
    });
    ROWS_CLAIMED.store(0, Ordering::Release);
    CTX.store(&mut *ctx as *mut _, Ordering::Release);

    let aps = k_nano::smp::ap_entry_count() as usize;
    let n_workers = aps + 1;
    let log_it = MATMUL_LOG_CALLS.fetch_add(1, Ordering::Relaxed) % 20 == 0;
    if log_it {
        k_nano::slog_cortex!("cortex", "warn", "matmul enter m={} k={} n={} aps={}", m, k, n, aps);
    }
    // T1 (s413): TSC DEPOIS do log (mesmo artefato do caminho ternario).
    let t_mm0 = k_nano::tsc::now_us();
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
    if aps > 0 && !k_nano::smp::ap_work::wait_barrier_timeout(MATMUL_BARRIER_TIMEOUT_US) {
        CTX.store(core::ptr::null_mut(), Ordering::Release);
        k_nano::slog_cortex!(
            "cortex",
            "warn",
            "matmul barrier timeout pending={} done={}",
            k_nano::smp::ap_work::barrier_pending(),
            k_nano::smp::ap_work::barrier_done()
        );
        // s419: evidência persistente (serial morre racy; BOOT.LOG sobrevive).
        k_nano::boot_logger::log_quiet(&alloc::format!(
            "matmul barrier timeout m={} k={} n={} pending={} done={}",
            m, k, n,
            k_nano::smp::ap_work::barrier_pending(),
            k_nano::smp::ap_work::barrier_done()
        ));
        if log_it {
            k_nano::slog_cortex!(
                "cortex",
                "warn",
                "matmul exit ok=false us={}",
                k_nano::tsc::now_us().saturating_sub(t_mm0)
            );
        }
        // APs podem ainda escrever no ctx/resultado → vazar em vez de liberar
        // (use-after-free). Backstop do caminho de estouro, nunca do feliz.
        // ponytail: leak só no timeout; resultado numérico intacto.
        let _ = alloc::boxed::Box::leak(ctx);
        core::mem::forget(c_data);
        return None;
    }

    CTX.store(core::ptr::null_mut(), Ordering::Release);
    if log_it {
        k_nano::slog_cortex!(
            "cortex",
            "warn",
            "matmul exit ok=true us={}",
            k_nano::tsc::now_us().saturating_sub(t_mm0)
        );
    }
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
    let t_w0 = k_nano::tsc::now_us();
    let w = &*c.w_ptr;
    // Tile de colunas múltiplo de 4 (byte-alinhado p/ bulk-load 4 pesos/byte).
    // `matmul_tile_rows` é fórmula de tile de LINHAS e devolvia 4 → 8 colunas =
    // 2 B usados por linha de cache (64 B). Sweep do piso em s411 (worker
    // k=2048 n=8192, Falcon3-1B, prefill m=8): 8→204, 16→154, 32→105, 64→130,
    // 128→127 ms. 32 é o ótimo — o strip packed (k/4 B por coluna) cabe no L1
    // junto do x (16 KB + 8 KB = 24 KB de 32); 16 amortece pouco o load de x e 64
    // já estoura o L1 (32 KB + 8 KB). AVX2 no alvo soft-float é 12× PIOR (256-bit
    // não emite; f32 vira libcall) — não reintroduzir.
    let tile = k_nano::platform_probe::matmul_tile_rows(c.k, c.n).clamp(32, 256) & !3;
    // Fase 2d: t-externo (acesso sequencial) + BULK — 1 load de byte por 4 pesos
    // em vez de 1 por peso. O LUT (que ADICIONAVA um load) regrediu 2,1×; este
    // remove 3 de cada 4 loads. Sem tabela, sem SIMD.
    let mut acc = [0.0f32; 256];
    loop {
        let jstart = COLS_CLAIMED.fetch_add(tile, Ordering::Relaxed);
        if jstart >= c.n {
            break;
        }
        let jend = (jstart + tile).min(c.n);
        let width = jend - jstart;
        for i in 0..c.m {
            for a in acc[..width].iter_mut() {
                *a = 0.0;
            }
            for t in 0..c.k {
                let xv = *c.x_ptr.add(i * c.k + t);
                let base = t * c.n + jstart;
                let mut jj = 0usize;
                while jj + 4 <= width {
                    let byte = *w.packed_data.get_unchecked((base + jj) >> 2);
                    match byte & 3 {
                        1 => acc[jj] += xv,
                        2 => acc[jj] -= xv,
                        _ => {}
                    }
                    match (byte >> 2) & 3 {
                        1 => acc[jj + 1] += xv,
                        2 => acc[jj + 1] -= xv,
                        _ => {}
                    }
                    match (byte >> 4) & 3 {
                        1 => acc[jj + 2] += xv,
                        2 => acc[jj + 2] -= xv,
                        _ => {}
                    }
                    match (byte >> 6) & 3 {
                        1 => acc[jj + 3] += xv,
                        2 => acc[jj + 3] -= xv,
                        _ => {}
                    }
                    jj += 4;
                }
                while jj < width {
                    match w.get_weight(base + jj) {
                        1 => acc[jj] += xv,
                        -1 => acc[jj] -= xv,
                        _ => {}
                    }
                    jj += 1;
                }
            }
            for jj in 0..width {
                *c.c_ptr.add(i * c.n + jstart + jj) = acc[jj];
            }
        }
    }
    let wd = k_nano::tsc::now_us().saturating_sub(t_w0);
    TERN_MAX_WORKER_US.fetch_max(wd, Ordering::Relaxed);
    TERN_WORKERS_DONE.fetch_add(1, Ordering::Relaxed);
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
    // SESSION_419: dispatch já em curso em outro core → `None` (caller cai
    // no caminho de CPU) — não cruzar barrier/statics do matmul alheio.
    let smp_ok = k_nano::platform_probe::allow_smp()
        && k_nano::smp::ap_pollable()
        && k_nano::smp::ap_entry_count() > 0
        && SmpMmGuard::try_acquire();
    let _smp_guard = if smp_ok { Some(SmpMmGuard) } else { None };
    if !smp_ok || n < 16 {
        return None;
    }

    let mut result = Tensor::new((m, n));
    if !result.is_valid() {
        return None;
    }
    let mut ctx = alloc::boxed::Box::new(TernaryJobCtx {
        w_ptr: weight as *const _,
        x_ptr: input.data.as_ptr(),
        c_ptr: result.data.as_mut_ptr(),
        m,
        k,
        n,
    });
    COLS_CLAIMED.store(0, Ordering::Release);
    T_CTX.store(&mut *ctx as *mut _, Ordering::Release);

    let aps = k_nano::smp::ap_entry_count() as usize;
    let n_workers = aps + 1;
    let log_it = TERNARY_LOG_CALLS.fetch_add(1, Ordering::Relaxed) % 20 == 0;
    if log_it {
        k_nano::slog_cortex!("cortex", "warn", "matmul enter m={} k={} n={} aps={}", m, k, n, aps);
    }
    // T1 (s413): TSC DEPOIS do log de entrada. Antes, `setup`/`us`/`sync_us`
    // incluiam o custo do serial do proprio log (~1,4 ms medido, constante em
    // todos os shapes) -> media o instrumento, nao o sync.
    let t_mm0 = k_nano::tsc::now_us();
    k_nano::smp::ap_work::clear_queue();
    k_nano::smp::ap_work::reset_barrier(aps.min(n_workers.saturating_sub(1)) as u32);
    for jid in 0..aps.min(n_workers.saturating_sub(1)) {
        let _ = k_nano::smp::ap_work::enqueue(ternary_worker, jid);
    }
    let (t_ipi, t_bsp);
    unsafe {
        k_nano::apic::send_ipi_reschedule();
        t_ipi = k_nano::tsc::now_us();
        ternary_worker(0, 0);
        t_bsp = k_nano::tsc::now_us();
    }
    // T1 (s413): split do sync por fase — setup (clear+enq+IPI), trabalho do BSP
    // e espera da barreira. E razao interna, entao robusta a carga do host
    // (diferente do `sync_us` absoluto, que e termometro de host).
    let bar_ok =
        aps == 0 || k_nano::smp::ap_work::wait_barrier_timeout(MATMUL_BARRIER_TIMEOUT_US);
    let t_bar = k_nano::tsc::now_us();
    if !bar_ok {
        T_CTX.store(core::ptr::null_mut(), Ordering::Release);
        k_nano::slog_cortex!(
            "cortex",
            "warn",
            "matmul barrier timeout pending={} done={}",
            k_nano::smp::ap_work::barrier_pending(),
            k_nano::smp::ap_work::barrier_done()
        );
        // s419: evidência persistente (BOOT.LOG).
        k_nano::boot_logger::log_quiet(&alloc::format!(
            "ternary barrier timeout m={} k={} n={} pending={} done={}",
            m, k, n,
            k_nano::smp::ap_work::barrier_pending(),
            k_nano::smp::ap_work::barrier_done()
        ));
        if log_it {
            k_nano::slog_cortex!(
                "cortex",
                "warn",
                "matmul exit ok=false us={}",
                k_nano::tsc::now_us().saturating_sub(t_mm0)
            );
        }
        // APs podem ainda escrever no ctx/resultado → vazar (use-after-free).
        // ponytail: leak só no timeout; caller cai em AVX512/CPU.
        let _ = alloc::boxed::Box::leak(ctx);
        core::mem::forget(result);
        return None;
    }
    T_CTX.store(core::ptr::null_mut(), Ordering::Release);
    let t_total = k_nano::tsc::now_us().saturating_sub(t_mm0);
    let wmax = TERN_MAX_WORKER_US.swap(0, Ordering::Relaxed);
    let wdone = TERN_WORKERS_DONE.swap(0, Ordering::Relaxed);
    if log_it {
        k_nano::slog_cortex!(
            "cortex",
            "warn",
            "matmul exit m={} k={} n={} us={} workers={} worker_max_us={} sync_us={} split=[setup={} bsp={} bar={}]",
            m,
            k,
            n,
            t_total,
            wdone,
            wmax,
            t_total.saturating_sub(wmax),
            t_ipi.saturating_sub(t_mm0),
            t_bsp.saturating_sub(t_ipi),
            t_bar.saturating_sub(t_bsp)
        );
    }
    Some(result)
}
