//! VideoPort — FE jarbas Vision/UVC; BE câmera via HalOffer (sem MMIO no R3). Cap H5+.

use crate::cap_gate::{self, CapResult, HalCap};
use core::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum VideoPortStatus {
    NotBound = 0,
    Bound = 1,
    Streaming = 2,
    Denied = 3,
}

static VIDEO_STATUS: AtomicU8 = AtomicU8::new(VideoPortStatus::NotBound as u8);

pub fn status() -> VideoPortStatus {
    match VIDEO_STATUS.load(Ordering::Relaxed) {
        1 => VideoPortStatus::Bound,
        2 => VideoPortStatus::Streaming,
        3 => VideoPortStatus::Denied,
        _ => VideoPortStatus::NotBound,
    }
}

pub fn set_status(s: VideoPortStatus) {
    VIDEO_STATUS.store(s as u8, Ordering::Relaxed);
}

/// FE R3: Cap FeVideo obrigatória.
pub fn fe_frame() -> VideoPortStatus {
    match cap_gate::check_fe_bound(HalCap::FeVideo) {
        CapResult::Allow => {
            let s = status();
            if matches!(s, VideoPortStatus::Bound | VideoPortStatus::Streaming) {
                set_status(VideoPortStatus::Streaming);
                VideoPortStatus::Streaming
            } else {
                s
            }
        }
        CapResult::Deny => {
            set_status(VideoPortStatus::Denied);
            VideoPortStatus::Denied
        }
    }
}
