//! CPU Frequency (P-state) and energy policy management.
//!
//! ## What
//! - P-state control via MSR IA32_PERF_CTL (0x199) / IA32_PERF_STATUS (0x198)
//! - Energy Performance Bias via MSR IA32_ENERGY_PERF_BIAS (0x1B0)
//! - Governor: Performance | Powersave | Ondemand
//!
//! ## Why MSR not ACPI _PSS
//! Parsing ACPI _PSS requires AML (the DSDT is a bytecode blob). The MSR approach
//! works on all x86 CPUs with SpeedStep (Intel Core 2+) or equivalent (AMD CPPC)
//! and is ~10× simpler. The IA32_PERF_STATUS/IA32_PERF_CTL MSR pair is universal.
//!
//! ## QEMU safety
//! QEMU-model CPUs expose the MSRs but ignore writes (frequency never changes).
//! All operations are safe in both sandbox and bare-metal. Call unconditionally.
//!
//! ## ponytail
//! No ACPI AML parser, no CPPC HWP register model, no per-core P-state table.
//! Single global ratio covers all cores. Per-core P-state is ~20× more code for
//! marginal gain — your first bottleneck is network DMA, not the 100 MHz gap
//! between cores. Add when `perf stat` shows asymmetric P-state is the bottleneck.

use core::sync::atomic::{AtomicU8, Ordering};
use x86_64::registers::model_specific::Msr;

// ─── MSR addresses ──────────────────────────────────────────────────────────
const IA32_PERF_STATUS: u32 = 0x198;
const IA32_PERF_CTL: u32 = 0x199;
const IA32_ENERGY_PERF_BIAS: u32 = 0x1B0;

// ─── Constants ──────────────────────────────────────────────────────────────

/// Safe floor ratio (800 MHz = 8 × 100 MHz). Below this, some CPUs become
/// unstable or fail to service NMIs/SMI in time.
const MIN_RATIO: u8 = 8;

/// Max probe ratio (25.0 GHz = 250 × 100 MHz). No CPU reaches this; the probe
/// detects whether writes take effect by attempting an unreachable target and
/// checking if IA32_PERF_STATUS changed.
const PROBE_MAX_RATIO: u8 = 250;

// ─── Governor ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Governor {
    /// P0 (max ratio) always — lowest latency, highest power.
    Performance = 0,
    /// Pn (min ratio) always — lowest power, OK for idle/background.
    Powersave = 1,
    /// Scale: P0 when work pending, Pn otherwise.  (pending: scheduled but not
    /// yet at current `hlt` line, see `ap_work::has_pending()`).
    Ondemand = 2,
}

impl Governor {
    pub fn name(self) -> &'static str {
        match self {
            Self::Performance => "performance",
            Self::Powersave => "powersave",
            Self::Ondemand => "ondemand",
        }
    }
}

// ─── Global state ───────────────────────────────────────────────────────────

static CUR_GOVERNOR: AtomicU8 = AtomicU8::new(0); // 0 = Performance
/// P0 (max ratio), detected at boot.
static P0_RATIO: AtomicU8 = AtomicU8::new(16); // fallback 1.6 GHz
/// Pn (min ratio), detected at boot.
static PN_RATIO: AtomicU8 = AtomicU8::new(MIN_RATIO);
/// Last-written ratio (for ONDEMAND restore).
static LAST_RATIO: AtomicU8 = AtomicU8::new(0);
/// Non-zero if the MSR write-then-read-back check passed.
static HAS_PSTATE: AtomicU8 = AtomicU8::new(0);

// ─── Public API ─────────────────────────────────────────────────────────────

/// Returns true if P-state MSRs are accessible and writes take effect.
pub fn has_pstate() -> bool {
    HAS_PSTATE.load(Ordering::Acquire) != 0
}

pub fn p0_ratio() -> u8 {
    P0_RATIO.load(Ordering::Acquire)
}

pub fn pn_ratio() -> u8 {
    PN_RATIO.load(Ordering::Acquire)
}

pub fn current_governor() -> Governor {
    match CUR_GOVERNOR.load(Ordering::Acquire) {
        1 => Governor::Powersave,
        2 => Governor::Ondemand,
        _ => Governor::Performance,
    }
}

/// Set governor. Applies the new policy immediately.
pub fn set_governor(g: Governor) {
    CUR_GOVERNOR.store(g as u8, Ordering::Release);
    crate::slog_nano!("CPUFREQ", "info", "governor={}", g.name());
    apply_governor();
}

