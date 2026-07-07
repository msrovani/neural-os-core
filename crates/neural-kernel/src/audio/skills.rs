use alloc::vec::Vec;
use alloc::string::String;
use skill_registry::{Skill, McpManifest, OutputSchema};
use spin;
use crate::audio::vad::{VAD, VadTransition};
use crate::audio::neural::PocketTtsEngine;

static TTS_ENGINE: spin::Mutex<Option<PocketTtsEngine>> = spin::Mutex::new(None);

pub fn init_neural_tts() {
    let engine = crate::audio::neural::try_load_pocket_tts();
    *TTS_ENGINE.lock() = engine;
}

pub struct TtsSkill;

impl Skill for TtsSkill {
    fn manifest(&self) -> McpManifest {
        McpManifest {
            name: String::from("tts_speak"),
            description: String::from("Sintetiza texto em audio PCM (neural GPU ou formant CPU)"),
            required_tokens: Vec::new(), preconditions: Vec::new(), context_links: Vec::new(),
            output_schema: OutputSchema::Any, idempotent: false, contracts: Vec::new(),
        }
    }

    fn execute(&self, input: &[u8]) -> Result<Vec<u8>, &'static str> {
        let text = core::str::from_utf8(input).map_err(|_| "UTF-8 invalido")?;
        let pcm = {
            let guard = TTS_ENGINE.lock();
            match guard.as_ref() {
                Some(engine) if engine.is_loaded() => {
                    let audio = engine.generate(text);
                    crate::serial_println!("[TTS] Neural (GPU): \"{}\" ({} samples, gpu_matmul ativo)", text, audio.len());
                    audio
                }
                _ => {
                    let audio = crate::audio::tts::synthesize(text);
                    crate::serial_println!("[TTS] Formant (CPU): \"{}\" ({} samples)", text, audio.len());
                    audio
                }
            }
        };
        Ok(pcm.iter().flat_map(|s| s.to_le_bytes()).collect())
    }
}

pub struct SttSkill;

impl Skill for SttSkill {
    fn manifest(&self) -> McpManifest {
        McpManifest {
            name: String::from("stt_transcribe"),
            description: String::from("Detecta atividade de voz e extrai caracteristicas do audio"),
            required_tokens: Vec::new(), preconditions: Vec::new(), context_links: Vec::new(),
            output_schema: OutputSchema::String, idempotent: false, contracts: Vec::new(),
        }
    }

    fn execute(&self, input: &[u8]) -> Result<Vec<u8>, &'static str> {
        let pcm: &[i16] = unsafe {
            core::slice::from_raw_parts(input.as_ptr() as *const i16, input.len() / 2)
        };

        if pcm.is_empty() { return Ok(alloc::vec![b'?']); }

        let mut vad = VAD::new(300.0, 16000);
        let mut speech_segments = 0u32;
        let mut total_energy = 0.0f32;
        let mut total_zcr = 0.0f32;
        let mut frame_count = 0u32;
        let frame_size = 320;

        for chunk in pcm.chunks(frame_size) {
            if chunk.len() < frame_size { continue; }
            let (energy, zcr, _active, transition) = vad.process_frame(chunk);
            total_energy += energy;
            total_zcr += zcr;
            frame_count += 1;
            if transition == VadTransition::SpeechStart {
                speech_segments += 1;
            }
        }

        let avg_energy = if frame_count > 0 { total_energy / frame_count as f32 } else { 0.0 };
        let avg_zcr = if frame_count > 0 { total_zcr / frame_count as f32 } else { 0.0 };

        let has_speech = speech_segments > 0 && avg_energy > 200.0;
        let result = if has_speech {
            alloc::format!("[VAD] {} segmentos de voz, energy={:.0}, zcr={:.3}", speech_segments, avg_energy, avg_zcr)
        } else {
            alloc::format!("[VAD] Silencio")
        };

        crate::serial_println!("[STT] {}", result);
        Ok(result.into_bytes())
    }
}
