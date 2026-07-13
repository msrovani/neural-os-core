use crate::memory::{PHYS_MEM_OFFSET, GLOBAL_ALLOCATOR};

const NVME_CC: u64 = 0x0014;
const NVME_CSTS: u64 = 0x001C;
const NVME_ASQ: u64 = 0x0028;
const NVME_ACQ: u64 = 0x0020;

const CC_EN: u32 = 0x0001;
const CSTS_RDY: u32 = 0x0001;

const ADMIN_IDENTIFY: u8 = 0x06;
const ADMIN_CREATE_SQ: u8 = 0x01;
const ADMIN_CREATE_CQ: u8 = 0x05;
const IO_READ: u8 = 0x02;
const IO_WRITE: u8 = 0x01;
const IO_DSM: u8 = 0x09;

pub struct NvmeDriver {
    mmio: *mut u32,
    pmoff: u64,
    qdepth: u32,
    dstrd: u32,
    admin_sq: QueueMem,
    admin_cq: QueueMem,
    io_sq: QueueMem,
    io_cq: QueueMem,
    pub nsid: u32,
    pub lba_count: u64,
    pub lba_size: u32,
    pub model: [u8; 40],
}

struct QueueMem {
    phys: u64,
    virt: *mut u32,
}

unsafe impl Send for NvmeDriver {}
unsafe impl Sync for NvmeDriver {}

impl NvmeDriver {
    pub unsafe fn probe() -> Option<Self> {
        let devs = crate::pci::scan_pci();
        let dev = devs.iter().find(|d| d.class == 0x01 && d.subclass == 0x08)?;
        let pmoff = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let bar0 = (dev.bar0 & !0xF) as u64 | ((dev.bar1 as u64) << 32);
        crate::apic::set_page_uc(bar0, pmoff);
        crate::apic::set_page_uc(bar0 + 0x1000, pmoff);

        let mmio = (bar0 + pmoff) as *mut u32;
        let cap = mmio.add(0).read_volatile() as u64 | ((mmio.add(1).read_volatile() as u64) << 32);
        let qdepth = ((cap >> 48) & 0xFFFF) as u32 + 1;
        let dstrd = ((cap >> 32) & 0xF) as u32;

        let admin_sq = Self::alloc_q(64)?;
        let admin_cq = Self::alloc_q(64)?;

        // Set admin queue base
        mmio.add((NVME_ACQ / 4) as usize).write_volatile(admin_cq.phys as u32);
        mmio.add((NVME_ACQ / 4 + 1) as usize).write_volatile((admin_cq.phys >> 32) as u32);
        mmio.add((NVME_ASQ / 4) as usize).write_volatile(admin_sq.phys as u32);
        mmio.add((NVME_ASQ / 4 + 1) as usize).write_volatile((admin_sq.phys >> 32) as u32);

        // Enable controller
        mmio.add((NVME_CC / 4) as usize).write_volatile(CC_EN | (6 << 16) | (4 << 20));
        for _ in 0..500000 {
            if mmio.add((NVME_CSTS / 4) as usize).read_volatile() & CSTS_RDY != 0 { break; }
            core::hint::spin_loop();
        }

        let mut drv = NvmeDriver {
            mmio, pmoff, qdepth, dstrd,
            admin_sq, admin_cq,
            io_sq: QueueMem { phys: 0, virt: core::ptr::null_mut() },
            io_cq: QueueMem { phys: 0, virt: core::ptr::null_mut() },
            nsid: 1, lba_count: 0, lba_size: 512, model: [0u8; 40],
        };

        if drv.admin_identify() && drv.create_io_cq() && drv.create_io_sq() {
            let gb = (drv.lba_count as u128 * drv.lba_size as u128) / (1024*1024*1024);
            crate::serial_println!("[NVMe] NS{}: {} setores x {}B = {}GB", drv.nsid, drv.lba_count, drv.lba_size, gb);
            let m = core::str::from_utf8(&drv.model).unwrap_or("NVMe");
            crate::serial_println!("[NVMe] Modelo: {}", m.trim());
            Some(drv)
        } else {
            None
        }
    }

