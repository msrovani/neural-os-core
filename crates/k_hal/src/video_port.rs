//! VideoPort — FE jarbas Vision/UVC; BE câmera via HalOffer (sem MMIO no R3). Cap H5+.

use crate::cap_gate::{self, CapResult, HalCap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoPortStatus {
    NotBound,
    Bound,
    Streaming,
    Denied,
}

static mut VIDEO_STATUS: VideoPortStatus = VideoPortStatus::NotBound;

pub fn status() -> VideoPortStatus {
    unsafe { VIDEO_STATUS }
}

pub fn set_status(s: VideoPortStatus) {
    unsafe {
        VIDEO_STATUS = s;
    }
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
