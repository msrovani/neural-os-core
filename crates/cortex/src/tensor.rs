use alloc::vec;
use alloc::vec::Vec;

/// AVX2 permitido pelo FeatureGate (ADR-0055) — não só CPUID.
pub fn has_avx2() -> bool {
    k_nano::platform_probe::allow_avx2()
}

/// Cache size via PlatformProbe CacheTopology (ADR-0055).
pub fn cache_size(level: usize) -> usize {
    k_nano::platform_probe::cache_size(level)
}

/// Block size FlashAttention baseado em L1/L2 detectados.
pub fn optimal_attention_block(hidden: usize) -> usize {
    k_nano::platform_probe::optimal_attention_block(hidden)
}

pub fn has_avx512() -> bool {
    k_nano::platform_probe::allow_avx512()
}

#[derive(Clone)]
pub struct Tensor {
    pub shape: (usize, usize),
    pub data: Vec<f32>,
}

/// Zeros f32 com `checked` + headroom — **nunca** `vec![0.0; a*b]` cru
/// (SESSION_349/351: wrap → OOM TALC ~4.7GB). Refuse → Vec vazio.
pub fn f32_zeros(len: usize) -> alloc::vec::Vec<f32> {
    const MAX_ELEMS: usize = 64 * 1024 * 1024; // 256 MiB f32
    if len == 0 {
        return alloc::vec::Vec::new();
    }
    if len > MAX_ELEMS {
        k_nano::slog_cortex!("Tensor", "fail", "f32_zeros refuse len={}", len);
        k_nano::allocator::note_alloc_refused(
            len.saturating_mul(4),
            k_nano::allocator::heap_window_bytes(),
            "f32_zeros",
        );
        return alloc::vec::Vec::new();
    }
    let Some(bytes) = len.checked_mul(4) else {
        k_nano::slog_cortex!("Tensor", "fail", "f32_zeros overflow len={}", len);
        return alloc::vec::Vec::new();
    };
    if bytes > 1024 * 1024 && !k_nano::allocator::can_alloc_bytes(bytes, 64) {
        k_nano::slog_cortex!(
            "Tensor",
            "warn",
            "f32_zeros headroom refuse len={}MB",
            bytes / (1024 * 1024)
        );
        k_nano::allocator::note_alloc_refused(
            bytes,
            k_nano::allocator::heap_window_bytes(),
            "f32_zeros",
        );
        return alloc::vec::Vec::new();
    }
    let mut v = alloc::vec::Vec::new();
    if v.try_reserve_exact(len).is_err() {
        k_nano::slog_cortex!("Tensor", "fail", "f32_zeros try_reserve len={}", len);
        k_nano::allocator::note_alloc_refused(
            bytes,
            k_nano::allocator::heap_window_bytes(),
            "f32_zeros",
        );
        return alloc::vec::Vec::new();
    }
    v.resize(len, 0.0);
    v
}

/// `a.checked_mul(b)` + `f32_zeros` — substitui `vec![0.0f32; a * b]`.
pub fn f32_zeros_2d(rows: usize, cols: usize) -> alloc::vec::Vec<f32> {
    match rows.checked_mul(cols) {
        Some(n) => f32_zeros(n),
        None => {
            k_nano::slog_cortex!(
                "Tensor",
                "fail",
                "f32_zeros_2d overflow {}x{}",
                rows,
                cols
            );
            alloc::vec::Vec::new()
        }
    }
}

