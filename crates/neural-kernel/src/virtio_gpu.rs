//! Minimal VirtIO-GPU driver — bare-metal, PCI legacy + modern transport.
//! 
//! Detecta GPU (1AF4:1045 transitional ou 1AF4:1050 modern), init control queue,
//! GET_DISPLAY_INFO → CREATE_2D → ATTACH_BACKING → SET_SCANOUT → framebuffer.
//!
//! ⚠️ Modern VirtIO (0x1050) não usa BARs padrão — o MMIO base está em PCI capability
//! vendor-specific (VirtIO PCI capability list). Nosso scanner lê BAR0=0x00 para este
//! dispositivo. Solução: implementar parser de capabilities PCI (VirtIO cap 0x09).
//! Legacy/transitional (0x1045) tem I/O BAR padrão.
//!
//! Por enquanto, o driver detecta mas init falha em QEMU moderno.
//! QEMU: `-device virtio-gpu-pci` cria device 0x1050 (modern).
//! Alternativa: `-device virtio-gpu-pci,disable-modern=true` mas BAR continua MMIO.
//!
//! Status: DETECTADO ✅ → INIT ⏳ (requer parser PCI cap)

extern crate alloc;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;
use x86_64::instructions::port::Port;
use crate::memory::{GLOBAL_ALLOCATOR, PHYS_MEM_OFFSET};
use crate::{serial_println};

pub const VIRTIO_GPU_TRANS: u16 = 0x1045; // transitional (legacy I/O + modern MMIO)
pub const VIRTIO_GPU_MODERN: u16 = 0x1050; // modern only (MMIO)

const QUEUE_NUM: u16 = 32;

// --- Register constraints (diferentes para I/O legacy vs MMIO modern) ---
#[derive(Clone, Copy)]
struct RegOffsets {
    df: u16,  // device features
    gf: u16,  // guest features
    qa: u16,  // queue address
    qs: u16,  // queue size
    qsl: u16, // queue select
    qn: u16,  // queue notify
    st: u16,  // device status
}

const LEGACY: RegOffsets = RegOffsets { df: 0x00, gf: 0x04, qa: 0x08, qs: 0x0C, qsl: 0x0E, qn: 0x10, st: 0x12 };
const MODERN: RegOffsets = RegOffsets { df: 0x000, gf: 0x008, qa: 0x010, qs: 0x014, qsl: 0x018, qn: 0x01C, st: 0x020 };

/// Register access: I/O ports (legacy) ou MMIO (modern)
struct Regs {
    base: u16,       // I/O port base
    mmio_base: u64,  // MMIO physical base
    is_mmio: bool,
    ro: RegOffsets,
}
impl Regs {
    fn new(io_base: u16, mmio_base: u64, is_mmio: bool) -> Self {
        let ro = if is_mmio { MODERN } else { LEGACY };
        Regs { base: io_base, mmio_base, is_mmio, ro }
    }

    fn mmio_ptr(&self, reg: u16) -> *mut u32 {
        let off = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        (self.mmio_base + off + reg as u64) as *mut u32
    }

    fn r8(&self, reg: u16) -> u8 {
        if self.is_mmio { unsafe { (self.mmio_ptr(self.ro.st) as *mut u8).read_volatile() } }
        else { unsafe { Port::new(self.base + reg).read() } }
    }
    fn w8(&self, reg: u16, v: u8) {
        if self.is_mmio { unsafe { (self.mmio_ptr(self.ro.st) as *mut u8).write_volatile(v); } }
        else { unsafe { Port::new(self.base + reg).write(v); } }
    }
    fn r32(&self, reg: u16) -> u32 {
        if self.is_mmio { unsafe { self.mmio_ptr(reg).read_volatile() } }
        else { unsafe { Port::new(self.base + reg).read() } }
    }
    fn w32(&self, reg: u16, v: u32) {
        if self.is_mmio { unsafe { self.mmio_ptr(reg).write_volatile(v); } }
        else { unsafe { Port::new(self.base + reg).write(v); } }
    }
    fn w16(&self, reg: u16, v: u16) {
        if self.is_mmio { unsafe { (self.mmio_ptr(reg) as *mut u16).write_volatile(v); } }
        else { unsafe { Port::new(self.base + reg).write(v); } }
    }

