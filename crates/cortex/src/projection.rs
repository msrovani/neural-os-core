//! Hidden → LatentBus projection (ADR-0047). Ad-hoc f16, mean-pool to 256D. Zero deps.

use event_bus::latent::{F16Bits, LatentPacket, LATENT_DIM, TOPIC_THOUGHT_LLM};
use event_bus::CapabilityToken;

/// Convert f32 → IEEE binary16 bits (round-to-nearest-even simplified).
pub fn f32_to_f16_bits(v: f32) -> F16Bits {
    let bits = v.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let mut exp = ((bits >> 23) & 0xFF) as i32;
    let mant = bits & 0x7FFFFF;

    if exp == 255 {
        // Inf/NaN
        return sign | 0x7C00 | if mant != 0 { 0x200 } else { 0 };
    }
    if exp == 0 {
        return sign; // ±0 / subnormal → 0
    }

    exp = exp - 127 + 15;
    if exp >= 31 {
        return sign | 0x7C00; // overflow → Inf
    }
    if exp <= 0 {
        return sign; // underflow
    }
    let mant16 = (mant >> 13) as u16;
    sign | ((exp as u16) << 10) | mant16
}

/// Project arbitrary-length hidden to [f16; 256] via chunk mean-pool (or pad/trunc).
pub fn project_hidden(hidden: &[f32]) -> [F16Bits; LATENT_DIM] {
    let mut out = [0u16; LATENT_DIM];
    if hidden.is_empty() {
        return out;
    }
    let n = hidden.len();
    if n <= LATENT_DIM {
        for (i, &v) in hidden.iter().enumerate() {
            out[i] = f32_to_f16_bits(v);
        }
        return out;
    }
    // Mean-pool: split into LATENT_DIM chunks
    for i in 0..LATENT_DIM {
        let start = i * n / LATENT_DIM;
        let end = ((i + 1) * n / LATENT_DIM).max(start + 1);
        let mut sum = 0.0f32;
        let mut count = 0u32;
        for j in start..end.min(n) {
            sum += hidden[j];
            count += 1;
        }
        let mean = if count > 0 { sum / count as f32 } else { 0.0 };
        out[i] = f32_to_f16_bits(mean);
    }
    out
}

pub fn latent_norm_f32(vec: &[F16Bits; LATENT_DIM]) -> f32 {
    // Cheap proxy: treat f16 bits as scaled — use top-byte magnitude approx
    let mut acc = 0.0f32;
    for &b in vec.iter() {
        let exp = ((b >> 10) & 0x1F) as i32;
        if exp == 0 {
            continue;
        }
        // rough |value| ~ 2^(exp-15)
        let e = (exp - 15).max(-15).min(15);
        let mag = libm::exp2f(e as f32) * 0.5;
        acc += mag * mag;
    }
    libm::sqrtf(acc)
}

/// Publish projected thought on global LatentBus (non-fatal).
pub fn publish_thought(hidden: &[f32]) {
    let vec = project_hidden(hidden);
    let norm = latent_norm_f32(&vec);
    let packet = LatentPacket {
        id: 0,
        topic: alloc::string::String::from(TOPIC_THOUGHT_LLM),
        vec,
        token: CapabilityToken::Legacy(1),
        norm_bits: norm.to_bits(),
    };
    match k_nano::globals::LATENT_BUS.publish(packet) {
        Ok(()) => {
            k_nano::serial_println!("[LATENT] published THOUGHT_LLM norm={:.3}", norm);
        }
        Err(e) => {
            k_nano::serial_println!("[LATENT] publish fail: {}", e);
        }
    }
}