impl Tensor {
    /// Aloca zeros. Overflow `rows*cols` → tensor vazio + slog (SESSION_349:
    /// wrap release produzia `layout.size` primo ~4.4GB e OOM `cortex_llm`).
    pub fn new(shape: (usize, usize)) -> Self {
        let Some(len) = shape.0.checked_mul(shape.1) else {
            k_nano::slog_cortex!(
                "Tensor",
                "fail",
                "shape overflow {}x{} — refuse alloc",
                shape.0,
                shape.1
            );
            return Tensor {
                shape: (0, 0),
                data: alloc::vec::Vec::new(),
            };
        };
        // Cap honesto: >2 GiB f32 (~512M elems) nunca cabe no bump window ~2GB.
        if len > (512 * 1024 * 1024) {
            k_nano::slog_cortex!(
                "Tensor",
                "fail",
                "shape too large {}x{} elems={} — refuse alloc",
                shape.0,
                shape.1,
                len
            );
            k_nano::allocator::note_alloc_refused(
                len.saturating_mul(4),
                k_nano::allocator::heap_window_bytes(),
                "tensor",
            );
            return Tensor {
                shape: (0, 0),
                data: alloc::vec::Vec::new(),
            };
        }
        let bytes = len.saturating_mul(4);
        // AIOS Observe pró-ativo: não entrar no grow→OOM se headroom não cobre.
        if bytes > 1024 * 1024 && !k_nano::allocator::can_alloc_bytes(bytes, 64) {
            k_nano::slog_cortex!(
                "Tensor",
                "warn",
                "headroom refuse {}x{} bytes={}MB",
                shape.0,
                shape.1,
                bytes / (1024 * 1024)
            );
            k_nano::allocator::note_alloc_refused(
                bytes,
                k_nano::allocator::heap_window_bytes(),
                "tensor",
            );
            return Tensor {
                shape: (0, 0),
                data: alloc::vec::Vec::new(),
            };
        }
        // SESSION_351: f32_zeros pode falhar pós-can_alloc → shape cheio + data vazia
        // é OOB no attn. Invariante: data.len() == rows*cols, senão (0,0).
        let data = f32_zeros(len);
        if data.len() != len {
            k_nano::slog_cortex!(
                "Tensor",
                "fail",
                "new refuse after f32_zeros {}x{} got={}",
                shape.0,
                shape.1,
                data.len()
            );
            return Tensor {
                shape: (0, 0),
                data: alloc::vec::Vec::new(),
            };
        }
        Tensor { shape, data }
    }

    /// True se o tensor tem dados e o shape bate com `data.len()`.
    /// Um tensor vazio `(0,0)` (OOM, overflow, refuse) NÃO é válido — callers devem
    /// propagar `None`/erro em vez de distribuir zero. (SESSION_362+)
    #[inline]
    pub fn is_valid(&self) -> bool {
        match self.shape.0.checked_mul(self.shape.1) {
            Some(n) => n > 0 && n == self.data.len(),
            None => false,
        }
    }

    /// Creates a zero-initialized tensor of the given shape (fallback for failed ops).
    pub fn zero(shape: (usize, usize)) -> Self {
        Self::new(shape)
    }

    pub fn from_row_major(shape: (usize, usize), data: Vec<f32>) -> Option<Self> {
        let Some(need) = shape.0.checked_mul(shape.1) else {
            return None;
        };
        if data.len() != need {
            return None;
        }
        Some(Tensor { shape, data })
    }

    pub fn matmul(&self, other: &Tensor) -> Option<Tensor> {
        let (m, k) = self.shape;
        let (k2, n) = other.shape;
        if k != k2 || !self.is_valid() || !other.is_valid() {
            return None;
        }
        let mut result = Tensor::new((m, n));
        if !result.is_valid() {
            return None;
        }
        // ADR-0057 WS-B: f32 grande distribui linhas entre P-cores (APs), só
        // quando os APs são workers vivos (`ap_pollable`, WS-F).
        if m >= 8
            && n >= 8
            && k >= 8
            && k_nano::platform_probe::allow_smp()
            && k_nano::smp::ap_pollable()
            && k_nano::smp::ap_entry_count() > 0
        {
            if let Some(r) = crate::parallel_matmul::parallel_matmul(self, other) {
                return Some(r);
            }
        }
        #[cfg(all(target_arch = "x86_64", not(target_os = "none")))]
        {
            if has_avx2() && k >= 8 && n >= 8 {
                return Some(self.matmul_avx2_inner(other, m, k, n));
            }
        }
        for i in 0..m {
            for j in 0..n {
                let mut sum = 0.0_f32;
                for t in 0..k {
                    sum += self.data[i * k + t] * other.data[t * n + j];
                }
                result.data[i * n + j] = sum;
            }
        }
        Some(result)
    }

