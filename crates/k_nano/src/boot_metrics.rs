//! Per-CPU boot metrics + init timings (plan lane 3, observability).
//!
//! Lock-free statics only (Atomics, no alloc, no Mutex): per-phase TSC entry
//! stamps + per-CPU fault/alloc counters. Zero dependency on cortex/hermes —
//! works when cognitive services never init.
//!
//! Hook sites (one-liners, no behavior change):
//! - `boot_report::emit_phase_banner_with_err` → [`note_phase_entry`]
//! - `interrupts::page_fault_handler` → [`note_pf`] / [`note_fault_in`]
//!   (next to the existing `PAGE_FAULT_COUNT` counter)
//! - [`note_dma`] is exposed as a seam for DMA drivers (no existing DMA
//!   counter to mirror — intentionally unwired until one exists).
//!
//! Validity contract (reviewer refinement — bounded hardening):
//! - TSC subtraction across CPUs is INVALID where the TSC is unsynchronized /
//!   non-invariant. Every phase-entry stamp is tagged with its origin CPU
//!   ([`PHASE_CPU`]); a span is only computed when both endpoints share the
//!   same CPU id AND the later stamp is ordered (`t1 >= t0`, so wrap/backwards
//!   can never produce a number). Cross-CPU or backwards spans are skipped;
//!   when no same-CPU span exists the line reports `invalid` (spans existed
//!   but none comparable) — never a fabricated delta.
//! - Unavailable (`n/a`) ≠ 0: [`Option`]-style. Fewer than 2 entered phases →
//!   `slowest_phase=none` + `dtsc_ticks=n/a` + `dus_us=n/a`. Uncalibrated TSC
//!   (`TSC_HZ == 0`) or non-invariant TSC → `tsc_valid=0` and `dus_us=n/a`
//!   (a `us` value without a trusted rate would be a lie, so it is withheld).
//! - Units are explicit in the field names: `dtsc_ticks` (raw TSC ticks),
//!   `dus_us` (microseconds). `tsc_valid=1` only when the TSC is invariant
//!   (CPUID 0x80000007 EDX bit 8) AND calibrated (`TSC_HZ != 0`).
//! - Overflow/wrap: counters saturate at `u64::MAX` (CAS loop, never wrap)
//!   and latch a bit in [`METRICS_OV`]; the line carries `ov=1` once any
//!   counter saturated. TSC wrap/backwards is fail-closed (span skipped).
//! - Deterministic output: fixed field order
//!   `slowest_phase dtsc_ticks dus_us tsc_valid ov top`, stable tie-breaks
//!   (slowest: later span wins ties, as before; top-3: fixed priority order
//!   `pf fault_ok fault_fail dma_ok dma_fail`, first-wins on ties).

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, AtomicU8, Ordering};

/// Phases 0..=8 (matches `boot_report` PHASE_RANK).
pub const MAX_PHASES: usize = 9;
/// Per-CPU lanes. ponytail: fixed 8 (SMP max), real AP ids wired later.
pub const MAX_CPUS: usize = 8;
/// Sentinel for "phase never entered" in [`PHASE_CPU`].
const CPU_EMPTY: u8 = 0xFF;

/// Overflow latch bits in [`METRICS_OV`] (one per saturating counter).
pub const OV_PF: u64 = 1 << 0;
pub const OV_FAULT_OK: u64 = 1 << 1;
pub const OV_FAULT_FAIL: u64 = 1 << 2;
pub const OV_DMA_OK: u64 = 1 << 3;
pub const OV_DMA_FAIL: u64 = 1 << 4;

/// Raw-TSC stamp at phase entry (0 = phase never entered).
static PHASE_TSC: [AtomicU64; MAX_PHASES] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];

/// Origin CPU id per phase-entry stamp (`CPU_EMPTY` = never entered).
/// Written once by the first-writer-wins path in [`note_phase_entry_at`].
static PHASE_CPU: [AtomicU8; MAX_PHASES] = [
    AtomicU8::new(CPU_EMPTY),
    AtomicU8::new(CPU_EMPTY),
    AtomicU8::new(CPU_EMPTY),
    AtomicU8::new(CPU_EMPTY),
    AtomicU8::new(CPU_EMPTY),
    AtomicU8::new(CPU_EMPTY),
    AtomicU8::new(CPU_EMPTY),
    AtomicU8::new(CPU_EMPTY),
    AtomicU8::new(CPU_EMPTY),
];

