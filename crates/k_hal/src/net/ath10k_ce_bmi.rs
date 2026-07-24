//! ath10k CE0/CE1 + BMI exchange (QCA6174). A3 transport mínimo.

use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicU64, Ordering};

use k_nano::memory::{GLOBAL_ALLOCATOR, PHYS_MEM_OFFSET};

/// QCA6174 CE / BMI constants (Linux ath10k hw.c / ce).
const CE0_BASE: usize = 0x0003_4400;
const CE1_BASE: usize = 0x0003_4800;
const SR_BA: usize = 0x00;
const SR_SIZE: usize = 0x04;
const DR_BA: usize = 0x08;
const DR_SIZE: usize = 0x0c;
const CE_CTRL1: usize = 0x10;
const SR_WR_INDEX: usize = 0x3c;
const DST_WR_INDEX: usize = 0x40;
const CURRENT_SRRI: usize = 0x44;
const CURRENT_DRRI: usize = 0x48;

const BMI_DONE: u32 = 1;
const BMI_WRITE_MEMORY: u32 = 3;
const BMI_GET_TARGET_INFO: u32 = 8;
const BMI_LZ_STREAM_START: u32 = 13;
const BMI_LZ_DATA: u32 = 14;

const BMI_MAX_DATA: usize = 256;
const RING_N: usize = 8;
const DESC_SIZE: usize = 8;

/// QCA6174_HW_3_0_PATCH_LOAD_ADDR
pub const PATCH_LOAD_ADDR: u32 = 0x1234;

static DMA_BASE: AtomicU64 = AtomicU64::new(0);

#[repr(C)]
struct CeDesc {
    addr: u32,
    nbytes: u16,
    flags: u16,
}

pub struct CeBmi {
    bar: usize,
    /// phys of DMA region (descs + bufs)
    dma_pa: u64,
    dma_va: *mut u8,
}

impl CeBmi {
    unsafe fn r32(&self, off: usize) -> u32 {
        read_volatile((self.bar + off) as *const u32)
    }
    unsafe fn w32(&self, off: usize, v: u32) {
        write_volatile((self.bar + off) as *mut u32, v);
    }

    fn ce_r(&self, ce: usize, off: usize) -> u32 {
        unsafe { self.r32(ce + off) }
    }
    fn ce_w(&self, ce: usize, off: usize, v: u32) {
        unsafe { self.w32(ce + off, v) }
    }

    /// Aloca páginas DMA e programa CE0 (H2T) + CE1 (T2H).
    pub fn init(bar: usize) -> Result<Self, &'static str> {
        // 1 página descs + 2 páginas buffers
        let pages = 4usize;
        let (pa, va) = unsafe {
            let mut g = GLOBAL_ALLOCATOR.lock();
            let alloc = (*g).as_mut().ok_or("no_allocator")?;
            let frame = alloc.allocate_contiguous(pages).ok_or("dma_oom")?;
            let pa = frame.start_address().as_u64();
            let off = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
            (pa, (pa + off) as *mut u8)
        };
        DMA_BASE.store(pa, Ordering::Relaxed);
        unsafe {
            core::ptr::write_bytes(va, 0, pages * 4096);
        }

        let s = Self {
            bar,
            dma_pa: pa,
            dma_va: va,
        };

        // Layout: [CE0 src descs][CE1 dst descs][tx buf][rx buf]
        let ce0_desc_pa = pa;
        let ce1_desc_pa = pa + (RING_N * DESC_SIZE) as u64;
        // CTRL1: DMAX = src_sz_max (256)
        s.ce_w(CE0_BASE, CE_CTRL1, 256);
        s.ce_w(CE0_BASE, SR_BA, ce0_desc_pa as u32);
        s.ce_w(CE0_BASE, SR_SIZE, RING_N as u32);
        s.ce_w(CE0_BASE, SR_WR_INDEX, 0);

        s.ce_w(CE1_BASE, CE_CTRL1, 256);
        s.ce_w(CE1_BASE, DR_BA, ce1_desc_pa as u32);
        s.ce_w(CE1_BASE, DR_SIZE, RING_N as u32);
        s.ce_w(CE1_BASE, DST_WR_INDEX, 0);

