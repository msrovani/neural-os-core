use alloc::vec::Vec;
use alloc::string::String;
use skill_registry::{Skill, McpManifest, OutputSchema};
use spin;
use crate::audio::vad::{VAD, VadTransition};
use crate::audio::piper::PiperEngine;

static TTS_ENGINE: spin::Mutex<Option<PiperEngine>> = spin::Mutex::new(None);

pub fn init_neural_tts() {
    let engine = try_load_piper();
    if engine.is_some() {
        crate::load_status::set(
            crate::load_status::AssetKind::Piper,
            crate::load_status::LoadStatus::Loaded,
        );
    } else {
        crate::load_status::set(
            crate::load_status::AssetKind::Piper,
            crate::load_status::LoadStatus::Absent,
        );
    }
    *TTS_ENGINE.lock() = engine;
}

fn try_load_piper() -> Option<PiperEngine> {
    // Boot PIO em TCG nao aguenta Piper 60MB+ — formant e o path N5 minimo.
    const MAX_BOOT_PIPER: usize = 2 * 1024 * 1024;
    unsafe {
        for try_slave in &[false, true] {
            let mut tmp = crate::ATA_DRIVER.lock();
            if let Some(ref mut ata) = *tmp {
                let orig = ata.slave;
                ata.slave = *try_slave;
                let parts = crate::fat32::read_mbr(ata);
                for p in &parts {
                    if p.type_code != 0x1C && p.type_code != 0x0C && p.type_code != 0x0B { continue; }
                    if let Some(fs) = crate::fat32::Fat32Reader::new(ata, p) {
                        for name in &["PIPER.BIN", "PIPER_EN.BIN", "PIPER_PT_BR.BIN"] {
                            if let Some(sz) = fs.lookup_file_size(name) {
                                if sz > MAX_BOOT_PIPER {
                                    crate::serial_println!(
                                        "[PIPER] {} presente ({} KB) — skip boot load (PIO); formant ativo",
                                        name, sz / 1024
                                    );
                                    continue;
                                }
                            }
                            if let Some(data) = fs.read_file(name) {
                                let mut eng = PiperEngine::new();
                                if eng.load(&data) {
                                    crate::serial_println!("[PIPER] Piper TTS loaded from FAT ({}): {}", if *try_slave {"slave"} else {"master"}, name);
                                    ata.slave = orig;
                                    return Some(eng);
                                }
                            }
                        }
                    }
                }
                ata.slave = orig;
            }
        }
    }
    crate::serial_println!("[PIPER] Piper TTS ausente — formant synth ativo");
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
                    crate::serial_println!("[TTS] Piper (neural): \"{}\" ({} samples, multi-lang PT-BR+EN)", text, audio.len());
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
