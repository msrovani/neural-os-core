use alloc::vec::Vec;

pub struct AudioFrame {
    pub pcm: Vec<i16>,
    pub channels: u8,
    pub sample_rate: u32,
}

pub struct TranscriptionFrame {
    pub text: alloc::string::String,
    pub confidence: f32,
}

pub struct TTSCommandFrame {
    pub text: alloc::string::String,
    pub voice: alloc::string::String,
}

pub struct WakeWordFrame {
    pub keyword: alloc::string::String,
    pub confidence: f32,
}
