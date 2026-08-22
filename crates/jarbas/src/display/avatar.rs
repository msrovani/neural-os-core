//! Áudio → energia espectral para o orb (Soul Mirror).
//!
//! O avatar de partículas legado (`JarbasAvatar`) foi removido: o herói visual
//! é o Soul Mirror + grafo mesh. Estados de persona vivem em `AvatarState`
//! (consumido por `jarvis.rs`) e em `avatar8` (telemetria → cor do orb).

use core::f32::consts::PI;
use libm::{sinf, cosf};
use spin::Mutex;

/// Buffer de energia FFT (16 bins espectrais)
static FFT_BINS: Mutex<[f32; 32]> = Mutex::new([0.0f32; 32]);

/// Le a energia FFT atual (usado pelo compositor para animar o orb)
pub fn read_audio_energy() -> f32 {
    let bins = FFT_BINS.lock();
    bins.iter().sum::<f32>() / bins.len() as f32
}

/// Le um bin individual (usado pelo waveform 32 barras)
pub fn read_fft_bin(i: usize) -> f32 {
    let bins = FFT_BINS.lock();
    if i < bins.len() { bins[i] } else { 0.0 }
}

/// Processa buffer de audio PCM (i16) em 16 bins de energia espectral
/// Usa Goertzel simplificado (sem FFT completa) para no_std leve
pub fn process_audio_fft(pcm: &[i16]) {
    let mut bins = FFT_BINS.lock();
    let n = pcm.len() as f32;
    if n < 16.0 {
        return;
    }
    for bin in 0..32 {
        let freq = (bin as f32 + 1.0) / 32.0 * (16000.0 / 2.0); // 0-8kHz
        let omega = 2.0 * PI * freq / 16000.0;
        let mut power = 0.0f32;
        let mut chunk = pcm;
        while chunk.len() >= 32 {
            let mut s: f32 = 0.0;
            for i in 0..32 {
                let w = 0.54 - 0.46 * cosf(2.0 * PI * i as f32 / 31.0); // Hamming
                s += (chunk[i] as f32 * w) * sinf(omega * i as f32);
            }
            power += (s / 32.0).abs();
            chunk = &chunk[16..]; // overlap 50%
        }
        bins[bin] = (power / (pcm.len() as f32 / 32.0 + 1.0).max(1.0)).min(1.0);
    }
}

/// Estado de persona (texto/voz) — distinto do `Avatar8State` de telemetria visual.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AvatarState {
    Idle,
    Listening,
    Processing,
    Speaking,
}