    unsafe fn admin_identify(&mut self) -> bool {
        let buf = match Self::alloc_dma(1) { Some(b) => b, None => return false };
        core::ptr::write_bytes(buf.1, 0, 4096);

        let cmd = self.admin_sq.virt;
        cmd.add(0).write_volatile(0);
        cmd.add(1).write_volatile(0);
        cmd.add(2).write_volatile(buf.0 as u32);
        cmd.add(3).write_volatile((buf.0 >> 32) as u32);
        cmd.add(4).write_volatile(0);
        cmd.add(5).write_volatile(0);
        cmd.add(6).write_volatile(ADMIN_IDENTIFY as u32 | (1 << 24));
        cmd.add(7).write_volatile(0);
        cmd.add(8).write_volatile(0);
        cmd.add(9).write_volatile(0);
        cmd.add(10).write_volatile(0);
        cmd.add(11).write_volatile(0);
        cmd.add(12).write_volatile(self.nsid);
        cmd.add(13).write_volatile(0);
        cmd.add(14).write_volatile(0);
        cmd.add(15).write_volatile(0);

        self.ring_db(0, 1);

        let cq = self.admin_cq.virt;
        for _ in 0..500000 {
            if cq.add(3).read_volatile() & 0x01 != 0 {
                let status = (cq.add(3).read_volatile() >> 1) & 0x7F;
                if status != 0 { return false; }

                let data = buf.1 as *const u32;
                self.lba_count = data.add(0).read_volatile() as u64
                    | ((data.add(1).read_volatile() as u64) << 32);
                self.lba_size = data.add(2).read_volatile() & 0xFFFF;

                let m = data.add(8) as *const u8;
                for i in 0..40 { self.model[i] = m.add(i).read_volatile(); }

                self.mmio.add((NVME_CC / 4) as usize).write_volatile(
                    CC_EN | (6 << 16) | (4 << 20) | (1 << 16));
                return true;
            }
            core::hint::spin_loop();
        }
        false
    }

    unsafe fn create_io_cq(&mut self) -> bool {
        let cq = match Self::alloc_q(64) { Some(q) => q, None => return false };
        let cq_phys = cq.phys;
        self.io_cq = cq;
        let cmd = self.admin_sq.virt;
        cmd.add(0).write_volatile(0); cmd.add(1).write_volatile(0);
        cmd.add(2).write_volatile(cq_phys as u32);
        cmd.add(3).write_volatile((cq_phys >> 32) as u32);
        cmd.add(4).write_volatile(0); cmd.add(5).write_volatile(0);
        cmd.add(6).write_volatile((1u32 << 31) | 63);
        cmd.add(7).write_volatile(0);
        cmd.add(8).write_volatile(1);
        cmd.add(9).write_volatile(ADMIN_CREATE_CQ as u32);
        for i in 10..16 { cmd.add(i).write_volatile(0); }
        self.ring_db(0, 1);
        self.poll_admin(50000)
    }

    unsafe fn create_io_sq(&mut self) -> bool {
        let sq = match Self::alloc_q(64) { Some(q) => q, None => return false };
        let sq_phys = sq.phys;
        self.io_sq = sq;
        let cmd = self.admin_sq.virt;
        cmd.add(0).write_volatile(0); cmd.add(1).write_volatile(0);
        cmd.add(2).write_volatile(sq_phys as u32);
        cmd.add(3).write_volatile((sq_phys >> 32) as u32);
        cmd.add(4).write_volatile(0); cmd.add(5).write_volatile(0);
        cmd.add(6).write_volatile(63 | (1 << 16));
        cmd.add(7).write_volatile(1 | (1 << 17));
        cmd.add(8).write_volatile(ADMIN_CREATE_SQ as u32);
        for i in 9..16 { cmd.add(i).write_volatile(0); }
        self.ring_db(0, 1);
        self.poll_admin(50000)
    }

    unsafe fn poll_admin(&mut self, timeout: u32) -> bool {
        let cq = self.admin_cq.virt;
        for _ in 0..timeout {
            if cq.add(3).read_volatile() & 0x01 != 0 {
                let status = (cq.add(3).read_volatile() >> 1) & 0x7F;
                self.mmio.add((NVME_CC / 4) as usize).write_volatile(
                    CC_EN | (6 << 16) | (4 << 20) | (1 << 16));
                return status == 0;
            }
            core::hint::spin_loop();
        }
        false
    }

