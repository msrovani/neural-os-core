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
use crate::display::fb::GpuDevice;
use crate::{serial_println};

pub const VIRTIO_GPU_TRANS: u16 = 0x1045; // transitional (legacy I/O + modern MMIO)
pub const VIRTIO_GPU_MODERN: u16 = 0x1050; // modern only (MMIO)

const QUEUE_NUM: u16 = 32;

// --- Register constraints (I/O legacy vs MMIO modern) ---
#[derive(Clone, Copy)]
struct RegOffsets {
    df: u16, gf: u16, qa: u16, qs: u16, qsl: u16, qn: u16, st: u16, qe: u16,
}

const LEGACY: RegOffsets = RegOffsets {
    df: 0x00, gf: 0x04, qa: 0x08, qs: 0x0C, qsl: 0x0E, qn: 0x10, st: 0x12, qe: 0,
};

// Modern VirtIO 1.1 §4.1.4.3: struct virtio_pci_common_cfg offsets
const MODERN: RegOffsets = RegOffsets {
    df: 0x04,  // device_feature (R, after DeviceFeatureSel=0x00)
    gf: 0x0C,  // driver_feature (W, after DriverFeatureSel=0x08)
    qa: 0x20,  // queue_desc (64-bit)
    qs: 0x18,  // queue_size
    qsl: 0x16, // queue_select
    qn: 0x0,   // notify not in common cfg
    st: 0x14,  // device_status
    qe: 0x1C,  // queue_enable (W: 1 = enable)
};

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
            // Modern: write 1 to DriverFeatureSel (0x08), then write to DriverFeature (0x0C)
            unsafe { (self.mmio_ptr(0x08)).write_volatile(1u32); }
            unsafe { (self.mmio_ptr(0x0C)).write_volatile(v); }
            unsafe { (self.mmio_ptr(0x08)).write_volatile(0u32); } // restore
        } else {
            self.w32(self.ro.gf + 4, v);
        }
    }
    fn read_feat_hi(&self) -> u32 {
        if self.is_mmio {
            // Modern: write 1 to DeviceFeatureSel (0x00), then read from DeviceFeature (0x04)
            unsafe { (self.mmio_ptr(0x00)).write_volatile(1u32); }
            let v = unsafe { (self.mmio_ptr(0x04)).read_volatile() };
            unsafe { (self.mmio_ptr(0x00)).write_volatile(0u32); } // restore
            v
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
    if io.r32(io.ro.qs) < sz as u32 { return None; }
    let (pa, va) = alloc_pages(3)?; core::ptr::write_bytes(va, 0, 12288);

    if io.ro.qa == LEGACY.qa {
        // Legacy I/O: single PFN
        io.w32(io.ro.qa, (pa >> 12) as u32);
    } else {
        // Modern MMIO: desc/driver/device split
        io.w32(io.ro.qa, pa as u32);
        io.w32(io.ro.qa + 4, (pa >> 32) as u32);
        io.w32(io.ro.qa + 8, (pa + 4096) as u32);
        io.w32(io.ro.qa + 12, ((pa + 4096) >> 32) as u32);
        io.w32(io.ro.qa + 16, (pa + 8192) as u32);
        io.w32(io.ro.qa + 20, ((pa + 8192) >> 32) as u32);
        // Enable queue (modern only)
        io.w16(io.ro.qe, 1);
    }
    Some(pa)
}

impl GpuDevice {
    pub fn new(dev: &crate::pci::PciDevice, phys_mem_offset: u64) -> Option<Self> {
        serial_println!("[VGPU] BAR0={:#x} BAR1={:#x}", dev.bar0, dev.bar1);

        // Debug: dump todas as capabilities PCI
        unsafe {
            let caps_list = crate::pci::read_pci_capabilities(dev.bus, dev.device, dev.function);
            for (id, ptr) in &caps_list {
                if *id == 0x09 {
                    let cfg = crate::pci::read_config_byte(dev.bus, dev.device, dev.function, ptr + 3);
                    let bar = crate::pci::read_config_byte(dev.bus, dev.device, dev.function, ptr + 4);
                    let off = crate::pci::read_config_dword(dev.bus, dev.device, dev.function, ptr + 8);
                    let len = crate::pci::read_config_dword(dev.bus, dev.device, dev.function, ptr + 12);
                    serial_println!("[VGPU] VirtIO cap ptr={:#x} cfg={} bar={} off={:#x} len={:#x}", ptr, cfg, bar, off, len);
                } else {
                    serial_println!("[VGPU] PCI cap id={:#x} ptr={:#x}", id, ptr);
                }
            }
        }

        // Para VirtIO moderno (0x1050), o MMIO base está em capabilities PCI,
        // não nas BARs padrão. Usamos read_virtio_cap + read_bar_value.
        let (io_base, mmio_base, is_mmio) = unsafe {
            if dev.bar0 & 1 == 1 {
                ((dev.bar0 & !0xFF) as u16, 0u64, false)
            } else {
                // Tenta encontrar o MMIO via VirtIO PCI capability (cfg_type=0 = common)
                    let cap = crate::pci::read_virtio_cap(dev.bus, dev.device, dev.function, 1);
                if let Some(cap) = cap {
                    let bar_addr = crate::pci::read_bar_value(dev.bus, dev.device, dev.function, cap.bar);
                    let base = bar_addr + cap.offset as u64;
                    // Verifica se endereço é mapeável
                    let test_virt = base.wrapping_add(phys_mem_offset);
                    if test_virt >> 47 != 0 && test_virt >> 47 != 0x1FFFF {
                        serial_println!("[VGPU] BAR {:x} não mapeável", base);
                        return None;
                    }
                    // Mapeia como uncacheable
                    crate::apic::map_mmio_page(base, phys_mem_offset);
                    serial_println!("[VGPU] VirtIO cap: bar={} off={:#x} len={:#x} base={:#x}",
                        cap.bar, cap.offset, cap.length, base);
                    (0u16, base, true)
                } else {
                    // Fallback: legacy I/O? Verifica BAR0 I/O bit
                    if dev.bar0 & 1 == 1 {
                        ((dev.bar0 & !0xFF) as u16, 0u64, false)
                    } else {
                        serial_println!("[VGPU] Sem capability VirtIO");
                        return None;
                    }
                }
            }
        };

        // Map MMIO BAR as uncacheable BEFORE accessing
        if is_mmio && mmio_base > 0 {
            unsafe { crate::apic::map_mmio_page(mmio_base, phys_mem_offset); }
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

            // Features (64-bit) — modern precisa selecionar low/high via FeatureSel
            if is_mmio {
                // Select low (0), read, write
                unsafe { (io.mmio_ptr(0x00)).write_volatile(0u32); } // DeviceFeatureSel = 0
                let feats = unsafe { (io.mmio_ptr(0x04)).read_volatile() }; // DeviceFeature
                unsafe { (io.mmio_ptr(0x08)).write_volatile(0u32); } // DriverFeatureSel = 0
                unsafe { (io.mmio_ptr(0x0C)).write_volatile(feats); } // DriverFeature (low)
                // Select high (1), read, write
                unsafe { (io.mmio_ptr(0x00)).write_volatile(1u32); }
                let feats_hi = unsafe { (io.mmio_ptr(0x04)).read_volatile() };
                unsafe { (io.mmio_ptr(0x08)).write_volatile(1u32); }
                unsafe { (io.mmio_ptr(0x0C)).write_volatile(feats_hi); }
                // Restore selects to 0
                unsafe { (io.mmio_ptr(0x00)).write_volatile(0u32); }
                unsafe { (io.mmio_ptr(0x08)).write_volatile(0u32); }
                serial_println!("[VGPU] dev_feat lo={:#x} hi={:#x}", feats, feats_hi);
            } else {
                let feat_low = io.r32(io.ro.df);
                let feat_high = io.r32(io.ro.df + 4);
                io.w32(io.ro.gf, feat_low);
                io.w32(io.ro.gf + 4, feat_high);
                serial_println!("[VGPU] dev_feat lo={:#x} hi={:#x}", feat_low, feat_high);
            }

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

            // Compute notify address for modern MMIO
            let notify_addr: u64 = if is_mmio {
                // Lê notify capability (cfg_type=2) do mesmo dispositivo
                let notify_cap = crate::pci::read_virtio_cap(dev.bus, dev.device, dev.function, 2);
                if let Some(nc) = notify_cap {
                    let bar_val = crate::pci::read_bar_value(dev.bus, dev.device, dev.function, nc.bar);
                    bar_val + nc.offset as u64
                } else {
                    0
                }
            } else { 0 };

            // DRIVER_OK
            io.add_status(4);
            serial_println!("[VGPU] status after DRIVER_OK={:x} notify={:#x}", io.r8(io.ro.st), notify_addr);

            // GET_DISPLAY_INFO via control queue
            let off = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
            let (cpa, cva) = alloc_pages(2)?;
            core::ptr::write_bytes(cva, 0, 8192);

            // Write command: GET_DISPLAY_INFO (use virtual address = phys + offset)
            let cva = cpa + off;
            *(cva as *mut u32) = 0x0100u32;

            // Setup descriptor in control queue
            let desc_base = (qpa + off) as *mut u8;
            let avail = (qpa + off + 4096) as *mut u16;

            // desc[0]: comando
            *(desc_base as *mut u64) = cpa;
            *((desc_base as *mut u32).add(2)) = 24;  // len = 24
            // desc[1]: resposta
            let d1 = desc_base.add(16);
            *(d1 as *mut u64) = cpa + 0x100;
            *((d1 as *mut u32).add(2)) = 128;         // len = 128
            *((d1 as *mut u16).add(6)) = 2;            // flags = WRITE

            // Avail ring: flags at +0, idx at +2, ring[] at +4
            *avail = 0;         // flags = 0
            *avail.add(1) = 0;  // ring[0] = 0 (descriptor chain head = descriptor 0)
            core::sync::atomic::fence(Ordering::SeqCst);
            *avail.add(2) = 1;  // idx = 1 (updated AFTER ring entries)

            // Notify queue 0
            if notify_addr > 0 {
                let no_off = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
                unsafe { ((notify_addr + no_off) as *mut u16).write_volatile(0); }
            } else {
                io.w16(io.ro.qn, 0);
            }

            // Poll for completion with HLT yield
            for _ in 0..2000 {
                if *((qpa + 8192 + off + 2) as *const u16) > 0 { break; }
                core::arch::asm!("sti; hlt", options(nomem, nostack));
            }

            let resp_type = *((cva + 0x100) as *const u32);
            if resp_type != 0x1100 {
                serial_println!("[VGPU] GET_DISPLAY resp={:#x}", resp_type);
                let (fw, fh) = (1024u32, 768u32);
                return init_framebuffer(&io, qpa, cpa, off, fw, fh, notify_addr);
            }

            let width = *((cva + 0x100 + 24) as *const u32);
            let height = *((cva + 0x100 + 28) as *const u32);
            let fb_w = if width > 0 && width <= 8192 { width } else { 1024 };
            let fb_h = if height > 0 && height <= 8192 { height } else { 768 };

            serial_println!("[VGPU] Display {}x{}", fb_w, fb_h);

            init_framebuffer(&io, qpa, cpa, off, fb_w, fb_h, notify_addr)
        }
    }
}

unsafe fn init_framebuffer(io: &Regs, qpa: u64, cpa: u64, off: u64, width: u32, height: u32, notify_addr: u64) -> Option<GpuDevice> {
    let cva = cpa + off; // virtual address for CPU writes
    let fb_sz = (width * height * 4) as usize;
    let fb_pg = (fb_sz + 4095) / 4096;
    let (fb_pa, fb_va) = alloc_pages(fb_pg)?;
    core::ptr::write_bytes(fb_va, 0, fb_pg * 4096);

    // RESOURCE_CREATE_2D
    *(cva as *mut u32) = 0x0101;
    *((cva as *mut u32).add(6)) = 1;     // resource_id
    *((cva as *mut u32).add(7)) = 1;     // format B8G8R8A8
    *((cva as *mut u32).add(8)) = width;
    *((cva as *mut u32).add(9)) = height;
    submit_q(io, qpa, cpa, 40, off, notify_addr);
    if !poll_q(qpa, off) { serial_println!("[VGPU] CREATE fail"); return None; }

    // RESOURCE_ATTACH_BACKING
    *(cva as *mut u32) = 0x0102;
    *((cva as *mut u32).add(6)) = 1;              // resource_id
    *((cva as *mut u32).add(7)) = fb_pg as u32;   // nr_entries
    for i in 0..fb_pg {
        let e = (cva + 32 + i as u64 * 16) as *mut u64;
        *e = fb_pa + (i as u64 * 4096);
        let lenp = (cva + 32 + i as u64 * 16 + 8) as *mut u32;
        *lenp = if i == fb_pg - 1 { (fb_sz % 4096) as u32 } else { 4096 };
    }
    submit_q(io, qpa, cpa, 32 + fb_pg * 16, off, notify_addr);
    if !poll_q(qpa, off) { serial_println!("[VGPU] ATTACH fail"); return None; }

    // SET_SCANOUT
    *(cva as *mut u32) = 0x0103;
    *((cva as *mut u32).add(6)) = 1;     // resource_id
    *((cva as *mut u32).add(7)) = 0;     // scanout_id = 0
    *((cva as *mut u32).add(8)) = 0;     // rect.x
    *((cva as *mut u32).add(9)) = 0;     // rect.y
    *((cva as *mut u32).add(10)) = width;
    *((cva as *mut u32).add(11)) = height;
    submit_q(io, qpa, cpa, 48, off, notify_addr);
    poll_q(qpa, off);

    serial_println!("[VGPU] VirtIO-GPU OK: {}x{} fb={:#x}", width, height, fb_pa);

    Some(GpuDevice { fb_addr: fb_pa, fb_width: width, fb_height: height, fb_stride: width*4, fb_bpp: 4, notify_addr, present: true })
}

unsafe fn submit_q(io: &Regs, qpa: u64, cpa: u64, cmd_len: usize, off: u64, notify_addr: u64) {
    // Descriptor table: cada descritor tem 16 bytes (addr:8, len:4, flags:2, next:2)
    // Usamos byte offsets explícitos para evitar erros de alinhamento
    let desc_base = (qpa + off) as *mut u8;
    let avail = (qpa + off + 4096) as *mut u16;

    // desc[0]: comando (device lê)
    *(desc_base as *mut u64) = cpa;                          // +0: addr = cmd phys
    *((desc_base as *mut u32).add(2)) = cmd_len as u32;      // +8: len
    // desc[0].flags = 0 (device read), desc[0].next = 0 (já são 0 do init)

    // desc[1]: resposta (device escreve)
    let d1 = desc_base.add(16);                              // +16: desc[1] start
    *(d1 as *mut u64) = cpa + 0x100;                         // +16: addr = resp phys
    *((d1 as *mut u32).add(2)) = 64;                          // +24: len
    *((d1 as *mut u16).add(6)) = 2;                           // +28: flags = WRITE

    // Avail ring: flags=0, ring[0]=0, idx=1
    *avail = 0;
    *avail.add(1) = 0;          // ring[0] = descriptor 0
    core::sync::atomic::fence(Ordering::SeqCst);
    *avail.add(2) = 1;          // idx = 1 (written AFTER ring entries)

    // Notify device
    if notify_addr > 0 {
        let no_off = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        ((notify_addr + no_off) as *mut u16).write_volatile(0);
    } else {
        io.w16(io.ro.qn, 0);
    }
}

/// Poll with HLT yield — QEMU TCG precisa de VM exit para processar VirtIO
unsafe fn poll_q(qpa: u64, off: u64) -> bool {
    let used_idx = (qpa + 8192 + off + 2) as *const u16;
    for _ in 0..2000 {
        if used_idx.read_volatile() > 0 { return true; }
        core::arch::asm!("sti; hlt", options(nomem, nostack));
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
