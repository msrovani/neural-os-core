//! Intel I225/I226 (igc) 2.5G Ethernet — ADR-0062 P7 / Labor 2 Hércules.
//! Regmap queue-0 (Linux igc); descritores legados estilo e1000.
//! Honesty: QEMU **não** emula i225 (só e1000/igb 82576). Validação plena = HW real.

use alloc::vec::Vec;
use crate::memory::{GLOBAL_ALLOCATOR, PHYS_MEM_OFFSET};
use crate::pci::PciDevice;

pub const I225_VENDOR_INTEL: u16 = 0x8086;

/// Device IDs conhecidos (igc family).
pub const I225_DID_LM: u16 = 0x15F2;
pub const I225_DID_V: u16 = 0x15F3;
pub const I225_DID_I: u16 = 0x15F8;
pub const I225_DID_K: u16 = 0x3100;
pub const I225_DID_K2: u16 = 0x0D9F;
pub const I226_DID_LM: u16 = 0x125B;
pub const I226_DID_V: u16 = 0x125C;
pub const I226_DID_IT: u16 = 0x125D;
pub const I226_DID_K: u16 = 0x125E;

const KNOWN_DIDS: &[u16] = &[
    I225_DID_LM,
    I225_DID_V,
    I225_DID_I,
    I225_DID_K,
    I225_DID_K2,
    I226_DID_LM,
    I226_DID_V,
    I226_DID_IT,
    I226_DID_K,
];

pub fn is_i225_family(vendor: u16, device: u16) -> bool {
    vendor == I225_VENDOR_INTEL && KNOWN_DIDS.contains(&device)
}

// IGC / i225 register offsets (queue 0)
const REG_CTRL: u64 = 0x0000;
const REG_STATUS: u64 = 0x0008;
const REG_CTRL_EXT: u64 = 0x0018;
const REG_RCTL: u64 = 0x0100;
const REG_TCTL: u64 = 0x0400;
const REG_TIPG: u64 = 0x0410;
const REG_IMC: u64 = 0x00D8;
const REG_RDBAL: u64 = 0xC000;
const REG_RDBAH: u64 = 0xC004;
const REG_RDLEN: u64 = 0xC008;
const REG_RDH: u64 = 0xC010;
const REG_RDT: u64 = 0xC018;
const REG_TDBAL: u64 = 0xE000;
const REG_TDBAH: u64 = 0xE004;
const REG_TDLEN: u64 = 0xE008;
const REG_TDH: u64 = 0xE010;
const REG_TDT: u64 = 0xE018;
const REG_RAL: u64 = 0x5400;
const REG_RAH: u64 = 0x5404;
const REG_MTA: u64 = 0x5200;

const CTRL_RST: u32 = 1 << 26;
const CTRL_SLU: u32 = 1 << 6;
const CTRL_FD: u32 = 1;
const CTRL_RFCE: u32 = 1 << 27;
const CTRL_TFCE: u32 = 1 << 28;

const RCTL_EN: u32 = 1 << 1;
const RCTL_BAM: u32 = 1 << 15;
const RCTL_SECRC: u32 = 1 << 26;
const RCTL_UPE: u32 = 1 << 3;
const RCTL_MPE: u32 = 1 << 4;
const RCTL_BSIZE_2048: u32 = 0;

const TCTL_EN: u32 = 1 << 1;
const TCTL_PSP: u32 = 1 << 3;

const TX_N: usize = 64;
const RX_N: usize = 64;

#[repr(C, packed)]
struct TxDesc {
    addr: u64,
    length: u16,
    cso: u8,
    cmd: u8,
    status: u8,
    css: u8,
    vlan: u16,
}

#[repr(C, packed)]
struct RxDesc {
    addr: u64,
    length: u16,
    csum: u16,
    status: u8,
    errors: u8,
    special: u16,
}

pub struct I225Driver {
    mmio_base: u64,
    mmio_virt: u64,
    mac_addr: [u8; 6],
    pci_bus: u8,
    pci_device: u8,
    pci_func: u8,
    device_id: u16,
    tx_ring_paddr: u64,
    rx_ring_paddr: u64,
    tx_buf_paddrs: [u64; TX_N],
    rx_buf_paddrs: [u64; RX_N],
    tx_cur: usize,
    rx_cur: usize,
}

