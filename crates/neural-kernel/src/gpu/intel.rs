//! Intel iGPU/GPU Ring Buffer — Gen9/Gen12/Xe/Xe2.
//! Controla o ring buffer de comandos da GPU Intel via MMIO.
//! Usado para matmul + blit + display.

use crate::gpu::detect::{GpuInfo, GpuVendor, GpuArch};
use crate::serial_println;

// MMIO offsets para ring buffer (Gen9+)
const RENDER_RING_BASE: u64 = 0x120000;
const RENDER_RING_HEAD: u64 = 0x120034;
const RENDER_RING_TAIL: u64 = 0x120038;
const RENDER_RING_CTL: u64 = 0x12003C;
const FORCE_WAKEUP: u64 = 0x0A278;

// GPU commands (dwords)
// MI_BATCH_BUFFER_END e MI_FLUSH compartilham opcode 0x00500000 em Gen9+ (MI_FLUSH removido Gen11+)
pub const MI_BATCH_BUFFER_START: u32 = 0x31A00000;
pub const MI_BATCH_BUFFER_END: u32 = 0x00500000;
pub const MI_NOOP: u32 = 0x00000000;

pub struct IntelRing {
    pub mmio: u64,           // BAR0 virtual
    pub ring_pa: u64,        // ring buffer physical address
    pub ring_va: *mut u32,   // ring buffer virtual address (page 0)
    pub ring_size: u32,      // in dwords (4096 = 16KB)
    pub tail: u32,
    pub has_render: bool,
    pub gen: u32,
}

// IntelRing so contem um raw pointer + integers. Seguro para enviar entre cores.
unsafe impl Send for IntelRing {}

impl IntelRing {
    /// Tenta detectar e inicializar GPU Intel
    pub fn probe(gpu: &GpuInfo, pmoff: u64) -> Option<Self> {
        if gpu.vendor != GpuVendor::Intel { return None; }
        let mmio = gpu.bar0 + pmoff;

        unsafe { crate::apic::map_page_uc(gpu.bar0, pmoff); }

        let test_val = unsafe { core::ptr::read_volatile((mmio + FORCE_WAKEUP) as *const u32) };
        if test_val == 0xFFFFFFFF || test_val == 0 {
            serial_println!("[INTEL] GPU nao respondeu. test_val={:#x}", test_val);
            return None;
        }

        let (ring_pa, ring_va) = unsafe { alloc_ring_buffer(4)? };

        unsafe { core::ptr::write_bytes(ring_va, 0, 16384); }

        unsafe {
            core::ptr::write_volatile((mmio + RENDER_RING_BASE) as *mut u64, ring_pa);
            core::ptr::write_volatile((mmio + RENDER_RING_CTL) as *mut u32, 4096);
            core::ptr::write_volatile((mmio + RENDER_RING_HEAD) as *mut u32, 0);
            core::ptr::write_volatile((mmio + RENDER_RING_TAIL) as *mut u32, 0);
        }

        // Inicializa GTT para que a GPU enxergue o ring buffer em RAM
        unsafe { init_gtt(mmio, ring_pa, 4); }

        let gen = match gpu.arch {
            GpuArch::IntelGen9 => 9,
            GpuArch::IntelGen12 | GpuArch::IntelXe => 12,
            GpuArch::IntelXe2 => 20,
            _ => 9,
        };

        serial_println!("[INTEL] Ring buffer OK: {} (Gen{}) mmio={:#x} ring={:#x}", gpu.name, gen, mmio, ring_pa);
        Some(IntelRing { mmio, ring_pa, ring_va, ring_size: 4096, tail: 0, has_render: true, gen })
    }

