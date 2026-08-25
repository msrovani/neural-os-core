//! NVMe driver (ADR-0062 P3) — Admin queue + I/O queue (qid=1) para NVM Read/Write.
//! SQE layout NVMe 1.4; doorbell 0x1000+(2*qid)*stride. Bounce DMA para BlockDevice.

use crate::memory::{GLOBAL_ALLOCATOR, PHYS_MEM_OFFSET};
use core::sync::atomic::Ordering;
use spin::Mutex;

/// Handle global do NVMe (BlockDevice + DiskAgent + StorageBus).
pub static NVME_DRIVER: Mutex<Option<NvmeDriver>> = Mutex::new(None);

const NVME_CAP: u64 = 0x0000;
const NVME_CC: u64 = 0x0014;
const NVME_CSTS: u64 = 0x001C;
const NVME_AQA: u64 = 0x0024;
const NVME_ASQ: u64 = 0x0028;
const NVME_ACQ: u64 = 0x0030;

const CC_EN: u32 = 0x0001;
const CSTS_RDY: u32 = 0x0001;

const ADMIN_CREATE_SQ: u8 = 0x01;
const ADMIN_CREATE_CQ: u8 = 0x05;
const ADMIN_IDENTIFY: u8 = 0x06;
const IO_WRITE: u8 = 0x01;
const IO_READ: u8 = 0x02;

const Q_ENTRIES: u32 = 64;

/// Resultado do layout PRP para uma transferência DMA (ADR-0087 Fase 1).
/// 512 entradas = 1 página 4KB de entradas de 8B (cabe em `prp_list` do driver).
#[derive(Debug, Clone, Copy)]
pub struct PrpLayout {
    pub prp1: u64,
    pub prp2: u64,
    pub list: [u64; 512],
    pub n_list: usize,
    /// true se a transferência precisa de mais de 512 entradas de lista
    /// (impossível de enviar com a página fixa do driver) — `n_list` fica 0.
    pub overflow: bool,
}

/// Algoritmo do Linux `nvme_setup_prps` (regras de PRP NVMe 1.4):
/// - cabe numa página (offset+len <= page_size): só PRP1.
/// - exatamente 2 páginas: PRP1 + PRP2 apontando direto para a 2ª página.
/// - mais de 2 páginas: PRP1 + PRP2 = endereço da página da lista PRP
///   (lista preenchida por entrada, cada uma apontando para uma página).
/// `page_size` deve ser potência de 2 (o driver usa 4096).
/// Função pura — sem HW, testável no host.
pub fn nvme_prp_layout(dma_addr: u64, len: usize, page_size: usize) -> PrpLayout {
    let mut out = PrpLayout {
        prp1: dma_addr,
        prp2: 0,
        list: [0u64; 512],
        n_list: 0,
        overflow: false,
    };
    if len == 0 {
        return out;
    }
    let ps = page_size as u64;
    let offset = dma_addr & (ps - 1);
    let first_page = dma_addr & !(ps - 1);
    if offset + len as u64 <= ps {
        return out; // prp1 = dma_addr, prp2 = 0
    }
    let mut remaining = len as u64 - (ps - offset);
    let mut cur = first_page + ps;
    if remaining <= ps {
        out.prp2 = cur; // segunda página direto, sem lista
        return out;
    }
    // Lista PRP: entradas apontam para cada página restante.
    let mut n = 0usize;
    while remaining > 0 {
        if n >= 512 {
            out.n_list = 0;
            out.prp2 = 0;
            out.overflow = true;
            return out;
        }
        out.list[n] = cur;
        n += 1;
        cur += ps;
        remaining = remaining.saturating_sub(ps);
    }
    out.n_list = n;
    // prp2 = endereço da PÁGINA da lista (o caller fornece a página DMA).
    out
}

pub struct NvmeDriver {
    mmio: *mut u32,
    pmoff: u64,
    dstrd: u32,
    admin_sq: QueueMem,
    admin_cq: QueueMem,
    io_sq: QueueMem,
    io_cq: QueueMem,
    /// 1 página fixa para listas PRP (ponytail: preenchida por chamada, sem alloc/free por transfer).
    prp_list: QueueMem,
    admin_sq_tail: u32,
    admin_cq_head: u32,
    admin_cq_phase: u16,
    io_sq_tail: u32,
    io_cq_head: u32,
    io_cq_phase: u16,
    next_cid: u16,
    pub nsid: u32,
    pub lba_count: u64,
    pub lba_size: u32,
    pub model: [u8; 40],
}

