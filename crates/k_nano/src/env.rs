use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum SystemEnv {
    Unknown = 0,
    QemuSandbox = 1,
    VBoxSandbox = 2,
    HwReal = 3,
    Offline = 4,
}

static SYSTEM_ENV: AtomicU8 = AtomicU8::new(0);
static NIC_KNOWN: AtomicBool = AtomicBool::new(false);
static NIC_OK: AtomicBool = AtomicBool::new(false);
static SLIP_DEGRADED: AtomicBool = AtomicBool::new(false);

pub fn set(env: SystemEnv) {
    SYSTEM_ENV.store(env as u8, Ordering::Release);
}

pub fn get() -> SystemEnv {
    match SYSTEM_ENV.load(Ordering::Acquire) {
        1 => SystemEnv::QemuSandbox,
        2 => SystemEnv::VBoxSandbox,
        3 => SystemEnv::HwReal,
        4 => SystemEnv::Offline,
        _ => SystemEnv::Unknown,
    }
}

pub fn is_sandbox() -> bool {
    let env = get();
    env == SystemEnv::QemuSandbox || env == SystemEnv::VBoxSandbox
}

pub fn is_online() -> bool {
    net_link_ok()
}

/// NIC física medida no boot (inclui I225 no bin). HwReal sozinho não implica link.
pub fn note_physical_nic(found: bool) {
    NIC_KNOWN.store(true, Ordering::Release);
    NIC_OK.store(found, Ordering::Release);
    if !found {
        let _ = crate::EVENT_BUS.publish(event_bus::Event {
            id: 0,
            topic: alloc::string::String::from("NETWORK_DEGRADED"),
            payload: b"NETWORK_DEGRADED:no_physical_nic".to_vec(),
            token: event_bus::CapabilityToken::Legacy(1),
        });
    }
}

pub fn note_slip_degraded(yes: bool) {
    SLIP_DEGRADED.store(yes, Ordering::Release);
}

pub fn net_link_ok() -> bool {
    if NIC_KNOWN.load(Ordering::Acquire) {
        return NIC_OK.load(Ordering::Acquire);
    }
    if get() == SystemEnv::HwReal {
        return true;
    }
    crate::nic_globals::E1000.lock().is_some()
        || crate::nic_globals::RTL8139.lock().is_some()
        || crate::nic_globals::VIRTIO_DEV.lock().is_some()
}

/// HUD: `NET` | `slip` | `off` (honesto — SLIP não é Net gate).
pub fn net_hud_label() -> &'static str {
    if net_link_ok() {
        "NET"
    } else if SLIP_DEGRADED.load(Ordering::Acquire) {
        "slip"
    } else {
        "off"
    }
}

pub fn name() -> &'static str {
    match get() {
        SystemEnv::QemuSandbox => "QEMU",
        SystemEnv::VBoxSandbox => "VirtualBox",
        SystemEnv::HwReal => "HW-Real",
        SystemEnv::Offline => "Offline",
        SystemEnv::Unknown => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_hud_distinguishes_slip_from_physical() {
        note_physical_nic(false);
        note_slip_degraded(true);
        assert_eq!(net_hud_label(), "slip");
        note_physical_nic(true);
        assert_eq!(net_hud_label(), "NET");
        note_physical_nic(false);
        note_slip_degraded(false);
        assert_eq!(net_hud_label(), "off");
    }
}
