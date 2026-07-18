use alloc::vec;
use alloc::vec::Vec;

pub fn has_avx2() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        unsafe {
            let leaf1 = core::arch::x86_64::__cpuid(1);
            let has_hypervisor = (leaf1.ecx & (1 << 31)) != 0;
            let leaf7 = core::arch::x86_64::__cpuid(7);
            let has_avx2 = (leaf7.ebx & (1 << 5)) != 0;
            if !has_avx2 { return false; }
            if has_hypervisor {
                let hv = core::arch::x86_64::__cpuid(0x40000000);
                let vendor: [u8; 12] = [
                    (hv.ebx >> 0) as u8, (hv.ebx >> 8) as u8, (hv.ebx >> 16) as u8, (hv.ebx >> 24) as u8,
                    (hv.ecx >> 0) as u8, (hv.ecx >> 8) as u8, (hv.ecx >> 16) as u8, (hv.ecx >> 24) as u8,
                    (hv.edx >> 0) as u8, (hv.edx >> 8) as u8, (hv.edx >> 16) as u8, (hv.edx >> 24) as u8,
                ];
                if &vendor[..9] == b"TCGTCGTCG" { return false; } // Só bloqueia TCG puro
            }
            true
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    { false }
}

/// Detecta tamanho do cache L1 Data via CPUID leaf 0x04 (Intel) ou 0x8000_001D (AMD).
/// Retorna bytes do cache de data do nivel especificado (0=L1D, 1=L1I, 2=L2, 3=L3).
pub fn cache_size(level: usize) -> usize {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        // Tenta Intel leaf 0x04 (com subleaf)
        let info = core::arch::x86_64::__cpuid_count(0x04, level as u32);
        if info.eax & 0x1F != 0 {
            let ways: u32 = ((info.ebx >> 22) & 0x3FF) + 1;
            let partitions: u32 = ((info.ebx >> 12) & 0x3FF) + 1;
            let line_size: u32 = (info.ebx & 0xFFF) + 1;
            let sets: u32 = info.ecx + 1;
            return (ways * partitions * line_size * sets) as usize;
        }
        // Fallback: AMD leaf 0x8000_001D
        let amd = core::arch::x86_64::__cpuid_count(0x8000_001D, level as u32);
        if amd.eax & 0x1F != 0 {
            let ways: u32 = ((amd.ebx >> 22) & 0x3FF) + 1;
            let partitions: u32 = ((amd.ebx >> 12) & 0x3FF) + 1;
            let line_size: u32 = (amd.ebx & 0xFFF) + 1;
            let sets: u32 = amd.ecx + 1;
            return (ways * partitions * line_size * sets) as usize;
        }
    }
    // Fallback seguro: assume L1D=32KB, L2=256KB, L3=4MB
    match level {
        0 => 32768,
        2 => 262144,
        3 => 4194304,
        _ => 32768,
    }
}

/// Calcula block size otimo para FlashAttention baseado no cache L1/L2 detectado.
/// block_size = quantos tokens cabem no cache sem estourar
pub fn optimal_attention_block(hidden: usize) -> usize {
    // Prioriza L1 data cache
    let l1 = cache_size(0);
    let qk_bytes = hidden * 2 * 4; // Q e K em f32
    // Precisamos de espaco para Q(block) + K(block) + resultado(block^2)
    let needed_per_token = qk_bytes + 4; // overhead resultado
    let block_by_l1 = l1 / needed_per_token;

    if block_by_l1 >= 4 {
        return block_by_l1.min(64);
    }
    // Se L1 for muito pequeno, usa L2
    let l2 = cache_size(2);
    let block_by_l2 = l2 / needed_per_token;
    block_by_l2.min(128).max(4)
}

pub fn has_avx512() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        unsafe {
            let result = core::arch::x86_64::__cpuid(7);
            (result.ebx & (1 << 16)) != 0
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    { false }
}

pub struct Tensor {
    pub shape: (usize, usize),
    pub data: Vec<f32>,
}

impl Tensor {
    pub fn new(shape: (usize, usize)) -> Self {
        let len = shape.0 * shape.1;
        Tensor {
            shape,
            data: vec![0.0; len],
        }
    }

    pub fn from_row_major(shape: (usize, usize), data: Vec<f32>) -> Option<Self> {
        if data.len() != shape.0 * shape.1 {
            return None;
        }
        Some(Tensor { shape, data })
    }

    pub fn matmul(&self, other: &Tensor) -> Option<Tensor> {
        let (m, k) = self.shape;
        let (k2, n) = other.shape;
        if k != k2 {
            return None;
        }
        let mut result = Tensor::new((m, n));
        #[cfg(target_arch = "x86_64")]
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
    #[cfg(target_arch = "x86_64")]
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
        let mut data = vec![0.0_f32; rows * cols];
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
        let bit_pos = (index % 4) * 2;
        let bits = (self.packed_data[byte_idx] >> bit_pos) & 0b11;
        Self::decode_weight(bits)
    }

    pub fn matmul_hybrid(&self, input: &Tensor) -> Option<Tensor> {
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