struct QueueMem {
    phys: u64,
    virt: *mut u8,
}

unsafe impl Send for NvmeDriver {}
unsafe impl Sync for NvmeDriver {}

impl NvmeDriver {
    pub unsafe fn probe() -> Option<Self> {
        let devs = crate::pci::scan_pci();
        let dev = devs.iter().find(|d| d.class == 0x01 && d.subclass == 0x08)?;
        let pmoff = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        let bar0 = (dev.bar0 & !0xF) as u64 | ((dev.bar1 as u64) << 32);
        crate::apic::map_page_uc(bar0, pmoff);
        crate::apic::map_page_uc(bar0 + 0x1000, pmoff);
        crate::pci::enable_pci_bus_master(dev);

        let mmio = (bar0 + pmoff) as *mut u32;
        let cap_lo = mmio.add((NVME_CAP / 4) as usize).read_volatile() as u64;
        let cap_hi = mmio.add((NVME_CAP / 4 + 1) as usize).read_volatile() as u64;
        let cap = cap_lo | (cap_hi << 32);
        let dstrd = ((cap >> 32) & 0xF) as u32;

        let admin_sq = Self::alloc_q(Q_ENTRIES)?;
        let admin_cq = Self::alloc_q(Q_ENTRIES)?;
        let prp_list = match Self::alloc_dma(1) {
            Some((pa, va)) => QueueMem { phys: pa, virt: va },
            None => return None,
        };

        // Disable controller
        mmio.add((NVME_CC / 4) as usize).write_volatile(0);
        for _ in 0..500_000 {
            if mmio.add((NVME_CSTS / 4) as usize).read_volatile() & CSTS_RDY == 0 {
                break;
            }
            core::hint::spin_loop();
        }

        let aqa = ((Q_ENTRIES - 1) << 16) | (Q_ENTRIES - 1);
        mmio.add((NVME_AQA / 4) as usize).write_volatile(aqa);
        write_u64(mmio, NVME_ASQ, admin_sq.phys);
        write_u64(mmio, NVME_ACQ, admin_cq.phys);

        // Enable: CSS=NVM, MPS=0 (4KiB), AMS=RR, IOSQES=6 (64B), IOCQES=4 (16B)
        let cc = CC_EN | (6 << 16) | (4 << 20);
        mmio.add((NVME_CC / 4) as usize).write_volatile(cc);
        for _ in 0..500_000 {
            if mmio.add((NVME_CSTS / 4) as usize).read_volatile() & CSTS_RDY != 0 {
                break;
            }
            core::hint::spin_loop();
        }
        if mmio.add((NVME_CSTS / 4) as usize).read_volatile() & CSTS_RDY == 0 {
            crate::slog_nano!("NVMe", "err", "controller enable timeout");
            return None;
        }

        let mut drv = NvmeDriver {
            mmio,
            pmoff,
            dstrd,
            admin_sq,
            admin_cq,
            io_sq: QueueMem {
                phys: 0,
                virt: core::ptr::null_mut(),
            },
            io_cq: QueueMem {
                phys: 0,
                virt: core::ptr::null_mut(),
            },
            prp_list,
            admin_sq_tail: 0,
            admin_cq_head: 0,
            admin_cq_phase: 1,
            io_sq_tail: 0,
            io_cq_head: 0,
            io_cq_phase: 1,
            next_cid: 1,
            nsid: 1,
            lba_count: 0,
            lba_size: 512,
            model: [0u8; 40],
        };

        if !drv.identify_controller() {
            crate::slog_nano!("NVMe", "err", "Identify Controller FAIL");
            return None;
        }
        if !drv.identify_namespace(1) {
            crate::slog_nano!("NVMe", "err", "Identify NS1 FAIL");
            return None;
        }
        if !drv.create_io_cq() || !drv.create_io_sq() {
            crate::slog_nano!("NVMe", "err", "Create I/O queues FAIL");
            return None;
        }

        let gb = (drv.lba_count as u128 * drv.lba_size as u128) / (1024 * 1024 * 1024);
        crate::slog_nano!(
            "NVMe",
            "info",
            "NS{}: {} setores x {}B = {}GB (IO qid=1)",
            drv.nsid,
            drv.lba_count,
            drv.lba_size,
            gb
        );
        let m = core::str::from_utf8(&drv.model).unwrap_or("NVMe");
        crate::slog_nano!("NVMe", "info", "Modelo: {}", m.trim());
        Some(drv)
    }

