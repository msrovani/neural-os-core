//! SoftMAC bridge ath10k ↔ smoltcp Ethernet (ADR-0066 Labor 22).
//! MVP: ring RX/TX eth frames; feed só com CapToken::WifiAssociated.
//! Net gate QEMU permanece e1000 — wifi SoftMAC = Note path.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;

const RING: usize = 8;
const MTU: usize = 1514;

struct SoftMac {
    rx: [Option<Vec<u8>>; RING],
    tx: [Option<Vec<u8>>; RING],
    rx_i: usize,
    tx_i: usize,
}

impl SoftMac {
    const fn empty() -> Self {
        Self {
            rx: [const { None }; RING],
            tx: [const { None }; RING],
            rx_i: 0,
            tx_i: 0,
        }
    }
}

static SM: Mutex<SoftMac> = Mutex::new(SoftMac::empty());
static ENABLED: AtomicBool = AtomicBool::new(false);
static RX_N: AtomicU32 = AtomicU32::new(0);
static TX_N: AtomicU32 = AtomicU32::new(0);

/// Liga SoftMAC quando assoc OK (Note). QEMU sem chip → no-op.
pub fn enable_if_associated() {
    if crate::unlock_dag::has(crate::unlock_dag::CapToken::WifiAssociated) {
        ENABLED.store(true, Ordering::Relaxed);
        k_nano::slog_bin!(
            "WIFI-HW",
            "info",
            "step=softmac status=OK VERDICT=PARTIAL reason=bridge_armed device=NetPhy (DHCP/HTTP L31)"
        );
    }
}

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Empurra frame Ethernet (já descapsulado do 802.11 data) para smoltcp.
pub fn push_rx_eth(frame: &[u8]) -> bool {
    if !is_enabled() || frame.len() < 14 || frame.len() > MTU {
        return false;
    }
    let mut g = SM.lock();
    let i = g.rx_i % RING;
    g.rx[i] = Some(frame.to_vec());
    g.rx_i = g.rx_i.wrapping_add(1);
    RX_N.fetch_add(1, Ordering::Relaxed);
    true
}

/// Pop RX para Device::receive do medium wifi (FE).
pub fn pop_rx_eth() -> Option<Vec<u8>> {
    let mut g = SM.lock();
    for slot in g.rx.iter_mut() {
        if slot.is_some() {
            RX_N.fetch_add(0, Ordering::Relaxed); // keep
            return slot.take();
        }
    }
    None
}

/// Queue TX Ethernet → driver encapsula 802.11 (residual encapsulate).
pub fn push_tx_eth(frame: &[u8]) -> bool {
    if !is_enabled() || frame.len() < 14 || frame.len() > MTU {
        return false;
    }
    let mut g = SM.lock();
    let i = g.tx_i % RING;
    g.tx[i] = Some(frame.to_vec());
    g.tx_i = g.tx_i.wrapping_add(1);
    TX_N.fetch_add(1, Ordering::Relaxed);
    true
}

pub fn pop_tx_eth() -> Option<Vec<u8>> {
    let mut g = SM.lock();
    for slot in g.tx.iter_mut() {
        if slot.is_some() {
            return slot.take();
        }
    }
    None
}

pub fn stats() -> (u32, u32) {
    (RX_N.load(Ordering::Relaxed), TX_N.load(Ordering::Relaxed))
}

/// Labor 31: path DHCP/HTTP via SoftMAC — honesty; e1000 = QEMU gate.
pub fn dhcp_http_path_smoke() {
    if !is_enabled() {
        k_nano::slog_bin!(
            "WIFI-HW",
            "info",
            "step=wifi_net status=SKIP VERDICT=AWAITING_REAL_HW reason=softmac_off (e1000=net_gate)"
        );
        return;
    }
    let (rx, tx) = stats();
    k_nano::slog_bin!(
        "WIFI-HW",
        "info",
        "step=wifi_net status=OK rx={} tx={} VERDICT=PARTIAL reason=device_wired_await_dhcp_rf",
        rx,
        tx
    );
}

/// Boot smoke — honesty; sem inventar PASS RF.
pub fn boot_smoke() {
    enable_if_associated();
    if is_enabled() {
        let _ = push_rx_eth(&[
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x02, 0x00, 0x00, 0x00, 0x00, 0x01, 0x08, 0x00,
            0x45, 0x00,
        ]);
        let (rx, tx) = stats();
        k_nano::slog_bin!(
            "WIFI-HW",
            "info",
            "step=softmac status=OK rx={} tx={} VERDICT=PARTIAL reason=ring_ok",
            rx,
            tx
        );
    } else {
        k_nano::slog_bin!(
            "WIFI-HW",
            "info",
            "step=softmac status=SKIP VERDICT=AWAITING_REAL_HW reason=no_WifiAssociated (e1000=net_gate)"
        );
    }
}
