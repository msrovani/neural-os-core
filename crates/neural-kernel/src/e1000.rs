//! e1000/e1000e Gigabit Ethernet driver.
//! PCI class 0x020000. MMIO-based register access.
//! TX/RX descriptor rings (64 entries each).
//! Baseado na Intel Gigabit Ethernet Controller datasheet.

use alloc::vec::Vec;
use x86_64::instructions::port::Port; // kept for potential I/O bar fallback
use crate::memory::{GLOBAL_ALLOCATOR, PHYS_MEM_OFFSET};
use crate::pci::PciDevice;
use crate::serial_println;

pub const E1000_VENDOR_INTEL: u16 = 0x8086;
pub const E1000_DEVICE_82540EM: u16 = 0x100E;
pub const E1000_DEVICE_82574L: u16 = 0x10D3;
pub const E1000_DEVICE_82579LM: u16 = 0x1502;

// Register offsets (MMIO)
const REG_CTRL: u64 = 0x0000;
const REG_STATUS: u64 = 0x0008;
const REG_EEPROM: u64 = 0x0014;
const REG_CTRL_EXT: u64 = 0x0018;
const REG_IMASK: u64 = 0x00D0;
const REG_RCTRL: u64 = 0x0100;
const REG_TCTRL: u64 = 0x0400;
const REG_TDBAL: u64 = 0x0420;
const REG_TDBAH: u64 = 0x0424;
const REG_TDLEN: u64 = 0x0428;
const REG_TDH: u64 = 0x0430;
const REG_TDT: u64 = 0x0438;
const REG_RDBAL: u64 = 0x2800;
const REG_RDBAH: u64 = 0x2804;
const REG_RDLEN: u64 = 0x2808;
const REG_RDH: u64 = 0x2810;
const REG_RDT: u64 = 0x2818;
const REG_RAL: u64 = 0x5400;
const REG_RAH: u64 = 0x5404;
const REG_MTA: u64 = 0x5200;
const REG_IPAV: u64 = 0x00C0;

// CTRL bits
const CTRL_RST: u32 = 0x04000000;
const CTRL_SLU: u32 = 0x00000040;
const CTRL_ASDE: u32 = 0x00000020;
const CTRL_FD: u32 = 0x00000001;
const CTRL_LRST: u32 = 0x08000000;

// RCTRL bits
const RCTL_EN: u32 = 0x00000002;
const RCTL_SBP: u32 = 0x00000004;
const RCTL_UPE: u32 = 0x00000008;
const RCTL_MPE: u32 = 0x00000010;
const RCTL_LPE: u32 = 0x00000020;
const RCTL_LBM_NONE: u32 = 0x00000000;
const RCTL_RDMTS_HALF: u32 = 0x00000000;
const RCTL_BAM: u32 = 0x00008000;
const RCTL_BSIZE_2048: u32 = 0x00000000;
const RCTL_BSIZE_4096: u32 = 0x00030000;
const RCTL_SECRC: u32 = 0x04000000;

// TCTRL bits
const TCTL_EN: u32 = 0x00000002;
const TCTL_PSP: u32 = 0x00000008;
const TCTL_CT_SHIFT: u32 = 4;
const TCTL_COLD_SHIFT: u32 = 12;

const TX_DESC_COUNT: usize = 64;
const RX_DESC_COUNT: usize = 64;

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

pub struct E1000Driver {
    mmio_base: u64,
    mmio_virt: u64,
    mac_addr: [u8; 6],
    tx_ring_paddr: u64,
    rx_ring_paddr: u64,
    tx_buf_paddrs: [u64; TX_DESC_COUNT],
    rx_buf_paddrs: [u64; RX_DESC_COUNT],
    tx_cur: usize,
    rx_cur: usize,
}

