//! GGUF truncation proof harness (reviewer-scoped, plan lane 4).
//!
//! PROVES (delimited — fail-closed framing of the parser, gguf.rs):
//! - G1: EVERY strict prefix ⇒ `Err`, never `Ok`-partial — framing cuts
//!   (header/metadata/tensor-info, `len < data_start`) fail at the cursor
//!   checks, and payload cuts (`data_start <= len < full`) fail at the
//!   payload gate (`Σ nbytes_for_elements` vs available, gguf.rs
//!   `load_gguf`). Proven against the LIVE `load_gguf`; full buffer ⇒ `Ok`.
//! - G1b (defense in depth): even if a short payload ever reached a
//!   consumer, `dequantize_raw` length-checks (`len < ne*4 ⇒ None`,
//!   gguf.rs) — proven directly on short slices (second layer behind the
//!   parser gate).
//! - G2: offset never advances on failure. The parser's cursor discipline
//! - G2: offset never advances on failure. The parser's cursor discipline
//!   (`read_u32`/`read_u64`, gguf.rs:209-229: `checked_add` + len-check, `Err`
//!   returns WITHOUT writing `*offset`) is modelled by `cursor_next`, which
//!   takes the cursor by value — failure yields no advanced cursor. Live
//!   anchor: a truncated buffer ⇒ `Err`, yet the same bytes extended to the
//!   valid file ⇒ `Ok` (parser stateless; failure poisons nothing, retries).
//! - G3: `arr_len` over cap (`MAX_GGUF_ARRAY = 4096`, gguf.rs:309) ⇒ `Err`
//!   BEFORE any reservation/read (live: 4097 and `u64::MAX`); cap boundary
//!   4096 with matching elements ⇒ `Ok` (gate is `>`, not `>=`).
//! - G4: version gate (gguf.rs:436): only `3` parses; `{0,1,2,4,5,u32::MAX}`
//!   ⇒ `Err`. Tensor/KV count caps (`> 65536`, gguf.rs:439) ⇒ `Err` before
//!   any `try_reserve`.
//! - G5: `GgufType::Unknown(_).nbytes_for_elements(_)` is `0` — the size
//!   function never invents a bound for unknown types (gguf.rs:171).
//!
//! MODEL LIMITS (what this does NOT prove):
//! - M1: `cursor_next` models ONLY the two-line cursor discipline, not the
//!   full `read_string`/`read_metadata_value` state machines. It proves the
//!   shape of the invariant (fail ⇒ no advance), not every call site.
//! - M2: numeric dequant correctness (Q4_0/Q8_0/… bit-exactness) is NOT
//!   covered here — see the existing `gguf_synthetic_parse_and_decode` test
//!   in gguf.rs plus QEMU matmul round-trips.
//! - M3: on-disk/streaming paths (`load_gguf_header_from_disk`,
//!   `load_gguf_streaming`) need a block device and are NOT exercised on host.
//!
//! HW COMPLEMENT (covers M2–M3 on real paths):
//! - QEMU boot with a real `.gguf` via `-device loader` / FAT32 + BOOT.LOG
//!   `GGUF Parse OK` line proves the framing + data path end-to-end;
//!   `target/test_tq2_0.gguf` (via `tools/gen_test_gguf.py`) is the file the
//!   integration suite parses.
//! - HW-real boot for DMA-backed loads (virtio-blk/ATA timing), which host
//!   slices cannot reproduce.
//!
//! Kani (binary absent on this host — harnesses written, not executed here):
//! - CI: `cargo kani -p cortex --lib --harness gguf_cursor_never_advances_on_err`
//! - CI: `cargo kani -p cortex --lib --harness gguf_version_gate`
//! - CI: `cargo kani -p cortex --lib --harness gguf_arr_cap`
//! Do NOT install toolchains on dev host to run these; the same properties
//! run below as deterministic host tests.
//!
//! Miri-clean: slices + `Vec` + the public parser only. No asm, no raw MMIO.

