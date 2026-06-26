use alloc::vec::Vec;
use x86_64::instructions::port::Port;

use crate::memory::{GLOBAL_ALLOCATOR, PHYS_MEM_OFFSET};
use crate::pci::PciDevice;
use crate::serial_println;

pub const RTL8139_VENDOR: u16 = 0x10EC;
pub const RTL8139_DEVICE: u16 = 0x8139;

const REG_MAC: u16 = 0x00;
const REG_TSD0: u16 = 0x10;
const REG_TSAD0: u16 = 0x20;
const REG_RBSTART: u16 = 0x30;
const REG_CR: u16 = 0x37;
const REG_CAPR: u16 = 0x38;
const REG_CBR: u16 = 0x3A;
const REG_RCR: u16 = 0x44;
const REG_IMR: u16 = 0x3C;

const CR_RST: u8 = 0x10;
const CR_RXE: u8 = 0x01;
const CR_TXE: u8 = 0x04;

const TSD_TOK: u32 = 0x0000_8000;
const TSD_TABT: u32 = 0x0000_2000;
const TSD_TUN: u32 = 0x0000_4000;
const TSD_SIZE_SHIFT: u32 = 16;

const TX_BUF_SIZE: usize = 4096;
const RX_BUF_SIZE: usize = 32768 + 16;

pub struct Rtl8139Driver {
    io_base: u16,
    mac_addr: [u8; 6],
    tx_cur: usize,
    tx_buf_paddrs: [u64; 4],
    rx_buf_paddr: u64,
    rx_offset: u16,
}

impl Rtl8139Driver {
    pub unsafe fn new(dev: &PciDevice) -> Option<Self> {
        if dev.vendor_id != RTL8139_VENDOR || dev.device_id != RTL8139_DEVICE {
            return None;
        }
        let io_base = (dev.bar0 & !0x3) as u16;
        let mut mac = [0u8; 6];
        for i in 0..6 {
            mac[i] = Port::new(io_base + REG_MAC + i as u16).read();
        }
        Some(Rtl8139Driver {
            io_base,
            mac_addr: mac,
            tx_cur: 0,
            tx_buf_paddrs: [0; 4],
            rx_buf_paddr: 0,
            rx_offset: 0,
        })
    }

    unsafe fn read8(&self, reg: u16) -> u8 {
        Port::new(self.io_base + reg).read()
    }
    unsafe fn read16(&self, reg: u16) -> u16 {
        Port::new(self.io_base + reg).read()
    }
    unsafe fn read32(&self, reg: u16) -> u32 {
        Port::new(self.io_base + reg).read()
    }
    unsafe fn write8(&self, reg: u16, val: u8) {
        Port::new(self.io_base + reg).write(val)
    }
    unsafe fn write16(&self, reg: u16, val: u16) {
        Port::new(self.io_base + reg).write(val)
    }
    unsafe fn write32(&self, reg: u16, val: u32) {
        Port::new(self.io_base + reg).write(val)
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

        self.write8(REG_CR, CR_RST);
        for _ in 0..100_000 {
            if self.read8(REG_CR) & CR_RST == 0 {
                break;
            }
            core::hint::spin_loop();
        }
        serial_println!(
            "[RTL8139] Reset OK. MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.mac_addr[0], self.mac_addr[1], self.mac_addr[2],
            self.mac_addr[3], self.mac_addr[4], self.mac_addr[5]
        );

        self.write32(REG_RCR, 0x0F);

        let rx_paddr = Self::alloc_page();
        if rx_paddr == 0 {
            serial_println!("[RTL8139] RX buffer alloc failed");
            return false;
        }
        self.rx_buf_paddr = rx_paddr;
        self.write32(REG_RBSTART, rx_paddr as u32);

        let rx_virt = (rx_paddr + pmoff) as *mut u8;
        for i in 0..RX_BUF_SIZE {
            rx_virt.add(i).write_volatile(0);
        }

        for i in 0..4 {
            let tx_paddr = Self::alloc_page();
            if tx_paddr == 0 {
                serial_println!("[RTL8139] TX buffer alloc failed at {}", i);
                return false;
            }
            self.tx_buf_paddrs[i] = tx_paddr;
            let tsad_reg = REG_TSAD0 + i as u16 * 4;
            self.write32(tsad_reg, tx_paddr as u32);
        }

        self.write16(REG_IMR, 0x0000);
        self.write8(REG_CR, CR_RXE | CR_TXE);

        serial_println!(
            "[RTL8139] Init OK. rx_buf=0x{:x} tx_bufs=[0x{:x},...]",
            self.rx_buf_paddr, self.tx_buf_paddrs[0]
        );
        true
    }