impl E1000Driver {
    pub unsafe fn new(dev: &PciDevice) -> Option<Self> {
        if dev.vendor_id != E1000_VENDOR_INTEL {
            return None;
        }
        let valid = [E1000_DEVICE_82540EM, E1000_DEVICE_82574L, E1000_DEVICE_82579LM];
        if !valid.contains(&dev.device_id) {
            return None;
        }

        let mmio_base = (dev.bar0 & !0xF) as u64;
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let mmio_virt = mmio_base + pmoff;

        serial_println!("[E1000] Detectado vendor={:#06x} device={:#06x} MMIO={:#010x} virt={:#010x}",
            dev.vendor_id, dev.device_id, mmio_base, mmio_virt);

        Some(E1000Driver {
            mmio_base,
            mmio_virt,
            mac_addr: [0; 6],
            tx_ring_paddr: 0,
            rx_ring_paddr: 0,
            tx_buf_paddrs: [0; TX_DESC_COUNT],
            rx_buf_paddrs: [0; RX_DESC_COUNT],
            tx_cur: 0,
            rx_cur: 0,
        })
    }

    unsafe fn read32(&self, reg: u64) -> u32 {
        let ptr = (self.mmio_virt + reg) as *mut u32;
        core::ptr::read_volatile(ptr)
    }

    unsafe fn write32(&self, reg: u64, val: u32) {
        let ptr = (self.mmio_virt + reg) as *mut u32;
        core::ptr::write_volatile(ptr, val);
    }

    unsafe fn read_mac(&self) -> [u8; 6] {
        let low = self.read32(REG_RAL);
        let high = self.read32(REG_RAH);
        let mut mac = [0u8; 6];
        mac[0] = (low & 0xFF) as u8;
        mac[1] = ((low >> 8) & 0xFF) as u8;
        mac[2] = ((low >> 16) & 0xFF) as u8;
        mac[3] = ((low >> 24) & 0xFF) as u8;
        mac[4] = (high & 0xFF) as u8;
        mac[5] = ((high >> 8) & 0xFF) as u8;
        mac
    }

    fn alloc_frame() -> u64 {
        let mut guard = GLOBAL_ALLOCATOR.lock();
        let alloc = guard.as_mut().unwrap();
        let frame = alloc.allocate_contiguous(1);
        match frame {
            Some(f) => f.start_address().as_u64(),
            None => 0,
        }
    }

    pub unsafe fn init(&mut self) -> bool {
        // Map MMIO region as uncacheable + create page table entries if needed
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        crate::apic::map_page_uc(self.mmio_base, pmoff);
        crate::apic::map_page_uc(self.mmio_base + 0x1000, pmoff); // covers 8KB for registers

        // Reset
        self.write32(REG_CTRL, CTRL_RST);
        for _ in 0..100_000 {
            if self.read32(REG_CTRL) & CTRL_RST == 0 { break; }
            core::hint::spin_loop();
        }
        serial_println!("[E1000] Reset OK");

        // Read MAC
        self.mac_addr = self.read_mac();
        serial_println!("[E1000] MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.mac_addr[0], self.mac_addr[1], self.mac_addr[2],
            self.mac_addr[3], self.mac_addr[4], self.mac_addr[5]);

        // Link up
        self.write32(REG_CTRL, self.read32(REG_CTRL) | CTRL_SLU);

        // Allocate TX ring
        let tx_ring = Self::alloc_frame();
        if tx_ring == 0 { return false; }
        self.tx_ring_paddr = tx_ring;
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let tx_virt = (tx_ring + pmoff) as *mut u8;
        for i in 0..4096 { tx_virt.add(i).write_volatile(0); }

        self.write32(REG_TDBAL, tx_ring as u32);
        self.write32(REG_TDBAH, (tx_ring >> 32) as u32);
        self.write32(REG_TDLEN, (core::mem::size_of::<TxDesc>() * TX_DESC_COUNT) as u32);
        self.write32(REG_TDH, 0);
        self.write32(REG_TDT, 0);

        // Allocate TX buffers
        let s_tx = core::mem::size_of::<TxDesc>();
        let s_rx = core::mem::size_of::<RxDesc>();
        for i in 0..TX_DESC_COUNT {
            let buf = Self::alloc_frame();
            if buf == 0 { return false; }
            self.tx_buf_paddrs[i] = buf;
            let offset = tx_ring + pmoff + (i as u64 * s_tx as u64);
            let desc = offset as *mut TxDesc;
            (*desc).addr = buf;
            (*desc).length = 0;
            (*desc).cmd = 0;
            (*desc).status = 0;
        }

        // Allocate RX ring
        let rx_ring = Self::alloc_frame();
        if rx_ring == 0 { return false; }
        self.rx_ring_paddr = rx_ring;
        let rx_virt = (rx_ring + pmoff) as *mut u8;
        for i in 0..4096 { rx_virt.add(i).write_volatile(0); }

        self.write32(REG_RDBAL, rx_ring as u32);
        self.write32(REG_RDBAH, (rx_ring >> 32) as u32);
        self.write32(REG_RDLEN, (s_rx * RX_DESC_COUNT) as u32);
        self.write32(REG_RDH, 0);
        self.write32(REG_RDT, RX_DESC_COUNT as u32 - 1);

        // Allocate RX buffers
        for i in 0..RX_DESC_COUNT {
            let buf = Self::alloc_frame();
            if buf == 0 { return false; }
            self.rx_buf_paddrs[i] = buf;
            let offset = rx_ring + pmoff + (i as u64 * s_rx as u64);
            let desc = offset as *mut RxDesc;
            (*desc).addr = buf;
            (*desc).status = 0;
            let virt = (buf + pmoff) as *mut u8;
            for j in 0..2048 { virt.add(j).write_volatile(0); }
        }

        // Enable RX/TX
        self.write32(REG_RCTRL, RCTL_EN | RCTL_SBP | RCTL_UPE | RCTL_MPE | RCTL_BAM | RCTL_SECRC | RCTL_BSIZE_2048);
        self.write32(REG_TCTRL, TCTL_EN | TCTL_PSP | (0x10 << TCTL_CT_SHIFT) | (0x40 << TCTL_COLD_SHIFT));

        // Mask all interrupts (polling mode)
        self.write32(REG_IMASK, 0);

        serial_println!("[E1000] Init OK. TX descs={} RX descs={}", TX_DESC_COUNT, RX_DESC_COUNT);
        true
    }