    unsafe fn identify_controller(&mut self) -> bool {
        let (pa, va) = match Self::alloc_dma(1) {
            Some(b) => b,
            None => return false,
        };
        core::ptr::write_bytes(va, 0, 4096);
        let cid = self.alloc_cid();
        // Identify CNS=0 (Controller), NSID=0
        let mut cdw = [0u32; 16];
        cdw[0] = ADMIN_IDENTIFY as u32 | ((cid as u32) << 16);
        cdw[1] = 0;
        cdw[6] = pa as u32;
        cdw[7] = (pa >> 32) as u32;
        cdw[10] = 0; // CNS = Controller
        if !self.admin_submit_and_wait(&cdw) {
            return false;
        }
        // MN at offset 24, 40 bytes
        let src = va.add(24);
        for i in 0..40 {
            self.model[i] = src.add(i).read_volatile();
        }
        true
    }

    unsafe fn identify_namespace(&mut self, nsid: u32) -> bool {
        let (pa, va) = match Self::alloc_dma(1) {
            Some(b) => b,
            None => return false,
        };
        core::ptr::write_bytes(va, 0, 4096);
        let cid = self.alloc_cid();
        let mut cdw = [0u32; 16];
        cdw[0] = ADMIN_IDENTIFY as u32 | ((cid as u32) << 16);
        cdw[1] = nsid;
        cdw[6] = pa as u32;
        cdw[7] = (pa >> 32) as u32;
        cdw[10] = 1; // CNS = Namespace
        if !self.admin_submit_and_wait(&cdw) {
            return false;
        }
        let nsze = read_u64(va as *const u8);
        let flbas = va.add(26).read_volatile();
        let lba_idx = (flbas & 0x0F) as usize;
        let lbaf = va.add(128 + lba_idx * 4) as *const u32;
        let lbaf_val = lbaf.read_volatile();
        let lbads = ((lbaf_val >> 16) & 0xFF) as u32;
        self.nsid = nsid;
        self.lba_count = nsze;
        self.lba_size = if lbads == 0 { 512 } else { 1u32 << lbads };
        true
    }

    unsafe fn create_io_cq(&mut self) -> bool {
        let cq = match Self::alloc_q(Q_ENTRIES) {
            Some(q) => q,
            None => return false,
        };
        let cq_phys = cq.phys;
        self.io_cq = cq;
        self.io_cq_head = 0;
        self.io_cq_phase = 1;
        let cid = self.alloc_cid();
        let mut cdw = [0u32; 16];
        cdw[0] = ADMIN_CREATE_CQ as u32 | ((cid as u32) << 16);
        cdw[6] = cq_phys as u32;
        cdw[7] = (cq_phys >> 32) as u32;
        // CDW10: QID=1 | QSIZE=Q_ENTRIES-1
        cdw[10] = 1 | ((Q_ENTRIES - 1) << 16);
        // CDW11: PC=1 (physically contiguous)
        cdw[11] = 1;
        self.admin_submit_and_wait(&cdw)
    }

    unsafe fn create_io_sq(&mut self) -> bool {
        let sq = match Self::alloc_q(Q_ENTRIES) {
            Some(q) => q,
            None => return false,
        };
        let sq_phys = sq.phys;
        self.io_sq = sq;
        self.io_sq_tail = 0;
        let cid = self.alloc_cid();
        let mut cdw = [0u32; 16];
        cdw[0] = ADMIN_CREATE_SQ as u32 | ((cid as u32) << 16);
        cdw[6] = sq_phys as u32;
        cdw[7] = (sq_phys >> 32) as u32;
        // CDW10: QID=1 | QSIZE
        cdw[10] = 1 | ((Q_ENTRIES - 1) << 16);
        // CDW11: CQID=1 | PC=1
        cdw[11] = 1 | (1 << 16);
        self.admin_submit_and_wait(&cdw)
    }

    unsafe fn admin_submit_and_wait(&mut self, cdw: &[u32; 16]) -> bool {
        let tail = self.admin_sq_tail;
        let entry = self.admin_sq.virt.add((tail as usize) * 64) as *mut u32;
        for i in 0..16 {
            entry.add(i).write_volatile(cdw[i]);
        }
        self.admin_sq_tail = (tail + 1) % Q_ENTRIES;
        self.ring_sq_db(0, self.admin_sq_tail);

        let deadline = 500_000u32;
        for _ in 0..deadline {
            if let Some(ok) = self.poll_admin_cq() {
                return ok;
            }
            core::hint::spin_loop();
        }
        false
    }

