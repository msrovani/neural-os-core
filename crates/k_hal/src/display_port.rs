//! DisplayPort — FE jarbas; BE scanout k-hal (H2+). Cap enforce H5+.

use crate::cap_gate::{self, CapResult, HalCap};
use core::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DisplayPortStatus {
    NotBound = 0,
    Bound = 1,
    Presenting = 2,
    Denied = 3,
}

static DISPLAY_STATUS: AtomicU8 = AtomicU8::new(DisplayPortStatus::NotBound as u8);

pub fn status() -> DisplayPortStatus {
    match DISPLAY_STATUS.load(Ordering::Relaxed) {
        1 => DisplayPortStatus::Bound,
        2 => DisplayPortStatus::Presenting,
        3 => DisplayPortStatus::Denied,
        _ => DisplayPortStatus::NotBound,
    }
}

pub fn set_status(s: DisplayPortStatus) {
    DISPLAY_STATUS.store(s as u8, Ordering::Relaxed);
}

/// FE R3: Cap FeDisplay obrigatória.
pub fn fe_present() -> DisplayPortStatus {
    match cap_gate::check_fe_bound(HalCap::FeDisplay) {
        CapResult::Allow => {
            let s = status();
            if matches!(s, DisplayPortStatus::Bound | DisplayPortStatus::Presenting) {
                set_status(DisplayPortStatus::Presenting);
                DisplayPortStatus::Presenting
            } else {
                s
            }
        }
        CapResult::Deny => {
            set_status(DisplayPortStatus::Denied);
            DisplayPortStatus::Denied
        }
    }
}