    /// AVX2 matmul: broadcast 1 input × 8 weights, soma em j-blocks.
    /// Só executa quando k%8==0 e n%8==0; caso contrário usa scalar.
    #[cfg(all(target_arch = "x86_64", not(target_os = "none")))]
    fn matmul_avx2_inner(&self, other: &Tensor, m: usize, k: usize, n: usize) -> Tensor {
        let mut result = Tensor::new((m, n));
        let avx_cols = if k >= 8 { (n / 8) * 8 } else { 0 };
        if avx_cols > 0 {
            unsafe {
                use core::arch::x86_64::*;
                for i in 0..m {
                    // AVX2: blocos completos de 8 colunas
                    for j in (0..avx_cols).step_by(8) {
                        let mut sum = _mm256_setzero_ps();
                        for t in 0..k {
                            let a = _mm256_set1_ps(self.data[i * k + t]);
                            let b = _mm256_loadu_ps(other.data[t * n + j..].as_ptr());
                            sum = _mm256_fmadd_ps(a, b, sum);
                        }
                        _mm256_storeu_ps(result.data[i * n + j..].as_mut_ptr(), sum);
                    }
                    // Tail scalar
                    for j in avx_cols..n {
                        let mut s = 0.0f32;
                        for t in 0..k {
                            s += self.data[i * k + t] * other.data[t * n + j];
                        }
                        result.data[i * n + j] = s;
                    }
                }
            }
        } else {
            // k < 8: scalar puro
            for i in 0..m {
                for j in 0..n {
                    let mut s = 0.0f32;
                    for t in 0..k {
                        s += self.data[i * k + t] * other.data[t * n + j];
                    }
                    result.data[i * n + j] = s;
                }
            }
        }
        result
    }

    fn matmul_scalar(&self, other: &Tensor, m: usize, k: usize, n: usize) -> Tensor {
        let mut result = Tensor::new((m, n));
        for i in 0..m {
            for j in 0..n {
                let mut s = 0.0f32;
                for t in 0..k {
                    s += self.data[i * k + t] * other.data[t * n + j];
                }
                result.data[i * n + j] = s;
            }
        }
        result
    }

    #[allow(dead_code)]
    pub fn add_scalar(&mut self, scalar: f32) {
        for x in self.data.iter_mut() {
            *x += scalar;
        }
    }

    #[allow(dead_code)]
    pub fn mul_scalar(&mut self, scalar: f32) {
        for x in self.data.iter_mut() {
            *x *= scalar;
        }
    }

    pub fn apply<F>(&mut self, f: F)
    where
        F: Fn(f32) -> f32,
    {
        for x in self.data.iter_mut() {
            *x = f(*x);
        }
    }

    pub fn transposed(&self) -> Self {
        let (rows, cols) = self.shape;
        if !self.is_valid() {
            return Tensor {
                shape: (0, 0),
                data: alloc::vec::Vec::new(),
            };
        }
        let mut data = f32_zeros_2d(cols, rows);
        if data.len() != cols.saturating_mul(rows) {
            return Tensor {
                shape: (0, 0),
                data: alloc::vec::Vec::new(),
            };
        }
        for i in 0..rows {
            for j in 0..cols {
                data[j * rows + i] = self.data[i * cols + j];
            }
        }
        Tensor { shape: (cols, rows), data }
    }

    pub fn add(&self, other: &Tensor) -> Option<Tensor> {
        if self.shape != other.shape { return None; }
        let mut data = self.data.clone();
        for (a, b) in data.iter_mut().zip(other.data.iter()) {
            *a += b;
        }
        Some(Tensor { shape: self.shape, data })
    }

    pub fn element_mul(&self, other: &Tensor) -> Option<Tensor> {
        if self.shape != other.shape { return None; }
        let mut data = self.data.clone();
        for (a, b) in data.iter_mut().zip(other.data.iter()) {
            *a *= b;
        }
        Some(Tensor { shape: self.shape, data })
    }
}