    unsafe fn poll_admin_cq(&mut self) -> Option<bool> {
        let head = self.admin_cq_head;
        let cqe = self.admin_cq.virt.add((head as usize) * 16) as *const u32;
        let dw3 = cqe.add(3).read_volatile();
        let phase = (dw3 & 1) as u16;
        if phase != self.admin_cq_phase {
            return None;
        }
        let status = (dw3 >> 17) & 0x7FF;
        self.admin_cq_head = (head + 1) % Q_ENTRIES;
        if self.admin_cq_head == 0 {
            self.admin_cq_phase ^= 1;
        }
        self.ring_cq_db(0, self.admin_cq_head);
        Some(status == 0)
    }

    /// Read `blocks` LBAs starting at `lba` into `buf` (length >= blocks * lba_size).
    /// `buf` must be identity-mapped DMA (phys = va - pmoff). Prefer `read_blocks_bounce`.
    pub unsafe fn read_blocks(&mut self, lba: u64, buf: *mut u8, blocks: u32) -> bool {
        if blocks == 0 || self.io_sq.virt.is_null() {
            return false;
        }
        let dma_phys = (buf as u64).wrapping_sub(self.pmoff);
        let len = blocks as usize * self.lba_size as usize;
        self.io_nvm_prp(IO_READ, lba, dma_phys, len, blocks)
    }

    pub unsafe fn write_blocks(&mut self, lba: u64, buf: *const u8, blocks: u32) -> bool {
        if blocks == 0 || self.io_sq.virt.is_null() {
            return false;
        }
        let dma_phys = (buf as u64).wrapping_sub(self.pmoff);
        let len = blocks as usize * self.lba_size as usize;
        self.io_nvm_prp(IO_WRITE, lba, dma_phys, len, blocks)
    }

    /// Zero-copy: o caller passa o endereço FÍSICO de uma região DMA contígua;
    /// o driver monta os PRPs direto (ADR-0087 Fase 1, path MHI).
    pub unsafe fn read_blocks_direct(&mut self, lba: u64, dma_phys: u64, len: usize) -> bool {
        if len == 0 || self.io_sq.virt.is_null() {
            return false;
        }
        let blocks = (len + self.lba_size as usize - 1) / self.lba_size as usize;
        self.io_nvm_prp(IO_READ, lba, dma_phys, len, blocks as u32)
    }

    /// Zero-copy: ver `read_blocks_direct`.
    pub unsafe fn write_blocks_direct(&mut self, lba: u64, dma_phys: u64, len: usize) -> bool {
        if len == 0 || self.io_sq.virt.is_null() {
            return false;
        }
        let blocks = (len + self.lba_size as usize - 1) / self.lba_size as usize;
        self.io_nvm_prp(IO_WRITE, lba, dma_phys, len, blocks as u32)
    }

    /// Monta PRPs (nvme_prp_layout) e submete o comando. Usa a página fixa
    /// `prp_list` quando a transferência precisa de lista PRP.
    unsafe fn io_nvm_prp(&mut self, opcode: u8, lba: u64, dma_phys: u64, len: usize, blocks: u32) -> bool {
        let layout = nvme_prp_layout(dma_phys, len, 4096);
        if layout.overflow {
            crate::slog_nano!("NVMe", "err", "PRP list overflow (>512 páginas)");
            return false;
        }
        let prp2 = if layout.n_list > 0 {
            // Preenche a página fixa de lista PRP (entradas de 8B, little-endian nativo).
            let list = self.prp_list.virt as *mut u64;
            for i in 0..layout.n_list {
                list.add(i).write_volatile(layout.list[i]);
            }
            self.prp_list.phys
        } else {
            layout.prp2
        };
        self.io_nvm(opcode, lba, layout.prp1, prp2, blocks)
    }

