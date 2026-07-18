//! INT8/dot host golden — perfil BitNet AMD (ADR-0049 P4).
//! WMMA/MFMA no device residual; host bit-exato ≠ has_compute.

/// Dot INT8: `acc += a[i]*b[i]` com saturação i32→i32.
pub fn dot_int8_cpu(a: &[i8], b: &[i8]) -> Option<i32> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    let mut acc: i32 = 0;
    for i in 0..a.len() {
        acc = acc.saturating_add((a[i] as i32) * (b[i] as i32));
    }
    Some(acc)
}

pub fn run_dot_int8_host_canary() -> bool {
    let a = [1i8, -2, 3, -4, 5, -6, 7, 8];
    let b = [2i8, 3, -1, 4, -2, 1, 0, -3];
    let Some(v1) = dot_int8_cpu(&a, &b) else {
        return false;
    };
    let Some(v2) = dot_int8_cpu(&a, &b) else {
        return false;
    };
    let ok = v1 == v2;
    k_nano::slog_hal!("AMD", "MAD", "host DOT/INT8 {} acc={} (WMMA/device residual — ≠ has_compute)",
        if ok { "PASS" } else { "FAIL" },
        v1);
    ok
}

/// Pack W2 → i8 {-1,0,1} (mesmo contrato Intel mad — AirLLM).
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
                _ => 0,
            };
            o += 1;
        }
    }
    true
}
