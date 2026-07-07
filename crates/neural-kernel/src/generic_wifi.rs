//! Generic WiFi Driver — Abordagem C: union + MaybeUninit + CriticalSectionMutex + vtable.
//! Implementado em tempo de execucao sem alloc, sem Box.
//!
//! Arquitetura:
//!   [Barramento PCIe] → scan_pci() → runtime_probe_and_bind(vid, did, bar)
//!     → union DriverStorage (maior driver) → &mut dyn WifiChipset → loop de rede

use core::cell::RefCell;
use core::mem::{MaybeUninit, ManuallyDrop};
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, Ordering};

// ── 1. TRAIT UNIFICADA ─────────────────────────────────────────

pub trait WifiChipset {
    fn init(&mut self) -> Result<(), &'static str>;
    fn send_packet(&mut self, packet: &[u8]) -> Result<(), &'static str>;
    /// Recebe um pacote. Preenche buffer[0..n] com dados recebidos, retorna n.
    /// Retorna Ok(0) se nenhum pacote disponivel.
    /// Compatível com smoltcp::phy::Device via bridge WifiPhyDevice.
    fn receive_packet(&mut self, buffer: &mut [u8]) -> Result<usize, &'static str>;
}

// ── 2. DRIVERS ESPECIFICOS ─────────────────────────────────────

pub struct RealtekRtl8188 {
    pub base_address: usize,
    pub tx_ready: bool,
}
impl WifiChipset for RealtekRtl8188 {
    fn init(&mut self) -> Result<(), &'static str> {
        unsafe { write_volatile((self.base_address + 0x20) as *mut u32, 0x1); }
        self.tx_ready = true;
        Ok(())
    }
    fn send_packet(&mut self, _p: &[u8]) -> Result<(), &'static str> {
        if !self.tx_ready { return Err("Realtek TX Busy"); }
        Ok(())
    }
    fn receive_packet(&mut self, _b: &mut [u8]) -> Result<usize, &'static str> { Ok(0) }
}

pub struct IntelAx200 {
    pub base_address: usize,
    pub firmware_loaded: bool,
}
impl WifiChipset for IntelAx200 {
    fn init(&mut self) -> Result<(), &'static str> {
        self.firmware_loaded = true;
        Ok(())
    }
    fn send_packet(&mut self, _p: &[u8]) -> Result<(), &'static str> { Ok(()) }
    fn receive_packet(&mut self, _b: &mut [u8]) -> Result<usize, &'static str> { Ok(0) }
}

pub struct AtherosAth9k {
    pub base_address: usize,
    pub dma_ring: [u32; 64],
    pub int_mask: u32,
}
impl WifiChipset for AtherosAth9k {
    fn init(&mut self) -> Result<(), &'static str> {
        unsafe { write_volatile((self.base_address + 0x40) as *mut u32, 0x00FF00FF); }
        self.int_mask = 0x00FF00FF;
        Ok(())
    }
    fn send_packet(&mut self, _p: &[u8]) -> Result<(), &'static str> { Ok(()) }
    fn receive_packet(&mut self, _b: &mut [u8]) -> Result<usize, &'static str> { Ok(0) }
}

pub struct BroadcomBcm4360 {
    pub base_address: usize,
}
impl WifiChipset for BroadcomBcm4360 {
    fn init(&mut self) -> Result<(), &'static str> { Ok(()) }
    fn send_packet(&mut self, _p: &[u8]) -> Result<(), &'static str> { Ok(()) }
    fn receive_packet(&mut self, _b: &mut [u8]) -> Result<usize, &'static str> { Ok(0) }
}

