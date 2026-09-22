//! AHCI driver — SATA 6G NCQ (Native Command Queuing).
//! PCI class 0x01/0x06 (SATA controller). MMIO-based register access.
//! Suporta ATAPI, NCQ, e múltiplas portas.

use alloc::vec::Vec;
use crate::memory::{GLOBAL_ALLOCATOR, PHYS_MEM_OFFSET};
use crate::pci::PciDevice;
use core::sync::atomic::Ordering;

const HBA_CAP: u64 = 0x00;     // Host Bus Adapter Capabilities
const HBA_GHC: u64 = 0x04;     // Global Host Control
const HBA_PORTS: u64 = 0x100;  // Port registers (32 ports × 0x80)

const GHC_AE: u32 = 0x80000000; // AHCI Enable
const GHC_HR: u32 = 0x00000001; // HBA Reset

// Port register offsets — AHCI 1.3.1 spec §3.3 (Table 3-1 do HBA Port register set)
const PXCLB: u64 = 0x00;  // Command List Base (32-bit)
const PXCLBU: u64 = 0x04; // Command List Base Upper (32-bit)
const PXFB: u64 = 0x08;   // FIS Base (32-bit)
const PXFBU: u64 = 0x0C;  // FIS Base Upper
const PXIS: u64 = 0x10;   // Interrupt Status
const PXIE: u64 = 0x14;   // Interrupt Enable
const PXCMD: u64 = 0x18;  // Command and Status
const PXTFD: u64 = 0x20;  // Task File Data
const PXSIG: u64 = 0x24;  // Signature
const PXSSTS: u64 = 0x28; // SATA Status
const PXSCTL: u64 = 0x2C; // SATA Control
const PXSERR: u64 = 0x30; // SATA Error
const PXSACT: u64 = 0x34; // SATA Active
const PXCI: u64 = 0x38;   // Command Issue

const CMD_ST: u32 = 0x0001; // Start
const CMD_FRE: u32 = 0x0010; // FIS Receive Enable
const CMD_CR: u32 = 0x8000; // Command Running
const CMD_FR: u32 = 0x4000; // FIS Receive Running

const SATA_SIG_ATAPI: u32 = 0xEB140101;
const SATA_SIG_PM: u32 = 0x96690101;
const SATA_SIG_SEMB: u32 = 0xCD340101;

