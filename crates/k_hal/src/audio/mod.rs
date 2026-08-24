//! Audio BE — HDA MMIO (ADR-0041 H3). jarbas voz/TTS/STT = FE via AudioPort.

pub mod hda;

use crate::audio_port::{self, AudioPortStatus};

pub fn register_hda_bound() {
    audio_port::set_status(AudioPortStatus::Bound);
    k_nano::slog_hal!("AUD", "info", "HDA Bound (BE owner=k_hal)");
}

pub fn set_streaming(on: bool) {
    audio_port::set_status(if on {
        AudioPortStatus::Streaming
    } else {
        AudioPortStatus::Bound
    });
}

/// Poll HDA via BE (delegado a hda::poll_hda_audio).
pub fn poll() {
    hda::poll_hda_audio();
}

/// Verifica se o HDA está bound (corn buffer + SD1 ativo).
pub fn is_hda_bound() -> bool {
    matches!(
        audio_port::status(),
        AudioPortStatus::Bound | AudioPortStatus::Streaming
    )
}
