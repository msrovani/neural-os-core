//! NetPort — FE hermes; BE k-hal (H3). Cap enforce H5+.

use crate::cap_gate::{self, CapResult, HalCap};
use core::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NetPortStatus {
    NotBound = 0,
    Bound = 1,
    Up = 2,
    Denied = 3,
}

static NET_STATUS: AtomicU8 = AtomicU8::new(NetPortStatus::NotBound as u8);

pub fn status() -> NetPortStatus {
    match NET_STATUS.load(Ordering::Relaxed) {
        1 => NetPortStatus::Bound,
        2 => NetPortStatus::Up,
        3 => NetPortStatus::Denied,
        _ => NetPortStatus::NotBound,
    }
}

pub fn set_status(s: NetPortStatus) {
    NET_STATUS.store(s as u8, Ordering::Relaxed);
}

/// FE R3: exige Cap FeNet (HalOffer bind). Sem Cap → Denied.
pub fn fe_tick() -> NetPortStatus {
    match cap_gate::check_fe_bound(HalCap::FeNet) {
        CapResult::Allow => {
            let s = status();
            if s == NetPortStatus::NotBound {
                NetPortStatus::NotBound
            } else {
                s
            }
        }
        CapResult::Deny => {
            set_status(NetPortStatus::Denied);
            NetPortStatus::Denied
        }
    }
}
