//! Diagnóstico do caminho de matmul ternário — ADR-0106 D0 / s362.
//!
//! Motivação medida: o `ROUTER.BITNET` **carrega** (log `loaded from file (v6)`),
//! mas em todos os boots do repositório a rota neural teve **0 sucessos** e o
//! `classify_intent` caiu em `classify_keywords` 468 vezes com
//! `classify matmul fail — keyword` (`crates/cortex/src/trinity.rs:383`).
//!
//! A leitura estática não fecha: com `w.shape=(64,7)`, `x.shape=(1,64)` e ambos
//! `is_valid()`, toda branch do `ternary_matmul` deveria devolver `Some`. Logo o
//! device tem de dizer **qual ramo retornou**. Este módulo faz isso:
//!
//! - contadores lock-free por ramo (relaxed; sem alloc, sem lock no hot path);
//! - um **dump único** na primeira falha, com shapes/validade no momento da falha;
//! - um resumo compacto a cada 64 falhas (para um boot só já responder tudo).
//!
//! Honestidade: nenhum contador aqui altera o comportamento da decisão.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

fn inc(c: &AtomicU64) {
    c.fetch_add(1, Ordering::Relaxed);
}

fn ld(c: &AtomicU64) -> u64 {
    c.load(Ordering::Relaxed)
}

// ── Contadores por ramo (todas as saídas do caminho) ───────────────────────
pub static GUARD_FAIL: AtomicU64 = AtomicU64::new(0); // k!=k2 / !valid / dim 0
pub static DISPATCH_OK: AtomicU64 = AtomicU64::new(0);
pub static DISPATCH_INVALID: AtomicU64 = AtomicU64::new(0); // tensor inválido do dispatcher
pub static DISPATCH_NONE: AtomicU64 = AtomicU64::new(0); // dispatcher sinalizou CPU
pub static W2A8_INVALID: AtomicU64 = AtomicU64::new(0);
pub static W2A8_OK: AtomicU64 = AtomicU64::new(0);
pub static GPU_DISP_AWAITING: AtomicU64 = AtomicU64::new(0);
pub static BITWISE_OK: AtomicU64 = AtomicU64::new(0);
pub static BITWISE_INVALID: AtomicU64 = AtomicU64::new(0);
pub static AVX512_INVALID: AtomicU64 = AtomicU64::new(0);
pub static AVX512_NONE: AtomicU64 = AtomicU64::new(0);
pub static SSE2_OK: AtomicU64 = AtomicU64::new(0);
pub static SSE2_ZERO_GUARD: AtomicU64 = AtomicU64::new(0); // sse2 devolveu zero((0,0))
pub static SSE_OK: AtomicU64 = AtomicU64::new(0);
pub static SSE_INVALID: AtomicU64 = AtomicU64::new(0);
pub static SSE_NONE: AtomicU64 = AtomicU64::new(0);
pub static OK_TOTAL: AtomicU64 = AtomicU64::new(0);
pub static FAIL_TOTAL: AtomicU64 = AtomicU64::new(0);

// ── Registro por chamada (decide qual RAMO foi tomado em cada chamada) ─────
// Motivação: os contadores agregados de s362 não fecham com os caminhos lidos
// (sse2_ok=1 + sse_invalid=1 exige dois ramos incompatíveis na mesma chamada).
// Aqui cada chamada ao wrapper registra (k,n,m,path,valid) com o caminho REAL.
pub static CALLS: AtomicU64 = AtomicU64::new(0);
static CALL_LOG: [AtomicU64; 8] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];

/// Caminho tomado dentro do wrapper ternário (campo `path` do registro).
/// 0=guard  1=avx512 2=avx512-invalid  3=baremetal-sse2  4=host-avx2
/// 5=x86-sse2 (sem counter de sse2)  6=lane4  7=scalar
pub fn note_call(k: usize, n: usize, m: usize, path: u32, valid: bool) {
    let idx = (CALLS.fetch_add(1, Ordering::Relaxed) as usize) & 7;
    let packed = (k.min(0xFFFF) as u64)
        | ((n.min(0xFF) as u64) << 16)
        | ((m.min(0xFF) as u64) << 24)
        | (((path & 0xF) as u64) << 32)
        | ((valid as u64) << 36);
    CALL_LOG[idx].store(packed, Ordering::Relaxed);
}

/// Shape vista DENTRO do kernel SSE2, imediatamente antes de retornar.
/// Comparar com `INVALID_SHAPE` separa "kernel devolve errado" de "o retorno (ABI
/// `#[target_feature]`) corrompe o struct" — SESSION_336 classe.
pub static KERNEL_SHAPE: AtomicU64 = AtomicU64::new(u64::MAX);

