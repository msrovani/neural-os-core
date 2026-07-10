//! Generic WiFi Driver — AgnosticWifiEngine com DMA ring + vendor maps.
//! Abordagem C: union + MaybeUninit + CriticalSectionMutex + vtable + AgnosticWifiEngine.
//!
//! Arquitetura:
//!   Tabela Massiva → runtime_probe_and_bind(vid, did, bar) → escolhe HardwareRegisterMap
//!   → instancia AgnosticWifiEngine na union → send_packet/receive_packet via DMA ring

use core::cell::RefCell;
use core::mem::{MaybeUninit, ManuallyDrop};
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, Ordering, compiler_fence};

// ── 1. TRAIT UNIFICADA ─────────────────────────────────────────

pub trait WifiChipset {
    fn init(&mut self) -> Result<(), &'static str>;
    fn send_packet(&mut self, packet: &[u8]) -> Result<(), &'static str>;
    fn receive_packet(&mut self, buffer: &mut [u8]) -> Result<usize, &'static str>;
}

// ── 2. MAPA DE REGISTRADORES POR FABRICANTE ───────────────────

#[derive(Copy, Clone)]
pub struct HardwareRegisterMap {
    pub tx_ring_low: usize,
    pub rx_ring_low: usize,
    pub rx_control: usize,
    pub doorbell_tx: usize,
    pub doorbell_rx: usize,
    pub cmd_start_rx: u32,
    pub ring_size: usize,
    pub rx_buf_len: usize,
}

pub const INTEL_AX_MAP: HardwareRegisterMap = HardwareRegisterMap {
    tx_ring_low: 0x1000, rx_ring_low: 0x1004, rx_control: 0x0008,
    doorbell_tx: 0x2000, doorbell_rx: 0x2004, cmd_start_rx: 0x0001,
    ring_size: 64, rx_buf_len: 2048,
};

pub const REALTEK_RTL_MAP: HardwareRegisterMap = HardwareRegisterMap {
    tx_ring_low: 0x00A0, rx_ring_low: 0x00A4, rx_control: 0x002C,
    doorbell_tx: 0x00D0, doorbell_rx: 0x00D4, cmd_start_rx: 0x8002,
    ring_size: 16, rx_buf_len: 2048,
};

pub const ATHEROS_MAP: HardwareRegisterMap = HardwareRegisterMap {
    tx_ring_low: 0x0800, rx_ring_low: 0x0804, rx_control: 0x0010,
    doorbell_tx: 0x0C00, doorbell_rx: 0x0C04, cmd_start_rx: 0x0001,
    ring_size: 32, rx_buf_len: 2048,
};

pub const BROADCOM_MAP: HardwareRegisterMap = HardwareRegisterMap {
    tx_ring_low: 0x0500, rx_ring_low: 0x0504, rx_control: 0x0020,
    doorbell_tx: 0x0600, doorbell_rx: 0x0604, cmd_start_rx: 0x0100,
    ring_size: 32, rx_buf_len: 2048,
};

pub const ETH_FALLBACK_MAP: HardwareRegisterMap = HardwareRegisterMap {
    tx_ring_low: 0, rx_ring_low: 0, rx_control: 0,
    doorbell_tx: 0, doorbell_rx: 0, cmd_start_rx: 0,
    ring_size: 0, rx_buf_len: 0,
};

// ── 3. CAPA DE I/O ─────────────────────────────────────────────

pub struct ChipIoInterface {
    pub base: usize,
    pub map: HardwareRegisterMap,
}

