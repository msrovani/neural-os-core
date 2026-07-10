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

#[allow(dead_code)]
pub struct TernaryTensor {
    pub shape: (usize, usize),
    pub data: Vec<i8>,
}

impl TernaryTensor {
    #[allow(dead_code)]
    pub fn new(shape: (usize, usize)) -> Self {
        let len = shape.0 * shape.1;
        TernaryTensor {
            shape,
            data: vec![0_i8; len],
        }
    }

    #[allow(dead_code)]
    pub fn from_row_major(shape: (usize, usize), data: Vec<i8>) -> Option<Self> {
        if data.len() != shape.0 * shape.1 {
            return None;
        }
        Some(TernaryTensor { shape, data })
    }

    #[allow(dead_code)]
    pub fn matmul_hybrid(&self, input: &Tensor) -> Option<Tensor> {
        let (k, n) = self.shape;
        let (m, k2) = input.shape;
        if k != k2 {
            return None;
        }
        let mut result = Tensor::new((m, n));
        #[cfg(target_arch = "x86_64")]
        {
            if has_avx2() && k >= 8 {
                return Some(self.matmul_hybrid_avx2(input, m, k, n));
            }
        }
        for i in 0..m {
            for j in 0..n {
                let mut sum = 0.0_f32;
                for t in 0..k {
                    match self.data[t * n + j] {
                        1 => sum += input.data[i * k + t],
                        -1 => sum -= input.data[i * k + t],
                        _ => {}
                    }
                }
                result.data[i * n + j] = sum;
            }
        }
        Some(result)
    }

    #[cfg(target_arch = "x86_64")]
    fn matmul_hybrid_avx2(&self, input: &Tensor, m: usize, k: usize, n: usize) -> Tensor {
        let mut result = Tensor::new((m, n));
        unsafe {
            use core::arch::x86_64::*;
            for i in 0..m {
                let avx_cols = if k >= 8 { (n / 8) * 8 } else { 0 };
                // AVX2 bloco 8 colunas
                for j in (0..avx_cols).step_by(8) {
                    let mut sum = _mm256_setzero_ps();
                    for t in 0..k {
                        let a = _mm256_set1_ps(input.data[i * k + t]);
                        let w_ptr = self.data.as_ptr().add(t * n + j) as *const i8;
                        let w_i8 = _mm_loadl_epi64(w_ptr as *const __m128i);
                        let w_i32 = _mm256_cvtepi8_epi32(w_i8);
                        let b = _mm256_cvtepi32_ps(w_i32);
                        sum = _mm256_fmadd_ps(a, b, sum);
                    }
                    _mm256_storeu_ps(result.data[i * n + j..].as_mut_ptr(), sum);
                }
                // Tail scalar (colunas restantes)
                for j in avx_cols..n {
                    let mut s = 0.0f32;
                    for t in 0..k {
                        match self.data[t * n + j] {
                            1 => s += input.data[i * k + t],
                            -1 => s -= input.data[i * k + t],
                            _ => {}
                        }
                    }
                    result.data[i * n + j] = s;
                }
            }
        }
        result
    }

    fn matmul_hybrid_scalar(&self, input: &Tensor, m: usize, k: usize, n: usize) -> Tensor {
        let mut result = Tensor::new((m, n));
        for i in 0..m {
            for j in 0..n {
                let mut sum = 0.0_f32;
                for t in 0..k {
                    match self.data[t * n + j] {
                        1 => sum += input.data[i * k + t],
                        -1 => sum -= input.data[i * k + t],
                        _ => {}
                    }
                }
                result.data[i * n + j] = sum;
            }
        }
        result
    }
}

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