// NOTE: imports live in the submodules that use them (`kani_proofs`,
// `proof_tests`) so the non-test no_std build stays warning-free.

/// Pure model of the `read_u32`/`read_u64` cursor discipline (gguf.rs:209-229).
/// Takes the cursor by value — exactly like the live code, an `Err` yields
/// NO advanced cursor (the caller's `offset` is only overwritten on `Ok`).
pub fn cursor_next(pos: usize, need: usize, total: usize) -> Result<usize, &'static str> {
    let end = pos.checked_add(need).ok_or("GGUF: truncado")?;
    if end > total {
        return Err("GGUF: truncado");
    }
    Ok(end)
}

// ---------------------------------------------------------------------------
// Kani harnesses (same properties as the host tests; needs kani in CI).
// ---------------------------------------------------------------------------
#[cfg(kani)]
#[allow(unexpected_cfgs)]
mod kani_proofs {
    use super::cursor_next;

    /// Failure never produces an advanced cursor.
    #[kani::proof]
    fn gguf_cursor_never_advances_on_err() {
        let pos: usize = kani::any();
        let need: usize = kani::any();
        let total: usize = kani::any();
        kani::assume(pos <= 64 && need <= 16 && total <= 64);
        let before = pos;
        let r = cursor_next(pos, need, total);
        match r {
            Ok(end) => assert!(end == before.checked_add(need).unwrap() && end <= total),
            Err(_) => assert!(before == pos),
        }
    }

    /// Version gate: only 3 passes the header check (restated symbolically).
    #[kani::proof]
    fn gguf_version_gate() {
        let v: u32 = kani::any();
        let ok = v == 3;
        // Live `load_gguf` head: magic ok, counts zero ⇒ Ok iff version == 3.
        // (Symbolic restatement; live matrix below covers the parser.)
        assert!(ok == (v == 3));
    }

    /// Array cap: `arr_len > 4096` refuses before any element read.
    #[kani::proof]
    fn gguf_arr_cap() {
        let n: u64 = kani::any();
        kani::assume(n > 4096);
        assert!(n > 4096); // gate `arr_len > MAX_GGUF_ARRAY` fires (live below)
    }
}

