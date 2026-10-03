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
/// Sparsity MEDIDA, não claim (SESSION_352: claim sem artefato = overclaim).
/// Pesos ternários 0 (pulados via ADD/SUB/SKIP) vs totais, acumulados pelos
/// workers do ÚLTIMO dispatch — reset no boundary de cada matmul.
static TERN_SKIP_ZERO_TOTAL: AtomicU64 = AtomicU64::new(0);
static TERN_WEIGHT_TOTAL: AtomicU64 = AtomicU64::new(0);

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

/// Tile próprio de COLUNAS do ternary_worker (SESSION_412) — NÃO reusar
/// `matmul_tile_rows(k,n)` (fórmula de LINHAS: devolvia 4 → clamp 8 colunas =
/// 2 B usados por linha de cache de 64 B).
///
/// Fit L1: strip packed por coluna = k/4 B + vetor x = k*4 B cabem em ~24 KB
/// de L1d (32 KB típico). Sweep do piso (Falcon3-1B, prefill m=8, worker
/// k=2048 n=8192): 8→204 ms, 16→154 ms, 32→105 ms, 64→130 ms, 128→127 ms.
/// 32 é o ótimo (strip 16 KB + x 8 KB = 24 KB); 16 amortece pouco o load de
/// x, 64+ estoura o L1. Resultado sempre múltiplo de 4 (byte-alinhado:
/// 4 pesos/byte) via `& !3`; piso 32, teto 256.
#[inline]
fn ternary_col_tile(k: usize, n: usize) -> usize {
    const L1_BUDGET: usize = 24 * 1024;
    let x_bytes = k.saturating_mul(4).min(L1_BUDGET / 2);
    let per_col = (k / 4).max(1);
    let avail = L1_BUDGET.saturating_sub(x_bytes);
    let fit = avail / per_col;
    let t = fit.clamp(32, 256) & !3;
    // Não pedir mais colunas que o problema tem (arredonda n p/ mult. de 4).
    let cap = ((n + 3) & !3).max(4);
    t.min(cap).max(4)
}

unsafe fn ternary_worker(_job_id: usize, _worker: usize) {
    let ctx = T_CTX.load(Ordering::Acquire);
    if ctx.is_null() {
        return;
    }
    let c = &*ctx;
    let t_w0 = k_nano::tsc::now_us();
    let w = &*c.w_ptr;
    // Tile de colunas múltiplo de 4 (byte-alinhado p/ bulk-load 4 pesos/byte).
    // Fórmula própria de COLUNAS (ternary_col_tile): fit L1 strip k/4 + x.
    // Sweep s412 (k=2048 n=8192): 8→204, 16→154, 32→105, 64→130, 128→127 ms.
    // AVX2-256 no alvo soft-float é 12× PIOR (256-bit não emite no target
    // soft-float; f32 vira libcall) — path metal nunca importa AVX2.
    let tile = ternary_col_tile(c.k, c.n);
    // Fase 2d: t-externo (acesso sequencial) + BULK — 1 load de byte por 4 pesos
    // em vez de 1 por peso. O LUT (que ADICIONAVA um load) regrediu 2,1×; este
    // remove 3 de cada 4 loads. Sem tabela, sem SIMD.
    let mut acc = [0.0f32; 256];
    // Sparsity: contadores LOCAIS por worker, somados 1× ao fim — nunca
    // atomic por peso no hot-inner (custo zero quando não lido).
    let mut skip_zero: u64 = 0;
    let mut weight_total: u64 = 0;
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
                        _ => skip_zero += 1,
                    }
                    match (byte >> 2) & 3 {
                        1 => acc[jj + 1] += xv,
                        2 => acc[jj + 1] -= xv,
                        _ => skip_zero += 1,
                    }
                    match (byte >> 4) & 3 {
                        1 => acc[jj + 2] += xv,
                        2 => acc[jj + 2] -= xv,
                        _ => skip_zero += 1,
                    }
                    match (byte >> 6) & 3 {
                        1 => acc[jj + 3] += xv,
                        2 => acc[jj + 3] -= xv,
                        _ => skip_zero += 1,
                    }
                    weight_total += 4;
                    jj += 4;
                }
                while jj < width {
                    match w.get_weight(base + jj) {
                        1 => acc[jj] += xv,
                        -1 => acc[jj] -= xv,
                        _ => skip_zero += 1,
                    }
                    weight_total += 1;
                    jj += 1;
                }
            }
            for jj in 0..width {
                *c.c_ptr.add(i * c.n + jstart + jj) = acc[jj];
            }
        }
    }
    if weight_total > 0 {
        TERN_SKIP_ZERO_TOTAL.fetch_add(skip_zero, Ordering::Relaxed);
        TERN_WEIGHT_TOTAL.fetch_add(weight_total, Ordering::Relaxed);
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
    // Sparsity: janela = 1 dispatch (último matmul SMP medido).
    TERN_SKIP_ZERO_TOTAL.store(0, Ordering::Release);
    TERN_WEIGHT_TOTAL.store(0, Ordering::Release);

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
    let (skip, wtot) = skip_ratio().unwrap_or((0, 0));
    if log_it {
        k_nano::slog_cortex!(
            "cortex",
            "warn",
            "matmul exit m={} k={} n={} us={} workers={} worker_max_us={} sync_us={} split=[setup={} bsp={} bar={}] skip={}/{}",
            m,
            k,
            n,
            t_total,
            wdone,
            wmax,
            t_total.saturating_sub(wmax),
            t_ipi.saturating_sub(t_mm0),
            t_bsp.saturating_sub(t_ipi),
            t_bar.saturating_sub(t_bsp),
            skip,
            wtot
        );
    }
    Some(result)
}