/// Latched overflow bits ([`OV_PF`]…): set when a counter saturates at
/// `u64::MAX` instead of wrapping. Fail-closed honesty: saturation keeps the
/// max value AND records that it happened.
static METRICS_OV: AtomicU64 = AtomicU64::new(0);

macro_rules! cpu_counters {
    ($name:ident) => {
        static $name: [AtomicU64; MAX_CPUS] = [
            AtomicU64::new(0),
            AtomicU64::new(0),
            AtomicU64::new(0),
            AtomicU64::new(0),
            AtomicU64::new(0),
            AtomicU64::new(0),
            AtomicU64::new(0),
            AtomicU64::new(0),
        ];
    };
}

cpu_counters!(PF);
cpu_counters!(FAULT_OK);
cpu_counters!(FAULT_FAIL);
cpu_counters!(DMA_OK);
cpu_counters!(DMA_FAIL);

#[inline]
fn lane(a: &[AtomicU64; MAX_CPUS], cpu: u8) -> &AtomicU64 {
    &a[(cpu as usize) % MAX_CPUS]
}

/// Saturating increment: caps at `u64::MAX` and latches `bit` in
/// [`METRICS_OV`] instead of wrapping to 0 (which would forge a small count).
fn sat_inc(slot: &AtomicU64, bit: u64) {
    let mut cur = slot.load(Ordering::Relaxed);
    loop {
        if cur == u64::MAX {
            METRICS_OV.fetch_or(bit, Ordering::Relaxed);
            return;
        }
        match slot.compare_exchange_weak(cur, cur + 1, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(v) => cur = v,
        }
    }
}

/// Best-effort current CPU id.
// ponytail: single id (BSP=0) until SMP wires real AP ids; arrays are ready.
#[inline]
pub fn this_cpu() -> u8 {
    0
}

/// Record phase-entry TSC stamp + origin CPU. First stamp wins (banner may
/// re-emit). The CPU tag is stored only by the TSC CAS winner, so stamp and
/// tag always belong to the same writer.
pub fn note_phase_entry(n: u8) {
    note_phase_entry_at(n, crate::tsc::rdtsc(), this_cpu());
}

/// Test/deterministic injection: explicit stamp + CPU with first-wins
/// semantics (0 stamp coerced to 1 to preserve the 0 = never-entered sentinel).
pub fn note_phase_entry_at(n: u8, t: u64, cpu: u8) {
    if (n as usize) >= MAX_PHASES {
        return;
    }
    let t = t.max(1);
    match PHASE_TSC[n as usize].compare_exchange(0, t, Ordering::AcqRel, Ordering::Relaxed) {
        Ok(_) => PHASE_CPU[n as usize].store(cpu, Ordering::Release),
        Err(_) => {}
    }
}

/// Raw entry stamp for phase `n` (0 = never entered).
pub fn phase_tsc(n: u8) -> u64 {
    if (n as usize) >= MAX_PHASES {
        return 0;
    }
    PHASE_TSC[n as usize].load(Ordering::Acquire)
}

/// Origin CPU of phase `n`'s entry stamp (`None` = never entered / OOB).
pub fn phase_cpu(n: u8) -> Option<u8> {
    if (n as usize) >= MAX_PHASES {
        return None;
    }
    if PHASE_TSC[n as usize].load(Ordering::Acquire) == 0 {
        return None;
    }
    match PHASE_CPU[n as usize].load(Ordering::Acquire) {
        CPU_EMPTY => None,
        c => Some(c),
    }
}

/// #PF observed on `cpu` (saturating).
pub fn note_pf(cpu: u8) {
    sat_inc(lane(&PF, cpu), OV_PF);
}

/// Demand-page fault-in outcome on `cpu` (`true` = cured; saturating).
pub fn note_fault_in(ok: bool, cpu: u8) {
    if ok {
        sat_inc(lane(&FAULT_OK, cpu), OV_FAULT_OK);
    } else {
        sat_inc(lane(&FAULT_FAIL, cpu), OV_FAULT_FAIL);
    }
}