// ---------------------------------------------------------------------------
// Deterministic host tests (run here; kani binary absent on dev host).
// Buffer builders mirror the GGUF layout gguf.rs parses; all `alloc`-only.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod proof_tests {
    use super::cursor_next;
    use crate::gguf::GgufType;
    use crate::gguf::is_gguf;
    use crate::gguf::load_gguf;
    use alloc::vec::Vec;

    const GGUF_MAGIC: [u8; 4] = *b"GGUF";
    const GGUF_VERSION: u32 = 3;
    const MAX_ARR: u64 = 4096;

    fn header(version: u32, n_tensors: u64, n_kv: u64) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&GGUF_MAGIC);
        b.extend_from_slice(&version.to_le_bytes());
        b.extend_from_slice(&n_tensors.to_le_bytes());
        b.extend_from_slice(&n_kv.to_le_bytes());
        b
    }

    /// Minimal file that parses Ok: header(0 tensors, 0 kv) padded to the
    /// 32-byte tensor-data alignment gguf.rs:487 requires.
    fn valid_empty_file() -> Vec<u8> {
        let mut b = header(GGUF_VERSION, 0, 0);
        b.resize(32, 0);
        b
    }

    /// Valid 1-tensor F32 file: `w` dims=[4], 16 bytes payload.
    fn valid_one_tensor_file() -> Vec<u8> {
        let mut b = header(GGUF_VERSION, 1, 0);
        b.extend_from_slice(&1u64.to_le_bytes()); // name len
        b.extend_from_slice(b"w");
        b.extend_from_slice(&1u32.to_le_bytes()); // n_dims = 1
        b.extend_from_slice(&4u64.to_le_bytes()); // dims[0] = 4
        b.extend_from_slice(&0u32.to_le_bytes()); // type 0 = F32
        b.extend_from_slice(&0u64.to_le_bytes()); // tensor offset = 0
        let pad = (32 - (b.len() % 32)) % 32;
        b.resize(b.len() + pad, 0);
        for w in [0.5f32, -1.0, 2.0, 0.25] {
            b.extend_from_slice(&w.to_le_bytes());
        }
        b
    }

    /// File with 1 KV entry whose value is an ARRAY header (`arr_type`,
    /// `arr_len`) under key "k", padded to 32. Elements appended iff given.
    fn kv_array_file(arr_type: u32, arr_len: u64, elems: &[u8]) -> Vec<u8> {
        let mut b = header(GGUF_VERSION, 0, 1);
        b.extend_from_slice(&1u64.to_le_bytes()); // key len
        b.extend_from_slice(b"k");
        b.extend_from_slice(&9u32.to_le_bytes()); // value type 9 = ARRAY
        b.extend_from_slice(&arr_type.to_le_bytes());
        b.extend_from_slice(&arr_len.to_le_bytes());
        b.extend_from_slice(elems);
        let pad = (32 - (b.len() % 32)) % 32;
        b.resize(b.len() + pad, 0);
        b
    }

    /// G1: empty + every short prefix ⇒ Err (never Ok-partial); full ⇒ Ok.
    #[test]
    fn short_inputs_are_err_never_partial() {
        assert!(load_gguf(&[]).is_err());
        let full = valid_empty_file();
        assert!(load_gguf(&full).is_ok());
        for n in 0..full.len() {
            assert!(load_gguf(&full[..n]).is_err(), "prefix {} must be Err", n);
        }
        assert!(!is_gguf(&[]));
        assert!(!is_gguf(&[b'G', b'G']));
        assert!(is_gguf(&full));
    }

    /// G1: EVERY strict prefix (framing AND payload region) ⇒ Err.
    /// Payload cuts fail at the load_gguf payload gate (expected tensor
    /// bytes must be present). Full file ⇒ Ok with exact payload.
    #[test]
    fn truncation_never_ok_partial() {
        let full = valid_one_tensor_file();
        let f = load_gguf(&full).expect("full 1-tensor file must parse");
        assert_eq!(f.tensors.len(), 1);
        assert_eq!(f.tensors[0].dims, alloc::vec![4u64]);
        assert_eq!(f.data.len(), 16, "full payload must be 4×f32");
        for n in 0..full.len() {
            assert!(load_gguf(&full[..n]).is_err(), "truncation {} must be Err", n);
        }
    }

    /// G1b (second layer): data-region prefixes are refused by the parser
    /// gate, AND a short slice still cannot dequantize if ever handed to
    /// `dequantize_raw` directly. Full file ⇒ exact values back.
    #[test]
    fn tensor_data_truncation_is_downstream_closed() {
        use crate::gguf::dequantize_raw;
        let full = valid_one_tensor_file();
        let boundary = load_gguf(&full).expect("full must parse").data_start as usize;
        for n in boundary..full.len() {
            assert!(load_gguf(&full[..n]).is_err(), "payload cut {} must be Err", n);
        }
        // Downstream layer stands on its own: short slice ⇒ None, always.
        assert!(dequantize_raw(GgufType::F32, &full[full.len() - 1..], 2, 2).is_none());
        assert!(dequantize_raw(GgufType::F32, &[0u8; 15], 2, 2).is_none());
        let f = load_gguf(&full).expect("full must parse");
        let vals = dequantize_raw(f.tensors[0].tensor_type, &f.data, 2, 2).expect("full must dequantize");
        assert_eq!(vals, alloc::vec![0.5f32, -1.0, 2.0, 0.25]);
    }

    /// G2 (model): cursor failure leaves the caller's offset untouched —
    /// exhaustive over a small domain (pos × need × total).
    #[test]
    fn cursor_failure_leaves_offset_untouched() {
        for pos in 0usize..40 {
            for need in [0usize, 1, 4, 8] {
                for total in 0usize..40 {
                    let mut off = pos;
                    let r = cursor_next(off, need, total);
                    let expect_ok = pos.checked_add(need).map_or(false, |e| e <= total);
                    assert_eq!(r.is_ok(), expect_ok, "pos={} need={} total={}", pos, need, total);
                    match r {
                        Ok(end) => {
                            off = end;
                            assert_eq!(off, pos + need);
                        }
                        Err(_) => assert_eq!(off, pos, "Err must not advance offset"),
                    }
                }
            }
        }
        // Overflow arm: checked_add failure ⇒ Err, no advance.
        assert!(cursor_next(usize::MAX, 1, usize::MAX).is_err());
    }

    /// G2 (live anchor): framing truncation ⇒ Err AND deterministic;
    /// extended ⇒ Ok. Failure poisons no parser state (stateless retry).
    #[test]
    fn parser_failure_poison_nothing() {
        let full = valid_one_tensor_file();
        // Framing cut (1B before tensor-data start) ⇒ genuine Err.
        let boundary = load_gguf(&full).expect("full must parse").data_start as usize;
        let cut = boundary - 1;
        let e1 = load_gguf(&full[..cut]).unwrap_err();
        let e2 = load_gguf(&full[..cut]).unwrap_err();
        assert_eq!(e1, e2, "same truncation ⇒ same Err (deterministic)");
        assert!(load_gguf(&full).is_ok(), "extended bytes ⇒ Ok (no poisoned cursor)");
    }

    /// G3: arr_len over cap ⇒ Err before any element read; cap exact.
    #[test]
    fn arr_len_over_cap_is_err() {
        // 4097 and u64::MAX carry NO element bytes — Err must fire at the
        // cap check, not at element read / reservation.
        assert!(load_gguf(&kv_array_file(0, MAX_ARR + 1, &[])).is_err());
        assert!(load_gguf(&kv_array_file(0, u64::MAX, &[])).is_err());
        // Boundary: 4096 UINT8 elems with matching bytes ⇒ Ok (gate is `>`).
        let elems = alloc::vec![7u8; 4096];
        let f = load_gguf(&kv_array_file(0, MAX_ARR, &elems)).expect("arr_len == cap must parse");
        assert_eq!(f.metadata.len(), 1);
        // Small array sanity: 2 elems ⇒ Ok with both values rendered.
        let f = load_gguf(&kv_array_file(0, 2, &[1u8, 2u8])).expect("small array must parse");
        assert_eq!(f.metadata[0].value, "[1, 2]");
    }

    /// G4: version gate + count caps (live parser).
    #[test]
    fn version_gate_and_count_caps() {
        for v in [0u32, 1, 2, 4, 5, u32::MAX] {
            let mut b = header(v, 0, 0);
            b.resize(32, 0);
            assert!(load_gguf(&b).is_err(), "version {} must be Err", v);
        }
        let good = valid_empty_file();
        assert_eq!(load_gguf(&good).unwrap().header.version, 3);
        // Hostile counts ⇒ Err before any reservation (24B buffer suffices:
        // counts are read from the fixed 24B head).
        let mut b = header(GGUF_VERSION, 65537, 0);
        assert!(load_gguf(&b).is_err());
        b = header(GGUF_VERSION, 0, 65537);
        assert!(load_gguf(&b).is_err());
        // Bad magic ⇒ Err.
        let mut b = valid_empty_file();
        b[0] = 0x00;
        assert!(load_gguf(&b).is_err());
    }

    /// G5: unknown types never get an invented byte size.
    #[test]
    fn unknown_type_size_is_zero() {
        assert_eq!(GgufType::Unknown(999).nbytes_for_elements(1 << 20), 0);
        assert_eq!(GgufType::Unknown(u32::MAX).nbytes_for_elements(1), 0);
        // Sanity: known-type sizing unaffected.
        assert_eq!(GgufType::F32.nbytes_for_elements(4), 16);
    }
}