/// Espera PxCI bit0 limpar. Antes: 100k spin + return true mesmo se CI ainda set
/// (falso sucesso). Agora: TSC 5s ou spin fallback; timeout → false.
unsafe fn wait_ci_clear(port_base: u64) -> bool {
    const BUDGET_US: u64 = 5_000_000;
    const SPIN_FALLBACK: u32 = 100_000;
    if crate::tsc::tsc_hz() != 0 {
        let t0 = crate::tsc::now_us();
        loop {
            let ci = core::ptr::read_volatile((port_base + PXCI) as *const u32);
            if ci & 1 == 0 {
                return true;
            }
            if crate::tsc::now_us().saturating_sub(t0) > BUDGET_US {
                return false;
            }
            core::hint::spin_loop();
        }
    }
    for _ in 0..SPIN_FALLBACK {
        let ci = core::ptr::read_volatile((port_base + PXCI) as *const u32);
        if ci & 1 == 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

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
        crate::slog_nano!("Disk", "ahci", "HBA at {:#x} GHC={:#x}", mmio_base, ghc);

        let cap = core::ptr::read_volatile((mmio_virt + HBA_CAP) as *const u32);
        let ports_impl = core::ptr::read_volatile((mmio_virt + 0x0C) as *const u32);
        crate::slog_nano!("Disk", "ahci", "CAP={:#x} PORTS_IMPL={:#x}", cap, ports_impl);

        let mut driver = AhciDriver { mmio_virt, ports: Vec::new(), pci_bus: dev.bus, pci_dev: dev.device, pci_fn: dev.function };

        for i in 0..32u8 {
            if ports_impl & (1 << i) == 0 { continue; }
            let port_base = mmio_virt + HBA_PORTS + i as u64 * 0x80;
            let ssts = core::ptr::read_volatile((port_base + PXSSTS) as *const u32);
            let dev_present = (ssts & 0x0F) == 0x03; // IPM active + DET present
            if !dev_present { continue; }

            let sig = core::ptr::read_volatile((port_base + PXSIG) as *const u32);
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

            // Stop port DMA before config — CR stuck ⇒ skip porta (não reprogramar).
            let cmd = core::ptr::read_volatile((port_base + PXCMD) as *const u32);
            core::ptr::write_volatile((port_base + PXCMD) as *mut u32, cmd & !CMD_ST);
            let mut cr_clear = false;
            for _ in 0..1000 {
                let c = core::ptr::read_volatile((port_base + PXCMD) as *const u32);
                if c & CMD_CR == 0 {
                    cr_clear = true;
                    break;
                }
                core::hint::spin_loop();
            }
            if !cr_clear {
                crate::slog_nano!(
                    "Disk",
                    "warn",
                    "AHCI port {} CMD.CR stuck — skip (não reprograma CLB)",
                    i
                );
                continue;
            }

            core::ptr::write_volatile((port_base + PXCLB) as *mut u32, clb_pa as u32);
            core::ptr::write_volatile((port_base + PXCLBU) as *mut u32, (clb_pa >> 32) as u32);
            core::ptr::write_volatile((port_base + PXFB) as *mut u32, fb_pa as u32);
            core::ptr::write_volatile((port_base + PXFBU) as *mut u32, (fb_pa >> 32) as u32);

            let cmd_new = CMD_ST | CMD_FRE;
            core::ptr::write_volatile((port_base + PXCMD) as *mut u32, cmd_new);
            crate::slog_nano!("Disk", "ahci", "Port {}: {} dev_type={} sig={:#x} ssts={:#x}",
                i, if dev_type == AHCI_DEV_ATA { "SATA" } else { "OTHER" }, dev_type, sig, ssts);

            driver.ports.push(AhciPort {
                present: true, port_num: i, dev_type, mmio_virt: port_base,
                clb_pa, fb_pa, cmd_slots: (cap >> 8) & 0x1F,
            });
        }
        if driver.ports.is_empty() { return None; }
        Some(driver)
    }

    /// 1º HBA AHCI no PCI; grava `AHCI_DRIVER`. Sem rescan se já ocupado.
    pub unsafe fn probe_first() -> bool {
        use crate::globals::AHCI_DRIVER;
        if AHCI_DRIVER.lock().is_some() {
            return true;
        }
        let mut found = false;
        crate::pci::scan_pci_cb(|bus, slot, func, vid, did| {
            let cr = crate::pci::read_config_word(bus, slot, func, 0x0A);
            if (cr >> 8) as u8 != 0x01 || (cr & 0xFF) as u8 != 0x06 {
                return false;
            }
            let pi = (crate::pci::read_config_word(bus, slot, func, 0x08) >> 8) as u8;
            let bar0_val = crate::pci::read_bar_value(bus, slot, func, 0);
            let bar5_val = crate::pci::read_bar_value(bus, slot, func, 5);
            let dev = crate::pci::PciDevice {
                bus,
                device: slot,
                function: func,
                vendor_id: vid,
                device_id: did,
                class: 0x01,
                subclass: 0x06,
                prog_if: pi,
                bar0: bar0_val,
                bar1: 0,
                bar2: 0,
                bar3: 0,
                bar4: 0,
                bar5: bar5_val,
            };
            if let Some(ahci) = AhciDriver::new(&dev) {
                crate::slog_nano!("Disk", "ahci", "SATA controller init: {} ports", ahci.ports.len());
                *AHCI_DRIVER.lock() = Some(ahci);
                found = true;
                true
            } else {
                false
            }
        });
        if !found {
            crate::slog_nano!("Disk", "ahci", "Nenhum controlador SATA AHCI encontrado");
        }
        found
    }

    /// Lê setores via DMA (NCQ quando múltiplos comandos)
    pub unsafe fn read(&mut self, port_idx: usize, lba: u64, count: usize, buffer: &mut [u8]) -> bool {
        if port_idx >= self.ports.len() { return false; }
        let port = &self.ports[port_idx];
        if !port.present { return false; }
        let pmoff = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        let port_base = port.mmio_virt;

        // H4: command table page cacheada por driver (comandos síncronos, 1 slot).
        let ct_pa = ahci_ct_page();
        if ct_pa == 0 { return false; }
        let ct_va = ct_pa + pmoff;
        core::ptr::write_bytes(ct_va as *mut u8, 0, 256);

        // H3: PRDT por página de 4KB (DMA não-contíguo ok); reject se o
        // buffer cruzar mais páginas do que cabe na command table (248 entradas)
        // ou passar do teto DBC de 22 bits (4MB).
        let bytes = count * 512;
        let prdt_n = match build_prdt(ct_va + 0x80, buffer.as_ptr() as u64, bytes, pmoff, true) {
            Some(n) => n,
            None => {
                crate::slog_nano!("Disk", "warn", "AHCI read refuse: count={} ({}B) fora do limite PRDT", count, bytes);
                return false;
            }
        };

        // Command Header (32 bytes) at CLB + slot*32
        // H1: DW0 bits 0-4 CFL=5, bit5 A=0 (ATA, não ATAPI!), bit6 W=0 (read).
        let ch_va = (port.clb_pa + pmoff) as *mut u8;
        core::ptr::write_volatile(ch_va as *mut u16, 5u16);
        core::ptr::write_volatile(ch_va.add(0x02) as *mut u16, prdt_n as u16); // PRDTL
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
        if !wait_ci_clear(port_base) {
            crate::slog_nano!("Disk", "warn", "AHCI read CI TIMEOUT port={}", port_idx);
            return false;
        }
        // Verifica erro: PxIS.TFES (bit 30) = Task File Error Status
        let is = core::ptr::read_volatile((port_base + PXIS) as *const u32);
        if is & (1 << 30) != 0 {
            core::ptr::write_volatile((port_base + PXIS) as *mut u32, is); // clear IRQ (RWC)
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

        let ct_pa = ahci_ct_page();
        if ct_pa == 0 { return false; }
        let ct_va = ct_pa + pmoff;
        core::ptr::write_bytes(ct_va as *mut u8, 0, 256);

        let bytes = count * 512;
        let prdt_n = match build_prdt(ct_va + 0x80, buffer.as_ptr() as u64, bytes, pmoff, false) {
            Some(n) => n,
            None => {
                crate::slog_nano!("Disk", "warn", "AHCI write refuse: count={} ({}B) fora do limite PRDT", count, bytes);
                return false;
            }
        };

        // H1: CFL=5, W=1 (bit6), A=0 — nunca ATAPI num disco ATA.
        let ch_va = (port.clb_pa + pmoff) as *mut u8;
        core::ptr::write_volatile(ch_va as *mut u16, (0x40 | 5) as u16);
        core::ptr::write_volatile(ch_va.add(0x02) as *mut u16, prdt_n as u16); // PRDTL
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
        if !wait_ci_clear(port_base) {
            crate::slog_nano!("Disk", "warn", "AHCI write CI TIMEOUT port={}", port_idx);
            return false;
        }
        let is = core::ptr::read_volatile((port_base + PXIS) as *const u32);
        if is & (1 << 30) != 0 {
            core::ptr::write_volatile((port_base + PXIS) as *mut u32, is);
            return false;
        }
        true
    }

    /// ATA FLUSH CACHE EXT (0xEA) — sem PRDT. sync_cache do BlockDevice.
    pub unsafe fn flush_cache(&mut self) -> bool {
        if self.ports.is_empty() || !self.ports[0].present {
            return false;
        }
        let port = &self.ports[0];
        let pmoff = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        let port_base = port.mmio_virt;
        let ct_pa = ahci_ct_page();
        if ct_pa == 0 {
            return false;
        }
        let ct_va = ct_pa + pmoff;
        core::ptr::write_bytes(ct_va as *mut u8, 0, 128);
        let ch_va = (port.clb_pa + pmoff) as *mut u8;
        // CFL=5 DWORDs, PRDTL=0, sem bit Write.
        core::ptr::write_volatile(ch_va as *mut u16, 5u16);
        core::ptr::write_volatile(ch_va.add(0x02) as *mut u16, 0);
        core::ptr::write_volatile(ch_va.add(0x04) as *mut u16, 0);
        core::ptr::write_volatile(ch_va.add(0x08) as *mut u32, ct_pa as u32);
        core::ptr::write_volatile(ch_va.add(0x0C) as *mut u32, (ct_pa >> 32) as u32);
        core::ptr::write_volatile((ct_va + 0x00) as *mut u8, 0x27);
        core::ptr::write_volatile((ct_va + 0x01) as *mut u8, 0x80);
        core::ptr::write_volatile((ct_va + 0x02) as *mut u8, 0xEA); // FLUSH CACHE EXT
        core::arch::asm!("sfence", options(nostack, preserves_flags));
        core::ptr::write_volatile((port_base + PXCI) as *mut u32, 1);
        if !wait_ci_clear(port_base) {
            crate::slog_nano!("Disk", "warn", "AHCI flush CI TIMEOUT");
            return false;
        }
        let is = core::ptr::read_volatile((port_base + PXIS) as *const u32);
        if is & (1 << 30) != 0 {
            core::ptr::write_volatile((port_base + PXIS) as *mut u32, is);
            return false;
        }
        true
    }
}

/// Traduz um endereço virtual de um buffer DMA para físico.
/// Unifica a lógica usada por `read()` e `write()`: buffers alocados no heap
/// Tier 1 (talc, base em `allocator::HEAP_START`) sao VA = PA + pmoff; buffers
/// identity-mapped (primeiros 4GB, ex: stack ou frames do bootloader) sao VA == PA.
/// HHDM: PA = VA − pmoff. HEAP_BUFFER (bump) vive no HHDM; o ramo antigo
/// só cobria TALC em HEAP_START e entregava VA canónica como PA no metal.
fn dma_va_to_pa(va: u64, pmoff: u64) -> u64 {
    if pmoff != 0 && va >= pmoff {
        va - pmoff
    } else {
        va
    }
}

fn alloc_ahci_page() -> u64 {
    let mut guard = GLOBAL_ALLOCATOR.lock();
    let alloc = match guard.as_mut() { Some(a) => a, None => return 0 };
    match alloc.allocate_contiguous(1) {
        Some(f) => f.start_address().as_u64(),
        None => 0,
    }
}

/// H4 (onda3): página de command table alocada UMA vez por driver — comandos
/// são síncronos (espera PxCI), slot único basta. Antes: allocate_contiguous
/// por I/O, sem free = leak de frames do PMM a cada read/write.
static AHCI_CT_PAGE: spin::Mutex<u64> = spin::Mutex::new(0);

fn ahci_ct_page() -> u64 {
    let mut g = AHCI_CT_PAGE.lock();
    if *g == 0 {
        *g = alloc_ahci_page();
    }
    *g
}

/// Entradas PRDT que cabem na página de 4KB da command table:
/// PRDT começa em +0x80 → (4096 - 128) / 16 = 248.
const AHCI_PRDT_MAX: usize = 248;
/// DBC tem 22 bits → um entry carrega no máx 4MB (nossos chunks são ≤4KB).
const AHCI_MAX_XFER: usize = AHCI_PRDT_MAX * 4096;

/// H3 (onda3): monta a PRDT página a página — o buffer não precisa ser
/// fisicamente contíguo; cada página vira uma entrada. `ioc_last` marca IOC
/// (bit 31, Interrupt On Completion) apenas na última entrada.
/// None = transferência grande demais (logado pelo caller).
unsafe fn build_prdt(prdt_va: u64, buf_va: u64, bytes: usize, pmoff: u64, ioc_last: bool) -> Option<usize> {
    if bytes == 0 || bytes > AHCI_MAX_XFER {
        return None;
    }
    let mut n = 0usize;
    let mut off = 0usize;
    while off < bytes {
        if n >= AHCI_PRDT_MAX {
            return None;
        }
        let va = buf_va + off as u64;
        let chunk = core::cmp::min(bytes - off, 4096 - (va as usize & 0xFFF));
        let pa = dma_va_to_pa(va, pmoff);
        let e = prdt_va + (n * 16) as u64;
        core::ptr::write_volatile(e as *mut u32, pa as u32);                     // DBA
        core::ptr::write_volatile((e + 4) as *mut u32, (pa >> 32) as u32);       // DBA upper
        let ioc = if ioc_last && off + chunk == bytes { 0x4000_0000 } else { 0 };
        core::ptr::write_volatile((e + 8) as *mut u32, (chunk - 1) as u32 | ioc); // DBC + IOC
        core::ptr::write_volatile((e + 12) as *mut u32, 0);                      // reserved
        n += 1;
        off += chunk;
    }
    Some(n)
}