    /// Escreve comandos no ring buffer e avanca tail
    pub fn write(&mut self, cmd: &[u32]) {
        let len = cmd.len();
        if len > self.ring_size as usize {
            serial_println!("[INTEL] WARNING: cmd len {} > ring size {}, truncating!", len, self.ring_size);
        }
        let len = len.min(self.ring_size as usize);
        let wrap = (self.tail as usize + len).saturating_sub(self.ring_size as usize);
        if wrap > 0 {
            let first = len - wrap;
            for i in 0..first {
                unsafe { self.ring_va.add(self.tail as usize + i).write_volatile(cmd[i]); }
            }
            for i in 0..wrap {
                unsafe { self.ring_va.add(i).write_volatile(cmd[first + i]); }
            }
        } else {
            for i in 0..len {
                unsafe { self.ring_va.add(self.tail as usize + i).write_volatile(cmd[i]); }
            }
        }
        self.tail = (self.tail + len as u32) % self.ring_size;
    }

    /// Notifica GPU para processar o ring buffer
    pub fn submit(&mut self) {
        unsafe {
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
            core::ptr::write_volatile((self.mmio + RENDER_RING_TAIL) as *mut u32, self.tail);
        }
    }

    /// Espera GPU completar (poll head == tail)
    pub fn wait_idle(&self, timeout: u32) -> bool {
        for _ in 0..timeout {
            let head = unsafe { core::ptr::read_volatile((self.mmio + RENDER_RING_HEAD) as *const u32) };
            if head == self.tail { return true; }
            core::hint::spin_loop();
        }
        false
    }

    /// Executa MI_BATCH_BUFFER_START (submete batch buffer em separado)
    pub fn exec_batch(&mut self, batch_pa: u64) -> bool {
        self.write(&[
            MI_BATCH_BUFFER_START | 0x02,
            (batch_pa & 0xFFFFFFFF) as u32,
            (batch_pa >> 32) as u32,
        ]);
        self.submit();
        self.wait_idle(1000000)
    }

    /// Matmul via GPU (stub — shader GEN em GEN assembly)
    pub fn gpu_matmul(&mut self, _a: &crate::tensor::Tensor, _b: &crate::tensor::Tensor) -> Option<crate::tensor::Tensor> {
        None
    }

    /// Blitter: copia de VRAM para framebuffer (usado pelo Desktop Cube)
    /// Nota: idealmente usa BCS ring (blitter engine), nao RCS.
    /// Sem GTT set up, batch buffers em RAM do sistema nao sao visiveis pela GPU.
    pub fn gpu_blit(&mut self, src: u64, dst: u64, w: u32, h: u32, bpp: u32) -> bool {
        let pitch = w * bpp;
        let cmd = [
            0x41000000 | (3 << 24) | (pitch << 0),
            (0xCC << 16) | (h << 0),
            (0 << 16) | (w << 0),
            (dst & 0xFFFFFFFF) as u32,
            ((dst >> 32) & 0xFFFFFFFF) as u32,
            (src & 0xFFFFFFFF) as u32,
            ((src >> 32) & 0xFFFFFFFF) as u32,
            MI_BATCH_BUFFER_END,
        ];
        self.write(&cmd);
        self.submit();
        self.wait_idle(1000000)
    }
}

// GTT (Graphics Translation Table) — GPU MMU que mapeia RAM do sistema.
// GMADR base tipicamente em 0x100000. GTT entries = primeiros 2MB da GMADR.
const GMADR_BASE: u64 = 0x100000;
const GFX_FLSH_CNTL: u64 = 0x101008;
const GTT_ENTRY_COUNT: usize = 512; // 512 entradas × 8 bytes = 4KB

/// Inicializa GTT para que a GPU enxergue paginas de RAM do sistema.
/// Escreve entradas GTT para o ring buffer e batch buffers.
pub unsafe fn init_gtt(mmio: u64, ring_pa: u64, ring_size_pages: u32) -> bool {
    // GTT entries ficam no inicio da GMADR (primeiros 4KB = 512 entradas × 8 bytes)
    let gtt_base = mmio + GMADR_BASE;

    // Cada entrada GTT = 8 bytes: bits 0-39 = addr >> 12, bit 0 = PRESENT
    for i in 0..ring_size_pages {
        let pa = ring_pa + (i as u64) * 4096;
        let entry: u64 = (pa >> 12) << 2 | 0x1; // PFN << 2 | PRESENT (formato Gen9+)
        core::ptr::write_volatile((gtt_base + (i as u64) * 8) as *mut u64, entry);
    }

    // Flush GTT
    core::ptr::write_volatile((mmio + GFX_FLSH_CNTL) as *mut u32, 0);

    serial_println!("[GTT] {} entradas escritas @ {:#x} para ring {:#x}",
        ring_size_pages, gtt_base, ring_pa);
    true
}

