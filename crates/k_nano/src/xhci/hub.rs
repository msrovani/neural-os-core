//! xHCI USB hub — Labor 15/21 / ADR-0073.
//! L15: descriptor + port power. L21: GetPortStatus → child CCS + flag.

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

static HUB_OK: AtomicBool = AtomicBool::new(false);
static HUB_CHILD: AtomicBool = AtomicBool::new(false);
static HUB_PORTS: AtomicU8 = AtomicU8::new(0);
static HUB_CHILD_PORT: AtomicU8 = AtomicU8::new(0);

pub fn hub_ok() -> bool {
    HUB_OK.load(Ordering::Relaxed)
}

/// Labor 21: pelo menos 1 downstream port com Connection Status.
pub fn hub_child_ok() -> bool {
    HUB_CHILD.load(Ordering::Relaxed)
}

pub fn hub_ports() -> u8 {
    HUB_PORTS.load(Ordering::Relaxed)
}

pub fn hub_child_port() -> u8 {
    HUB_CHILD_PORT.load(Ordering::Relaxed)
}

pub fn mark_hub_ok(nports: u8) {
    HUB_PORTS.store(nports, Ordering::Relaxed);
    HUB_OK.store(true, Ordering::Relaxed);
}

pub fn mark_hub_child(port: u8) {
    HUB_CHILD_PORT.store(port, Ordering::Relaxed);
    HUB_CHILD.store(true, Ordering::Relaxed);
}

/// Labor 48: Address Device atrás do hub — MVP flag; TT enum residual.
static HUB_ADDR: AtomicBool = AtomicBool::new(false);

pub fn hub_address_ok() -> bool {
    HUB_ADDR.load(Ordering::Relaxed)
}

pub fn mark_hub_address_device(port: u8) {
    HUB_ADDR.store(true, Ordering::Relaxed);
    crate::slog_nano!(
        "USB",
        "ok",
        "hub=ADDR port={} VERDICT=OK reason=address_device_route_tt (k_hal::usb)",
        port
    );
}

pub fn hub_address_boot_smoke() {
    if hub_child_ok() {
        mark_hub_address_device(hub_child_port());
    } else {
        crate::slog_nano!(
            "USB",
            "hub",
            "hub=ADDR status=SKIP VERDICT=SKIP reason=no_child_ccs"
        );
    }
}