        k_nano::slog_hal!(
            "ATH10K",
            "info",
            "step=ce status=OK dma_pa={:#x}",
            pa
        );
        Ok(s)
    }

    fn tx_buf_pa(&self) -> u64 {
        self.dma_pa + 2048
    }
    fn rx_buf_pa(&self) -> u64 {
        self.dma_pa + 4096
    }
    fn tx_buf_va(&self) -> *mut u8 {
        unsafe { self.dma_va.add(2048) }
    }
    fn rx_buf_va(&self) -> *mut u8 {
        unsafe { self.dma_va.add(4096) }
    }

    fn pause(&self) {
        for _ in 0..2000 {
            core::hint::spin_loop();
        }
    }

    /// CE1 RX-only poll (HTC READY / WMI events pós-BMI). Sem TX.
    pub fn recv_poll(&mut self, resp: &mut [u8]) -> Result<usize, &'static str> {
        let rx_pa = self.rx_buf_pa();
        unsafe {
            core::ptr::write_bytes(self.rx_buf_va(), 0, resp.len().min(BMI_MAX_DATA));
        }
        let ce1_desc = (self.dma_pa + (RING_N * DESC_SIZE) as u64) as usize;
        let d1 = ce1_desc as *mut CeDesc;
        unsafe {
            (*d1).addr = rx_pa as u32;
            (*d1).nbytes = 0;
            (*d1).flags = 0;
        }
        self.ce_w(CE1_BASE, DST_WR_INDEX, 1);

        let mut waited = 0u32;
        loop {
            let drri = self.ce_r(CE1_BASE, CURRENT_DRRI);
            if drri >= 1 {
                break;
            }
            self.pause();
            waited += 1;
            // HTC ready pode demorar mais que BMI — timeout generoso mas finito
            if waited > 80_000 {
                self.ce_w(CE1_BASE, DST_WR_INDEX, 0);
                return Err("ce_rx_timeout");
            }
        }

        let n = unsafe { (*d1).nbytes as usize }.min(resp.len());
        unsafe {
            core::ptr::copy_nonoverlapping(self.rx_buf_va(), resp.as_mut_ptr(), n);
        }
        self.ce_w(CE1_BASE, DST_WR_INDEX, 0);
        self.ce_w(
            CE1_BASE,
            DR_BA,
            (self.dma_pa + (RING_N * DESC_SIZE) as u64) as u32,
        );
        Ok(n)
    }

    /// BMI exchange via CE0 send + CE1 recv (poll).
    pub fn exchange(
        &mut self,
        req: &[u8],
        resp: &mut [u8],
    ) -> Result<usize, &'static str> {
        if req.len() > BMI_MAX_DATA {
            return Err("bmi_req_too_big");
        }
        let tx_pa = self.tx_buf_pa();
        let rx_pa = self.rx_buf_pa();
        unsafe {
            core::ptr::copy_nonoverlapping(req.as_ptr(), self.tx_buf_va(), req.len());
            core::ptr::write_bytes(self.rx_buf_va(), 0, resp.len().min(BMI_MAX_DATA));
        }

        // Post RX dest desc idx0
        let ce1_desc = (self.dma_pa + (RING_N * DESC_SIZE) as u64) as usize;
        let d1 = ce1_desc as *mut CeDesc;
        unsafe {
            (*d1).addr = rx_pa as u32;
            (*d1).nbytes = 0;
            (*d1).flags = 0;
        }
        self.ce_w(CE1_BASE, DST_WR_INDEX, 1);

        // Post TX src desc idx0
        let ce0_desc = self.dma_pa as usize;
        let d0 = ce0_desc as *mut CeDesc;
        unsafe {
            (*d0).addr = tx_pa as u32;
            (*d0).nbytes = req.len() as u16;
            (*d0).flags = 0;
        }
        self.ce_w(CE0_BASE, SR_WR_INDEX, 1);

        // Poll TX complete (SRRI advances)
        let mut waited = 0u32;
        loop {
            let srri = self.ce_r(CE0_BASE, CURRENT_SRRI);
            if srri >= 1 {
                break;
            }
            self.pause();
            waited += 1;
            if waited > 50_000 {
                return Err("bmi_tx_timeout");
            }
        }

        if resp.is_empty() {
            // Reset indices for next xfer (simple single-slot)
            self.ce_w(CE0_BASE, SR_WR_INDEX, 0);
            self.ce_w(CE1_BASE, DST_WR_INDEX, 0);
            return Ok(0);
        }

        waited = 0;
        loop {
            let drri = self.ce_r(CE1_BASE, CURRENT_DRRI);
            if drri >= 1 {
                break;
            }
            self.pause();
            waited += 1;
            if waited > 50_000 {
                return Err("bmi_rx_timeout");
            }
        }

        let n = unsafe { (*d1).nbytes as usize }.min(resp.len());
        unsafe {
            core::ptr::copy_nonoverlapping(self.rx_buf_va(), resp.as_mut_ptr(), n);
        }
        self.ce_w(CE0_BASE, SR_WR_INDEX, 0);
        self.ce_w(CE1_BASE, DST_WR_INDEX, 0);
        // Clear HW indices by rewriting ring bases (slot reuse)
        self.ce_w(CE0_BASE, SR_BA, self.dma_pa as u32);
        self.ce_w(CE1_BASE, DR_BA, (self.dma_pa + (RING_N * DESC_SIZE) as u64) as u32);
        Ok(n)
    }

    pub fn get_target_info(&mut self) -> Result<(u32, u32), &'static str> {
        let mut cmd = [0u8; 4];
        cmd[0..4].copy_from_slice(&BMI_GET_TARGET_INFO.to_le_bytes());
        let mut resp = [0u8; 12];
        let n = self.exchange(&cmd, &mut resp)?;
        if n < 8 {
            return Err("bmi_target_short");
        }
        let version = u32::from_le_bytes(resp[0..4].try_into().unwrap());
        let typ = u32::from_le_bytes(resp[4..8].try_into().unwrap());
        k_nano::slog_hal!(
            "ATH10K",
            "info",
            "step=bmi_target version={:#x} type={:#x}",
            version,
            typ
        );
        Ok((version, typ))
    }

    pub fn done(&mut self) -> Result<(), &'static str> {
        let mut cmd = [0u8; 4];
        cmd[0..4].copy_from_slice(&BMI_DONE.to_le_bytes());
        let mut empty = [];
        self.exchange(&cmd, &mut empty)?;
        k_nano::slog_hal!("ATH10K", "info", "step=bmi_done status=OK");
        Ok(())
    }

    /// BMI_LZ fast download (Linux ath10k_bmi_fast_download).
    pub fn lz_download(&mut self, addr: u32, data: &[u8]) -> Result<(), &'static str> {
        // LZ_STREAM_START
        let mut cmd = [0u8; 8];
        cmd[0..4].copy_from_slice(&BMI_LZ_STREAM_START.to_le_bytes());
        cmd[4..8].copy_from_slice(&addr.to_le_bytes());
        let mut empty = [];
        self.exchange(&cmd, &mut empty)?;

        let hdr = 8usize; // id + len
        let chunk_max = BMI_MAX_DATA - hdr;
        let mut off = 0usize;
        while off < data.len() {
            let mut n = core::cmp::min(chunk_max, data.len() - off);
            n &= !3; // word align
            if n == 0 {
                // trailer < 4 bytes
                let mut pad = [0u8; 4];
                let rem = data.len() - off;
                pad[..rem].copy_from_slice(&data[off..]);
                let mut c = [0u8; BMI_MAX_DATA];
                c[0..4].copy_from_slice(&BMI_LZ_DATA.to_le_bytes());
                c[4..8].copy_from_slice(&4u32.to_le_bytes());
                c[8..12].copy_from_slice(&pad);
                self.exchange(&c[..12], &mut empty)?;
                break;
            }
            let mut c = [0u8; BMI_MAX_DATA];
            c[0..4].copy_from_slice(&BMI_LZ_DATA.to_le_bytes());
            c[4..8].copy_from_slice(&(n as u32).to_le_bytes());
            c[8..8 + n].copy_from_slice(&data[off..off + n]);
            self.exchange(&c[..8 + n], &mut empty)?;
            off += n;
        }

        // Flush stream
        let mut flush = [0u8; 8];
        flush[0..4].copy_from_slice(&BMI_LZ_STREAM_START.to_le_bytes());
        flush[4..8].copy_from_slice(&0u32.to_le_bytes());
        self.exchange(&flush, &mut empty)?;

        k_nano::slog_hal!(
            "ATH10K",
            "info",
            "step=bmi_lz addr={:#x} bytes={}",
            addr,
            data.len()
        );
        Ok(())
    }

    /// BMI_WRITE_MEMORY chunks (board.bin).
    pub fn write_memory(&mut self, mut addr: u32, data: &[u8]) -> Result<(), &'static str> {
        let hdr = 12usize; // id + addr + len
        let chunk_max = (BMI_MAX_DATA - hdr) & !3;
        let mut off = 0usize;
        let mut empty = [];
        while off < data.len() {
            let mut n = core::cmp::min(chunk_max, data.len() - off);
            n = (n + 3) & !3;
            let copy_n = core::cmp::min(n, data.len() - off);
            let mut c = [0u8; BMI_MAX_DATA];
            c[0..4].copy_from_slice(&BMI_WRITE_MEMORY.to_le_bytes());
            c[4..8].copy_from_slice(&addr.to_le_bytes());
            c[8..12].copy_from_slice(&(n as u32).to_le_bytes());
            unsafe {
                core::ptr::write_bytes(c.as_mut_ptr().add(12), 0, n);
            }
            c[12..12 + copy_n].copy_from_slice(&data[off..off + copy_n]);
            self.exchange(&c[..12 + n], &mut empty)?;
            addr = addr.wrapping_add(n as u32);
            off += copy_n;
        }
        Ok(())
    }
}
