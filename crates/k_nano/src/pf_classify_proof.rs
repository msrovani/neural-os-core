//! PF classification proof harness (reviewer-scoped, plan lane 4).
//!
//! PROVES (delimited — decision table only):
//! - P1: `(err & 0x3) == 0x3` (P|W: present + write ⇒ protection-violation
//!   write) ALWAYS refuses cure. Proven two ways: (a) against the LIVE
//!   `crate::allocator::try_fault_in_heap` guard (allocator.rs:1684) for a
//!   spread of cr2/err inputs — that path returns before any page-table walk,
//!   so it is host-safe; (b) exhaustively on the pure model below for all
//!   256 low-byte err values × present × in-range.
//! - P2: absent fault (`P=0`) outside every authorized range NEVER attempts a
//!   map (model ⇒ `RefuseOutOfRange`).
//! - P3: absent fault inside an authorized range routes to a map ATTEMPT
//!   (model ⇒ `AttemptMap`; success still depends on frame alloc + HW walk).
//! - P4: stale-TLB case (PTE already present) routes to flush-cure, never to
//!   alloc (model ⇒ `CureStaleTlb`).
//!
//! MODEL LIMITS (what this does NOT prove):
//! - M1: `classify_pf_model` mirrors the branch ORDER of
//!   `try_fault_in_heap` (guard → present-check → range-check) but NOT the
//!   range arithmetic itself (TALC span / bump limit / kernel-virt window) and
//!   NOT the page-table walk, frame alloc, or TLB flush. A drift between the
//!   model and allocator.rs range code would not be caught here — the live
//!   P1 test only pins the P|W guard.
//! - M2: the live test covers ONLY the P|W early-refuse path. Non-P|W inputs
//!   would proceed into `Cr3::read` / page-table dereference, which traps on
//!   host — so they are NEVER passed to the live fn here (no real fault is
//!   simulated in any host test).
//! - M3: cure SUCCESS (`true` ⇒ fault actually fixed) is unproven on host; it
//!   needs a real MMU (map + flush + re-access).
//!
//! HW COMPLEMENT (covers M1–M3 on real iron):
//! - QEMU boot with `-d int`: `#PF` storm absence + `PF_DIAG_OK` increments in
//!   BOOT.LOG prove demand-map cures; `PF_DIAG_NO_RANGE` on wild faults
//!   proves the refuse path; `PF_DIAG_MAP_FAIL`/`ALLOC_FAIL` distinguish the
//!   attempt-failed case. See SESSION_299/301 demand-page evidence.
//! - HW-real boot (16 GB class machine) for true CR3/TLB behaviour — QEMU
//!   masks stale-TLB and HHDM aliasing effects.
//!
//! Kani (binary absent on this host — harnesses written, not executed here):
//! - CI: `cargo kani -p k-nano --lib --harness pf_pw_dominates`
//! - CI: `cargo kani -p k-nano --lib --harness pf_out_of_range_never_maps`
//! - CI: `cargo kani -p k-nano --lib --harness pf_absent_in_range_attempts`
//! Do NOT install toolchains on dev host to run these; the same properties
//! run below as deterministic host tests.
//!
//! Miri-clean: no asm, no raw MMIO, no CR3/page-table access in any harness
//! path (live fn is only called with P|W err, which returns pre-walk).

/// Disposition of a #PF `(cr2, err)` under the allocator decision table.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PfDisposition {
    /// P|W protection-violation write — refuse cure (fail-closed).
    RefuseProtectionWrite,
    /// PTE already present — stale TLB, flush cures (no alloc).
    CureStaleTlb,
    /// Absent page inside an authorized range — attempt demand-map.
    AttemptMap,
    /// Absent page outside all authorized ranges — refuse.
    RefuseOutOfRange,
}

/// Pure model of the `try_fault_in_heap` branch order
/// (allocator.rs:1684 guard → :1697 present-check → :1738 range gate).
/// `pte_present` = Check-0 result; `in_authorized_range` =
/// `in_talc || in_bump || in_kernel_virt`. No HW touch.
pub fn classify_pf_model(err: u64, pte_present: bool, in_authorized_range: bool) -> PfDisposition {
    // allocator.rs:1684 — RO-write on a PRESENT page is never stale-TLB.
    if (err & 0x3) == 0x3 {
        return PfDisposition::RefuseProtectionWrite;
    }
    // allocator.rs:1697 — already present ⇒ flush cures.
    if pte_present {
        return PfDisposition::CureStaleTlb;
    }
    // allocator.rs:1738 — outside every range ⇒ refuse.
    if !in_authorized_range {
        return PfDisposition::RefuseOutOfRange;
    }
    PfDisposition::AttemptMap
}

// ---------------------------------------------------------------------------
// Kani harnesses (same properties as the host tests; needs kani in CI).
// ---------------------------------------------------------------------------
#[cfg(kani)]
#[allow(unexpected_cfgs)]
mod kani_proofs {
    use super::classify_pf_model;
    use super::PfDisposition;

