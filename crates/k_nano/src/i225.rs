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

// Descriptor sizes (legacy 16-byte)
const TX_DESC_SIZE: usize = 16;
const RX_DESC_SIZE: usize = 16;

// TX descriptor field offsets
const TX_OFF_ADDR: usize = 0;
const TX_OFF_LENGTH: usize = 8;
const TX_OFF_CSO: usize = 10;
const TX_OFF_CMD: usize = 11;
const TX_OFF_STATUS: usize = 12;
const TX_OFF_CSS: usize = 13;
const TX_OFF_VLAN: usize = 14;

// RX descriptor field offsets
const RX_OFF_ADDR: usize = 0;
const RX_OFF_LENGTH: usize = 8;
const RX_OFF_CSUM: usize = 10;
const RX_OFF_STATUS: usize = 12;
const RX_OFF_ERRORS: usize = 13;
const RX_OFF_SPECIAL: usize = 14;

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

    /// Get TX descriptor base pointer
    unsafe fn tx_desc_base(&self) -> *mut u8 {
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        (self.tx_ring_paddr + pmoff) as *mut u8
    }

    /// Get RX descriptor base pointer
    unsafe fn rx_desc_base(&self) -> *mut u8 {
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        (self.rx_ring_paddr + pmoff) as *mut u8
    }

    /// Write TX descriptor using raw byte pointers (avoid packed struct UB)
    unsafe fn write_tx_desc(&self, idx: usize, buf_paddr: u64, len: u16, cmd: u8) {
        let base = self.tx_desc_base();
        let d = base.add(idx * TX_DESC_SIZE);
        core::ptr::write_volatile(d.add(TX_OFF_ADDR) as *mut u64, buf_paddr);
        core::ptr::write_volatile(d.add(TX_OFF_LENGTH) as *mut u16, len);
        core::ptr::write_volatile(d.add(TX_OFF_CSO), 0);
        core::ptr::write_volatile(d.add(TX_OFF_CMD), cmd);
        core::ptr::write_volatile(d.add(TX_OFF_STATUS), 0);
        core::ptr::write_volatile(d.add(TX_OFF_CSS), 0);
        core::ptr::write_volatile(d.add(TX_OFF_VLAN) as *mut u16, 0);
    }

    /// Read RX descriptor DD bit using raw pointers + clflush
    unsafe fn read_rx_dd(&self, idx: usize) -> u8 {
        let base = self.rx_desc_base();
        let d = base.add(idx * RX_DESC_SIZE);
        // clflush before reading DD (belt-and-suspenders vs UC map)
        core::arch::x86_64::_mm_clflush(d.add(RX_OFF_STATUS) as *const _);
        core::arch::asm!("lfence", options(nostack, preserves_flags));
        core::ptr::read_volatile(d.add(RX_OFF_STATUS))
    }

    /// Read RX descriptor length using raw pointers
    unsafe fn read_rx_len(&self, idx: usize) -> u16 {
        let base = self.rx_desc_base();
        let d = base.add(idx * RX_DESC_SIZE);
        core::ptr::read_volatile(d.add(RX_OFF_LENGTH) as *const u16)
    }

    /// Clear RX descriptor DD bit using raw pointers
    unsafe fn clear_rx_dd(&self, idx: usize) {
        let base = self.rx_desc_base();
        let d = base.add(idx * RX_DESC_SIZE);
        core::ptr::write_volatile(d.add(RX_OFF_STATUS), 0);
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
        // Re-check Bus Master after reset (some HW clears it)
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

        // I225 defaults to advanced 32-byte descriptors (RXDCTL.DEXT=1).
        // Driver uses legacy 16-byte descriptors — clear DEXT for Q0.
        self.write32(0xC028, 0x00000000);  // RXDCTL(0): DEXT=0, legacy RX descriptors
        self.write32(0xE028, 0x00000000);  // TXDCTL(0): DEXT=0, legacy TX descriptors

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
            // Initialize TX descriptor as free (DD=1)
            self.write_tx_desc(i, b, 0, 0);
            // Set DD=1 to mark free
            let base = self.tx_desc_base();
            core::ptr::write_volatile(base.add(i * TX_DESC_SIZE + TX_OFF_STATUS), 1);
        }
        for i in 0..RX_N {
            let b = Self::alloc_frame();
            if b == 0 {
                return false;
            }
            crate::apic::map_page_uc(b, pmoff);
            self.rx_buf_paddrs[i] = b;
            // Initialize RX descriptor
            let base = self.rx_desc_base();
            core::ptr::write_volatile((base.add(i * RX_DESC_SIZE + RX_OFF_ADDR)) as *mut u64, b);
            core::ptr::write_volatile(base.add(i * RX_DESC_SIZE + RX_OFF_STATUS), 0);
        }

        self.write32(REG_TDBAL, tx_ring as u32);
        self.write32(REG_TDBAH, (tx_ring >> 32) as u32);
        self.write32(REG_TDLEN, (TX_DESC_SIZE * TX_N) as u32);
        self.write32(REG_TDH, 0);
        self.write32(REG_TDT, 0);

        self.write32(REG_RDBAL, rx_ring as u32);
        self.write32(REG_RDBAH, (rx_ring >> 32) as u32);
        self.write32(REG_RDLEN, (RX_DESC_SIZE * RX_N) as u32);
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
        let idx = self.tx_cur;
        // Check DD bit using raw pointer
        let base = self.tx_desc_base();
        let d = base.add(idx * TX_DESC_SIZE);
        if core::ptr::read_volatile(d.add(TX_OFF_STATUS)) & 1 == 0 {
            return false; // still owned by HW
        }
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let buf = (self.tx_buf_paddrs[idx] + pmoff) as *mut u8;
        core::ptr::copy_nonoverlapping(data.as_ptr(), buf, data.len());
        self.write_tx_desc(idx, self.tx_buf_paddrs[idx], data.len() as u16, 0x0B); // EOP|IFCS|RS
        core::arch::asm!("sfence", options(nostack, preserves_flags));
        let next = (idx + 1) % TX_N;
        self.write32(REG_TDT, next as u32);
        self.tx_cur = next;
        true
    }

    pub unsafe fn recv(&mut self) -> Option<Vec<u8>> {
        let idx = self.rx_cur;
        let dd = self.read_rx_dd(idx);
        if dd & 1 == 0 {
            return None;
        }
        let len = self.read_rx_len(idx) as usize;
        if len < 14 || len > 2048 {
            self.clear_rx_dd(idx);
            self.write32(REG_RDT, idx as u32);
            self.rx_cur = (idx + 1) % RX_N;
            return None;
        }
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let mut out = Vec::with_capacity(len);
        let src = (self.rx_buf_paddrs[idx] + pmoff) as *const u8;
        for i in 0..len {
            out.push(src.add(i).read_volatile());
        }
        self.clear_rx_dd(idx);
        core::arch::asm!("sfence", options(nostack, preserves_flags));
        self.write32(REG_RDT, idx as u32);
        self.rx_cur = (idx + 1) % RX_N;
        Some(out)
    }

    /// Kick RX on link transition (disable→clear→re-enable→RDT=N-1)
    pub unsafe fn kick_rx(&mut self) {
        // Disable RX
        let rctl = self.read32(REG_RCTL);
        self.write32(REG_RCTL, rctl & !RCTL_EN);
        // Clear all RX descriptors
        for i in 0..RX_N {
            self.clear_rx_dd(i);
        }
        // Re-enable RX
        self.write32(REG_RCTL, rctl | RCTL_EN);
        // Make all descriptors available
        self.write32(REG_RDT, (RX_N as u32).wrapping_sub(1));
        crate::slog_nano!("Net", "i225", "kick_rx done");
    }

    /// Prove RX works: send ARP who-has, wait TX DD, poll RX DD
    pub unsafe fn prove_rx(&mut self) -> bool {
        // Build minimal ARP who-has for 192.168.1.1 (gateway)
        let mut arp = [0u8; 60];
        // Ethernet header
        arp[0..6].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]); // dst MAC broadcast
        arp[6..12].copy_from_slice(&self.mac_addr); // src MAC
        arp[12..14].copy_from_slice(&[0x08, 0x06]); // EtherType ARP
        // ARP payload
        arp[14..16].copy_from_slice(&[0x00, 0x01]); // HTYPE Ethernet
        arp[16..18].copy_from_slice(&[0x08, 0x00]); // PTYPE IPv4
        arp[18] = 6; // HLEN
        arp[19] = 4; // PLEN
        arp[20..22].copy_from_slice(&[0x00, 0x01]); // OPER request
        arp[22..28].copy_from_slice(&self.mac_addr); // SHA
        arp[28..32].copy_from_slice(&[192, 168, 1, 100]); // SPA (our IP)
        arp[32..38].copy_from_slice(&[0, 0, 0, 0, 0, 0]); // THA
        arp[38..42].copy_from_slice(&[192, 168, 1, 1]); // TPA (gateway)
        // Pad to 60 bytes (min Ethernet frame)

        // Send ARP
        if !self.send(&arp) {
            crate::slog_nano!("Net", "i225", "prove_rx: send FAIL");
            return false;
        }
        // Wait TX DD
        for _ in 0..100_000 {
            let base = self.tx_desc_base();
            let d = base.add(self.tx_cur * TX_DESC_SIZE);
            if core::ptr::read_volatile(d.add(TX_OFF_STATUS)) & 1 != 0 {
                break;
            }
            core::hint::spin_loop();
        }
        // Poll RX DD with wall-clock delay
        for _ in 0..50_000 {
            if self.recv().is_some() {
                crate::slog_nano!("Net", "i225", "prove_rx: RX PASS");
                return true;
            }
            core::hint::spin_loop();
        }
        crate::slog_nano!("Net", "i225", "prove_rx: RX TIMEOUT");
        false
    }

    /// Check if any RX descriptor has DD bit set
    pub unsafe fn any_rx_dd(&self) -> bool {
        for i in 0..RX_N {
            if self.read_rx_dd(i) & 1 != 0 {
                return true;
            }
        }
        false
    }

    /// Count RX descriptors with DD bit set
    pub unsafe fn count_rx_dd(&self) -> usize {
        let mut count = 0;
        for i in 0..RX_N {
            if self.read_rx_dd(i) & 1 != 0 {
                count += 1;
            }
        }
        count
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