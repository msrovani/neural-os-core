//! e1000/e1000e Gigabit Ethernet driver.
//! PCI class 0x020000. MMIO-based register access.
//! TX/RX descriptor rings (64 entries each).
//! Baseado na Intel Gigabit Ethernet Controller datasheet.

use alloc::vec::Vec;
 // kept for potential I/O bar fallback
use crate::memory::{GLOBAL_ALLOCATOR, PHYS_MEM_OFFSET};
use crate::pci::PciDevice;
pub const E1000_VENDOR_INTEL: u16 = 0x8086;
pub const E1000_DEVICE_82540EM: u16 = 0x100E;
pub const E1000_DEVICE_82574L: u16 = 0x10D3;
pub const E1000_DEVICE_82579LM: u16 = 0x1502;

// Register offsets (MMIO)
const REG_CTRL: u64 = 0x0000;
pub const REG_STATUS: u64 = 0x0008;
const REG_EEPROM: u64 = 0x0014;
const REG_CTRL_EXT: u64 = 0x0018;
const REG_ICR: u64 = 0x00C0;
const REG_IMASK: u64 = 0x00D0;
const REG_IMC: u64 = 0x00D8;
const REG_RCTRL: u64 = 0x0100;
const REG_TCTRL: u64 = 0x0400;
// TX ring: offsets canônicos Intel 8254x / QEMU e1000 (NÃO os aliases 0x0420..0x0438 —
// QEMU não wireia TDBAL_A/TDT_A; write em alias = no-op → TDT fica 0 e ARP nunca sai).
const REG_TDBAL: u64 = 0x3800;
const REG_TDBAH: u64 = 0x3804;
const REG_TDLEN: u64 = 0x3808;
const REG_TDH: u64 = 0x3810;
const REG_TDT: u64 = 0x3818;
const REG_RDBAL: u64 = 0x2800;
const REG_RDBAH: u64 = 0x2804;
const REG_RDLEN: u64 = 0x2808;
pub const REG_RDH: u64 = 0x2810;
pub const REG_RDT: u64 = 0x2818;
const REG_RAL: u64 = 0x5400;
const REG_RAH: u64 = 0x5404;
const REG_MTA: u64 = 0x5200;
const REG_RXDCTL: u64 = 0x3828;
const REG_TIPG: u64 = 0x0410;
const REG_RDTR: u64 = 0x2820;
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
const RCTL_LBM_MAC: u32 = 0x00000100;
const RCTL_LBM_PHY: u32 = 0x00000200;
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
    pci_bus: u8,
    pci_device: u8,
    pci_func: u8,
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

        crate::slog_nano!("Net", "e1000", "Detectado vendor={:#06x} device={:#06x} MMIO={:#010x} virt={:#010x}", dev.vendor_id, dev.device_id, mmio_base, mmio_virt);

        // Garantir PCI Bus Master + Memory Space habilitados para DMA
        crate::pci::enable_pci_bus_master(dev);

        Some(E1000Driver {
            mmio_base,
            mmio_virt,
            mac_addr: [0; 6],
            pci_bus: dev.bus,
            pci_device: dev.device,
            pci_func: dev.function,
            tx_ring_paddr: 0,
            rx_ring_paddr: 0,
            tx_buf_paddrs: [0; TX_DESC_COUNT],
            rx_buf_paddrs: [0; RX_DESC_COUNT],
            tx_cur: 0,
            rx_cur: 0,
        })
    }

    pub unsafe fn read32(&self, reg: u64) -> u32 {
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
        let alloc = guard.as_mut().expect("GLOBAL_ALLOCATOR not initialized in alloc_frame");
        let frame = alloc.allocate_contiguous(1);
        match frame {
            Some(f) => f.start_address().as_u64(),
            None => 0,
        }
    }

    pub unsafe fn init(&mut self) -> bool {
        // Map MMIO region as uncacheable — cobre TODOS os registros
        // e1000 registers span from 0x0000 to ~0x5400 (RAL/RAH, MTA, etc.)
        // Registers above 0x2000: RCTRL(0x2800), RDBAL(0x2800), RDLEN(0x2808),
        // RDH(0x2810), RDT(0x2818), RXDCTL(0x3828), MTA(0x5200), RAL/RAH(0x5400)
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        // Map 24KB (6 pages) to cover 0x0000-0x5FFF (RAF/RAH at 0x5400)
        for page in 0..6 {
            crate::apic::map_page_uc(self.mmio_base + page * 0x1000, pmoff);
        }

        // Reset
        self.write32(REG_CTRL, CTRL_RST);
        for _ in 0..100_000 {
            if self.read32(REG_CTRL) & CTRL_RST == 0 { break; }
            core::hint::spin_loop();
        }
        crate::slog_nano!("Net", "e1000", "Reset OK");

        // Re-check PCI Bus Master after reset (CTRL_RST pode ter limpado)
        let cmd = crate::pci::read_config_word(self.pci_bus, self.pci_device, self.pci_func, 0x04);
        if cmd & 0x04 == 0 {
            crate::slog_nano!("Net", "e1000", "Bus Master lost after reset! Re-enabling...");
            crate::pci::enable_pci_bus_master_unsafe(self.pci_bus, self.pci_device, self.pci_func);
        } else {
            crate::slog_nano!("Net", "e1000", "Bus Master OK after reset: cmd={:#06x}", cmd);
        }

        // Read MAC
        self.mac_addr = self.read_mac();

        // Força link UP: PHY_RST + SLU + FD (PHY reset + Set Link Up + Full Duplex)
        self.write32(REG_CTRL, 0x80000000);
        for _ in 0..1000 { core::hint::spin_loop(); }
        self.write32(REG_CTRL, CTRL_SLU | CTRL_FD);
        crate::slog_nano!("Net", "e1000", "MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.mac_addr[0], self.mac_addr[1], self.mac_addr[2],
            self.mac_addr[3], self.mac_addr[4], self.mac_addr[5]);

        // Re-write MAC address into RAL/RAH with AV bit (some emulations need this)
        let mac = self.mac_addr;
        let rah_val = (mac[5] as u32) << 8 | (mac[4] as u32) | 0x80000000; // AV=1
        let ral_val = (mac[3] as u32) << 24 | (mac[2] as u32) << 16 |
                      (mac[1] as u32) << 8 | (mac[0] as u32);
        self.write32(REG_RAL, ral_val);
        self.write32(REG_RAH, rah_val);
        crate::slog_nano!("Net", "e1000", "MAC re-written: RAL={:#010x} RAH={:#010x}", ral_val, rah_val);

        // Force link UP: SLU + ASDE + FD (Linux e1000 default-ish for QEMU)
        let ctrl_want = CTRL_SLU | CTRL_ASDE | CTRL_FD;
        self.write32(REG_CTRL, ctrl_want);
        let ctrl_new = self.read32(REG_CTRL);
        crate::slog_nano!("Net", "e1000", "CTRL forced link UP: wrote={:#010x} readback={:#010x}", ctrl_want, ctrl_new);

        // Allocate TX ring + mapear como uncacheable
        let tx_ring = Self::alloc_frame();
        if tx_ring == 0 { return false; }
        self.tx_ring_paddr = tx_ring;
        crate::apic::map_page_uc(tx_ring, pmoff);
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let tx_virt = (tx_ring + pmoff) as *mut u8;
        for i in 0..4096 { tx_virt.add(i).write_volatile(0); }

        self.write32(REG_TDBAL, tx_ring as u32);
        self.write32(REG_TDBAH, (tx_ring >> 32) as u32);
        self.write32(REG_TDLEN, (core::mem::size_of::<TxDesc>() * TX_DESC_COUNT) as u32);
        self.write32(REG_TDH, 0);
        self.write32(REG_TDT, 0);

        // Allocate TX buffers + uncacheable
        let s_tx = core::mem::size_of::<TxDesc>();
        let s_rx = core::mem::size_of::<RxDesc>();
        for i in 0..TX_DESC_COUNT {
            let buf = Self::alloc_frame();
            if buf == 0 { return false; }
            self.tx_buf_paddrs[i] = buf;
            crate::apic::map_page_uc(buf, pmoff);
            let offset = tx_ring + pmoff + (i as u64 * s_tx as u64);
            let desc = offset as *mut TxDesc;
            (*desc).addr = buf;
            (*desc).length = 0;
            (*desc).cmd = 0;
            (*desc).status = 0;
        }

        // Allocate RX ring + uncacheable
        let rx_ring = Self::alloc_frame();
        if rx_ring == 0 { return false; }
        self.rx_ring_paddr = rx_ring;
        crate::apic::map_page_uc(rx_ring, pmoff);
        let rx_virt = (rx_ring + pmoff) as *mut u8;
        for i in 0..4096 { rx_virt.add(i).write_volatile(0); }

        self.write32(REG_RDBAL, rx_ring as u32);
        self.write32(REG_RDBAH, (rx_ring >> 32) as u32);
        self.write32(REG_RDLEN, (s_rx * RX_DESC_COUNT) as u32);
        self.write32(REG_RDH, 0);

        // Allocate RX buffers + uncacheable
        for i in 0..RX_DESC_COUNT {
            let buf = Self::alloc_frame();
            if buf == 0 { return false; }
            self.rx_buf_paddrs[i] = buf;
            crate::apic::map_page_uc(buf, pmoff);
            let offset = rx_ring + pmoff + (i as u64 * s_rx as u64);
            let desc = offset as *mut RxDesc;
            (*desc).addr = buf;
            (*desc).status = 0;
            let virt = (buf + pmoff) as *mut u8;
            for j in 0..2048 { virt.add(j).write_volatile(0); }
        }

        // RDT=0 primeiro para esvaziar o ring (Linux faz isso)
        self.write32(REG_RDT, 0);

        // RXDCTL: enable RX descriptor fetching (bit 25 = QUEUE_ENABLE)
        self.write32(REG_RXDCTL, 0x02000000);

        // Clear MTA (multicast array) — Linux sempre limpa
        for mta_idx in 0..128 {
            self.write32(REG_MTA + (mta_idx as u64 * 4), 0);
        }

        // TIPG required on real HW; QEMU accepts it. RDTR=0 = no interrupt delay (poll mode).
        self.write32(REG_TIPG, 0x0060_200A);
        self.write32(REG_RDTR, 0);

        // Enable RX/TX — Rx habilita PRIMEIRO, depois RDT (ordem do Linux)
        let rctl = RCTL_EN | RCTL_SBP | RCTL_UPE | RCTL_MPE | RCTL_LBM_NONE
            | RCTL_BAM | RCTL_SECRC | RCTL_BSIZE_2048 | RCTL_RDMTS_HALF;
        self.write32(REG_RCTRL, rctl);
        self.write32(REG_TCTRL, TCTL_EN | TCTL_PSP | (0x10 << TCTL_CT_SHIFT) | (0x40 << TCTL_COLD_SHIFT));

        // RDT só DEPOIS de habilitar o receiver — NIC precisa ver RDT válido após enable
        self.write32(REG_RDT, RX_DESC_COUNT as u32 - 1);

        // Mask all interrupts via IMC + IMASK (poll mode)
        self.write32(REG_IMC, 0xFFFF_FFFF);
        self.write32(REG_IMASK, 0);
        let _ = self.read32(REG_ICR); // clear pending
        Self::fence_write();

        self.rx_cur = 0;
        self.tx_cur = 0;
        crate::slog_nano!("Net", "e1000", "Init OK. TX descs={} RX descs={} RCTL={:#010x}", TX_DESC_COUNT, RX_DESC_COUNT, rctl);

        true
    }

    /// Mini-driver: ARP Request → SLIRP → ARP Reply (força RX real)
    unsafe fn fence_write() {
        core::arch::asm!("sfence", options(nostack, preserves_flags));
    }

    unsafe fn fence_read() {
        core::arch::asm!("lfence", options(nostack, preserves_flags));
    }

    /// Escreve TX descriptor via raw ptr (sem packed struct ref).
    unsafe fn write_tx_desc(ring_virt: *mut u8, idx: usize, len: u16, cmd_byte: u8) {
        let desc = ring_virt.add(idx * core::mem::size_of::<TxDesc>());
        // addr já foi setado no init; escrever length, cso, cmd, status, css, vlan
        (desc.add(8) as *mut u16).write_volatile(len);   // length
        (desc.add(10) as *mut u8).write_volatile(0);      // cso
        (desc.add(11) as *mut u8).write_volatile(cmd_byte); // cmd
        (desc.add(12) as *mut u8).write_volatile(0);      // status
        (desc.add(13) as *mut u8).write_volatile(0);      // css
        (desc.add(14) as *mut u16).write_volatile(0);     // vlan
    }

    /// Invalidate cache line then read DD (belt-and-suspenders vs UC map under WHPX).
    unsafe fn clflush_desc(ring_virt: *const u8, idx: usize) {
        let desc = ring_virt.add(idx * core::mem::size_of::<RxDesc>());
        core::arch::asm!("clflush [{}]", in(reg) desc, options(nostack, preserves_flags));
        Self::fence_read();
    }

    /// Lê DD bit do RX descriptor via raw ptr.
    unsafe fn read_rx_dd(ring_virt: *const u8, idx: usize) -> bool {
        Self::clflush_desc(ring_virt, idx);
        let desc = ring_virt.add(idx * core::mem::size_of::<RxDesc>());
        (desc.add(12) as *const u8).read_volatile() & 0x01 != 0
    }

    /// Lê length do RX descriptor via raw ptr.
    unsafe fn read_rx_len(ring_virt: *const u8, idx: usize) -> u16 {
        let desc = ring_virt.add(idx * core::mem::size_of::<RxDesc>());
        Self::fence_read();
        (desc.add(8) as *const u16).read_volatile()
    }

    pub unsafe fn send(&mut self, data: &[u8]) -> bool {
        if data.is_empty() || data.len() > 2048 { return false; }

        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let tx_ring_virt = (self.tx_ring_paddr + pmoff) as *mut u8;
        let idx = self.tx_cur;

        // Copy data into TX buffer
        let buf_virt = (self.tx_buf_paddrs[idx] + pmoff) as *mut u8;
        for i in 0..data.len() {
            buf_virt.add(i).write_volatile(data[i]);
        }

        // Write TX descriptor via raw ptr + sfence
        Self::write_tx_desc(tx_ring_virt, idx, data.len() as u16, 0x0B);
        Self::fence_write();

        // Advance TDT
        let next = (idx + 1) % TX_DESC_COUNT;
        self.tx_cur = next;
        self.write32(REG_TDT, next as u32);

        true
    }

    pub unsafe fn recv(&mut self) -> Option<Vec<u8>> {
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let rx_ring_virt = (self.rx_ring_paddr + pmoff) as *const u8;
        let idx = self.rx_cur;

        if !Self::read_rx_dd(rx_ring_virt, idx) { return None; }

        let len = Self::read_rx_len(rx_ring_virt, idx) as usize;
        if len < 14 || len > 2048 {
            // Mark descriptor as available again via raw ptr
            let desc = (self.rx_ring_paddr + pmoff) as *mut u8;
            (desc.add(idx * core::mem::size_of::<RxDesc>() + 12) as *mut u8).write_volatile(0);
            self.write32(REG_RDT, idx as u32);
            self.rx_cur = (idx + 1) % RX_DESC_COUNT;
            return None;
        }

        let mut buf = Vec::with_capacity(len);
        let data_virt = (self.rx_buf_paddrs[idx] + pmoff) as *const u8;
        for i in 0..len {
            buf.push(data_virt.add(i).read_volatile());
        }

        // Mark descriptor as processed
        let desc = (self.rx_ring_paddr + pmoff) as *mut u8;
        (desc.add(idx * core::mem::size_of::<RxDesc>() + 12) as *mut u8).write_volatile(0);
        Self::fence_write();
        // Return THIS descriptor to HW (RDT = last software-owned index).
        self.write32(REG_RDT, idx as u32);
        self.rx_cur = (idx + 1) % RX_DESC_COUNT;

        Some(buf)
    }

    pub fn mac(&self) -> [u8; 6] { self.mac_addr }
    pub unsafe fn read_e1000_rdh(&self) -> u32 { self.read32(REG_RDH) }
    pub fn rx_cur_val(&self) -> usize { self.rx_cur }

    /// True if any RX descriptor has DD set (after clflush).
    pub unsafe fn any_rx_dd(&self) -> bool {
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let ring = (self.rx_ring_paddr + pmoff) as *const u8;
        for i in 0..RX_DESC_COUNT {
            if Self::read_rx_dd(ring, i) {
                return true;
            }
        }
        false
    }

    /// Count RX descriptors with DD=1.
    pub unsafe fn count_rx_dd(&self) -> u32 {
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let ring = (self.rx_ring_paddr + pmoff) as *const u8;
        let mut n = 0u32;
        for i in 0..RX_DESC_COUNT {
            if Self::read_rx_dd(ring, i) {
                n += 1;
            }
        }
        n
    }

    /// Wall-clock pause (rdtsc @~2GHz). Needed so QEMU slirp can inject ARP reply.
    unsafe fn pause_us(us: u64) {
        let cycles = us.saturating_mul(2_000);
        let start = {
            let lo: u32;
            let hi: u32;
            core::arch::asm!("rdtsc", out("eax") lo, out("edx") hi, options(nostack, nomem, preserves_flags));
            ((hi as u64) << 32) | (lo as u64)
        };
        loop {
            let now = {
                let lo: u32;
                let hi: u32;
                core::arch::asm!("rdtsc", out("eax") lo, out("edx") hi, options(nostack, nomem, preserves_flags));
                ((hi as u64) << 32) | (lo as u64)
            };
            if now.wrapping_sub(start) >= cycles {
                break;
            }
            core::hint::spin_loop();
        }
    }

    /// Wait until TX descriptor `idx` reports DD (or timeout).
    unsafe fn wait_tx_dd(&self, idx: usize, timeout_us: u64) -> bool {
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let desc = (self.tx_ring_paddr + pmoff) as *const u8;
        let status_ptr = desc.add(idx * core::mem::size_of::<TxDesc>() + 12);
        let steps = (timeout_us / 50).max(1);
        for _ in 0..steps {
            Self::fence_read();
            if status_ptr.read_volatile() & 0x01 != 0 {
                return true;
            }
            Self::pause_us(50);
        }
        false
    }

    /// Kick RX engine: clear DD, re-enable, THEN set RDT.
    /// NÃO escrever RDH (RO no HW real) nem RDT==RDH — QEMU e1000 trata RDH==RDT
    /// como ring full e deixa de entregar RX (slirp/TAP).
    pub unsafe fn kick_rx(&mut self) {
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let rctl = RCTL_EN | RCTL_SBP | RCTL_UPE | RCTL_MPE | RCTL_LBM_NONE
            | RCTL_BAM | RCTL_SECRC | RCTL_BSIZE_2048 | RCTL_RDMTS_HALF;

        // Disable receiver
        self.write32(REG_RCTRL, 0);
        Self::fence_write();

        // Clear DD on all RX descs and reset software cursor
        let ring = (self.rx_ring_paddr + pmoff) as *mut u8;
        for i in 0..RX_DESC_COUNT {
            let status_ptr = ring.add(i * core::mem::size_of::<RxDesc>() + 12) as *mut u8;
            status_ptr.write_volatile(0);
            // Re-assert buffer addr (QEMU may have rewritten desc)
            let addr_ptr = ring.add(i * core::mem::size_of::<RxDesc>()) as *mut u64;
            addr_ptr.write_volatile(self.rx_buf_paddrs[i]);
        }
        Self::fence_write();
        self.rx_cur = 0;

        // Re-enable receiver FIRST (Linux order), then RDT with one spare slot.
        // Never poke RDH; never park RDT==RDH (QEMU false-full).
        self.write32(REG_RCTRL, rctl);
        Self::fence_write();
        let rdh = self.read32(REG_RDH) as usize % RX_DESC_COUNT;
        let rdt = (rdh + RX_DESC_COUNT - 1) % RX_DESC_COUNT;
        self.write32(REG_RDT, rdt as u32);
        let _ = self.read32(REG_ICR);
        crate::slog_nano!(
            "Net",
            "e1000",
            "RX kick: RDH={} RDT={} RCTL={:#010x} (no RDH poke)",
            rdh,
            rdt,
            rctl
        );
    }

    /// Build + send ARP request for `tip` (who-has). Used to prove RX before DNS.
    pub unsafe fn send_arp_request(&mut self, sip: [u8; 4], tip: [u8; 4]) -> bool {
        let mac = self.mac_addr;
        let mut frame = [0u8; 42];
        // Ethernet: dst broadcast, src MAC, ethertype ARP
        frame[0..6].copy_from_slice(&[0xff; 6]);
        frame[6..12].copy_from_slice(&mac);
        frame[12] = 0x08;
        frame[13] = 0x06;
        // ARP: HTYPE=1 PTYPE=0x0800 HLEN=6 PLEN=4 OPER=1 (request)
        frame[14] = 0x00; frame[15] = 0x01;
        frame[16] = 0x08; frame[17] = 0x00;
        frame[18] = 6; frame[19] = 4;
        frame[20] = 0x00; frame[21] = 0x01;
        frame[22..28].copy_from_slice(&mac);
        frame[28..32].copy_from_slice(&sip);
        // tha = zeros
        frame[38..42].copy_from_slice(&tip);
        self.send(&frame)
    }

    /// Poll RX after ARP who-has; returns (rdh, dd_count, got_pkt).
    /// `iters` ≈ poll rounds × 200µs wall (~iters*0.2ms). Retries ARP 3× for slirp latency.
    pub unsafe fn prove_rx(&mut self, sip: [u8; 4], tip: [u8; 4], iters: u32) -> (u32, u32, bool) {
        self.kick_rx();
        let rounds = 3u32;
        let per_round = (iters / rounds).max(200);
        for attempt in 0..rounds {
            let tx_idx = self.tx_cur;
            let sent = self.send_arp_request(sip, tip);
            let tx_dd = if sent {
                self.wait_tx_dd(tx_idx, 5_000)
            } else {
                false
            };
            let tdh = self.read32(REG_TDH);
            let tdt = self.read32(REG_TDT);
            crate::slog_nano!(
                "Net",
                "e1000",
                "prove_rx ARP#{} sent={} tx_dd={} TDH={} TDT={}",
                attempt + 1,
                sent,
                tx_dd,
                tdh,
                tdt
            );
            for _ in 0..per_round {
                if let Some(_pkt) = self.recv() {
                    let rdh = self.read32(REG_RDH);
                    let dd = self.count_rx_dd();
                    return (rdh, dd.saturating_add(1), true);
                }
                if self.any_rx_dd() {
                    let rdh = self.read32(REG_RDH);
                    return (rdh, self.count_rx_dd(), true);
                }
                Self::pause_us(200);
            }
        }
        let rdh = self.read32(REG_RDH);
        (rdh, self.count_rx_dd(), false)
    }

    pub unsafe fn dump_status(&mut self) {
        let ctrl = self.read32(REG_CTRL);
        let status = self.read32(REG_STATUS);
        let rdh = self.read32(REG_RDH);
        let rdt = self.read32(REG_RDT);
        let tdh = self.read32(REG_TDH);
        let tdt = self.read32(REG_TDT);
        let rctrl = self.read32(REG_RCTRL);
        let rah = self.read32(REG_RAH);
        let rdbal = self.read32(REG_RDBAL);
        let rdbah = self.read32(REG_RDBAH);
        let rdlen = self.read32(REG_RDLEN);
        let icr = self.read32(REG_ICR);
        // Read first RX descriptor status from descriptor ring in RAM
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let desc0_virt = (self.rx_ring_paddr + pmoff) as *const RxDesc;
        let desc0_status = (*desc0_virt).status;
        let desc0_len = (*desc0_virt).length;
        crate::slog_nano!("Net", "e1000", "CTRL={:#010x} STATUS={:#010x} link={} speed={} AV={}",
            ctrl, status,
            if status & 0x02 != 0 { "UP" } else { "DOWN" },
            match (status >> 6) & 0x03 {
            0 => "10Mb", 1 => "100Mb", 2 => "1000Mb", _ => "?"
            },
            if rah & 0x80000000 != 0 { "1" } else { "0" });
        crate::slog_nano!("Net", "e1000", "RDH={} RDT={} TDH={} TDT={} RCTRL={:#010x} RAH={:#010x}", rdh, rdt, tdh, tdt, rctrl, rah);
        crate::slog_nano!("Net", "e1000", "RDBAL={:#010x} RDBAH={:#010x} RDLEN={} ICR={:#010x}", rdbal, rdbah, rdlen, icr);
        let desc0_addr = (*desc0_virt).addr;
        // Raw bytes of descriptor 0 (read as u8 array to verify exact format)
        let raw = desc0_virt as *const u8;
        crate::slog_nano!("Net", "e1000", "desc0 status={:#04x} len={} dd={} addr={:#010x}",
            desc0_status, desc0_len, if desc0_status & 0x01 != 0 { "1" } else { "0" }, desc0_addr);
        crate::slog_nano!("Net", "e1000", "desc0 raw: {:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x} {:02x}{:02x}|{:02x}{:02x}|{:02x}{:02x}{:02x}{:02x}",
            core::ptr::read_volatile(raw.add(0)),
            core::ptr::read_volatile(raw.add(1)),
            core::ptr::read_volatile(raw.add(2)),
            core::ptr::read_volatile(raw.add(3)),
            core::ptr::read_volatile(raw.add(4)),
            core::ptr::read_volatile(raw.add(5)),
            core::ptr::read_volatile(raw.add(6)),
            core::ptr::read_volatile(raw.add(7)),
            core::ptr::read_volatile(raw.add(8)),
            core::ptr::read_volatile(raw.add(9)),
            core::ptr::read_volatile(raw.add(10)),
            core::ptr::read_volatile(raw.add(11)),
            core::ptr::read_volatile(raw.add(12)),
            core::ptr::read_volatile(raw.add(13)),
            core::ptr::read_volatile(raw.add(14)),
            core::ptr::read_volatile(raw.add(15)));
    }
}
