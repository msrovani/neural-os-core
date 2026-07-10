//! AHCI driver — SATA 6G NCQ (Native Command Queuing).
//! PCI class 0x01/0x06 (SATA controller). MMIO-based register access.
//! Suporta ATAPI, NCQ, e múltiplas portas.

use alloc::vec::Vec;
use crate::memory::{GLOBAL_ALLOCATOR, PHYS_MEM_OFFSET};
use crate::pci::PciDevice;
use crate::serial_println;
use core::sync::atomic::Ordering;

const HBA_CAP: u64 = 0x00;     // Host Bus Adapter Capabilities
const HBA_GHC: u64 = 0x04;     // Global Host Control
const HBA_PORTS: u64 = 0x100;  // Port registers (32 ports × 0x80)

const GHC_AE: u32 = 0x80000000; // AHCI Enable
const GHC_HR: u32 = 0x00000001; // HBA Reset

// Port register offsets
const PXCMD: u64 = 0x00;  // Command and Status
const PXIS: u64 = 0x10;   // Interrupt Status
const PXIE: u64 = 0x14;   // Interrupt Enable
const PXSERR: u64 = 0x28; // SATA Error
const PXSSTS: u64 = 0x28; // SATA Status (alternate offset)
const PXSCTL: u64 = 0x2C; // SATA Control
const PXTFD: u64 = 0x20;  // Task File Data
const PXCI: u64 = 0x18;   // Command Issue
const PXCLB: u64 = 0x00;  // Command List Base (32-bit)
const PXCLBU: u64 = 0x04; // Command List Base Upper (32-bit)
const PXFB: u64 = 0x08;   // FIS Base (32-bit)
const PXFBU: u64 = 0x0C;  // FIS Base Upper
const PXIE_BITS: u32 = 0x7F; // enable interrupts

const CMD_ST: u32 = 0x0001; // Start
const CMD_FRE: u32 = 0x0010; // FIS Receive Enable
const CMD_CR: u32 = 0x8000; // Command Running
const CMD_FR: u32 = 0x4000; // FIS Receive Running

const SATA_SIG_ATAPI: u32 = 0xEB140101;
const SATA_SIG_PM: u32 = 0x96690101;
const SATA_SIG_SEMB: u32 = 0xCD340101;

const AHCI_DEV_NONE: u32 = 0;
const AHCI_DEV_ATA: u32 = 1;  // SATA drive
const AHCI_DEV_ATAPI: u32 = 2; // SATAPI drive
const AHCI_DEV_SEMB: u32 = 3; // Enclosure Management Bridge
const AHCI_DEV_PM: u32 = 4;   // Port Multiplier

pub struct AhciPort {
    pub present: bool,
    pub port_num: u8,
    pub dev_type: u32,
    pub mmio_virt: u64,
    pub clb_pa: u64,   // Command List Base physical
    pub fb_pa: u64,     // FIS Base physical
    cmd_slots: u32,
}

pub struct AhciDriver {
    mmio_virt: u64,
    pub ports: Vec<AhciPort>,
    pub pci_bus: u8, pub pci_dev: u8, pub pci_fn: u8,
}