    fn add_status(&self, b: u8) { self.w8(self.ro.st, self.r8(self.ro.st) | b); }

    /// Modern MMIO requires writing the register select first for 64-bit features
    fn write_feat_hi(&self, v: u32) {
        if self.is_mmio {
            // Modern: select high 32 bits, then write
            unsafe { (self.mmio_ptr(0x00C)).write_volatile(v); } // GuestFeaturesHigh at 0x00C
        } else {
            // Legacy: write at offset +4
            self.w32(self.ro.gf + 4, v);
        }
    }
    fn read_feat_hi(&self) -> u32 {
        if self.is_mmio {
            // Modern: select high, read
            unsafe { (self.mmio_ptr(0x004)).read_volatile() } // DeviceFeaturesHigh at 0x004
        } else {
            self.r32(self.ro.df + 4)
        }
    }
}

unsafe fn alloc_pages(n: usize) -> Option<(u64, *mut u8)> {
    use x86_64::structures::paging::FrameAllocator;
    let mut g = GLOBAL_ALLOCATOR.lock(); let a = (*g).as_mut()?;
    let f = a.allocate_contiguous(n)?; let pa = f.start_address().as_u64();
    Some((pa, (pa + PHYS_MEM_OFFSET.load(Ordering::Relaxed)) as *mut u8))
}

unsafe fn setup_q(io: &Regs, idx: u16, sz: u16) -> Option<u64> {
    io.w16(io.ro.qsl, idx);
    // MMIO: queue size at ro.qs, legacy: same
    if io.r32(io.ro.qs) < sz as u32 { return None; }
    let (pa, va) = alloc_pages(3)?; core::ptr::write_bytes(va, 0, 12288);
    io.w32(io.ro.qa, (pa >> 12) as u32);
    Some(pa)
}

pub struct GpuDevice {
    pub fb_addr: u64, pub fb_width: u32, pub fb_height: u32, pub fb_stride: u32,
    pub present: bool,
}

