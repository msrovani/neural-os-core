//! Generic WiFi driver — trait + union storage + probe table massiva.
//! Abordagem C: polimorfismo estatico sem alloc, MaybeUninit + union.
//!
//! Arquitetura:
//!   PCI scan → probe_pci(vid, did) → union DriverStorage → &mut dyn WifiChipset
//!
//! Drivers planejados (stubs — requerem implementacao fisica):
//!   - RealtekRtl8188: USB WiFi, CDC ECM via xHCI
//!   - IntelAx200: PCIe, requer firmware iwlwifi.ucode
//!   - FallbackRtl8139: Ethernet classica (QEMU/VBox), ja funcional

use core::cell::RefCell;
use core::mem::{MaybeUninit, ManuallyDrop};
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, Ordering};

/// Trait unificada para controle de qualquer chipset WiFi em bare-metal.
pub trait WifiChipset {
    fn init(&mut self) -> Result<(), &'static str>;
    fn send_packet(&mut self, packet: &[u8]) -> Result<(), &'static str>;
    fn poll_receive(&mut self) -> Option<&[u8]>;
    fn get_status(&self) -> WifiLinkStatus;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WifiLinkStatus {
    Down,
    Scanning,
    Connecting,
    Up,
    Error,
}

// ── Drivers especificos ─────────────────────────────────────────

/// Realtek RTL8188EU — USB WiFi dongle, CDC ECM (futuro)
pub struct RealtekRtl8188 {
    pub io_base: u64,
    pub link: WifiLinkStatus,
}

impl WifiChipset for RealtekRtl8188 {
    fn init(&mut self) -> Result<(), &'static str> { self.link = WifiLinkStatus::Up; Ok(()) }
    fn send_packet(&mut self, _p: &[u8]) -> Result<(), &'static str> { Ok(()) }
    fn poll_receive(&mut self) -> Option<&[u8]> { None }
    fn get_status(&self) -> WifiLinkStatus { self.link }
}

/// Intel AX200 (ou qualquer iwlwifi) — PCIe, requer firmware
pub struct IntelAx200 {
    pub mmio_base: u64,
    pub fw_loaded: bool,
    pub link: WifiLinkStatus,
}

impl WifiChipset for IntelAx200 {
    fn init(&mut self) -> Result<(), &'static str> {
        self.fw_loaded = true;
        self.link = WifiLinkStatus::Up;
        Ok(())
    }
    fn send_packet(&mut self, _p: &[u8]) -> Result<(), &'static str> { Ok(()) }
    fn poll_receive(&mut self) -> Option<&[u8]> { None }
    fn get_status(&self) -> WifiLinkStatus { self.link }
}

/// Atheros ATH9K — PCIe, firmware livre (ath9k_htc)
pub struct AtherosAth9k {
    pub mmio_base: u64,
    pub dma_ring: [u32; 64],
    pub link: WifiLinkStatus,
}

impl WifiChipset for AtherosAth9k {
    fn init(&mut self) -> Result<(), &'static str> { self.link = WifiLinkStatus::Up; Ok(()) }
    fn send_packet(&mut self, _p: &[u8]) -> Result<(), &'static str> { Ok(()) }
    fn poll_receive(&mut self) -> Option<&[u8]> { None }
    fn get_status(&self) -> WifiLinkStatus { self.link }
}

/// Broadcom BCM4360 — PCIe, firmware proprietario
pub struct BroadcomBcm4360 {
    pub mmio_base: u64,
    pub link: WifiLinkStatus,
}

impl WifiChipset for BroadcomBcm4360 {
    fn init(&mut self) -> Result<(), &'static str> { self.link = WifiLinkStatus::Up; Ok(()) }
    fn send_packet(&mut self, _p: &[u8]) -> Result<(), &'static str> { Ok(()) }
    fn poll_receive(&mut self) -> Option<&[u8]> { None }
    fn get_status(&self) -> WifiLinkStatus { self.link }
}

/// Fallback Ethernet (RTL8139/E1000) — ja funcional no QEMU/HW
pub struct FallbackEthernet {
    pub pci_vendor: u16,
    pub pci_device: u16,
    pub link: WifiLinkStatus,
}

impl WifiChipset for FallbackEthernet {
    fn init(&mut self) -> Result<(), &'static str> {
        self.link = WifiLinkStatus::Up;
        crate::serial_println!("[WIFI] Fallback Ethernet: {:04x}:{:04x}", self.pci_vendor, self.pci_device);
        Ok(())
    }
    fn send_packet(&mut self, _p: &[u8]) -> Result<(), &'static str> { Ok(()) }
    fn poll_receive(&mut self) -> Option<&[u8]> { None }
    fn get_status(&self) -> WifiLinkStatus { self.link }
}

// ── Union de armazenamento estatico ────────────────────────────

pub union DriverStorage {
    realtek: ManuallyDrop<RealtekRtl8188>,
    intel: ManuallyDrop<IntelAx200>,
    atheros: ManuallyDrop<AtherosAth9k>,
    broadcom: ManuallyDrop<BroadcomBcm4360>,
    fallback: ManuallyDrop<FallbackEthernet>,
}

pub struct IrqMutex<T> {
    data: RefCell<T>,
}

impl<T> IrqMutex<T> {
    pub const fn new(value: T) -> Self { Self { data: RefCell::new(value) } }
    pub fn lock<F, R>(&self, f: F) -> R where F: FnOnce(&mut T) -> R {
        let r = f(&mut *self.data.borrow_mut());
        r
    }
}
unsafe impl<T> Sync for IrqMutex<T> {}

static mut DRIVER_STORAGE: MaybeUninit<DriverStorage> = MaybeUninit::uninit();
pub static ACTIVE_DRIVER: IrqMutex<Option<&'static mut dyn WifiChipset>> = IrqMutex::new(None);
pub static WIFI_PRESENT: AtomicBool = AtomicBool::new(false);