    /// BlockDevice-friendly: DMA bounce (1 page) then copy out.
    pub unsafe fn read_sectors_bounce(&mut self, lba: u64, buf: &mut [u8]) -> bool {
        if buf.len() % 512 != 0 || buf.is_empty() {
            return false;
        }
        let sectors = (buf.len() / 512) as u32;
        let (pa, va) = match Self::alloc_dma(((sectors as usize * 512) + 4095) / 4096) {
            Some(b) => b,
            None => return false,
        };
        let nlb = if self.lba_size == 512 {
            sectors
        } else if self.lba_size == 4096 {
            // Caller still uses 512B BlockDevice units — read covering LBAs
            ((sectors as u64 * 512 + self.lba_size as u64 - 1) / self.lba_size as u64) as u32
        } else {
            sectors
        };
        let start_lba = if self.lba_size == 4096 {
            lba / 8
        } else {
            lba
        };
        let pages = ((sectors as usize * 512) + 4095) / 4096;
        let xfer = pages * 4096;
        if !self.io_nvm_prp(IO_READ, start_lba, pa, xfer, nlb.max(1)) {
            return false;
        }
        let src = if self.lba_size == 4096 {
            va.add(((lba % 8) * 512) as usize)
        } else {
            va
        };
        core::ptr::copy_nonoverlapping(src, buf.as_mut_ptr(), buf.len());
        true
    }

    pub unsafe fn write_sectors_bounce(&mut self, lba: u64, buf: &[u8]) -> bool {
        if buf.len() % 512 != 0 || buf.is_empty() {
            return false;
        }
        let sectors = (buf.len() / 512) as u32;
        let pages = ((sectors as usize * 512) + 4095) / 4096;
        let (pa, va) = match Self::alloc_dma(pages) {
            Some(b) => b,
            None => return false,
        };
        if self.lba_size == 4096 {
            // RMW for partial 4K
            let start_lba = lba / 8;
            if !self.io_nvm(IO_READ, start_lba, pa, 0, 1) {
                return false;
            }
            let dst = va.add(((lba % 8) * 512) as usize);
            core::ptr::copy_nonoverlapping(buf.as_ptr(), dst, buf.len());
            self.io_nvm(IO_WRITE, start_lba, pa, 0, 1)
        } else {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), va, buf.len());
            self.io_nvm_prp(IO_WRITE, lba, pa, pages * 4096, sectors)
        }
    }

    unsafe fn io_nvm(&mut self, opcode: u8, lba: u64, prp1: u64, prp2: u64, blocks: u32) -> bool {
        let cid = self.alloc_cid();
        let tail = self.io_sq_tail;
        let entry = self.io_sq.virt.add((tail as usize) * 64) as *mut u32;
        for i in 0..16 {
            entry.add(i).write_volatile(0);
        }
        entry.add(0).write_volatile(opcode as u32 | ((cid as u32) << 16));
        entry.add(1).write_volatile(self.nsid);
        entry.add(6).write_volatile(prp1 as u32);
        entry.add(7).write_volatile((prp1 >> 32) as u32);
        entry.add(8).write_volatile(prp2 as u32);
        entry.add(9).write_volatile((prp2 >> 32) as u32);
        entry.add(10).write_volatile(lba as u32);
        entry.add(11).write_volatile((lba >> 32) as u32);
        entry.add(12).write_volatile(blocks.saturating_sub(1)); // NLB 0-based

        self.io_sq_tail = (tail + 1) % Q_ENTRIES;
        self.ring_sq_db(1, self.io_sq_tail);

        for _ in 0..500_000 {
            if let Some(ok) = self.poll_io_cq() {
                return ok;
            }
            core::hint::spin_loop();
        }
        false
    }

    unsafe fn poll_io_cq(&mut self) -> Option<bool> {
        let head = self.io_cq_head;
        let cqe = self.io_cq.virt.add((head as usize) * 16) as *const u32;
        let dw3 = cqe.add(3).read_volatile();
        let phase = (dw3 & 1) as u16;
        if phase != self.io_cq_phase {
            return None;
        }
        let status = (dw3 >> 17) & 0x7FF;
        self.io_cq_head = (head + 1) % Q_ENTRIES;
        if self.io_cq_head == 0 {
            self.io_cq_phase ^= 1;
        }
        self.ring_cq_db(1, self.io_cq_head);
        Some(status == 0)
    }

    fn alloc_cid(&mut self) -> u16 {
        let c = self.next_cid;
        self.next_cid = self.next_cid.wrapping_add(1);
        if self.next_cid == 0 {
            self.next_cid = 1;
        }
        c
    }

    /// SQ doorbell for queue y at 0x1000 + (2y) * (4 << dstrd)
    unsafe fn ring_sq_db(&self, qid: u32, tail: u32) {
        core::sync::atomic::fence(Ordering::Release);
        let stride = 4u32 << self.dstrd;
        let off = 0x1000 + (2 * qid) * stride;
        self.mmio.add((off / 4) as usize).write_volatile(tail);
    }

    /// CQ doorbell for queue y at 0x1000 + (2y+1) * (4 << dstrd)
    unsafe fn ring_cq_db(&self, qid: u32, head: u32) {
        core::sync::atomic::fence(Ordering::Release);
        let stride = 4u32 << self.dstrd;
        let off = 0x1000 + (2 * qid + 1) * stride;
        self.mmio.add((off / 4) as usize).write_volatile(head);
    }

    unsafe fn alloc_q(entries: u32) -> Option<QueueMem> {
        let bytes = (entries * 64) as usize;
        let pages = (bytes + 4095) / 4096;
        let mut g = GLOBAL_ALLOCATOR.lock();
        let a = g.as_mut()?;
        let f = a.allocate_contiguous(pages)?;
        let phys = f.start_address().as_u64();
        let pm = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        let virt = (phys + pm) as *mut u8;
        core::ptr::write_bytes(virt, 0, pages * 4096);
        Some(QueueMem { phys, virt })
    }

    pub unsafe fn alloc_dma(pages: usize) -> Option<(u64, *mut u8)> {
        let mut g = GLOBAL_ALLOCATOR.lock();
        let a = g.as_mut()?;
        let f = a.allocate_contiguous(pages)?;
        let phys = f.start_address().as_u64();
        let pm = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        Some((phys, (phys + pm) as *mut u8))
    }
}