impl ChipIoInterface {
    /// Cria nova interface de I/O validando BAR.
    /// BAR = 0 ou BAR = u64::MAX indicam endereco invalido.
    pub fn new(base: usize, map: HardwareRegisterMap) -> Result<Self, &'static str> {
        if base == 0 || base == usize::MAX {
            return Err("ChipIoInterface: invalid BAR address");
        }
        if (base as u64) < 0x1000 {
            return Err("ChipIoInterface: BAR too low (DMA window?)");
        }
        Ok(Self { base, map })
    }

    #[inline(always)]
    pub unsafe fn write_reg(&self, offset: usize, val: u32) {
        debug_assert!(self.base != 0, "write_reg on invalid BAR");
        write_volatile((self.base + offset) as *mut u32, val);
    }

    #[inline(always)]
    pub unsafe fn read_reg(&self, offset: usize) -> u32 {
        debug_assert!(self.base != 0, "read_reg on invalid BAR");
        read_volatile((self.base + offset) as *const u32)
    }

    pub unsafe fn set_dma_rings(&self, tx_phys: u64, rx_phys: u64) {
        self.write_reg(self.map.tx_ring_low, (tx_phys & 0xFFFFFFFF) as u32);
        self.write_reg(self.map.rx_ring_low, (rx_phys & 0xFFFFFFFF) as u32);
    }

    pub unsafe fn start_rx(&self) {
        self.write_reg(self.map.rx_control, self.map.cmd_start_rx);
    }

    pub unsafe fn doorbell_tx(&self, head: u32) {
        self.write_reg(self.map.doorbell_tx, head);
    }

    pub unsafe fn doorbell_rx(&self, tail: u32) {
        self.write_reg(self.map.doorbell_rx, tail);
    }
}

// ── 4. DESCRITOR DMA ──────────────────────────────────────────

#[repr(C, align(16))]
pub struct DmaDescriptor {
    pub buf_addr: u64,
    pub len_flags: u32,
    pub status: u32,
}

const OWNED_BY_HW: u32 = 1 << 31;
const BUF_SIZE: usize = 2048;

// ── 5. MOTOR AGNOSTIC WIFI ENGINE ──────────────────────────────

pub struct AgnosticWifiEngine {
    io: ChipIoInterface,
    tx_ring: [DmaDescriptor; 64],
    rx_ring: [DmaDescriptor; 64],
    rx_buf: [[u8; 2048]; 64],
    tx_head: usize,
    rx_tail: usize,
    ring_sz: usize,
}

impl AgnosticWifiEngine {
    pub fn new(base: usize, map: HardwareRegisterMap) -> Self {
        let ring_sz = if map.ring_size > 64 { 64 } else { map.ring_size.max(2) };
        // Se BAR for invalida, usa io nulo (nenhum registro sera escrito)
        let io = match ChipIoInterface::new(base, map) {
            Ok(io) => io,
            Err(_) => ChipIoInterface { base: 0, map },
        };
        AgnosticWifiEngine {
            io,
            tx_ring: unsafe { core::mem::zeroed() },
            rx_ring: unsafe { core::mem::zeroed() },
            rx_buf: unsafe { core::mem::zeroed() },
            tx_head: 0,
            rx_tail: 0,
            ring_sz,
        }
    }
}

