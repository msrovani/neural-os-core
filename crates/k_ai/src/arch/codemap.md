# k_ai/src/arch — codemap

**Responsibility:** ISA-optimized compute kernels for BitNet ternary matmul. Runtime dispatch only — no compile-time SIMD gating (kernels built unconditionally, chosen at boot by CPUID + FeatureGate policy).

**Key symbols:**
- `x86_64.rs` — `AlignedBuffer<T>` (64-byte aligned), `Ternary` (-1/0/+1), `BitNetKernel` fn-pointer type; SSE4.2/AVX2/AVX-512 ternary kernels (unsafe intrinsics, caller-verified alignment/ISA).
- `simd.rs` — ADR-0061 static dispatch: `IsaLevel` (Scalar/Sse42/Avx2/Avx512), `TernaryMatmulFn`, `init_dispatch()` picks kernel via `k_nano::platform_probe::{gate, allow_avx2, allow_avx512}` (WHFX: no xsave gate); `scalar_ternary_matmul` fallback.

**Integration:** called by cortex tensor matmul paths and k_ai training (`training_agent.rs`); `init_dispatch()` runs once at boot (bin `main.rs` platform init).