// BCS (Blitter Command Streamer) ring — engine dedicado para blit.
// Register base em 0x22000, layout identico ao RCS.
const BCS_RING_BASE: u64 = 0x220000;
const BCS_RING_HEAD: u64 = 0x220034;
const BCS_RING_TAIL: u64 = 0x220038;
const BCS_RING_CTL: u64 = 0x22003C;

pub struct BcsRing {
    pub mmio: u64,
    pub ring_pa: u64,
    pub ring_va: *mut u32,
    pub ring_size: u32,
    pub tail: u32,
}

impl BcsRing {
    pub fn probe(mmio_base: u64) -> Option<Self> {
        let mmio = mmio_base;
        let (ring_pa, ring_va) = unsafe { alloc_ring_buffer(4)? };

        unsafe {
            core::ptr::write_bytes(ring_va, 0, 16384);
            core::ptr::write_volatile((mmio + BCS_RING_BASE) as *mut u64, ring_pa);
            core::ptr::write_volatile((mmio + BCS_RING_CTL) as *mut u32, 4096);
            core::ptr::write_volatile((mmio + BCS_RING_HEAD) as *mut u32, 0);
            core::ptr::write_volatile((mmio + BCS_RING_TAIL) as *mut u32, 0);
        }
        serial_println!("[BCS] Blitter ring at {:#x} size 4096 dw", ring_pa);
        Some(BcsRing { mmio, ring_pa, ring_va, ring_size: 4096, tail: 0 })
    }

    pub fn write(&mut self, cmd: &[u32]) {
        let len = cmd.len().min(self.ring_size as usize);
        for i in 0..len {
            unsafe { self.ring_va.add(self.tail as usize + i).write_volatile(cmd[i]); }
        }
        self.tail = (self.tail + len as u32) % self.ring_size;
    }

    pub fn submit(&mut self) {
        unsafe {
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
            core::ptr::write_volatile((self.mmio + BCS_RING_TAIL) as *mut u32, self.tail);
        }
    }

    pub fn wait_idle(&self, timeout: u32) -> bool {
        for _ in 0..timeout {
            let head = unsafe { core::ptr::read_volatile((self.mmio + BCS_RING_HEAD) as *const u32) };
            if head == self.tail { return true; }
            core::hint::spin_loop();
        }
        false
    }

    /// Executa blit no BCS ring (XY_SRC_COPY_BLT)
    pub fn blit(&mut self, src: u64, dst: u64, w: u32, h: u32, bpp: u32) -> bool {
        let pitch = w * bpp;
        let cmd = [
            0x41000000 | (3 << 24) | (pitch << 0),
            (0xCC << 16) | (h << 0),
            (0 << 16) | (w << 0),
            (dst & 0xFFFFFFFF) as u32,
            ((dst >> 32) & 0xFFFFFFFF) as u32,
            (src & 0xFFFFFFFF) as u32,
            ((src >> 32) & 0xFFFFFFFF) as u32,
            MI_BATCH_BUFFER_END,
        ];
        self.write(&cmd);
        self.submit();
        self.wait_idle(1000000)
    }
}

unsafe impl Send for BcsRing {}

unsafe fn alloc_ring_buffer(pages: usize) -> Option<(u64, *mut u32)> {
    
    let mut g = crate::memory::GLOBAL_ALLOCATOR.lock();
    let a = g.as_mut()?;
    let f = a.allocate_contiguous(pages)?;
    let pa = f.start_address().as_u64();
    if pa & 0xFFF != 0 {
        serial_println!("[INTEL] WARNING: ring buffer not page-aligned! {:#x}", pa);
    }
    let off = crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    let va = (pa + off) as *mut u32;
    Some((pa, va))
}
