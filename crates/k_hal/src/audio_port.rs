//! AudioPort — FE jarbas voz; BE HDA/UAC k-hal (H3). Cap enforce H5+.

use crate::cap_gate::{self, CapResult, HalCap};
use core::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AudioPortStatus {
    NotBound = 0,
    Bound = 1,
    Streaming = 2,
    Denied = 3,
}

static AUDIO_STATUS: AtomicU8 = AtomicU8::new(AudioPortStatus::NotBound as u8);

pub fn status() -> AudioPortStatus {
    match AUDIO_STATUS.load(Ordering::Relaxed) {
        1 => AudioPortStatus::Bound,
        2 => AudioPortStatus::Streaming,
        3 => AudioPortStatus::Denied,
        _ => AudioPortStatus::NotBound,
    }
}

pub fn set_status(s: AudioPortStatus) {
    AUDIO_STATUS.store(s as u8, Ordering::Relaxed);
}

/// FE R3: Cap FeAudio obrigatória + HDA status real.
/// Verifica CapGate E se o HDA está bound (corn buffer + SD1 ativo).
pub fn fe_stream() -> AudioPortStatus {
    match cap_gate::check_fe_bound(HalCap::FeAudio) {
        CapResult::Allow => {
            let s = status();
            if matches!(s, AudioPortStatus::Bound | AudioPortStatus::Streaming) {
                // Verifica se HDA está realmente bound (corn_buffer_ok)
                if crate::audio::is_hda_bound() {
                    set_status(AudioPortStatus::Streaming);
                    AudioPortStatus::Streaming
                } else {
                    // Cap OK mas HDA não bound — Bound mas não streaming
                    set_status(AudioPortStatus::Bound);
                    AudioPortStatus::Bound
                }
            } else {
                s
            }
        }
        CapResult::Deny => {
            set_status(AudioPortStatus::Denied);
            AudioPortStatus::Denied
        }
    }
}

/// Retorna status detalhado do áudio (para HUD/telemetria).
pub fn detailed_status() -> (AudioPortStatus, bool, bool) {
    let st = status();
    let hda_bound = crate::audio::is_hda_bound();
    let streaming = matches!(st, AudioPortStatus::Streaming);
    (st, hda_bound, streaming)
}
