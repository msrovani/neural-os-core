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

pub struct NvmeDriver {
    mmio: *mut u32,
    pmoff: u64,
    dstrd: u32,
    admin_sq: QueueMem,
    admin_cq: QueueMem,
    io_sq: QueueMem,
    io_cq: QueueMem,
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
        let data_pa = (buf as u64).wrapping_sub(self.pmoff);
        self.io_nvm(IO_READ, lba, data_pa, blocks)
    }

    pub unsafe fn write_blocks(&mut self, lba: u64, buf: *const u8, blocks: u32) -> bool {
        if blocks == 0 || self.io_sq.virt.is_null() {
            return false;
        }
        let data_pa = (buf as u64).wrapping_sub(self.pmoff);
        self.io_nvm(IO_WRITE, lba, data_pa, blocks)
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
        if !self.io_nvm(IO_READ, start_lba, pa, nlb.max(1)) {
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
            if !self.io_nvm(IO_READ, start_lba, pa, 1) {
                return false;
            }
            let dst = va.add(((lba % 8) * 512) as usize);
            core::ptr::copy_nonoverlapping(buf.as_ptr(), dst, buf.len());
            self.io_nvm(IO_WRITE, start_lba, pa, 1)
        } else {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), va, buf.len());
            self.io_nvm(IO_WRITE, lba, pa, sectors)
        }
    }

    unsafe fn io_nvm(&mut self, opcode: u8, lba: u64, data_pa: u64, blocks: u32) -> bool {
        let cid = self.alloc_cid();
        let tail = self.io_sq_tail;
        let entry = self.io_sq.virt.add((tail as usize) * 64) as *mut u32;
        for i in 0..16 {
            entry.add(i).write_volatile(0);
        }
        entry.add(0).write_volatile(opcode as u32 | ((cid as u32) << 16));
        entry.add(1).write_volatile(self.nsid);
        entry.add(6).write_volatile(data_pa as u32);
        entry.add(7).write_volatile((data_pa >> 32) as u32);
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