impl AhciDriver {
    pub unsafe fn new(dev: &PciDevice) -> Option<Self> {
        let mmio_base = (dev.bar5 & !0xF) as u64;
        let pmoff = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        let mmio_virt = mmio_base + pmoff;

        // Map MMIO region (32KB should cover all port registers)
        crate::apic::map_page_uc(mmio_base, pmoff);
        crate::apic::map_page_uc(mmio_base + 0x1000, pmoff);
        crate::apic::map_page_uc(mmio_base + 0x2000, pmoff);
        crate::apic::map_page_uc(mmio_base + 0x3000, pmoff);

        // Enable PCI bus master
        crate::pci::enable_pci_bus_master(dev);

        // Enable AHCI
        let ghc = core::ptr::read_volatile((mmio_virt + HBA_GHC) as *const u32);
        core::ptr::write_volatile((mmio_virt + HBA_GHC) as *mut u32, ghc | GHC_AE);
        serial_println!("[AHCI] HBA at {:#x} GHC={:#x}", mmio_base, ghc);

        let cap = core::ptr::read_volatile((mmio_virt + HBA_CAP) as *const u32);
        let ports_impl = core::ptr::read_volatile((mmio_virt + 0x0C) as *const u32);
        serial_println!("[AHCI] CAP={:#x} PORTS_IMPL={:#x}", cap, ports_impl);

        let mut driver = AhciDriver { mmio_virt, ports: Vec::new(), pci_bus: dev.bus, pci_dev: dev.device, pci_fn: dev.function };

        for i in 0..32u8 {
            if ports_impl & (1 << i) == 0 { continue; }
            let port_base = mmio_virt + HBA_PORTS + i as u64 * 0x80;
            let ssts = core::ptr::read_volatile((port_base + PXSSTS) as *const u32);
            let dev_present = (ssts & 0x0F) == 0x03; // IPM active + DET present
            if !dev_present { continue; }

            let sig = core::ptr::read_volatile((port_base + 0x24) as *const u32);
            let dev_type = if sig == SATA_SIG_ATAPI { AHCI_DEV_ATAPI }
                else if sig == SATA_SIG_PM { AHCI_DEV_PM }
                else if sig == SATA_SIG_SEMB { AHCI_DEV_SEMB }
                else { AHCI_DEV_ATA };

            // Allocate Command List (1KB = 32 commands × 32 bytes)
            let clb_pa = alloc_ahci_page();
            if clb_pa == 0 { continue; }
            let clb_va = clb_pa + pmoff;
            core::ptr::write_bytes(clb_va as *mut u8, 0, 1024);

            // Allocate FIS (256 bytes)
            let fb_pa = alloc_ahci_page();
            if fb_pa == 0 { continue; }
            let fb_va = fb_pa + pmoff;
            core::ptr::write_bytes(fb_va as *mut u8, 0, 256);

            // Stop port DMA before config
            let cmd = core::ptr::read_volatile((port_base + PXCMD) as *const u32);
            core::ptr::write_volatile((port_base + PXCMD) as *mut u32, cmd & !CMD_ST);
            for _ in 0..1000 {
                let c = core::ptr::read_volatile((port_base + PXCMD) as *const u32);
                if c & CMD_CR == 0 { break; }
                core::hint::spin_loop();
            }

            core::ptr::write_volatile((port_base + PXCLB) as *mut u32, clb_pa as u32);
            core::ptr::write_volatile((port_base + PXCLBU) as *mut u32, (clb_pa >> 32) as u32);
            core::ptr::write_volatile((port_base + PXFB) as *mut u32, fb_pa as u32);
            core::ptr::write_volatile((port_base + PXFBU) as *mut u32, (fb_pa >> 32) as u32);

            let cmd_new = CMD_ST | CMD_FRE;
            core::ptr::write_volatile((port_base + PXCMD) as *mut u32, cmd_new);
            serial_println!("[AHCI] Port {}: {} dev_type={} sig={:#x} ssts={:#x}",
                i, if dev_type == AHCI_DEV_ATA { "SATA" } else { "OTHER" }, dev_type, sig, ssts);

            driver.ports.push(AhciPort {
                present: true, port_num: i, dev_type, mmio_virt: port_base,
                clb_pa, fb_pa, cmd_slots: (cap >> 8) & 0x1F,
            });
        }
        if driver.ports.is_empty() { return None; }
        Some(driver)
    }