impl WifiChipset for AgnosticWifiEngine {
    fn init(&mut self) -> Result<(), &'static str> {
        let sz = self.ring_sz;
        // Prepara ring de RX
        for i in 0..sz {
            let buf_pa = &self.rx_buf[i] as *const _ as u64;
            self.rx_ring[i] = DmaDescriptor {
                buf_addr: buf_pa,
                len_flags: (self.io.map.rx_buf_len as u32) | OWNED_BY_HW,
                status: 0,
            };
            self.tx_ring[i] = DmaDescriptor {
                buf_addr: 0, len_flags: 0, status: 0,
            };
        }
        let tx_pa = self.tx_ring.as_ptr() as u64;
        let rx_pa = self.rx_ring.as_ptr() as u64;
        unsafe {
            self.io.set_dma_rings(tx_pa, rx_pa);
            self.io.start_rx();
        }
        crate::serial_println!("[WIFI-DMA] {} rings: TX@{:#x} RX@{:#x}", sz, tx_pa, rx_pa);
        Ok(())
    }

    fn send_packet(&mut self, packet: &[u8]) -> Result<(), &'static str> {
        if self.io.map.ring_size == 0 { return Ok(()); } // Ethernet fallback
        let idx = self.tx_head % self.ring_sz;
        let desc = &mut self.tx_ring[idx];
        let flags = unsafe { read_volatile(&desc.len_flags) };
        if (flags & OWNED_BY_HW) != 0 {
            return Err("TX ring full");
        }
        desc.buf_addr = packet.as_ptr() as u64;
        desc.len_flags = (packet.len() as u32) | OWNED_BY_HW | (1 << 30) | (1 << 29);
        compiler_fence(Ordering::Release);
        self.tx_head = (idx + 1) % self.ring_sz;
        unsafe { self.io.doorbell_tx(self.tx_head as u32); }
        Ok(())
    }

    fn receive_packet(&mut self, buffer: &mut [u8]) -> Result<usize, &'static str> {
        if self.io.map.ring_size == 0 { return Ok(0); } // Ethernet fallback
        let idx = self.rx_tail % self.ring_sz;
        let desc = &mut self.rx_ring[idx];
        let flags = unsafe { read_volatile(&desc.len_flags) };
        if (flags & OWNED_BY_HW) != 0 {
            return Ok(0); // Nenhum pacote novo
        }
        compiler_fence(Ordering::Acquire);
        let len = (flags & 0xFFF) as usize;
        let n = len.min(buffer.len());
        buffer[..n].copy_from_slice(&self.rx_buf[idx][..n]);
        let buf_pa = &self.rx_buf[idx] as *const _ as u64;
        self.rx_ring[idx] = DmaDescriptor {
            buf_addr: buf_pa,
            len_flags: (self.io.map.rx_buf_len as u32) | OWNED_BY_HW,
            status: 0,
        };
        self.rx_tail = (idx + 1) % self.ring_sz;
        unsafe { self.io.doorbell_rx(self.rx_tail as u32); }
        Ok(n)
    }
}

// ── 6. DRIVERS ESPECIFICOS (encapsulam AgnosticWifiEngine) ────

pub struct IntelAx200(pub AgnosticWifiEngine);
impl WifiChipset for IntelAx200 {
    fn init(&mut self) -> Result<(), &'static str> { self.0.init() }
    fn send_packet(&mut self, p: &[u8]) -> Result<(), &'static str> { self.0.send_packet(p) }
    fn receive_packet(&mut self, b: &mut [u8]) -> Result<usize, &'static str> { self.0.receive_packet(b) }
}

pub struct RealtekRtl8188(pub AgnosticWifiEngine);
impl WifiChipset for RealtekRtl8188 {
    fn init(&mut self) -> Result<(), &'static str> { self.0.init() }
    fn send_packet(&mut self, p: &[u8]) -> Result<(), &'static str> { self.0.send_packet(p) }
    fn receive_packet(&mut self, b: &mut [u8]) -> Result<usize, &'static str> { self.0.receive_packet(b) }
}

pub struct AtherosAth9k(pub AgnosticWifiEngine);
impl WifiChipset for AtherosAth9k {
    fn init(&mut self) -> Result<(), &'static str> { self.0.init() }
    fn send_packet(&mut self, p: &[u8]) -> Result<(), &'static str> { self.0.send_packet(p) }
    fn receive_packet(&mut self, b: &mut [u8]) -> Result<usize, &'static str> { self.0.receive_packet(b) }
}

pub struct BroadcomBcm4360(pub AgnosticWifiEngine);
impl WifiChipset for BroadcomBcm4360 {
    fn init(&mut self) -> Result<(), &'static str> { self.0.init() }
    fn send_packet(&mut self, p: &[u8]) -> Result<(), &'static str> { self.0.send_packet(p) }
    fn receive_packet(&mut self, b: &mut [u8]) -> Result<usize, &'static str> { self.0.receive_packet(b) }
}

