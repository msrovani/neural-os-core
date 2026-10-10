//! BootErr — stable machine-parseable tags for boot fail/warn lines (plan lane 2).
//!
//! Today boot fail/warn lines are free-form strings, unparseable. This enum
//! gives every PHASE checkpoint failure a stable `[CODE]` tag with a
//! documented discriminant. Log text is otherwise unchanged: the tag is
//! appended inside the legacy banner (`... status=<s> [CODE] ===`) so existing
//! `PHASE n=… name=… status=…` parsers keep matching.

extern crate alloc;

use alloc::string::String;
use core::sync::atomic::{AtomicU32, Ordering};

/// Severity codes for typed boot events (mirrors the banner sev mapping:
/// `fail`→2, `warn`/`degraded`→1, anything else→0).
pub const SEV_OK: u8 = 0;
pub const SEV_WARN: u8 = 1;
pub const SEV_FAIL: u8 = 2;
/// Subsystem id for the PHASE banner path (single producer today).
pub const SUBSYS_BOOT: u8 = 1;
/// No extra detail code: the human-readable text stays in the legacy banner
/// (existing slog formatting); the typed event carries codes only.
pub const DETAIL_NONE: u32 = 0;

/// Counts wire codes with no known [`BootErr`] variant (see
/// [`from_u8_counted`] / [`parse_event`]). Saturates instead of wrapping;
/// never panics.
static UNKNOWN_EVENTS: AtomicU32 = AtomicU32::new(0);

/// Number of unknown wire codes observed so far.
pub fn unknown_events() -> u32 {
    UNKNOWN_EVENTS.load(Ordering::Relaxed)
}

/// Reset the unknown-code counter (tests).
pub fn reset_unknown_events() {
    UNKNOWN_EVENTS.store(0, Ordering::Relaxed);
}

fn note_unknown_wire() {
    // Relaxed is enough: diagnostic counter, exactness not required.
    let c = UNKNOWN_EVENTS.load(Ordering::Relaxed);
    if c < u32::MAX {
        UNKNOWN_EVENTS.store(c + 1, Ordering::Relaxed);
    }
}

/// Stable boot failure codes. Discriminants are part of the log contract —
/// do not renumber; append new variants only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BootErr {
    /// #PF with no covering range (demand-page miss outside heap/sandbox).
    PfNoRange = 1,
    /// Page-table map failure (map_page_direct / map_mmio refused).
    MapFail = 2,
    /// Allocator refused (OOM / heap window / budget).
    AllocFail = 3,
    /// APIC degraded (fallback PIC path, no IPI/SIPI pleno).
    ApicDegraded = 4,
    /// SMP degraded (BSP-only, AP wake failed).
    SmpDegraded = 5,
    /// Tickv/sgdb fell back to volatile RAM (no durable backend).
    TickvVolatile = 6,
    /// ELF truncated / no program headers (loader refused).
    ElfTruncated = 7,
    /// Unknown / unclassified (default for legacy emitters).
    Unknown = 255,
}

