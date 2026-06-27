//! Minimal VirtIO-GPU driver
extern crate alloc;
use x86_64::instructions::port::Port;
use crate::memory::{GLOBAL_ALLOCATOR, PHYS_MEM_OFFSET};
use crate::pci::PciDevice;
use crate::{serial_println};

pub const VIRTIO_GPU_TRANSITIONAL: u16 = 0x1045;
pub const VIRTIO_GPU_MODERN: u16 = 0x1050;

const QUEUE_NUM: u16 = 32;
const REG_QSIZE: u16 = 0x0C;
const REG_QSEL: u16 = 0x0E;
const REG_QADDR: u16 = 0x08;
const REG_QNOTIFY: u16 = 0x10;
const REG_STATUS: u16 = 0x12;
const REG_DFEAT: u16 = 0x00;
const REG_GFEAT: u16 = 0x04;
const S_ACK: u8 = 1; const S_DRV: u8 = 2; const S_OK: u8 = 4; const S_FOK: u8 = 8;

const CMD_GDISP: u32 = 0x0100;
const CMD_RCREAT: u32 = 0x0101;
const CMD_RATTA: u32 = 0x0102;
const CMD_SCAN: u32 = 0x0103;
const CMD_FLUSH: u32 = 0x0104;
const RESP_OK: u32 = 0x1100;
const FMT_BGRA: u32 = 1;

struct Regs { base: u16, mmio: u64, use_mmio: bool }
impl Regs {
    fn r8(&self, r: u16) -> u8 { unsafe { if self.use_mmio { (self.va(r) as *mut u8).read_volatile() } else { Port::new(self.base+r).read() } } }
    fn w8(&self, r: u16, v: u8) { unsafe { if self.use_mmio { (self.va(r) as *mut u8).write_volatile(v) } else { Port::new(self.base+r).write(v) } } }
    fn r32(&self, r: u16) -> u32 { unsafe { if self.use_mmio { (self.va(r) as *mut u32).read_volatile() } else { Port::new(self.base+r).read() } } }
    fn w32(&self, r: u16, v: u32) { unsafe { if self.use_mmio { (self.va(r) as *mut u32).write_volatile(v) } else { Port::new(self.base+r).write(v) } } }
    fn w16(&self, r: u16, v: u16) { unsafe { if self.use_mmio { (self.va(r) as *mut u16).write_volatile(v) } else { Port::new(self.base+r).write(v) } } }
    fn va(&self, r: u16) -> u64 { self.mmio + PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed) + r as u64 }
    fn add_status(&self, b: u8) { self.w8(REG_STATUS, self.r8(REG_STATUS)|b); }
}

unsafe fn alloc_pages(n: usize) -> Option<(u64, *mut u8)> {
    use x86_64::structures::paging::FrameAllocator;
    let mut g = GLOBAL_ALLOCATOR.lock(); let a = (*g).as_mut()?;
    let f = a.allocate_contiguous(n)?; let pa = f.start_address().as_u64();
    Some((pa, (pa + PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed)) as *mut u8))
}

unsafe fn setup_q(io: &Regs, idx: u16, sz: u16) -> Option<u64> {
    io.w16(REG_QSEL, idx); if io.r32(REG_QSIZE) < sz as u32 { return None; }
    let (pa, va) = alloc_pages(3)?; core::ptr::write_bytes(va, 0, 12288);
    io.w32(REG_QADDR, (pa >> 12) as u32); Some(pa)
}

fn resp_ok(io: &Regs, qpa: u64) -> bool {
    unsafe {
        let off = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let used = *((qpa + 8192 + off) as *const u16).wrapping_add(0x100); // used.idx
        used > 0
    }
}

pub struct GpuDevice {
    pub fb_addr: u64, pub fb_width: u32, pub fb_height: u32, pub fb_stride: u32,
    pub present: bool,
}