impl I225Driver {
    pub unsafe fn new(dev: &PciDevice) -> Option<Self> {
        if !is_i225_family(dev.vendor_id, dev.device_id) {
            return None;
        }
        let mmio_base = (dev.bar0 & !0xF) as u64;
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        crate::pci::enable_pci_bus_master(dev);
        crate::slog_nano!(
            "Net",
            "i225",
            "Detectado DID={:#06x} MMIO={:#x} (igc Q0)",
            dev.device_id,
            mmio_base
        );
        Some(I225Driver {
            mmio_base,
            mmio_virt: mmio_base + pmoff,
            mac_addr: [0; 6],
            pci_bus: dev.bus,
            pci_device: dev.device,
            pci_func: dev.function,
            device_id: dev.device_id,
            tx_ring_paddr: 0,
            rx_ring_paddr: 0,
            tx_buf_paddrs: [0; TX_N],
            rx_buf_paddrs: [0; RX_N],
            tx_cur: 0,
            rx_cur: 0,
        })
    }

    pub unsafe fn read32(&self, reg: u64) -> u32 {
        core::ptr::read_volatile((self.mmio_virt + reg) as *const u32)
    }

    unsafe fn write32(&self, reg: u64, val: u32) {
        core::ptr::write_volatile((self.mmio_virt + reg) as *mut u32, val);
    }

    fn alloc_frame() -> u64 {
        let mut g = GLOBAL_ALLOCATOR.lock();
        g.as_mut()
            .and_then(|a| a.allocate_contiguous(1))
            .map(|f| f.start_address().as_u64())
            .unwrap_or(0)
    }

    pub unsafe fn init(&mut self) -> bool {
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        // igc BAR spans farther (Q0 @ 0xC000/0xE000) — map 64KB
        for page in 0..16 {
            crate::apic::map_page_uc(self.mmio_base + page * 0x1000, pmoff);
        }

        self.write32(REG_IMC, !0);
        self.write32(REG_CTRL, CTRL_RST);
        for _ in 0..200_000 {
            if self.read32(REG_CTRL) & CTRL_RST == 0 {
                break;
            }
            core::hint::spin_loop();
        }
        crate::pci::enable_pci_bus_master_unsafe(self.pci_bus, self.pci_device, self.pci_func);

        // MAC from RAL0 (flash/NVM may already program it)
        let low = self.read32(REG_RAL);
        let high = self.read32(REG_RAH);
        self.mac_addr = [
            (low & 0xFF) as u8,
            ((low >> 8) & 0xFF) as u8,
            ((low >> 16) & 0xFF) as u8,
            ((low >> 24) & 0xFF) as u8,
            (high & 0xFF) as u8,
            ((high >> 8) & 0xFF) as u8,
        ];
        if self.mac_addr.iter().all(|&b| b == 0) {
            // Fallback locally-administered MAC for bring-up
            self.mac_addr = [0x02, 0x22, 0x5A, 0x00, 0x00, 0x01];
        }
        let mac = self.mac_addr;
        let ral = (mac[3] as u32) << 24
            | (mac[2] as u32) << 16
            | (mac[1] as u32) << 8
            | mac[0] as u32;
        let rah = (mac[5] as u32) << 8 | mac[4] as u32 | 0x8000_0000;
        self.write32(REG_RAL, ral);
        self.write32(REG_RAH, rah);

        self.write32(REG_CTRL, CTRL_SLU | CTRL_FD | CTRL_RFCE | CTRL_TFCE);
        let _ = self.read32(REG_CTRL_EXT);
        self.write32(REG_TIPG, 0x0060_2008);

        for i in 0..128 {
            self.write32(REG_MTA + (i as u64) * 4, 0);
        }

        let tx_ring = Self::alloc_frame();
        let rx_ring = Self::alloc_frame();
        if tx_ring == 0 || rx_ring == 0 {
            crate::slog_nano!("Net", "i225", "alloc ring FAIL");
            return false;
        }
        self.tx_ring_paddr = tx_ring;
        self.rx_ring_paddr = rx_ring;
        crate::apic::map_page_uc(tx_ring, pmoff);
        crate::apic::map_page_uc(rx_ring, pmoff);
        core::ptr::write_bytes((tx_ring + pmoff) as *mut u8, 0, 4096);
        core::ptr::write_bytes((rx_ring + pmoff) as *mut u8, 0, 4096);

        for i in 0..TX_N {
            let b = Self::alloc_frame();
            if b == 0 {
                return false;
            }
            crate::apic::map_page_uc(b, pmoff);
            self.tx_buf_paddrs[i] = b;
            let d = (tx_ring + pmoff) as *mut TxDesc;
            (*d.add(i)).addr = b;
            (*d.add(i)).cmd = 0;
            (*d.add(i)).status = 1; // DD free
        }
        for i in 0..RX_N {
            let b = Self::alloc_frame();
            if b == 0 {
                return false;
            }
            crate::apic::map_page_uc(b, pmoff);
            self.rx_buf_paddrs[i] = b;
            let d = (rx_ring + pmoff) as *mut RxDesc;
            (*d.add(i)).addr = b;
            (*d.add(i)).status = 0;
        }

        self.write32(REG_TDBAL, tx_ring as u32);
        self.write32(REG_TDBAH, (tx_ring >> 32) as u32);
        self.write32(REG_TDLEN, (core::mem::size_of::<TxDesc>() * TX_N) as u32);
        self.write32(REG_TDH, 0);
        self.write32(REG_TDT, 0);

        self.write32(REG_RDBAL, rx_ring as u32);
        self.write32(REG_RDBAH, (rx_ring >> 32) as u32);
        self.write32(REG_RDLEN, (core::mem::size_of::<RxDesc>() * RX_N) as u32);
        self.write32(REG_RDH, 0);
        self.write32(REG_RDT, (RX_N as u32).wrapping_sub(1));

        self.write32(REG_TCTL, TCTL_EN | TCTL_PSP | (0x10 << 4) | (0x40 << 12));
        self.write32(
            REG_RCTL,
            RCTL_EN | RCTL_BAM | RCTL_SECRC | RCTL_UPE | RCTL_MPE | RCTL_BSIZE_2048,
        );

        crate::slog_nano!(
            "Net",
            "i225",
            "Init OK DID={:#06x} MAC={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} STATUS={:#x}",
            self.device_id,
            mac[0],
            mac[1],
            mac[2],
            mac[3],
            mac[4],
            mac[5],
            self.read32(REG_STATUS)
        );
        true
    }