impl BootErr {
    /// Short stable tag embedded as `[TAG]` in log lines. Distinct, non-empty,
    /// ASCII-only, no spaces (grep/awk friendly).
    pub fn code(self) -> &'static str {
        match self {
            BootErr::PfNoRange => "PF_NORANGE",
            BootErr::MapFail => "MAP_FAIL",
            BootErr::AllocFail => "ALLOC_FAIL",
            BootErr::ApicDegraded => "APIC_DEGRADED",
            BootErr::SmpDegraded => "SMP_DEGRADED",
            BootErr::TickvVolatile => "TICKV_VOLATILE",
            BootErr::ElfTruncated => "ELF_TRUNCATED",
            BootErr::Unknown => "UNKNOWN",
        }
    }

    /// String form of the tag (alias of [`BootErr::code`], kept so callers
    /// that format `err.as_str()` don't need to know the embedding rule).
    pub fn as_str(self) -> &'static str {
        self.code()
    }

    /// All variants in discriminant order (for tests / exhaustive matching).
    pub fn all() -> [BootErr; 8] {
        [
            BootErr::PfNoRange,
            BootErr::MapFail,
            BootErr::AllocFail,
            BootErr::ApicDegraded,
            BootErr::SmpDegraded,
            BootErr::TickvVolatile,
            BootErr::ElfTruncated,
            BootErr::Unknown,
        ]
    }

    /// Decode a stored discriminant; unknown values map to [`BootErr::Unknown`]
    /// (never panic on wire data).
    pub fn from_u8(v: u8) -> BootErr {
        match v {
            1 => BootErr::PfNoRange,
            2 => BootErr::MapFail,
            3 => BootErr::AllocFail,
            4 => BootErr::ApicDegraded,
            5 => BootErr::SmpDegraded,
            6 => BootErr::TickvVolatile,
            7 => BootErr::ElfTruncated,
            _ => BootErr::Unknown,
        }
    }
}

/// Counting variant of [`BootErr::from_u8`]: unknown wire values map to
/// [`BootErr::Unknown`] (never panics) and bump [`unknown_events`]. The
/// known `Unknown` discriminant (255) is a classified value, not an
/// unknown wire value — it does not count.
pub fn from_u8_counted(v: u8) -> BootErr {
    let e = BootErr::from_u8(v);
    if e == BootErr::Unknown && v != BootErr::Unknown as u8 {
        note_unknown_wire();
    }
    e
}

/// Severity code for a banner status string (same mapping as the banner sev).
pub fn severity_of_status(status: &str) -> u8 {
    match status {
        "fail" => SEV_FAIL,
        "warn" | "degraded" => SEV_WARN,
        _ => SEV_OK,
    }
}

/// Typed boot event — machine-readable, core-compatible (`Copy`, no alloc
/// fields). Text stays a later layer: consumers must use this struct, never
/// log parsing. Serialized form is fixed-order `k=v` tokens (see
/// [`BootEvent::serialize`]); the legacy banner text is untouched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootEvent {
    pub severity: u8,
    pub subsystem: u8,
    pub code: BootErr,
    pub phase: u8,
    pub cpu_id: u8,
    pub detail: u32,
}

impl BootEvent {
    /// Deterministic serialization: fixed field order, decimal integers,
    /// `EVT ` prefix. Additive line — never replaces the legacy banner.
    pub fn serialize(&self) -> String {
        alloc::format!(
            "EVT sev={} sub={} code={} phase={} cpu={} detail={}",
            self.severity,
            self.subsystem,
            self.code as u8,
            self.phase,
            self.cpu_id,
            self.detail
        )
    }
}

/// Single construction site for banner-path events (severity derived from
/// `status`, subsystem fixed to [`SUBSYS_BOOT`], detail [`DETAIL_NONE`]).
pub fn phase_event(n: u8, status: &str, err: BootErr, cpu_id: u8) -> BootEvent {
    BootEvent {
        severity: severity_of_status(status),
        subsystem: SUBSYS_BOOT,
        code: err,
        phase: n,
        cpu_id,
        detail: DETAIL_NONE,
    }
}

/// Emit-path serializer: builds the banner event and returns its
/// deterministic line. The caller logs it additively (legacy banner text
/// stays byte-identical).
pub fn emit_event(n: u8, status: &str, err: BootErr, cpu_id: u8) -> String {
    phase_event(n, status, err, cpu_id).serialize()
}

