use alloc::vec::Vec;
use alloc::string::String;
use skill_registry::{Skill, McpManifest, OutputSchema};
use spin;
use crate::audio::vad::{VAD, VadTransition};
use crate::audio::piper::PiperEngine;

static TTS_ENGINE: spin::Mutex<Option<PiperEngine>> = spin::Mutex::new(None);

pub fn init_neural_tts() {
    let engine = try_load_piper();
    *TTS_ENGINE.lock() = engine;
}

fn try_load_piper() -> Option<PiperEngine> {
    // Try VFS via neural-kernel (ring2→ring2) - isolamento de camadas
    for name in &["PIPER.BIN", "PIPER_EN.BIN", "PIPER_PT_BR.BIN"] {
        if let Ok(data) = hermes::globals::read_vfs(name) {
                let mut eng = PiperEngine::new();
                if eng.load(&data) {
                    k_nano::serial_println!("[PIPER] Piper TTS loaded from VFS: {}", name);
                    return Some(eng);
            }
        }
    }
    // Try QEMU loader fallback
    let pm = k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    for &addr in &[0x110000000u64, 0x120000000u64] {
        unsafe {
            let probe = (addr + pm) as *const u32;
            if core::ptr::read_volatile(probe) == 0xBE11BE11 {
                let data = core::slice::from_raw_parts(probe as *const u8, 70 * 1024 * 1024);
                let mut eng = PiperEngine::new();
                if eng.load(data) {
                    k_nano::serial_println!("[PIPER] Piper TTS loaded from QEMU loader @{:x}", addr);
                    return Some(eng);
                }
            }
        }
    }
    k_nano::serial_println!("[PIPER] Piper TTS ausente — formant synth ativo");
    None
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
                    k_nano::serial_println!("[TTS] Piper (neural): \"{}\" ({} samples, multi-lang PT-BR+EN)", text, audio.len());
                    audio
                }
                _ => {
                    let audio = crate::audio::tts::synthesize(text);
                    k_nano::serial_println!("[TTS] Formant (CPU): \"{}\" ({} samples)", text, audio.len());
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

        k_nano::serial_println!("[STT] {}", result);
        Ok(result.into_bytes())
    }
}