    /// P|W dominates every other input bit / state.
    #[kani::proof]
    fn pf_pw_dominates() {
        let err: u64 = kani::any();
        let present: bool = kani::any();
        let in_range: bool = kani::any();
        kani::assume((err & 0x3) == 0x3);
        assert!(classify_pf_model(err, present, in_range) == PfDisposition::RefuseProtectionWrite);
    }

    /// Out-of-range absent faults never reach a map attempt.
    #[kani::proof]
    fn pf_out_of_range_never_maps() {
        let err: u64 = kani::any();
        kani::assume((err & 0x3) != 0x3);
        let d = classify_pf_model(err, false, false);
        assert!(d == PfDisposition::RefuseOutOfRange);
    }

    /// Absent faults inside an authorized range always route to map attempt.
    #[kani::proof]
    fn pf_absent_in_range_attempts() {
        let err: u64 = kani::any();
        kani::assume((err & 0x3) != 0x3);
        assert!(classify_pf_model(err, false, true) == PfDisposition::AttemptMap);
    }
}

// ---------------------------------------------------------------------------
// Deterministic host tests (run here; kani binary absent on dev host).
// ---------------------------------------------------------------------------
#[cfg(test)]
mod proof_tests {
    use super::classify_pf_model;
    use super::PfDisposition;
    use crate::allocator::try_fault_in_heap;

    /// LIVE (P1a): the real P|W guard refuses for every cr2 class without
    /// touching page tables (returns at allocator.rs:1684 pre-walk).
    /// No real fault is simulated — err is a controlled input word.
    #[test]
    fn live_pw_guard_always_refuses() {
        let cr2s: [u64; 6] = [
            0x0,
            0x1000,
            0x_4000_0000_0000, // TALC heap start (authorized-range representative)
            0xFFFF_8000_0000_0000, // HHDM (MMIO-ish high-half representative)
            0xFFFF_FFFF_8000_0000, // kernel-virt representative
            u64::MAX,
        ];
        // All have (err & 0x3) == 0x3, incl. high-bit / full-mask variants.
        let pw_errs: [u64; 6] = [0x3, 0x7, 0xB, 0x13, 0x1B, u64::MAX];
        for &cr2 in &cr2s {
            for &err in &pw_errs {
                assert!(
                    !try_fault_in_heap(cr2, err),
                    "P|W must refuse: cr2={:#x} err={:#x}",
                    cr2,
                    err
                );
            }
        }
    }

    /// MODEL (P1b): P|W dominates — exhaustive over err low byte × state cube.
    #[test]
    fn model_pw_dominates_all_state() {
        for err in 0u64..256 {
            let is_pw = (err & 0x3) == 0x3;
            for &present in &[false, true] {
                for &in_range in &[false, true] {
                    let d = classify_pf_model(err, present, in_range);
                    assert_eq!(
                        d == PfDisposition::RefuseProtectionWrite,
                        is_pw,
                        "err={:#x} present={} in_range={}",
                        err,
                        present,
                        in_range
                    );
                }
            }
        }
    }

    /// MODEL (P2): out-of-range absent faults never map, for any non-P|W err.
    #[test]
    fn model_out_of_range_never_maps() {
        for err in 0u64..256 {
            if (err & 0x3) == 0x3 {
                continue;
            }
            assert_eq!(
                classify_pf_model(err, false, false),
                PfDisposition::RefuseOutOfRange,
                "err={:#x}",
                err
            );
        }
    }

    /// MODEL (P3): absent + in-range ⇒ map attempt, for any non-P|W err.
    #[test]
    fn model_absent_in_range_attempts() {
        for err in 0u64..256 {
            if (err & 0x3) == 0x3 {
                continue;
            }
            assert_eq!(
                classify_pf_model(err, false, true),
                PfDisposition::AttemptMap,
                "err={:#x}",
                err
            );
        }
    }

    /// MODEL (P4): PTE present ⇒ stale-TLB cure regardless of range.
    #[test]
    fn model_present_cures_stale_tlb() {
        for err in 0u64..256 {
            if (err & 0x3) == 0x3 {
                continue;
            }
            for &in_range in &[false, true] {
                assert_eq!(
                    classify_pf_model(err, true, in_range),
                    PfDisposition::CureStaleTlb,
                    "err={:#x} in_range={}",
                    err,
                    in_range
                );
            }
        }
    }

    /// MODEL gate shape: only (P=1,W=1) refuses at the gate; the other three
    /// (P,W) combos pass through. Catches `& 0x3 != 0`-style over-refuse.
    #[test]
    fn model_gate_shape_only_pw_refuses() {
        assert_ne!(classify_pf_model(0x0, false, true), PfDisposition::RefuseProtectionWrite);
        assert_ne!(classify_pf_model(0x1, false, true), PfDisposition::RefuseProtectionWrite);
        assert_ne!(classify_pf_model(0x2, false, true), PfDisposition::RefuseProtectionWrite);
        assert_eq!(classify_pf_model(0x3, false, true), PfDisposition::RefuseProtectionWrite);
    }
}
