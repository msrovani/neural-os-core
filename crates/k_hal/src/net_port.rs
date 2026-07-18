//! NetPort — FE hermes; BE k-hal (H3). Cap enforce H5+.

use crate::cap_gate::{self, CapResult, HalCap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetPortStatus {
    NotBound,
    Bound,
    Up,
    Denied,
}

static mut NET_STATUS: NetPortStatus = NetPortStatus::NotBound;

pub fn status() -> NetPortStatus {
    unsafe { NET_STATUS }
}

pub fn set_status(s: NetPortStatus) {
    unsafe {
        NET_STATUS = s;
    }
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