/// DMA outcome on `cpu` (`true` = ok; saturating). Seam: no caller yet (no DMA
/// counter exists).
pub fn note_dma(ok: bool, cpu: u8) {
    if ok {
        sat_inc(lane(&DMA_OK, cpu), OV_DMA_OK);
    } else {
        sat_inc(lane(&DMA_FAIL, cpu), OV_DMA_FAIL);
    }
}

fn sum(a: &[AtomicU64; MAX_CPUS]) -> u64 {
    let mut t = 0u64;
    for i in 0..MAX_CPUS {
        t = t.saturating_add(a[i].load(Ordering::Relaxed));
    }
    t
}

pub fn pf_total() -> u64 {
    sum(&PF)
}
pub fn fault_ok_total() -> u64 {
    sum(&FAULT_OK)
}
pub fn fault_fail_total() -> u64 {
    sum(&FAULT_FAIL)
}
pub fn dma_ok_total() -> u64 {
    sum(&DMA_OK)
}
pub fn dma_fail_total() -> u64 {
    sum(&DMA_FAIL)
}

/// Latched overflow bits ([`OV_PF`]…); 0 = no counter ever saturated.
pub fn metrics_overflow() -> u64 {
    METRICS_OV.load(Ordering::Relaxed)
}

/// True once any counter saturated instead of wrapping.
pub fn metrics_overflowed() -> bool {
    metrics_overflow() != 0
}

/// Invariant-TSC bit (CPUID 0x80000007 EDX[8]): the TSC ticks at a constant
/// rate across P/C-states. Without it, cross-CPU deltas are meaningless and
/// even same-CPU `us` conversion is untrusted.
pub fn tsc_invariant() -> bool {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let max = core::arch::x86_64::__cpuid(0x8000_0000).eax;
        if max < 0x8000_0007 {
            return false;
        }
        (core::arch::x86_64::__cpuid(0x8000_0007).edx & (1 << 8)) != 0
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

/// True once [`crate::tsc::TSC_HZ`] holds a measured rate (pre-calibration
/// the `us` unit has no basis, so it must render `n/a`).
pub fn tsc_calibrated() -> bool {
    crate::tsc::TSC_HZ.load(Ordering::Relaxed) != 0
}

/// Full trust flag for the `us` unit: invariant AND calibrated.
pub fn tsc_valid() -> bool {
    tsc_invariant() && tsc_calibrated()
}

/// Pure ticks→µs conversion against an explicit rate: `None` when the rate
/// is unknown (0) — unavailable renders `n/a`, never 0.
pub fn ticks_to_us_with_hz(ticks: u64, hz: u64) -> Option<u64> {
    if hz == 0 {
        return None;
    }
    Some(((ticks as u128) * 1_000_000 / (hz as u128)) as u64)
}

/// Live ticks→µs conversion against the calibrated rate (`None` while
/// uncalibrated).
pub fn ticks_to_us_opt(ticks: u64) -> Option<u64> {
    ticks_to_us_with_hz(ticks, crate::tsc::TSC_HZ.load(Ordering::Relaxed))
}

/// A duration field on the SCORE line: a real number, unavailable (`n/a`),
/// or proven-incomparable (`invalid`). Unavailable ≠ 0 — `n/a` is never a
/// number and a number is never a placeholder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurField {
    /// Comparable same-CPU delta (TSC ticks, or converted µs).
    Num(u64),
    /// No basis for a value (< 2 entered phases, or uncalibrated rate).
    Unavailable,
    /// Spans exist but none are same-CPU comparable — never a number.
    Invalid,
}

impl DurField {
    fn render(&self) -> String {
        match *self {
            DurField::Num(v) => alloc::format!("{}", v),
            DurField::Unavailable => alloc::format!("n/a"),
            DurField::Invalid => alloc::format!("invalid"),
        }
    }

    fn parse(s: &str) -> Option<DurField> {
        match s {
            "n/a" => Some(DurField::Unavailable),
            "invalid" => Some(DurField::Invalid),
            _ => s.parse::<u64>().ok().map(DurField::Num),
        }
    }
}

