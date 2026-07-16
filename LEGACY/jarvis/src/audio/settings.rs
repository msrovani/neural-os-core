use alloc::vec::Vec;
use alloc::string::String;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use skill_registry::{Skill, McpManifest, OutputSchema};

pub static AUDIO_VOLUME: AtomicU8 = AtomicU8::new(80);
pub static VOICE_CLONE_ENABLED: AtomicBool = AtomicBool::new(false);
pub static WAKEWORD_SENSITIVITY: AtomicU8 = AtomicU8::new(5);
pub static CURRENT_VOICE: spin::Mutex<Option<String>> = spin::Mutex::new(None);

pub fn init_audio_settings() {
    *CURRENT_VOICE.lock() = Some(String::from("jarvis (Pocket TTS)"));
}

pub struct AudioGetSettingsSkill;

impl Skill for AudioGetSettingsSkill {
    fn manifest(&self) -> McpManifest {
        McpManifest {
            name: String::from("audio_get_settings"),
            description: String::from("Retorna configuracoes de audio atuais"),
            required_tokens: Vec::new(),
            preconditions: Vec::new(),
            context_links: Vec::new(),
            output_schema: OutputSchema::String,
            idempotent: true,
            contracts: Vec::new(),
        }
    }

    fn execute(&self, _input: &[u8]) -> Result<Vec<u8>, &'static str> {
        let voice = CURRENT_VOICE.lock();
        let voice_name = voice.as_deref().unwrap_or("default");
        let info = alloc::format!(
            "Volume: {}%\nVoice: {}\nWake Word: Jarvis\nSensitivity: {}\nVoice Clone: {}",
            AUDIO_VOLUME.load(Ordering::Relaxed),
            voice_name,
            WAKEWORD_SENSITIVITY.load(Ordering::Relaxed),
            if VOICE_CLONE_ENABLED.load(Ordering::Relaxed) { "on" } else { "off" },
        );
        Ok(info.into_bytes())
    }
}

pub struct AudioSetVolumeSkill;

impl Skill for AudioSetVolumeSkill {
    fn manifest(&self) -> McpManifest {
        McpManifest {
            name: String::from("audio_set_volume"),
            description: String::from("Define volume do audio (0-100)"),
            required_tokens: Vec::new(),
            preconditions: Vec::new(),
            context_links: Vec::new(),
            output_schema: OutputSchema::String,
            idempotent: false,
            contracts: Vec::new(),
        }
    }

    fn execute(&self, input: &[u8]) -> Result<Vec<u8>, &'static str> {
        let s = core::str::from_utf8(input).unwrap_or("80");
        let vol: u8 = s.trim().parse().unwrap_or(80).min(100);
        AUDIO_VOLUME.store(vol, Ordering::Relaxed);
        Ok(alloc::format!("Volume definido para {}%", vol).into_bytes())
    }
}

pub struct AudioToggleVoiceCloneSkill;

impl Skill for AudioToggleVoiceCloneSkill {
    fn manifest(&self) -> McpManifest {
        McpManifest {
            name: String::from("audio_toggle_voice_clone"),
            description: String::from("Ativa/desativa clonagem de voz"),
            required_tokens: Vec::new(),
            preconditions: Vec::new(),
            context_links: Vec::new(),
            output_schema: OutputSchema::String,
            idempotent: false,
            contracts: Vec::new(),
        }
    }

    fn execute(&self, _input: &[u8]) -> Result<Vec<u8>, &'static str> {
        let current = VOICE_CLONE_ENABLED.load(Ordering::Relaxed);
        VOICE_CLONE_ENABLED.store(!current, Ordering::Relaxed);
        let status = if !current { "ativada" } else { "desativada" };
        Ok(alloc::format!("Clonagem de voz {}", status).into_bytes())
    }
}
