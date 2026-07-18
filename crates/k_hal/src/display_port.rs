//! DisplayPort — FE jarbas; BE scanout k-hal (H2+). Cap enforce H5+.

use crate::cap_gate::{self, CapResult, HalCap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayPortStatus {
    NotBound,
    Bound,
    Presenting,
    Denied,
}

static mut DISPLAY_STATUS: DisplayPortStatus = DisplayPortStatus::NotBound;

pub fn status() -> DisplayPortStatus {
    unsafe { DISPLAY_STATUS }
}

pub fn set_status(s: DisplayPortStatus) {
    unsafe {
        DISPLAY_STATUS = s;
    }
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
