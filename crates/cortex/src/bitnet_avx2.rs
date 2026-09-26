#![allow(dead_code)]
//! BitNet ternary matmul com otimizacoes de cache:
//! - align(64) no PackedTernaryTensor (evita split cache line)
//! - Matmul ternario bitwise sem branch (16 pesos por iteracao)
//! - Prefetch entre camadas do transformer
//! - Dispatch adaptativo por CPU: scalar, AVX2, bitwise
//!
//! Honesty ADR-0101 / SESSION_298 + SESSION_348 (lab Falcon3-3B 1.58):
//! - Scalar/SSE: ADD/SUB/SKIP de ativações f32 sobre packed 2-bit — nativo algébrico.
//! - AVX2 host (`not(target_os="none")`): unpack i8 → f32 FMA — NÃO skip-native SIMD.
//! - Bare-metal (`target_os="none"`): `avx2_ternary_matmul_impl` delega ao SSE2 ADD/SUB/SKIP.
//! - W2A8: gate ADR-0105 B3 (`w2a8_enabled`); host=maddubs, no_std=scalar quantizado.

use crate::tensor::{PackedTernaryTensor, Tensor};
use alloc::vec;
use core::sync::atomic::{AtomicBool, Ordering};

// ─── Tiling Constants (ADR-0084 F5, tuning por HW) ───────────────────────
// ponytail: defaults para cache L2 256KB; ajustar por target (faixas:
// p∈[2,4,8], row∈[2..32], col∈[32..1024])
pub const ROW_BLOCK_SIZE: usize = 4;
pub const COL_BLOCK_SIZE: usize = 128;
pub const PARALLEL_SIZE: usize = 4;

// ─── HW Detection ───────────────────────────────────────────────────────

fn avx2_available() -> bool {
    k_nano::platform_probe::allow_avx2()
}

// ─── Main Dispatch ──────────────────────────────────────────────────────

/// Lane D-accel: shape de prova 3B (só a prova faz k,n ≥ 1024 — self-test
/// mesh 64×64 e HWExpert 128 ficam de fora do log).
fn is_proof_shaped(k: usize, n: usize) -> bool {
    k >= 1024 && n >= 1024
}

/// Rota logada 1×/boot (sem spam por layer).
static A2_ROUTE_LOGGED: AtomicBool = AtomicBool::new(false);

/// Mesh seria tentado? Espelho barato do gate de dispatch_ternary (p2p only).
#[cfg(feature = "p2p")]
fn mesh_route_attempted() -> bool {
    let role = k_nano::net::mesh::local_role();
    let can_send = matches!(
        role,
        k_nano::net::mesh::NodeRole::Worker
            | k_nano::net::mesh::NodeRole::Memory
            | k_nano::net::mesh::NodeRole::Compute
    );
    if !can_send {
        return false;
    }
    let peers = k_nano::net::mesh::MESH_ENGINE
        .lock()
        .as_ref()
        .map_or(0, |eng| eng.node_count());
    peers >= 1 && !k_nano::memory::refuse_heavy_frag()
}

/// Lane D-accel: rótulo da rota que o dispatch TENTARIA (inspeção pura dos
/// mesmos gates, na mesma ordem — manter em sync com `ternary_matmul` abaixo
/// + `bitnet_sse::ternary_matmul`). Não executa nada, nunca aloca.
pub fn cpu_route_label(w: &PackedTernaryTensor, x: &Tensor) -> &'static str {
    let (k, n) = w.shape;
    let (m, k2) = x.shape;
    if k != k2 || m == 0 || n == 0 || k == 0 {
        return "refuse-guard";
    }
    let big = n >= 64 && k >= 64;
    #[cfg(feature = "p2p")]
    if mesh_route_attempted() {
        return "mesh";
    }
    if crate::compute::npu_registered() {
        return "npu";
    }
    if big && crate::compute::gpu_registered() {
        return "gpu";
    }
    if big
        && k_nano::platform_probe::allow_smp()
        && k_nano::smp::ap_pollable()
        && k_nano::smp::ap_entry_count() > 0
    {
        return "smp";
    }
    if big && k_nano::platform_probe::allow_avx512() {
        return "avx512";
    }
    if crate::bitnet_w2a8::w2a8_enabled() && (m == 1 || m >= 8) {
        return "w2a8";
    }
    if m >= 8 && k_nano::platform_probe::allow_avx2() {
        // Impl só existe no host; no metal cai no SSE abaixo.
        #[cfg(all(target_arch = "x86_64", not(target_os = "none")))]
        return "bitwise-avx2";
    }
    // bitnet_sse::ternary_matmul, ordem interna:
    if k_nano::platform_probe::allow_avx512() {
        return "avx512";
    }
    #[cfg(all(target_arch = "x86_64", target_os = "none"))]
    if n >= 4 {
        return "sse";
    }
    if k_nano::platform_probe::allow_avx2() && k >= 8 && n >= 8 && n % 4 == 0 {
        return "avx2-host";
    }
    #[cfg(target_arch = "x86_64")]
    if n >= 4 {
        return "sse-fill";
    }
    if n >= 4 {
        return "sse-unrolled";
    }
    "scalar"
}