/// Slowest entered phase span, same-CPU only.
/// - `phase = Some(n)` + `dtsc = Num(d)`: span starting at phase `n` took
///   `d` TSC ticks (both endpoints on one CPU, ordered).
/// - `phase = None` + `dtsc = Unavailable`: fewer than 2 phases entered.
/// - `phase = None` + `dtsc = Invalid`: spans exist but every one is
///   cross-CPU or backwards — no comparable delta, and none is fabricated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlowestSpan {
    pub phase: Option<u8>,
    pub dtsc: DurField,
}

pub fn slowest_span() -> SlowestSpan {
    let mut prev_t: u64 = 0;
    let mut prev_cpu: u8 = CPU_EMPTY;
    let mut prev_n: u8 = u8::MAX;
    let mut entered: u32 = 0;
    let mut best_n: Option<u8> = None;
    let mut best_d: u64 = 0;
    let mut any_valid = false;
    for n in 0..MAX_PHASES {
        let t = PHASE_TSC[n].load(Ordering::Acquire);
        if t == 0 {
            continue;
        }
        entered += 1;
        let c = PHASE_CPU[n].load(Ordering::Acquire);
        if prev_t != 0 {
            // Same-CPU AND ordered: the only comparable case. Cross-CPU,
            // untagged (transient CPU_EMPTY), or backwards/wrapped spans are
            // skipped — never turned into a number.
            if prev_cpu != CPU_EMPTY && c != CPU_EMPTY && c == prev_cpu && t >= prev_t {
                let d = t - prev_t;
                // Stable tie-break (legacy): later span wins ties (`>=`).
                if !any_valid || d >= best_d {
                    best_d = d;
                    best_n = Some(prev_n);
                    any_valid = true;
                }
            }
        }
        prev_t = t;
        prev_cpu = c;
        prev_n = n as u8;
    }
    if entered < 2 {
        SlowestSpan {
            phase: None,
            dtsc: DurField::Unavailable,
        }
    } else if any_valid {
        SlowestSpan {
            phase: best_n,
            dtsc: DurField::Num(best_d),
        }
    } else {
        SlowestSpan {
            phase: None,
            dtsc: DurField::Invalid,
        }
    }
}

/// Legacy (phase index, delta TSC ticks) of the slowest SAME-CPU span.
/// Returns `(u8::MAX, 0)` when fewer than 2 phases were entered OR when no
/// span is same-CPU comparable — use [`slowest_span`] to distinguish
/// unavailable from invalid.
pub fn slowest_phase() -> (u8, u64) {
    match slowest_span() {
        SlowestSpan {
            phase: Some(n),
            dtsc: DurField::Num(d),
        } => (n, d),
        _ => (u8::MAX, 0),
    }
}

/// Counter names in fixed priority order (top-3 tie-break: first-wins).
pub const COUNTER_NAMES: [&str; 5] = ["pf", "fault_ok", "fault_fail", "dma_ok", "dma_fail"];

/// Snapshot behind one SCORE line (pure input for [`render_score_line`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScoreSnapshot {
    pub span: SlowestSpan,
    /// Converted µs for the slowest span (`Unavailable` while `tsc_valid`
    /// is false; mirrors `Invalid` when the span itself is invalid).
    pub dus: DurField,
    pub tsc_valid: bool,
    pub overflow: bool,
    pub counters: [u64; 5],
}

impl ScoreSnapshot {
    fn counter(&self, i: usize) -> u64 {
        self.counters[i]
    }
}

/// Read live statics into a [`ScoreSnapshot`] (the only impure step; the
/// render itself is pure and deterministic).
pub fn capture_snapshot() -> ScoreSnapshot {
    let span = slowest_span();
    let valid = tsc_valid();
    let dus = match span.dtsc {
        DurField::Num(d) => match (valid, ticks_to_us_opt(d)) {
            (true, Some(us)) => DurField::Num(us),
            _ => DurField::Unavailable,
        },
        DurField::Unavailable => DurField::Unavailable,
        DurField::Invalid => DurField::Invalid,
    };
    ScoreSnapshot {
        span,
        dus,
        tsc_valid: valid,
        overflow: metrics_overflowed(),
        counters: [
            pf_total(),
            fault_ok_total(),
            fault_fail_total(),
            dma_ok_total(),
            dma_fail_total(),
        ],
    }
}