unsafe fn write_u64(mmio: *mut u32, off: u64, val: u64) {
    mmio.add((off / 4) as usize).write_volatile(val as u32);
    mmio.add((off / 4 + 1) as usize).write_volatile((val >> 32) as u32);
}

unsafe fn read_u64(p: *const u8) -> u64 {
    let lo = core::ptr::read_unaligned(p as *const u32) as u64;
    let hi = core::ptr::read_unaligned(p.add(4) as *const u32) as u64;
    lo | (hi << 32)
}

#[cfg(test)]
mod tests {
    use super::nvme_prp_layout;

    const PS: usize = 4096;

    #[test]
    fn prp_cabe_em_uma_pagina() {
        // alinhado: 1 página inteira
        let l = nvme_prp_layout(0x1000, PS, PS);
        assert_eq!(l.prp1, 0x1000);
        assert_eq!(l.prp2, 0);
        assert_eq!(l.n_list, 0);
        assert!(!l.overflow);
        // desalinhado: 1 byte no fim da página 0x1000 + 1 byte na página 0x2000
        let l2 = nvme_prp_layout(0x1FFF, 2, PS);
        assert_eq!(l2.prp1, 0x1FFF);
        assert_eq!(l2.prp2, 0x2000);
        assert_eq!(l2.n_list, 0);
        assert!(!l2.overflow);
    }

    #[test]
    fn prp_exatamente_duas_paginas() {
        let l = nvme_prp_layout(0x1000, 2 * PS, PS);
        assert_eq!(l.prp1, 0x1000);
        assert_eq!(l.prp2, 0x2000);
        assert_eq!(l.n_list, 0);
        assert!(!l.overflow);
    }

    #[test]
    fn prp_lista_com_mais_de_duas_paginas() {
        // 4 páginas alinhadas: prp1 = 0x1000, lista = 0x2000..0x4000 (3 entradas)
        let l = nvme_prp_layout(0x1000, 4 * PS, PS);
        assert_eq!(l.prp1, 0x1000);
        assert_eq!(l.n_list, 3);
        assert_eq!(l.list[0], 0x2000);
        assert_eq!(l.list[1], 0x3000);
        assert_eq!(l.list[2], 0x4000);
        assert!(!l.overflow);
        // desalinhado: 0x1FFF + 2 páginas → prp1=0x1FFF, lista cobre 0x2000,0x3000
        let l2 = nvme_prp_layout(0x1FFF, PS + 2 * PS + 1, PS);
        assert_eq!(l2.prp1, 0x1FFF);
        assert_eq!(l2.n_list, 3);
        assert_eq!(l2.list[0], 0x2000);
        assert_eq!(l2.list[1], 0x3000);
        assert_eq!(l2.list[2], 0x4000);
        assert!(!l2.overflow);
    }
}
