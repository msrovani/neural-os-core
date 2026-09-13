//! Modern VirtIO drivers using the `virtio-drivers` crate (rcore-os).
//!
//! Uses PCI modern transport (BAR-based common config) via [`virtio_hal::AiosHal`].
//! Falls back to legacy manual drivers in `virtio_net.rs` / `virtio_blk.rs` when
//! the device doesn't support modern transport.
//!
//! Key advantage over legacy: proper descriptor chain management, buffer recycling,
//! feature negotiation handled by the crate, no manual ring manipulation.

extern crate alloc;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;
use spin::Mutex;

use virtio_drivers::transport::pci::PciTransport;
use virtio_drivers::transport::Transport;
use virtio_drivers::device::net::VirtIONet;
use virtio_drivers::device::blk::VirtIOBlk;

use crate::virtio_hal::{AiosHal, AiosPciAccess};
use crate::pci::PciDevice;

const NET_QUEUE_SIZE: usize = 64;
const NET_BUF_SIZE: usize = 2048;

// ---------------------------------------------------------------------------
// Modern VirtIO-Net wrapper
// ---------------------------------------------------------------------------

/// Wraps the crate's `VirtIONet` to match k_nano's `send`/`recv` API.
pub struct VirtIoNetModern {
    inner: VirtIONet<AiosHal, PciTransport, NET_QUEUE_SIZE>,
    mac: [u8; 6],
}

impl VirtIoNetModern {
    /// Try to create a modern VirtIO-net driver from a PCI device.
    /// Returns `None` if the device doesn't support modern transport.
    pub fn try_new(dev: &PciDevice) -> Option<Self> {
        use virtio_drivers::transport::pci::bus::DeviceFunction;
        use virtio_drivers::transport::pci::virtio_device_type;
        use virtio_drivers::transport::DeviceType;

        let df = DeviceFunction {
            bus: dev.bus,
            device: dev.device,
            function: dev.function,
        };

        let mut root = virtio_drivers::transport::pci::bus::PciRoot::new(AiosPciAccess);
        let transport = match PciTransport::new::<AiosHal, _>(&mut root, df) {
            Ok(t) => t,
            Err(e) => {
                crate::slog_nano!("VIRTIO", "warn",
                    "Modern net transport failed: {:?}", e);
                return None;
            }
        };

        if transport.device_type() != DeviceType::Network {
            return None;
        }

        let mut net = match VirtIONet::<AiosHal, PciTransport, NET_QUEUE_SIZE>::new(
            transport, NET_BUF_SIZE,
        ) {
            Ok(n) => n,
            Err(e) => {
                crate::slog_nano!("VIRTIO", "warn",
                    "VirtIONet::new failed: {:?}", e);
                return None;
            }
        };

        let mac = net.mac_address();
        crate::slog_nano!("VIRTIO", "ok",
            "Modern VirtIO-net OK. MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);

        Some(VirtIoNetModern { inner: net, mac })
    }

    pub fn mac(&self) -> [u8; 6] { self.mac }

    pub fn send(&mut self, data: &[u8]) -> bool {
        let mut tx_buf = self.inner.new_tx_buffer(data.len());
        tx_buf.packet_mut().copy_from_slice(data);
        self.inner.send(tx_buf).is_ok()
    }

    pub fn recv(&mut self) -> Option<Vec<u8>> {
        let rx_buf = self.inner.receive().ok()?;
        let data = Vec::from(rx_buf.packet());
        self.inner.recycle_rx_buffer(rx_buf).ok()?;
        Some(data)
    }
}

// ---------------------------------------------------------------------------
// Modern VirtIO-Block wrapper
// ---------------------------------------------------------------------------

/// Wraps the crate's `VirtIOBlk` to implement `BlockDevice`.
pub struct VirtIoBlkModern {
    inner: VirtIOBlk<AiosHal, PciTransport>,
    capacity: u64,
    readonly: bool,
}

impl VirtIoBlkModern {
    /// Try to create a modern VirtIO-blk driver from a PCI device.
    pub fn try_new(dev: &PciDevice) -> Option<Self> {
        use virtio_drivers::transport::pci::bus::DeviceFunction;
        use virtio_drivers::transport::pci::virtio_device_type;
        use virtio_drivers::transport::DeviceType;

        let df = DeviceFunction {
            bus: dev.bus,
            device: dev.device,
            function: dev.function,
        };

        let mut root = virtio_drivers::transport::pci::bus::PciRoot::new(AiosPciAccess);
        let transport = match PciTransport::new::<AiosHal, _>(&mut root, df) {
            Ok(t) => t,
            Err(e) => {
                crate::slog_nano!("VBLK", "warn",
                    "Modern blk transport failed: {:?}", e);
                return None;
            }
        };

        if transport.device_type() != DeviceType::Block {
            return None;
        }

        let capacity;
        let readonly;
        match VirtIOBlk::<AiosHal, PciTransport>::new(transport) {
            Ok(blk) => {
                capacity = blk.capacity();
                readonly = blk.readonly();
                crate::slog_nano!("VBLK", "ok",
                    "Modern VirtIO-blk OK. cap={}MB ro={}",
                    capacity * 512 / (1024 * 1024), readonly);
                Some(VirtIoBlkModern { inner: blk, capacity, readonly })
            }
            Err(e) => {
                crate::slog_nano!("VBLK", "warn",
                    "VirtIOBlk::new failed: {:?}", e);
                None
            }
        }
    }