/// Log 1×/boot da rota da prova (shape 3B). Chamado no topo do dispatch.
fn maybe_log_a2_route_once(w: &PackedTernaryTensor, x: &Tensor) {
    let (k, n) = w.shape;
    if !is_proof_shaped(k, n) {
        return;
    }
    if A2_ROUTE_LOGGED.swap(true, Ordering::Relaxed) {
        return;
    }
    let (m, _) = x.shape;
    k_nano::slog_cortex!(
        "InferQ",
        "ok",
        "a2_proof matmul={} shape={}x{}x{}",
        cpu_route_label(w, x),
        m,
        k,
        n
    );
}

pub fn ternary_matmul(weight: &PackedTernaryTensor, input: &Tensor) -> Option<Tensor> {
    let (k, n) = weight.shape;
    let (m, k2) = input.shape;
    if k != k2 || !input.is_valid() || m == 0 || n == 0 || k == 0 {
        crate::matmul_diag::note_guard_fail();
        return None;
    }
    // Lane D-accel: rota visível 1×/boot (fora do hot path por shape+once).
    maybe_log_a2_route_once(weight, input);

    // ADR-0057 WS-C: NPU/GPU/parallel dispatch
    if let Some(r) = crate::compute::dispatch_ternary(weight, input) {
        if r.is_valid() {
            crate::matmul_diag::note_dispatch_ok();
            return Some(r);
        }
        crate::matmul_diag::note_dispatch_invalid();
        return None;
    }
    crate::matmul_diag::note_dispatch_none();

    // s364+: nó frugal (<1.5G) — após mesh skip, local big #GP (SSE2 sret /
    // heap). Recusa honesta; matmul pequeno (<64) segue no SSE/scalar.
    #[cfg(feature = "p2p")]
    if n >= 64
        && k >= 64
        && k_nano::memory::mesh_frag_pressure()
    {
        k_nano::slog_cortex!(
            "MESH",
            "warn",
            "matmul local skip DEGRADED RAM={}MB shape={}x{}x{}",
            k_nano::memory::TOTAL_RAM_MB.load(core::sync::atomic::Ordering::Relaxed),
            m,
            k,
            n
        );
        return None;
    }

    // ADR-0105 B3 / ADR-0084 F4: W2A8 CPU ladder (WHPX/HW + gaps).
    if crate::bitnet_w2a8::w2a8_enabled() && (m == 1 || m >= 8) {
        unsafe {
            if let Some(r) = crate::bitnet_w2a8::w2a8_ternary_matmul(weight, input) {
                if r.is_valid() {
                    crate::matmul_diag::note_w2a8_ok();
                    return Some(r);
                }
                crate::matmul_diag::note_w2a8_invalid();
                return None;
            }
        }
    }

    // ADR-0084 F2: activation-parallel for prefill (m >= 8)
    // bitwise_matmul uses LUT-based FMA per byte-group; wins ~2x at m≥32
    // (src/README bitnet.cpp: activation-parallel 1.85-2.0x at m≥32)
    let big_m = m >= 8;
    if big_m && avx2_available() {
        #[cfg(all(target_arch = "x86_64", not(target_os = "none")))]
        unsafe {
            let r = avx2_bitwise_matmul(weight, input, m, k, n);
            if r.is_valid() {
                crate::matmul_diag::note_bitwise_ok();
                return Some(r);
            }
            crate::matmul_diag::note_bitwise_invalid();
            return None;
        }
    }

    // ADR-0061 unified dispatch: AVX-512 → AVX2 → SSE4.2 → scalar
    match crate::bitnet_sse::ternary_matmul(weight, input) {
        Some(r) if r.is_valid() => {
            crate::matmul_diag::note_sse_ok();
            Some(r)
        }
        Some(r) => {
            crate::matmul_diag::note_sse_invalid();
            crate::matmul_diag::note_invalid_shape(r.shape.0, r.shape.1, r.data.len());
            None
        }
        None => {
            crate::matmul_diag::note_sse_none();
            None
        }
    }
}