impl GpuDevice {
    pub fn new(dev: &crate::pci::PciDevice, phys_mem_offset: u64) -> Option<Self> {
        serial_println!("[VGPU] BAR0={:#x} BAR1={:#x}", dev.bar0, dev.bar1);
        // Detect BAR type com verificação de 32 vs 64-bit
        let (io_base, mmio_base, is_mmio) = if dev.bar0 & 1 == 1 {
            // Legacy I/O BAR
            ((dev.bar0 & !0xFF) as u16, 0u64, false)
        } else {
            let btype = (dev.bar0 >> 1) & 3; // bits 2-1: 00=32bit, 10=64bit
            let base = if btype == 2 {
                // 64-bit MMIO: BAR0 (low) + BAR1 (high)
                let low = dev.bar0 as u64 & !0xF;
                let high = (dev.bar1 as u64) << 32;
                low | high
            } else {
                // 32-bit MMIO
                (dev.bar0 & !0xF) as u64
            };
            if base == 0 { return None; }
            // Verifica se address cabe no mapeamento (fitness check)
            let test_virt = base.wrapping_add(phys_mem_offset);
            if test_virt >> 47 != 0 && test_virt >> 47 != 0x1FFFF {
                serial_println!("[VGPU] BAR address {:x} não mapeável (test={:x})", base, test_virt);
                return None;
            }
            (0u16, base, true)
        };

        // Map MMIO BAR as uncacheable
        if is_mmio && mmio_base > 0 {
            unsafe { crate::apic::set_page_uc(mmio_base, phys_mem_offset); }
            serial_println!("[VGPU] MMIO BAR {:x} mapeado UC", mmio_base);
        }

        // Map MMIO BAR as uncacheable BEFORE accessing
        if is_mmio && mmio_base > 0 {
            unsafe { crate::apic::set_page_uc(mmio_base, phys_mem_offset); }
            serial_println!("[VGPU] MMIO BAR mapeado UC em {:x}", mmio_base);
        }

        let io = Regs::new(io_base, mmio_base, is_mmio);

        unsafe {
            // Reset
            io.w8(io.ro.st, 0);
            for _ in 0..1000 { core::hint::spin_loop(); if io.r8(io.ro.st) == 0 { break; } }

            serial_println!("[VGPU] status={:x} feat={:#x}", io.r8(io.ro.st), io.r32(io.ro.df));

            // ACK + DRIVER
            io.add_status(1); io.add_status(2);

            // Features (64-bit)
            let feat_low = io.r32(io.ro.df);
            let feat_high = io.read_feat_hi();
            io.w32(io.ro.gf, feat_low);
            io.write_feat_hi(feat_high);

            // FEATURES_OK
            io.add_status(8);
            serial_println!("[VGPU] status after FEATURES_OK={:x}", io.r8(io.ro.st));
            if io.r8(io.ro.st) & 8 == 0 {
                serial_println!("[VGPU] Features falhou");
                return None;
            }

            // Control queue
            let qpa = match setup_q(&io, 0, QUEUE_NUM) {
                Some(q) => q,
                None => { serial_println!("[VGPU] Queue setup falhou"); return None; }
            };

            // DRIVER_OK
            io.add_status(4);
            serial_println!("[VGPU] status after DRIVER_OK={:x} qs={:x}", io.r8(io.ro.st), io.r32(io.ro.qs));

            // GET_DISPLAY_INFO via control queue
            let off = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
            let (cpa, cva) = alloc_pages(2)?;
            core::ptr::write_bytes(cva, 0, 8192);

            // Write command: GET_DISPLAY_INFO
            *(cpa as *mut u32) = 0x0100u32;

            // Setup descriptor in control queue
            // Queue memory layout: page0=desc, page1=avail, page2=used
            let d = (qpa + off) as *mut u64;              // desc table
            let a = (qpa + off + 4096) as *mut u16;       // avail ring starts at +4096

            // desc[0]: command buffer (device reads)
            *d = cpa;                             // addr
            *((d as *mut u32).add(2)) = 24;       // len
            // desc[1]: response buffer (device writes) at cpa+0x100
            *d.add(2) = cpa + 0x100;              // addr
            *((d as *mut u32).add(6)) = 128;       // len
            *((d as *mut u16).add(6)) = 2;         // flags = WRITE

            // Submit to avail ring
            *a.add(4) = 0;  // ring[0] = 0
            core::sync::atomic::fence(Ordering::SeqCst);
            *a = 1;         // idx = 1

            // Notify queue 0
            io.w16(io.ro.qn, 0);

            // Poll for completion
            for _ in 0..1000000 {
                core::hint::spin_loop();
                let used_idx = *((qpa + off + 8192) as *const u16);
                if used_idx > 0 { break; }
            }

            let resp_type = *((cpa + 0x100 + off) as *const u32);
            if resp_type != 0x1100 {
                serial_println!("[VGPU] GET_DISPLAY resp={:#x}", resp_type);
                // Use default resolution
                let (fw, fh) = (1024u32, 768u32);
                return init_framebuffer(&io, qpa, cpa, off, fw, fh);
            }

            let width = *((cpa + 0x100 + off + 24) as *const u32);
            let height = *((cpa + 0x100 + off + 28) as *const u32);
            let fb_w = if width > 0 && width <= 8192 { width } else { 1024 };
            let fb_h = if height > 0 && height <= 8192 { height } else { 768 };

            serial_println!("[VGPU] Display {}x{}", fb_w, fb_h);

            init_framebuffer(&io, qpa, cpa, off, fb_w, fb_h)
        }
    }
}