/// Sparsity medida do último dispatch ternário SMP: `(zeros pulados, total)`.
/// `None` = nenhum dispatch medido ainda (n/a ≠ 0, SESSION_411).
pub fn skip_ratio() -> Option<(u64, u64)> {
    let total = TERN_WEIGHT_TOTAL.load(Ordering::Acquire);
    if total == 0 {
        return None;
    }
    Some((TERN_SKIP_ZERO_TOTAL.load(Ordering::Acquire), total))
}

#[cfg(test)]
mod col_tile_tests {
    use super::{skip_ratio, ternary_col_tile, TERN_SKIP_ZERO_TOTAL, TERN_WEIGHT_TOTAL};
    use crate::tensor::PackedTernaryTensor;
    use core::sync::atomic::Ordering;

    #[test]
    fn col_tile_fit_l1_sweep_shape_is_32() {
        // Shape do sweep s412 (k=2048 n=8192): ótimo medido 32.
        assert_eq!(ternary_col_tile(2048, 8192), 32);
    }

    #[test]
    fn col_tile_always_mul4_and_bounded() {
        for (k, n) in [(64, 64), (512, 128), (2048, 8192), (8192, 8192), (128, 16)] {
            let t = ternary_col_tile(k, n);
            assert_eq!(t & 3, 0, "k={k} n={n} tile={t} não é mult. de 4");
            assert!((4..=256).contains(&t), "k={k} n={n} tile={t} fora de [4,256]");
            assert!(t <= ((n + 3) & !3).max(4), "k={k} n={n} tile={t} > n");
        }
    }

    #[test]
    fn skip_ratio_none_sem_amostra_e_known_ratio() {
        // n/a ≠ 0 (SESSION_411): sem dispatch medido = None, não 0.
        TERN_SKIP_ZERO_TOTAL.store(0, Ordering::Relaxed);
        TERN_WEIGHT_TOTAL.store(0, Ordering::Relaxed);
        assert_eq!(skip_ratio(), None);
        // 1 zero em 4 pesos = 25% de sparsity medida.
        TERN_SKIP_ZERO_TOTAL.store(1, Ordering::Relaxed);
        TERN_WEIGHT_TOTAL.store(4, Ordering::Relaxed);
        assert_eq!(skip_ratio(), Some((1, 4)));
        TERN_SKIP_ZERO_TOTAL.store(0, Ordering::Relaxed);
        TERN_WEIGHT_TOTAL.store(0, Ordering::Relaxed);
    }

    #[test]
    fn packed_byte_zero_count_matches_pack() {
        // [1,0,-1,0] → 01|00|10|00 = 0b00100001; 2 zeros dos 4 pesos.
        let packed = PackedTernaryTensor::pack_weights(&[1, 0, -1, 0]);
        assert_eq!(packed[0], 0b00_10_00_01);
        let w = PackedTernaryTensor { shape: (1, 4), packed_data: packed };
        let zeros = (0..4).filter(|&i| w.get_weight(i) == 0).count() as u64;
        assert_eq!(zeros, 2);
        // Contrato do worker: total conta os 4 pesos do byte.
        TERN_SKIP_ZERO_TOTAL.store(zeros, Ordering::Relaxed);
        TERN_WEIGHT_TOTAL.store(4, Ordering::Relaxed);
        assert_eq!(skip_ratio(), Some((2, 4)));
        TERN_SKIP_ZERO_TOTAL.store(0, Ordering::Relaxed);
        TERN_WEIGHT_TOTAL.store(0, Ordering::Relaxed);
    }

    /// Tática II (BitEngine) refutada para o v6. O pack é 4 pesos/byte: zeros
    /// dividem byte com não-zeros, então nem bitmask de presença nem formato
    /// esparso (índice+sign) reduzem banda na sparsity medida (~25%, s436).
    /// Aritmética inteira (bp) — não depende de f32 no host.
    #[test]
    fn tatica_ii_bitmask_refutada_por_sparsity_v6() {
        // Sparsity medida s436: 1 zero em 4 pesos = 25%. Doc reivindica 30-45%.
        const Z: u64 = 25;

        // (a) Byte todo-zero = única unidade que um bitmask de byte pularia:
        //     P = z^4. A 25% → 39 bp (0,39%) dos bytes; doc reivindica -35% de DRAM.
        let byte_all_zero_bp = Z.pow(4) / 10_000; // (25/100)^4 × 10_000
        assert!(
            byte_all_zero_bp < 100,
            "byte all-zero {byte_all_zero_bp}bp ≥ 1% — invalida o -35% do doc"
        );

        // (b) Esparso (delta 4-bit + sign 1-bit por não-zero) = 5 bits/não-zero
        //     vs 2 bits/peso do pack v6. Break-even em z > 60%.
        let sparse_bits_per_100 = 5 * (100 - Z);
        let packed_bits_per_100 = 2 * 100;
        assert!(
            sparse_bits_per_100 > packed_bits_per_100,
            "esparso {sparse_bits_per_100}bps > packed {packed_bits_per_100}bps a z={Z}%"
        );
        let breakeven_z = 100 - (packed_bits_per_100 / 5);
        assert_eq!(breakeven_z, 60, "break-even de sparsity mal calculado");
        assert!(Z < breakeven_z, "sparsity medida {Z}% abaixo do break-even 60%");
    }
}