    pub unsafe fn send(&mut self, data: &[u8]) -> bool {
        if data.is_empty() || data.len() > TX_BUF_SIZE {
            return false;
        }
        let idx = self.tx_cur;
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let tsd_reg = REG_TSD0 + idx as u16 * 4;

        for _ in 0..100_000 {
            let tsd = self.read32(tsd_reg);
            if tsd & (TSD_TOK | TSD_TABT | TSD_TUN) != 0 {
                break;
            }
            if tsd == 0 {
                break;
            }
            core::hint::spin_loop();
        }

        let buf_virt = (self.tx_buf_paddrs[idx] + pmoff) as *mut u8;
        for i in 0..data.len() {
            buf_virt.add(i).write_volatile(data[i]);
        }

        self.write32(tsd_reg, (data.len() as u32) << TSD_SIZE_SHIFT);

        let tx0 = idx;
        if tx0 < 4 {
            let tsd = self.read32(tsd_reg);
            serial_println!("[RTL8139] TX{} len={} tsd=0x{:x}", tx0, data.len(), tsd);
        }

        for _ in 0..100_000 {
            let tsd = self.read32(tsd_reg);
            if tsd & (TSD_TOK | TSD_TABT | TSD_TUN) != 0 {
                self.tx_cur = (idx + 1) % 4;
                return tsd & TSD_TOK != 0;
            }
            core::hint::spin_loop();
        }
        serial_println!("[RTL8139] TX{} timeout tsd=0x{:x}", idx, self.read32(tsd_reg));
        false
    }

    pub unsafe fn recv(&mut self) -> Option<Vec<u8>> {
        let capr = self.read16(REG_CAPR) % (RX_BUF_SIZE - 16) as u16;
        if capr == self.rx_offset {
            return None;
        }

        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let rx_virt = (self.rx_buf_paddr + pmoff) as *const u8;

        let off = self.rx_offset as usize;
        let status = u16::from_le_bytes([
            rx_virt.add(off).read_volatile(),
            rx_virt.add(off + 1).read_volatile(),
        ]);
        let pkt_len = u16::from_le_bytes([
            rx_virt.add(off + 2).read_volatile(),
            rx_virt.add(off + 3).read_volatile(),
        ]);

        if status & 0x0001 == 0 || pkt_len < 64 || pkt_len > 1536 {
            self.rx_offset = capr;
            self.write16(REG_CBR, capr.wrapping_sub(16));
            return None;
        }

        let data_len = (pkt_len as usize).saturating_sub(4);
        let mut buf = Vec::with_capacity(data_len);
        let data_start = off + 4;
        for i in 0..data_len {
            buf.push(rx_virt.add(data_start + i).read_volatile());
        }

        let consumed = ((4 + data_len + 4 + 3) / 4) * 4;
        self.rx_offset = ((off + consumed) % (RX_BUF_SIZE - 16)) as u16;

        let cbr = if self.rx_offset < 16 {
            (RX_BUF_SIZE - 16) as u16 + self.rx_offset - 16
        } else {
            self.rx_offset - 16
        };
        self.write16(REG_CBR, cbr);

        Some(buf)
    }

    pub fn mac(&self) -> [u8; 6] {
        self.mac_addr
    }

    pub fn debug_regs(&self) {
        unsafe {
            let cr = self.read8(REG_CR);
            let capr = self.read16(REG_CAPR);
            serial_println!(
                "[RTL8139] CR=0x{:02x} CAPR=0x{:04x} rx_off=0x{:04x} tx_cur={}",
                cr, capr, self.rx_offset, self.tx_cur
            );
        }
    }
}