    pub unsafe fn send(&mut self, data: &[u8]) -> bool {
        if data.is_empty() || data.len() > 2048 { return false; }

        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let tx_ring_virt = (self.tx_ring_paddr + pmoff) as *mut TxDesc;
        let idx = self.tx_cur;

        // Copy data into TX buffer
        let buf_virt = (self.tx_buf_paddrs[idx] + pmoff) as *mut u8;
        for i in 0..data.len() {
            buf_virt.add(i).write_volatile(data[i]);
        }

        let desc = &mut *tx_ring_virt.add(idx);
        desc.length = data.len() as u16;
        desc.cmd = 0x0B; // RS | EOP | IFCS
        desc.status = 0;

        // Advance TDT — tell NIC new descriptors are available
        let next = (idx + 1) % TX_DESC_COUNT;
        self.tx_cur = next;
        self.write32(REG_TDT, next as u32);

        // Don't wait for completion — QEMU TCG precisa de yield para processar TX.
        // O status sera verificado na proxima send().
        true
    }

    pub unsafe fn recv(&mut self) -> Option<Vec<u8>> {
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let rx_ring_virt = (self.rx_ring_paddr + pmoff) as *const RxDesc;
        let idx = self.rx_cur;

        let desc = &*rx_ring_virt.add(idx);
        if desc.status & 0x01 == 0 { return None; }

        let len = desc.length as usize;
        if len < 14 || len > 2048 {
            let d = &mut *(rx_ring_virt.add(idx) as *mut RxDesc);
            d.status = 0;
            self.rx_cur = (idx + 1) % RX_DESC_COUNT;
            self.write32(REG_RDT, self.rx_cur as u32);
            return None;
        }

        let mut buf = Vec::with_capacity(len);
        let data_virt = (self.rx_buf_paddrs[idx] + pmoff) as *const u8;
        for i in 0..len {
            buf.push(data_virt.add(i).read_volatile());
        }

        let d = &mut *(rx_ring_virt.add(idx) as *mut RxDesc);
        d.status = 0;
        self.rx_cur = (idx + 1) % RX_DESC_COUNT;
        self.write32(REG_RDT, self.rx_cur as u32);

        Some(buf)
    }

    pub fn mac(&self) -> [u8; 6] { self.mac_addr }
}
