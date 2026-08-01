# crates/k_hal/src/audio/ — Intel HDA Backend (R1)

**Responsibility**: Intel HDA audio driver (ADR-0041 H3) — controller reset, CORB/RIRB
verb ring, codec probe, and dual DMA stream descriptors: SD0 capture (mic) + SD1
playback (speaker), 16-bit 48kHz mono, fixed phys buffers (0x103000/0x104000).

**Key symbols**: `hda.rs::{HdaAudioAgent, init_hda, poll_hda_audio,
write_hda_playback}`; `mod.rs::{register_hda_bound, set_streaming, poll}` — the
`HdaAudioAgent` (AgentManifest "hda_audio", Driver/Oneshot) is registered in the bin
agent fleet.

**Integration**: `mod::poll()` → `hda::poll_hda_audio()` publishes `AUDIO_IN` on
`k_nano::EVENT_BUS`; `jarbas::audio::{mixer, voice}` call
`k_hal::audio::hda::{write_hda_playback, poll_hda_audio}`. Port status synced via
`audio_port` (`AudioPortStatus::Bound/Streaming`); Cap FeAudio enforced at the port.