/// Pure, deterministic SCORE render. Fixed field order
/// (`slowest_phase dtsc_ticks dus_us tsc_valid ov top`); top-3 by value with
/// stable ties (fixed priority order [`COUNTER_NAMES`], first-wins — the
/// pre-existing rule, kept byte-stable for comparable inputs).
pub fn render_score_line(s: &ScoreSnapshot) -> String {
    // Top-3 by value, stable on ties (fixed priority order above).
    let mut top: [usize; 3] = [0, 1, 2];
    for i in 3..s.counters.len() {
        let mut m = 0;
        for k in 1..3 {
            if s.counter(top[k]) < s.counter(top[m]) {
                m = k;
            }
        }
        if s.counter(i) > s.counter(top[m]) {
            top[m] = i;
        }
    }
    // Order the 3 desc, stable on ties.
    for i in 0..3 {
        for j in (i + 1)..3 {
            let (a, b) = (top[i], top[j]);
            if s.counter(b) > s.counter(a) {
                top[i] = b;
                top[j] = a;
            }
        }
    }
    let phase_txt: String = match (s.span.phase, s.span.dtsc) {
        (Some(n), DurField::Num(_)) => alloc::format!("P{}", n),
        (_, DurField::Invalid) => alloc::format!("invalid"),
        _ => alloc::format!("none"),
    };
    alloc::format!(
        "BOOT_METRICS slowest_phase={} dtsc_ticks={} dus_us={} tsc_valid={} ov={} top={}:{},{}:{},{}:{}",
        phase_txt,
        s.span.dtsc.render(),
        s.dus.render(),
        if s.tsc_valid { 1 } else { 0 },
        if s.overflow { 1 } else { 0 },
        COUNTER_NAMES[top[0]],
        s.counter(top[0]),
        COUNTER_NAMES[top[1]],
        s.counter(top[1]),
        COUNTER_NAMES[top[2]],
        s.counter(top[2]),
    )
}

/// Parsed SCORE line (round-trip target of [`render_score_line`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedScore {
    /// Slowest-span phase token: `Some(n)` for `Pn`, `None` otherwise.
    pub phase: Option<u8>,
    /// True when the phase token was `invalid` (vs `none` = unavailable).
    pub phase_invalid: bool,
    pub dtsc: DurField,
    pub dus: DurField,
    pub tsc_valid: bool,
    pub overflow: bool,
    /// Top-3 as (priority index into [`COUNTER_NAMES`], value), in line order.
    pub top: [(usize, u64); 3],
}

fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    for tok in line.split_whitespace() {
        if let Some((k, v)) = tok.split_once('=') {
            if k == key {
                return Some(v);
            }
        }
    }
    None
}

/// Parse a [`render_score_line`] line back (`None` = not a SCORE line or
/// malformed). Strict: all six keys required; unknown counter names reject.
pub fn parse_score_line(line: &str) -> Option<ParsedScore> {
    if !line.contains("BOOT_METRICS") {
        return None;
    }
    let phase_tok = field(line, "slowest_phase")?;
    let (phase, phase_invalid) = if let Some(rest) = phase_tok.strip_prefix('P') {
        (Some(rest.parse::<u8>().ok()?), false)
    } else if phase_tok == "none" {
        (None, false)
    } else if phase_tok == "invalid" {
        (None, true)
    } else {
        return None;
    };
    let dtsc = DurField::parse(field(line, "dtsc_ticks")?)?;
    let dus = DurField::parse(field(line, "dus_us")?)?;
    let tsc_valid = match field(line, "tsc_valid")? {
        "1" => true,
        "0" => false,
        _ => return None,
    };
    let overflow = match field(line, "ov")? {
        "1" => true,
        "0" => false,
        _ => return None,
    };
    let top_s = field(line, "top")?;
    let parts: Vec<&str> = top_s.split(',').collect();
    if parts.len() != 3 {
        return None;
    }
    let mut top = [(0usize, 0u64); 3];
    for (i, p) in parts.iter().enumerate() {
        let (name, val) = p.split_once(':')?;
        let idx = COUNTER_NAMES.iter().position(|n| *n == name)?;
        let v = val.parse::<u64>().ok()?;
        top[i] = (idx, v);
    }
    Some(ParsedScore {
        phase,
        phase_invalid,
        dtsc,
        dus,
        tsc_valid,
        overflow,
        top,
    })
}

