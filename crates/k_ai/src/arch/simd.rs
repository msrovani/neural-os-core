//! ADR-0061: Static SIMD Kernel Dispatch
//! 
//! Runtime dispatch table selecting the best available BitNet ternary matmul kernel
//! based on CPUID features and FeatureGate policy. Zero overhead after init.

use cortex::{tensor::{PackedTernaryTensor, Tensor}, bitnet_avx512, bitnet_avx2};
use core::sync::atomic::{AtomicUsize, Ordering};

/// Ternary matmul kernel signature
pub type TernaryMatmulFn = fn(&PackedTernaryTensor, &Tensor) -> Option<Tensor>;

/// ISA capability level (matches platform_probe::IsaPath)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IsaLevel {
    Scalar = 0,
    Sse42 = 1,
    Avx2 = 2,
    Avx512 = 3,
}

impl IsaLevel {
    pub fn name(self) -> &'static str {
        match self {
            IsaLevel::Scalar => "scalar",
            IsaLevel::Sse42 => "SSE4.2",
            IsaLevel::Avx2 => "AVX2",
            IsaLevel::Avx512 => "AVX-512",
        }
    }
}

/// Static dispatch table — initialized once at boot, then zero-cost calls
static KERNEL_FN: AtomicUsize = AtomicUsize::new(0);
static CURRENT_ISA: AtomicUsize = AtomicUsize::new(IsaLevel::Scalar as usize);

/// Initialize the dispatch table based on hardware capabilities
/// Called once during boot after platform_probe::detect()
pub fn init_dispatch() {
    use k_nano::platform_probe::{gate, allow_avx512, allow_avx2};
    
    let g = gate();
    let isa = if allow_avx512() {
        IsaLevel::Avx512
    } else if allow_avx2() {
        IsaLevel::Avx2
    } else if g.allow_prefetch { // SSE4.2 proxy
        IsaLevel::Sse42
    } else {
        IsaLevel::Scalar
    };
    
    CURRENT_ISA.store(isa as usize, Ordering::Release);
    
    // Select kernel function pointer
    let kernel_ptr = match isa {
        IsaLevel::Avx512 => bitnet_avx512::ternary_matmul_avx512 as TernaryMatmulFn,
        IsaLevel::Avx2 => bitnet_avx2::ternary_matmul as TernaryMatmulFn,
        IsaLevel::Sse42 => bitnet_avx2::ternary_matmul as TernaryMatmulFn, // AVX2 kernel handles SSE4.2 fallback
        IsaLevel::Scalar => scalar_ternary_matmul as TernaryMatmulFn,
    };
    
    KERNEL_FN.store(kernel_ptr as usize, Ordering::Release);
    
    k_nano::slog_kai!("SIMD", "dispatch", "ISA={} kernel={:p}", isa.name(), kernel_ptr as *const ());
}

/// Get the currently selected kernel (zero-cost after init)
#[inline]
pub fn kernel() -> TernaryMatmulFn {
    let ptr = KERNEL_FN.load(Ordering::Acquire);
    if ptr == 0 {
        // Fallback if not initialized
        scalar_ternary_matmul
    } else {
        unsafe { core::mem::transmute::<usize, TernaryMatmulFn>(ptr) }
    }
}

/// Get current ISA level
#[inline]
pub fn current_isa() -> IsaLevel {
    unsafe { core::mem::transmute(CURRENT_ISA.load(Ordering::Acquire) as u8) }
}

/// Scalar fallback kernel (always available)
fn scalar_ternary_matmul(
    weight: &PackedTernaryTensor,
    input: &Tensor,
) -> Option<Tensor> {
    let (k, n) = weight.shape;
    let (m, k2) = input.shape;
    if k != k2 { return None; }
    
    let mut result = Tensor::new((m, n));
    for i in 0..m {
        for j in 0..n {
            let mut sum = 0.0f32;
            for t in 0..k {
                let w = weight.get_weight(t * n + j);
                if w != 0 {
                    sum += w as f32 * input.data[i * k + t];
                }
            }
            result.data[i * n + j] = sum;
        }
    }
    Some(result)
}

/// Self-test: verify dispatch works and produces correct results
#[cfg(test)]
pub fn self_test() -> bool {
    use cortex::tensor::{PackedTernaryTensor, Tensor, quantize_to_packed};
    
    init_dispatch();
    
    // Small test matrix
    let weight_data = [1.0f32, -1.0, 0.0, 1.0, -1.0, 0.0, 1.0, -1.0];
    let weight_tensor = Tensor::from_row_major((2, 4), weight_data.to_vec()).unwrap();
    let weight = quantize_to_packed(&weight_tensor, 0.5);
    
    let input = Tensor::from_row_major((1, 2), vec![1.0, -1.0]).unwrap();
    let expected = [2.0f32, -1.0, -1.0, 2.0];
    
    if let Some(result) = kernel()(&weight, &input) {
        if result.shape != (1, 4) { return false; }
        for j in 0..4 {
            if (result.data[j] - expected[j]).abs() > 1e-5 { return false; }
        }
        true
    } else {
        false
    }
}