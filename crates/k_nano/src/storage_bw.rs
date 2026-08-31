//! Medição de banda de bloco (ADR-0100 T-007–T-011 / IDEA #513).
//! TSC, amostra curta; TCG não dispara PIO longo (hang clássico).

use crate::platform_probe::{hypervisor, HypervisorKind};
use crate::tsc::{rdtsc, TSC_HZ};

/// Setores por amostra (8 KiB) — 256 setores no default antigo travava TCG.
const SAMPLE_SECTORS: usize = 16;

/// TCG: skip honesto. BMIDE 0xC8: **UNSUPPORTED** (T-010) — ATA usa amostra PIO 16 setores no metal.
/// Usado APENAS para benchmark (medir MB/s). NÃO impede o probe ATA.
pub fn skip_measure() -> bool {
    hypervisor() == HypervisorKind::Tcg
}

/// Probe de storage SEMPRE permitido (QEMU, TCG, metal).
/// SESSION_293: skip_measure() bloqueava ATA probe inteiro em TCG → boot
/// sem disco → sem BOOT.LOG → sem cross-boot NSGDB recall.
/// Aprendizado: TCG PIO 16 setores é lento mas NÃO trava; o problema
/// era a benchmark de 256 setores (T-010). Probe usa identify + 1 setor.
pub fn allow_probe() -> bool {
    true
}

/// MB/s a partir de bytes, delta TSC e Hz. `0` se inválido.
pub fn mbs_from_tsc(bytes: u64, dt_cycles: u64, hz: u64) -> u32 {
    if bytes == 0 || dt_cycles == 0 || hz == 0 {
        return 0;
    }
    let mbs = (bytes as u128)
        .saturating_mul(hz as u128)
        / (dt_cycles as u128)
        / 1_000_000;
    mbs.min(u32::MAX as u128) as u32
}

/// Lê `SAMPLE_SECTORS` via callback; TCG → 0 + log CRÍTICO.
pub fn measure_read_mbs(mut read: impl FnMut(&mut [u8]) -> bool) -> u32 {
    if skip_measure() {
        crate::slog_nano!(
            "Storage",
            "bw",
            "CRITICO measure skip (TCG) — nao PIO 256 setores"
        );
        return 0;
    }
    let bytes = SAMPLE_SECTORS * 512;
    let mut buf = alloc::vec![0u8; bytes];
    let t0 = rdtsc();
    if !read(&mut buf) {
        return 0;
    }
    let dt = rdtsc().saturating_sub(t0);
    let mut hz = TSC_HZ.load(core::sync::atomic::Ordering::Relaxed);
    if hz == 0 {
        hz = crate::tsc::cpuid_estimate();
    }
    mbs_from_tsc(bytes as u64, dt, hz).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mbs_one_mb_in_one_sec() {
        assert_eq!(mbs_from_tsc(1_000_000, 2_000_000_000, 2_000_000_000), 1);
    }

    #[test]
    fn mbs_zero_on_bad_dt() {
        assert_eq!(mbs_from_tsc(512, 0, 2_000_000_000), 0);
    }
}
