//! AudioPort — FE jarbas voz; BE HDA/UAC k-hal (H3). Cap enforce H5+.

use crate::cap_gate::{self, CapResult, HalCap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioPortStatus {
    NotBound,
    Bound,
    Streaming,
    Denied,
}

static mut AUDIO_STATUS: AudioPortStatus = AudioPortStatus::NotBound;

pub fn status() -> AudioPortStatus {
    unsafe { AUDIO_STATUS }
}

pub fn set_status(s: AudioPortStatus) {
    unsafe {
        AUDIO_STATUS = s;
    }
}

/// FE R3: Cap FeAudio obrigatória.
pub fn fe_stream() -> AudioPortStatus {
    match cap_gate::check_fe_bound(HalCap::FeAudio) {
        CapResult::Allow => {
            let s = status();
            if matches!(s, AudioPortStatus::Bound | AudioPortStatus::Streaming) {
                set_status(AudioPortStatus::Streaming);
                AudioPortStatus::Streaming
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
