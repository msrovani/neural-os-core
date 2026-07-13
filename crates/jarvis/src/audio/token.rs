use alloc::vec::Vec;
use jarvis::audio::codebook::{TRAINED_CODEBOOK, TRAINED_CODEBOOK_SIZE, TRAINED_NUM_LEVELS};

pub const TOKEN_FRAME_MS: u32 = 80;
pub const SAMPLE_RATE: u32 = 16000;
pub const TOKEN_FRAME_SAMPLES: usize = (SAMPLE_RATE / 1000) as usize * TOKEN_FRAME_MS as usize;
pub const NUM_RESIDUAL_LEVELS: usize = TRAINED_NUM_LEVELS;
pub const CODEBOOK_SIZE: usize = TRAINED_CODEBOOK_SIZE;

pub struct ResidualQuantizer;

impl ResidualQuantizer {
    /// Codifica um frame PCM em tokens residuais (3 níveis × codebook 64).
    /// Nível 0: energia grossa do frame. Nível 1: médio. Nível 2: fino.
    pub fn encode_frame(pcm: &[i16]) -> [u8; NUM_RESIDUAL_LEVELS] {
        let mut tokens = [0u8; NUM_RESIDUAL_LEVELS];
        let energy: f32 = pcm.iter().map(|&s| (s as f32).abs()).sum::<f32>() / pcm.len().max(1) as f32;
        let mut residual = energy;
        for level in 0..NUM_RESIDUAL_LEVELS {
            let mut best_idx = 0usize;
            let mut best_dist = core::f32::MAX;
            for (i, &c) in TRAINED_CODEBOOK[level].iter().enumerate() {
                let d = (residual - c).abs();
                if d < best_dist { best_dist = d; best_idx = i; }
            }
            tokens[level] = best_idx as u8;
            residual -= TRAINED_CODEBOOK[level][best_idx];
        }
        tokens
    }

    /// Decodifica tokens em amplitude do frame.
    pub fn decode_tokens(tokens: &[u8; NUM_RESIDUAL_LEVELS]) -> f32 {
        let mut val = 0.0f32;
        for level in 0..NUM_RESIDUAL_LEVELS {
            val += TRAINED_CODEBOOK[level][tokens[level] as usize % CODEBOOK_SIZE];
        }
        val
    }

    /// Converte stream de PCM para tokens residuais.
    pub fn encode_stream(pcm: &[i16]) -> Vec<[u8; NUM_RESIDUAL_LEVELS]> {
        let mut tokens = Vec::new();
        for chunk in pcm.chunks(TOKEN_FRAME_SAMPLES) {
            if chunk.len() < TOKEN_FRAME_SAMPLES {
                let mut padded = [0i16; TOKEN_FRAME_SAMPLES];
                for (i, &s) in chunk.iter().enumerate() { padded[i] = s; }
                tokens.push(Self::encode_frame(&padded));
            } else {
                tokens.push(Self::encode_frame(chunk));
            }
        }
        tokens
    }

    /// Decodifica tokens para sinal PCM reconstruído (sine wave na frequência da energia).
    /// Usa senoide na amplitude decodificada para som audível, não apenas DC.
    pub fn decode_stream(tokens: &[[u8; NUM_RESIDUAL_LEVELS]]) -> Vec<i16> {
        let mut pcm = Vec::with_capacity(tokens.len() * TOKEN_FRAME_SAMPLES);
        let mut phase = 0.0f32;
        for t in tokens {
            let amp = Self::decode_tokens(t);
            let freq = 220.0 + (t[0] as f32 / CODEBOOK_SIZE as f32) * 440.0;
            for _ in 0..TOKEN_FRAME_SAMPLES {
                let s = (amp * libm::sinf(phase * 2.0 * core::f32::consts::PI)) as i16;
                pcm.push(s);
                phase += freq / SAMPLE_RATE as f32;
                if phase >= 1.0 { phase -= 1.0; }
            }
        }
        pcm
    }
}
