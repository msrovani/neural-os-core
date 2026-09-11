//! Áudio → energia espectral para o orb (Soul Mirror).
//!
//! O avatar de partículas legado (`JarbasAvatar`) foi removido: o herói visual
//! é o Soul Mirror + grafo mesh. Estados de persona vivem em `AvatarState`
//! (consumido por `jarvis.rs`); o estado do orb é `soul_mirror::OrbState`.
//!
//! Transformada: Goertzel real (2 mul + 2 add por amostra/bin, sem trig) sobre
//! janela Hann de 256 amostras com tabelas pré-computadas (`spin::Once`). O
//! código anterior refazia a janela Hamming e chamava `sinf`/`cosf` por
//! amostra/bin (~129K transcendentais por buffer em soft-float) e travava o
//! scheduler quando mic/playback estavam armados.

use core::f32::consts::PI;
use libm::{cosf, sqrtf};
use spin::{Mutex, Once};

/// Bins espectrais do orb (32 barras do waveform lidas pelo compositor).
const N_BINS: usize = 32;
/// Janela da transformada: 256 amostras = 16 ms @ 16 kHz.
const FFT_N: usize = 256;
/// Teto de amostras drenadas pelo tap FFT por chamada. O mixer entrega até
/// 1024 amostras/tick e o HDA pode publicar um burst de chunks — o FFT nunca
/// processa mais que isto (VAD/STT seguem recebendo o buffer completo).
const FFT_TAP_MAX: usize = 1024;
/// Normalização de magnitude: |X| / (N/4) / full-scale i16 = |X| / (N*8192).
const NORM_SCALE: f32 = 8192.0;

/// Buffer de energia FFT (32 bins espectrais)
static FFT_BINS: Mutex<[f32; N_BINS]> = Mutex::new([0.0f32; N_BINS]);

/// Tabelas da transformada — janela Hann + coeficientes Goertzel
/// `2*cos(2*PI*(k+1)/64)`. Computadas uma única vez, nunca por chunk.
struct FftTables {
    window: [f32; FFT_N],
    coeff: [f32; N_BINS],
}

static FFT_TABLES: Once<FftTables> = Once::new();

fn tables() -> &'static FftTables {
    FFT_TABLES.call_once(|| {
        let mut window = [0.0f32; FFT_N];
        for (n, w) in window.iter_mut().enumerate() {
            *w = 0.5 - 0.5 * cosf(2.0 * PI * n as f32 / (FFT_N - 1) as f32); // Hann
        }
        let mut coeff = [0.0f32; N_BINS];
        for (k, c) in coeff.iter_mut().enumerate() {
            // bin k -> (k+1)*250 Hz @ 16 kHz -> (k+1)/64 rad normalizado
            *c = 2.0 * cosf(2.0 * PI * (k + 1) as f32 / 64.0);
        }
        FftTables { window, coeff }
    })
}

/// Le a energia FFT atual (usado pelo compositor para animar o orb).
/// QEMU stub: se bins = zeros (sem hardware audio), gera energia sintetica
/// baseada no tick do timer para o orb pulsar visualmente.
pub fn read_audio_energy() -> f32 {
    let real_energy = {
        let bins = FFT_BINS.lock();
        bins.iter().sum::<f32>() / bins.len() as f32
    };
    if real_energy > 0.01 {
        return real_energy;
    }
    // Pulso sintético via SIN_LUT (sem libm no caminho do compositor):
    // ~sin(tick*0.020) e ~sin(tick*0.007) quantizados na LUT de 256 entradas.
    let tick =
        k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
    let a = crate::display::fb::sin_lut(tick.wrapping_mul(815) >> 10);
    let b = crate::display::fb::sin_lut(tick.wrapping_mul(292) >> 10);
    (0.15 + a * 0.12 + b * 0.08).clamp(0.0, 1.0)
}

/// Le um bin individual (usado pelo waveform 32 barras)
pub fn read_fft_bin(i: usize) -> f32 {
    let bins = FFT_BINS.lock();
    if i < bins.len() { bins[i] } else { 0.0 }
}

/// Processa buffer de audio PCM (i16) em 32 bins de energia espectral.
///
/// Goertzel por bin (sem trig no hot path) sobre janela Hann pré-computada.
/// Custo por chamada (pior caso): 32 bins x 256 amostras — independente do
/// tamanho do buffer, que é capado por [`FFT_TAP_MAX`].
pub fn process_audio_fft(pcm: &[i16]) {
    let tap = &pcm[..pcm.len().min(FFT_TAP_MAX)];
    let n = tap.len().min(FFT_N);
    if n < 32 {
        return;
    }
    let t = tables();

    // Janela + amostras janeladas fora do loop de bins (reuso por bin).
    let mut xw = [0.0f32; FFT_N];
    for i in 0..n {
        let w = if n == FFT_N { t.window[i] } else { t.window[i * FFT_N / n] };
        xw[i] = tap[i] as f32 * w;
    }
    let norm = 1.0 / (n as f32 * NORM_SCALE);

    let mut out = [0.0f32; N_BINS];
    for (k, o) in out.iter_mut().enumerate() {
        let coeff = t.coeff[k];
        let (mut s1, mut s2) = (0.0f32, 0.0f32);
        for &x in xw.iter().take(n) {
            let s = x + coeff * s1 - s2;
            s2 = s1;
            s1 = s;
        }
        // |X|^2 de Goertzel: s1^2 + s2^2 - coeff*s1*s2
        let power = s1 * s1 + s2 * s2 - coeff * s1 * s2;
        let mag = if power > 0.0 { sqrtf(power) } else { 0.0 };
        *o = (mag * norm).clamp(0.0, 1.0);
    }
    *FFT_BINS.lock() = out; // lock curto: só a cópia final
}

/// Estado de persona (texto/voz) — distinto do `OrbState` visual do orb.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AvatarState {
    Idle,
    Listening,
    Processing,
    Speaking,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sanidade da transformada: silêncio → 0; tom puro → bin certo, sem vazamento.
    #[test]
    fn transform_sanity_silence_and_tone() {
        let silence = [0i16; 512];
        process_audio_fft(&silence);
        for i in 0..N_BINS {
            assert!(
                read_fft_bin(i) < 0.01,
                "bin {i} = {} em silêncio",
                read_fft_bin(i)
            );
        }

        // 1000 Hz @ 16 kHz -> bin 3 ((3+1)*250 Hz). 256 amostras = 16 períodos.
        let mut tone = [0i16; 512];
        for (n, s) in tone.iter_mut().enumerate() {
            let v = (2.0 * PI * 1000.0 * n as f32 / 16000.0).sin() * 20000.0;
            *s = v as i16;
        }
        process_audio_fft(&tone);
        let target = read_fft_bin(3);
        assert!(target > 0.3, "bin 3 = {target} (esperado ~0.6)");
        for i in [0usize, 1, 2, 4, 5, 6, 7, 8] {
            let leak = read_fft_bin(i);
            assert!(target > 2.0 * leak + 0.05, "vazamento bin {i} = {leak}");
        }

        // Limpa para não poluir outros testes do crate.
        process_audio_fft(&silence);
    }
}
