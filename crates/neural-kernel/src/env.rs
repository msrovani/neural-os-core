//! SystemEnv — ADR-0055: compartilha átomo com k_nano::env; is_online local (NET_CONFIG).

pub use k_nano::env::{SystemEnv, get, is_sandbox, name, set};

pub fn is_online() -> bool {
    if get() == SystemEnv::HwReal {
        return true;
    }
    crate::net::NET_CONFIG.lock().online
}
