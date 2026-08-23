//! Architecture-specific optimizations for AI operations.
//! SIMD (`simd.rs` + cortex avx) nao compila em x86_64-unknown-none soft-float.
pub mod x86_64;
#[cfg(not(target_os = "none"))]
pub mod simd;
#[cfg(target_os = "none")]
pub mod simd {
    pub fn init_dispatch() {}
}
