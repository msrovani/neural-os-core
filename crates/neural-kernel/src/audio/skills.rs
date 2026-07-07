use alloc::vec::Vec;
use alloc::string::String;
use skill_registry::{Skill, McpManifest, OutputSchema};

pub struct TtsSkill;

impl Skill for TtsSkill {
    fn manifest(&self) -> McpManifest {
        McpManifest {
            name: String::from("tts_speak"),
            description: String::from("Sintetiza texto em audio PCM via Pocket TTS (sherpa-onnx)"),
            required_tokens: Vec::new(),
            preconditions: Vec::new(),
            context_links: Vec::new(),
            output_schema: OutputSchema::Any,
            idempotent: false,
            contracts: Vec::new(),
        }
    }

    fn execute(&self, input: &[u8]) -> Result<Vec<u8>, &'static str> {
        let text = core::str::from_utf8(input).unwrap_or("");
        crate::serial_println!("[TTS] Falando: \"{}\" (sherpa-onnx pendente — Sprint Sound)", text);
        let silent: Vec<i16> = alloc::vec![0i16; 16000];
        Ok(silent.iter().flat_map(|s| s.to_le_bytes()).collect())
    }
}

pub struct SttSkill;

impl Skill for SttSkill {
    fn manifest(&self) -> McpManifest {
        McpManifest {
            name: String::from("stt_transcribe"),
            description: String::from("Transcreve audio PCM para texto via Whisper (sherpa-onnx)"),
            required_tokens: Vec::new(),
            preconditions: Vec::new(),
            context_links: Vec::new(),
            output_schema: OutputSchema::String,
            idempotent: false,
            contracts: Vec::new(),
        }
    }

    fn execute(&self, _input: &[u8]) -> Result<Vec<u8>, &'static str> {
        crate::serial_println!("[STT] Transcrevendo audio... (sherpa-onnx pendente — Sprint Sound)");
        Ok(Vec::new())
    }
}