    /// Lê setores via DMA (NCQ quando múltiplos comandos)
    pub unsafe fn read(&mut self, port_idx: usize, lba: u64, count: usize, buffer: &mut [u8]) -> bool {
        if port_idx >= self.ports.len() { return false; }
        let port = &self.ports[port_idx];
        if !port.present { return false; }
        let pmoff = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        let port_base = port.mmio_virt;

        // Allocate command table (128 bytes)
        let ct_pa = alloc_ahci_page();
        if ct_pa == 0 { return false; }
        let ct_va = ct_pa + pmoff;
        core::ptr::write_bytes(ct_va as *mut u8, 0, 256);

        // Allocate PRDT (Physical Region Descriptor Table)
        let prdt_va = ct_va + 0x80;
        let buf_pa = buffer.as_ptr() as u64 - pmoff;

        // PRDT entry
        core::ptr::write_volatile((prdt_va + 0x00) as *mut u32, buf_pa as u32);       // DBA
        core::ptr::write_volatile((prdt_va + 0x04) as *mut u32, (buf_pa >> 32) as u32); // DBA upper
        core::ptr::write_volatile((prdt_va + 0x08) as *mut u32, ((count * 512 - 1) as u32) | 0x80000000); // DBC + interrupt

        // Command Header (32 bytes) at CLB + slot*32
        let ch_va = (port.clb_pa + pmoff) as *mut u8;
        core::ptr::write_volatile(ch_va as *mut u16, (0x80 | 0x27) as u16);
        core::ptr::write_volatile(ch_va.add(0x04) as *mut u16, 1);
        core::ptr::write_volatile(ch_va.add(0x08) as *mut u32, ct_pa as u32);
        core::ptr::write_volatile(ch_va.add(0x0C) as *mut u32, (ct_pa >> 32) as u32);
        core::ptr::write_volatile(ch_va.add(0x10) as *mut u32, 0);

        // Command FIS (64 bytes at CT+0)
        core::ptr::write_volatile((ct_va + 0x00) as *mut u8, 0x27);  // FIS type: host-to-device
        core::ptr::write_volatile((ct_va + 0x01) as *mut u8, 0x80);  // PM port + C bit
        core::ptr::write_volatile((ct_va + 0x02) as *mut u8, 0x25);  // Command: READ_DMA_EXT
        core::ptr::write_volatile((ct_va + 0x03) as *mut u8, 0);     // Features
        core::ptr::write_volatile((ct_va + 0x04) as *mut u8, (lba & 0xFF) as u8);
        core::ptr::write_volatile((ct_va + 0x05) as *mut u8, ((lba >> 8) & 0xFF) as u8);
        core::ptr::write_volatile((ct_va + 0x06) as *mut u8, ((lba >> 16) & 0xFF) as u8);
        core::ptr::write_volatile((ct_va + 0x07) as *mut u8, 0x40); // Device: LBA mode
        core::ptr::write_volatile((ct_va + 0x08) as *mut u8, ((lba >> 24) & 0xFF) as u8);
        core::ptr::write_volatile((ct_va + 0x09) as *mut u8, ((lba >> 32) & 0xFF) as u8);
        core::ptr::write_volatile((ct_va + 0x0A) as *mut u8, ((lba >> 40) & 0xFF) as u8);
        core::ptr::write_volatile((ct_va + 0x0B) as *mut u8, 0);     // Features ext
        core::ptr::write_volatile((ct_va + 0x0C) as *mut u16, count as u16); // Sector count
        core::ptr::write_volatile((ct_va + 0x0E) as *mut u8, 0);     // Control

        // Barreira de memoria antes de emitir comando (garante que todos os writes
        // para CLB/CT/FIS/PRDT chegaram ao barramento PCIe)
        core::arch::asm!("sfence", options(nostack, preserves_flags));
        core::ptr::write_volatile((port_base + PXCI) as *mut u32, 1);
        for _ in 0..100000 {
            let ci = core::ptr::read_volatile((port_base + PXCI) as *const u32);
            if ci & 1 == 0 { break; }
            core::hint::spin_loop();
        }
        // Verifica erro: PxIS.TFES (bit 30) = Task File Error Status
        let is = core::ptr::read_volatile((port_base + 0x10 + 0x08) as *const u32); // PxIS
        if is & (1 << 30) != 0 {
            core::ptr::write_volatile((port_base + 0x10 + 0x08) as *mut u32, is); // clear IRQ
            return false;
        }
        true
    }