/// Inverse of [`BootEvent::serialize`]: `None` on malformed input (never
/// panics). Unknown `code` values still parse to `code: Unknown` and bump
/// [`unknown_events`] via [`from_u8_counted`].
pub fn parse_event(s: &str) -> Option<BootEvent> {
    let rest = s.find("EVT ").map(|i| &s[i + 4..])?;
    let (mut sev, mut sub, mut code, mut phase, mut cpu, mut detail) =
        (None, None, None, None, None, None);
    for tok in rest.split_whitespace() {
        let Some((k, v)) = tok.split_once('=') else {
            continue;
        };
        match k {
            "sev" => sev = Some(v.parse::<u8>().ok()?),
            "sub" => sub = Some(v.parse::<u8>().ok()?),
            "code" => code = Some(from_u8_counted(v.parse::<u8>().ok()?)),
            "phase" => phase = Some(v.parse::<u8>().ok()?),
            "cpu" => cpu = Some(v.parse::<u8>().ok()?),
            "detail" => detail = Some(v.parse::<u32>().ok()?),
            _ => {}
        }
    }
    Some(BootEvent {
        severity: sev?,
        subsystem: sub?,
        code: code?,
        phase: phase?,
        cpu_id: cpu?,
        detail: detail?,
    })
}

/// True for statuses that carry a `[CODE]` tag (fail/warn family).
/// `ok` lines keep the legacy banner byte-identical (no tag).
pub fn needs_tag(status: &str) -> bool {
    !matches!(status, "ok")
}

/// Migrated PHASE banner body: legacy text + ` [CODE]` before the closing
/// `===`, so `PHASE n=<n> name=<name> status=<s>` still matches verbatim.
/// Always tags (explicit-err path); see `maybe_tagged_phase_body` for the
/// legacy ok-passthrough.
pub fn phase_body(n: u8, name: &str, status: &str, err: BootErr) -> String {
    alloc::format!(
        "=== PHASE n={} name={} status={} [{}] ===",
        n,
        name,
        status,
        err.code()
    )
}

