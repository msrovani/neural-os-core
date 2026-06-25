use crate::memory::{GLOBAL_ALLOCATOR, PHYS_MEM_OFFSET};
use crate::pci::PciDevice;
use crate::serial_println;
use core::ptr::{read_volatile, write_volatile};
use alloc::vec::Vec;

const E1000_VENDOR: u16 = 0x8086;
pub const E1000_DEVICE: u16 = 0x100E;

const REG_CTRL: u64 = 0x0000;
const REG_STATUS: u64 = 0x0008;
const REG_EERD: u64 = 0x0014;
const REG_ICR: u64 = 0x00C0;
const REG_IMS: u64 = 0x00D0;
const REG_RDBAL: u64 = 0x2800;
const REG_RDBAH: u64 = 0x2804;
const REG_RDLEN: u64 = 0x2808;
const REG_RDH: u64 = 0x2810;
const REG_RDT: u64 = 0x2818;
const REG_RXDCTL: u64 = 0x3828;
const REG_TDBAL: u64 = 0x3800;
const REG_TDBAH: u64 = 0x3804;
const REG_TDLEN: u64 = 0x3808;
const REG_TDH: u64 = 0x3810;
const REG_TDT: u64 = 0x3818;
const REG_TXDCTL: u64 = 0xE000;
const REG_RAL: u64 = 0x5400;
const REG_RAH: u64 = 0x5404;
const REG_RCTL: u64 = 0x0100;
const REG_TCTL: u64 = 0x0400;

const CTRL_RST: u32 = 0x04000000;
const CTRL_SLU: u32 = 0x40;
const RCTL_EN: u32 = 0x00000002;
const RCTL_SBP: u32 = 0x00000004;
const RCTL_UPE: u32 = 0x00000008;
const RCTL_MPE: u32 = 0x00000010;
const RCTL_LPE: u32 = 0x00000020;
const RCTL_BSIZE: u32 = 0x00000200;
const RCTL_BSEX: u32 = 0x00000400;
const RCTL_SECRC: u32 = 0x04000000;
const TCTL_EN: u32 = 0x00000002;
const TCTL_PSP: u32 = 0x00000008;
const TCTL_CT: u32 = 0x00000F00;
const TCTL_COLD: u32 = 0x003FF000;
const STATUS_LU: u32 = 0x02;
const EERD_START: u32 = 0x01;
const EERD_DONE: u32 = 0x10;
const RXDCTL_GRAN: u32 = 0x01000000;
const TXDCTL_GRAN: u32 = 0x01000000;

const NUM_DESC: usize = 48;

#[repr(C)]
struct RxDesc {
    buffer_addr: u64,
    length: u16,
    checksum: u16,
    status: u8,
    errors: u8,
    special: u16,
}

#[repr(C)]
struct TxDesc {
    buffer_addr: u64,
    length: u16,
    cso: u8,
    cmd: u8,
    status: u8,
    css: u8,
    special: u16,
}

const RX_DESC_DONE: u8 = 0x01;
const TX_DESC_DONE: u8 = 0x01;
const TX_CMD_EOP: u8 = 0x01;
const TX_CMD_RS: u8 = 0x08;
const TX_CMD_IFCS: u8 = 0x02;

pub struct E1000Driver {
    mmio_base: u64,
    mac_addr: [u8; 6],
    rx_cur: usize,
    tx_cur: usize,
    rx_desc_paddr: u64,
    tx_desc_paddr: u64,
    rx_buf_paddrs: [u64; NUM_DESC],
    tx_buf_paddrs: [u64; NUM_DESC],
}

impl E1000Driver {
    pub unsafe fn new(dev: &PciDevice) -> Option<Self> {
        if dev.vendor_id != E1000_VENDOR || dev.device_id != E1000_DEVICE {
            return None;
        }
        let bar0 = dev.bar0;
        let mmio_phys = (bar0 & !0xF) as u64;
        let phys_mem_offset = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        serial_println!("[E1000] MMIO: phys=0x{:x} virt=0x{:x}", mmio_phys, mmio_phys + phys_mem_offset);

        Some(E1000Driver {
            mmio_base: mmio_phys + phys_mem_offset,
            mac_addr: [0; 6],
            rx_cur: 0, tx_cur: 0,
            rx_desc_paddr: 0,
            tx_desc_paddr: 0,
            rx_buf_paddrs: [0; NUM_DESC],
            tx_buf_paddrs: [0; NUM_DESC],
        })
    }

    unsafe fn r32(&self, reg: u64) -> u32 {
        read_volatile((self.mmio_base + reg) as *const u32)
    }
    unsafe fn w32(&self, reg: u64, val: u32) {
        write_volatile((self.mmio_base + reg) as *mut u32, val);
    }

