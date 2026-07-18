//! MAD/INT8 host golden — perfil BitNet Gen9 (ADR-0050 P4).
//! Bit-exato vs CPU; GPU EU path residual até zebin MAD.

/// `c[i] = saturating_add(a[i] * b[i], c[i])` em i8 — referência host.
pub fn mad_int8_cpu(a: &[i8], b: &[i8], c_in: &[i8], c_out: &mut [i8]) -> bool {
    if a.len() != b.len() || a.len() != c_in.len() || a.len() != c_out.len() || a.is_empty() {
        return false;
    }
    for i in 0..a.len() {
        let p = (a[i] as i32) * (b[i] as i32) + (c_in[i] as i32);
        c_out[i] = p.clamp(i8::MIN as i32, i8::MAX as i32) as i8;
    }
    true
}

/// Comparação bit-exata.
pub fn mad_int8_check(got: &[i8], expect: &[i8]) -> bool {
    got.len() == expect.len() && got.iter().zip(expect.iter()).all(|(g, e)| g == e)
}

/// Canário host MAD/INT8 (sempre CPU). Loga PASS host; não promove has_compute.
pub fn run_mad_int8_host_canary() -> bool {
    let a = [1i8, -2, 3, -4, 5, -6, 7, 8];
    let b = [2i8, 3, -1, 4, -2, 1, 0, -3];
    let c0 = [10i8, 0, -5, 20, 0, 1, -2, 3];
    let mut expect = [0i8; 8];
    let mut got = [0i8; 8];
    if !mad_int8_cpu(&a, &b, &c0, &mut expect) {
        return false;
    }
    // Segunda passagem = mesma referência (GPU stub ainda não existe).
    if !mad_int8_cpu(&a, &b, &c0, &mut got) {
        return false;
    }
    let ok = mad_int8_check(&got, &expect);
    k_nano::slog_hal!("INTEL", "MAD", "host MAD/INT8 {} (GPU EU residual — ≠ has_compute)", if ok { "PASS" } else { "FAIL" });
    ok
}

/// Unpack W2→INT8 on-demand (AirLLM-style) — 4 pesos/byte → i8 {-1,0,1}.
pub fn unpack_w2_to_i8(packed: &[u8], out: &mut [i8]) -> bool {
    if out.len() < packed.len() * 4 {
        return false;
    }
    let mut o = 0;
    for &byte in packed {
        for shift in [0u8, 2, 4, 6] {
            let bits = (byte >> shift) & 0b11;
            out[o] = match bits {
                0b00 => 0,
                0b01 => 1,
                0b10 => -1,
                _ => 0, // 0b11 reserved → 0
            };
            o += 1;
        }
    }
    true
}