impl GpuDevice {
    pub fn new(dev: &PciDevice) -> Option<Self> {
        let (io_base, mmio_base, is_mmio) = if dev.bar0 & 1 == 1 {
            ((dev.bar0 & !0xFF) as u16, 0u64, false)
        } else {
            let low = dev.bar0 as u64 & !0xF;
            let high = (dev.bar1 as u64) << 32;
            (0u16, low | high, true)
        };
        if io_base == 0 && mmio_base == 0 { return None; }
        let io = Regs { base: io_base, mmio: mmio_base, use_mmio: is_mmio };

        unsafe {
            io.w8(REG_STATUS, 0); while io.r8(REG_STATUS) != 0 { core::hint::spin_loop(); }
            io.add_status(S_ACK); io.add_status(S_DRV);
            let feat = io.r32(REG_DFEAT) as u64 | ((io.r32(REG_DFEAT+4) as u64)<<32);
            io.w32(REG_GFEAT, feat as u32); io.w32(REG_GFEAT+4, (feat>>32) as u32);
            io.add_status(S_FOK);
            if io.r8(REG_STATUS) & S_FOK == 0 { serial_println!("[VGPU] feat fail"); return None; }
            let qpa = setup_q(&io, 0, QUEUE_NUM)?;
            io.add_status(S_OK);

            // GET_DISPLAY_INFO
            let off = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
            let (cpa, _cva) = alloc_pages(1)?; core::ptr::write_bytes(_cva, 0, 4096);
            *(cpa as *mut u32) = CMD_GDISP;
            let d = (qpa + off) as *mut u64;
            // desc[0]: addr=cmd, len=24, flags=0 (device read)
            *d = cpa;
            *((d as *mut u64).add(1) as *mut u32) = 24;
            // desc[1]: addr=resp, len=128, flags=2 (WRITE)
            *((d as *mut u64).add(2)) = cpa + 0x100;
            *((d as *mut u64).add(3) as *mut u32) = 128;
            *(((d as *mut u64).add(3) as *mut u16).add(2)) = 2;
            let a = ((qpa + 4096) + off) as *mut u16;
            *a.add(4) = 0; *a.add(0) = 0;   // avail.ring[0]=0, .idx=0
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
            *a = 1; // avail.idx = 1
            io.w16(REG_QNOTIFY, 0);
            for _ in 0..500000 { core::hint::spin_loop(); }
            let used_idx = *((qpa + 8192 + off) as *const u16);
            if used_idx == 0 { serial_println!("[VGPU] GET_DISPLAY timeout"); return None; }
            let width = *((cpa + 0x100 + off + 24) as *const u32);
            let height = *((cpa + 0x100 + off + 28) as *const u32);
            serial_println!("[VGPU] Display {}x{}", width, height);

            // CREATE_2D + ATTACH_BACKING + SET_SCANOUT
            let fb_w = if width > 0 { width } else { 1024 };
            let fb_h = if height > 0 { height } else { 768 };
            let fb_sz = (fb_w * fb_h * 4) as usize;
            let fb_pg = (fb_sz + 4095) / 4096;
            let (fb_pa, fb_va) = alloc_pages(fb_pg)?; let _ = fb_va;
            core::ptr::write_bytes(fb_va, 0, fb_pg * 4096);

            // Resource ID 1
            *(cpa as *mut u32) = CMD_RCREAT; // type
            *((cpa as *mut u32).add(6)) = 1; // resource_id
            *((cpa as *mut u32).add(7)) = FMT_BGRA;
            *((cpa as *mut u32).add(8)) = fb_w;
            *((cpa as *mut u32).add(9)) = fb_h;
            submit(&io, qpa, cpa, 40, off);
            spin_until(qpa, off);

            // ATTACH_BACKING
            *(cpa as *mut u32) = CMD_RATTA;
            *((cpa as *mut u32).add(6)) = 1; // resource_id
            *((cpa as *mut u32).add(7)) = fb_pg as u32; // nr_entries
            for i in 0..fb_pg {
                let e = (cpa + 32 + i as u64 * 16) as *mut u64;
                *e = fb_pa + (i as u64 * 4096);
                let len_ptr = (cpa + 32 + i as u64 * 16 + 8) as *mut u32;
                *len_ptr = if i == fb_pg-1 { (fb_sz % 4096) as u32 } else { 4096 };
            }
            submit(&io, qpa, cpa, 32 + fb_pg * 16, off);
            if !spin_until(qpa, off) { serial_println!("[VGPU] ATTACH fail"); return None; }

            // SET_SCANOUT
            *(cpa as *mut u32) = CMD_SCAN;
            *((cpa as *mut u32).add(6)) = 1;  // resource_id
            *((cpa as *mut u32).add(7)) = 0;  // scanout_id
            *((cpa as *mut u32).add(8)) = 0;  // rect.x
            *((cpa as *mut u32).add(9)) = 0;  // rect.y
            *((cpa as *mut u32).add(10)) = fb_w;
            *((cpa as *mut u32).add(11)) = fb_h;
            submit(&io, qpa, cpa, 48, off);
            spin_until(qpa, off);

            serial_println!("[VGPU] VirtIO-GPU OK: {}x{} fb={:#x}", fb_w, fb_h, fb_pa);
            Some(GpuDevice { fb_addr: fb_pa, fb_width: fb_w, fb_height: fb_h, fb_stride: fb_w*4, present: true })
        }
    }
}

unsafe fn submit(io: &Regs, qpa: u64, cpa: u64, cmd_len: usize, off: u64) {
    let d = (qpa + off) as *mut u64;
    *d = cpa; *(d.add(1) as *mut u32) = cmd_len as u32; *(d.add(1) as *mut u16).add(2) = 0;
    *(d.add(2)) = cpa + 0x100; *(d.add(3) as *mut u32) = 64; *(d.add(3) as *mut u16).add(2) = 2;
    let a = (qpa + 4096 + off) as *mut u16; *a.add(4) = 0;
    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst); *a = 1;
    io.w16(REG_QNOTIFY, 0);
}

unsafe fn spin_until(qpa: u64, off: u64) -> bool {
    for _ in 0..500000 {
        if *((qpa + 8192 + off) as *const u16) > 0 { return true; }
        core::hint::spin_loop();
    }
    false
}

pub unsafe fn init_driver_virtio_gpu() -> bool {
    let devices = crate::pci::scan_pci();
    for dev in &devices {
        if dev.vendor_id == 0x1AF4 &&
           (dev.device_id == VIRTIO_GPU_TRANSITIONAL || dev.device_id == VIRTIO_GPU_MODERN) {
            serial_println!("[VGPU] Detectado: {:02x}:{:02x}.{:02x}", dev.bus, dev.device, dev.function);
            if let Some(gpu) = GpuDevice::new(dev) {
                *crate::display::fb::GPU.lock() = Some(gpu);
                return true;
            }
        }
    }
    false
}