    unsafe fn eeprom_read(&self, addr: u8) -> u16 {
        self.w32(REG_EERD, (addr as u32) << 8 | EERD_START);
        for _ in 0..1000 {
            if self.r32(REG_EERD) & EERD_DONE != 0 {
                return (self.r32(REG_EERD) >> 16) as u16;
            }
            core::hint::spin_loop();
        }
        0
    }

    fn alloc_page() -> u64 {
        let mut guard = GLOBAL_ALLOCATOR.lock();
        let allocator = guard.as_mut().unwrap();
        let frame = allocator.allocate_contiguous(1);
        match frame {
            Some(f) => f.start_address().as_u64(),
            None => 0,
        }
    }

    pub unsafe fn init(&mut self) -> bool {
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);

        // Read MAC from EEPROM
        let mac_low = self.eeprom_read(0) as u32;
        let mac_high = self.eeprom_read(1) as u32;
        self.mac_addr = [
            (mac_low & 0xFF) as u8,
            ((mac_low >> 8) & 0xFF) as u8,
            ((mac_low >> 16) & 0xFF) as u8,
            ((mac_low >> 24) & 0xFF) as u8,
            (mac_high & 0xFF) as u8,
            ((mac_high >> 8) & 0xFF) as u8,
        ];
        serial_println!("[E1000] MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.mac_addr[0], self.mac_addr[1], self.mac_addr[2],
            self.mac_addr[3], self.mac_addr[4], self.mac_addr[5]);

        // Bring link up
        let ctrl = self.r32(REG_CTRL);
        self.w32(REG_CTRL, ctrl | CTRL_SLU);
        let link_up = self.r32(REG_STATUS) & STATUS_LU != 0;
        serial_println!("[E1000] Link: {}", if link_up { "UP" } else { "DOWN (forcing)" });

        self.rx_desc_paddr = Self::alloc_page();
        if self.rx_desc_paddr == 0 { return false; }
        let rx_desc_vaddr = self.rx_desc_paddr + pmoff;
        let rx_desc = core::slice::from_raw_parts_mut(rx_desc_vaddr as *mut RxDesc, NUM_DESC);
        for i in 0..NUM_DESC { rx_desc[i] = core::mem::zeroed(); }

        self.tx_desc_paddr = Self::alloc_page();
        if self.tx_desc_paddr == 0 { return false; }
        let tx_desc_vaddr = self.tx_desc_paddr + pmoff;
        let tx_desc = core::slice::from_raw_parts_mut(tx_desc_vaddr as *mut TxDesc, NUM_DESC);
        for i in 0..NUM_DESC {
            tx_desc[i] = core::mem::zeroed();
            tx_desc[i].status = TX_DESC_DONE;
        }

        for i in 0..NUM_DESC {
            let buf = Self::alloc_page();
            if buf == 0 { return false; }
            rx_desc[i].buffer_addr = buf;
            rx_desc[i].status = 0;
            self.rx_buf_paddrs[i] = buf;
        }

        for i in 0..NUM_DESC {
            let buf = Self::alloc_page();
            if buf == 0 { return false; }
            tx_desc[i].buffer_addr = buf;
            self.tx_buf_paddrs[i] = buf;
        }

        // Set MAC address filter (address valid flag bit 31)
        let mac_low32 = u32::from_le_bytes([self.mac_addr[0], self.mac_addr[1], self.mac_addr[2], self.mac_addr[3]]);
        let mac_high16 = (self.mac_addr[4] as u32) | ((self.mac_addr[5] as u32) << 8) | (1u32 << 31);
        self.w32(REG_RAL, mac_low32);
        self.w32(REG_RAH, mac_high16);

        // Program RX ring base (antes de habilitar)
        self.w32(REG_RDBAL, self.rx_desc_paddr as u32);
        self.w32(REG_RDBAH, (self.rx_desc_paddr >> 32) as u32);
        self.w32(REG_RDLEN, (NUM_DESC * 16) as u32);
        self.w32(REG_RDH, 0);
        self.w32(REG_RXDCTL, (8u32 << 16)); // PTHRESH=8 (Linux driver min)

        // Program TX ring
        self.w32(REG_TDBAL, self.tx_desc_paddr as u32);
        self.w32(REG_TDBAH, (self.tx_desc_paddr >> 32) as u32);
        self.w32(REG_TDLEN, (NUM_DESC * 16) as u32);
        self.w32(REG_TDH, 0);
        self.w32(REG_TDT, 0);
        self.w32(REG_TXDCTL, 0);

        // Enable RX primeiro, DEPOIS setar RDT
        self.w32(REG_RCTL, RCTL_EN | RCTL_SBP | RCTL_UPE | RCTL_MPE | RCTL_LPE | RCTL_BSIZE | RCTL_SECRC);
        for _ in 0..1000 { core::hint::spin_loop(); }
        self.w32(REG_RDT, (NUM_DESC - 1) as u32);