// ─── Scalar Fallback ────────────────────────────────────────────────────

fn scalar_ternary_matmul(weight: &PackedTernaryTensor, input: &Tensor, m: usize, k: usize, n: usize) -> Tensor {
    let mut result = Tensor::new((m, n));
    if !result.is_valid() || !input.is_valid() {
        return Tensor::zero((0, 0));
    }
    for i in 0..m {
        for j in 0..n {
            let mut sum = 0.0f32;
            for t in 0..k {
                sum += match weight.get_weight(t * n + j) {
                    1 => input.data[i * k + t],
                    -1 => -input.data[i * k + t],
                    _ => 0.0,
                };
            }
            result.data[i * n + j] = sum;
        }
    }
    result
}

// ─── AVX2 Bitwise Ternary Matmul (sem branch, 4 pesos/byte direto) ─────

/// Processa 4 pesos ternarios de uma vez sem branch (match).
/// Cada byte = 4 pesos: bits (0,1) = peso0, (2,3)=peso1, (4,5)=peso2, (6,7)=peso3
/// Codificacao: 00=0, 01=+1, 10=-1
#[cfg(all(target_arch = "x86_64", not(target_os = "none")))]
unsafe fn process_quad(quad: u8, inputs: &[f32; 4]) -> f32 {
    let mut sum = 0.0f32;
    // peso0
    match quad & 3 { 1 => sum += inputs[0], 2 => sum -= inputs[0], _ => {} }
    // peso1
    match (quad >> 2) & 3 { 1 => sum += inputs[1], 2 => sum -= inputs[1], _ => {} }
    // peso2
    match (quad >> 4) & 3 { 1 => sum += inputs[2], 2 => sum -= inputs[2], _ => {} }
    // peso3
    match (quad >> 6) & 3 { 1 => sum += inputs[3], 2 => sum -= inputs[3], _ => {} }
    sum
}

/// AVX2 bitwise: processa 16 pesos ternarios por iteracao sem unpack.
/// Carrega 4 bytes (16 pesos) → expande para 16 f32 → FMA com input.
#[cfg(all(target_arch = "x86_64", not(target_os = "none")))]
unsafe fn avx2_bitwise_matmul(weight: &PackedTernaryTensor, input: &Tensor, m: usize, k: usize, n: usize) -> Tensor {
    use core::arch::x86_64::*;

    let mut result = Tensor::new((m, n));
    // Pre-computa lookup table: byte → [f32; 4] para pesos ternarios
    // Cada entrada do byte e mapeada diretamente sem branch
    let mut lut = [0.0f32; 256 * 4];
    for byte in 0..256u16 {
        let b = byte as u8;
        let base = (byte as usize) * 4;
        // Cada par de bits vira f32 sem match
        for q in 0..4 {
            let bits = (b >> (q * 2)) & 3;
            lut[base + q] = match bits {
                1 => 1.0,
                2 => -1.0,
                _ => 0.0,
            };
        }
    }

    for i in 0..m {
        let inp_row = &input.data[i * k..];
        let out_row = &mut result.data[i * n..];
        for j in 0..n { out_row[j] = 0.0; }

        // Processa k em grupos de 4 (pesos por byte)
        let k_blocks = k / 4;
        let packed_cols = n.div_ceil(4);

        for t in 0..k_blocks {
            let inp_base = t * 4;
            let inp_vals = _mm_loadu_ps(inp_row.as_ptr().add(inp_base));

            for j_block in 0..packed_cols {
                let byte_idx = t * packed_cols + j_block;
                if byte_idx >= weight.packed_data.len() { break; }
                let p = weight.packed_data[byte_idx];
                let out_off = j_block * 4;
                let lanes = core::cmp::min(4, n - out_off);

                // Lookup table: byte → [f32; 4]
                let lut_base = &lut[(p as usize) * 4] as *const f32;
                let w_f32 = _mm_loadu_ps(lut_base);

                if lanes == 4 {
                    // FMA: out[j_block*4..] += inp[t*4..] * w[byte]
                    let prev = _mm_loadu_ps(out_row.as_mut_ptr().add(out_off));
                    let updated = _mm_fmadd_ps(inp_vals, w_f32, prev);
                    _mm_storeu_ps(out_row.as_mut_ptr().add(out_off), updated);
                } else {
                    // Tail (n % 4 != 0): escalar, para nao ler/escrever alem da linha.
                    for q in 0..lanes {
                        let w = unsafe { *lut_base.add(q) };
                        out_row[out_off + q] += input.data[i * k + t * 4 + q] * w;
                    }
                }
            }
        }
    }

    result
}