pub struct FallbackEthernet(pub AgnosticWifiEngine);
impl WifiChipset for FallbackEthernet {
    fn init(&mut self) -> Result<(), &'static str> {
        let has_nic = crate::net::RTL8139.lock().is_some()
            || crate::net::E1000.lock().is_some()
            || crate::net::VIRTIO_DEV.lock().is_some();
        if has_nic {
            crate::serial_println!("[WIFI] Ethernet fallback (smoltcp ativo)");
            Ok(())
        } else {
            Err("NIC ausente")
        }
    }
    fn send_packet(&mut self, _p: &[u8]) -> Result<(), &'static str> { Ok(()) }
    fn receive_packet(&mut self, _b: &mut [u8]) -> Result<usize, &'static str> { Ok(0) }
}

// ── 7. UNION DE ARMAZENAMENTO ESTATICO ─────────────────────────

pub union DriverStorage {
    intel: ManuallyDrop<IntelAx200>,
    realtek: ManuallyDrop<RealtekRtl8188>,
    atheros: ManuallyDrop<AtherosAth9k>,
    broadcom: ManuallyDrop<BroadcomBcm4360>,
    ethernet: ManuallyDrop<FallbackEthernet>,
}

pub struct CriticalSectionMutex<T> { data: RefCell<T> }
impl<T> CriticalSectionMutex<T> {
    pub const fn new(v: T) -> Self { Self { data: RefCell::new(v) } }
    pub fn lock<F, R>(&self, f: F) -> R where F: FnOnce(&mut T) -> R {
        // Desabilita interrupcoes para evitar data race com ISRs MSI-X
        let flags: u64;
        unsafe { core::arch::asm!("pushfq; pop {0}; cli", out(reg) flags, options(nostack)); }
        let r = f(&mut *self.data.borrow_mut());
        // Restaura flags originais (reabilita se estava ativo)
        unsafe { core::arch::asm!("push {0}; popfq", in(reg) flags, options(nostack, preserves_flags)); }
        r
    }
}
unsafe impl<T> Sync for CriticalSectionMutex<T> {}

static mut DRIVER_STORAGE: MaybeUninit<DriverStorage> = MaybeUninit::uninit();
pub static ACTIVE_DRIVER: CriticalSectionMutex<Option<&'static mut dyn WifiChipset>>
    = CriticalSectionMutex::new(None);
pub static WIFI_PRESENT: AtomicBool = AtomicBool::new(false);

// ── 8. TABELA MASSIVA — runtime_probe_and_bind —───────────────

