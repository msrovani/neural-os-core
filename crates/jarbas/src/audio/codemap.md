# crates/jarbas/src/audio/ — JARVIS voice pipeline (ADR-0045)

**Responsibility**: voice I/O pipeline — mic (HDA|UAC) → `AUDIO_IN` →
wake-word → `WAKEWORD` → VAD → STT (`stt.rs`, MFCC→LSTM→CTC) → `USER_INTENT`;
`HERMES_RESPONSE` → Piper (`piper.rs`, VITS) / formant (`tts.rs`) → `AUDIO_OUT`
→ mixer (`mixer.rs`) → speaker; barge-in (`pipeline.rs`).

**Key symbols**: topics `TOPIC_AUDIO_IN/OUT`, `TOPIC_WAKEWORD`, `TOPIC_STT_TEXT`,
`TOPIC_TTS_CMD` (mod.rs); agents `JarbasVoiceAgent` (voice.rs),
`JarbasAgent` (jarvis.rs), `AudioPipelineAgent` (pipeline.rs),
`WakeWordAgent`, `AudioMixerAgent`; helpers `VAD` (vad.rs), `ser.rs`
(emotion), `settings.rs`, `usb.rs` (UAC1/2), `skills.rs` (Tts/Stt/volume
skills), `neural.rs` (Pocket-TTS), `ringbuf.rs` (SPSC capture ring).

**Integration**: the bin's `audio` module is a facade
(`pub use jarbas_crate::audio::*`, neural-kernel/src/audio/mod.rs) — jarbas is
the single source; bin registers the agents at main.rs:2562–2577 and calls
`init_audio()`, `stt::try_load_from_*`, `skills::synthesize_tts`.