pub struct FallbackEthernet {
    pub vendor: u16,
    pub device: u16,
}
impl WifiChipset for FallbackEthernet {
    fn init(&mut self) -> Result<(), &'static str> {
        // Verifica se algum driver NIC ja foi inicializado
        let has_nic = crate::net::RTL8139.lock().is_some()
            || crate::net::E1000.lock().is_some()
            || crate::net::VIRTIO_DEV.lock().is_some();
        if has_nic {
            crate::serial_println!("[WIFI] Ethernet fallback: {:04x}:{:04x} (nic ativo)", self.vendor, self.device);
            Ok(())
        } else {
            crate::serial_println!("[WIFI] Ethernet fallback: {:04x}:{:04x} (sem nic)", self.vendor, self.device);
            Err("nenhum driver NIC encontrado")
        }
    }
    fn send_packet(&mut self, _p: &[u8]) -> Result<(), &'static str> {
        // nic_send() em netstack.rs ja tenta RTL8139/E1000/VirtIO primeiro
        Ok(())
    }
    fn receive_packet(&mut self, _b: &mut [u8]) -> Result<usize, &'static str> {
        Ok(0) // nic_recv() em netstack.rs ja polla os NICs
    }
}

// ── 3. UNION DE ARMAZENAMENTO ESTATICO ─────────────────────────

/// Tamanho = maior driver (AtherosAth9k = 64*4 + 8 + 4 = ~268 bytes)
pub union DriverStorage {
    realtek: ManuallyDrop<RealtekRtl8188>,
    intel: ManuallyDrop<IntelAx200>,
    atheros: ManuallyDrop<AtherosAth9k>,
    broadcom: ManuallyDrop<BroadcomBcm4360>,
    fallback: ManuallyDrop<FallbackEthernet>,
}

// ── 4. MUTEX CRITICAL SECTION ──────────────────────────────────

pub struct CriticalSectionMutex<T> {
    data: RefCell<T>,
}
impl<T> CriticalSectionMutex<T> {
    pub const fn new(value: T) -> Self {
        Self { data: RefCell::new(value) }
    }
    pub fn lock<F, R>(&self, f: F) -> R where F: FnOnce(&mut T) -> R {
        // NOTA: Em hardware real, disable_interrupts() aqui
        let r = f(&mut *self.data.borrow_mut());
        // NOTA: enable_interrupts() aqui
        r
    }
}
unsafe impl<T> Sync for CriticalSectionMutex<T> {}

// ── 5. GLOBAIS ESTATICOS ───────────────────────────────────────

static mut DRIVER_MEMORY: MaybeUninit<DriverStorage> = MaybeUninit::uninit();
pub static ACTIVE_DRIVER: CriticalSectionMutex<Option<&'static mut dyn WifiChipset>>
    = CriticalSectionMutex::new(None);
pub static WIFI_PRESENT: AtomicBool = AtomicBool::new(false);

// ── 6. TABELA MASSIVA DE PROBE ─────────────────────────────────