    pub fn capacity_sectors(&self) -> u64 { self.capacity }
    pub fn readonly(&self) -> bool { self.readonly }

    pub fn read_sectors(&mut self, lba: u64, buf: &mut [u8]) -> bool {
        let sector_size = virtio_drivers::device::blk::SECTOR_SIZE as u64;
        let n_sectors = buf.len() as u64 / sector_size;
        if lba + n_sectors > self.capacity {
            return false;
        }
        // Read sector by sector (the crate reads one sector at a time)
        let mut offset = 0usize;
        for i in 0..n_sectors {
            let mut sec_buf = [0u8; virtio_drivers::device::blk::SECTOR_SIZE];
            if self.inner.read_blocks(lba as usize + i as usize, &mut sec_buf).is_err() {
                return false;
            }
            let end = core::cmp::min(offset + sec_buf.len(), buf.len());
            buf[offset..end].copy_from_slice(&sec_buf[..end - offset]);
            offset = end;
        }
        true
    }

    pub fn write_sectors(&mut self, lba: u64, buf: &[u8]) -> bool {
        if self.readonly { return false; }
        let sector_size = virtio_drivers::device::blk::SECTOR_SIZE as u64;
        let n_sectors = buf.len() as u64 / sector_size;
        if lba + n_sectors > self.capacity { return false; }
        let mut offset = 0usize;
        for i in 0..n_sectors {
            let end = core::cmp::min(offset + sector_size as usize, buf.len());
            let mut sec_buf = [0u8; virtio_drivers::device::blk::SECTOR_SIZE];
            let copy_len = end - offset;
            sec_buf[..copy_len].copy_from_slice(&buf[offset..end]);
            if self.inner.write_blocks(lba as usize + i as usize, &sec_buf).is_err() {
                return false;
            }
            offset = end;
        }
        true
    }
}

// ---------------------------------------------------------------------------
// Global state
// ---------------------------------------------------------------------------

/// Modern VirtIO-net driver (populated by `init_driver_virtio_modern`).
pub static MODERN_NET: Mutex<Option<VirtIoNetModern>> = Mutex::new(None);

/// Modern VirtIO-blk driver (populated by `init_driver_virtio_blk_modern`).
pub static MODERN_BLK: Mutex<Option<VirtIoBlkModern>> = Mutex::new(None);

/// Initialize modern VirtIO-net. Returns true if a modern device was found.
pub unsafe fn init_driver_virtio_modern() -> bool {
    if MODERN_NET.lock().is_some() { return true; }

    let devices = crate::pci::scan_pci();
    for dev in &devices {
        if dev.vendor_id == crate::virtio_net::VIRTIO_VENDOR &&
           (dev.device_id == crate::virtio_net::VIRTIO_NET_TRANSITIONAL ||
            dev.device_id == crate::virtio_net::VIRTIO_NET_MODERN) {
            if let Some(net) = VirtIoNetModern::try_new(dev) {
                let mac = net.mac();
                crate::nic_globals::NET_CONFIG.lock().mac = mac;
                *MODERN_NET.lock() = Some(net);
                return true;
            }
        }
    }
    false
}

/// Initialize modern VirtIO-blk. Returns true if a modern device was found.
pub unsafe fn init_driver_virtio_blk_modern() -> bool {
    if MODERN_BLK.lock().is_some() { return true; }

    let devices = crate::pci::scan_pci();
    for dev in &devices {
        if dev.vendor_id == crate::virtio_net::VIRTIO_VENDOR &&
           (dev.device_id == crate::virtio_blk::VIRTIO_BLK_TRANSITIONAL ||
            dev.device_id == crate::virtio_blk::VIRTIO_BLK_MODERN) {
            if let Some(blk) = VirtIoBlkModern::try_new(dev) {
                *MODERN_BLK.lock() = Some(blk);
                return true;
            }
        }
    }
    false
}