        // Enable TX
        self.w32(REG_TCTL, TCTL_EN | TCTL_PSP | TCTL_CT | TCTL_COLD);

        // Enable interrupts
        self.w32(REG_IMS, 0x01 | 0x02 | 0x04);

        serial_println!("[E1000] Init OK. rx_desc=0x{:x} tx_desc=0x{:x}", self.rx_desc_paddr, self.tx_desc_paddr);
        true
    }

    pub unsafe fn send(&mut self, data: &[u8]) -> bool {
        if data.len() > 4096 { return false; }
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);

        let idx = self.tx_cur % NUM_DESC;
        let tx_vaddr = self.tx_desc_paddr + pmoff;
        let tx = &mut *(tx_vaddr as *mut [TxDesc; NUM_DESC]);

        if tx[idx].status & TX_DESC_DONE == 0 && tx[idx].buffer_addr != 0 {
            return false;
        }

        let buf_paddr = self.tx_buf_paddrs[idx];
        let buf_vaddr = buf_paddr + pmoff;
        core::ptr::copy_nonoverlapping(data.as_ptr(), buf_vaddr as *mut u8, data.len());

        tx[idx].length = data.len() as u16;
        tx[idx].cmd = TX_CMD_EOP | TX_CMD_IFCS | TX_CMD_RS;
        tx[idx].status = TX_DESC_DONE; // DD=1: software pronto, hardware pode enviar

        self.w32(REG_TDT, ((idx + 1) % NUM_DESC) as u32);
        // Le descritor de volta apos hardware processar
        let st_after = tx[idx].status;
        let ln_after = tx[idx].length;
        if self.tx_cur < 3 {
            serial_println!("[E1000] TX{} apos: status=0x{:x} len={}", idx, st_after, ln_after);
        }
        self.tx_cur += 1;
        true
    }

    pub unsafe fn recv(&mut self) -> Option<Vec<u8>> {
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);

        let idx = self.rx_cur % NUM_DESC;
        let rx_vaddr = self.rx_desc_paddr + pmoff;
        let rx = &*(rx_vaddr as *const [RxDesc; NUM_DESC]);

        if rx[idx].status & RX_DESC_DONE == 0 {
            return None;
        }

        let len = rx[idx].length as usize;
        let mut data = alloc::vec![0u8; len];
        let buf_paddr = self.rx_buf_paddrs[idx];
        let buf_vaddr = buf_paddr + pmoff;
        core::ptr::copy_nonoverlapping(buf_vaddr as *const u8, data.as_mut_ptr(), len);

        // Replenish
        let rx_mut = &mut *(rx_vaddr as *mut [RxDesc; NUM_DESC]);
        rx_mut[idx].status = 0;
        rx_mut[idx].length = 0;
        rx_mut[idx].checksum = 0;
        rx_mut[idx].errors = 0;

        self.w32(REG_RDT, idx as u32);
        self.rx_cur += 1;
        Some(data)
    }

    pub fn mac(&self) -> [u8; 6] { self.mac_addr }
}

impl E1000Driver {
    pub unsafe fn rx_ready(&self) -> bool {
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let idx = self.rx_cur % NUM_DESC;
        let rx_vaddr = self.rx_desc_paddr + pmoff;
        let rx = &*(rx_vaddr as *const [RxDesc; NUM_DESC]);
        rx[idx].status & RX_DESC_DONE != 0
    }

    pub unsafe fn debug_mmio_read(&self, reg: u64) -> u32 {
        self.r32(reg)
    }

    pub unsafe fn debug_rx_desc(&self, idx: usize) -> (u64, u16, u8) {
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let rx_vaddr = self.rx_desc_paddr + pmoff;
        let rx = &*(rx_vaddr as *const [RxDesc; NUM_DESC]);
        let i = idx % NUM_DESC;
        (rx[i].buffer_addr, rx[i].length, rx[i].status)
    }

    pub unsafe fn debug_tx_desc(&self, idx: usize) -> (u64, u16, u8) {
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let tx_vaddr = self.tx_desc_paddr + pmoff;
        let tx = &*(tx_vaddr as *const [TxDesc; NUM_DESC]);
        let i = idx % NUM_DESC;
        (tx[i].buffer_addr, tx[i].length, tx[i].status)
    }

    unsafe fn tx_ready(&self) -> bool {
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let idx = self.tx_cur % NUM_DESC;
        let tx_vaddr = self.tx_desc_paddr + pmoff;
        let tx = &*(tx_vaddr as *const [TxDesc; NUM_DESC]);
        tx[idx].status & TX_DESC_DONE != 0 || tx[idx].buffer_addr == 0
    }
}