/// `BOOT_METRICS slowest_phase=… dtsc_ticks=… dus_us=… tsc_valid=… ov=… top=…`
/// (pure format via [`capture_snapshot`] + [`render_score_line`]).
pub fn score_metrics_line() -> String {
    render_score_line(&capture_snapshot())
}

/// Emit SCORE metrics line via the existing slog path (sev=ok, visible).
pub fn publish_metrics_score() {
    let line = score_metrics_line();
    crate::slog_bin!(
        "BOOT",
        "ok",
        "home=k_nano::boot_metrics | {}",
        line
    );
}

/// Reset all stamps/counters/overflow (tests + boot re-entry).
pub fn reset_all() {
    for i in 0..MAX_PHASES {
        PHASE_TSC[i].store(0, Ordering::Relaxed);
        PHASE_CPU[i].store(CPU_EMPTY, Ordering::Relaxed);
    }
    for a in [&PF, &FAULT_OK, &FAULT_FAIL, &DMA_OK, &DMA_FAIL] {
        for i in 0..MAX_CPUS {
            a[i].store(0, Ordering::Relaxed);
        }
    }
    METRICS_OV.store(0, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use spin::Mutex;

    /// Serializes tests mutating shared statics (SESSION_346 race).
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn counters_increment_per_cpu_and_aggregate() {
        let _g = TEST_LOCK.lock();
        reset_all();
        note_pf(0);
        note_pf(0);
        note_pf(3);
        assert_eq!(pf_total(), 3);
        assert_eq!(PF[0].load(Ordering::Relaxed), 2);
        assert_eq!(PF[3].load(Ordering::Relaxed), 1);
        note_fault_in(true, 1);
        note_fault_in(false, 1);
        note_fault_in(false, 2);
        assert_eq!(fault_ok_total(), 1);
        assert_eq!(fault_fail_total(), 2);
        note_dma(true, 0);
        note_dma(false, 7);
        assert_eq!(dma_ok_total(), 1);
        assert_eq!(dma_fail_total(), 1);
        // cpu id wraps into lane (no panic, no OOB).
        note_pf(255);
        assert_eq!(pf_total(), 4);
        reset_all();
        assert_eq!(pf_total(), 0);
    }

    #[test]
    fn phase_timestamps_ordered_and_first_wins() {
        let _g = TEST_LOCK.lock();
        reset_all();
        assert_eq!(phase_tsc(0), 0);
        assert_eq!(phase_tsc(99), 0); // OOB guard
        assert_eq!(phase_cpu(0), None);
        assert_eq!(phase_cpu(99), None);
        note_phase_entry(0);
        note_phase_entry(1);
        note_phase_entry(9); // OOB guard, no panic
        let t0 = phase_tsc(0);
        let t1 = phase_tsc(1);
        assert!(t0 != 0 && t1 != 0);
        assert!(t1 >= t0, "tsc not ordered: t0={} t1={}", t0, t1);
        assert_eq!(phase_cpu(0), Some(0));
        assert_eq!(phase_cpu(1), Some(0));
        note_phase_entry(0); // re-emit must not move entry stamp
        assert_eq!(phase_tsc(0), t0);
        let (n, d) = slowest_phase();
        assert_eq!(n, 0);
        assert_eq!(d, t1 - t0);
        let span = slowest_span();
        assert_eq!(span.phase, Some(0));
        assert_eq!(span.dtsc, DurField::Num(t1 - t0));
        reset_all();
    }

    #[test]
    fn slowest_phase_needs_two_phases() {
        let _g = TEST_LOCK.lock();
        reset_all();
        assert_eq!(slowest_phase(), (u8::MAX, 0));
        assert_eq!(
            slowest_span(),
            SlowestSpan {
                phase: None,
                dtsc: DurField::Unavailable
            }
        );
        note_phase_entry(4);
        assert_eq!(slowest_phase(), (u8::MAX, 0));
        reset_all();
    }

    #[test]
    fn score_line_format_and_top3() {
        let _g = TEST_LOCK.lock();
        reset_all();
        note_phase_entry(2);
        note_phase_entry(5);
        note_pf(0);
        note_pf(1);
        note_pf(2);
        note_fault_in(true, 0);
        note_dma(false, 0);
        let line = score_metrics_line();
        assert!(line.starts_with("BOOT_METRICS slowest_phase=P2 "), "line={}", line);
        assert!(line.contains("top="), "line={}", line);
        // pf=3 is the top counter and must lead.
        let top = line.split("top=").nth(1).unwrap();
        assert!(top.starts_with("pf:3,"), "top={}", top);
        assert!(top.contains("fault_ok:1"), "top={}", top);
        assert!(top.contains("dma_fail:1"), "top={}", top);
        reset_all();
        let empty = score_metrics_line();
        assert!(empty.contains("slowest_phase=none"), "line={}", empty);
        assert!(empty.contains("top=pf:0,fault_ok:0,fault_fail:0"), "line={}", empty);
        reset_all();
    }

    #[test]
    fn cross_cpu_delta_is_invalid_never_a_number() {
        let _g = TEST_LOCK.lock();
        reset_all();
        // Same-CPU control: exact delta, deterministic (no rdtsc).
        note_phase_entry_at(0, 1000, 0);
        note_phase_entry_at(1, 2000, 0);
        assert_eq!(
            slowest_span(),
            SlowestSpan {
                phase: Some(0),
                dtsc: DurField::Num(1000)
            }
        );
        assert_eq!(slowest_phase(), (0, 1000));
        reset_all();
        // Cross-CPU: endpoints entered, but the delta is incomparable.
        note_phase_entry_at(0, 1000, 0);
        note_phase_entry_at(1, 2000, 1);
        assert_eq!(
            slowest_span(),
            SlowestSpan {
                phase: None,
                dtsc: DurField::Invalid
            }
        );
        // Legacy pair stays fail-closed (no fabricated delta).
        assert_eq!(slowest_phase(), (u8::MAX, 0));
        let line = score_metrics_line();
        assert!(line.contains("slowest_phase=invalid"), "line={}", line);
        assert!(line.contains("dtsc_ticks=invalid"), "line={}", line);
        assert!(line.contains("dus_us=invalid"), "line={}", line);
        assert!(!line.contains("dtsc_ticks=1000"), "line={}", line);
        // Re-emit keeps the first writer's CPU tag (tag follows TSC winner).
        note_phase_entry_at(0, 5000, 7);
        assert_eq!(phase_tsc(0), 1000);
        assert_eq!(phase_cpu(0), Some(0));
        reset_all();
    }

    #[test]
    fn unavailable_renders_na_never_zero() {
        let _g = TEST_LOCK.lock();
        reset_all();
        let line = score_metrics_line();
        assert!(line.contains("slowest_phase=none"), "line={}", line);
        assert!(line.contains("dtsc_ticks=n/a"), "line={}", line);
        assert!(line.contains("dus_us=n/a"), "line={}", line);
        // Pure unit check: unknown rate has no `us` value — not even 0.
        assert_eq!(ticks_to_us_with_hz(1_000_000, 0), None);
        assert_eq!(ticks_to_us_with_hz(1_000_000, 1_000_000), Some(1_000_000));
        // Forced-untrusted snapshot: valid tick delta, withheld `us`.
        let s = ScoreSnapshot {
            span: SlowestSpan {
                phase: Some(2),
                dtsc: DurField::Num(500),
            },
            dus: DurField::Unavailable,
            tsc_valid: false,
            overflow: false,
            counters: [3, 1, 0, 0, 1],
        };
        let l2 = render_score_line(&s);
        assert!(l2.contains("tsc_valid=0"), "line={}", l2);
        assert!(l2.contains("dus_us=n/a"), "line={}", l2);
        assert!(!l2.contains("dus_us=0,"), "line={}", l2);
        reset_all();
    }

    #[test]
    fn overflow_saturates_and_marks_ov() {
        let _g = TEST_LOCK.lock();
        reset_all();
        assert_eq!(metrics_overflow(), 0);
        PF[0].store(u64::MAX, Ordering::Relaxed);
        note_pf(0); // must saturate, not wrap to 0
        assert_eq!(PF[0].load(Ordering::Relaxed), u64::MAX);
        assert_eq!(metrics_overflow() & OV_PF, OV_PF);
        assert!(metrics_overflowed());
        // Total saturates too (no wrap through `sum`).
        assert_eq!(pf_total(), u64::MAX);
        let line = score_metrics_line();
        assert!(line.contains("ov=1"), "line={}", line);
        reset_all();
        assert_eq!(metrics_overflow(), 0);
        assert!(!metrics_overflowed());
        assert!(score_metrics_line().contains("ov=0"));
        reset_all();
    }

    #[test]
    fn score_line_is_deterministic() {
        let _g = TEST_LOCK.lock();
        reset_all();
        note_phase_entry_at(1, 100, 0);
        note_phase_entry_at(3, 600, 0);
        note_phase_entry_at(4, 900, 0);
        note_pf(0);
        note_fault_in(false, 1);
        let a = score_metrics_line();
        let b = score_metrics_line();
        assert_eq!(a, b, "same input must give byte-identical lines");
        // Stable tie-break: all-zero counters keep priority order.
        reset_all();
        let tie = score_metrics_line();
        let top = tie.split("top=").nth(1).unwrap();
        assert!(top.starts_with("pf:0,fault_ok:0,fault_fail:0"), "top={}", top);
        // Fixed field order.
        let order = ["slowest_phase=", "dtsc_ticks=", "dus_us=", "tsc_valid=", "ov=", "top="];
        let mut pos = 0usize;
        for k in order {
            let i = tie.find(k).unwrap_or_else(|| panic!("missing {} in {}", k, tie));
            assert!(i >= pos, "field {} out of order in {}", k, tie);
            pos = i;
        }
        reset_all();
    }

    #[test]
    fn score_line_round_trips() {
        let _g = TEST_LOCK.lock();
        // Case 1: live line with a valid same-CPU span.
        reset_all();
        note_phase_entry_at(2, 1000, 0);
        note_phase_entry_at(5, 3000, 0);
        note_pf(0);
        note_pf(1);
        let line = score_metrics_line();
        let p = parse_score_line(&line).unwrap_or_else(|| panic!("unparseable {}", line));
        assert_eq!(p.phase, Some(2));
        assert!(!p.phase_invalid);
        assert_eq!(p.dtsc, DurField::Num(2000));
        assert_eq!(p.tsc_valid, tsc_valid());
        assert_eq!(p.overflow, false);
        assert_eq!(p.top[0], (0, 2)); // pf:2 leads
        // Case 2: unavailable renders n/a and parses back as n/a.
        reset_all();
        let empty = score_metrics_line();
        let q = parse_score_line(&empty).unwrap_or_else(|| panic!("unparseable {}", empty));
        assert_eq!(q.phase, None);
        assert!(!q.phase_invalid);
        assert_eq!(q.dtsc, DurField::Unavailable);
        assert_eq!(q.dus, DurField::Unavailable);
        // Case 3: cross-CPU invalid + forced overflow bit round-trip,
        // re-rendered through the pure renderer (byte-identical).
        let s = ScoreSnapshot {
            span: SlowestSpan {
                phase: None,
                dtsc: DurField::Invalid,
            },
            dus: DurField::Invalid,
            tsc_valid: false,
            overflow: true,
            counters: [0, 0, 0, 0, 0],
        };
        let l1 = render_score_line(&s);
        let l2 = render_score_line(&s);
        assert_eq!(l1, l2);
        let r = parse_score_line(&l1).unwrap_or_else(|| panic!("unparseable {}", l1));
        assert_eq!(r.phase, None);
        assert!(r.phase_invalid);
        assert_eq!(r.dtsc, DurField::Invalid);
        assert_eq!(r.dus, DurField::Invalid);
        assert!(!r.tsc_valid);
        assert!(r.overflow);
        // Case 4: garbage rejected.
        assert!(parse_score_line("hello").is_none());
        assert!(parse_score_line("BOOT_METRICS slowest_phase=P1").is_none());
        reset_all();
    }
}
