use alloc::vec::Vec;

pub const TOKEN_FRAME_MS: u32 = 80;
pub const SAMPLE_RATE: u32 = 16000;
pub const TOKEN_FRAME_SAMPLES: usize = (SAMPLE_RATE / 1000) as usize * TOKEN_FRAME_MS as usize;
pub const NUM_RESIDUAL_LEVELS: usize = 3;
pub const CODEBOOK_SIZE: usize = 64;

pub struct ResidualQuantizer {
    codebooks: [[f32; CODEBOOK_SIZE]; NUM_RESIDUAL_LEVELS],
}

impl ResidualQuantizer {
    pub fn new() -> Self {
        let mut codebooks = [[0.0f32; CODEBOOK_SIZE]; NUM_RESIDUAL_LEVELS];
        for l in 0..NUM_RESIDUAL_LEVELS {
            for i in 0..CODEBOOK_SIZE {
                let t = i as f32 / CODEBOOK_SIZE as f32;
                codebooks[l][i] = (t * 2.0 - 1.0) * (0.5 / (l + 1) as f32);
            }
        }
        ResidualQuantizer { codebooks }
    }

    pub fn encode_frame(&self, pcm: &[i16]) -> [u8; NUM_RESIDUAL_LEVELS] {
        let mut tokens = [0u8; NUM_RESIDUAL_LEVELS];
        let energy: f32 = pcm.iter().map(|&s| (s as f32).abs()).sum::<f32>() / pcm.len().max(1) as f32;
        let mut residual = energy;
        for l in 0..NUM_RESIDUAL_LEVELS {
            let mut best_idx = 0usize;
            let mut best_dist = core::f32::MAX;
            for (i, &c) in self.codebooks[l].iter().enumerate() {
                let d = (residual - c).abs();
                if d < best_dist { best_dist = d; best_idx = i; }
            }
            tokens[l] = best_idx as u8;
            residual -= self.codebooks[l][best_idx];
        }
        tokens
    }

    pub fn decode_frame(&self, tokens: &[u8; NUM_RESIDUAL_LEVELS]) -> i16 {
        let mut val = 0.0f32;
        for l in 0..NUM_RESIDUAL_LEVELS {
            val += self.codebooks[l][tokens[l] as usize % CODEBOOK_SIZE];
        }
        (val * 32767.0).max(-32768.0).min(32767.0) as i16
    }

    pub fn encode_stream(&self, pcm: &[i16]) -> Vec<[u8; NUM_RESIDUAL_LEVELS]> {
        let mut tokens = Vec::new();
        for chunk in pcm.chunks(TOKEN_FRAME_SAMPLES) {
            if chunk.len() < TOKEN_FRAME_SAMPLES {
                let mut padded = [0i16; TOKEN_FRAME_SAMPLES];
                for (i, &s) in chunk.iter().enumerate() { padded[i] = s; }
                tokens.push(self.encode_frame(&padded));
            } else {
                tokens.push(self.encode_frame(chunk));
            }
        }
        tokens
    }

    pub fn decode_stream(&self, tokens: &[[u8; NUM_RESIDUAL_LEVELS]]) -> Vec<i16> {
        let mut pcm = Vec::with_capacity(tokens.len() * TOKEN_FRAME_SAMPLES);
        for t in tokens {
            let s = self.decode_frame(t);
            for _ in 0..TOKEN_FRAME_SAMPLES {
                pcm.push(s);
            }
        }
        pcm
    }
}
