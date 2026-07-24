use core::sync::atomic::{AtomicU8, Ordering};

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
    if get() == SystemEnv::HwReal {
        return true;
    }
    // ponytail: check if any NIC driver is initialized
    crate::nic_globals::E1000.lock().is_some()
        || crate::nic_globals::RTL8139.lock().is_some()
        || crate::nic_globals::VIRTIO_DEV.lock().is_some()
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