    pub unsafe fn read_blocks(&mut self, lba: u64, buf: *mut u8, blocks: u32) -> bool {
        let data_pa = (buf as u64) - self.pmoff;
        let cmd = self.io_sq.virt;
        cmd.add(0).write_volatile(0);
        cmd.add(1).write_volatile(0);
        cmd.add(2).write_volatile(data_pa as u32);
        cmd.add(3).write_volatile((data_pa >> 32) as u32);
        cmd.add(4).write_volatile(self.nsid);
        cmd.add(5).write_volatile(lba as u32);
        cmd.add(6).write_volatile((lba >> 32) as u32);
        cmd.add(7).write_volatile(blocks - 1);
        cmd.add(8).write_volatile(IO_READ as u32);
        for i in 9..16 { cmd.add(i).write_volatile(0); }

        self.ring_db(1, 1);

        let cq = self.io_cq.virt;
        for _ in 0..500000 {
            if cq.add(3).read_volatile() & 0x01 != 0 {
                let status = (cq.add(3).read_volatile() >> 1) & 0x7F;
                cq.add(3).write_volatile(0);
                return status == 0;
            }
            core::hint::spin_loop();
        }
        false
    }

    unsafe fn ring_db(&self, qid: u32, count: u32) {
        // sfence ANTES do doorbell: garante que command data esta visivel via DMA
        core::sync::atomic::fence(core::sync::atomic::Ordering::Release);
        let stride = 1 << (self.dstrd + 2);
        let db = self.mmio.add((0x1000 + qid as usize * stride as usize) / 4);
        db.write_volatile(count);
    }

    unsafe fn alloc_q(entries: u32) -> Option<QueueMem> {
        let bytes = (entries * 64) as usize;
        let pages = (bytes + 4095) / 4096;
        let mut g = GLOBAL_ALLOCATOR.lock();
        let a = g.as_mut()?;
        let f = a.allocate_contiguous(pages)?;
        let phys = f.start_address().as_u64();
        let pm = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let virt = (phys + pm) as *mut u32;
        core::ptr::write_bytes(virt, 0, bytes / 4);
        Some(QueueMem { phys, virt })
    }

    /// Envia comando Dataset Management (TRIM/deallocate) para um range de LBAs.
    /// NVMe DSM com atributo Deallocate (bit 0 do range type).
    pub unsafe fn deallocate(&mut self, lba: u64, count: u64) -> bool {
        if count == 0 { return true; }
        let (buf_pa, buf_va) = match NvmeDriver::alloc_dma(1) { Some(v) => v, None => return false };
        core::ptr::write(buf_va as *mut u64, lba);
        core::ptr::write(buf_va.add(8) as *mut u32, count as u32);
        core::ptr::write_volatile(buf_va.add(12) as *mut u32, 0u32);

        let cmd_phys = self.io_sq.phys + 64; // slot 1
        let cmd_virt = (cmd_phys + self.pmoff) as *mut u32;
        core::ptr::write_bytes(cmd_virt, 0, 64);

        let cdw10 = (1 << 0) | ((self.qdepth & 0xFF) << 16); // deallocate + nr ranges
        core::ptr::write_volatile(cmd_virt.add(0) as *mut u32, (IO_DSM as u32) | (self.nsid << 16));
        core::ptr::write_volatile(cmd_virt.add(1) as *mut u32, cdw10);
        core::ptr::write_volatile(cmd_virt.add(6) as *mut u32, buf_pa as u32);
        core::ptr::write_volatile(cmd_virt.add(7) as *mut u32, (buf_pa >> 32) as u32);

        let sq_tdb = self.mmio.add(0x1000) as *mut u32; // SQ0 doorbell
        core::ptr::write_volatile(sq_tdb, 1);
        // Poll completion
        let cq_va = (self.io_cq.phys + self.pmoff) as *mut u32;
        for _ in 0..100000 {
            let _status = core::ptr::read_volatile(cq_va.add(3)) >> 1;
            if core::ptr::read_volatile(cq_va) == (1 << 16) { break; }
            core::hint::spin_loop();
        }
        true
    }

    pub unsafe fn alloc_dma(pages: usize) -> Option<(u64, *mut u8)> {
        let mut g = GLOBAL_ALLOCATOR.lock();
        let a = g.as_mut()?;
        let f = a.allocate_contiguous(pages)?;
        let phys = f.start_address().as_u64();
        let pm = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        Some((phys, (phys + pm) as *mut u8))
    }
}