pub unsafe fn runtime_probe_and_bind(vendor_id: u16, device_id: u16,
    base_address: usize) -> Result<(), &'static str> {
    let ptr = DRIVER_MEMORY.as_mut_ptr();

    ACTIVE_DRIVER.lock(|active| {
        *active = None;
        let r = match (vendor_id, device_id) {
            // Realtek USB WiFi
            (0x0BDA, 0x8176) | (0x0BDA, 0x8179) | (0x0BDA, 0x8178)
            | (0x0BDA, 0x8171) | (0x0BDA, 0x8188) | (0x0BDA, 0x8192)
            | (0x0BDA, 0x8193) | (0x0BDA, 0x8812) | (0x0BDA, 0x8821)
            | (0x0BDA, 0xB822) | (0x0BDA, 0xC822) | (0x0BDA, 0x8852)
            | (0x10EC, 0x8176) | (0x10EC, 0x8179) | (0x10EC, 0x8812) => {
                (*ptr).realtek = ManuallyDrop::new(RealtekRtl8188 { base_address, tx_ready: false });
                *active = Some(&mut *(*ptr).realtek as &mut dyn WifiChipset);
                "Realtek RTL8188/RTL881x"
            }
            // Intel Wireless
            (0x8086, 0x08B1) | (0x8086, 0x08B2) | (0x8086, 0x24F3)
            | (0x8086, 0x24F4) | (0x8086, 0x24F5) | (0x8086, 0x24F6)
            | (0x8086, 0x24FD) | (0x8086, 0x2526) | (0x8086, 0x2527)
            | (0x8086, 0x2723) | (0x8086, 0x2725) | (0x8086, 0x2726)
            | (0x8086, 0x3165) | (0x8086, 0x3166) | (0x8086, 0x06F0)
            | (0x8086, 0x02F0) => {
                (*ptr).intel = ManuallyDrop::new(IntelAx200 { base_address, firmware_loaded: false });
                *active = Some(&mut *(*ptr).intel as &mut dyn WifiChipset);
                "Intel Wireless"
            }
            // Atheros/Qualcomm
            (0x168C, 0x0030) | (0x168C, 0x0032) | (0x168C, 0x0033)
            | (0x168C, 0x0034) | (0x168C, 0x003C) | (0x168C, 0x0040)
            | (0x168C, 0x0041) | (0x168C, 0x0042) | (0x168C, 0x0050) => {
                (*ptr).atheros = ManuallyDrop::new(AtherosAth9k {
                    base_address, dma_ring: [0; 64], int_mask: 0 });
                *active = Some(&mut *(*ptr).atheros as &mut dyn WifiChipset);
                "Atheros/Qualcomm"
            }
            // Broadcom
            (0x14E4, 0x43A0) | (0x14E4, 0x43B1) | (0x14E4, 0x43DC)
            | (0x14E4, 0x4464) | (0x14E4, 0x4488) | (0x14E4, 0x4425) => {
                (*ptr).broadcom = ManuallyDrop::new(BroadcomBcm4360 { base_address });
                *active = Some(&mut *(*ptr).broadcom as &mut dyn WifiChipset);
                "Broadcom"
            }
            // Fallback Ethernet (RTL8139, E1000, VirtIO-net)
            (_, _) if is_ethernet(vendor_id, device_id) => {
                (*ptr).fallback = ManuallyDrop::new(FallbackEthernet { vendor: vendor_id, device: device_id });
                *active = Some(&mut *(*ptr).fallback as &mut dyn WifiChipset);
                "Ethernet (fallback)"
            }
            _ => return Err("não suportado")
        };
        WIFI_PRESENT.store(true, Ordering::Relaxed);
        crate::serial_println!("[WIFI] {} detectado: {:04x}:{:04x}", r, vendor_id, device_id);
        Ok(())
    })
}

fn is_ethernet(vendor: u16, device: u16) -> bool {
    matches!((vendor, device),
        (0x10EC, 0x8139) |
        (0x8086, 0x100E) | (0x8086, 0x100F) | (0x8086, 0x10D3)
        | (0x8086, 0x10D5) | (0x8086, 0x10D6) | (0x8086, 0x10D7)
        | (0x8086, 0x10D8) | (0x8086, 0x10D9) | (0x8086, 0x10DA)
        | (0x8086, 0x10F5) | (0x8086, 0x1501) | (0x8086, 0x1502)
        | (0x8086, 0x1503) | (0x8086, 0x150C) | (0x8086, 0x1521) |
        (0x1AF4, 0x1000) | (0x1AF4, 0x1041))
}

/// Detecta adaptador WiFi (class 02/80 = wireless). Fallback Ethernet ja ativo via smoltcp.
pub fn detect_wifi() -> bool {
    let devices = unsafe { crate::pci::scan_pci() };
    for dev in &devices {
        // So procura wireless (subclass 0x80). Ethernet ja funciona via smoltcp/RTL8139.
        if dev.class == 0x02 && dev.subclass == 0x80 {
            let bar_raw = (dev.bar0 as u64) | ((dev.bar1 as u64) << 32);
            let bar = (bar_raw & !0xF) as usize;
            if unsafe { runtime_probe_and_bind(dev.vendor_id, dev.device_id, bar).is_ok() } {
                return true;
            }
        }
    }
    // Nenhum WiFi encontrado — Ethernet cabeada ja esta ativa via smoltcp/DHCP.
    // Nao criar FallbackEthernet, nao pedir SSID ao usuario.
    crate::serial_println!("[WIFI] Nenhum adaptador wireless — usando Ethernet existente.");
    false
}