pub fn note_kernel_shape(a: usize, b: usize, len: usize) {
    let packed = (a.min(0xFFFF) as u64) | ((b.min(0xFFFF) as u64) << 16) | ((len.min(0xFFFF) as u64) << 32);
    KERNEL_SHAPE.store(packed, Ordering::Relaxed);
}

/// Shape do tensor inválido devolvido pelo wrapper (o dado que faltava).
pub static INVALID_SHAPE: AtomicU64 = AtomicU64::new(u64::MAX);

pub fn note_invalid_shape(a: usize, b: usize, len: usize) {
    let packed = (a.min(0xFFFF) as u64) | ((b.min(0xFFFF) as u64) << 16) | ((len.min(0xFFFF) as u64) << 32);
    INVALID_SHAPE.store(packed, Ordering::Relaxed);
}

fn calls_into(mut s: alloc::string::String) -> alloc::string::String {
    use alloc::format;
    let total = ld(&CALLS);
    s.push_str(&format!(" calls={}:", total));
    let shown = core::cmp::min(total as usize, 8);
    for i in 0..shown {
        let p = CALL_LOG[i].load(Ordering::Relaxed);
        s.push_str(&format!(
            " [{},{},{} p{} v{}]",
            p & 0xFFFF,
            (p >> 16) & 0xFF,
            (p >> 24) & 0xFF,
            (p >> 32) & 0xF,
            (p >> 36) & 1
        ));
    }
    let inv = INVALID_SHAPE.load(Ordering::Relaxed);
    if inv != u64::MAX {
        s.push_str(&format!(
            " invalid.shape=({},{}) len={}",
            inv & 0xFFFF,
            (inv >> 16) & 0xFFFF,
            (inv >> 32) & 0xFFFF
        ));
    }
    let ker = KERNEL_SHAPE.load(Ordering::Relaxed);
    if ker != u64::MAX {
        s.push_str(&format!(
            " kernel.shape=({},{}) len={}",
            ker & 0xFFFF,
            (ker >> 16) & 0xFFFF,
            (ker >> 32) & 0xFFFF
        ));
    }
    s
}

/// Último estado observado no momento de uma falha (para o dump único).
pub struct FailSnapshot {
    pub w_shape: (usize, usize),
    pub w_packed_len: usize,
    pub x_shape: (usize, usize),
    pub x_len: usize,
    pub x_valid: bool,
    pub num_experts: usize,
    pub emb_zero: bool,
}

static DUMPED: AtomicBool = AtomicBool::new(false);
static LAST_SNAP: AtomicU64 = AtomicU64::new(0);

/// Snapshot compactado (sem alloc) para o resumo periódico.
pub fn stash_fail_snapshot(s: &FailSnapshot) {
    // Empacota o essencial em 64 bits: w.k(16) w.n(8) x.m(8) x.k(8) x_len(8) flags(16)
    let packed: u64 = (s.w_shape.0.min(0xFFFF) as u64)
        | ((s.w_shape.1.min(0xFF) as u64) << 16)
        | ((s.x_shape.0.min(0xFF) as u64) << 24)
        | ((s.x_shape.1.min(0xFF) as u64) << 32)
        | ((s.x_len.min(0xFF) as u64) << 40)
        | (((s.x_valid as u64) | ((s.emb_zero as u64) << 1)) << 48);
    LAST_SNAP.store(packed, Ordering::Relaxed);
}

pub fn last_snapshot() -> u64 {
    LAST_SNAP.load(Ordering::Relaxed)
}

pub fn note_guard_fail() {
    inc(&GUARD_FAIL);
}

pub fn note_dispatch_ok() {
    inc(&DISPATCH_OK);
    inc(&OK_TOTAL);
}

pub fn note_dispatch_invalid() {
    inc(&DISPATCH_INVALID);
}

pub fn note_dispatch_none() {
    inc(&DISPATCH_NONE);
}

pub fn note_w2a8_invalid() {
    inc(&W2A8_INVALID);
}

pub fn note_w2a8_ok() {
    inc(&W2A8_OK);
    inc(&OK_TOTAL);
}

/// Device W2A8 staged but AWAITING_HW (≠ CPU W2A8).
pub fn note_gpu_disp_awaiting() {
    inc(&GPU_DISP_AWAITING);
}

pub fn note_bitwise_ok() {
    inc(&BITWISE_OK);
    inc(&OK_TOTAL);
}

pub fn note_bitwise_invalid() {
    inc(&BITWISE_INVALID);
}

pub fn note_avx512_invalid() {
    inc(&AVX512_INVALID);
}