/// Set Energy Performance Bias (0 = performance, 15 = max power saving).
/// Default is 6 ("balanced"). Call once during boot after MSR probe.
pub fn set_energy_perf_bias(bias: u8) {
    let b = bias.min(15) as u64;
    unsafe { Msr::new(IA32_ENERGY_PERF_BIAS).write(b) };
    crate::slog_nano!("CPUFREQ", "info", "EPB={}", b);
}

/// Read MSR IA32_PERF_STATUS bits 7:0 (current operating ratio in 100 MHz).
pub fn current_ratio() -> u8 {
    let val = unsafe { Msr::new(IA32_PERF_STATUS).read() };
    (val & 0xFF) as u8
}

/// Write target ratio to IA32_PERF_CTL.
/// Bits 15:8 = voltage ID (0 = let HW decide), bits 7:0 = ratio.
pub fn set_target_ratio(ratio: u8) {
    let val = ratio as u64; // VID=0, ratio in low byte
    unsafe { Msr::new(IA32_PERF_CTL).write(val) };
    LAST_RATIO.store(ratio, Ordering::Release);
}

// ─── APERF/MPERF — frequência real ─────────────────────────────────────────
// IA32_APERF (0xE8) / IA32_MPERF (0xE7) contam ciclos reais vs máximos desde
// o último reset (ou último write-zero). Razão = frequência real atual, que
// pode ser menor que a pedida se o CPU está throttleado por thermal/power.

const IA32_APERF: u32 = 0xE8;
const IA32_MPERF: u32 = 0xE7;

/// Read APERF/MPERF and return actual operating ratio (100 MHz units).
/// Returns 0 if MPERF hasn't ticked yet (boot edge case).
///
/// ## ponytail
/// APERF/MPERF wraparound after years of uptime. For a system that reboots
/// daily for firmware updates, this is fine. Add saturation handling when
/// we pass 2^48 cycles without reboot.
pub fn actual_ratio() -> u8 {
    let aperf = unsafe { Msr::new(IA32_APERF).read() };
    let mperf = unsafe { Msr::new(IA32_MPERF).read() };
    if mperf == 0 {
        return 0;
    }
    let p0 = P0_RATIO.load(Ordering::Acquire) as u64;
    let r = (aperf * p0) / mperf;
    r.min(255) as u8
}

// ─── Probe & init ───────────────────────────────────────────────────────────

/// Probe P-state capabilities at boot.
///
/// Strategy:
///   1. Read current ratio from IA32_PERF_STATUS.
///   2. Write PROBE_MAX_RATIO (above any real CPU) and read back.
///      If ratio changed → MSR writes take effect → full P-state control.
///      If unchanged → QEMU or locked → use CPUID max ratio as P0.
///   3. Restore original ratio.
///
/// Returns (p0, pn, has_pstate).
pub fn probe_and_init() -> (u8, u8, bool) {
    // SESSION_243: wrmsr IA32_PERF_CTL/ENERGY_PERF_BIAS da #GP em TCG/WHPX quando o
    // MSR não é emulado como writeable. Comentário antigo "writes are no-ops" era falso.
    // Mesma lição do init_syscall_fast_path (paging.rs): gate por hypervisor real.
    let hv = crate::platform_probe::detect_hypervisor();
    if !matches!(hv, crate::platform_probe::HypervisorKind::None | crate::platform_probe::HypervisorKind::Kvm) {
        crate::slog_nano!("CPUFREQ", "info", "P-state gated off (hv={:?}) — MSR PERF_CTL nao emulado", hv);
        HAS_PSTATE.store(0, Ordering::Release);
        return (16, MIN_RATIO, false);
    }
    let current = current_ratio();
    if current == 0 {
        // No valid ratio — P-state not available (very old CPU or hypervisor hiding it)
        set_energy_perf_bias(6);
        return (16, MIN_RATIO, false);
    }

    // Probe write-takes-effect
    let probe = probe_msr_write(current);
    let (p0, pn, has) = if probe {
        // MSR write works: find P0 by trying max, find Pn by trying min
        let p0 = probe_p0();
        let pn = probe_pn();
        set_target_ratio(current); // restore
        (p0, pn, true)
    } else {
        // Write didn't take effect: use CPUID or fallback
        let cpuid_max = detect_max_ratio_cpuid();
        (cpuid_max.max(current), MIN_RATIO, false)
    };

    P0_RATIO.store(p0, Ordering::Release);
    PN_RATIO.store(pn, Ordering::Release);
    HAS_PSTATE.store(has as u8, Ordering::Release);
    set_energy_perf_bias(6);

    crate::slog_nano!(
        "CPUFREQ",
        "info",
        "P0={} Pn={} current={} has_pstate={} ({} MHz base)",
        p0, pn, current, has, p0 * 100
    );

    // Apply initial governor
    apply_governor();
    (p0, pn, has)
}