    pub fn mac(&self) -> [u8; 6] {
        self.mac_addr
    }

    pub fn device_id(&self) -> u16 {
        self.device_id
    }

    pub unsafe fn send(&mut self, data: &[u8]) -> bool {
        if data.is_empty() || data.len() > 1518 {
            return false;
        }
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let idx = self.tx_cur;
        let desc = (self.tx_ring_paddr + pmoff) as *mut TxDesc;
        let d = &mut *desc.add(idx);
        if d.status & 1 == 0 {
            return false; // still owned by HW
        }
        let buf = (self.tx_buf_paddrs[idx] + pmoff) as *mut u8;
        core::ptr::copy_nonoverlapping(data.as_ptr(), buf, data.len());
        d.addr = self.tx_buf_paddrs[idx];
        d.length = data.len() as u16;
        d.cso = 0;
        d.cmd = 0x0B; // EOP|IFCS|RS
        d.status = 0;
        d.css = 0;
        d.vlan = 0;
        core::arch::asm!("sfence", options(nostack, preserves_flags));
        let next = (idx + 1) % TX_N;
        self.write32(REG_TDT, next as u32);
        self.tx_cur = next;
        true
    }

    pub unsafe fn recv(&mut self) -> Option<Vec<u8>> {
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let idx = self.rx_cur;
        let desc = (self.rx_ring_paddr + pmoff) as *mut RxDesc;
        let d = &mut *desc.add(idx);
        if d.status & 1 == 0 {
            return None;
        }
        let len = d.length as usize;
        if len < 14 || len > 2048 {
            d.status = 0;
            self.write32(REG_RDT, idx as u32);
            self.rx_cur = (idx + 1) % RX_N;
            return None;
        }
        let mut out = Vec::with_capacity(len);
        let src = (self.rx_buf_paddrs[idx] + pmoff) as *const u8;
        for i in 0..len {
            out.push(src.add(i).read_volatile());
        }
        d.status = 0;
        core::arch::asm!("sfence", options(nostack, preserves_flags));
        self.write32(REG_RDT, idx as u32);
        self.rx_cur = (idx + 1) % RX_N;
        Some(out)
    }

    pub unsafe fn dump_status(&self) {
        crate::slog_nano!(
            "Net",
            "i225",
            "STATUS={:#x} CTRL={:#x} RDH={} RDT={} TDH={} TDT={}",
            self.read32(REG_STATUS),
            self.read32(REG_CTRL),
            self.read32(REG_RDH),
            self.read32(REG_RDT),
            self.read32(REG_TDH),
            self.read32(REG_TDT)
        );
    }
}