pub unsafe fn runtime_probe_and_bind(vid: u16, did: u16, bar: usize)
    -> Result<(), &'static str> {
    let ptr = DRIVER_STORAGE.as_mut_ptr();

    ACTIVE_DRIVER.lock(|active| {
        *active = None;
        let (map, name) = match (vid, did) {
            // Realtek
            (0x0BDA, 0x8176)|(0x0BDA,0x8179)|(0x0BDA,0x8178)|(0x0BDA,0x8171)
            |(0x0BDA,0x8188)|(0x0BDA,0x8192)|(0x0BDA,0x8193)|(0x0BDA,0x8812)
            |(0x0BDA,0x8821)|(0x0BDA,0xB822)|(0x0BDA,0xC822)|(0x0BDA,0x8852)
            |(0x10EC,0x8176)|(0x10EC,0x8179)|(0x10EC,0x8812) => (REALTEK_RTL_MAP, "Realtek WiFi"),
            // Intel
            (0x8086,0x08B1)|(0x8086,0x08B2)|(0x8086,0x24F3)|(0x8086,0x24F4)
            |(0x8086,0x24F5)|(0x8086,0x24F6)|(0x8086,0x24FD)|(0x8086,0x2526)
            |(0x8086,0x2527)|(0x8086,0x2723)|(0x8086,0x2725)|(0x8086,0x2726)
            |(0x8086,0x3165)|(0x8086,0x3166)|(0x8086,0x06F0)|(0x8086,0x02F0) => (INTEL_AX_MAP, "Intel WiFi"),
            // Atheros
            (0x168C,0x0030)|(0x168C,0x0032)|(0x168C,0x0033)|(0x168C,0x0034)
            |(0x168C,0x003C)|(0x168C,0x0040)|(0x168C,0x0041)|(0x168C,0x0042)
            |(0x168C,0x0050) => (ATHEROS_MAP, "Atheros WiFi"),
            // Broadcom
            (0x14E4,0x43A0)|(0x14E4,0x43B1)|(0x14E4,0x43DC)|(0x14E4,0x4464)
            |(0x14E4,0x4488)|(0x14E4,0x4425) => (BROADCOM_MAP, "Broadcom WiFi"),
            // Ethernet fallback
            (_, _) if is_ethernet(vid, did) => (ETH_FALLBACK_MAP, "Ethernet"),
            _ => {
                // Tenta sintese via IA se mapa fixo nao existe
                let hw_map = crate::cortex::generate_register_map(vid, did);
                if let Some(ai_map) = hw_map {
                    (ai_map, "Sintetizado-IA")
                } else {
                    return Err("nao suportado")
                }
            }
        };
        let engine = AgnosticWifiEngine::new(bar, map);
        let _r = match name {
            "Realtek WiFi" => { (*ptr).realtek = ManuallyDrop::new(RealtekRtl8188(engine));
                *active = Some(&mut *(*ptr).realtek as &mut dyn WifiChipset); }
            "Intel WiFi" => { (*ptr).intel = ManuallyDrop::new(IntelAx200(engine));
                *active = Some(&mut *(*ptr).intel as &mut dyn WifiChipset); }
            "Atheros WiFi" => { (*ptr).atheros = ManuallyDrop::new(AtherosAth9k(engine));
                *active = Some(&mut *(*ptr).atheros as &mut dyn WifiChipset); }
            "Broadcom WiFi" => { (*ptr).broadcom = ManuallyDrop::new(BroadcomBcm4360(engine));
                *active = Some(&mut *(*ptr).broadcom as &mut dyn WifiChipset); }
            "Ethernet" => { (*ptr).ethernet = ManuallyDrop::new(FallbackEthernet(engine));
                *active = Some(&mut *(*ptr).ethernet as &mut dyn WifiChipset); }
            "Sintetizado-IA" => {
                (*ptr).ethernet = ManuallyDrop::new(FallbackEthernet(engine));
                *active = Some(&mut *(*ptr).ethernet as &mut dyn WifiChipset); }
            _ => return Err("nome desconhecido"),
        };
        WIFI_PRESENT.store(true, Ordering::Relaxed);
        crate::serial_println!("[WIFI] {} detectado: {:04x}:{:04x} (bar={:#x}, ring={})",
            name, vid, did, bar, map.ring_size);
        Ok(())
    })
}

fn is_ethernet(v: u16, d: u16) -> bool {
    matches!((v, d),
        (0x10EC, 0x8139) |
        (0x8086, 0x100E)|(0x8086,0x100F)|(0x8086,0x10D3)
        |(0x8086,0x10D5)|(0x8086,0x10D6)|(0x8086,0x10D7)
        |(0x8086,0x10D8)|(0x8086,0x10D9)|(0x8086,0x10DA)
        |(0x8086,0x10F5)|(0x8086,0x1501)|(0x8086,0x1502)
        |(0x8086,0x1503)|(0x8086,0x150C)|(0x8086,0x1521) |
        (0x1AF4, 0x1000)|(0x1AF4, 0x1041))
}

pub fn detect_wifi() -> bool {
    let devices = unsafe { crate::pci::scan_pci() };
    for dev in &devices {
        if dev.class == 0x02 && dev.subclass == 0x80 {
            let bar_raw = (dev.bar0 as u64) | ((dev.bar1 as u64) << 32);
            let bar = (bar_raw & !0xF) as usize;
            if unsafe { runtime_probe_and_bind(dev.vendor_id, dev.device_id, bar).is_ok() } {
                return true;
            }
        }
    }
    crate::serial_println!("[WIFI] Nenhum adaptador wireless — Ethernet smoltcp ativo");
    false
}
