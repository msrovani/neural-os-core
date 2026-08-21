//! Audio subsystem — Intel HDA capture driver (SD0 input stream).
//! Feeds real microphone audio into MIC_CAPTURE_RING for wake word → STT → TTS pipeline.

pub mod hda;

pub use hda::{init_hda, poll_hda_audio, write_hda_playback, is_ready};