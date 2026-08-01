# crates/neural-kernel/src/audio/

Voice/audio facade (single `mod.rs`): `pub use jarbas_crate::audio::*` — the bin declares
`mod audio` and re-exports the jarbas crate's audio tree wholesale (E4 "emagrecer").
No local logic remains.

## Key symbols

Voice agents (`audio::jarvis::JarbasAgent`, `audio::voice::JarbasVoiceAgent`,
`audio::wakeword::WakeWordAgent`, `audio::pipeline::AudioPipelineAgent`,
`audio::mixer::AudioMixerAgent`), TTS/STT skills, settings contract, and
`TOPIC_AUDIO_IN/OUT`, `TOPIC_WAKEWORD`, `TOPIC_STT_TEXT`, `TOPIC_TTS_CMD`.

## Integration

Agents registered in `kernel_boot()` after Display; skills registered via
`register_builtin_skills()` (`TtsSkill`, `SttSkill`, `Audio*SettingsSkill`,
`EmotionalContextSkill`); `jarbas_bridge::topics_in_sync()`/`settings_contract_ok()`
verify the mirror contract at boot.