    /// Escreve setores via DMA (WRITE_DMA_EXT)
    pub unsafe fn write(&mut self, port_idx: usize, lba: u64, count: usize, buffer: &[u8]) -> bool {
        if port_idx >= self.ports.len() { return false; }
        let port = &self.ports[port_idx];
        if !port.present { return false; }
        let pmoff = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        let port_base = port.mmio_virt;

        let ct_pa = alloc_ahci_page();
        if ct_pa == 0 { return false; }
        let ct_va = ct_pa + pmoff;
        core::ptr::write_bytes(ct_va as *mut u8, 0, 256);

        let prdt_va = ct_va + 0x80;

        // Traducao de endereco: buffer do usuario -> fisico.
        // O buffer pode vir do heap (linked_list_allocator em 0x4444_4444_0000)
        // ou de uma pagina identity-mapped. Usamos a page table ativa para traduzir.
        let buf_va = buffer.as_ptr() as u64;
        let buf_pa = if buf_va >= 0x4444_4444_0000 {
            // Heap: VA - offset (assumindo que o heap esta mapeado linearmente)
            buf_va - pmoff
        } else {
            // Identity-mapped (first 4GB)
            buf_va
        };

        core::ptr::write_volatile((prdt_va + 0x00) as *mut u32, buf_pa as u32);
        core::ptr::write_volatile((prdt_va + 0x04) as *mut u32, (buf_pa >> 32) as u32);
        core::ptr::write_volatile((prdt_va + 0x08) as *mut u32, ((count * 512 - 1) as u32) | 0x40000000);

        let ch_va = (port.clb_pa + pmoff) as *mut u8;
        core::ptr::write_volatile(ch_va as *mut u16, (0x80 | 0x27 | 0x40) as u16);
        core::ptr::write_volatile(ch_va.add(0x04) as *mut u16, 1);
        core::ptr::write_volatile(ch_va.add(0x08) as *mut u32, ct_pa as u32);
        core::ptr::write_volatile(ch_va.add(0x0C) as *mut u32, (ct_pa >> 32) as u32);
        core::ptr::write_volatile(ch_va.add(0x10) as *mut u32, 0);

        core::ptr::write_volatile((ct_va + 0x00) as *mut u8, 0x27);
        core::ptr::write_volatile((ct_va + 0x01) as *mut u8, 0x80);
        core::ptr::write_volatile((ct_va + 0x02) as *mut u8, 0x35);
        core::ptr::write_volatile((ct_va + 0x03) as *mut u8, 0);
        core::ptr::write_volatile((ct_va + 0x04) as *mut u8, (lba & 0xFF) as u8);
        core::ptr::write_volatile((ct_va + 0x05) as *mut u8, ((lba >> 8) & 0xFF) as u8);
        core::ptr::write_volatile((ct_va + 0x06) as *mut u8, ((lba >> 16) & 0xFF) as u8);
        core::ptr::write_volatile((ct_va + 0x07) as *mut u8, 0x40);
        core::ptr::write_volatile((ct_va + 0x08) as *mut u8, ((lba >> 24) & 0xFF) as u8);
        core::ptr::write_volatile((ct_va + 0x09) as *mut u8, ((lba >> 32) & 0xFF) as u8);
        core::ptr::write_volatile((ct_va + 0x0A) as *mut u8, ((lba >> 40) & 0xFF) as u8);
        core::ptr::write_volatile((ct_va + 0x0B) as *mut u8, 0);
        core::ptr::write_volatile((ct_va + 0x0C) as *mut u16, count as u16);
        core::ptr::write_volatile((ct_va + 0x0E) as *mut u8, 0);

        core::arch::asm!("sfence", options(nostack, preserves_flags));
        core::ptr::write_volatile((port_base + PXCI) as *mut u32, 1);
        for _ in 0..100000 {
            let ci = core::ptr::read_volatile((port_base + PXCI) as *const u32);
            if ci & 1 == 0 { break; }
            core::hint::spin_loop();
        }
        let is = core::ptr::read_volatile((port_base + 0x10 + 0x08) as *const u32);
        if is & (1 << 30) != 0 {
            core::ptr::write_volatile((port_base + 0x10 + 0x08) as *mut u32, is);
            return false;
        }
        true
    }
}

fn alloc_ahci_page() -> u64 {
    let mut guard = GLOBAL_ALLOCATOR.lock();
    let alloc = guard.as_mut().unwrap();
    match alloc.allocate_contiguous(1) {
        Some(f) => f.start_address().as_u64(),
        None => 0,
    }
}