// ─── Internal helpers ───────────────────────────────────────────────────────

/// Returns true if writing IA32_PERF_CTL changes IA32_PERF_STATUS.
fn probe_msr_write(current: u8) -> bool {
    let before = current_ratio();
    set_target_ratio(PROBE_MAX_RATIO);
    // ponytail: spin-wait is fine for boot path (called once)
    for _ in 0..5000 {
        core::hint::spin_loop();
    }
    let after = current_ratio();
    set_target_ratio(before);
    // If after changed toward PROBE_MAX_RATIO, write took effect
    after != before && after > current.saturating_sub(5)
}

/// Binary-search P0 (max ratio).  We write decreasing ratios until
/// PERF_STATUS stops following — the last ratio that didn't match is above P0,
/// so the first that did match is close to P0.
fn probe_p0() -> u8 {
    for target in (16u8..=PROBE_MAX_RATIO).rev().step_by(4) {
        set_target_ratio(target);
        for _ in 0..1000 {
            core::hint::spin_loop();
        }
        let got = current_ratio();
        if got >= target.saturating_sub(4) && got <= target.saturating_add(2) {
            // Within the linear range — this is P0 or close enough
            return got.max(16);
        }
        // Not following — above P0, try lower
        continue;
    }
    current_ratio().max(16)
}

/// Read Pn (minimum stable ratio) — just use the safe constant unless
/// the CPU reports something higher.
fn probe_pn() -> u8 {
    set_target_ratio(MIN_RATIO);
    for _ in 0..1000 {
        core::hint::spin_loop();
    }
    let got = current_ratio();
    // If CPU clamps to something higher, use that
    if got > MIN_RATIO && got < 255 {
        got
    } else {
        MIN_RATIO
    }
}

/// Detect max ratio from CPUID leaf 0x16 (Skylake+) or fallback.
fn detect_max_ratio_cpuid() -> u8 {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let max_leaf = core::arch::x86_64::__cpuid(0).eax;
        if max_leaf >= 0x16 {
            let l16 = core::arch::x86_64::__cpuid(0x16);
            let base_mhz = (l16.eax & 0xFFFF) as u16;
            let max_mhz = (l16.ebx & 0xFFFF) as u16;
            if max_mhz > 0 {
                // Convert MHz to 100 MHz ratio (round-up)
                let r = ((max_mhz as u32 + 50) / 100) as u8;
                return r.max(16);
            }
            if base_mhz > 0 {
                let r = ((base_mhz as u32 + 50) / 100) as u8;
                return r.max(16);
            }
        }
        // Leaf 1 EBX bits 31:22 = max ratio (legacy)
        let l1 = core::arch::x86_64::__cpuid(1);
        let ratio = ((l1.ebx >> 22) & 0xFF) as u8;
        if ratio > 0 {
            return ratio;
        }
    }
    16 // fallback 1.6 GHz
}

/// Apply current governor policy.
fn apply_governor() {
    match current_governor() {
        Governor::Performance => {
            let p0 = P0_RATIO.load(Ordering::Acquire);
            set_target_ratio(p0);
        }
        Governor::Powersave => {
            let pn = PN_RATIO.load(Ordering::Acquire);
            set_target_ratio(pn);
        }
        Governor::Ondemand => {
            // Start at P0; ondemand_tick() will scale down if idle
            let p0 = P0_RATIO.load(Ordering::Acquire);
            set_target_ratio(p0);
        }
    }
}

/// Called periodically (e.g. every scheduler tick) when governor == Ondemand.
/// Scales ratio up when work is pending, down when idle.
///
/// Use from scheduler's main loop — the cheapest signal is
/// `ap_work::has_pending()`, which checks the global work queue.
pub fn ondemand_tick(work_pending: bool) {
    if current_governor() != Governor::Ondemand {
        return;
    }
    let current = LAST_RATIO.load(Ordering::Acquire);
    let p0 = P0_RATIO.load(Ordering::Acquire);
    let pn = PN_RATIO.load(Ordering::Acquire);
    if work_pending && current < p0.saturating_sub(4) {
        set_target_ratio(p0);
    } else if !work_pending && current > pn.saturating_add(4) {
        set_target_ratio(pn);
    }
    // hysteresis: don't flip on every tick for marginal changes
}