pub fn note_avx512_none() {
    inc(&AVX512_NONE);
}

pub fn note_sse2_ok() {
    inc(&SSE2_OK);
}

pub fn note_sse2_zero_guard() {
    inc(&SSE2_ZERO_GUARD);
}

pub fn note_sse_ok() {
    inc(&SSE_OK);
    inc(&OK_TOTAL);
}

pub fn note_sse_invalid() {
    inc(&SSE_INVALID);
}

pub fn note_sse_none() {
    inc(&SSE_NONE);
}

/// Dump completo — **uma única vez** (a primeira falha é a informativa; as
/// outras 467 são a mesma coisa). Sem `serial_print!` (deadlock em IRQ).
pub fn dump_once(s: &FailSnapshot) {
    stash_fail_snapshot(s);
    if DUMPED.swap(true, Ordering::Relaxed) {
        return;
    }
    k_nano::slog_cortex!(
        "MatmulDiag",
        "warn",
        "1a falha: w.shape=({},{}) w.packed={} x.shape=({},{}) x.len={} x.valid={} n_exp={} emb_zero={} \
         | guard={} disp_ok={} disp_invalid={} disp_none={} w2a8_invalid={} bitwise_ok={} bitwise_invalid={} \
         avx512_invalid={} avx512_none={} sse2_ok={} sse2_zero={} sse_ok={} sse_invalid={} sse_none={} ok={} fail={}",
        s.w_shape.0,
        s.w_shape.1,
        s.w_packed_len,
        s.x_shape.0,
        s.x_shape.1,
        s.x_len,
        s.x_valid as u32,
        s.num_experts,
        s.emb_zero as u32,
        ld(&GUARD_FAIL),
        ld(&DISPATCH_OK),
        ld(&DISPATCH_INVALID),
        ld(&DISPATCH_NONE),
        ld(&W2A8_INVALID),
        ld(&BITWISE_OK),
        ld(&BITWISE_INVALID),
        ld(&AVX512_INVALID),
        ld(&AVX512_NONE),
        ld(&SSE2_OK),
        ld(&SSE2_ZERO_GUARD),
        ld(&SSE_OK),
        ld(&SSE_INVALID),
        ld(&SSE_NONE),
        ld(&OK_TOTAL),
        ld(&FAIL_TOTAL)
    );
}

/// Conta a falha do caminho do router e, a cada 64, emite o resumo agregado.
pub fn note_router_fail(s: &FailSnapshot) {
    let n = FAIL_TOTAL.fetch_add(1, Ordering::Relaxed) + 1;
    dump_once(s);
    if n % 64 == 0 {
        k_nano::slog_cortex!(
            "MatmulDiag",
            "warn",
            "resumo #{}: guard={} disp_ok={} disp_invalid={} disp_none={} bitwise_ok={} avx512_invalid={} avx512_none={} sse2_ok={} sse2_zero={} sse_ok={} sse_invalid={} sse_none={} ok={} fail={}",
            n,
            ld(&GUARD_FAIL),
            ld(&DISPATCH_OK),
            ld(&DISPATCH_INVALID),
            ld(&DISPATCH_NONE),
            ld(&BITWISE_OK),
            ld(&AVX512_INVALID),
            ld(&AVX512_NONE),
            ld(&SSE2_OK),
            ld(&SSE2_ZERO_GUARD),
            ld(&SSE_OK),
            ld(&SSE_INVALID),
            ld(&SSE_NONE),
            ld(&OK_TOTAL),
            ld(&FAIL_TOTAL)
        );
    }
}

/// Linha de status para HUD/shell (uma só string, formato estável).
pub fn status_line() -> alloc::string::String {
    calls_into(alloc::format!(
        "MatmulDiag ok={} fail={} | guard={} disp[ok={} inv={} none={}] w2a8[ok={} inv={}] gpu_await={} bitwise_ok={} avx512[inv={} none={}] sse2_ok={} sse2_zero={} sse[ok={} inv={} none={}]",
        ld(&OK_TOTAL),
        ld(&FAIL_TOTAL),
        ld(&GUARD_FAIL),
        ld(&DISPATCH_OK),
        ld(&DISPATCH_INVALID),
        ld(&DISPATCH_NONE),
        ld(&W2A8_OK),
        ld(&W2A8_INVALID),
        ld(&GPU_DISP_AWAITING),
        ld(&BITWISE_OK),
        ld(&AVX512_INVALID),
        ld(&AVX512_NONE),
        ld(&SSE2_OK),
        ld(&SSE2_ZERO_GUARD),
        ld(&SSE_OK),
        ld(&SSE_INVALID),
        ld(&SSE_NONE)
    ))
}
