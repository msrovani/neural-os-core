//! Architecture-specific optimizations for AI operations.
//! x86_64.rs removido (P0): dead code SSE4.2/AVX2 que crashava rustc com soft-float.
//! Dispatch real: cortex::compute::dispatch_ternary.
pub mod simd;
