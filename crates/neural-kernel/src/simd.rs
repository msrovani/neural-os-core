//! SIMD/FPU — delega a k_nano (ADR-0055 OSXSAVE/FeatureGate).

pub fn enable_simd() {
    k_nano::simd::enable_simd();
}

pub fn has_whpx() -> bool {
    k_nano::simd::has_whpx()
}

pub fn has_kvm() -> bool {
    k_nano::simd::has_kvm()
}