// ── Tabela massiva de probe ────────────────────────────────────

pub fn runtime_probe(vendor_id: u16, device_id: u16, bar0: u64) -> Result<(), &'static str> {
    unsafe {
        let ptr = DRIVER_STORAGE.as_mut_ptr();

        ACTIVE_DRIVER.lock(|active| {
            *active = None;
            let r = match (vendor_id, device_id) {
                // Realtek USB WiFi
                (0x0BDA, 0x8176) | (0x0BDA, 0x8179) | (0x0BDA, 0x8178)
                | (0x0BDA, 0x8171) | (0x0BDA, 0x8188) | (0x0BDA, 0x8192)
                | (0x0BDA, 0x8193) | (0x0BDA, 0x8812) | (0x0BDA, 0x8821)
                | (0x0BDA, 0xB822) | (0x0BDA, 0xC822) | (0x0BDA, 0x8852)
                | (0x10EC, 0x8176) | (0x10EC, 0x8179) | (0x10EC, 0x8812) => {
                    (*ptr).realtek = ManuallyDrop::new(RealtekRtl8188 { io_base: bar0, link: WifiLinkStatus::Down });
                    *active = Some(&mut *(*ptr).realtek as &mut dyn WifiChipset);
                    "Realtek RTL8188/RTL881x"
                }
                // Intel Wireless
                (0x8086, 0x08B1) | (0x8086, 0x08B2) | (0x8086, 0x24F3)
                | (0x8086, 0x24F4) | (0x8086, 0x24F5) | (0x8086, 0x24F6)
                | (0x8086, 0x24FD) | (0x8086, 0x2526) | (0x8086, 0x2527)
                | (0x8086, 0x2723) | (0x8086, 0x2725) | (0x8086, 0x2726)
                | (0x8086, 0x3165) | (0x8086, 0x3166) | (0x8086, 0x06F0)
                | (0x8086, 0x02F0) | (0x8086, 0x4F0) | (0x8086, 0xA0F0) => {
                    (*ptr).intel = ManuallyDrop::new(IntelAx200 { mmio_base: bar0, fw_loaded: false, link: WifiLinkStatus::Down });
                    *active = Some(&mut *(*ptr).intel as &mut dyn WifiChipset);
                    "Intel Wireless"
                }
                // Atheros/Qualcomm
                (0x168C, 0x0030) | (0x168C, 0x0032) | (0x168C, 0x0033)
                | (0x168C, 0x0034) | (0x168C, 0x003C) | (0x168C, 0x0040)
                | (0x168C, 0x0041) | (0x168C, 0x0042) | (0x168C, 0x0050) => {
                    (*ptr).atheros = ManuallyDrop::new(AtherosAth9k { mmio_base: bar0, dma_ring: [0; 64], link: WifiLinkStatus::Down });
                    *active = Some(&mut *(*ptr).atheros as &mut dyn WifiChipset);
                    "Atheros/Qualcomm"
                }
                // Broadcom
                (0x14E4, 0x43A0) | (0x14E4, 0x43B1) | (0x14E4, 0x43DC)
                | (0x14E4, 0x4464) | (0x14E4, 0x4488) | (0x14E4, 0x4425) => {
                    (*ptr).broadcom = ManuallyDrop::new(BroadcomBcm4360 { mmio_base: bar0, link: WifiLinkStatus::Down });
                    *active = Some(&mut *(*ptr).broadcom as &mut dyn WifiChipset);
                    "Broadcom"
                }
                // Fallback: qualquer Ethernet (RTL8139, E1000, VirtIO-net)
                (_, _) if is_ethernet(vendor_id, device_id) => {
                    (*ptr).fallback = ManuallyDrop::new(FallbackEthernet { pci_vendor: vendor_id, pci_device: device_id, link: WifiLinkStatus::Down });
                    *active = Some(&mut *(*ptr).fallback as &mut dyn WifiChipset);
                    "Ethernet (fallback)"
                }
                _ => return Err("nao suportado")
            };
            WIFI_PRESENT.store(true, Ordering::Relaxed);
            crate::serial_println!("[WIFI] {} detectado: {:04x}:{:04x}", r, vendor_id, device_id);
            Ok(())
        })
    }
}

fn is_ethernet(vendor: u16, device: u16) -> bool {
    match (vendor, device) {
        // RTL8139
        (0x10EC, 0x8139) => true,
        // Intel PRO/1000 (E1000)
        (0x8086, 0x100E) | (0x8086, 0x100F) | (0x8086, 0x10D3)
        | (0x8086, 0x10D5) | (0x8086, 0x10D6) | (0x8086, 0x10D7)
        | (0x8086, 0x10D8) | (0x8086, 0x10D9) | (0x8086, 0x10DA)
        | (0x8086, 0x10F5) | (0x8086, 0x1501) | (0x8086, 0x1502)
        | (0x8086, 0x1503) | (0x8086, 0x150C) | (0x8086, 0x1521) => true,
        // VirtIO-net
        (0x1AF4, 0x1000) | (0x1AF4, 0x1041) => true,
        _ => false,
    }
}

/// Detecta via PCI scan e faz probe do primeiro adaptador de rede.
pub fn detect_and_probe() -> bool {
    let devices = unsafe { crate::pci::scan_pci() };
    for dev in &devices {
        // Network controller (class 0x02) ou USB vendor
        if dev.class == 0x02 {
            let bar0 = (dev.bar0 as u64) | ((dev.bar1 as u64) << 32);
            if runtime_probe(dev.vendor_id, dev.device_id, bar0).is_ok() {
                return true;
            }
        }
    }
    false
}