unsafe fn init_framebuffer(io: &Regs, qpa: u64, cpa: u64, off: u64, width: u32, height: u32) -> Option<GpuDevice> {
    let fb_sz = (width * height * 4) as usize;
    let fb_pg = (fb_sz + 4095) / 4096;
    let (fb_pa, fb_va) = alloc_pages(fb_pg)?;
    core::ptr::write_bytes(fb_va, 0, fb_pg * 4096);

    // RESOURCE_CREATE_2D: type=0x0101, fields: res_id, format, w, h
    *(cpa as *mut u32) = 0x0101;
    *((cpa as *mut u32).add(6)) = 1;     // resource_id
    *((cpa as *mut u32).add(7)) = 1;     // format B8G8R8A8
    *((cpa as *mut u32).add(8)) = width;
    *((cpa as *mut u32).add(9)) = height;
    submit_q(io, qpa, cpa, 40, off);
    if !poll_q(qpa, off) { serial_println!("[VGPU] CREATE fail"); return None; }

    // RESOURCE_ATTACH_BACKING: type=0x0102
    *(cpa as *mut u32) = 0x0102;
    *((cpa as *mut u32).add(6)) = 1;              // resource_id
    *((cpa as *mut u32).add(7)) = fb_pg as u32;   // nr_entries
    for i in 0..fb_pg {
        let e = (cpa + 32 + i as u64 * 16) as *mut u64;
        *e = fb_pa + (i as u64 * 4096);
        let lenp = (cpa + 32 + i as u64 * 16 + 8) as *mut u32;
        *lenp = if i == fb_pg - 1 { (fb_sz % 4096) as u32 } else { 4096 };
    }
    submit_q(io, qpa, cpa, 32 + fb_pg * 16, off);
    if !poll_q(qpa, off) { serial_println!("[VGPU] ATTACH fail"); return None; }

    // SET_SCANOUT: type=0x0103, fields: res_id, scanout_id, rect{x,y,w,h}
    *(cpa as *mut u32) = 0x0103;
    *((cpa as *mut u32).add(6)) = 1;     // resource_id
    *((cpa as *mut u32).add(7)) = 0;     // scanout_id = 0
    *((cpa as *mut u32).add(8)) = 0;     // rect.x
    *((cpa as *mut u32).add(9)) = 0;     // rect.y
    *((cpa as *mut u32).add(10)) = width;
    *((cpa as *mut u32).add(11)) = height;
    submit_q(io, qpa, cpa, 48, off);
    poll_q(qpa, off);

    serial_println!("[VGPU] VirtIO-GPU OK: {}x{} fb={:#x}", width, height, fb_pa);

    Some(GpuDevice { fb_addr: fb_pa, fb_width: width, fb_height: height, fb_stride: width*4, present: true })
}

unsafe fn submit_q(io: &Regs, qpa: u64, cpa: u64, cmd_len: usize, off: u64) {
    let d = (qpa + off) as *mut u64;
    let a = (qpa + off + 4096) as *mut u16;
    *d = cpa;                                // desc[0].addr = cmd
    *((d as *mut u32).add(2)) = cmd_len as u32; // len
    *d.add(2) = cpa + 0x100;                 // desc[1].addr = resp
    *((d as *mut u32).add(6)) = 64;           // resp len
    *((d as *mut u16).add(6)) = 2;            // WRITE
    *a.add(4) = 0;
    core::sync::atomic::fence(Ordering::SeqCst);
    *a = 1;  // avail.idx = 1
    io.w16(io.ro.qn, 0);
}

unsafe fn poll_q(qpa: u64, off: u64) -> bool {
    for _ in 0..2000000 {
        if *((qpa + 8192 + off) as *const u16) > 0 { return true; }
        core::hint::spin_loop();
    }
    false
}

/// Init público — chamado pelo GpuDriverAgent
pub unsafe fn init_driver_virtio_gpu() -> bool {
    let phys_off = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    let devices = crate::pci::scan_pci();
    for dev in &devices {
        if dev.vendor_id == 0x1AF4 &&
           (dev.device_id == VIRTIO_GPU_TRANS || dev.device_id == VIRTIO_GPU_MODERN) {
            serial_println!("[VGPU] Detectado: {:02x}:{:02x}.{:02x}",
                dev.bus, dev.device, dev.function);
            if let Some(gpu) = GpuDevice::new(dev, phys_off) {
                *crate::display::fb::GPU.lock() = Some(gpu);
                return true;
            }
            serial_println!("[VGPU] Init falhou");
        }
    }
    false
}