#[repr(C, align(64))]
pub struct PackedTernaryTensor {
    pub shape: (usize, usize),
    pub packed_data: Vec<u8>,
}

impl PackedTernaryTensor {
    fn encode_weight(v: i8) -> u8 {
        match v {
            -1 => 0b10,
            0 => 0b00,
            1 => 0b01,
            _ => 0b00,
        }
    }

    fn decode_weight(bits: u8) -> i8 {
        match bits & 0b11 {
            0b00 => 0,
            0b01 => 1,
            0b10 => -1,
            _ => 0,
        }
    }

    pub fn pack_weights(weights: &[i8]) -> Vec<u8> {
        let packed_len = (weights.len() + 3) / 4;
        let mut packed = vec![0u8; packed_len];
        for (i, &w) in weights.iter().enumerate() {
            let byte_idx = i / 4;
            let bit_pos = (i % 4) * 2;
            packed[byte_idx] |= Self::encode_weight(w) << bit_pos;
        }
        packed
    }

    pub fn get_weight(&self, index: usize) -> i8 {
        let byte_idx = index / 4;
        let Some(&byte) = self.packed_data.get(byte_idx) else {
            return 0;
        };
        let bit_pos = (index % 4) * 2;
        let bits = (byte >> bit_pos) & 0b11;
        Self::decode_weight(bits)
    }

    pub fn matmul_hybrid(&self, input: &Tensor) -> Option<Tensor> {
        if !input.is_valid() {
            return None;
        }
        crate::bitnet_avx2::ternary_matmul(self, input)
    }
}

const CODEBOOK_SIZE: usize = 16;

pub struct CodebookVQ {
    pub codebook: Vec<f32>,
    pub codes: Vec<u8>,
}

impl CodebookVQ {
    pub fn train(data: &[f32], size: usize) -> Vec<f32> {
        let mut cb = vec![0.0f32; size];
        let step = data.len() / size;
        for i in 0..size {
            let start = i * step;
            let end = (i + 1) * step;
            cb[i] = data[start..end.min(data.len())].iter().sum::<f32>() / (end - start).max(1) as f32;
        }
        cb
    }

    pub fn new(data: &[f32]) -> Self {
        let codebook = Self::train(data, CODEBOOK_SIZE);
        let mut codes = Vec::with_capacity(data.len());
        for &v in data {
            let mut best = 0;
            let mut best_d = (v - codebook[0]).abs();
            for (j, &c) in codebook.iter().enumerate().skip(1) {
                let d = (v - c).abs();
                if d < best_d { best_d = d; best = j; }
            }
            codes.push(best as u8);
        }
        CodebookVQ { codebook, codes }
    }

    pub fn compress(&self) -> &[u8] { &self.codes }

    pub fn decompress(&self) -> Vec<f32> {
        self.codes.iter().map(|&c| self.codebook[c as usize]).collect()
    }

    pub fn ratio(&self) -> f32 {
        (self.codes.len() as f32 * core::mem::size_of::<u8>() as f32)
            / (self.codes.len() as f32 * core::mem::size_of::<f32>() as f32)
    }
}

/// Resultado da predição do HW Expert v4 multi-head.
/// Espelha os 5 heads do modelo BitNet.
pub struct HwPrediction {
    pub family_id: u8,    // 0-16 (17 classes)
    pub fw_id: u8,        // 0-7 (8 firmwares)
    pub agent_id: u8,     // 0-8 (9 agents)
    pub caps_bits: u32,   // 10-bit bitmap
    pub next_action: u8,  // 0-8 (9 actions)
}

pub fn quantize_to_packed(tensor: &Tensor, threshold: f32) -> PackedTernaryTensor {
    let mut ternary = Vec::with_capacity(tensor.data.len());
    for &val in tensor.data.iter() {
        let q = if val > threshold {
            1_i8
        } else if val < -threshold {
            -1_i8
        } else {
            0_i8
        };
        ternary.push(q);
    }
    let packed = PackedTernaryTensor::pack_weights(&ternary);
    PackedTernaryTensor {
        shape: tensor.shape,
        packed_data: packed,
    }
}