// ─── AVX2 Original (fallback para shapes pequenos) ──────────────────────

#[cfg(all(target_arch = "x86_64", not(target_os = "none")))]
fn unpack_row_into(weight: &PackedTernaryTensor, row: usize, n: usize, buf: &mut [i8]) {
    if n % 4 == 0 {
        let words = n / 4;
        let row_start = row * words;
        for pw in 0..words {
            let p = weight.packed_data[row_start + pw];
            let base = pw * 4;
            // Branchless unpack: (pair&1) - (pair>>1) — same as AVX-512 pattern
            // 0b00→0, 0b01→1, 0b10→-1, 0b11→0  (ADR-0084 F1)
            let p0 = (p & 3) as i8;
            let p1 = ((p >> 2) & 3) as i8;
            let p2 = ((p >> 4) & 3) as i8;
            let p3 = ((p >> 6) & 3) as i8;
            buf[base] = (p0 & 1) - (p0 >> 1);
            buf[base + 1] = (p1 & 1) - (p1 >> 1);
            buf[base + 2] = (p2 & 1) - (p2 >> 1);
            buf[base + 3] = (p3 & 1) - (p3 >> 1);
        }
    } else {
        // Flat packing when n%4 != 0 (e.g. embed vocab=32002)
        let start = row * n;
        for j in 0..n {
            let idx = start + j;
            let byte = idx >> 2;
            let shift = (idx & 3) << 1;
            let bits = (weight.packed_data[byte] >> shift) & 0b11;
            buf[j] = ((bits & 1) as i8) - ((bits >> 1) as i8); // branchless
        }
    }
}

#[cfg(all(target_arch = "x86_64", not(target_os = "none")))]
pub(super) unsafe fn avx2_ternary_matmul_impl(weight: &PackedTernaryTensor, input: &Tensor, m: usize, k: usize, n: usize) -> Tensor {
    use core::arch::x86_64::*;

    let mut result = Tensor::new((m, n));
    let mut row_buf = vec![0i8; n];
    let n8 = n & !7; // maior multiplo de 8 <= n

    for i in 0..m {
        let inp_row = &input.data[i * k..];
        let out_row = &mut result.data[i * n..];
        for j in 0..n {
            out_row[j] = 0.0;
        }

        for t in 0..k {
            unpack_row_into(weight, t, n, &mut row_buf);
            let a = _mm256_set1_ps(inp_row[t]);
            let mut j = 0usize;
            while j < n8 {
                let w_ptr = row_buf.as_ptr().add(j) as *const __m128i;
                let w_i8 = _mm_loadl_epi64(w_ptr);
                let w_i32 = _mm256_cvtepi8_epi32(w_i8);
                let w_f32 = _mm256_cvtepi32_ps(w_i32);
                let prev = _mm256_loadu_ps(out_row.as_mut_ptr().add(j));
                let updated = _mm256_fmadd_ps(a, w_f32, prev);
                _mm256_storeu_ps(out_row.as_mut_ptr().add(j), updated);
                j += 8;
            }
            // cauda n%8 (vocab 32002 → 2 elems) — sem store AVX past-end
            let scale = inp_row[t];
            while j < n {
                out_row[j] += scale * (row_buf[j] as f32);
                j += 1;
            }
        }
    }
    result
}

#[cfg(any(not(target_arch = "x86_64"), target_os = "none"))]
pub(super) unsafe fn avx2_ternary_matmul_impl(
    weight: &PackedTernaryTensor,
    input: &Tensor,
    m: usize,
    k: usize,
    n: usize,
) -> Tensor {
    // Metal: SSE2 ADD/SUB/SKIP (ADR-0101 Onda 0 residual) — não FMA dequant.
    #[cfg(all(target_arch = "x86_64", target_os = "none"))]
    {
        if n >= 4 {
            return crate::bitnet_sse::sse2_ternary_matmul_add_sub_skip(weight, input, m, k, n);
        }
    }
    scalar_ternary_matmul(weight, input, m, k, n)
}

// ─── Cache-aware dispatch ───────────────────────────────────────────────

/// Seleciona implementacao otima baseada no tamanho das matrizes e HW disponivel.
pub fn ternary_matmul_adaptive(weight: &PackedTernaryTensor, input: &Tensor) -> Option<Tensor> {
    // Mesmo caminho seguro que ternary_matmul (bitwise AVX2 desactivado).
    ternary_matmul(weight, input)
}