/// Legacy-compatible body: `ok` → untagged legacy text; anything else →
/// tagged with the given err (callers without a classified cause pass
/// [`BootErr::Unknown`]).
pub fn maybe_tagged_phase_body(n: u8, name: &str, status: &str, err: BootErr) -> String {
    if needs_tag(status) {
        phase_body(n, name, status, err)
    } else {
        alloc::format!("=== PHASE n={} name={} status={} ===", n, name, status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeSet;
    use alloc::vec::Vec;

    #[test]
    fn every_variant_renders_distinct_non_empty_tag() {
        let mut seen = BTreeSet::new();
        for e in BootErr::all() {
            let tag = e.code();
            assert!(!tag.is_empty(), "{:?} renders empty tag", e);
            assert!(
                tag.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_'),
                "tag {:?} must be [A-Z0-9_], got {:?}",
                e,
                tag
            );
            assert!(seen.insert(tag), "duplicate tag {:?}", tag);
            assert_eq!(e.as_str(), tag, "as_str must equal code for {:?}", e);
        }
        assert_eq!(seen.len(), BootErr::all().len());
    }

    #[test]
    fn discriminants_are_stable() {
        assert_eq!(BootErr::PfNoRange as u8, 1);
        assert_eq!(BootErr::MapFail as u8, 2);
        assert_eq!(BootErr::AllocFail as u8, 3);
        assert_eq!(BootErr::ApicDegraded as u8, 4);
        assert_eq!(BootErr::SmpDegraded as u8, 5);
        assert_eq!(BootErr::TickvVolatile as u8, 6);
        assert_eq!(BootErr::ElfTruncated as u8, 7);
        assert_eq!(BootErr::Unknown as u8, 255);
        // Round-trip incl. unknown-wire → Unknown (never panics).
        for e in BootErr::all() {
            assert_eq!(BootErr::from_u8(e as u8), e);
        }
        assert_eq!(BootErr::from_u8(0), BootErr::Unknown);
        assert_eq!(BootErr::from_u8(42), BootErr::Unknown);
    }

    #[test]
    fn migrated_fail_warn_lines_match_code_format() {
        // Sample of migrated lines: legacy prefix intact + [CODE] before `===`.
        let samples: Vec<(u8, &str, &str, BootErr)> = alloc::vec![
            (5, "DriverInit", "fail", BootErr::MapFail),
            (4, "HardwareDiscovery", "warn", BootErr::SmpDegraded),
            (4, "HardwareDiscovery", "degraded", BootErr::ApicDegraded),
            (6, "AgentFleet", "fail", BootErr::ElfTruncated),
            (3, "Diagnostics", "fail", BootErr::PfNoRange),
            (5, "DriverInit", "warn", BootErr::TickvVolatile),
            (5, "DriverInit", "fail", BootErr::AllocFail),
            (5, "DriverInit", "fail", BootErr::Unknown),
        ];
        for (n, name, status, err) in samples {
            let line = phase_body(n, name, status, err);
            // Legacy substring parsers must still match.
            let legacy = alloc::format!("PHASE n={} name={} status={}", n, name, status);
            assert!(line.contains(&legacy), "legacy prefix lost: {}", line);
            // CODE format: ` [<TAG>] ===` suffix.
            let suffix = alloc::format!("[{}] ===", err.code());
            assert!(line.ends_with(&suffix), "CODE suffix lost: {}", line);
        }
    }

    #[test]
    fn ok_lines_stay_untagged_legacy_identical() {
        let line = maybe_tagged_phase_body(5, "DriverInit", "ok", BootErr::Unknown);
        assert_eq!(line, "=== PHASE n=5 name=DriverInit status=ok ===");
        let fail = maybe_tagged_phase_body(5, "DriverInit", "fail", BootErr::Unknown);
        assert_eq!(fail, "=== PHASE n=5 name=DriverInit status=fail [UNKNOWN] ===");
        assert!(needs_tag("fail") && needs_tag("warn") && needs_tag("degraded"));
        assert!(!needs_tag("ok"));
    }

    #[test]
    fn typed_events_round_trip_every_variant() {
        reset_unknown_events();
        for (i, e) in BootErr::all().iter().enumerate() {
            let ev = BootEvent {
                severity: SEV_FAIL,
                subsystem: SUBSYS_BOOT,
                code: *e,
                phase: i as u8,
                cpu_id: 3,
                detail: i as u32,
            };
            let line = ev.serialize();
            let back = parse_event(&line).expect("typed event must parse");
            assert_eq!(back, ev, "round-trip failed for {:?}", e);
            // Same via the emit-path serializer (single construction site).
            let emitted = emit_event(i as u8, "fail", *e, 3);
            let back2 = parse_event(&emitted).expect("emit_event output must parse");
            assert_eq!(back2.code, *e);
            assert_eq!(back2.phase, i as u8);
        }
        // Round-trip of known codes never counts unknowns.
        assert_eq!(unknown_events(), 0);
        reset_unknown_events();
    }

    #[test]
    fn unknown_wire_code_counts_never_panics() {
        reset_unknown_events();
        assert_eq!(from_u8_counted(42), BootErr::Unknown);
        assert_eq!(unknown_events(), 1);
        // Unknown code still parses (typed Unknown), and counts.
        let ev = parse_event("EVT sev=2 sub=1 code=0 phase=5 cpu=0 detail=0")
            .expect("unknown-code line must still parse");
        assert_eq!(ev.code, BootErr::Unknown);
        assert_eq!(unknown_events(), 2);
        // Known Unknown discriminant (255) is classified, not unknown wire.
        let known = parse_event("EVT sev=0 sub=1 code=255 phase=5 cpu=0 detail=0").unwrap();
        assert_eq!(known.code, BootErr::Unknown);
        assert_eq!(unknown_events(), 2);
        // Malformed input is None, never a panic.
        assert!(parse_event("hello").is_none());
        assert!(parse_event("EVT sev=x sub=1 code=2 phase=0 cpu=0 detail=0").is_none());
        assert!(parse_event("EVT sev=2 sub=1 phase=0 cpu=0 detail=0").is_none());
        reset_unknown_events();
    }
}
